# Project VECTOR Definition of Done

Every row below is binding. A rule cannot be waived by narrative. Evidence is observed output, not an assertion.

## DOD-001 - task,feature,milestone,node,release
**Rule:** Every promised behavior has a stable requirement ID and at least one acceptance test before implementation is declared complete.
**Because:** Without stable identity, scope drifts and tests cannot prove the promised behavior.
**Required evidence:** Requirement-to-test traceability row plus executed acceptance evidence.
**Or else:** The item is INCOMPLETE and NODE_DONE/GO is prohibited until the mapping exists and passes.
**Must account:** true

## DOD-002 - node,release
**Rule:** The repository builds from a clean checkout using the committed frozen/locked dependency files and declared toolchain.
**Because:** Developer caches and floating dependencies can hide undeclared state and non-reproducible builds.
**Required evidence:** Clean-environment install/build logs, lockfile digest, tool versions, exit codes, and build sentinel.
**Or else:** Record FAIL for the clean-build gate; dependent runtime tests may be BLOCKED_PREREQUISITE, but independent tests continue.
**Must account:** true

## DOD-003 - feature,node,release
**Rule:** The production distribution artifact is created successfully in every supported format.
**Because:** Source code is not what users install or operators deploy.
**Required evidence:** Artifact paths, formats, sizes, metadata, checksums, signatures/attestations when applicable, and build logs.
**Or else:** The release is NO_GO and artifact-dependent tests remain BLOCKED_PREREQUISITE.
**Must account:** true

## DOD-004 - feature,node,release
**Rule:** Final smoke and E2E tests run against the exact production artifact digest, not merely source or a development server.
**Because:** Packaging can omit files, alter configuration, or introduce behavior not visible in source-level tests.
**Required evidence:** Artifact digest bound to test environment, commands, logs, and observed outcomes.
**Or else:** Artifact-level behavior is UNVERIFIED and completion is prohibited.
**Must account:** true

## DOD-005 - milestone,node,release
**Rule:** Required tests execute in an ephemeral clean environment or a documented persistent environment created from a known baseline.
**Because:** Hidden developer state, caches, databases, and environment variables can create false greens.
**Required evidence:** Environment manifest, image/VM digest, cache policy, provisioning logs, and teardown proof.
**Or else:** Results are ERROR or INCONCLUSIVE and cannot satisfy completion.
**Must account:** true

## DOD-006 - task,feature,milestone,node,release
**Rule:** No required test is skipped, disabled, ignored, pending, quarantined, or xfailed without an approved requirement-scoped waiver.
**Because:** Skipped work creates silent blind spots while allowing green CI.
**Required evidence:** Test-runner collection/skip report and waiver IDs with owner, expiry, rationale, and compensating evidence.
**Or else:** The affected requirement is INCOMPLETE and the release is NO_GO unless policy explicitly permits the time-bounded waiver.
**Must account:** true

## DOD-007 - milestone,node,release
**Rule:** The harness fails when zero tests or fewer than the expected manifest are collected.
**Because:** Many runners exit zero for empty, misconfigured, or partially discovered suites.
**Required evidence:** Expected-test manifest/count, collected IDs/count, and test-collection-guard output.
**Or else:** The run is ERROR; no pass result from that suite is valid.
**Must account:** true

## DOD-008 - feature,milestone,node,release
**Rule:** Unit tests verify semantic results, boundaries, invalid inputs, error behavior, state transitions, and invariants for critical logic.
**Because:** Line execution alone does not prove correctness of meaning or failure behavior.
**Required evidence:** Mapped unit tests, assertions, boundary partitions, invariant list, coverage, and mutation sensitivity.
**Or else:** The feature remains PARTIAL/UNVERIFIED even if compilation and happy-path tests pass.
**Must account:** true

