#![no_std]

mod error;
mod events;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use crate::error::ContractError;
use crate::types::{CandidateStatus, ResolutionCandidate, ResolutionConfig};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};
use soroban_sdk::{IntoVal, Symbol, Val, Vec};

const MIN_CHALLENGE_WINDOW_SECONDS: u64 = 60;
const MAX_CHALLENGE_WINDOW_SECONDS: u64 = 14 * 24 * 60 * 60;
const MAX_URI_BYTES: u32 = 512;

/// Maximum number of times a candidate may be re-proposed via `appeal`
/// after being challenged, before the dispute must be resolved off-chain
/// (e.g. by admin intervention) rather than through further appeals.
const MAX_APPEAL_ROUNDS: u32 = 3;

/// Minimum bond a proposer must post (in the market's collateral token,
/// stroops) when calling `propose`. Locked in this contract and refunded on
/// successful finalize.
const MIN_BOND_AMOUNT: i128 = 10_000_000;

#[contract]
pub struct ResolutionContract;

#[contractimpl]
impl ResolutionContract {
    /// Register the resolution lifecycle contract with its factory and market.
    ///
    /// The factory can index this contract as the challenge-window companion
    /// for `market_contract`. Final settlement still happens through the
    /// market contract's `resolve_market` function after a candidate finalizes.
    pub fn initialize(
        env: Env,
        admin: Address,
        factory: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        if storage::has_config(&env) {
            return Err(ContractError::AlreadyInitialized);
        }

        storage::set_config(
            &env,
            &ResolutionConfig {
                admin,
                factory: factory.clone(),
                market_contract: market_contract.clone(),
            },
        );
        events::emit_resolution_registered(&env, &factory, &market_contract);
        Ok(())
    }

    pub fn get_config(env: Env) -> ResolutionConfig {
        storage::get_config(&env)
    }

    /// Update the registered factory address.
    pub fn set_factory(env: Env, admin: Address, factory: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        config.factory = factory.clone();
        storage::set_config(&env, &config);
        events::emit_resolution_registered(&env, &factory, &config.market_contract);
        Ok(())
    }

    /// Update the market contract address that finalized candidates target.
    pub fn set_market_contract(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        config.market_contract = market_contract.clone();
        storage::set_config(&env, &config);
        events::emit_resolution_registered(&env, &config.factory, &market_contract);
        Ok(())
    }

