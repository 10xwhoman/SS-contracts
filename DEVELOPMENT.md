# Developer Guide & Local Workflow

This guide covers local environment setup, build steps, and local CI validation for the StellarSettle smart contract workspace.

---

## 🔍 Running CI Checks Locally

Before submitting a Pull Request targeting `dev`, run these checks locally to ensure your changes pass CI:

```bash
# 1. Format check
cargo fmt --all -- --check

# 2. Lint check with Clippy
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run all unit and integration tests across all workspace contracts
cargo test --all --verbose

# 4. Audit dependency vulnerabilities against the RustSec advisory database
cargo audit

# 5. Build release WASM binaries for the target in rust-toolchain.toml (wasm32v1-none)
cargo build --release --target wasm32v1-none

# 6. Verify rust-toolchain.toml stays in sync with CI workflows and scripts
bash scripts/check-toolchain-sync.sh
```

---

## 🔐 Dependency Vulnerability Triage

CI runs `cargo audit` on every Pull Request targeting `dev` and every push to `dev`. When an advisory is reported:

1. Confirm the affected crate, version range, and advisory details in the RustSec database.
2. Prefer upgrading the vulnerable dependency or its direct parent dependency in the same Pull Request.
3. If no patched version is available, document the impact analysis, affected code paths, and mitigation plan in the Pull Request before requesting review.
4. Do not add advisory ignores unless the advisory is demonstrably unreachable or a maintainer approves a temporary exception with a tracked follow-up issue.

Run `cargo install cargo-audit --locked` once locally if the `cargo audit` command is unavailable.

---

## 🛠️ Local Development Setup

### 1. Requirements
- Rust stable toolchain (`1.80+`) with target `wasm32v1-none` (see `rust-toolchain.toml`)
- Stellar CLI / Soroban CLI (`stellar-cli` / `soroban-cli` pinned to `22.0.0`)
- `cargo-tarpaulin` (optional, for code coverage reports)

### 2. Standard Commands

```bash
# Build dev binaries
cargo build

# Run tests for specific contract
cargo test --package invoice-escrow
cargo test --package invoice-token
cargo test --package payment-distributor

# Run deployment script locally or against testnet
bash scripts/deploy.sh
```

### 3. Git Pre-Commit Hooks

To automatically enforce formatting and linting before every commit:

```bash
bash scripts/install-hooks.sh
```

This installs a git hook at `.git/hooks/pre-commit` that runs `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`.

---

## 🧪 Smoke Testing & Testnet Script Execution

To run end-to-end smoke tests against Stellar Testnet:

1. **Configure Environment Variables**: Set `STELLAR_NETWORK=testnet` and export your test account secret key `SECRET_KEY=S...`.
2. **Execute Deploy Script**: Run `bash scripts/deploy.sh` to compile WASM, deploy contract instances, and register test accounts.
3. **Execute Smoke Test Recipe**:
   ```bash
   # Initialize escrow contract
   soroban contract invoke --id <ESCROW_ID> --source seller --network testnet -- initialize --admin <ADMIN> --payment_token <TOKEN_ID>
   ```

---

## 🌿 Contribution Workflow Reminder

All Pull Requests MUST target the `dev` branch. Direct pushes to `main` or `dev` are prohibited. See [CONTRIBUTING.md](./CONTRIBUTING.md) for details.
