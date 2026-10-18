# Contributing to StellarSettle Smart Contracts

Thank you for your interest in contributing to StellarSettle! We welcome contributions from developers of all skill levels.

---

## 🌿 Branching & Pull Request Workflow

### Default Branch: `dev`
All active development happens on the `dev` branch. The `main` branch is reserved exclusively for stable, production-ready releases.

### How to Contribute

1. **Fork** the repository (external contributors) or create a branch (team members).
2. **Branch off `dev`**:
   ```bash
   git checkout dev
   git pull origin dev
   git checkout -b feature/your-feature-name
   ```
   *Note: Always ensure your feature branch branches off `dev`.*
3. **Make your changes**, adhering to code style guidelines and writing unit tests.
4. **Commit using [Conventional Commits](https://www.conventionalcommits.org/)**:
   ```bash
   git commit -m "feat(escrow): add partial payment refund hook"
   ```
5. **Push to your fork/branch**:
   ```bash
   git push origin feature/your-feature-name
   ```
6. **Open a Pull Request targeting `dev`** (NOT `main`).
7. **Ensure all CI checks pass** (automated via GitHub Actions).
8. **Address review feedback** if requested.
9. **Merge** will be performed by maintainers after approval.

> ⚠️ **IMPORTANT**: Pull Requests targeting `main` will be **closed automatically**. Always target `dev`.

### Branch Naming Convention

| Prefix | Purpose | Example |
|---|---|---|
| `feature/` | New features or contract functions | `feature/add-clawback-token` |
| `fix/` | Bug fixes or edge-case handling | `fix/integer-overflow-fee` |
| `docs/` | Documentation changes | `docs/update-api-reference` |
| `test/` | Unit or integration test additions | `test/add-escrow-fuzz-tests` |
| `refactor/` | Code refactoring without behavioral changes | `refactor/simplify-storage-keys` |
| `chore/` | Maintenance tasks & dependency updates | `chore/update-soroban-sdk` |

---

## 🔒 Branch Protection Rules

### `main` Branch
- ✅ Require pull request before merging
- ✅ Require at least 2 maintainer approvals
- ✅ Require status checks to pass (CI workflow)
- ✅ Require branches to be up to date before merging
- ✅ Require conversation resolution before merging
- ❌ Do not allow force pushes
- ❌ Do not allow deletions

### `dev` Branch
- ✅ Require pull request before merging
- ✅ Require at least 1 maintainer approval
- ✅ Require status checks to pass (CI workflow)
- ✅ Require branches to be up to date before merging
- ❌ Do not allow force pushes
- ❌ Do not allow deletions

### How to Configure (Repository Admins)
1. Go to **Settings → Branches** in the GitHub repository.
2. Click **Add branch protection rule**.
3. Enter the branch name pattern (`main` and `dev`).
4. Enable the rules listed above.
5. Click **Save changes**.
6. Under **Settings → General → Default branch**, set the default branch to `dev`.

---

## ✅ CI Status Checks

Every Pull Request targeting `dev` automatically triggers our CI pipeline via GitHub Actions. Your PR must pass ALL checks before it can be merged:

- **Format Check**: `cargo fmt --all -- --check`
- **Clippy Lints**: `cargo clippy --all-targets --all-features -- -D warnings`
- **Unit & Integration Tests**: `cargo test --all --verbose`
- **WASM Build**: `cargo build --release --target wasm32v1-none`
- **Security Audit**: `cargo audit` — checks `Cargo.lock` against the RustSec Advisory Database
- **WASM Size Regression**: compares PR branch WASM sizes against base branch; fails if any contract grows by more than 10%

---

## 🚀 Release Process

Releases follow a controlled merge from `dev` to `main`:

1. A maintainer creates a release PR: `dev` → `main`.
2. The release PR includes a changelog update and version bump.
3. Two maintainer approvals are required.
4. All CI checks must pass cleanly.
5. After merge, a GitHub Release is tagged and published.
6. `dev` is rebased on `main` to stay in sync.

Contributors focus on getting PRs merged into `dev`. Maintenance of `main` is handled by core maintainers.

---

## 🔒 Security Audit

CI runs `cargo audit` on every PR to check for known vulnerabilities in dependencies. To run locally:

```bash
cargo install cargo-audit --locked
cargo audit
```

If an advisory affects a dependency you intentionally keep, document the justification in `audit.toml` at the repo root.
