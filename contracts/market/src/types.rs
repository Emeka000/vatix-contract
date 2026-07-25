use soroban_sdk::{contracttype, Address, BytesN, String};

/// Represents the possible states of a prediction market.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MarketStatus {
    Active,
    Resolved,
    Canceled,
}

/// Represents the oracle adapter type used for market resolution.
///
/// This enum determines which oracle adapter (Ed25519, Reflector, or Pyth)
/// will be used to verify the outcome when resolving the market.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AdapterType {
    Ed25519,
    Reflector,
    Pyth,
}

/// Core structure containing all relevant information for a Market.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Market {
    pub id: u32,
    pub question: String,
    pub end_time: u64,
    pub oracle_pubkey: BytesN<32>,
    pub status: MarketStatus,
    pub result: Option<bool>,
    pub creator: Address,
    pub created_at: u64,
    pub collateral_token: Address,
    /// Current market price in basis points (0–10_000). Updated on every trade.
    pub price_bps: i128,
    /// Address of the resolver who resolved this market (only set when status is Resolved).
    pub resolver: Option<Address>,
    /// Timestamp when the market was resolved (only set when status is Resolved).
    pub resolved_at: Option<u64>,
    /// Oracle adapter type used for resolving this market.
    pub adapter_type: AdapterType,
    /// Number of possible outcomes for this market. Always 2 (YES/NO) for binary
    /// prediction markets. Set once at creation and immutable thereafter.
    pub outcome_count: u32,
    /// Flag indicating whether the market is closed to new deposits.
    /// When true, users cannot deposit new collateral, but can still withdraw and trade.
    pub closed_to_deposits: bool,
}

/// Tracks the position and shares of a specific user in a market.
///
/// # Storage layout (#482)
///
/// Fields are ordered largest/widest-first (`Address`, then the `i128` share
/// and collateral amounts) down to the narrowest fields (the `u32` market id
/// and the single-byte `is_settled` flag) last. This groups same-width fields
/// together to avoid wasted padding in the on-chain encoded representation
/// and keeps the compact layout intent explicit for future field additions.
///
/// Note: this is a breaking storage-layout change — reordering the declared
/// fields changes the serialized on-chain representation of `Position`. No
/// migration is included per the scope of #482; existing deployments would
/// need to redeploy/reinitialize as with any other breaking storage change
/// (see `STORAGE_VERSION` in `storage.rs`).
///
/// The `i128` amount fields (`yes_shares`, `no_shares`, `locked_collateral`,
/// `total_deposited`) are intentionally left wide: they represent token
/// quantities that can legitimately grow very large, so narrowing them would
/// risk overflow. Only the naturally-bounded `market_id` (`u32`) and
/// `is_settled` (`bool`) fields are narrow.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Position {
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    /// Collateral required to back current YES/NO shares (from calculate_locked_collateral).
    pub locked_collateral: i128,
    /// Total collateral deposited by user in this market (never decreased except by withdraw).
    pub total_deposited: i128,
    pub market_id: u32,
    pub is_settled: bool,
}

/// A fee-rate change awaiting its timelock delay before it can take effect
/// (Issue #496). Only one change may be pending at a time; proposing a new
/// one overwrites any earlier pending change.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingFeeRateChange {
    pub new_rate_bps: i128,
    /// Ledger timestamp at or after which `execute_fee_rate_change` may apply this change.
    pub effective_at: u64,
}

impl Position {
    /// Create an empty position for a user in a market.
    /// Used when a position has not been previously recorded in storage.
    pub fn new_empty(market_id: u32, user: Address) -> Self {
        Position {
            market_id,
            user,
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        }
    }
}
