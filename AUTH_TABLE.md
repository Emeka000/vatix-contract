# Admin Authorization Audit

Inventory of every admin-gated mutator across the `market`, `treasury`, and
`resolution` contracts, and the check each one performs. Produced as part of
the auth-hardening pass; keep this in sync whenever an admin entrypoint is
added, removed, or renamed.

Every row below follows the same two-step pattern unless noted otherwise:

1. `caller.require_auth()` — cryptographic proof the caller signed the call.
2. `caller == stored_admin` — proof the signer is *the* admin, not merely
   *some* authenticated address. Step 1 alone is insufficient: a caller can
   authenticate as themselves but must still be rejected if they are not the
   admin.

## Market contract (`contracts/market/src/lib.rs`)

| Entrypoint                     | require_auth | admin-equality check | Notes |
|---------------------------------|:---:|:---:|-------|
| `initialize`                   | ✅ | n/a (bootstraps admin) | Guarded by `AlreadyInitialized` instead. |
| `propose_admin`                 | — | ✅ (`storage::get_admin`) | Auth deferred to `accept_admin`. |
| `accept_admin`                  | ✅ | ✅ (must match `PendingAdmin`) | Two-step transfer. |
| `initialize_market`             | ✅ | ✅ | |
| `cancel_market`                 | ✅ | ✅ | |
| `set_adapter_enabled`           | ✅ | ✅ | |
| `update_market_oracle`          | ✅ | ✅ | |
| `set_threshold_signers`         | ✅ | ✅ | |
| `set_treasury_contract`         | ✅ | ✅ | |
| `set_outcome_token_contract`    | ✅ | ✅ | |
| `set_resolution_contract`       | ✅ | ✅ | **Was missing entirely** — storage helpers and tests existed but no contract entrypoint called them; added by this pass. |
| `add_fee_waiver`                | ✅ | ✅ | |
| `remove_fee_waiver`             | ✅ | ✅ | |
| `set_fee_rate`                  | ✅ | ✅ | Also enforces the fee cap at propose time. |
| `pause`                         | ✅ | ✅ | **Was missing entirely** — `Paused` storage and every `require_not_paused` gate existed, but no entrypoint could ever set the flag. Added by this pass. |
| `unpause`                       | ✅ | ✅ | Added alongside `pause`. |
| `execute_fee_rate_change`       | — | n/a (intentionally public) | Access control is the timelock (`effective_at`), not caller identity — anyone may trigger it once due. |

## Treasury contract (`contracts/treasury/src/lib.rs`)

| Entrypoint            | require_auth | admin-equality check |
|------------------------|:---:|:---:|
| `initialize`           | ✅ | n/a (bootstraps admin) |
| `withdraw_fees`        | ✅ | ✅ |
| `transfer_admin`       | ✅ | ✅ |
| `add_market`           | ✅ | ✅ |
| `remove_market`        | ✅ | ✅ |
| `set_market_contract`  | ✅ | ✅ |
| `pause`                | ✅ | ✅ |
| `unpause`              | ✅ | ✅ |
| `set_stakeholders`     | ✅ | ✅ |
| `distribute_fees`      | ✅ | ✅ |

`collect_fee` requires auth from `caller` but intentionally checks
*registered-market* membership (`is_authorized_market`) rather than admin
identity — it is a market-contract-facing entrypoint, not an admin mutator.

## Resolution contract (`contracts/resolution/src/lib.rs`)

| Entrypoint                     | require_auth | admin-equality check |
|---------------------------------|:---:|:---:|
| `initialize`                   | ✅ | n/a (bootstraps admin) |
| `set_default_challenge_window`  | ✅ | ✅ (`require_admin`) |
| `set_factory`                   | ✅ | ✅ (`require_admin`) |
| `set_market_contract`           | ✅ | ✅ (`require_admin`) |
| `slash_collateral`              | ✅ | ✅ (`require_admin`) |

`propose`, `challenge`, `appeal`, `finalize`, and `deposit_collateral` all
`require_auth()` the acting party (proposer/challenger/finalizer) but are
deliberately open to any caller — the bond, challenge window, and finalize
conditions are the access control, not an admin check.

## Conclusion

Every admin mutator in `treasury` and `resolution` already performed both
checks. `market` had one entrypoint that was entirely absent
(`set_resolution_contract` — storage helpers and tests already existed for
it, but no contract entrypoint ever called them) and one storage flag with
no way to ever be set (`pause`/`unpause`) — those are the gaps this audit
closed. No admin mutator was found calling `require_auth()` without also
verifying the caller against the stored admin.
