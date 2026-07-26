//! Persistent storage helpers for the Vatix Treasury contract.

use crate::error::TreasuryError;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Bump this constant whenever the treasury storage layout changes in a breaking way.
/// `initialize()` writes this value so that future migrations can detect stale deployments.
///
/// ## Version history
/// - **v2:** Completed the multi-market `AuthorizedMarkets` registry
///   (`add_market`/`remove_market`/`list_markets`/`is_authorized_market`) and
///   added the `Stakeholders` fee-distribution list (#485).
/// - **v1:** Initial storage layout.
pub const STORAGE_VERSION: u32 = 2;

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum StorageKey {
    /// Written by `initialize`; used to detect stale or uninitialized deployments.
    StorageVersion,
    /// The address that can call `withdraw_fees` and other admin operations.
    Admin,
    /// The set of market contract addresses allowed to call `collect_fee`.
    AuthorizedMarkets,
    /// Current custodied balance for a specific token (decreases on withdrawal).
    TokenBalance(Address),
    /// Monotonically increasing cumulative fees collected per token (never decreases).
    CumulativeFees(Address),
    /// Global monotone counter: total of all fees ever collected across all tokens.
    TotalCollected,
    /// When `true`, `collect_fee` and `withdraw_fees` are blocked until unpaused.
    Paused,
    /// Ordered list of `(stakeholder, share_bps)` pairs used by `distribute_fees`
    /// (#485). `share_bps` values must sum to exactly 10_000.
    Stakeholders,
    /// Registry of every distinct token mint that has ever had a fee routed
    /// through `collect_fee` (#484). Lets callers enumerate which tokens hold
    /// a balance without needing prior knowledge of the token address.
    FeeTokens,
}

// ── Version ───────────────────────────────────────────────────────────────────

pub fn set_version(env: &Env) {
    env.storage()
        .instance()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

pub fn get_version(env: &Env) -> Option<u32> {
    env.storage().instance().get(&StorageKey::StorageVersion)
}

/// Guard used by every versioned storage accessor: rejects reads/writes
/// against a deployment whose on-chain schema doesn't match this build.
pub fn assert_version(env: &Env) -> Result<(), TreasuryError> {
    if get_version(env) != Some(STORAGE_VERSION) {
        return Err(TreasuryError::UpgradeRequired);
    }
    Ok(())
}

// ── Admin ─────────────────────────────────────────────────────────────────────

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&StorageKey::Admin)
}

pub fn get_admin(env: &Env) -> Result<Address, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .expect("treasury not initialized"))
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

// ── Authorized markets registry ───────────────────────────────────────────────
//
// Note: fixed alongside #484 (multi-token fee collection) since this file was
// touched for that change — `get_authorized_markets`/`is_authorized_market`
// previously referenced a non-existent singular `AuthorizedMarket` key.

/// Return the full list of markets currently authorized to call `collect_fee`.
///
/// Returns an empty list (rather than erroring) when nothing has been
/// registered yet, mirroring the market contract's `Vec`-storage convention.
pub fn get_authorized_markets(env: &Env) -> Result<Vec<Address>, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::AuthorizedMarkets)
        .unwrap_or_else(|| Vec::new(env)))
}

pub fn set_authorized_markets(env: &Env, markets: &Vec<Address>) {
    env.storage()
        .instance()
        .set(&StorageKey::AuthorizedMarkets, markets);
}

/// Return the first registered market — kept for backwards compatibility with
/// the original single-market `market_contract()` getter.
pub fn get_authorized_market(env: &Env) -> Result<Address, TreasuryError> {
    let markets = get_authorized_markets(env)?;
    markets.get(0).ok_or(TreasuryError::NotInitialized)
}

pub fn is_authorized_market(env: &Env, market: &Address) -> bool {
    match get_authorized_markets(env) {
        Ok(markets) => markets.contains(market),
        Err(_) => false,
    }
}

// ── Token balance (current, decreasable on withdrawal) ────────────────────────

pub fn get_token_balance(env: &Env, token: &Address) -> Result<i128, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::TokenBalance(token.clone()))
        .unwrap_or(0i128))
}

pub fn set_token_balance(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::TokenBalance(token.clone()), &amount);
}

// ── Cumulative fees (monotone historical counter per token) ───────────────────

pub fn get_cumulative_fees(env: &Env, token: &Address) -> Result<i128, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::CumulativeFees(token.clone()))
        .unwrap_or(0i128))
}

pub fn set_cumulative_fees(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::CumulativeFees(token.clone()), &amount);
}

// ── Fee token registry (#484: multi-token fee collection support) ────────────

/// Return every distinct token mint that has ever had a fee collected for it.
pub fn get_fee_tokens(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::FeeTokens)
        .unwrap_or_else(|| Vec::new(env))
}

/// Record `token` in the fee-token registry if it hasn't been seen before.
/// Idempotent: re-registering an already-known token is a no-op.
pub fn register_fee_token(env: &Env, token: &Address) {
    let mut tokens = get_fee_tokens(env);
    if !tokens.contains(token) {
        tokens.push_back(token.clone());
        env.storage()
            .instance()
            .set(&StorageKey::FeeTokens, &tokens);
    }
}

// ── Global cumulative (sum across all tokens, monotone) ───────────────────────

pub fn get_total_collected(env: &Env) -> Result<i128, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::TotalCollected)
        .unwrap_or(0i128))
}

pub fn set_total_collected(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalCollected, &amount);
}

// ── Pause flag ────────────────────────────────────────────────────────────────

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&StorageKey::Paused, &paused);
}

// ── Stakeholder revenue share (#485) ──────────────────────────────────────────

/// Return the configured `(stakeholder, share_bps)` list, or an empty list if
/// `set_stakeholders` has never been called.
pub fn get_stakeholders(env: &Env) -> Result<Vec<(Address, u32)>, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::Stakeholders)
        .unwrap_or_else(|| Vec::new(env)))
}

pub fn set_stakeholders(env: &Env, stakeholders: &Vec<(Address, u32)>) {
    env.storage()
        .instance()
        .set(&StorageKey::Stakeholders, stakeholders);
}
