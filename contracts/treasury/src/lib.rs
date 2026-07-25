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
//! | Operation                        | Who may call              |
//! |-----------------------------------|---------------------------|
//! | `initialize`                      | anyone (once)             |
//! | `collect_fee`                      | registered market contract|
//! | `withdraw_fees`                    | admin                     |
//! | `add_market` / `remove_market`     | admin                     |
//! | `set_market_contract`              | admin                     |
//! | Getters                            | anyone                    |
//!
//! ## Storage layout
//!
//! | Key                       | Type            | Description                              |
//! |---------------------------|-----------------|-------------------------------------------|
//! | `StorageVersion`          | `u32`           | Schema version guard                     |
//! | `Admin`                   | `Address`       | Protocol admin                           |
//! | `AuthorizedMarkets`       | `Vec<Address>`  | Market contracts allowed to call `collect_fee` |
//! | `TokenBalance(Address)`   | `i128`          | Current custodied balance per token (decreasable) |
//! | `CumulativeFees(Address)` | `i128`          | Historical total collected per token (monotone)   |
//! | `FeeTokens`               | `Vec<Address>`  | Registry of every token ever collected (#484)     |

pub mod error;
pub mod events;
pub mod storage;
#[cfg(test)]
mod test;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};

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
        let markets = soroban_sdk::vec![&env, market_contract.clone()];
        storage::set_authorized_markets(&env, &markets);
        storage::set_version(&env);
        events::emit_treasury_initialized(&env, &admin, &market_contract);
        Ok(())
    }

    // ── Fee collection ─────────────────────────────────────────────────────────

    /// Record a protocol fee transferred from any registered market contract.
    ///
    /// `token` identifies which token mint the fee was paid in (#484): the
    /// treasury custodies an independent balance per token, so markets using
    /// different collateral tokens can all route fees through the same
    /// treasury deployment without their balances colliding.
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

        // Track every token we've ever seen so callers can enumerate the full
        // set of fee-bearing tokens without prior knowledge (#484).
        storage::register_fee_token(&env, &token);

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
    /// Idempotent: adding an already-registered market is a no-op. Only the
    /// admin may call this.
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

        let mut markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            markets.push_back(market_contract.clone());
            storage::set_authorized_markets(&env, &markets);
        }
        Ok(())
    }

    /// Deregister a market contract, revoking its ability to call `collect_fee`.
    ///
    /// Returns [`TreasuryError::CallerNotMarket`] if the address is not registered.
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

        let markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            return Err(TreasuryError::CallerNotMarket);
        }
        let mut updated = Vec::new(&env);
        for m in markets.iter() {
            if m != market_contract {
                updated.push_back(m);
            }
        }
        storage::set_authorized_markets(&env, &updated);
        Ok(())
    }

    /// Rotate the full set of authorized markets to a single new market
    /// contract (e.g. after a market-contract upgrade). Existing
    /// registrations are replaced entirely — use [`add_market`] /
    /// [`remove_market`] to manage individual entries instead.
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

        let old_markets = storage::get_authorized_markets(&env);
        let old = old_markets.get(0).unwrap_or_else(|| new_market_contract.clone());
        let updated = soroban_sdk::vec![&env, new_market_contract.clone()];
        storage::set_authorized_markets(&env, &updated);
        events::emit_market_contract_updated(&env, &old, &new_market_contract);
        Ok(())
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

    // ── Getters ────────────────────────────────────────────────────────────────

    /// Return whether the treasury is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Return the admin address. Returns `UpgradeRequired` if version mismatches.
    pub fn admin(env: Env) -> Result<Address, TreasuryError> {
        storage::get_admin(&env)
    }

    /// Return the primary registered market contract address (the first entry
    /// in the authorized-markets registry). Returns `NotInitialized` if no
    /// market has ever been registered.
    pub fn market_contract(env: Env) -> Result<Address, TreasuryError> {
        storage::get_authorized_markets(&env)
            .get(0)
            .ok_or(TreasuryError::NotInitialized)
    }

    /// Return whether `market` is currently authorized to call `collect_fee`.
    pub fn is_authorized_market(env: Env, market: Address) -> bool {
        storage::is_authorized_market(&env, &market)
    }

    /// Return every market contract currently authorized to call `collect_fee`.
    pub fn list_markets(env: Env) -> Vec<Address> {
        storage::get_authorized_markets(&env)
    }

    /// Return every distinct token mint that has ever had a fee collected for
    /// it (#484). Useful for admin tooling to discover which per-token
    /// balances exist without prior knowledge of the token addresses.
    pub fn list_fee_tokens(env: Env) -> Vec<Address> {
        storage::get_fee_tokens(&env)
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
