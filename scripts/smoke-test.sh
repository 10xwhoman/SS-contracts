#!/usr/bin/env bash
# smoke-test.sh — StellarSettle testnet smoke test
#
# Deploys (or re-uses) the invoice-escrow, invoice-token, and
# payment-distributor contracts on Stellar testnet and runs one
# minimal happy-path lifecycle: create → fund → settle.
#
# Required environment variables (copy from .env.example and fill in):
#   STELLAR_NETWORK      testnet | futurenet | mainnet  (default: testnet)
#   ADMIN_SECRET         admin Stellar secret key
#   SELLER_SECRET        seller Stellar secret key
#   BUYER_SECRET         buyer Stellar secret key
#   PAYER_SECRET         payer Stellar secret key (settles the invoice)
#   USDC_TOKEN_ADDRESS   address of the USDC / payment token contract
#
# Optional — skip deployment and reuse existing contracts:
#   ESCROW_CONTRACT_ID
#   INV_TOKEN_CONTRACT_ID
#   DISTRIBUTOR_CONTRACT_ID
#
# Usage:
#   chmod +x scripts/smoke-test.sh
#   source .env && ./scripts/smoke-test.sh

set -euo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
RPC_URL="https://soroban-testnet.stellar.org"
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"

if [[ "$NETWORK" == "mainnet" ]]; then
  RPC_URL="https://mainnet.sorobanrpc.com"
  NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
fi

INVOICE_ID="SMOKE$(date +%s)"

log() { echo "[smoke] $*"; }
assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "[FAIL] $label: expected=$expected actual=$actual" >&2
    exit 1
  fi
  log "[PASS] $label"
}

# ── 1. Build WASM ──────────────────────────────────────────────────────────────
log "Building WASM targets..."
cargo build --target wasm32v1-none --release \
  -p invoice-escrow -p invoice-token -p payment-distributor

ESCROW_WASM="target/wasm32v1-none/release/invoice_escrow.wasm"
INV_TOKEN_WASM="target/wasm32v1-none/release/invoice_token.wasm"
DISTRIBUTOR_WASM="target/wasm32v1-none/release/payment_distributor.wasm"

# ── 2. Deploy contracts (skip if IDs supplied) ─────────────────────────────────
deploy_contract() {
  local wasm="$1" label="$2"
  log "Deploying $label..."
  stellar contract deploy \
    --wasm "$wasm" \
    --source "$ADMIN_SECRET" \
    --network "$NETWORK" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE"
}

ESCROW_CONTRACT_ID="${ESCROW_CONTRACT_ID:-$(deploy_contract "$ESCROW_WASM" invoice-escrow)}"
INV_TOKEN_CONTRACT_ID="${INV_TOKEN_CONTRACT_ID:-$(deploy_contract "$INV_TOKEN_WASM" invoice-token)}"
DISTRIBUTOR_CONTRACT_ID="${DISTRIBUTOR_CONTRACT_ID:-$(deploy_contract "$DISTRIBUTOR_WASM" payment-distributor)}"

log "ESCROW_CONTRACT_ID=$ESCROW_CONTRACT_ID"
log "INV_TOKEN_CONTRACT_ID=$INV_TOKEN_CONTRACT_ID"
log "DISTRIBUTOR_CONTRACT_ID=$DISTRIBUTOR_CONTRACT_ID"

invoke() {
  local contract="$1" fn="$2" source="$3"
  shift 3
  stellar contract invoke \
    --id "$contract" \
    --source "$source" \
    --network "$NETWORK" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    -- "$fn" "$@"
}

# ── 3. Initialize contracts ────────────────────────────────────────────────────
ADMIN_PUB=$(stellar keys address "$ADMIN_SECRET" 2>/dev/null || \
            stellar keys show --source "$ADMIN_SECRET" | grep "Public Key" | awk '{print $NF}')
