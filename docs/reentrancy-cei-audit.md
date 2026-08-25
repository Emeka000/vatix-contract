# Reentrancy & Checks-Effects-Interactions (CEI) Audit Report

## Audit Scope
- `contracts/market/src/withdraw.rs`
- `contracts/market/src/settlement.rs`
- `contracts/treasury/src/lib.rs` (`distribute_fees`)
- `contracts/resolution/src/lib.rs` (`propose`, `challenge`)

## Summary of Findings

| Contract | Function | Issue / Order Violation | Severity | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Market** | `withdraw_unused_collateral` | External fee transfer & `collect_fee` call occurred **before** `storage::set_position`. | High | **Fixed** |
| **Market** | `settle_position` | Outcome token `burn()` external call occurred **before** `storage::set_position`. | Medium | **Fixed** |
| **Treasury** | `distribute_fees` | Per-stakeholder `token_client.transfer` calls occurred **before** `storage::set_token_balance` decremented the treasury's balance. | High | **Fixed** (#688) |
| **Resolution** | `propose` | Proposer bond `token_client.transfer` occurred **before** `storage::set_candidate` created the candidate record. | High | **Fixed** (#686) |
| **Resolution** | `challenge` | Challenger bond `token_client.transfer` occurred **before** the candidate's `status`/`challenged_by` mutation and `storage::set_candidate`. | High | **Fixed** (#686) |

---

## Detailed Remediation

### 1. `withdraw_unused_collateral` (`withdraw.rs`)
- **Before:** Fee routing (`token_client.transfer` and `env.invoke_contract`) was executed prior to updating `position.total_deposited` and persisting it with `storage::set_position`.
- **After:** Decremented `position.total_deposited` and called `storage::set_position` **first**, satisfying CEI before making external token/treasury calls.

### 2. `settle_position` (`settlement.rs`)
- **Before:** Outcome tokens were burned via `burn_settled_outcome_tokens` (external contract calls) before persisting the updated `Position` state to storage.
- **After:** Reordered logic so `storage::set_position` persists state changes **first**, followed by token burns and final payout transfers.

### 3. `distribute_fees` (`treasury/src/lib.rs`, issue #688)
- **Before:** The distribution loop called `token_client.transfer(&treasury, &stakeholder, &amount)` for each stakeholder *inside* the loop that computed amounts, and only wrote the reduced `TokenBalance(token)` back to storage after every transfer had already gone out.
- **After:** Per-stakeholder amounts are computed into a local list first (no external calls), `storage::set_token_balance` persists the reduced balance, and only then does a second loop perform the external `transfer` calls. See the CEI note on `distribute_fees` in `contracts/treasury/src/lib.rs` for the exact ordering.
- **Dust remainder:** floor-division on `share_bps` can leave `balance - distributed` (up to `stakeholders.len() - 1` stroops) undistributed. This is written back into `TokenBalance(token)` (not dropped), so it rolls forward and is redistributed the next time `distribute_fees` is called for that token — documented explicitly on the function now.

### 4. `propose` / `challenge` (`resolution/src/lib.rs`, issue #686)
- **Before (`propose`):** The proposer's bond was transferred to the contract via `token_client.transfer` before `storage::set_candidate` created the `ResolutionCandidate` record.
- **After (`propose`):** The candidate is built and persisted via `storage::set_candidate` (and its `candidate_proposed` event emitted) **first**; the bond transfer happens last.
- **Before (`challenge`):** The challenger's bond was transferred before `candidate.status` was flipped to `Challenged` and `storage::set_candidate` persisted it.
- **After (`challenge`):** `candidate.status`/`challenged_by`/`challenge_uri` are mutated, `storage::set_candidate` and `storage::append_challenger` persist the new state, and the `candidate_challenged` event is emitted **first**; the bond transfer happens last.