## DOD-009 - feature,node,release
**Rule:** Integration tests use production-type databases, queues, caches, storage, brokers, and other required dependency classes.
**Because:** In-memory substitutes frequently differ in transactions, locking, serialization, consistency, and failure modes.
**Required evidence:** Ephemeral production-type service versions, configuration, migrations, test commands, and independently observed state.
**Or else:** Integration claims are PARTIAL or SIMULATED and cannot support production readiness.
**Must account:** true

## DOD-010 - feature,node,release
**Rule:** Mocks may support isolation tests but may not be the sole proof of a claimed integration or production feature.
**Because:** A mock proves the test author's expectation, not the real dependency or wiring.
**Required evidence:** At least one real/sandbox dependency execution at the final acceptance boundary.
**Or else:** The feature is SIMULATED/UNVERIFIED and the claim-to-release traceability gate fails.
**Must account:** true

## DOD-011 - feature,node,release
**Rule:** Black-box acceptance tests exercise only public user-facing interfaces and do not call private internals to force success.
**Because:** Users and external systems cannot rely on internal shortcuts.
**Required evidence:** HTTP/UI/CLI/SDK/event entry-point evidence plus public outputs and side effects.
**Or else:** The test is implementation-coupled and cannot satisfy end-to-end acceptance.
**Must account:** true

## DOD-012 - feature,node,release
**Rule:** Important side effects are independently verified through a second connection, client, provider API, database query, event consumer, or durable artifact.
**Because:** The component under test can falsely report success without performing the effect.
**Required evidence:** Primary response plus independent observation with correlation/canary ID.
**Or else:** The side-effect claim is UNVERIFIED and any success response is insufficient.
**Must account:** true

## DOD-013 - feature,node,release
**Rule:** Runtime-generated unpredictable canary data is used for critical black-box proofs.
**Because:** Static examples can be hard-coded, cached, or accidentally satisfied by canned responses.
**Required evidence:** Seed/source, generated canary, propagation trace, and final independent observation.
**Or else:** The proof is susceptible to fabrication and does not satisfy Functional Reality.
**Must account:** true

## DOD-014 - feature,node,release
**Rule:** Wrong credentials, revoked permissions, expired tokens, and unavailable dependencies cause accurate fail-closed behavior rather than simulated success.
**Because:** Production systems must communicate real dependency/auth failures and preserve integrity.
**Required evidence:** Negative live-fire logs, status/error contracts, no-side-effect proof, and recovery behavior.
**Or else:** The feature FAILS resilience/security acceptance and release is blocked when critical.
**Must account:** true

## DOD-015 - feature,node,release
**Rule:** Persistent state survives full process and container restart and is readable by the supported runtime.
**Because:** Memory-only or process-local success can masquerade as durable implementation.
**Required evidence:** Pre-restart state hash, hard restart evidence, post-restart independent read, and reconciliation.
**Or else:** The persistence claim is FALSE/SIMULATED and completion is prohibited.
**Must account:** true

## DOD-016 - feature,node,release
**Rule:** Required migrations work from an empty database and every supported prior released schema with logical data preservation.
**Because:** Fresh installs and upgrades exercise different paths and failures can strand customers.
**Required evidence:** Baseline schema/data hashes, migration logs, post-migration invariants, retry/rollback evidence, and supported-version matrix.
**Or else:** The release is NO_GO for affected install/upgrade paths.
**Must account:** true

## DOD-017 - feature,node,release
**Rule:** Duplicate, retried, reordered, delayed, and concurrent operations preserve idempotency, integrity, and the documented delivery semantics.
**Because:** Distributed systems naturally redeliver and race; naive handling duplicates or loses side effects.
**Required evidence:** Idempotency keys, concurrent traces, commit/ack fault cases, final reconciliation, and invariant results.
**Or else:** The feature FAILS data-integrity acceptance and cannot ship when state-changing.
**Must account:** true

## DOD-018 - feature,node,release
**Rule:** At least one controlled defect or mutation is introduced for each critical feature and the relevant test must fail.
**Because:** A permanently green test may not observe the behavior it claims to protect.
**Required evidence:** Mutation/defect ID, changed behavior, failing test evidence, restoration, and green rerun.
**Or else:** The test is non-discriminating; its pass cannot prove the feature.
**Must account:** true

