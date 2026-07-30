use soroban_sdk::{contractevent, Address, Env, String};

#[contractevent]
#[derive(Clone, Debug)]
pub struct ResolutionRegistered {
    #[topic]
    pub factory: Address,
    pub market_contract: Address,
    pub registered_at: u64,
}

pub fn emit_resolution_registered(env: &Env, factory: &Address, market_contract: &Address) {
    ResolutionRegistered {
        factory: factory.clone(),
        market_contract: market_contract.clone(),
        registered_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateProposed {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub proposer: Address,
    pub evidence_uri: String,
    pub challenge_deadline: u64,
    pub signature_expiry: u64,
}

pub fn emit_candidate_proposed(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateProposed {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        proposer: candidate.proposer.clone(),
        evidence_uri: candidate.evidence_uri.clone(),
        challenge_deadline: candidate.challenge_deadline,
        signature_expiry: candidate.signature_expiry,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateChallenged {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub challenger: Address,
    pub challenge_uri: String,
    pub challenged_at: u64,
}

pub fn emit_candidate_challenged(
    env: &Env,
    candidate_id: u32,
    market_id: u32,
    challenger: &Address,
    challenge_uri: &String,
) {
    CandidateChallenged {
        candidate_id,
        market_id,
        challenger: challenger.clone(),
        challenge_uri: challenge_uri.clone(),
        challenged_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FactoryProposed {
    #[topic]
    pub factory: Address,
    pub effective_at: u64,
}

pub fn emit_factory_proposed(env: &Env, factory: &Address, effective_at: u64) {
    FactoryProposed {
        factory: factory.clone(),
        effective_at,
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FactorySet {
    #[topic]
    pub factory: Address,
    pub set_at: u64,
}

pub fn emit_factory_set(env: &Env, factory: &Address) {
    FactorySet {
        factory: factory.clone(),
        set_at: env.ledger().timestamp(),
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractProposed {
    #[topic]
    pub market_contract: Address,
    pub effective_at: u64,
}

pub fn emit_market_contract_proposed(env: &Env, market_contract: &Address, effective_at: u64) {
    MarketContractProposed {
        market_contract: market_contract.clone(),
        effective_at,
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractSet {
    #[topic]
    pub market_contract: Address,
    pub set_at: u64,
}

pub fn emit_market_contract_set(env: &Env, market_contract: &Address) {
    MarketContractSet {
        market_contract: market_contract.clone(),
        set_at: env.ledger().timestamp(),
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateFinalized {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub finalized_at: u64,
}

pub fn emit_candidate_finalized(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateFinalized {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        finalized_at: candidate.finalized_at.unwrap_or(env.ledger().timestamp()),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateAppealed {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub proposer: Address,
    pub appeal_round: u32,
    pub evidence_uri: String,
    pub challenge_deadline: u64,
    pub appealed_at: u64,
}

pub fn emit_candidate_appealed(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateAppealed {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        proposer: candidate.proposer.clone(),
        appeal_round: candidate.appeal_round,
        evidence_uri: candidate.evidence_uri.clone(),
        challenge_deadline: candidate.challenge_deadline,
        appealed_at: env.ledger().timestamp(),
    }
    .publish(env);
}
