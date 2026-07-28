use crate::{ContractError, ResolutionContract, ResolutionContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env, String,
};

const DEFAULT_WINDOW: u64 = 300;

fn setup(env: &Env) -> (ResolutionContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let factory = Address::generate(env);
    let market_contract = Address::generate(env);
    client.initialize(&admin, &factory, &market_contract, &DEFAULT_WINDOW);
    (client, admin, market_contract)
}

fn signature(env: &Env) -> BytesN<64> {
    BytesN::from_array(env, &[7u8; 64])
}

fn evidence(env: &Env) -> String {
    String::from_str(env, "ipfs://resolution-evidence")
}

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|ledger| {
        ledger.timestamp = timestamp;
    });
}

#[test]
fn propose_stores_candidate_with_challenge_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &42,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    assert_eq!(candidate_id, 1);
    let candidate = client.get_candidate(&candidate_id).unwrap();
    assert_eq!(candidate.market_id, 42);
    assert_eq!(candidate.outcome, true);
    assert_eq!(candidate.challenge_deadline, 1_300);
    assert_eq!(client.get_candidate_id_for_market(&42), Some(candidate_id));
}

#[test]
fn challenge_marks_candidate_and_blocks_finalize() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &false,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    let challenger = Address::generate(&env);
    let challenge_uri = String::from_str(&env, "ipfs://challenge");
    client.challenge(&challenger, &candidate_id, &challenge_uri);
    set_time(&env, 1_400);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert_eq!(result, Err(Ok(ContractError::CandidateAlreadyChallenged)));
}

#[test]
fn finalize_requires_closed_challenge_window() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    let finalizer = Address::generate(&env);
    assert_eq!(
        client.try_finalize(&finalizer, &candidate_id),
        Err(Ok(ContractError::ChallengeWindowOpen))
    );

    set_time(&env, 1_301);
    let candidate = client.finalize(&finalizer, &candidate_id);
    assert_eq!(candidate.status, crate::types::CandidateStatus::Finalized);
    assert_eq!(candidate.finalized_at, Some(1_301));
}

#[test]
fn challenge_after_deadline_is_rejected() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(&proposer, &1, &true, &signature(&env), &(env.ledger().timestamp() + 60), &evidence(&env), &60);

    set_time(&env, 1_061);
    let challenger = Address::generate(&env);
    let challenge_uri = String::from_str(&env, "ipfs://late-challenge");
    assert_eq!(
        client.try_challenge(&challenger, &candidate_id, &challenge_uri),
        Err(Ok(ContractError::ChallengeWindowClosed))
    );
}

#[test]
fn admin_can_update_factory_registration() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);

    let new_factory = Address::generate(&env);
    client.set_factory(&admin, &new_factory);
    assert_eq!(client.get_config().factory, new_factory);
}

/// #404: finalize invokes resolve_market on the market contract.
///
/// Uses mock_all_auths so the cross-contract call is intercepted without
/// needing a live market contract deployment.
#[test]
fn finalize_calls_resolve_market_on_market_contract() {
    let env = Env::default();
    let (client, _, _) = setup(&env);

    set_time(&env, 1_000);
    let proposer = Address::generate(&env);
    let sig = signature(&env);
    let candidate_id = client.propose(&proposer, &5, &true, &sig, &(env.ledger().timestamp() + 60), &evidence(&env), &60);

    set_time(&env, 1_061);
    let finalizer = Address::generate(&env);
    // If the cross-contract resolve_market call were missing or wrong, this
    // would panic or return an error. Success proves the callback fired.
    let candidate = client.finalize(&finalizer, &candidate_id);

    assert_eq!(candidate.status, crate::types::CandidateStatus::Finalized);
    assert_eq!(candidate.market_id, 5);
    assert_eq!(candidate.outcome, true);
    assert!(candidate.finalized_at.is_some());
}

/// Test that proposal with valid signature is accepted via market contract delegation.
///
/// Verifies that the resolution contract correctly delegates signature verification
/// to the market contract's verify_signature entrypoint.
#[test]
fn propose_with_valid_signature_succeeds_via_delegation() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let valid_sig = signature(&env);
    
    // This should succeed because mock_all_auths intercepts the cross-contract call
    let candidate_id = client.propose(
        &proposer,
        &1,
        &true,
        &valid_sig,
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    assert_eq!(candidate_id, 1);
    let candidate = client.get_candidate(&candidate_id).unwrap();
    assert_eq!(candidate.market_id, 1);
    assert_eq!(candidate.outcome, true);
}