## DOD-019 - feature,milestone,node,release
**Rule:** Placeholder, stub, fake, demo, simulation, no-op, dead-route, hard-coded-success, and unfinished-code scans run against all production paths.
**Because:** AI-generated and scaffolded repositories often appear complete while returning fabricated behavior.
**Required evidence:** Lexical scan, structural trace, reachable-path analysis, allowlist decisions, and findings.
**Or else:** Any unexplained hit is a release blocker or the affected claim is explicitly marked incomplete.
**Must account:** true

## DOD-020 - feature,node,release
**Rule:** Production mode never selects mock, fake, demo, sample, or in-memory adapters for features represented as production-ready unless that adapter is the documented production architecture.
**Because:** Environment switches can silently route real users into simulated behavior.
**Required evidence:** Production configuration resolution, dependency injection graph, runtime adapter identity, and live-fire evidence.
**Or else:** The feature is SIMULATED and the release is NO_GO.
**Must account:** true

## DOD-021 - milestone,node,release
**Rule:** Applicable formatting, linting, static analysis, type checking, secret scanning, dependency/license/SBOM scanning, IaC/container checks, and security tests pass under enforced thresholds.
**Because:** These gates catch classes of defects before runtime and protect the supply chain.
**Required evidence:** Commands, versions, reports, exit codes, thresholds, and approved time-bounded waivers.
**Or else:** The gate records FAIL or unresolved risk; silent continuation is prohibited.
**Must account:** true

## DOD-022 - feature,node,release
**Rule:** Performance, resource, cost, and SLO requirements are encoded as automated pass/fail thresholds against a defined workload and environment.
**Because:** Unmeasured claims such as fast, scalable, or cheap cannot be verified or regression-gated.
**Required evidence:** Workload model, environment, samples, percentiles, resource metrics, thresholds, and verdict.
**Or else:** The nonfunctional claim is UNVERIFIED and a mandatory SLO failure is NO_GO.
**Must account:** true

## DOD-023 - node,release
**Rule:** README commands, examples, quickstarts, install, upgrade, deployment, rollback, and operator instructions are executed exactly as published in clean environments.
**Because:** Documentation drift creates hidden operator knowledge and failed customer installs.
**Required evidence:** Extracted command manifest, clean execution logs, expected/actual outputs, and corrected documentation.
**Or else:** Documentation is defective and the affected support/install claim cannot pass.
**Must account:** true

## DOD-024 - task,feature,milestone,node,release
**Rule:** No test or scanner failure is hidden by continue-on-error, ignored exit codes, unconditional success, swallowed exceptions, all-retry policies, filtered output, or baseline auto-acceptance.
**Because:** Failure masking converts real defects into fraudulent green status.
**Required evidence:** CI/script review, raw exit codes, first-failure logs, status-transition audit, and no-mask checks.
**Or else:** The run is INVALID/ERROR and all dependent pass claims are revoked.
**Must account:** true

## DOD-025 - milestone,node,release
**Rule:** Raw commands, tool versions, exit codes, logs, reports, traces, seeds, environment fingerprints, candidate SHA, artifact hashes, and evidence digests are preserved.
**Because:** A claim that cannot be reproduced or tied to an exact identity cannot be trusted.
**Required evidence:** Evidence index entries and content hashes linked to each result.
**Or else:** The result is INCONCLUSIVE and cannot satisfy a release gate.
**Must account:** true

## DOD-026 - task,feature,milestone,node,release
**Rule:** Any unmet condition is reported as incomplete, partial, simulated, blocked, experimental, external-required, deferred, failed, errored, or unverified using the exact taxonomy.
**Because:** Honest status prevents uncertainty from being laundered into completion.
**Required evidence:** Status record, rationale, evidence, dependency edge, and next action.
**Or else:** DONE/GO is prohibited and any contrary narrative is a fabrication defect.
**Must account:** true

