# StellarSettle Deployment & Initialization Guide

Step-by-step tutorial for building, deploying, and initializing the full StellarSettle contract suite on Stellar Testnet and Mainnet using `soroban-cli`.

---

## Prerequisites

- **Rust:** `1.80.0` or higher with `wasm32v1-none` target.
- **Soroban CLI:** `v22.0.0` or higher (`cargo install --locked soroban-cli`).
- **Account:** A funded Stellar keypair on Testnet (`soroban keys generate admin --network testnet`).

---

## Step 1: Build Optimized WASM Binaries

```bash
# Compile release WASMs
cargo build --release --target wasm32v1-none

# Optimize WASM binaries (optional but recommended)
soroban contract optimize --wasm target/wasm32v1-none/release/invoice_escrow.wasm
soroban contract optimize --wasm target/wasm32v1-none/release/invoice_token.wasm
soroban contract optimize --wasm target/wasm32v1-none/release/payment_distributor.wasm
```

---

## Step 2: Deploy Contracts

```bash
# 1. Deploy Invoice Escrow
ESCROW_ID=$(soroban contract deploy \
  --wasm target/wasm32v1-none/release/invoice_escrow.wasm \
  --source admin \
  --network testnet)
echo "Escrow Contract ID: $ESCROW_ID"

# 2. Deploy Invoice Token
TOKEN_ID=$(soroban contract deploy \
  --wasm target/wasm32v1-none/release/invoice_token.wasm \
  --source admin \
  --network testnet)
echo "Invoice Token Contract ID: $TOKEN_ID"

# 3. Deploy Payment Distributor
DISTRIBUTOR_ID=$(soroban contract deploy \
  --wasm target/wasm32v1-none/release/payment_distributor.wasm \
  --source admin \
  --network testnet)
echo "Payment Distributor Contract ID: $DISTRIBUTOR_ID"
```

---

## Step 3: Initialize Contracts

```bash
# Initialize Escrow Contract
soroban contract invoke \
  --id $ESCROW_ID \
  --source admin \
  --network testnet \
  -- initialize \
  --admin $(soroban keys address admin)

# Initialize Payment Distributor with 50 BPS (0.5%) fee
soroban contract invoke \
  --id $DISTRIBUTOR_ID \
  --source admin \
  --network testnet \
  -- initialize \
  --admin $(soroban keys address admin) \
  --fee_bps 50
```

---

## Step 4: Verification

```bash
# Query Escrow Admin
soroban contract invoke \
  --id $ESCROW_ID \
  --source admin \
  --network testnet \
  -- get_admin
```

---

## References

- Contract implementation: [`contracts/invoice-escrow/src/lib.rs`](../contracts/invoice-escrow/src/lib.rs)
- Architecture overview: [`docs/ARCHITECTURE.md`](ARCHITECTURE.md)
- Gas benchmarks: [`docs/benchmarks.md`](benchmarks.md)