SELLER_PUB=$(stellar keys address "$SELLER_SECRET" 2>/dev/null || true)
BUYER_PUB=$(stellar keys address "$BUYER_SECRET" 2>/dev/null || true)
PAYER_PUB=$(stellar keys address "$PAYER_SECRET" 2>/dev/null || true)
AMOUNT=1000

log "Initializing invoice-token..."
invoke "$INV_TOKEN_CONTRACT_ID" initialize "$ADMIN_SECRET" \
  --admin "$ADMIN_PUB" \
  --name "Smoke Invoice" \
  --symbol "SINV" \
  --decimals 18 \
  --invoice_id "$INVOICE_ID" \
  --minter "$ESCROW_CONTRACT_ID"

log "Initializing invoice-escrow..."
invoke "$ESCROW_CONTRACT_ID" initialize "$ADMIN_SECRET" \
  --admin "$ADMIN_PUB" \
  --platform_fee_bps 300

log "Initializing payment-distributor..."
invoke "$DISTRIBUTOR_CONTRACT_ID" initialize "$ADMIN_SECRET" \
  --admin "$ADMIN_PUB"

log "Wiring invoice-escrow to payment-distributor..."
invoke "$ESCROW_CONTRACT_ID" set_payment_distributor "$ADMIN_SECRET" \
  --payment_distributor "$DISTRIBUTOR_CONTRACT_ID"

# ── 4. Create escrow ───────────────────────────────────────────────────────────
DUE_DATE=$(( $(date +%s) + 86400 ))   # 24 hours from now
log "Creating escrow invoice_id=$INVOICE_ID amount=$AMOUNT due_date=$DUE_DATE..."
invoke "$ESCROW_CONTRACT_ID" create_escrow "$SELLER_SECRET" \
  --invoice_id "$INVOICE_ID" \
  --seller "$SELLER_PUB" \
  --amount "$AMOUNT" \
  --due_date "$DUE_DATE" \
  --payment_token "$USDC_TOKEN_ADDRESS" \
  --invoice_token "$INV_TOKEN_CONTRACT_ID"

STATUS=$(invoke "$ESCROW_CONTRACT_ID" get_escrow_status "$ADMIN_SECRET" --invoice_id "$INVOICE_ID")
log "Status after create: $STATUS"
assert_eq "status=Created" "Created" "$STATUS"

# ── 5. Fund escrow ─────────────────────────────────────────────────────────────
log "Funding escrow (buyer=$BUYER_PUB)..."
invoke "$ESCROW_CONTRACT_ID" fund_escrow "$BUYER_SECRET" \
  --invoice_id "$INVOICE_ID" \
  --buyer "$BUYER_PUB"

STATUS=$(invoke "$ESCROW_CONTRACT_ID" get_escrow_status "$ADMIN_SECRET" --invoice_id "$INVOICE_ID")
log "Status after fund: $STATUS"
assert_eq "status=Funded" "Funded" "$STATUS"

# ── 6. Settle (record payment) ─────────────────────────────────────────────────
log "Recording payment (payer=$PAYER_PUB amount=$AMOUNT)..."
invoke "$ESCROW_CONTRACT_ID" record_payment "$PAYER_SECRET" \
  --invoice_id "$INVOICE_ID" \
  --payer "$PAYER_PUB" \
  --amount "$AMOUNT"

STATUS=$(invoke "$ESCROW_CONTRACT_ID" get_escrow_status "$ADMIN_SECRET" --invoice_id "$INVOICE_ID")
log "Status after settlement: $STATUS"
assert_eq "status=Settled" "Settled" "$STATUS"

# ── 7. Done ────────────────────────────────────────────────────────────────────
log "Smoke test PASSED."
log "  ESCROW_CONTRACT_ID=$ESCROW_CONTRACT_ID"
log "  INV_TOKEN_CONTRACT_ID=$INV_TOKEN_CONTRACT_ID"
log "  DISTRIBUTOR_CONTRACT_ID=$DISTRIBUTOR_CONTRACT_ID"