## DOD-027 - task,feature,milestone,node,release
**Rule:** An incomplete implementation may not be replaced with a fabricated success response, and acceptance criteria may not be weakened merely to obtain a pass.
**Because:** Changing the oracle or faking output hides the defect instead of solving it.
**Required evidence:** Diff review, requirement history, gate hash, mutation/live-fire proof, and decision log.
**Or else:** Revert the manipulation, record a critical process finding, and keep the item incomplete.
**Must account:** true

## DOD-028 - task,feature,milestone,node,release
**Rule:** The final completion report distinguishes verified behavior, partially verified behavior, unverified assumptions, blocked work, external gates, accepted risks, and every remaining limitation.
**Because:** Readers need to know exactly what evidence supports each claim.
**Required evidence:** Structured final report and complete accounting linked to requirements/tests/evidence.
**Or else:** The report is invalid and completion cannot be declared.
**Must account:** true

## DOD-029 - node,release
**Rule:** The exact candidate commit, base revision, test-overlay revision, build inputs, and release artifact digest are pinned and recorded.
**Because:** Evidence can otherwise drift across unidentifiable code and artifacts.
**Required evidence:** Run manifest and artifact-identity output.
**Or else:** All identity-dependent evidence is INCONCLUSIVE and release is prohibited.
**Must account:** true

## DOD-030 - node,release
**Rule:** All 484 canonical test IDs receive an individual applicability decision and final status; zero IDs are missing or duplicated.
**Because:** Exhaustive testing requires exhaustive accounting, not silent omission.
**Required evidence:** Registry snapshot, applicability matrix, test ledger, validator report, and final totals invariant.
**Or else:** Harness validation fails and the project cannot be declared production-ready.
**Must account:** true

## DOD-031 - node,release
**Rule:** A failed prerequisite blocks only tests with explicit dependency edges; independent tests continue to completion.
**Because:** Blanket blocking hides defects and confuses product failures with environment limitations.
**Required evidence:** Dependency blocker graph, per-test blocker fields, continuation logs, and blanket-block validator.
**Or else:** The accounting is invalid, the campaign remains IN_PROGRESS/ERROR, and final verdict cannot be trusted.
**Must account:** true

## DOD-032 - node,release
**Rule:** Every FAIL, ERROR, BLOCKED_PREREQUISITE, BLOCKED_ENVIRONMENT, BLOCKED_CAPABILITY, BLOCKED_CREDENTIALS, BLOCKED_SAFETY, EXTERNAL_REQUIRED, and DEFERRED_LONG_RUNNING result uses the precise definition and required fields.
**Because:** Different causes require different remediation and release decisions.
**Required evidence:** Status-transition audit and schema validation.
**Or else:** Misclassified rows are rejected and must be corrected before final accounting.
**Must account:** true

## DOD-033 - node,release
**Rule:** The harness non-invasively discovers canonical bootstrap and provisions declared test infrastructure when the adapter can do so.
**Because:** Missing PostgreSQL, queues, browsers, or build ordering is often a harness setup problem rather than a product capability gap.
**Required evidence:** Repository evidence for commands/versions plus provisioning, health, migration, and teardown logs.
**Or else:** The attempt is ERROR, not BLOCKED_CAPABILITY or candidate FAIL, until the harness setup is corrected.
**Must account:** true

## DOD-034 - release
**Rule:** A virgin clean room installs and boots the exact final artifact using only public documentation and declared prerequisites, then completes the golden path.
**Because:** Developer-machine bias and hidden dependencies make otherwise green software undistributable.
**Required evidence:** Zero-state proof, documented prerequisite log, artifact transfer/digest, install/boot/golden-path evidence.
**Or else:** The distribution is NO_GO and documentation/install findings remain open.
**Must account:** true

