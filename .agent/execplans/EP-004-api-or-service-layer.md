# EP-004 - Application services and integrations

**Requirements:** REQ-008,REQ-011,REQ-012,REQ-013,REQ-014,REQ-015,REQ-016,REQ-017,REQ-018,REQ-019,REQ-024,REQ-025,REQ-026,REQ-027,REQ-042,REQ-043

**Purpose:** study services, scoring/readiness, local/native LLM router, evidence RAG, MCP, notebook export.

**Preconditions:** every dependency in `.agent/GRAPH.md` is DONE_VERIFIED; preflight has no blocker for this node.

**Owned paths:** paths introduced by this node plus its direct tests. Shared manifests require an explicit lease.

**Acceptance oracle:** behavior is observable through the real public boundary, produces the required state or side effect, survives restart when stateful, rejects invalid/unauthorized input, and fails when the production behavior is deliberately mutated.

**Test-first order:** map requirement -> define failing test -> run narrow red proof when feasible -> implement -> run green proof -> negative/property/mutation proof -> affected regression suites.

**Required proof:** real local model/MCP proof; provider lanes only when authorized. Evidence is stored under `.agent/evidence/EP-004/` with hashes and command exit codes.

**Blocker handling:** execute the bounded ladder in `.agent/LOOPS.md`; never bypass a failed dependency or weaken the oracle.

**Exit:** requirement traceability complete; architecture drift clean; all applicable DoD rows accounted; anti-gaming review PASS; node ledger event appended with evidence hashes.
