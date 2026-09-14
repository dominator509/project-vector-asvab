#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh
mkdir -p .agent/evidence/EP-004
# Probes every configured lane with the provider's own status command and writes
# the observed state. No prompt is sent: see scripts/provider-live-fire.sh for
# the check that contacts a provider.
cargo run -p vector-tools -- provider probe --all-configured \
  --out .agent/evidence/EP-004/provider-probe.json
