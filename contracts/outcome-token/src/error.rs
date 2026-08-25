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
    /// A peer-to-peer `transfer` was rejected because the associated market
    /// has already resolved. See [`crate::OutcomeTokenContract::transfer`]
    /// for why this is blocked unconditionally rather than only before
    /// resolution (Issue #690): settlement pays out against the `Position`
    /// record keyed by the original depositor's address, not against
    /// whichever address currently holds the outcome-token balance, so a
    /// post-resolution transfer would let the same claim be walked away
    /// with twice.
    TransferBlockedAfterResolve = 8,
}
