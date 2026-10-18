#!/usr/bin/env bash
# =============================================================================
# StellarSettle – Toolchain Synchronization Check (#452)
# =============================================================================
#
# Verifies that rust-toolchain.toml stays in sync with every GitHub Actions
# workflow matrix/pin and every hard-coded wasm target in scripts/docs:
#
#   - Workflow `toolchain:` values must equal rust-toolchain.toml `channel`.
#   - dtolnay/rust-toolchain@REF (REF not master/main) must equal `channel`.
#   - Workflow `target:`/`targets:` values and `--target X` flags must appear
#     in rust-toolchain.toml `targets`.
#   - Hard-coded `wasm32-*` strings in scripts/docs must appear in `targets`.
#
# Usage:
#   bash scripts/check-toolchain-sync.sh
#
# Exit codes:
#   0  everything in sync
#   1  mismatch found (file:line reported on stderr)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

fail=0
err() { echo "[toolchain-sync] MISMATCH: $*" >&2; fail=1; }

# ---------------------------------------------------------------------------
# Parse rust-toolchain.toml (source of truth)
# ---------------------------------------------------------------------------
toolchain_file="rust-toolchain.toml"
[[ -f "${toolchain_file}" ]] || { err "${toolchain_file} not found"; exit 1; }

channel=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "${toolchain_file}" | head -n1)
if [[ -z "${channel}" ]]; then
  err "could not parse 'channel' from ${toolchain_file}"
  exit 1
fi

