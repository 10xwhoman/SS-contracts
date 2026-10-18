# Workspace static analysis review

Scope: Rust contract source in `contracts/invoice-escrow`, `contracts/invoice-token`, and `contracts/payment-distributor`. This is a source-level review aid, not a substitute for a formal audit or a proof of runtime safety.

## Repeatable checks

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --all -- -D warnings
cargo test --all
```

The CI workflow runs these checks on every pull request to `dev`. Clippy is useful for compiler-recognized suspicious patterns, but does not prove that arithmetic is safe or that storage keys are live. Review financial operations for `checked_*` arithmetic and compare each `StorageKey` variant against its read/write helpers and tests.

For a quick manual source inventory (review every hit; test and documentation code also contains intentional panics):

```sh
grep -RInE 'panic!|\.unwrap\(|\.expect\(' contracts/*/src --include='*.rs'
grep -RInE 'checked_(add|sub|mul|div)|[[:alnum:]_)][[:space:]]*[+*][[:space:]]*[[:alnum:]_(]' contracts/*/src --include='*.rs'
```

## Findings and disposition

| Area | Review result | Follow-up |
| --- | --- | --- |
| Arithmetic | The core escrow and token paths use checked arithmetic at financial boundaries. Auxiliary insurance-pool and earnest-deposit helpers contain direct multiplication/addition that need separate overflow handling before they are exposed to untrusted maximum values. | Track and resolve before enabling these helpers for production funding paths; the source inventory above makes the arithmetic sites reviewable. |
| Panic paths | `insurance_pool.rs` uses `panic!`/`expect` for invalid config, missing config, unauthorized claims, and invalid/overlarge claims. `earnest_deposit.rs` also panics on an out-of-range configuration despite returning `Result`. | These are explicit audit findings. Callers must validate inputs and configuration before invoking these helper paths; replace panic-based control flow with typed errors before production integration. |
| Storage keys | Token storage variants have corresponding typed get/set/remove operations in `storage.rs`; the lifecycle tests exercise representative balance, allowance, history, role, nonce, fee, metadata, and total-supply keys. Escrow includes feature-specific keys and therefore requires feature-level lifecycle review when a feature is modified. | Keep key lifecycle tests alongside new storage variants; do not remove a key based only on textual search because Soroban keys may be accessed through generic helpers. |
| Authorization | Contract entry points apply Soroban `require_auth` checks or role checks for privileged mutation. Integration and unit tests cover representative unauthorized calls. | Re-review new entry points and cross-contract callbacks against their expected invoker. |

## Limitations

This review does not claim zero findings. The auxiliary panic and unchecked arithmetic sites are called out above rather than hidden by excluding those modules from analysis. CI checks compilation, lint cleanliness, and test behavior; they do not perform symbolic execution, dependency vulnerability scanning, or a formal dead-storage proof.
