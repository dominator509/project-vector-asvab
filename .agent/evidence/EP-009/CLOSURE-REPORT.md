# Project VECTOR: closure report

Written at the end of the corpus and content-domain programme, after the thirty-sixth round of
work in this session. Everything below is read from the repository's own generated state and
evidence; nothing here is a summary of intent.

## What this project is

A local-first ASVAB/AFQT preparation desktop application: Tauri 2 with a React/TypeScript interface
over a Rust workspace of thirteen crates, a local SQLite database, an original-item factory with
executable answer proofs, a corpus built from public-domain sources, signed content packs, and a
verification control plane (GraphLock) that accounts for 60 requirements, 484 registered tests and
42 Definition-of-Done clauses.

## The programme this session ran

The goal was an extensive, sourced study corpus and the content domain that serves it, with a hard
exclusion on leaked, controlled, recalled or copyrighted third-party prep content. All six
components are delivered and evidenced:

| Component | State |
|---|---|
| (1) Item store schema and persistence | Seven migrations, `content_items` with per-item objective, proof, reviewer and hashes; lifecycle states with a review trail (28,508 review rows) |
| (2) Tauri serving API replacing `sample.ts` | `content_next`, `content_stats`, `content_manager`, quarantine and reinstate; the practice view loads from the corpus, with `sample.ts` left only for the review-queue fixture |
| (3) The REQ-022 original-item factory | Deterministic templates with misconception-derived distractors; every generated item carries an executable proof the verifier recomputes, and `check-corpus.py` re-derives all 1,334 of them independently |
| (4) Ingestion of permitted sources | NEETS for Electronics Information, Project Gutenberg for Paragraph Comprehension, Webster's 1913 and Moby for Word Knowledge, federal science and technical works for General Science, Shop and Auto Information; 70 vault records, 8,910 citations |
| (5) Content manager with pack install and quarantine | Signed packs with a curriculum graph and calibration; install, rollback and activate are all exercised against the installation's own store |
| (6) Per-item provenance | Every active item cites a vault record, names an objective, carries a proof, a named reviewer and two distinct hashes; every provenance invariant in the corpus readback is zero |

**Corpus at closure: 6,961 active items** — Word Knowledge 1,949, Paragraph Comprehension 1,676,
Electronics Information 1,565, Arithmetic Reasoning 466, Mechanical Comprehension 447, Mathematics
Knowledge 421, General Science 301, Auto Information 94, Shop Information 42. Pack `core-asvab v6`,
signed, 56 sources, 28 objectives over nine subtests.

The exclusion held: every source is public domain, a US government work, or the project's own
original text. Four source families were *refused* with measurements recorded rather than used
quietly (seven NAVEDTRA microfiche manuals, ten Army ordnance technical manuals, Machinery's
Reference, and Modern Machine-Shop Practice at measured precision), and the pack licence policy
refuses any licence outside its permitted list.

## The verification state at closure

| | |
|---|---|
| Sweep | `sh scripts/verify.sh` — **21 gates, all exit 0** |
| Gates that continue past a failure | A gate whose declared prerequisite did not pass is recorded `BLOCKED_PREREQUISITE` with the gate that blocked it, so one failure cannot hide the rest |
| Requirements | **57 of 60 done**; three open, all external |
| Test registry | 484 IDs accounted: 106 PASS, 352 SKIPPED_NOT_APPLICABLE, 26 BLOCKED_* classified per ID by `dependency-graph.py` |
| Definition of Done | **34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED** |
| Mutations | **46 caught** across cargo, vitest and Playwright; each verified by the runner that can actually execute it |
| Artifact | digest `b33833c5…`, candidate epoch 27, `RUN_MANIFEST.json` status VERIFIED |
| Verdict | **CONDITIONAL_EXTERNAL_GATES** — the build and its gates pass; three requirements cannot be satisfied inside this repository |

