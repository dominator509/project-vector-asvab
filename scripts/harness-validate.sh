#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
python3 scripts/validate-generated-pack.py .
python3 scripts/validate-hash-ledger.py .agent/state/LEDGER.jsonl
