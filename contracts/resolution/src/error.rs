use soroban_sdk::contracterror;

/// Error codes for the Vatix resolution candidate contract.
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
    /// `appeal` was called after `MAX_APPEAL_ROUNDS` re-proposals were
    /// already used up for this candidate.
    AppealLimitExceeded = 9,
    /// `appeal` was called on a candidate that is not currently `Challenged`.
    CandidateNotChallenged = 10,
    /// Bond posted with `propose` was below `MIN_BOND_AMOUNT`.
    InsufficientBond = 11,
    Unauthorized = 40,
    NotAdmin = 41,
    AlreadyInitialized = 42,
}
