#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

# The packaged desktop artifact, launched for real and read back independently.
#
# Exit code 3 means the check is Windows-only and this platform is not Windows;
# that is a documented not-applicable result, not a pass, and it is printed as
# such. Any other non-zero code fails the gate.
if [ -f target/release/vector-desktop.exe ]; then
  python3 scripts/desktop-live-fire.py \
    --report .agent/evidence/EP-001/desktop-live-fire.json || {
      code=$?
      if [ "$code" -ne 3 ]; then
        exit "$code"
      fi
    }
else
  echo "live-fire: no packaged artifact at target/release/vector-desktop.exe; skipping the packaged-app launch check" >&2
fi

corepack pnpm test:live-fire
