#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 scripts/validate-generated-pack.py .
python3 scripts/anti-gaming-scan.py .
sh scripts/format-check.sh
sh scripts/lint.sh
sh scripts/typecheck.sh
sh scripts/test-unit.sh
sh scripts/test-integration.sh
sh scripts/test-e2e.sh
sh scripts/security-check.sh
sh scripts/dependency-audit.sh
sh scripts/build.sh
sh scripts/smoke-test.sh
sh scripts/live-fire.sh
