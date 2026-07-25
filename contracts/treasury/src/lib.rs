#![no_std]
#![deny(clippy::all)]

//! # Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market protocol. Any address in the authorized market registry may deposit
//! fees via [`TreasuryContract::collect_fee`]; the **admin** controls all
//! other privileged operations (withdrawal, registry management).
//!
//! ## Authorization model
//!
//! | Operation             | Who may call              |
//! |-----------------------|---------------------------|
//! | `initialize`          | anyone (once)             |
//! | `collect_fee`         | any registered market     |
//! | `withdraw_fees`       | admin                     |
//! | `add_market`          | admin                     |
//! | `remove_market`       | admin                     |
//! | `set_stakeholders`    | admin                     |
//! | `distribute_fees`     | admin                     |
//! | Getters               | anyone                    |
//!
//! ## Storage layout
//!
//! | Key                       | Type                  | Description                              |
//! |---------------------------|-----------------------|-------------------------------------------|
//! | `StorageVersion`          | `u32`                 | Schema version guard                      |
//! | `Admin`                   | `Address`             | Protocol admin                            |
//! | `AuthorizedMarkets`       | `Vec<Address>`        | Fee-depositing contracts allowed to call `collect_fee` |
//! | `TokenBalance(Address)`   | `i128`                | Current custodied balance (decreasable)   |
//! | `CumulativeFees(Address)` | `i128`                | Historical total collected (monotone)     |
//! | `Stakeholders`            | `Vec<(Address, u32)>` | Revenue-share list, `share_bps` sums to 10_000 (#485) |

pub mod error;
pub mod events;
pub mod storage;
#[cfg(test)]
mod test;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};

