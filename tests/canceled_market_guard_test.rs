//! Coverage for the "canceled markets must reject trades" guard.
//!
//! `MarketContract::update_position`, `deposit_collateral`, and
//! `withdraw_unused_collateral` all gate on `market.status != Active`, which
//! means a `Canceled` market is rejected through the exact same
//! `ContractError::MarketNotActive` path as a `Resolved` one. That existing
//! guard was previously only exercised against `Resolved` markets in the
//! test suite (see `test_withdraw_validates_market_not_active_resolved` in
//! `contracts/market/src/withdraw.rs`) — this file adds the missing
//! `Canceled` coverage explicitly, plus a before/after regression check that
//! trades succeed right up until the market is canceled and fail
//! immediately after.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{error::ContractError, storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn setup_canceled_market() -> (Env, Address, u32, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let collateral_token = token.address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    client.cancel_market(&admin, &market_id);

    (env, contract_id, market_id, user, collateral_token)
}

/// Buying shares (positive delta) on a canceled market must be rejected.
#[test]
fn update_position_buy_rejected_on_canceled_market() {
    let (_env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&_env, &contract_id);

    let result = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Selling shares (negative delta) on a canceled market must also be
/// rejected — the market-status guard runs before the share-balance check.
#[test]
fn update_position_sell_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    let result = client.try_update_position(
        &user,
        &market_id,
        &(-1i128),
        &0i128,
        &6_000i128,
    );
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Depositing new collateral into a canceled market must be rejected.
#[test]
fn deposit_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    StellarAssetClient::new(&env, &token).mint(&user, &(10 * STROOPS_PER_USDC));
    let result = client.try_deposit_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Withdrawing from a canceled market must be rejected — cancellation does
/// not open an alternate withdrawal path in this contract version; funds
/// remain accounted for in the position until the market's lifecycle
/// dictates otherwise.
#[test]
fn withdraw_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    let result =
        client.try_withdraw_unused_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Regression: the exact same trade that succeeds while the market is
/// Active must fail immediately once the market transitions to Canceled,
/// proving the guard reacts to the live status rather than a cached value.
#[test]
fn same_trade_succeeds_before_cancel_and_fails_after() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let collateral_token = token.address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Trade succeeds while Active.
    let before = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert!(before.is_ok(), "trade should succeed on an Active market");

    client.cancel_market(&admin, &market_id);

    // The identical trade shape now fails.
    let after = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert_eq!(after, Err(Ok(ContractError::MarketNotActive)));
}
