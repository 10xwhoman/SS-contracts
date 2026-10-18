#!/usr/bin/env bash
# Human-review inventory for panic/unwrap and likely arithmetic sites in contract Rust.
# This intentionally reports candidates instead of failing on matches: tests contain
# expected unwraps/panics, and a textual scan cannot establish exploitability.
set -euo pipefail

cd "$(dirname "$0")/.."
for pattern in 'panic!|\.unwrap\(|\.expect\(' 'checked_(add|sub|mul|div)|[[:alnum:]_)][[:space:]]*[+*][[:space:]]*[[:alnum:]_(]'; do
  printf '\n== %s ==\n' "$pattern"
  grep -RInE "$pattern" contracts/*/src --include='*.rs' || true
done