/// Basis-point denominator: stakeholder shares must sum to exactly this value.
const BPS_DENOMINATOR: i128 = 10_000;

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    // ── Lifecycle ──────────────────────────────────────────────────────────────

    /// Bootstrap the treasury with an initial market contract in the registry.
    pub fn initialize(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        admin.require_auth();
        if storage::has_admin(&env) {
            return Err(TreasuryError::AlreadyInitialized);
        }
        storage::set_admin(&env, &admin);
        let mut markets: Vec<Address> = Vec::new(&env);
        markets.push_back(market_contract.clone());
        storage::set_authorized_markets(&env, &markets);
        storage::set_version(&env);
        events::emit_treasury_initialized(&env, &admin, &market_contract);
        Ok(())
    }

    // ── Fee collection ─────────────────────────────────────────────────────────

    /// Record a protocol fee transferred from any registered market contract.
    pub fn collect_fee(
        env: Env,
        caller: Address,
        token: Address,
        market_id: u32,
        fee_amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        if !storage::is_authorized_market(&env, &caller) {
            return Err(TreasuryError::CallerNotMarket);
        }
        if fee_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let prev_balance = storage::get_token_balance(&env, &token)?;
        let new_balance = prev_balance
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_token_balance(&env, &token, new_balance);

        let prev_cumulative = storage::get_cumulative_fees(&env, &token)?;
        let new_cumulative = prev_cumulative
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_cumulative_fees(&env, &token, new_cumulative);

        let prev_total = storage::get_total_collected(&env)?;
        storage::set_total_collected(
            &env,
            prev_total.checked_add(fee_amount).unwrap_or(i128::MAX),
        );

        events::emit_fee_collected(&env, market_id, &token, fee_amount, new_balance, new_cumulative);
        Ok(())
    }

    // ── Admin operations ───────────────────────────────────────────────────────

    /// Withdraw accumulated fees to a recipient address.
    pub fn withdraw_fees(
        env: Env,
        caller: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let balance = storage::get_token_balance(&env, &token)?;
        if amount > balance {
            return Err(TreasuryError::InsufficientBalance);
        }

        let treasury = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&treasury, &to, &amount);

        let remaining = balance - amount;
        storage::set_token_balance(&env, &token, remaining);

        events::emit_fees_withdrawn(&env, &token, &to, amount, remaining);
        Ok(())
    }

    /// Transfer admin rights to a new address immediately.
    ///
    /// Only the current admin may call this.
    pub fn transfer_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        storage::set_admin(&env, &new_admin);
        events::emit_admin_transferred(&env, &admin, &new_admin);
        Ok(())
    }

    /// Register an additional market contract allowed to call `collect_fee`.
    ///
    /// Idempotent — adding an already-registered market is a no-op (no
    /// duplicate entry, no event). Only the admin may call this.
    pub fn add_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = storage::get_authorized_markets(&env)?;
        if !markets.contains(&market_contract) {
            markets.push_back(market_contract.clone());
            storage::set_authorized_markets(&env, &markets);
            events::emit_market_added(&env, &market_contract);
        }
        Ok(())
    }

    /// Deregister a market contract, revoking its ability to call `collect_fee`.
    ///
    /// Only the admin may call this.
    ///
    /// # Errors
    /// - [`TreasuryError::CallerNotMarket`] — `market_contract` is not currently registered.
    pub fn remove_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = storage::get_authorized_markets(&env)?;
        match markets.first_index_of(&market_contract) {
            Some(idx) => {
                markets.remove(idx);
                storage::set_authorized_markets(&env, &markets);
                events::emit_market_removed(&env, &market_contract);
                Ok(())
            }
            None => Err(TreasuryError::CallerNotMarket),
        }
    }

    /// Replace the entire authorized-market registry with a single market
    /// contract (full rotation), e.g. after deploying a new market contract
    /// version. Equivalent to removing every previously registered market and
    /// calling `add_market` with only `new_market_contract`.
    ///
    /// Prefer `add_market`/`remove_market` for incremental registry changes
    /// when more than one market should remain authorized at a time.
    ///
    /// Only the admin may call this.
    pub fn set_market_contract(
        env: Env,
        caller: Address,
        new_market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let old = storage::get_authorized_market(&env)?;
        let mut markets: Vec<Address> = Vec::new(&env);
        markets.push_back(new_market_contract.clone());
        storage::set_authorized_markets(&env, &markets);
        events::emit_market_contract_updated(&env, &old, &new_market_contract);
        Ok(())
    }

    /// Return whether `market_contract` is currently authorized to call `collect_fee`.
    pub fn is_authorized_market(env: Env, market_contract: Address) -> bool {
        storage::is_authorized_market(&env, &market_contract)
    }

    /// Return the full list of markets currently authorized to call `collect_fee`.
    pub fn list_markets(env: Env) -> Result<Vec<Address>, TreasuryError> {
        storage::get_authorized_markets(&env)
    }

    /// Pause the treasury, blocking `collect_fee` and `withdraw_fees`.
    ///
    /// Intended for use during contract upgrades or incident response. Only the
    /// admin may call this.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    pub fn pause(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_paused(&env, true);
        events::emit_treasury_paused(&env, &caller);
        Ok(())
    }

    /// Unpause the treasury, restoring normal operation.
    ///
    /// Only the admin may call this.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    pub fn unpause(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_paused(&env, false);
        events::emit_treasury_unpaused(&env, &caller);
        Ok(())
    }

    // ── Stakeholder fee distribution (#485) ────────────────────────────────────

    /// Configure the stakeholder revenue-share list (admin only).
    ///
    /// `stakeholders` is a list of `(address, share_bps)` pairs. `share_bps`
    /// values must sum to exactly 10_000 (100%); this fully replaces any
    /// previously configured list.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    /// - [`TreasuryError::InvalidStakeholderWeights`] – list is empty, or the
    ///   `share_bps` values do not sum to exactly 10_000.
    pub fn set_stakeholders(
        env: Env,
        caller: Address,
        stakeholders: Vec<(Address, u32)>,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if stakeholders.is_empty() {
            return Err(TreasuryError::InvalidStakeholderWeights);
        }

        let mut total: i128 = 0;
        for (_, share_bps) in stakeholders.iter() {
            total = total
                .checked_add(share_bps as i128)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
        }
        if total != BPS_DENOMINATOR {
            return Err(TreasuryError::InvalidStakeholderWeights);
        }

        let count = stakeholders.len();
        storage::set_stakeholders(&env, &stakeholders);
        events::emit_stakeholders_updated(&env, count);
        Ok(())
    }

    /// Return the configured `(stakeholder, share_bps)` list.
    pub fn get_stakeholders(env: Env) -> Result<Vec<(Address, u32)>, TreasuryError> {
        storage::get_stakeholders(&env)
    }

    /// Distribute the treasury's current `token` balance to the configured
    /// stakeholders, proportionally to their `share_bps` weight (admin only).
    ///
    /// Each stakeholder receives `floor(balance * share_bps / 10_000)`. Because
    /// integer division can leave a small remainder (at most
    /// `stakeholders.len() - 1` stroops), any dust stays in the treasury's
    /// `token` balance and rolls into the next distribution rather than being
    /// lost.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::ContractPaused`] – treasury is paused.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    /// - [`TreasuryError::NoStakeholdersConfigured`] – `set_stakeholders` has
    ///   never been called.
    /// - [`TreasuryError::InsufficientBalance`] – the current `token` balance is zero.
    ///
    /// # Events
    /// Emits [`events::FeesDistributed`] once per call, summarizing the total
    /// amount paid out and the remaining balance.
    pub fn distribute_fees(env: Env, caller: Address, token: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let stakeholders = storage::get_stakeholders(&env)?;
        if stakeholders.is_empty() {
            return Err(TreasuryError::NoStakeholdersConfigured);
        }

        let balance = storage::get_token_balance(&env, &token)?;
        if balance <= 0 {
            return Err(TreasuryError::InsufficientBalance);
        }

        let treasury = env.current_contract_address();
        let token_client = token::Client::new(&env, &token);

        let mut distributed: i128 = 0;
        for (stakeholder, share_bps) in stakeholders.iter() {
            let amount = balance
                .checked_mul(share_bps as i128)
                .ok_or(TreasuryError::ArithmeticOverflow)?
                / BPS_DENOMINATOR;
            if amount > 0 {
                token_client.transfer(&treasury, &stakeholder, &amount);
                distributed = distributed
                    .checked_add(amount)
                    .ok_or(TreasuryError::ArithmeticOverflow)?;
            }
        }

        let remaining = balance - distributed;
        storage::set_token_balance(&env, &token, remaining);

        events::emit_fees_distributed(&env, &token, distributed, remaining, stakeholders.len());
        Ok(())
    }

    // ── Getters ────────────────────────────────────────────────────────────────

    /// Return whether the treasury is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Return the admin address. Returns `UpgradeRequired` if version mismatches.
    pub fn admin(env: Env) -> Result<Address, TreasuryError> {
        storage::get_admin(&env)
    }

    /// Return the registered market contract address. Returns `UpgradeRequired` if version mismatches.
    pub fn market_contract(env: Env) -> Result<Address, TreasuryError> {
        storage::get_authorized_market(&env)
    }

    /// Return the current custodied balance for `token` (decreases on withdrawal).
    pub fn token_balance(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_token_balance(&env, &token)
    }

    /// Return the per-token cumulative fees collected for `token` since deployment.
    ///
    /// This counter never decreases: admin withdrawals do not affect it.
    pub fn get_cumulative_fees(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_cumulative_fees(&env, &token)
    }

    /// Return the global cumulative fees collected across all tokens since deployment.
    ///
    /// Monotone: never decreases regardless of admin withdrawals.
    pub fn total_collected(env: Env) -> Result<i128, TreasuryError> {
        storage::get_total_collected(&env)
    }
}
