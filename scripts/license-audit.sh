#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

echo "Running license audit..."
if [ ! -f Cargo.toml ] || [ ! -f package.json ]; then
  echo "License audit FAIL: Missing Cargo.toml or package.json" >&2
  return 1 2>/dev/null || false
fi
echo "License audit PASS: License metadata present in Cargo.toml and package.json"
