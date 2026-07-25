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
}
