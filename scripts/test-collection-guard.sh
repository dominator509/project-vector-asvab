#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

if [ "$#" -lt 1 ]; then echo "usage: sh scripts/test-collection-guard.sh COUNT" >&2; exit 2; fi
[ "$1" -gt 0 ] || { echo "required test collection is zero" >&2; exit 1; }
