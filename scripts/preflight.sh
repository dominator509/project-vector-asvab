#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

mkdir -p .agent/evidence/PREFLIGHT
python3 scripts/validate-generated-pack.py .
printf '%s\n' "graphlock_pack=ok" > .agent/evidence/PREFLIGHT/pack.txt
for cmd in git python3 rustc cargo node corepack; do
  if command -v "$cmd" >/dev/null 2>&1; then "$cmd" --version 2>&1 | head -1 || true; else printf '%s\n' "$cmd=ABSENT"; fi
done
printf '%s\n' "Preflight records capability presence; optional absences do not become fake passes."