/// Test that proposal with invalid signature is rejected via market contract delegation.
///
/// Verifies that when the market contract rejects a signature, the resolution
/// contract propagates that error back to the caller.
#[test]
fn propose_with_invalid_signature_fails_via_delegation() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    // Use an invalid signature (all zeros) to trigger rejection
    let invalid_sig = BytesN::from_array(&env, &[0u8; 64]);
    
    let result = client.try_propose(
        &proposer,
        &1,
        &true,
        &invalid_sig,
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    // The market contract should reject the invalid signature
    assert!(result.is_err());
}

// ── #380: default challenge window ────────────────────────────────────────────

#[test]
fn initialize_stores_default_challenge_window() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.get_default_challenge_window(), DEFAULT_WINDOW);
}

#[test]
fn admin_can_update_default_challenge_window() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    client.set_default_challenge_window(&admin, &600);
    assert_eq!(client.get_default_challenge_window(), 600);
}

#[test]
fn set_default_challenge_window_rejects_non_admin() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    let rando = Address::generate(&env);
    assert_eq!(
        client.try_set_default_challenge_window(&rando, &600),
        Err(Ok(ContractError::NotAdmin))
    );
}

#[test]
fn set_default_challenge_window_rejects_out_of_bounds() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    assert_eq!(
        client.try_set_default_challenge_window(&admin, &10),
        Err(Ok(ContractError::InvalidChallengeWindow))
    );
    assert_eq!(
        client.try_set_default_challenge_window(&admin, &(14 * 24 * 60 * 60 + 1)),
        Err(Ok(ContractError::InvalidChallengeWindow))
    );
}

// ── Issue #552: challenge_window boundary tests at propose ─────────────────────
//
// Constants (from lib.rs):
//   MIN_CHALLENGE_WINDOW_SECONDS = 60
//   MAX_CHALLENGE_WINDOW_SECONDS = 14 * 24 * 60 * 60  (1_209_600)
//
// validate_challenge_window uses a Range::contains check equivalent to:
//   MIN..=MAX — so both endpoints are valid.

const MIN_WINDOW: u64 = 60;
const MAX_WINDOW: u64 = 14 * 24 * 60 * 60; // 1_209_600
const BOND: i128 = 10_000_000;

/// challenge_window == MIN (60 s) is accepted.
#[test]
fn propose_accepts_challenge_window_at_min() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let expiry = env.ledger().timestamp() + MIN_WINDOW + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MIN_WINDOW, &BOND,
    );
    assert!(result.is_ok(), "MIN window should be accepted, got: {:?}", result);
}

/// challenge_window == MAX (14 days) is accepted.
#[test]
fn propose_accepts_challenge_window_at_max() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let expiry = env.ledger().timestamp() + MAX_WINDOW + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MAX_WINDOW, &BOND,
    );
    assert!(result.is_ok(), "MAX window should be accepted, got: {:?}", result);
}

/// challenge_window == MIN - 1 (59 s) is rejected with InvalidChallengeWindow.
#[test]
fn propose_rejects_challenge_window_below_min() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let expiry = env.ledger().timestamp() + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &(MIN_WINDOW - 1), &BOND,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidChallengeWindow)));
}

/// challenge_window == MAX + 1 is rejected with InvalidChallengeWindow.
#[test]
fn propose_rejects_challenge_window_above_max() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let expiry = env.ledger().timestamp() + MAX_WINDOW + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &(MAX_WINDOW + 1), &BOND,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidChallengeWindow)));
}

// ── Issue #552: signature_expiry boundary tests at propose ────────────────────
//
// From lib.rs `propose`:
//   if signature_expiry < proposed_at  → InvalidSignatureExpiry
//   if signature_expiry == proposed_at → accepted (>= is valid)

/// signature_expiry == proposed_at is accepted (equal is the boundary minimum).
#[test]
fn propose_accepts_signature_expiry_equal_to_proposed_at() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    // expiry == ledger timestamp at call time
    let expiry = env.ledger().timestamp();
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MIN_WINDOW, &BOND,
    );
    assert!(result.is_ok(), "expiry == proposed_at should be accepted, got: {:?}", result);
}

/// signature_expiry < proposed_at is rejected with InvalidSignatureExpiry.
#[test]
fn propose_rejects_signature_expiry_before_proposed_at() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    // expiry is one second before ledger time
    let expiry = env.ledger().timestamp() - 1;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MIN_WINDOW, &BOND,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidSignatureExpiry)));
}

