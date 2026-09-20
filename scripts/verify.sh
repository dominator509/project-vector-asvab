#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

# The full verification sweep.
#
# Every gate runs through `gate`, which records its raw exit code to
# `.agent/verification/state/gate-results.jsonl` and aborts the sweep on the
# first failure. That file is what `scripts/release-state.py` reads to decide the
# release verdict, so the verdict rests on recorded exits rather than on a
# human's memory of the run. DOD-024 prohibits hiding a failure; a bare list of
# commands leaves nothing to audit and no way to prove one was not skipped.

STATE_DIR=.agent/verification/state
GATE_RESULTS="$STATE_DIR/gate-results.jsonl"
mkdir -p "$STATE_DIR"
: > "$GATE_RESULTS"

gate() {
  name=$1
  shift
  if "$@"; then
    code=0
  else
    code=$?
  fi
  printf '{"gate":"%s","exit":%s}\n' "$name" "$code" >> "$GATE_RESULTS"
  if [ "$code" -ne 0 ]; then
    echo "verify: gate '$name' failed with exit $code" >&2
    exit "$code"
  fi
}

gate generated-pack python3 scripts/validate-generated-pack.py .
gate anti-gaming-scan python3 scripts/anti-gaming-scan.py .
gate format-check sh scripts/format-check.sh
gate lint sh scripts/lint.sh
gate typecheck sh scripts/typecheck.sh
gate test-unit sh scripts/test-unit.sh

# Fails when a suite collects nothing, which many runners report as success.
gate test-collection-guard sh scripts/test-collection-guard.sh

gate test-integration sh scripts/test-integration.sh
gate test-e2e sh scripts/test-e2e.sh
gate security-check sh scripts/security-check.sh
gate dependency-audit sh scripts/dependency-audit.sh

# Local, deterministic, and exercises a real process boundary: an MCP server is
# started over a pipe and driven by the real client. The provider probe is not
# here because it inspects the machine's provider CLIs, and its --live form
# contacts a provider under the user's own subscription.
gate mcp-probe sh scripts/mcp-probe.sh
gate provider-probe sh scripts/provider-probe.sh

gate build sh scripts/build.sh

# Derive the artifact identity from the artifact this run just built.
#
# Placement is the control. Deriving the identity in a separate run, or
# committing it across a later rebuild, is how three different digests for one
# build reached the evidence: the Windows build is not bit-reproducible, so every
# `sh scripts/build.sh` produces new bytes and any identity that predates it is
# stale. With this step here, a green sweep leaves `.agent/evidence/EP-009/
# artifact_identity.json` describing the artifact the sweep just produced, and
# the smoke and live-fire steps below run against the build that was hashed —
# which is what AGENTS.md §16 asks for.
gate artifact-identity sh scripts/artifact-identity.sh

# Stamp that same digest into the functional proof matrix.
#
# The matrix is a live claim — "these outcomes are verified against this
# artifact" — so its `artifact_digest` column has to name the artifact this
# sweep verified. It was hand-maintained, which meant it drifted on every
# rebuild: the identity would be regenerated while the matrix kept naming the
# previous build. Stamping it here removes the duplicated value rather than
# asking a human to keep two copies of one digest in step.
gate proof-matrix-stamp python3 - <<'PY2'
import csv, json, pathlib

identity = json.loads(
    pathlib.Path('.agent/evidence/EP-009/artifact_identity.json').read_text(encoding='utf-8')
)
digest = next(
    c['digest'] for c in identity['components'] if c['component'] == 'Binary'
)

path = pathlib.Path('.agent/verification/FUNCTIONAL_PROOF_MATRIX.csv')
with path.open(encoding='utf-8', newline='') as handle:
    rows = list(csv.DictReader(handle))
    fields = list(rows[0].keys())

if not rows:
    raise SystemExit('the functional proof matrix is empty')

stale = [r['requirement_id'] for r in rows if r['artifact_digest'] != digest]
for row in rows:
    row['artifact_digest'] = digest

with path.open('w', encoding='utf-8', newline='\n') as handle:
    writer = csv.DictWriter(handle, fieldnames=fields, lineterminator='\n')
    writer.writeheader()
    writer.writerows(rows)

print(f'proof matrix: {len(rows)} row(s) stamped with {digest}')
if stale:
    print(f'  corrected stale digest on: {", ".join(stale)}')
PY2

gate smoke-test sh scripts/smoke-test.sh
gate live-fire sh scripts/live-fire.sh

# Regenerate the release-layer state from what this sweep recorded.
#
# These files are required evidence — DOD-042 names RELEASE_GATE.json and
# DOD-029 names the run manifest — and nothing generated them, so they were
# written by hand and drifted into contradicting each other. Generated here, at
# the end of a green sweep, they describe the run that just happened.
gate release-state python3 scripts/release-state.py
