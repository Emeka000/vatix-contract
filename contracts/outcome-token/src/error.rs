use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InsufficientBalance = 4,
    InvalidAmount = 5,
    Overflow = 6,
    /// A peer-to-peer `transfer` was attempted before the associated market
    /// resolved. Outcome tokens are only transferable once the market they
    /// belong to has settled its outcome.
    MarketNotResolved = 7,
    /// `execute_market_contract` / `cancel_market_contract` was called but
    /// no market-contract rotation is currently pending (Issue #691).
    NoPendingMarketContractChange = 8,
    /// A pending `market_contract` rotation's `effective_at` timelock has
    /// not elapsed yet (Issue #691).
    TimelockNotElapsed = 9,
}