    /// Propose a signed resolution candidate for a market.
    ///
    /// The proposer must post a `bond_amount` (>= `MIN_BOND_AMOUNT`) in the
    /// market's collateral token, transferred from the proposer to this
    /// contract and locked until the candidate (or one of its appeals)
    /// finalizes, at which point it is refunded to the proposer.
    ///
    /// The returned candidate is the on-chain anchor for the backend
    /// `ResolutionCandidate`: off-chain services may display the same
    /// `challenge_deadline` and evidence URI while listening for challenge and
    /// finalize events.
    pub fn propose(
        env: Env,
        proposer: Address,
        market_id: u32,
        outcome: bool,
        signature: BytesN<64>,
        evidence_uri: String,
        challenge_window_seconds: u64,
        bond_amount: i128,
    ) -> Result<u32, ContractError> {
        proposer.require_auth();
        let config = storage::get_config(&env);
        validate_uri(&evidence_uri)?;
        validate_challenge_window(challenge_window_seconds)?;
        if bond_amount < MIN_BOND_AMOUNT {
            return Err(ContractError::InsufficientBond);
        }
        if storage::get_candidate_id_for_market(&env, market_id).is_some() {
            return Err(ContractError::CandidateAlreadyExists);
        }

        // Verify the provided oracle signature by delegating to the market
        // contract's `verify_signature` entrypoint. This ensures proposals are
        // rejected early if the signature does not verify.
        let args: Vec<Val> = soroban_sdk::vec![&env,
            market_id.into_val(&env),
            outcome.into_val(&env),
            signature.clone().into_val(&env),
        ];
        let verification: Result<(), ContractError> = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "verify_signature"),
            args,
        );
        verification?;

        // Lock the proposer's bond in this contract's collateral-token
        // balance. Uses the same token as the market's collateral so a
        // single `finalize` refund path can rely on it.
        let collateral_token = get_collateral_token(&env, &config, market_id);
        let token_client = TokenClient::new(&env, &collateral_token);
        token_client.transfer(&proposer, &env.current_contract_address(), &bond_amount);

        let proposed_at = env.ledger().timestamp();
        let candidate = ResolutionCandidate {
            id: storage::increment_candidate_id(&env),
            market_id,
            outcome,
            signature,
            proposer,
            evidence_uri,
            proposed_at,
            challenge_deadline: proposed_at + challenge_window_seconds,
            status: CandidateStatus::Proposed,
            challenged_by: None,
            challenge_uri: None,
            finalized_at: None,
            appeal_round: 0,
            bond_amount,
        };

        storage::set_candidate(&env, &candidate);
        events::emit_candidate_proposed(&env, &candidate);
        Ok(candidate.id)
    }

    /// Challenge a candidate while its challenge window is still open.
    pub fn challenge(
        env: Env,
        challenger: Address,
        candidate_id: u32,
        challenge_uri: String,
    ) -> Result<(), ContractError> {
        challenger.require_auth();
        validate_uri(&challenge_uri)?;

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        if candidate.status == CandidateStatus::Finalized {
            return Err(ContractError::CandidateAlreadyFinalized);
        }
        if candidate.status == CandidateStatus::Challenged {
            return Err(ContractError::CandidateAlreadyChallenged);
        }
        if env.ledger().timestamp() > candidate.challenge_deadline {
            return Err(ContractError::ChallengeWindowClosed);
        }

        candidate.status = CandidateStatus::Challenged;
        candidate.challenged_by = Some(challenger.clone());
        candidate.challenge_uri = Some(challenge_uri.clone());
        storage::set_candidate(&env, &candidate);
        events::emit_candidate_challenged(
            &env,
            candidate_id,
            candidate.market_id,
            &challenger,
            &challenge_uri,
        );
        Ok(())
    }

    /// Re-propose a challenged candidate, resetting it to `Proposed` with a
    /// fresh challenge window so it can be finalized (or challenged again).
    ///
    /// Only usable while the candidate is `Challenged`, and capped at
    /// `MAX_APPEAL_ROUNDS` total appeals per candidate — once the cap is
    /// reached, the dispute can no longer be advanced through this contract
    /// and must be resolved by the admin/factory out of band. The proposer's
    /// original bond stays locked and carries over across appeals; it is
    /// only refunded once the candidate is ultimately finalized.
    pub fn appeal(
        env: Env,
        proposer: Address,
        candidate_id: u32,
        outcome: bool,
        signature: BytesN<64>,
        evidence_uri: String,
        challenge_window_seconds: u64,
    ) -> Result<(), ContractError> {
        proposer.require_auth();
        let config = storage::get_config(&env);
        validate_uri(&evidence_uri)?;
        validate_challenge_window(challenge_window_seconds)?;

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        if candidate.status != CandidateStatus::Challenged {
            return Err(ContractError::CandidateNotChallenged);
        }
        if candidate.appeal_round >= MAX_APPEAL_ROUNDS {
            return Err(ContractError::AppealLimitExceeded);
        }

        // Re-verify the new signed outcome the same way `propose` does.
        let args: Vec<Val> = soroban_sdk::vec![&env,
            candidate.market_id.into_val(&env),
            outcome.into_val(&env),
            signature.clone().into_val(&env),
        ];
        let verification: Result<(), ContractError> = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "verify_signature"),
            args,
        );
        verification?;

        let proposed_at = env.ledger().timestamp();
        candidate.outcome = outcome;
        candidate.signature = signature;
        candidate.evidence_uri = evidence_uri;
        candidate.proposed_at = proposed_at;
        candidate.challenge_deadline = proposed_at + challenge_window_seconds;
        candidate.status = CandidateStatus::Proposed;
        candidate.challenged_by = None;
        candidate.challenge_uri = None;
        candidate.appeal_round += 1;

        storage::set_candidate(&env, &candidate);
        events::emit_candidate_appealed(&env, &candidate);
        Ok(())
    }

    /// Finalize an unchallenged candidate after its challenge window closes.
    ///
    /// This does not call `resolve_market` directly. It returns the signed
    /// candidate payload so the backend/factory can submit
    /// `market.resolve_market(market_id, outcome, signature)` as the final
    /// market-state transition.
    pub fn finalize(
        env: Env,
        finalizer: Address,
        candidate_id: u32,
    ) -> Result<ResolutionCandidate, ContractError> {
        finalizer.require_auth();
        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;

        if candidate.status == CandidateStatus::Finalized {
            return Err(ContractError::CandidateAlreadyFinalized);
        }
        if candidate.status == CandidateStatus::Challenged {
            return Err(ContractError::CandidateAlreadyChallenged);
        }
        if env.ledger().timestamp() <= candidate.challenge_deadline {
            return Err(ContractError::ChallengeWindowOpen);
        }

        candidate.status = CandidateStatus::Finalized;
        candidate.finalized_at = Some(env.ledger().timestamp());
        storage::set_candidate(&env, &candidate);

        // Refund the proposer's locked bond now that the candidate has
        // finalized successfully.
        if candidate.bond_amount > 0 {
            let config = storage::get_config(&env);
            let collateral_token = get_collateral_token(&env, &config, candidate.market_id);
            let token_client = TokenClient::new(&env, &collateral_token);
            token_client.transfer(
                &env.current_contract_address(),
                &candidate.proposer,
                &candidate.bond_amount,
            );
        }

        events::emit_candidate_finalized(&env, &candidate);
        Ok(candidate)
    }

    pub fn get_candidate(env: Env, candidate_id: u32) -> Option<ResolutionCandidate> {
        storage::get_candidate(&env, candidate_id)
    }

    pub fn get_candidate_id_for_market(env: Env, market_id: u32) -> Option<u32> {
        storage::get_candidate_id_for_market(&env, market_id)
    }
}

fn require_admin(admin: &Address, config: &ResolutionConfig) -> Result<(), ContractError> {
    if admin != &config.admin {
        return Err(ContractError::NotAdmin);
    }
    Ok(())
}

fn validate_challenge_window(seconds: u64) -> Result<(), ContractError> {
    if !(MIN_CHALLENGE_WINDOW_SECONDS..=MAX_CHALLENGE_WINDOW_SECONDS).contains(&seconds) {
        return Err(ContractError::InvalidChallengeWindow);
    }
    Ok(())
}

fn validate_uri(uri: &String) -> Result<(), ContractError> {
    let len = uri.len();
    if len == 0 || len > MAX_URI_BYTES {
        return Err(ContractError::InvalidEvidenceUri);
    }
    Ok(())
}

/// Look up a market's collateral token via a cross-contract call to the
/// registered market contract's `get_collateral_token`, used to lock and
/// refund proposer bonds in the same token as the market itself.
fn get_collateral_token(env: &Env, config: &ResolutionConfig, market_id: u32) -> Address {
    env.invoke_contract(
        &config.market_contract,
        &Symbol::new(env, "get_collateral_token"),
        soroban_sdk::vec![env, market_id.into_val(env)],
    )
}
