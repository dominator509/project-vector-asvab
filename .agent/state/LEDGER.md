# Project VECTOR Ledger

| Node | Status | Timestamp | Evidence Directory |
|---|---|---|---|
| EP-000 | NODE_DONE | 2026-09-11T19:00:00Z | .agent/evidence/EP-000 |
| EP-001 | NODE_DONE | 2026-09-09T01:30:00Z | .agent/evidence/EP-001 |
| EP-002 | NODE_DONE | 2026-09-11T19:00:00Z | .agent/evidence/EP-002 |
| EP-003 | NODE_DONE | 2026-09-10T00:00:00Z | .agent/evidence/EP-003 |
| EP-004 | NODE_DONE | 2026-09-10T06:00:00Z | .agent/evidence/EP-004 |
| EP-005 | NODE_DONE | 2026-09-10T12:00:00Z | .agent/evidence/EP-005 |
| EP-006 | NODE_DONE | 2026-09-10T18:00:00Z | .agent/evidence/EP-006 |
| EP-007 | NODE_DONE | 2026-09-10T22:00:00Z | .agent/evidence/EP-007 |
| EP-008 | NODE_DONE | 2026-09-11T04:00:00Z | .agent/evidence/EP-008 |
| EP-009 | NODE_DONE | 2026-09-11T10:00:00Z | .agent/evidence/EP-009 |
| EP-010 | NODE_DONE | 2026-09-11T16:00:00Z | .agent/evidence/EP-010 |

## Reopened nodes

EP-000 and EP-002 were reopened on 2026-09-11 after a closure audit found their
recorded evidence could not support a `NODE_DONE` claim. Their artifacts are
largely real; their evidence was not.

**EP-000.** Closed `NODE_DONE` by `root@vmi3357656.contaboserver.net` on a remote
VPS, two seconds after producing an accounting that reports `total 484,
{'PENDING': 484}` — nothing verified. A later session correctly re-closed it
`CLOSED_BLOCKED` with `BLOCKED_INSUFFICIENT_EVIDENCE`; that record was deleted as
"spurious" in `c8e3e4f` while one of its three stated unblock conditions — an
independent frontier audit — was never met. No such audit exists in the
repository.

**EP-002.** Closed `NODE_DONE` while its own evidence directory recorded a
`unit-tests` exit code of `1`. All three gate logs it hashed were never
committed. Its anti-gaming review was a three-line `{"verdict": "PASS"}` sticker,
deleted in `65ca5f4`. Two stubs remained in the crate the node owns, and one of
its tests was proven vacuous. Two commits titled "EP-003" and authored by
`google-labs-jules[bot]` flipped that exit code from `1` to `0` and rewrote the
hashes; those commits are on the unmerged branch
`jules-15445701925711240996-2fe6800f`, not on `main`.

Both nodes are remediated and re-verified in the commits that follow.

## Work completed after the node closures

The eleven nodes were closed before the desktop command boundary existed end to
end. The following was found and fixed afterwards; it is listed here because a
`NODE_DONE` row that predates the work is not evidence about it.