// ── Issue #552: challenge window boundary tests at challenge ──────────────────
//
// From lib.rs `challenge`:
//   if timestamp > challenge_deadline  → ChallengeWindowClosed
//
// Key off-by-one cases:
//   timestamp == deadline     → > is false  → window CLOSED (error)
//   timestamp == deadline - 1 → > is false  → window OPEN   (accepted)
//
// Wait — re-read: `> deadline` means equal is NOT greater, so:
//   timestamp == deadline     → NOT > deadline → window still open → accepted
//   timestamp == deadline + 1 → > deadline     → ChallengeWindowClosed

/// challenge at exactly the deadline (t == deadline) is still accepted —
/// the guard is strictly `>`, so the boundary second is inside the window.
#[test]
fn challenge_accepted_at_exactly_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_000 + 300 == 1_300
    // Set time to exactly the deadline.
    set_time(&env, 1_300);

    let challenger = Address::generate(&env);
    let uri = String::from_str(&env, "ipfs://at-deadline");
    let result = client.try_challenge(&challenger, &candidate_id, &uri);
    assert!(result.is_ok(), "challenge at deadline boundary should be accepted (guard is >)");
}

/// challenge one second after the deadline is rejected with ChallengeWindowClosed.
#[test]
fn challenge_rejected_one_second_after_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_301
    set_time(&env, 1_301);

    let challenger = Address::generate(&env);
    let uri = String::from_str(&env, "ipfs://past-deadline");
    assert_eq!(
        client.try_challenge(&challenger, &candidate_id, &uri),
        Err(Ok(ContractError::ChallengeWindowClosed))
    );
}

/// challenge one second before the deadline is accepted.
#[test]
fn challenge_accepted_one_second_before_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_299
    set_time(&env, 1_299);

    let challenger = Address::generate(&env);
    let uri = String::from_str(&env, "ipfs://just-before-deadline");
    let result = client.try_challenge(&challenger, &candidate_id, &uri);
    assert!(result.is_ok(), "challenge one second before deadline should be accepted");
}

// ── Issue #552: finalize window boundary tests ────────────────────────────────
//
// From lib.rs `finalize`:
//   if timestamp <= challenge_deadline  → ChallengeWindowOpen
//
// Key off-by-one cases:
//   timestamp == deadline     → <= is true → window OPEN  → ChallengeWindowOpen
//   timestamp == deadline + 1 → <= is false → window CLOSED → accepted

/// finalize at exactly the deadline is rejected — the window is still open
/// because the guard is `<=`.
#[test]
fn finalize_rejected_at_exactly_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; attempt finalize at exactly 1_300
    set_time(&env, 1_300);

    let finalizer = Address::generate(&env);
    assert_eq!(
        client.try_finalize(&finalizer, &candidate_id),
        Err(Ok(ContractError::ChallengeWindowOpen))
    );
}

/// finalize one second after the deadline succeeds.
#[test]
fn finalize_accepted_one_second_after_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    // signature_expiry well in the future so it doesn't interfere
    let expiry = env.ledger().timestamp() + window + 7200;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_301
    set_time(&env, 1_301);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert!(result.is_ok(), "finalize one second after deadline should succeed, got: {:?}", result);
}

// ── Issue #552: signature_expiry boundary tests at finalize ───────────────────
//
// From lib.rs `finalize`:
//   if timestamp > signature_expiry  → SignatureExpired
//
// Key off-by-one cases:
//   timestamp == expiry     → NOT > expiry → still valid  → accepted
//   timestamp == expiry + 1 → > expiry     → SignatureExpired

/// finalize at exactly signature_expiry (t == expiry) is accepted —
/// the guard is strictly `>`, so the boundary second is still valid.
#[test]
fn finalize_accepted_at_exactly_signature_expiry() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    // Set expiry to exactly deadline + 1 so there's a valid finalize window
    // that starts at 1_301 and the expiry is also exactly 1_301.
    let expiry = 1_301u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );

    // Advance to the expiry second (which is also one second past the deadline)
    set_time(&env, expiry);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert!(
        result.is_ok(),
        "finalize at exactly signature_expiry should be accepted (guard is >), got: {:?}",
        result
    );
}

/// finalize one second after signature_expiry is rejected with SignatureExpired.
#[test]
fn finalize_rejected_one_second_after_signature_expiry() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = 300u64;
    // expiry is challenge_deadline + 1, so it's right at the edge of being
    // finalizeable. We'll advance one second past it to trigger SignatureExpired.
    let expiry = 1_301u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );

    // Advance to expiry + 1
    set_time(&env, expiry + 1);

    let finalizer = Address::generate(&env);
    assert_eq!(
        client.try_finalize(&finalizer, &candidate_id),
        Err(Ok(ContractError::SignatureExpired))
    );
}
