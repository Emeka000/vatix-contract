use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    CandidateNotFound = 1,
    CandidateAlreadyExists = 2,
    CandidateAlreadyChallenged = 3,
    CandidateAlreadyFinalized = 4,
    ChallengeWindowOpen = 5,
    ChallengeWindowClosed = 6,
    InvalidChallengeWindow = 7,
    InvalidEvidenceUri = 8,
    /// The provided signature has expired and can no longer be finalized.
    SignatureExpired = 9,
    /// The provided signature expiry timestamp is invalid (e.g. in the past).
    InvalidSignatureExpiry = 10,
    /// The underlying market is already resolved (or canceled) and can no
    /// longer accept a new resolution proposal (Issue #497).
    MarketAlreadyResolved = 11,
    /// `appeal` was called on a candidate that is not currently `Challenged`.
    CandidateNotChallenged = 12,
    /// A candidate has already been re-proposed `MAX_APPEAL_ROUNDS` times.
    AppealLimitExceeded = 13,
    /// `propose`'s `bond_amount` is below `MIN_BOND_AMOUNT`.
    InsufficientBond = 14,
    /// A proposer/challenger does not have enough deposited collateral for
    /// the requested operation.
    InsufficientCollateral = 15,
    /// A collateral amount is invalid (e.g. zero or negative).
    InvalidCollateral = 16,
    Unauthorized = 40,
    NotAdmin = 41,
    AlreadyInitialized = 42,
    /// The operation is blocked by the current emergency mode (Issue #662).
    /// Check [`crate::storage::get_emergency_mode`] for the active mode.
    EmergencyModeActive = 50,
}
