#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh
sh scripts/verify.sh
sh scripts/artifact-identity.sh
sh scripts/dod-gate.sh
