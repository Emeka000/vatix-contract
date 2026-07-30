use crate::types::{EmergencyMode, ResolutionCandidate, ResolutionConfig};
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum StorageKey {
    Config,
    CandidateCounter,
    Candidate(u32),
    CandidateByMarket(u32),
    ProposerCollateral(Address),
    /// Coordinated emergency mode mirrored from the Market contract (Issue #662).
    /// Defaults to `Normal` when unset. Only the admin can change this value.
    EmergencyMode,
}

pub fn has_config(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Config)
}

pub fn get_config(env: &Env) -> ResolutionConfig {
    env.storage()
        .persistent()
        .get(&StorageKey::Config)
        .expect("Resolution config not set")
}

pub fn set_config(env: &Env, config: &ResolutionConfig) {
    env.storage().persistent().set(&StorageKey::Config, config);
}

pub fn increment_candidate_id(env: &Env) -> u32 {
    let next = env
        .storage()
        .persistent()
        .get(&StorageKey::CandidateCounter)
        .unwrap_or(0u32)
        + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::CandidateCounter, &next);
    next
}

pub fn get_candidate(env: &Env, candidate_id: u32) -> Option<ResolutionCandidate> {
    env.storage()
        .persistent()
        .get(&StorageKey::Candidate(candidate_id))
}

pub fn set_candidate(env: &Env, candidate: &ResolutionCandidate) {
    env.storage()
        .persistent()
        .set(&StorageKey::Candidate(candidate.id), candidate);
    env.storage().persistent().set(
        &StorageKey::CandidateByMarket(candidate.market_id),
        &candidate.id,
    );
}

pub fn get_candidate_id_for_market(env: &Env, market_id: u32) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&StorageKey::CandidateByMarket(market_id))
}

pub fn get_proposer_collateral(env: &Env, proposer: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::ProposerCollateral(proposer.clone()))
        .unwrap_or(0i128)
}

pub fn set_proposer_collateral(env: &Env, proposer: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::ProposerCollateral(proposer.clone()), &amount);
}

// ── Emergency Mode (Issue #662) ──────────────────────────────────────────────

/// Return the current mirrored emergency mode. Defaults to `Normal` when unset.
pub fn get_emergency_mode(env: &Env) -> EmergencyMode {
    env.storage()
        .persistent()
        .get(&StorageKey::EmergencyMode)
        .unwrap_or(EmergencyMode::Normal)
}

/// Set the mirrored emergency mode. Only the admin may call this (enforced in
/// `lib.rs`). Operators should keep this value in sync with the Market and
/// Treasury contracts for coordinated behaviour.
pub fn set_emergency_mode(env: &Env, mode: &EmergencyMode) {
    env.storage()
        .persistent()
        .set(&StorageKey::EmergencyMode, mode);
}