## DOD-035 - release
**Rule:** Every claimed upgrade, downgrade, rollback, version-skew, mixed-fleet, and compatibility path is executed with realistic persistent state.
**Because:** Releases fail in transitions even when each version works alone.
**Required evidence:** Compatibility matrix, old/new artifacts, state hashes, queue/event evidence, and rollback outcome.
**Or else:** Unsupported or failed claimed paths block release or must be explicitly removed from support claims.
**Must account:** true

## DOD-036 - release
**Rule:** Backup, restore, disaster recovery, hard-failure recovery, RPO, RTO, and MTTR claims are executed against reconciled state where applicable.
**Because:** Restarting a process is not recovery if transactions are lost, duplicated, or corrupted.
**Required evidence:** Pre-disaster hash, fault injection, restore/recovery logs, post-state reconciliation, and measured objectives.
**Or else:** Recovery readiness FAILS and stateful production release is NO_GO when mandatory.
**Must account:** true

## DOD-037 - feature,node,release
**Rule:** Health, readiness, logs, metrics, traces, alerts, dashboards, and correlation IDs truthfully describe critical workflows and induced failures without leaking secrets.
**Because:** Operators cannot run or recover a system whose telemetry lies or lacks causality.
**Required evidence:** Known-failure injection mapped to signals, alert lifecycle, trace/log correlation, and redaction evidence.
**Or else:** Observability is FAIL/INCOMPLETE and mandatory production operations gate fails.
**Must account:** true

## DOD-038 - release
**Rule:** Required soak, endurance, fuzz, performance, stress, and recovery durations/workloads are completed at their specified scale; abbreviated trials are labeled separately.
**Because:** Short samples cannot prove long-duration stability or representative capacity.
**Required evidence:** Start/end timestamps, continuous heartbeats, telemetry, corpus/workload, interruptions, and full-duration report.
**Or else:** Status is DEFERRED_LONG_RUNNING, EXTERNAL_REQUIRED, PARTIAL, or FAIL -- never PASS for the full requirement.
**Must account:** true

## DOD-039 - release
**Rule:** Human UAT, manual assistive-technology validation, legal/compliance review, physical hardware/HSM work, and accredited assessment are signed only by the required real participants.
**Because:** AI or automated tooling cannot impersonate business acceptance, lived accessibility use, hardware evidence, or professional certification.
**Required evidence:** Named authorized sign-off, scope, date, scenarios/evidence, and unresolved findings.
**Or else:** Status remains EXTERNAL_REQUIRED and GO is prohibited when the gate is mandatory.
**Must account:** true

## DOD-040 - milestone,node,release
**Rule:** Any code, dependency, schema, configuration, build, test-oracle, or artifact change invalidates and reruns every affected downstream result.
**Because:** Old evidence does not prove a changed candidate.
**Required evidence:** Change invalidation graph, prior/new epoch IDs, rerun list, and current evidence hashes.
**Or else:** Affected PASS statuses are revoked until rerun.
**Must account:** true

## DOD-041 - feature,node,release
**Rule:** Conditional domain packs such as HIPAA, blockchain, AI/agentic, multi-tenant, mobile, cloud, and hardware are activated or skipped from repository evidence, never assumption.
**Because:** Running irrelevant tests wastes effort while skipping relevant domain risk creates dangerous blind spots.
**Required evidence:** Architecture/data/interface/support evidence attached to every applicability decision.
**Or else:** The applicability matrix is invalid and final accounting fails.
**Must account:** true

## DOD-042 - release
**Rule:** The final release verdict is produced only by the machine-validated ship gate and is one of GO, NO_GO, CONDITIONAL_EXTERNAL_GATES, or INCONCLUSIVE.
**Because:** Free-form completion language can obscure release blockers and external dependencies.
**Required evidence:** RELEASE_GATE.json, validator outputs, DOD status, 484-test accounting, and exact artifact identity.
**Or else:** No production-ready tag or deployment is allowed; any contradictory claim is invalid.
**Must account:** true
