#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

# The full verification sweep.
#
# Every gate runs through `gate`, which records its raw exit code to
# `.agent/verification/state/gate-results.jsonl`. That file is what
# `scripts/release-state.py` reads to decide the release verdict, so the verdict rests
# on recorded exits rather than on a human's memory of the run. DOD-024 prohibits
# hiding a failure; a bare list of commands leaves nothing to audit and no way to
# prove one was not skipped.
#
# **The sweep does not stop at the first failure.** It used to, and that contradicted
# DOD-031: a failed gate is a *prerequisite* for the gates that consume its artifact and
# for nothing else, and stopping meant eighteen gates reported nothing at all -- so one
# failure hid everything behind it, and a reader could not tell "this gate failed" from
# "we never got there". Now every gate runs unless one of the gates it declares as a
# prerequisite did not pass, and a gate in that position is recorded as
# `BLOCKED_PREREQUISITE` with the gate that blocked it. Blocked is not passed: the file
# records `"exit": null`, `release-state.py` reads that as "not run in this sweep", and
# the sweep exits non-zero if anything failed or was blocked.
#
# The edges are declared in `blocked_by`, and `scripts/dependency-graph.py` validates
# them against this file and the test accounting, so the graph is machine-checked rather
# than asserted in a comment.

STATE_DIR=.agent/verification/state
GATE_RESULTS="$STATE_DIR/gate-results.jsonl"
mkdir -p "$STATE_DIR"
: > "$GATE_RESULTS"

failed=0
blocked=0
first_failure=""

# The gates a gate consumes, space-separated. A gate not listed consumes nothing and
# always runs.
blocked_by() {
  case "$1" in
    artifact-identity) echo "build" ;;
    proof-matrix-stamp) echo "artifact-identity" ;;
    smoke-test) echo "build" ;;
    live-fire) echo "build" ;;
    release-state) echo "artifact-identity" ;;
    dependency-graph) echo "release-state" ;;
    change-invalidation) echo "release-state" ;;
    *) echo "" ;;
  esac
}

# Whether a gate's recorded outcome was a pass.
passed() {
  grep -q "^{\"gate\":\"$1\",\"exit\":0" "$GATE_RESULTS"
}

gate() {
  name=$1
  shift

  prerequisites=$(blocked_by "$name")
  for prerequisite in $prerequisites; do
    if ! passed "$prerequisite"; then
      printf '{"gate":"%s","exit":null,"status":"BLOCKED_PREREQUISITE","blocked_by":"%s"}\n' \
        "$name" "$prerequisite" >> "$GATE_RESULTS"
      echo "verify: gate '$name' blocked: '$prerequisite' did not pass"
      blocked=$((blocked + 1))
      return 0
    fi
  done

  if "$@"; then
    code=0
  else
    code=$?
  fi
  printf '{"gate":"%s","exit":%s}\n' "$name" "$code" >> "$GATE_RESULTS"
  if [ "$code" -ne 0 ]; then
    echo "verify: gate '$name' failed with exit $code" >&2
    failed=$((failed + 1))
    if [ -z "$first_failure" ]; then
      first_failure=$code
    fi
  fi
  return 0
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
# here because it inspects the machine's provider CLI capabilities, and its --live form
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
# the smoke and live-fire steps below run against the build that was hashed --
# which is what AGENTS.md §16 asks for.
gate artifact-identity sh scripts/artifact-identity.sh

# Stamp that same digest into the functional proof matrix.
#
# The matrix is a live claim -- "these outcomes are verified against this
# artifact" -- so its `artifact_digest` column has to name the artifact this
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
# These files are required evidence -- DOD-042 names RELEASE_GATE.json and
# DOD-029 names the run manifest -- and nothing generated them, so they were
# written by hand and drifted into contradicting each other. Generated here, at
# the end of a green sweep, they describe the run that just happened.
gate release-state python3 scripts/release-state.py

# Derive the dependency blocker graph from the accounting and this file, and refuse a
# blanket block: every blocked row has to name the prerequisite or capability that
# blocks it, and no independent row may be blocked by cascade.
gate dependency-graph python3 scripts/dependency-graph.py

# Derive what this candidate's changes invalidate, from the previous epoch's commit.
# DOD-040: old evidence does not prove changed bytes.
gate change-invalidation python3 scripts/change-invalidation.py

echo
echo "verify: 19+2 gate(s) recorded; failed=$failed blocked=$blocked"
if [ "$failed" -ne 0 ]; then
  echo "verify: $failed gate(s) failed (first exit $first_failure)" >&2
  exit "$first_failure"
fi
if [ "$blocked" -ne 0 ]; then
  echo "verify: $blocked gate(s) blocked by a failed prerequisite" >&2
  exit 1
fi
