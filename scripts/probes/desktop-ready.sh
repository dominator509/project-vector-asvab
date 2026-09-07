#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
test -f .agent/evidence/dev.pid && kill -0 $(cat .agent/evidence/dev.pid)
