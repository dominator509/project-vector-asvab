# DEFINITION OF DONE — GraphLock Execution Law (READ THIS AT BOOT)

You are bound by this document. AGENTS.md, DONE_LAW.md, and GRAPH.md stay authoritative;
this file exists to make the meaning and the REASONING unmistakable so you never overclaim,
never ship stubs, and never fake completion. When in doubt, this file wins over your
inclination to declare victory.

---

## 1. WHAT "DONE" MEANS — BY NODE CLASS

### Implementation node (adds real production code)
DONE means ALL of:
1. You implemented the node that `.agent/GRAPH.md` + `scripts/graph-next.sh` mark READY — and ONLY that node. Nothing past it, nothing parallel to it.
2. Every behavior you added maps to a requirement ID or accepted ADR (traceability row).
3. You wrote the failing acceptance oracle (test) BEFORE the implementation for every non-trivial behavior, and you can show it failed before and passes after.
4. The code has real semantics: real domain logic, real persistence/transport boundaries, real workflows — NOT thin scaffolds. A file that is 6–22 lines "implementing" a core domain concept is a stub, and stubs are NOT DONE.
5. You ran the exact commands from COMMANDS.md; recorded real exit codes; evidence files contain actual command output + SHA-256 hashes of artifacts.
6. dod-gate / preflight / reality-gate scripts pass with real output.
7. Anti-gaming review and architecture-drift accounting ran and PASSED.
8. Ledger row appended (hash-chained), evidence committed, milestone commits carry requirement IDs.

### Infrastructure / scaffolding node
DONE means the artifact is real and exercised: real config that boots, real dependency wiring
with committed lockfiles, a real smoke/health proof — not config that "looks right".

### Audit / verification node
DONE means a verdict table where every category is PASS / FAIL / NOT_RUNNABLE_ENV(reason) /
N/A with the exact commands + outputs that justify it. "Looks fine" is not a verdict.

---

## 2. WHAT "DONE" IS NOT — the anti-patterns that void your work

Your claims are VOID — and a prior session's claims will be explicitly voided — when ANY of
these appear. If you find yourself doing these, STOP and do the real thing instead:

- **Whole-program claims.** "EP-000 to EP-010 complete" in one session is never true. One node.
  One node at a time. The graph exists because dependencies are strict. A single session cannot
  legitimately close ten nodes of a real system.
- **Evidence stickers.** A file named `EP-00X/STATUS.md` or `verify.txt` that asserts completion
  is not evidence. Evidence = command output with exit codes, logs, hashes, and artifacts that
  independently prove the behavior. Assertion files with no executed proof are stickers.
- **Stub code wearing a completion costume.** 10 files × 15 lines of "use-case" classes that call
  nothing, persist nothing, and prove nothing = scaffolding, not software. If the code does not
  exercise real semantics against real boundaries, the node is NOT done.
- **Thin tests.** A 20-line test file with one happy path does not prove semantics, boundaries,
  persistence, recovery, authorization, or failure behavior. Tests must fail when the behavior
  is actually broken (mutation/negative proof).
- **Junk commits.** node_modules, generated blobs, lockfile-only noise, unrelated config.
- **Unverified claims.** Anything you did not run, did not observe, did not record with exit
  code and hash — you did not do. Say so instead.
- **Scope drift.** Files outside your node's lease, refactors, "cleanups", extra dependencies.

---

## 3. THE WHY — understand the reasoning, not just the rules

- **Why one node at a time?** Because each node's proof is the foundation for the next. If you
  "complete" ten nodes on stubs, every downstream node builds on sand. One honest node beats ten
  claimed ones. The graph's DONE law exists to make sure the building is load-bearing.
- **Why acceptance-oracle-first?** A test written after the code can be written to pass. A test
  written first defines the contract, and watching it fail first is the only proof it tests
  anything real.
- **Why evidence with exit codes and hashes?** Because prose summaries can lie — yours and ours.
  A recorded `exit 0` from the exact command plus a SHA-256 of the artifact is what an auditor
  (human or automated) can verify independently. We WILL audit your PR. We WILL check.
- **Why no evidence stickers?** Because the audit treats them as fraud, not as progress. They
  waste everyone's time and they will be called out by name in the critique sent back to the
  session. It is better to deliver one FAIL with reproduction than ten stickers.
- **Why honesty over completion?** The system is built to absorb truth: a node closed
  CLOSED_BLOCKED with a reproduction and an exact unblock condition is REAL progress and is
  respected. A fake NODE_DONE is debt that poisons every downstream node and gets voided —
  costing more than the truth ever would.
- **Why no stubs in production paths?** Placeholder code in a production path is a lie that
  ships. If the real implementation cannot be completed in this session, close the node blocked
  with the exact gap. Never leave a stub that pretends to be the feature.

---

## 4. COMPLETION CHECKLIST — run this before you ever write NODE_DONE

- [ ] I implemented ONLY the READY node (graph-next output), nothing past it.
- [ ] Every change maps to a requirement ID or ADR.
- [ ] Acceptance oracles existed BEFORE implementation and failed first (or are attached).
- [ ] Code is real: semantics, boundaries, and workflows — no stubs, no placeholders.
- [ ] I ran the exact COMMANDS.md commands; evidence has exit codes and hashes.
- [ ] dod-gate / reality checks ran with real output, anti-gaming PASSED.
- [ ] Ledger appended + committed with hashes; no junk files.
- [ ] If anything above is impossible: I close CLOSED_BLOCKED with reproduction +
      exact unblock condition — I do NOT claim done.

---

## 5. CONSEQUENCES (so you understand the stakes)

Every PR you submit is audited against this document BEFORE it is accepted. A PR that fails
the audit is NOT merged as proof — it is critiqued line-by-line back into the same session,
its claims are VOIDED, and you must re-execute the real work. An audit critique is not a
nudge; it is the list of exactly what was wrong and why. Address every point in it. The
fastest path through this system is the honest one: small, real, evidenced, one node at a time.
