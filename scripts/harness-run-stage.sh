#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

if [ "$#" -ne 1 ]; then echo "usage: sh scripts/harness-run-stage.sh V-xxx" >&2; exit 2; fi
stage=$1
plan=$(find .agent/verification/stage-plans -type f -name "$stage-*.md" -print -quit)
[ -n "$plan" ] || { echo "unknown verification stage $stage" >&2; exit 2; }
echo "Stage plan: $plan"
echo "Materialize applicable registry cases and execute through COMMANDS.md; blueprint does not manufacture execution results."
exit 3
