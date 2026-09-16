#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

# The full release gate.
#
# `verify.sh` includes the artifact-identity step immediately after the build, so
# the identity it leaves behind describes the artifact this run produced. It is
# deliberately not repeated here: re-deriving it would only re-measure the same
# files, and a second derivation site is a second place for the ordering to drift.
sh scripts/verify.sh
sh scripts/dod-gate.sh
