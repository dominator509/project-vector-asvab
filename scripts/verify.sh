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
# Local, deterministic, and exercises a real process boundary: an MCP server is
# started over a pipe and driven by the real client. The provider probe is not
# here because it inspects the machine's provider CLIs, and its --live form
# contacts a provider under the user's own subscription.
sh scripts/mcp-probe.sh
sh scripts/provider-probe.sh
sh scripts/build.sh
sh scripts/smoke-test.sh
sh scripts/live-fire.sh
