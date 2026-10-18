# Storage tier audit

Reference for [#492](https://github.com/StellarState/SS-contracts/issues/492).

The issue asked for temporary counters to be moved out of persistent/instance
storage to cut state rent. This document records what was actually audited, what
was changed, and — more importantly — what looked like a transient counter but is
lifetime data and must not be moved.

## How to pick a tier

| Tier | Rent | Lifetime | Use for |
| --- | --- | --- | --- |
| `temporary` | free | Until TTL (~5 min, or until the next ledger close) | Values only meaningful inside a single invocation. Cleared on error by the host's write rollback. |
| `instance` | archived with the contract | As long as the contract lives | Configuration and small hot-path data that is read on nearly every invocation and is cheap to keep archived. |
| `persistent` | rent-billed, needs `extend_ttl` | Indefinite, until explicitly removed | Balances, escrows, positions, nonces, and anything that must survive restarts. |

The failure mode to avoid: putting lifetime data in `temporary`. It does not error
— it silently disappears at TTL and reads back as the default value.

## Changes

### `payment-distributor` — `StorageKey::Locked` → `temporary`

The re-entrancy guard is set at the top of a guarded entrypoint and cleared by
`release_lock` on the success path; any error path rolls the write back via the
host. The flag therefore has no meaning between invocations, so it does not
belong in the archived instance that every invocation has to load.

Beyond the rent argument this is a robustness improvement: if any path ever left
the flag set, an instance entry would permanently block every guarded
entrypoint until an admin intervened, because nothing would clear it. A
temporary entry expires on its own.

`set_lock(env, false)` writes `false` rather than removing the entry, so the
key is present-but-unset outside a guarded call. `is_locked()` reads it through
`.unwrap_or(false)`, so this is correct; tests should assert on `is_locked()`
rather than on `has()`.

## Deliberately not migrated

These are the entries the issue's wording points at. All are lifetime data.

### `invoice-escrow` — `StorageKey::EscrowCount` (instance)

This is a monotonic counter, not a scratch value. It is the `total_count` for
`get_escrows` pagination, and each `create_escrow` writes the new invoice id at
`EscrowIdByIndex[count]` before incrementing. Moving the counter to `temporary`
would reset it to `0` on expiry, which makes `get_escrows` see
`start >= total_count` and return an **empty list for every escrow in existence**
— silent data loss with no error anywhere. The rent saved is a single instance
entry. Not worth it.

### `invoice-escrow` — `StorageKey::EscrowIdByIndex` (persistent)

Already in the correct tier, and the same reasoning applies: the index →
invoice-id map is the pagination index itself.

### `invoice-token` — `StorageKey::Nonce` (persistent)

Monotonic replay-protection counter. It must not reset while the token is
deployed, or previously-valid signatures could be replayed.

## Full inventory

### invoice-escrow

| Key | Tier | Verdict |
| --- | --- | --- |
| `Config` | instance | Correct — read on most entrypoints. |
| `EmergencyConfig` | instance | Correct. |
| `EscrowCount` | instance | Correct — lifetime counter, see above. |
| `MaxInvestors` | instance | Correct — cap, persists for the contract's life. |
| `CategoryFee` | instance | Correct — configuration. |
| `PendingParamChange` | instance | Correct — short-lived by design, but the timelock is measured in ledger time and must survive across the delay. |
| `Escrow` | persistent | Correct. |
| `EscrowIdByIndex` | persistent | Correct. |
| `FunderAmount` | persistent | Correct — per-funder accounting. |
| `Nonce` | persistent | Correct. |
| `BuyerWhitelist` | persistent | Correct. |
| `Invoice` / `InvoiceRecord` | persistent | Correct. |
| `EmergencyApprovals` | persistent | Correct — multi-sig votes must survive. |
| `InvestorPosition` | persistent | Correct — balances. |
| `InvestorCount` | persistent | Correct — lifetime counter. |
| `Dispute` | persistent | Correct. |
| `InstallmentSchedule` | persistent | Correct. |

### payment-distributor

| Key | Tier | Verdict |
| --- | --- | --- |
| `Locked` | **temporary** | **Changed in this PR** — transient guard. |
| `Admin` | instance | Correct. |
| `FeeRecipient` | instance | Correct. |
| `EscrowContract` | instance | Correct. |
| `RoleAdmin` / `RoleGrant` | instance | Correct. |
| `FeeTiers` | instance | Correct — small, read per distribution. |
| `InvestorBonusBps` | instance | Correct. |
| `Distribution` | persistent | Correct — per-escrow accounting, already `extend_ttl`'d. |

### invoice-token

| Key | Tier | Verdict |
| --- | --- | --- |
| `Metadata` | instance | Correct. |
| `TotalSupply` | instance | Correct — a scalar read on nearly every transfer. |
| `FeeBps` | instance | Correct. |
| `RoleAdmin` / `RoleGrant` | instance | Correct. |
| `Balance` | persistent | Correct. |
| `Frozen` | persistent | Correct. |
| `Allowance` | persistent | Correct. |
| `Nonce` | persistent | Correct — see above. |
| `History` | persistent | Correct. |

## Note on the `invoice-token` allowance accessors

`storage::get_allowance` and `storage::get_allowance_data` in `invoice-token`
read the same `Allowance` key, and `get_allowance` is a thinner variant that
takes the ledger timestamp separately. Both are used internally
(`get_allowance` at `lib.rs:154`, `get_allowance_data` at the three
`approve`/`transfer`/`burn` sites), so neither is dead — but the pair is
redundant API surface and the two call conventions are easy to mix up. Worth
collapsing into one accessor in a separate cleanup; noted here only because it
showed up during the tier audit. It is not a tiering problem.
