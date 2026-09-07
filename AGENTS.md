# Project VECTOR Agent Control Plane

## 1. Mission
Build Project VECTOR as a production-grade, local-first ASVAB/AFQT preparation desktop application whose study content, scoring estimates, AI guidance, integrations, and release claims are independently verifiable. Optimize for learning efficacy, offline reliability, privacy, accessibility, lawful source use, and evidence-backed completion rather than demo appearance.

## 2. THE BOOT SEQUENCE
1. Read this file, PROJECT_BRIEF.md, ARCHITECTURE.md, SECURITY.md, TESTING.md, COMMANDS.md, `.agent/GRAPH.md`, `.agent/DONE_LAW.md`, and the active ExecPlan.
2. Run `sh scripts/preflight.sh`.
3. Run `sh scripts/graph-next.sh` and select only a READY node.
4. Read the node's specs, requirements, ADRs, and verification rows before writing code.
5. Define the acceptance oracle and failing test first for non-trivial behavior.
6. Implement the smallest architecture-conforming slice.
7. Run the exact commands from COMMANDS.md; do not invent command names or flags.
8. Run mutation/negative proof, architecture-drift checks, anti-gaming review, and DoD accounting.
9. Close a node only when its evidence proves the real production path. Otherwise remediate or record a truthful blocked state.
10. Continue to the next READY node without asking the user for routine next-step confirmation.

## 3. Source-of-truth hierarchy
Conflicts resolve in this order: current user-approved product goal and constraints; `.agent/DONE_LAW.md`; accepted ADRs; SPEC files; REQUIREMENTS.csv; ARCHITECTURE.md; active ExecPlan; implementation; prose summaries. Lower layers never silently override higher layers. Any material change requires an ADR plus traceability updates.

## 4. Graph protocol
Only READY nodes may be implemented. Dependencies are strict. A node has exactly one active writer for overlapping production paths. `NODE_DONE` means requirements mapped, real implementation exercised, required tests passed, evidence recorded, and anti-gaming review PASS. A failed node must be remediated within the bounded loop or closed with a machine-readable blocker record. Downstream nodes never bypass an open or failed dependency.

## 5. STOP conditions
Stop only when: (a) preflight fails before the run in a way that prevents any meaningful work; (b) an action would destroy user/production data or cause an irreversible external side effect not explicitly specified; (c) a legal, financial, or security judgment the specs do not answer; (d) NODE_BLOCKED after the full remediation ladder; or (e) production deployment is requested while AUTO_DEPLOY is false, in which case the ship gate completes and deployment is marked MANUAL. Everything else uses the smallest reversible option, records a decision, and continues. Do not ask the user for next steps, preferences, or confirmation. Proceed.

## 6. Anti-drift rules
Every code, dependency, route, schema, provider, worker, feature, and test must map to a requirement or ADR. No broad refactor or unrelated cleanup is allowed inside a node. Run expected-files and architecture-drift audits before closure.

## 7. Anti-hallucination rules
Never invent package APIs, commands, environment variables, tables, routes, configuration keys, model names, provider capabilities, or CLI flags. Confirm names from repository files or official provider documentation. Commands come only from COMMANDS.md. Unknowns go in ASSUMPTIONS.md with an explicit verification method.

## 8. Anti-fixation rules
After a failure: reproduce; classify; inspect direct evidence; try the smallest local repair; check the architectural assumption; then use an alternate implementation only if the same requirement still holds. Limit each candidate epoch to five remediation attempts unless the ExecPlan defines a smaller bound. Never weaken a test or gate to make a failure disappear.

## 9. Reality law
UI screenshots, successful compilation, mocked tests, self-written summaries, and health-check HTTP 200 responses are not feature proof. Software that appears to work is a failure state. Only software proven by live-fire counts. Final integration evidence must exercise real local dependencies or an explicitly authorized external sandbox and independently read back the effect.

## 10. Dependency rules
Check existing dependencies first. Prefer standard library or already-approved dependencies. Add only what is necessary, pin versions through committed lockfiles, document license and purpose, and update installation/build evidence. Commercial distribution requires an approved license record.

## 11. File-creation and commit rules
Write only paths leased to the active node. Keep generated evidence separate from production code. Commit coherent, reviewable units with requirement IDs. Never force-push, rewrite shared history, or merge a failing candidate. Isolate parallel agents in git worktrees and merge through a queue.

## 12. Testing rules
TESTING.md and `.agent/verification/MASTER_TEST_REGISTRY.csv` are binding. Every registered test ID must be accounted for. Tests must prove semantics, boundaries, persistence, recovery, authorization where applicable, and failure behavior. Gate weakening, assertion deletion, blanket skipping, and mock-only final paths are prohibited.

## 13. Documentation update rules
Product scope changes update requirements/specs/ADR first. Architecture changes update ARCHITECTURE.md and the relevant ADR before code. Operational changes update runbooks and COMMANDS.md. Evidence files record observed facts and must never be edited to claim an unobserved pass.

## 14. Security rules
SECURITY.md is binding. No secret scraping, OAuth token replay, leaked-question acquisition, credential logging, uncontrolled MCP execution, or unredacted crash uploads. Production data is never used in destructive tests.

## 15. Definition of done
`.agent/DONE_LAW.md` contains the complete Rule-Because-Evidence-Or-Else registry. At minimum every task/feature/node/release requires stable requirement identity, acceptance evidence, clean reproducible build where applicable, real artifact execution, negative behavior, persistence/readback, recovery, no skipped required tests, architecture traceability, evidence hashes, and independent verification. `sh scripts/dod-gate.sh` must pass before closure.

## 16. Built-in verification harness
Statuses are PENDING, PASS, FAIL, BLOCKED_PREREQUISITE, BLOCKED_CAPABILITY, SKIPPED_NOT_APPLICABLE, or ACCEPTED_EXTERNAL_GATE. Blockers do not cascade into unrelated tests. Candidate epochs are immutable evidence units. All 484 registered tests remain accounted for. Exact-artifact verification hashes the artifact once and uses that same digest through smoke, E2E, security, recovery, and release gates. Verification follows `.agent/verification/GRAPH.md` V-000 through V-021.

## 17. Final response requirements
Report implemented nodes; verified user outcomes; exact commands run and exit codes; test accounting; artifact digest; unresolved blockers/risks; manual external gates; architecture drift result; security result; rollback path; and final GO, CONDITIONAL_EXTERNAL_GATES, or NO_GO verdict. Never describe planned work as completed work.
