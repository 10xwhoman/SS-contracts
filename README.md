<div align="center">
  <img src="logo.png" alt="StellarSettle Logo" width="200"/>
  
  # StellarSettle Smart Contracts
  
  **Soroban smart contracts powering decentralized invoice financing on Stellar**
  
  [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
  [![Stellar](https://img.shields.io/badge/Stellar-Soroban-blue)](https://stellar.org)
  [![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
  [![CI](https://github.com/StellarState/SS-contracts/actions/workflows/ci.yml/badge.svg)](https://github.com/StellarState/SS-contracts/actions)
  [![Coverage Status](https://img.shields.io/badge/coverage-94%25-brightgreen)](https://github.com/StellarState/SS-contracts)
</div>

## 📋 Overview

This repository contains the core Soroban smart contracts that power StellarSettle's decentralized invoice financing platform. These contracts handle:

- **Invoice Escrow**: Secure escrow creation, funding, and settlement
- **Invoice Tokens**: SEP-41 compliant tokenization of invoices
- **Payment Distribution**: Lifecycle-driven payout fan-out for seller, investor, and platform fee
- **Emergency Pause**: Admin-controlled stop switches for escrow and token operations

## 🏗️ Architecture
```
contracts/
├── invoice-escrow/       # Main escrow contract
├── invoice-token/        # Invoice tokenization (SEP-41)
└── payment-distributor/  # Settlement & distribution logic
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.74+
- Soroban CLI
- Stellar account (testnet/mainnet)

**Windows:** Rust uses the MSVC toolchain by default. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **“Desktop development with C++”** workload so `link.exe` is available. (VS Code alone is not sufficient.)

### Installation
```bash
# Install Soroban CLI
cargo install --locked soroban-cli --features opt

# Install dependencies
cargo build

# Run tests
cargo test

# Build contracts
soroban contract build
```

### Deployment

Repeatable deployment scripts live in [`scripts/`](scripts/). See the [**How to run scripts**](#-how-to-run-scripts) section below for full instructions.

The scripts deploy and initialise all three contracts, then wire `invoice-escrow` to `payment-distributor` so settlement and refund payouts run through the distributor flow by default.

For an interactive testnet setup wizard and a single CLI entrypoint wrapping setup/build/package/deploy/smoke:

```bash
bash scripts/cli.sh            # interactive menu
bash scripts/cli.sh setup      # write .env (network, keys, fee, token metadata)
bash scripts/cli.sh deploy     # multi-contract deploy (wraps scripts/deploy.sh)
```

CI releases: merges to `main` trigger [`.github/workflows/release.yml`](.github/workflows/release.yml), which tags the next semver version, generates a changelog, and attaches WASM artifacts + `checksums.txt` to the GitHub Release. Toolchain drift between `rust-toolchain.toml` and workflows/scripts is enforced by `scripts/check-toolchain-sync.sh` (run automatically in CI).

## 📚 Contract Documentation

- [TypeScript contract error SDK](sdk/README.md) — exported error codes, TypeScript types, and JSON Schema.

### Invoice Escrow Contract
```rust
// Create new invoice escrow
pub fn create_escrow(
    env: Env,
    invoice_id: Symbol,
    seller: Address,
    amount: i128,
    due_date: u64,
    payment_token: Address
)

// Fund escrow (investor buys invoice)
pub fn fund_escrow(
    env: Env,
    invoice_id: Symbol,
    buyer: Address
)

// Record payment and distribute funds
pub fn record_payment(
    env: Env,
    invoice_id: Symbol,
    payer: Address,
    amount: i128
)
```

Full API documentation: [docs/API.md](docs/API.md)

## 📜 How to run scripts

### 1. Prerequisites

- Soroban CLI installed (`cargo install --locked soroban-cli --features opt`)
- Contracts compiled: `soroban contract build`

### 2. Configure environment variables

```bash
# Copy the template and fill in your values
cp .env.example .env
$EDITOR .env   # set STELLAR_SECRET_KEY, ADMIN_PUBLIC_KEY, STELLAR_NETWORK, etc.
```

See [`.env.example`](.env.example) for the full list of variables and their descriptions.

> ⚠️ **Never commit `.env` to version control.** It is already listed in `.gitignore`.

### 3a. Bash (macOS / Linux)

```bash
# Interactive path (wizard + menu)
bash scripts/cli.sh

# Or run the deploy script directly from the repo root
bash scripts/deploy.sh
```

The script:
1. Loads `.env` automatically
2. Validates all required variables and WASM paths
3. Deploys **invoice-token**, **invoice-escrow**, and **payment-distributor** in order
4. Calls `initialize` on each contract with the configured arguments
5. Calls `invoice-escrow.set_payment_distributor(...)` to enable distributor-based payouts
6. Prints a summary of deployed contract IDs

### 3c. Mock payment token (local integration testing)

To exercise a flow locally you need a payment asset, and the only one the
existing smoke test accepts is a real USDC contract address. `deploy-mock-token.sh`
deploys a throwaway SEP-41 equivalent in one command, with no dependency on the
escrow, invoice token, or distributor:

```bash
# Local standalone network
MOCK_TOKEN_SECRET_KEY=S... bash scripts/deploy-mock-token.sh

# Print the commands without touching the network
bash scripts/deploy-mock-token.sh --dry-run
```

It deploys the repo's own `invoice-token` WASM as the mock, initialises it, mints
a balance to the deployer plus any `MOCK_TOKEN_RECIPIENTS` you pass as
`addr:amount` pairs, prints the contract ID, and writes it to
`target/mock-token-id.txt`.

The mock is publicly mintable, so the script refuses to deploy one to mainnet.

### 3b. PowerShell (Windows)

```powershell
# Allow script execution for this session (if not already set)
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass

# Run from the repo root
.\scripts\deploy.ps1
```

The PowerShell script is functionally equivalent to the Bash script above.

### Re-using existing contract IDs

If a contract is already deployed and only needs re-initialisation, set its ID in `.env`:

```dotenv
INVOICE_ESCROW_CONTRACT_ID=C...
INVOICE_TOKEN_CONTRACT_ID=C...
PAYMENT_DISTRIBUTOR_CONTRACT_ID=C...
```

The deploy step is skipped for any contract whose ID is pre-filled; `initialize` is still called, and the escrow-to-distributor wiring step still runs.

## 🧪 Testing
```bash
# Run all tests
cargo test

# Run specific contract tests
cargo test --package invoice-escrow

# Run with output
cargo test -- --nocapture

# Coverage report
cargo tarpaulin --out Html
```

## 🔒 Security

- **Audit Status**: Smart contracts audit prep in progress. Formal security audit schedule trackable via [SECURITY.md](SECURITY.md).
- **Automated Analysis**: Continuous security scanning via GitHub Actions (Cargo Audit, Clippy, Tarpaulin).
- **Responsible Disclosure**: Please report security vulnerabilities to `security@stellarsettle.com`. See [SECURITY.md](SECURITY.md) for vulnerability disclosure guidelines.
- **Bug Bounty Program**: Active community bounties managed via GitHub Issues & Opire.

## 📊 Contract Addresses

> Contract IDs are dynamically generated at the end of each `deploy.sh` / `deploy.ps1` execution and persisted to `.env`.

### Testnet
Refer to [`.env.example`](.env.example) and your local `.env` configuration generated after running:
```bash
# Execute deployment script to generate contract IDs
bash scripts/deploy.sh
```
Contract IDs generated upon deployment:
- **Invoice Escrow**: `INVOICE_ESCROW_CONTRACT_ID` (see `.env`)
- **Invoice Token**: `INVOICE_TOKEN_CONTRACT_ID` (see `.env`)
- **Payment Distributor**: `PAYMENT_DISTRIBUTOR_CONTRACT_ID` (see `.env`)

### Mainnet
- Scheduled after audit completion and formal security sign-off.

## 🤝 Contributing

We welcome contributions! Please read our [Contributing Guide](./CONTRIBUTING.md) before submitting a Pull Request.

**Important**: All Pull Requests must target the `dev` branch. See our [branching workflow](./CONTRIBUTING.md#-branching--pull-request-workflow) for details.

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details

## 🔗 Links

- [Main Website](https://stellarsettle.com)
- [Documentation](https://docs.stellarsettle.com)
- [API Backend](https://github.com/stellarsettle/stellarsettle-api)
- [Web App](https://github.com/stellarsettle/stellarsettle-app)

---

Implemented initial updates to the test suite to align with the new soroban-sdk 27.x ContractEvents API:

Replaced direct events.last() calls with events.events().last() where appropriate.
Updated length checks to use .events().len().
Adjusted iterator usage for event verification, switching to .events().iter() and .events().rev() as needed.
Modified event loop logic to work with the new slice‑based API (events_after.get(i)).
These changes address the compilation errors caused by the breaking API change. Further refactoring may be needed for remaining events.last() and iterator patterns.
