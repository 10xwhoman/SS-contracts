#!/usr/bin/env bash
# =============================================================================
# StellarSettle – Contract Deployment Script (Bash)
# =============================================================================
#
# Deploys and initialises the following Soroban contracts:
#   1. invoice-token      (SEP-41 invoice tokenisation)
#   2. invoice-escrow     (escrow lifecycle + settlement)
#   3. payment-distributor (settlement/refund payout fan-out)
#
# Usage:
#   # 1. Copy .env.example to .env and fill in your values
#   cp .env.example .env && $EDITOR .env
#
#   # 2. Run from the repo root
#   bash scripts/deploy.sh
#
# Environment variables (loaded from .env if present, or set externally):
#   See .env.example for the full list and documentation.
#
# Requirements:
#   - soroban-cli  (cargo install --locked soroban-cli --features opt)
#   - Contracts already built:  soroban contract build
# =============================================================================

set -euo pipefail

DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    *)
      shift
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Colour

info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
die()     { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Load .env (if present)
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

ENV_FILE="${REPO_ROOT}/.env"
if [[ -f "${ENV_FILE}" ]]; then
    info "Loading environment from ${ENV_FILE}"
    # Export variables, skipping comments and blank lines
    set -o allexport
    # shellcheck disable=SC1090
    source "${ENV_FILE}"
    set +o allexport
else
    warn ".env file not found – using environment variables already set in shell"
fi

# ---------------------------------------------------------------------------
# Validate required variables
# ---------------------------------------------------------------------------
: "${STELLAR_NETWORK:?  Set STELLAR_NETWORK (testnet | mainnet | futurenet)}"
: "${STELLAR_SECRET_KEY:? Set STELLAR_SECRET_KEY to your deployer secret key}"
: "${ADMIN_PUBLIC_KEY:?   Set ADMIN_PUBLIC_KEY  to the corresponding public key}"
: "${PLATFORM_FEE_BPS:?   Set PLATFORM_FEE_BPS  (e.g. 300 for 3%)}"
: "${INVOICE_TOKEN_NAME:?  Set INVOICE_TOKEN_NAME}"
: "${INVOICE_TOKEN_SYMBOL:? Set INVOICE_TOKEN_SYMBOL}"
: "${INVOICE_TOKEN_DECIMALS:? Set INVOICE_TOKEN_DECIMALS}"
: "${INVOICE_TOKEN_INVOICE_ID:? Set INVOICE_TOKEN_INVOICE_ID}"

WASM_INVOICE_ESCROW="${WASM_INVOICE_ESCROW:-target/wasm32v1-none/release/invoice_escrow.wasm}"
WASM_INVOICE_TOKEN="${WASM_INVOICE_TOKEN:-target/wasm32v1-none/release/invoice_token.wasm}"
WASM_PAYMENT_DISTRIBUTOR="${WASM_PAYMENT_DISTRIBUTOR:-target/wasm32v1-none/release/payment_distributor.wasm}"

# ---------------------------------------------------------------------------
# Verify WASM artifacts exist
# ---------------------------------------------------------------------------
cd "${REPO_ROOT}"

for wasm in "${WASM_INVOICE_ESCROW}" "${WASM_INVOICE_TOKEN}" "${WASM_PAYMENT_DISTRIBUTOR}"; do
    if [[ ! -f "${wasm}" ]]; then
        die "WASM not found: ${wasm}\n       Run 'soroban contract build' first."
    fi
done

# ---------------------------------------------------------------------------
# Shared soroban-cli flags
# ---------------------------------------------------------------------------
SOROBAN_FLAGS=(
    --source "${STELLAR_SECRET_KEY}"
    --network "${STELLAR_NETWORK}"
)

# ---------------------------------------------------------------------------
# deploy_contract <label> <wasm_path> [existing_id_var]
#   Prints and returns the contract ID.
#   If the variable named by existing_id_var is non-empty the deploy step
#   is skipped and that ID is reused.
# ---------------------------------------------------------------------------
deploy_contract() {
    local label="$1"
    local wasm="$2"
    local existing_id="${3:-}"

    if [[ -n "${existing_id}" ]]; then
        warn "${label}: Skipping deploy – reusing existing ID: ${existing_id}"
        echo "${existing_id}"
        return
    fi

    info "Deploying ${label} from ${wasm} …"
    
    if [[ "${DRY_RUN}" == "true" ]]; then
        contract_id="SIMULATED_${label^^}_ID"
        contract_id=$(echo "$contract_id" | tr - _ | tr -d - | tr '[:lower:]' '[:upper:]')
        success "[DRY-RUN] ${label} deployed → ${contract_id}"
        echo "${contract_id}"
        return
    fi

    local contract_id
    contract_id=$(soroban contract deploy \
        "${SOROBAN_FLAGS[@]}" \
        --wasm "${wasm}")

    success "${label} deployed → ${contract_id}"
    echo "${contract_id}"
}

# ---------------------------------------------------------------------------
# 1. Deploy invoice-token
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Step 1 / 3  –  invoice-token"
echo "════════════════════════════════════════════════════════"

INVOICE_TOKEN_ID=$(deploy_contract \
    "invoice-token" \
    "${WASM_INVOICE_TOKEN}" \
    "${INVOICE_TOKEN_CONTRACT_ID:-}")

# ---------------------------------------------------------------------------
# 2. Deploy invoice-escrow (needs invoice-token address at init time)
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Step 2 / 3  –  invoice-escrow"
echo "════════════════════════════════════════════════════════"

INVOICE_ESCROW_ID=$(deploy_contract \
    "invoice-escrow" \
    "${WASM_INVOICE_ESCROW}" \
    "${INVOICE_ESCROW_CONTRACT_ID:-}")

# ---------------------------------------------------------------------------
# 3. Deploy payment-distributor
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Step 3 / 3  –  payment-distributor"
echo "════════════════════════════════════════════════════════"

PAYMENT_DISTRIBUTOR_ID=$(deploy_contract \
    "payment-distributor" \
    "${WASM_PAYMENT_DISTRIBUTOR}" \
    "${PAYMENT_DISTRIBUTOR_CONTRACT_ID:-}")

# ---------------------------------------------------------------------------
# Initialise contracts
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Initialising contracts"
echo "════════════════════════════════════════════════════════"

# --- invoice-token.initialize ---
info "Initialising invoice-token …"
info "  admin        = ${ADMIN_PUBLIC_KEY}"
info "  name         = ${INVOICE_TOKEN_NAME}"
info "  symbol       = ${INVOICE_TOKEN_SYMBOL}"
info "  decimals     = ${INVOICE_TOKEN_DECIMALS}"
info "  invoice_id   = ${INVOICE_TOKEN_INVOICE_ID}"
info "  minter       = ${INVOICE_ESCROW_ID}  (escrow contract)"

if [[ "${DRY_RUN}" == "true" ]]; then
    success "[DRY-RUN] invoice-token initialised"
else
    soroban contract invoke \
        "${SOROBAN_FLAGS[@]}" \
        --id "${INVOICE_TOKEN_ID}" \
        -- initialize \
        --admin        "${ADMIN_PUBLIC_KEY}" \
        --name         "${INVOICE_TOKEN_NAME}" \
        --symbol       "${INVOICE_TOKEN_SYMBOL}" \
        --decimals     "${INVOICE_TOKEN_DECIMALS}" \
        --invoice_id   "${INVOICE_TOKEN_INVOICE_ID}" \
        --minter       "${INVOICE_ESCROW_ID}"
fi

success "invoice-token initialised"

# --- invoice-escrow.initialize ---
info "Initialising invoice-escrow …"
info "  admin            = ${ADMIN_PUBLIC_KEY}"
info "  platform_fee_bps = ${PLATFORM_FEE_BPS}"

if [[ "${DRY_RUN}" == "true" ]]; then
    success "[DRY-RUN] invoice-escrow initialised"
else
    soroban contract invoke \
        "${SOROBAN_FLAGS[@]}" \
        --id "${INVOICE_ESCROW_ID}" \
        -- initialize \
        --admin            "${ADMIN_PUBLIC_KEY}" \
        --platform_fee_bps "${PLATFORM_FEE_BPS}"
fi

success "invoice-escrow initialised"

# --- payment-distributor.initialize ---
info "Initialising payment-distributor …"
info "  admin = ${ADMIN_PUBLIC_KEY}"

if [[ "${DRY_RUN}" == "true" ]]; then
    success "[DRY-RUN] payment-distributor initialised"
else
    soroban contract invoke \
        "${SOROBAN_FLAGS[@]}" \
        --id "${PAYMENT_DISTRIBUTOR_ID}" \
        -- initialize \
        --admin "${ADMIN_PUBLIC_KEY}"
fi

success "payment-distributor initialised"

# --- invoice-escrow.set_payment_distributor ---
info "Wiring invoice-escrow to payment-distributor …"

if [[ "${DRY_RUN}" == "true" ]]; then
    success "[DRY-RUN] invoice-escrow wired to payment-distributor"
else
    soroban contract invoke \
        "${SOROBAN_FLAGS[@]}" \
        --id "${INVOICE_ESCROW_ID}" \
        -- set_payment_distributor \
        --payment_distributor "${PAYMENT_DISTRIBUTOR_ID}"
fi

success "invoice-escrow wired to payment-distributor"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Deployment complete  🚀"
echo "════════════════════════════════════════════════════════"
printf "  %-28s %s\n" "invoice-token:"       "${INVOICE_TOKEN_ID}"
printf "  %-28s %s\n" "invoice-escrow:"      "${INVOICE_ESCROW_ID}"
printf "  %-28s %s\n" "payment-distributor:" "${PAYMENT_DISTRIBUTOR_ID}"
echo ""
echo "  Network: ${STELLAR_NETWORK}"
echo ""
warn "Copy the contract IDs above into README.md → Contract Addresses."