targets_line=$(sed -n 's/^[[:space:]]*targets[[:space:]]*=[[:space:]]*\[\(.*\)\].*/\1/p' "${toolchain_file}" | head -n1)
toolchain_targets=()
if [[ -n "${targets_line}" ]]; then
  while IFS= read -r t; do
    [[ -n "${t}" ]] && toolchain_targets+=("${t}")
  done < <(echo "${targets_line}" | tr -d '"' | tr ',' '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
fi

if (( ${#toolchain_targets[@]} == 0 )); then
  err "could not parse 'targets' from ${toolchain_file}"
  exit 1
fi

echo "[toolchain-sync] source of truth: channel=${channel} targets=[${toolchain_targets[*]}]"

target_known() {
  local t="$1"
  local known
  for known in "${toolchain_targets[@]}"; do
    [[ "${t}" == "${known}" ]] && return 0
  done
  return 1
}

# ---------------------------------------------------------------------------
# 1. Workflows: toolchain values, dtolnay REF pins, target:/targets:/--target
# ---------------------------------------------------------------------------
workflow_files=(.github/workflows/*.yml .github/workflows/*.yaml)
for wf in ${workflow_files[@]+"${workflow_files[@]}"}; do
  [[ -f "${wf}" ]] || continue

  # matrix/static `toolchain:` values (skip expressions like ${{ ... }})
  while IFS= read -r line; do
    lineno="${line%%:*}"
    rest="${line#*:}"
    val=$(echo "${rest}" | sed -n 's/^[[:space:]]*toolchain:[[:space:]]*\([^$[:space:]][^#]*\).*/\1/p' | tr -d '"' | tr -d "'" | sed 's/[[:space:]]*$//')
    # matrix form: toolchain: [stable] or [a, b]
    val="${val#[}"
    val="${val%]}"
    if [[ -n "${val}" ]]; then
      IFS=',' read -ra parts <<< "${val}"
      for p in "${parts[@]}"; do
        p="$(echo "${p}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
        [[ -z "${p}" || "${p}" == '$'* ]] && continue
        if [[ "${p}" != "${channel}" ]]; then
          err "${wf}:${lineno} toolchain '${p}' != rust-toolchain.toml channel '${channel}'"
        fi
      done
    fi
  done < <(grep -n 'toolchain:' "${wf}" || true)

  # dtolnay/rust-toolchain@REF where REF is not a floating branch
  while IFS= read -r line; do
    lineno="${line%%:*}"
    ref=$(echo "${line}" | sed -n 's|.*dtolnay/rust-toolchain@\([A-Za-z0-9._/-]*\).*|\1|p')
    case "${ref}" in
      ""|master|main) ;; # floating; channel comes from `with.toolchain` (checked above)
      *)
        if [[ "${ref}" != "${channel}" ]]; then
          err "${wf}:${lineno} dtolnay/rust-toolchain@${ref} != rust-toolchain.toml channel '${channel}'"
        fi
        ;;
    esac
  done < <(grep -n 'dtolnay/rust-toolchain@' "${wf}" || true)

  # target: / targets: values (skip expressions)
  while IFS= read -r line; do
    lineno="${line%%:*}"
    rest="${line#*:}"
    val=$(echo "${rest}" | sed -n 's/^[[:space:]]*targets\?:[[:space:]]*\([^$[:space:]][^#]*\).*/\1/p' | tr -d '"' | tr -d "'" | sed 's/[[:space:]]*$//')
    # matrix form: target: [wasm32v1-none]
    val="${val#[}"
    val="${val%]}"
    if [[ -n "${val}" ]] && ! target_known "${val}"; then
      err "${wf}:${lineno} target '${val}' not in rust-toolchain.toml targets [${toolchain_targets[*]}]"
    fi
  done < <(grep -nE '^[[:space:]]*targets?:' "${wf}" || true)

  # cargo/stellar --target X hard-coded flags (skip shell vars and matrix refs)
  while IFS= read -r line; do
    lineno="${line%%:*}"
    for t in $(echo "${line}" | grep -oE -- '--target[[:space:]]+[A-Za-z0-9_-]+' | awk '{print $2}'); do
      case "${t}" in
        '$'*) ;; # expression / shell variable
        *)
          if ! target_known "${t}"; then
            err "${wf}:${lineno} --target ${t} not in rust-toolchain.toml targets [${toolchain_targets[*]}]"
          fi
          ;;
      esac
    done
  done < <(grep -n -- '--target' "${wf}" || true)
done

# ---------------------------------------------------------------------------
# 2. Scripts & docs: hard-coded wasm32-* targets
# ---------------------------------------------------------------------------
scan_files=(scripts/*.sh scripts/*.ps1 .env.example DEVELOPMENT.md README.md CONTRIBUTING.md docs/*.md)
for f in ${scan_files[@]+"${scan_files[@]}"}; do
  [[ -f "${f}" ]] || continue
  while IFS= read -r line; do
    lineno="${line%%:*}"
    for t in $(echo "${line}" | grep -oE 'wasm32-[A-Za-z0-9_-]+' | sort -u); do
      if ! target_known "${t}"; then
        err "${f}:${lineno} wasm target '${t}' not in rust-toolchain.toml targets [${toolchain_targets[*]}]"
      fi
    done
  done < <(grep -nE 'wasm32-[A-Za-z0-9_-]+' "${f}" || true)
done

# ---------------------------------------------------------------------------
# 3. Same wasm target must not be mixed with a non-listed alias in package paths
# ---------------------------------------------------------------------------
for f in scripts/*.sh; do
  [[ -f "${f}" ]] || continue
  while IFS= read -r line; do
    lineno="${line%%:*}"
    for t in $(echo "${line}" | grep -oE 'target/wasm32-[A-Za-z0-9_-]+/release' | sed 's|target/||;s|/release||' | sort -u); do
      if ! target_known "${t}"; then
        err "${f}:${lineno} build path uses '${t}' which is not in rust-toolchain.toml targets [${toolchain_targets[*]}]"
      fi
    done
  done < <(grep -nE 'target/wasm32-[A-Za-z0-9_-]+/release' "${f}" || true)
done

if (( fail != 0 )); then
  echo "" >&2
  echo "[toolchain-sync] FAILED — update rust-toolchain.toml or the listed files so they agree." >&2
  exit 1
fi

echo "[toolchain-sync] OK — workflows, scripts, and docs are in sync with rust-toolchain.toml."
