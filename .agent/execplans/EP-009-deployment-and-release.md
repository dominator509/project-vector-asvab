# EP-009 - Packaging and release

**Requirements:** REQ-036,REQ-038,REQ-053,REQ-060

**Purpose:** Windows package, updater metadata, SBOM, provenance, content release tooling.

**Preconditions:** every dependency in `.agent/GRAPH.md` is DONE_VERIFIED; preflight has no blocker for this node.

**Owned paths:** paths introduced by this node plus its direct tests. Shared manifests require an explicit lease.

**Acceptance oracle:** behavior is observable through the real public boundary, produces the required state or side effect, survives restart when stateful, rejects invalid/unauthorized input, and fails when the production behavior is deliberately mutated.

**Test-first order:** map requirement -> define failing test -> run narrow red proof when feasible -> implement -> run green proof -> negative/property/mutation proof -> affected regression suites.

**Required proof:** exact-artifact digest, install/upgrade/rollback, offline launch, license bundle. Evidence is stored under `.agent/evidence/EP-009/` with hashes and command exit codes.

**Blocker handling:** execute the bounded ladder in `.agent/LOOPS.md`; never bypass a failed dependency or weaken the oracle.

**Exit:** requirement traceability complete; architecture drift clean; all applicable DoD rows accounted; anti-gaming review PASS; node ledger event appended with evidence hashes.