| Area | What was wrong | Where the work is |
|---|---|---|
| Desktop command boundary | `main.rs` registered no handler at all, so the packaged window could not answer a single command. | `apps/desktop/src-tauri/src/{lib,commands}.rs`, `tests/command_boundary.rs` |
| Fabricated study data | The daily plan, the readiness band and the "delete everything" result were hard-coded in `data/sample.ts` and component state; the delete announced success without deleting anything. | `src/views/{TodayView,ReadinessView,PrivacyView}.tsx`, `src/ipc/*` |
| Webview-to-Rust hop | Never proven. It is now recorded by the frontend through a real command and read back from the database by a separate process. | `scripts/desktop-live-fire.py`, `.agent/evidence/EP-001/desktop-live-fire.json` |
| Provider and MCP probes | `COMMANDS.md` advertised `provider probe` and `mcp probe-loopback`; neither subcommand existed, so both scripts failed and nothing depending on them had run. | `tools/vector-tools/src/{transports,repair_lane}.rs`, `crates/vector-mcp/src/{protocol,server,client}.rs` |
| Line endings | `.gitattributes` was absent, so `core.autocrlf` made a fresh checkout CRLF and `prettier --check` failed on every frontend file in any clean clone. Found by the new worktree lane. | `.gitattributes` |
| Secrets scan | `secret-scan.py` resolved its repository root to `C:\` and scanned nothing while reporting PASS. | `scripts/secret-scan.py` |
| Graph truncation | `.agent/GRAPH.md` had been cut from 11 nodes to 4; restored and recorded. | `DECISIONS.md` ADR-012 |

## External gates (not blockers of the node)

These are requirements whose evidence cannot be produced inside this repository.
They are recorded here so a release verdict does not silently treat them as met.

| Requirement | Gate | Why it cannot be satisfied here |
|---|---|---|
| REQ-036 | Windows code signing certificate | Signing requires a private key held by the release owner. A binary, an MSI and an NSIS installer were all built and the binary was launched and observed; signing is a release-authority action. |
| REQ-037 | Release signature | The digest and tamper-detection halves are proven, including a refused restore of a byte-flipped archive. A signature needs the same key as REQ-036. |
| REQ-038 | Manual screen-reader validation (Narrator/NVDA) | Requires a human using assistive technology. Automated checks exist; they do not substitute. PREFLIGHT PF-017. |
| REQ-060 | Trademark / name clearance | A legal judgement with no in-repo evidence. Must be resolved before GA. |
| REQ-032 | Opening a pull request | Needs GitHub credentials. The record, the approval gate and the absence of any auto-merge path are enforced and tested. |
| REQ-014 | A running llama.cpp server and a GGUF model | An environment provisioning decision, not code. The adapter, its health probe and its fail-closed routing are implemented and tested. |
| REQ-016, REQ-018 | A signed-in grok and claude session | Both CLIs are installed and probed; both report that they are not signed in. Signing in is an account action that cannot be fabricated. |

The full account of the post-closure work, including the defects it uncovered, is
in `.agent/evidence/EP-010/PARTIAL_CLOSURE_REPORT.md`.

## Content corpus rounds (after the eleven node closures)

The corpus work runs after every node was closed. It is recorded here for the same
reason the post-closure work above is: a `NODE_DONE` row that predates this work is
not evidence about it.

| Round | What it delivered | Evidence |
|---|---|---|
| 12 | Item store, serving API, generator wiring, Word Knowledge and Electronics Information ingestion, content manager, per-item provenance. | `.agent/evidence/EP-007/` (earlier rounds), `crates/vector-{persistence,application,questions}` |
| 13 | Paragraph Comprehension ingestion (1,047 active items from 17 public-domain works), the passage column (`005_content_passage.sql`), corpus reachability in the practice surface, and the defects listed below. | `.agent/evidence/EP-007/content-corpus/ROUND-13-REPORT.md` |

Round 13 found four defects that the earlier rounds' evidence could not see, because
each needed a check that did not exist yet:

1. The passage builder rejoined split sentences with a space, so where the split was
   not at a space the item quoted text the work does not contain. Passages are now
   cut from the paragraph by character span.
2. Sentence terminators inside initials and abbreviations (`R.R.`) split tokens in
   half.
3. A list of illustrations was built into an item as a passage, with index entries as
   its options.
4. `darwin-origin.txt` was cited as Project Gutenberg **#2009** while the file is
   **#1228** — a false provenance claim that only a re-download could reveal. The 13
   affected items were deleted and re-ingested; 17/17 works now re-download to the
   bytes they are cited as.

Separately, four E2E tests had been failing since the corpus replaced the sample
literals, asserting option text from `sample.ts` that the practice view no longer
serves. They were proven pre-existing by running the same specs at `1ae9dce` in a
worktree, and fixed by giving the specs a shared stub of the Tauri bridge and by
reading the correct option and the item id from the page rather than hard-coding
them. The E2E suite is green: 34 passed.

Corpus as stored in the application's own database after round 13: 2,000 WK,
3,668 EI, 1,047 PC = 6,715 active items, 29 evidence records, 8,715 citations,
26,860 review rows, database SHA-256
`9beaf1c2731936617eca25f70e5790808b294d6665d353a139f3b02b75801376`.

### Round 14

| Area | What it delivered | Evidence |
|---|---|---|
| Mechanical Comprehension | Eight computable templates (lever, gear ratio, block and tackle, wheel and axle, piston pressure, hydraulic jack, inclined plane, mechanical advantage). Each answer is arithmetic over the machine's geometry with an executable proof; the interface now offers the subtest. | `crates/vector-questions/src/factory.rs`, `mc_probe` example, factory tests extended to MC |
| General Science | `crates/vector-questions/src/facts.rs` reads a public-domain question-and-answer work: the book's question is the stem, its answer is the correct option quoted, and the distractors are its answers to other questions. `content ingest-facts` wires it through the pipeline and the vault. 301 active items from 450 questions, 0 refused. | `.agent/evidence/EP-007/content-corpus/ROUND-14-REPORT.md` |

Corpus as stored after round 14: 2,000 WK, 3,668 EI, 1,047 PC, 301 GS = 7,016 active
items, 29 evidence records, 9,016 citations, 28,064 review rows. AR, MK and MC are
generated on demand by the application. Database SHA-256
`745803ba30c331345c78920614e81573e063bdde890a7ccb4e13db0cabddbb73`.

Still open after round 14, with the reconnaissance recorded rather than repeated:
Shop Information has a source whose 30 purpose sentences have not been mined yet, and
Auto Information has no public-domain source with usable structure. Content packs
remain unpopulated: quarantine is exercised, install is not.

### Round 15

| Area | What it delivered | Evidence |
|---|---|---|
| Signed content packs | `crates/vector-application/src/packs.rs` builds a pack from everything a store serves, verifies a received pack (schema, compatibility, content hash, Ed25519 signature against a trusted key, provenance completeness, prohibited-source scan, answer consistency, reviewer state) and installs it in one transaction. `migrations/006` gives the registry the identity installation verifies; `migrations/007` makes pack membership its own relation. Five tool commands, all in `COMMANDS.md`. | `.agent/evidence/EP-007/content-corpus/ROUND-15-REPORT.md`, `packs/pack-e2e.log`, `packs/pack-digests.json` |

The pack built from the real corpus carries 7,016 items and 28 sources; installed into
an empty store it delivers all of them and the serving path offers every one. Three
defects were found by exercising it against the real corpus rather than an empty one: a
new version that repackaged content withdrew it, a pack overlapping the corpus failed
on a foreign key, and rollback could reinstate a deliberately quarantined version.

The signing key is deliberately outside the repository
(`%APPDATA%\com.vector.app\keys\content-pack-signing.key`); the first attempt wrote it
under `.agent/evidence/` and would have committed a private key.

Still open after round 15: packs are not reachable from the interface or from a Tauri
command, Shop and Auto Information still have no items, and the pack schema carries
neither the curriculum graph nor the calibration metadata that `CONTENT_PACK_SPEC.md`
lists.
