#!/usr/bin/env bash
# =============================================================================
# StellarSettle – Standalone Mock Token Deployer
# =============================================================================
#
# Deploys and funds a SEP-41 mock payment token for local integration testing.
# Stands alone: it does not deploy or require the escrow, the invoice token, or
# the payment distributor, so an integration harness can be exercised end to end
# before any of the real contracts exist on the network.
#
# Why this is needed: `scripts/smoke-test.sh` expects USDC_TOKEN_ADDRESS to point
# at a real payment asset, which means the only way to test a local flow today is
# to fund a mainnet-flavoured asset by hand. This deploys a throwaway equivalent
# in one command.
#
# The mock is the repo's own `invoice-token` WASM. It is already SEP-41
# compliant, so no fourth contract has to be maintained just to be a test
# fixture.
#
# Usage:
#   # Standalone local network (the default)
#   bash scripts/deploy-mock-token.sh
#
#   # Against testnet
#   STELLAR_NETWORK=testnet \
#   MOCK_TOKEN_SECRET_KEY=S... \
#   bash scripts/deploy-mock-token.sh
#
#   # Print the commands without touching the network
#   bash scripts/deploy-mock-token.sh --dry-run
#
# Environment variables (all optional except the key):
#   STELLAR_NETWORK        local | testnet | futurenet   (default: local)
#   MOCK_TOKEN_SECRET_KEY  deployer/admin secret key
#   MOCK_TOKEN_NAME        token name       (default: "StellarSettle Mock USD")
#   MOCK_TOKEN_SYMBOL      ticker, <=12 chars (default: MOCKUSD)
#   MOCK_TOKEN_DECIMALS    decimal places   (default: 7, matches native)
#   MOCK_TOKEN_ADMIN       admin address    (default: derived from the key)
#   MOCK_TOKEN_MINTER      minter address   (default: the admin)
#   MOCK_TOKEN_MINT_AMOUNT amount to mint   (default: 10000000000)
#   MOCK_TOKEN_RECIPIENTS  extra "addr:amount" pairs, space separated
#   MOCK_TOKEN_ID          reuse an existing mock instead of deploying
#
# The resulting contract ID is printed on stdout and written to
# target/mock-token-id.txt for other scripts to read.
#
# Requirements:
#   - stellar CLI (or the legacy `soroban` alias)
#   - Contracts already built:  soroban contract build
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
RED='\033[0.31m'
GREEN='\033[0.32m'
CYAN='\033[0.36m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Colour

