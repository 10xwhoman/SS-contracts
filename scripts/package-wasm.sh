#!/usr/bin/env bash
# =============================================================================
# StellarSettle – WASM Packaging & Checksum Script
# =============================================================================
#
# Compiles all Soroban contracts to WASM and generates a deterministic
# SHA256 checksums file for release transparency and contract verification.
#
# Usage:
#   bash scripts/package-wasm.sh [output_dir]
#
# Arguments:
#   output_dir   Directory to place WASM files and checksums.txt
#                (default: dist/)
#
# Requirements:
#   - Rust with wasm32v1-none target installed
#   - sha256sum (coreutils)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

OUTPUT_DIR="${1:-${REPO_ROOT}/dist}"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
die()     { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Verify required tools
# ---------------------------------------------------------------------------
command -v sha256sum >/dev/null 2>&1 || die "sha256sum not found. Install coreutils."

# ---------------------------------------------------------------------------
# Build WASM contracts
# ---------------------------------------------------------------------------
info "Building WASM contracts..."
cd "${REPO_ROOT}"

cargo build --release --target wasm32v1-none \
  -p invoice-escrow -p invoice-token -p payment-distributor

WASM_DIR="target/wasm32v1-none/release"
REQUIRED_FILES=(
  "invoice_escrow.wasm"
  "invoice_token.wasm"
  "payment_distributor.wasm"
)

for wasm in "${REQUIRED_FILES[@]}"; do
  [[ -f "${WASM_DIR}/${wasm}" ]] || die "WASM not found: ${WASM_DIR}/${wasm}"
done

# ---------------------------------------------------------------------------
# Copy WASM artifacts to output directory
# ---------------------------------------------------------------------------
info "Packaging WASM artifacts to ${OUTPUT_DIR}..."
mkdir -p "${OUTPUT_DIR}"

for wasm in "${REQUIRED_FILES[@]}"; do
  cp "${WASM_DIR}/${wasm}" "${OUTPUT_DIR}/${wasm}"
  success "Copied ${wasm}"
done

# ---------------------------------------------------------------------------
# Generate SHA256 checksums
# ---------------------------------------------------------------------------
info "Generating checksums..."
cd "${OUTPUT_DIR}"

sha256sum "${REQUIRED_FILES[@]}" > checksums.txt

success "checksums.txt generated"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "════════════════════════════════════════════════════════"
echo "  Package complete"
echo "════════════════════════════════════════════════════════"
echo ""
cat checksums.txt
echo ""
info "Output directory: ${OUTPUT_DIR}"