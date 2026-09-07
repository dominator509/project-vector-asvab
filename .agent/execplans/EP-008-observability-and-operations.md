# EP-008 - Observability and repair operations

**Requirements:** REQ-028,REQ-057,REQ-059

**Purpose:** local telemetry, crash signature, repair broker, worktree/PR workflow, runbooks.

**Preconditions:** every dependency in `.agent/GRAPH.md` is DONE_VERIFIED; preflight has no blocker for this node.

**Owned paths:** paths introduced by this node plus its direct tests. Shared manifests require an explicit lease.

**Acceptance oracle:** behavior is observable through the real public boundary, produces the required state or side effect, survives restart when stateful, rejects invalid/unauthorized input, and fails when the production behavior is deliberately mutated.

**Test-first order:** map requirement -> define failing test -> run narrow red proof when feasible -> implement -> run green proof -> negative/property/mutation proof -> affected regression suites.

**Required proof:** crash injection to redacted bundle and gated PR dry-run with no auto-merge. Evidence is stored under `.agent/evidence/EP-008/` with hashes and command exit codes.

**Blocker handling:** execute the bounded ladder in `.agent/LOOPS.md`; never bypass a failed dependency or weaken the oracle.

**Exit:** requirement traceability complete; architecture drift clean; all applicable DoD rows accounted; anti-gaming review PASS; node ledger event appended with evidence hashes.
