#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

artifact=$(find release-artifacts -type f -maxdepth 2 2>/dev/null | head -1 || true)
[ -n "$artifact" ] || { echo "no release artifact found" >&2; exit 3; }
mkdir -p .agent/evidence/V-020
sha256sum "$artifact" | tee .agent/evidence/V-020/artifact.sha256
