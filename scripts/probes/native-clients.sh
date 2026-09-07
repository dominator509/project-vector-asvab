#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
for x in codex grok claude; do command -v "$x" >/dev/null 2>&1 && "$x" --version || true; done
