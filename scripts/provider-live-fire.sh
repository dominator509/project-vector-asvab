#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh
mkdir -p .agent/evidence/EP-004

# Sends one minimal prompt ("Reply with exactly: READY") through every lane that
# probed healthy, and validates the response with the provider-output bounds.
#
# This is why it is a separate command from `scripts/provider-probe.sh`: it
# contacts the provider under the user's own login and consumes their quota or
# subscription. It is run deliberately, not as part of every verification sweep.
cargo run -p vector-tools -- provider probe --all-configured --live \
  --out .agent/evidence/EP-004/provider-probe-live.json