Live-fire evidence behind those numbers, not just unit tests: the packaged executable boots and
writes through the real IPC layer; a zero-state directory takes the artifact, the signed pack and a
full study journey; a canary drawn from the OS CSPRNG at run time propagates through the study path
and is read back by a separate process; three hard database failures (truncated, corrupted, deleted)
are each restored and reconciled byte for byte; a 12-minute soak ran 112 iterations with 0 failures;
the content pack moved v5 → v6 → v5 → v6 with the state hash recorded at every step; a signed update
manifest was signed, verified, tampered with and refused; and the pull-request lane opened, verified
and closed a real pull request on the public repository without any merge capability existing in the
code.

## What is open, and whose move it is

Seven clauses and three requirements. **None of them is unfinished code**: each is a person this
repository cannot employ, an environment it cannot provision, a release that has not happened, or a
claim it deliberately does not make. `RESIDUAL_RISK_AND_EXTERNAL_GATES.md` carries the same table
with the same owners, generated on every sweep.

| Open item | Whose move | What they have to do |
|---|---|---|
| REQ-036 (a code-signing certificate) | the publisher | obtain the certificate, sign the MSI/NSIS artifacts, re-run the sweep |
| REQ-038 (a person using a screen reader) | a human tester | run the packaged app with Narrator or NVDA and record findings |
| REQ-060 (trademark clearance) | counsel | clear the working title or choose another |
| DOD-039 (manual validation, signing, legal) | the same three people | as above |
| DOD-005 (clean-VM lane) and DOD-034 (foreign machine) | whoever has a VM or a second machine | run the published commands on a clean image; the clean-room lane is the recipe |
| DOD-022 (hardware lanes) | whoever has the declared hardware | run the SLO suite on the low, mid and high lanes |
| DOD-016 (migration from a prior release) | the second release | cut a release, then migrate a copy of its database forward |
| DOD-038 (full-duration soak) | whoever can give the machine the time | declare the scale, then run `soak.py --minutes <that scale>` |
| DOD-004 (E2E inside the packaged process) | whoever has a driver that exposes the WebView2 document | start `tauri-driver` with a matching `msedgedriver` there and run `packaged-journey.py`; here the window is reachable and the document is empty, measured over 30 seconds |
| DOD-033 (infrastructure provisioning adapter) | nobody, unless infrastructure is declared | the declared toolchain is provisioned and discovered; a desktop binary needs no fleet, so there is nothing to provision today |

## What this repository deliberately does not claim

- **A score prediction.** Readiness is a band with uncertainty; there is no official-score claim
  anywhere in the product or the evidence (ADR-010).
- **A reproducible build.** Two clean environments build the same source to different bytes on
  Windows; the measurement is recorded and no gate asserts digest equality across builds.
- **A complete corpus.** Shop Information at 42 items is thin, Auto Information carries a known
  noise floor, General Science rests on one work, and Word Knowledge's rare distractors are
  sometimes archaic. Each is recorded as a measured limitation rather than smoothed over.
- **An in-app model picker or tutor surface.** The local GGUF lane answers and the product's probe
  reports it healthy; choosing the model file is still the server's argument, and no lesson question
  is routed through a tutor screen yet.
- **Deployment automation.** `AUTO_DEPLOY` is false and no deployment target is declared.

## How to read the state rather than this document

| Question | File |
|---|---|
| Release verdict, and why | `.agent/verification/reports/RELEASE_GATE.json` |
| Which artifact was verified | `.agent/verification/state/RUN_MANIFEST.json` |
| Which requirement is not done, and whose move it is | `.agent/verification/reports/RESIDUAL_RISK_AND_EXTERNAL_GATES.md` |
| Which clause is not PASS, with the check that decided it | `.agent/verification/state/DOD_STATUS.jsonl` |
| What evidence exists, with hashes | `.agent/verification/state/EVIDENCE_INDEX.json` |
| What each gate recorded | `.agent/verification/state/gate-results.jsonl` |
| What this candidate's changes invalidate | `.agent/verification/state/CHANGE_INVALIDATION_GRAPH.md` |
| The round-by-round record, including the defects found | `.agent/state/LEDGER.md` and `.agent/evidence/**/ROUND-*-REPORT.md` |
