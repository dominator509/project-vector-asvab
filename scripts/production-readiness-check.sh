#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

# The full release gate.
#
# `verify.sh` records every gate's exit code, derives the artifact identity from
# the build it produced, stamps the proof matrix, and regenerates the
# release-layer state — verdict, run manifest, DoD status, evidence index and
# reports — from what it recorded. None of that is duplicated here: a second
# derivation site is a second place for the ordering to drift.
sh scripts/verify.sh
sh scripts/dod-gate.sh
