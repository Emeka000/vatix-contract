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
    /// Proposer's bond is below the required minimum.
    InsufficientBond = 11,
    /// Candidate is not in the Challenged state, so appeal is not applicable.
    CandidateNotChallenged = 12,
    /// Candidate has reached the maximum number of appeal rounds.
    AppealLimitExceeded = 13,
    /// Collateral amount is invalid (e.g. zero or negative).
    InvalidCollateral = 14,
    /// Proposer has insufficient locked collateral for the operation.
    InsufficientCollateral = 15,
    Unauthorized = 40,
    NotAdmin = 41,
    AlreadyInitialized = 42,
}