info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
die()     { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            sed -n '2,48p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            die "Unknown argument: $1 (try --help)"
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

WASM_MOCK_TOKEN="${WASM_MOCK_TOKEN:-target/wasm32v1-none/release/invoice_token.wasm}"
ID_FILE="${REPO_ROOT}/target/mock-token-id.txt"

# ---------------------------------------------------------------------------
# Load .env (if present). Optional: every value has a default or a flag, so
# this script runs with no .env at all.
# ---------------------------------------------------------------------------
ENV_FILE="${REPO_ROOT}/.env"
if [[ -f "${ENV_FILE}" ]]; then
    info "Loading environment from ${ENV_FILE}"
    set -o allexport
    # shellcheck disable=SC1090
    source "${ENV_FILE}"
    set +o allexport
fi

# ---------------------------------------------------------------------------
# Resolve the CLI. Repos have used both names; prefer the current one.
# ---------------------------------------------------------------------------
if command -v stellar >/dev/null 2>&1; then
    CLI=(stellar)
elif command -v soroban >/dev/null 2>&1; then
    CLI=(soroban)
else
    die "Neither 'stellar' nor 'soroban' CLI found on PATH. Install the Stellar CLI."
fi
info "Using CLI: ${CLI[*]}"

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
NETWORK="${STELLAR_NETWORK:-local}"
SECRET_KEY="${MOCK_TOKEN_SECRET_KEY:-${STELLAR_SECRET_KEY:-}}"
TOKEN_NAME="${MOCK_TOKEN_NAME:-StellarSettle Mock USD}"
TOKEN_SYMBOL="${MOCK_TOKEN_SYMBOL:-MOCKUSD}"
TOKEN_DECIMALS="${MOCK_TOKEN_DECIMALS:-7}"
MINT_AMOUNT="${MOCK_TOKEN_MINT_AMOUNT:-10000000000}"
RECIPIENTS="${MOCK_TOKEN_RECIPIENTS:-}"
EXISTING_ID="${MOCK_TOKEN_ID:-}"

# A mock token on a live network is a footgun waiting to happen: anyone can mint
# it, and a test harness that silently pays real USDC-equivalent balances to the
# wrong contract is worse than no harness at all.
if [[ "${NETWORK}" == "mainnet" ]] && [[ "${DRY_RUN}" != "true" ]]; then
    die "Refusing to deploy a publicly mintable mock token on mainnet."
fi

if [[ -z "${SECRET_KEY}" ]]; then
    die "Set MOCK_TOKEN_SECRET_KEY (or STELLAR_SECRET_KEY) to the deployer key."
fi

if [[ ${#TOKEN_SYMBOL} -gt 12 ]]; then
    die "MOCK_TOKEN_SYMBOL must be 12 characters or fewer (got ${#TOKEN_SYMBOL})."
fi

SOROBAN_FLAGS=(
    --source "${SECRET_KEY}"
    --network "${NETWORK}"
)

# ---------------------------------------------------------------------------
# Resolve the admin address from the deployer key when not given
# ---------------------------------------------------------------------------
if [[ -n "${MOCK_TOKEN_ADMIN:-}" ]]; then
    ADMIN="${MOCK_TOKEN_ADMIN}"
else
    if [[ "${DRY_RUN}" == "true" ]]; then
        ADMIN="GDRYRUNADMINADDRESS000000000000000000000000000000000"
    else
        ADMIN=$("${CLI[@]}" keys address "${SECRET_KEY}" 2>/dev/null) \
            || die "Could not derive an admin address from the secret key. Set MOCK_TOKEN_ADMIN."
    fi
fi

MINTER="${MOCK_TOKEN_MINTER:-${ADMIN}}"

# ---------------------------------------------------------------------------
# Deploy (or reuse)
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Mock token  –  ${NETWORK}"
echo "════════════════════════════════════════════════════════"
echo ""

if [[ -n "${EXISTING_ID}" ]]; then
    MOCK_TOKEN_ID="${EXISTING_ID}"
    warn "Reusing existing mock token ID: ${MOCK_TOKEN_ID}"
elif [[ "${DRY_RUN}" == "true" ]]; then
    MOCK_TOKEN_ID="C_MOCK_TOKEN_DRY_RUN"
    success "[DRY-RUN] mock token deployed → ${MOCK_TOKEN_ID}"
else
    [[ -f "${WASM_MOCK_TOKEN}" ]] \
        || die "WASM not found: ${WASM_MOCK_TOKEN}\n       Run 'soroban contract build' first."

    info "Deploying mock token from ${WASM_MOCK_TOKEN} …"
    MOCK_TOKEN_ID=$("${CLI[@]}" contract deploy \
        "${SOROBAN_FLAGS[@]}" \
        --wasm "${WASM_MOCK_TOKEN}")
    success "mock token deployed → ${MOCK_TOKEN_ID}"
fi

# ---------------------------------------------------------------------------
# Initialise. `invoice-token` takes (admin, name, symbol, decimals,
# invoice_id, minter); the invoice_id only has to be a valid Symbol.
# ---------------------------------------------------------------------------
INVOICE_ID="MOCK${TOKEN_SYMBOL:0:8}"

info "Initialising mock token …"
info "  admin      = ${ADMIN}"
info "  name       = ${TOKEN_NAME}"
info "  symbol     = ${TOKEN_SYMBOL}"
info "  decimals   = ${TOKEN_DECIMALS}"
info "  invoice_id = ${INVOICE_ID}"
info "  minter     = ${MINTER}"

if [[ "${DRY_RUN}" == "true" ]]; then
    success "[DRY-RUN] mock token initialised"
else
    "${CLI[@]}" contract invoke \
        "${SOROBAN_FLAGS[@]}" \
        --id "${MOCK_TOKEN_ID}" \
        -- initialize \
        --admin      "${ADMIN}" \
        --name       "${TOKEN_NAME}" \
        --symbol     "${TOKEN_SYMBOL}" \
        --decimals   "${TOKEN_DECIMALS}" \
        --invoice_id "${INVOICE_ID}" \
        --minter     "${MINTER}" >/dev/null
    success "mock token initialised"
fi

# ---------------------------------------------------------------------------
# Fund. The admin is minter-by-default, so it can mint to itself and to any
# test account. `mint` is (to, amount, by).
# ---------------------------------------------------------------------------
mint_to() {
    local to="$1"
    local amount="$2"

    if [[ "${DRY_RUN}" == "true" ]]; then
        success "[DRY-RUN] minted ${amount} → ${to}"
        return
    fi

    info "Minting ${amount} → ${to}"
    "${CLI[@]}" contract invoke \
        "${SOROBAN_FLAGS[@]}" \
        --id "${MOCK_TOKEN_ID}" \
        -- mint \
        --to     "${to}" \
        --amount "${amount}" \
        --by     "${MINTER}" >/dev/null
    success "minted ${amount} → ${to}"
}

echo ""
info "Funding mock token …"
mint_to "${ADMIN}" "${MINT_AMOUNT}"

if [[ -n "${RECIPIENTS}" ]]; then
    for pair in ${RECIPIENTS}; do
        addr="${pair%%:*}"
        amt="${pair##*:}"
        [[ -n "${amt}" ]] || amt="${MINT_AMOUNT}"
        mint_to "${addr}" "${amt}"
    done
fi

# ---------------------------------------------------------------------------
# Persist the ID for other scripts
# ---------------------------------------------------------------------------
if [[ "${DRY_RUN}" != "true" ]]; then
    mkdir -p "$(dirname "${ID_FILE}")"
    printf '%s\n' "${MOCK_TOKEN_ID}" > "${ID_FILE}"
fi

# ---------------------------------------------------------------------------
# Result
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Done"
echo "════════════════════════════════════════════════════════"
echo ""
echo "  MOCK_TOKEN_ID = ${MOCK_TOKEN_ID}"
echo "  ID file       = ${ID_FILE}"
echo ""
echo "  Use it as the payment asset:"
echo "    export MOCK_TOKEN_ID=${MOCK_TOKEN_ID}"
echo ""
echo "  Read the balance of any address:"
echo "    ${CLI[*]} contract invoke --id ${MOCK_TOKEN_ID} --network ${NETWORK} \\"
echo "      -- balance --id \"<address>\""
echo ""
echo "  This token is publicly mintable by ${MINTER}. Never treat it as"
echo "  a real asset, and never deploy one to mainnet."
echo ""

# Contract ID on stdout, so this can be captured with $(...).
echo "${MOCK_TOKEN_ID}"
