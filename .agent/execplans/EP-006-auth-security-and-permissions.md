# EP-006 - Security and permissions

**Requirements:** REQ-028,REQ-045,REQ-046,REQ-047,REQ-048,REQ-049

**Purpose:** MCP capability security, process sandboxing, crash redaction, consent, secret rules.

**Preconditions:** every dependency in `.agent/GRAPH.md` is DONE_VERIFIED; preflight has no blocker for this node.

**Owned paths:** paths introduced by this node plus its direct tests. Shared manifests require an explicit lease.

**Acceptance oracle:** behavior is observable through the real public boundary, produces the required state or side effect, survives restart when stateful, rejects invalid/unauthorized input, and fails when the production behavior is deliberately mutated.

**Test-first order:** map requirement -> define failing test -> run narrow red proof when feasible -> implement -> run green proof -> negative/property/mutation proof -> affected regression suites.

**Required proof:** threat-model tests, negative auth, path/process isolation, redaction mutation. Evidence is stored under `.agent/evidence/EP-006/` with hashes and command exit codes.

**Blocker handling:** execute the bounded ladder in `.agent/LOOPS.md`; never bypass a failed dependency or weaken the oracle.

**Exit:** requirement traceability complete; architecture drift clean; all applicable DoD rows accounted; anti-gaming review PASS; node ledger event appended with evidence hashes.
