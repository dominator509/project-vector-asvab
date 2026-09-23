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

### Round 16

| Area | What it delivered | Evidence |
|---|---|---|
| Packs in the product | Three Tauri commands (`content_packs`, `content_pack_install`, `content_pack_rollback`), the trusted signing key as configuration rather than an argument, the content manager's packs panel (status, signature state, rollback, install by path), IPC types and validating readers, and a fake that models the registry. | `.agent/evidence/EP-007/content-corpus/ROUND-16-REPORT.md` |

The reader refuses an install report claiming more deliveries than the pack holds, and
the command refuses every pack when no trusted key is configured, because a caller able
to name the key it trusts could install a pack it signed itself.

Gates: all eight exit 0. 880 Rust tests across 69 binaries, 210 frontend tests, 36 E2E
tests; the packaged artifact was rebuilt and the packaged live-fire passed.

Objective part 5 is now complete: the content manager installs, quarantines and rolls
back packs. Still open: Shop and Auto Information items, the curriculum graph and
calibration metadata in the pack schema, and the Word Knowledge distractor gap.

### Round 17

| Area | What it delivered | Evidence |
|---|---|---|
| Shop Information | `crates/vector-questions/src/purposes.rs` reads a tool manual's descriptions and forms a "Which tool is used to ...?" item from each, with the purpose quoted verbatim and distractors drawn from the manual's other tools. `content ingest-tools` wires it through the pipeline, the vault and the CLI. 31 items from *Tools and Their Uses* (NAVEDTRA 1971) and *TM 11-453 Shop Work* (US Army, 1942). | `.agent/evidence/EP-007/content-corpus/ROUND-17-REPORT.md`, `ingest-tools-si.json` |
| The OCR check | Found doing nothing on two paths (a per-word test handed whole sentences, so it could not fail) and, once fixed, found doing harm (226 of 301 General Science items refused, every refusal an ordinary English word). It now runs only where its precision was measured: NEETS glossary terms (EI) and tool-name options (SI). `scripts/probes/check-options.py` reports 0 options containing scan damage across 7,047 items. | `check-options.log`, `ROUND-17-REPORT.md` |

Three Machinery's Reference Series booklets were downloaded, measured and **refused**: their
scans mangle tool names (`MaWng Oonoave Forming`, `Drill Jiers Drill`) and shipping them would
have put visibly broken options in front of learners.

Corpus as stored after round 17: 2,000 WK, 3,668 EI, 1,047 PC, 301 GS, 31 SI = 7,047 active
items, 31 evidence records, 9,047 citations, 28,188 review rows. Gates: all eight exit 0; 906
Rust tests across 70 binaries, 210 frontend, 36 E2E.

Still open: Auto Information has no items; Shop Information is thin at 31; the Word Knowledge
distractor gap is unchanged; the pack schema still lacks the curriculum graph and calibration
metadata.

### Round 18

| Area | What it delivered | Evidence |
|---|---|---|
| Auto Information | `purposes.rs` reads the other half of a description: `Screw extractors are used to remove broken screws` now also forms "What are the screw extractors used for?", with the manual's other components' functions as the wrong answers. Two Army manuals supply it (*TM 9-8000*, *TM 9-2700*), recorded in the vault with their own URL and digest. **59 items.** | `.agent/evidence/EP-007/content-corpus/ROUND-18-REPORT.md`, `ingest-tools-ai.json` |
| More of the relation | The reader knew only `is used to` and `is used for`. `scripts/probes/description-shapes.py` counted the shapes it could not read -- in TM 9-8000, `is designed to` 66 times and `serves to` 23 times beside 134 `is used to` sentences -- and reading `is designed to`, `is intended to` and `serves to` took the two automotive manuals from 46 descriptions to 78. `serves as` is deliberately not read: its complement is a noun. | `description-shapes.py`, `ROUND-18-REPORT.md` |
| Nine reading rules | Each written against a sentence that reached a learner: a purpose opens with a verb these manuals use (`Which tool is used to heavy, for medium, and for light duty only?`); punctuation is not part of the word it follows (`used to gripping`); a purpose holds no full stop (a quoted sentence ran on); Latin letters only (`Oй filters`, `fluid pressure юг`); a class is not a tool (`the more permanent type`); a possessive names a document; scan damage is refused rather than repaired (`be- tween`, `EVAPORATOR CORE CAPILLARY TUBE`, `of to candlepower`); a name is not ended by its own first word (`Inside micrometers`); Auto Information options are parallel infinitives. All 16 mutations caught. | `ROUND-18-REPORT.md`, `mutation-round18.py` |
| One question, asked once | `ContentItemRepo::question_exists` identifies a question by the stem, the correct answer and the passage, and all five ingest paths ask it before storing. It found that **3,305 of the corpus's 7,111 items were questions it had already asked** -- 3,254 in Electronics Information, whose bank was 89% the same definition with its options shuffled. The corpus is 3,809 distinct questions. | `count-items.log`, `check-corpus.log`, `ingest-ei.json` |
| Two defects behind it | A refused item left a draft row (the row is written before the citation is checked), which the question check then read as a question already held: an ingestion naming an unrecorded source reported six items already present and activated none. And ten Paragraph Comprehension items cited Project Gutenberg #2009 for a file that is #1228 -- the provenance check passed because it re-downloads the manifest, and the manifest had been corrected while the vault record had not. | `ROUND-18-REPORT.md`, `gutenberg-provenance.log` |

Corpus as stored after round 18: 1,949 WK, 414 EI, 1,050 PC, 301 GS, 36 SI, 59 AI = **3,809
active items**, 33 evidence records, 5,758 citations, 15,236 review rows, every provenance
invariant a zero -- including the four added this round: the Auto Information stem and option
shapes, the one-question-per-stem rule, and scan damage read back by an implementation written
independently of the Rust rules it checks.

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; 1,700 Rust tests across
109 binaries, 210 frontend tests, 36 E2E, packaged live-fire passed against artifact
`50d6a5a4216458c88fba7ae59f316a49990b33d185e5bfb4f1bd3bc070714ae1`. Verdict unchanged:
`CONDITIONAL_EXTERNAL_GATES` (27 PASS, 11 PARTIAL, 2 PENDING, 1 EXTERNAL_REQUIRED, 1
DEFERRED_LONG_RUNNING, 0 FAIL).

**The number that matters is 3,809, not the 7,111 that preceded it.** A smaller corpus that asks
3,809 different questions is worth more to a learner than a larger one that asks 414 questions
repeatedly, and the round's own counts were the thing that had to be corrected before any of the
new content could be trusted.

Two work products are the measurement rather than the code: `scripts/rebuild-corpus.py` rebuilds
every subtest from its sources in one run (and prunes the vault record that is not evidence),
and `scripts/probes/purpose-verbs.py` is how the verb list was written from the sources instead
of guessed. Seven NAVEDTRA manuals were downloaded, measured and refused for scan quality
(`Special equipment`, `Relatively large amounts`, `reiatively`, `ordir.aty`), with the refusals
recorded rather than shipped to make a count look better.

Still open: the bank sizes (414 EI, 1,949 WK, 36 SI, 59 AI against subtests that ask sixteen
questions in a sitting, with no better source found than the ones measured); a noun-head check
for the six residual generic names, which needs Webster's part-of-speech markers exposed through
`Dictionary`; the Word Knowledge rare-distractor gap; the curriculum graph and calibration
metadata the pack schema still lacks.

### Round 19

| Area | What it delivered | Evidence |
|---|---|---|
| The rest of NEETS | The Electronics Information bank was built from ten of the series' twenty-four modules, because ten were what an earlier round had downloaded. The whole collection is one Internet Archive item; `fetch-archive-text.py --list` lists it, the other fourteen modules are downloaded, and the bank goes from **414 to 1,565 questions**. | `.agent/evidence/EP-007/content-corpus/ROUND-19-REPORT.md`, `ingest-ei.json` |
| Eight more Gutenberg works | Agricola's *De Re Metallica*, Whewell's *History of the Inductive Sciences*, Candolle's *Origin of Cultivated Plants*, Thorndike's *A History of Magic and Experimental Science*, two volumes of Kirby and Spence's *Introduction to Entomology*, two of Pliny's *Natural History*: Paragraph Comprehension goes from **1,050 to 1,676 questions**. The new `fetch-gutenberg.py` reads each file's own Gutenberg header and refuses a download whose number disagrees with the one asked for. | `gutenberg-manifest.txt`, `gutenberg-provenance.log` |
| The relation the manuals name | `The purpose of the piston skirt is to keep the piston from rocking in the cylinder` puts the subject *after* the phrase, so reading what precedes the verb names `purpose`. Reading `function of X is to Y` and `purpose of X is to Y` takes the automotive manuals from 78 descriptions to 97 and Auto Information from **59 to 70 questions** (`hair spring`, `engine lubrication`, `piston skirt`, `air-over-hydraulic suspension system`). A gerund subject is refused: `The purpose of burning fuel in the priming cup is to ...` describes an action, not a thing. | `description-shapes.py`, `ROUND-19-REPORT.md` |
| A thin module could end the run | Ingesting all twenty-four modules **failed**: the pipeline refuses a module that builds nothing (correctly, for a one-module caller) and the loop propagated the refusal, so the run aborted on module 24 after storing 1,152 items from the other twenty-three and never wrote its report. A module that contributes nothing is now recorded as `skipped` and the run continues; a run in which *no* module produced an item still fails. Both are mutation-proved. | `ROUND-19-REPORT.md`, `mutation-round18.py` |
| Ingestion had become quadratic | The question check runs once per built candidate, and a builder produces about 10,000 candidates to find a few hundred questions; without an index the predicate scanned the table every time and the 24-module ingestion had not finished after ten minutes. `migrations/008_content_question_index.sql` adds `(subtest, LOWER(TRIM(stem)))` — the expression, because the comparison is case- and space-insensitive — and the application embeds it with the other seven. | `migrations/008_content_question_index.sql` |

Corpus as stored after round 19: 1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 37 SI, 70 AI = **5,598
active items**, 55 evidence records, 7,547 citations, every provenance invariant a zero
(including no question asked twice within a subtest, and no option that is not the shape its
subtest asks for). **25/25 Gutenberg works re-download to the bytes they are cited as.**

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; 1,703 Rust tests across
109 binaries, 210 frontend tests, 36 E2E, packaged live-fire passed against artifact
`36c47ec7d18a55dfe11eef7dc975604a38dcdc8d4cfd65c523f37e07c4709c7e`. Verdict unchanged:
`CONDITIONAL_EXTERNAL_GATES` (27 PASS, 11 PARTIAL, 2 PENDING, 1 EXTERNAL_REQUIRED, 1
DEFERRED_LONG_RUNNING, 0 FAIL).

Ten Army ordnance maintenance manuals were downloaded and measured for the Auto Information
shape and **refused**: they hold two sentences of that shape between them, because they are
repair *procedures* (`Remove the four bolts`) and the relation `X is used to Y` belongs to the
principles manuals. Two *Machinery's Reference* booklets hold three descriptions each.

Still open: **SI 37 and AI 70 are thin**, and every source measured for them across two rounds
has been refused with numbers rather than shipped; **GS is one work** (301), because only *The
book of wonders* in the corpus has the question-and-answer shape `ingest-facts` reads, so the
next step there is finding works of that shape rather than more prose; the noun-head check for
the residual generic names still needs Webster's part-of-speech markers exposed through
`Dictionary`; the Word Knowledge rare-distractor gap; the curriculum graph and calibration
metadata the pack schema still lacks.

### Round 20

| Area | What it delivered | Evidence |
|---|---|---|
| More sources for the thin banks | Pre-1929 technical books on Project Gutenberg, a vein the corpus had not tried: *Modern Machine-Shop Practice*, *Farm Mechanics*, *How it Works*, *Elementary Lathe Practice*, *The Economy of Workshop Manipulation*, *Precision Locating and Dividing Methods*, four engine manuals and three invention books were fetched and measured. Shop Information goes 37 -> **55**, Auto Information 70 -> **75**. | `.agent/evidence/EP-007/content-corpus/ROUND-20-REPORT.md` |
| A citation that would have been wrong | `ingest-tools` cited every source as an Internet Archive page. Ingesting a Gutenberg work through that form would have recorded a URL that does not resolve to the bytes -- the defect class round 18 found in the vault. The work argument is now `<source>:<id>:<title>=<path>` with `archive` or `gutenberg`, written down rather than guessed from the id's shape, because an all-digit id is a valid id in both systems. | `ROUND-20-REPORT.md`, `mutation-round18.py` |
| The third instance of one bad source ending a run | Adding *Elementary Lathe Practice* (which describes no tool) to the Shop sources aborted a run that had stored 49 items **and left Auto Information un-ingested**. The pipeline's per-source refusal was propagated by a loop built to be resilient, exactly as in Electronics Information before round 19. A source that contributes nothing is now `skipped` and the run continues; a run in which nothing was produced still fails. | `ROUND-20-REPORT.md`, `content.rs` tests |
| The pack's curriculum and calibration | `CONTENT_PACK_SPEC.md` lists both and rounds 15-19 recorded them missing. A pack now carries `curriculum` (objective, subtest, title, prerequisites) and `calibration` (expected-correct share, responses, basis). Installation verifies that every item's objective is declared with a matching subtest, that prerequisites exist and are acyclic, and that every taught objective has exactly one in-range calibration entry. `build-curriculum.py` writes the corpus's own; a pack carrying both was built and installed: 5,621 items, 56 sources, 11.5 MB, `signature_valid: true`. | `packs.rs`, `pack-curriculum.json`, `pack-calibration.json`, `packs/pack-build-v3.json` |

Corpus as stored after round 20: 1,949 WK, 1,565 EI, 1,676 PC, 301 GS, **55 SI**, **75 AI** =
**5,621 active items**, 69 evidence records, 7,570 citations, every provenance invariant a zero.
**25/25 Gutenberg works re-download to the bytes they are cited as.**

Gates: `sh scripts/verify.sh` exits 0; all 24 mutations caught by `mutation-round18.py` (the
nineteen reading rules of round 18, round 19's run-level and named-relation rules, and this
round's per-source tolerance, source-kind citation and two pack rules).

**The calibration is honestly empty of measurements.** No learner has answered these items, so
every entry carries `responses: 0` and a basis that says so; the number it carries is derived
from the items' own difficulty scale. The field that separates a declaration from a measurement
is `responses`, and it is zero on purpose -- a pack claiming measured difficulty it had never
measured would be the same class of lie as a citation nobody can re-check.

Still open: **SI 55 and AI 75 remain thin** for a subtest that asks sixteen questions in a
sitting, and every source measured across three rounds is either in the corpus or refused with
numbers; **the curriculum and calibration are not yet visible in the product** -- they travel in
the pack and the installer verifies them, but the content manager does not show what a pack
teaches and the study planner does not order work by the prerequisites, which is the next
round's natural work; GS is still one work; the noun-head check and the Word Knowledge
rare-distractor gap are unchanged.

### Round 21

| Area | What it delivered | Evidence |
|---|---|---|
| What a pack teaches, in the product | `InstalledPackDto` gains one entry per objective the pack's manifest declares -- title, subtest, prerequisites, calibration -- and the content manager renders them: `Auto Information: what a component is for (AI) -- after OBJ-SI-TOOLS-01 -- 53% expected correct, declared, no responses recorded`. The sentence turns on `responses`: a figure with none behind it is a declaration from the material, and the panel says so rather than printing a percentage that reads like an observation. A pack whose manifest predates the curriculum says that instead of rendering an empty list. | `.agent/evidence/EP-007/content-corpus/ROUND-21-REPORT.md`, `ContentManagerView.tsx` |
| The plan follows the curriculum | `Services::study_plan` reads the active pack's graph and moves a drill that teaches a prerequisite ahead of the drill that requires it, appending `; it is the prerequisite for AI in this pack's curriculum` to the drill's own reason. The order is the *pack's*, not this layer's opinion: a device with no pack installed has no declared curriculum and its plan is unchanged. The move is conservative -- it pulls a prerequisite earlier only when both subtests are already planned, and the planner's own reason survives. | `service.rs`, `service_layer.rs` |
| The defect it found | `objectives_from_manifest` parsed the registry's stored manifest as a `PackDocument`, but the installer stores the pack's *payload* (the bytes the signature covers). Every pack therefore listed zero objectives, and the panel would have shown "declares no curriculum" for a pack that declares six. The integration test caught it on its first run: `left: 0, right: 4`. | `packs.rs` |

Corpus unchanged at **5,621 active items** (1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 55 SI, 75 AI),
69 evidence records, 7,570 citations, pack `core-asvab v3` active, every provenance invariant a
zero.

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; 1,717 Rust tests across
109 binaries, **211** frontend unit tests, 36 E2E, packaged live-fire passed against artifact
`f14cd345df8568de02059b2fe0249dad53f44a12d20442b1d743cecd84504b38`. **All 26 mutations
caught.** Verdict unchanged: `CONDITIONAL_EXTERNAL_GATES`.

The panel is covered at three levels, because each level can fail on its own: a Rust test that an
installed pack reports the graph and calibration it was built with, a view test that the panel
renders the prerequisite edge and distinguishes a declared figure from a measured one, and an
end-to-end test in the built bundle through the stub boundary.

Still open: **SI 55 and AI 75 remain thin**; **the plan orders by prerequisite but does not
track mastery per objective** -- attempts are recorded per item and items carry an objective id,
so per-objective mastery is derivable, and the plan still reasons about subtests while the
curriculum's finer grain is used only for ordering; GS is one work; the noun-head check and the
Word Knowledge rare-distractor gap are unchanged.

### Round 23

| Area | What it delivered | Evidence |
|---|---|---|
| Four more shapes read | `scripts/probes/relation-shapes.py` counts every way the sources state the relation; beside the shapes already read they write `is employed to` 52 times, `is made to` 43, `is adapted to` 11, `is arranged to` 13. Reading `employed`, `utilized`, `adapted` and `arranged` produced real items (*diagonal pliers are adapted to cutting small objects flush with a surface*; *a tractor is arranged to pull its load in two different ways*). `is made to` is deliberately **not** read: `A provision usually is made to install a fuel gage` has a provision for a subject, and a shape that produces "Which tool ...? -- a provision" is worse than an unread one. | `.agent/evidence/EP-007/content-corpus/ROUND-23-REPORT.md` |
| Three precision rules | A measurement cannot head a name (`about inch thick` from `softened sheet copper about 1/32 inch thick`); a reference back into the paragraph is not a name (`the same device`, `in connection`, `third class`); a figure's label is not part of a sentence (a standalone lower-case letter, as in `the tool e would cut the [V]-shaped groove i`). `work`, `substance`, `class` and `tool` join the head nouns that mean a name describes something other than an object. | `purposes.rs`, `mutation-round18.py` |
| A source refused, and the count that went down | *Modern Machine-Shop Practice* (Rose, PG #39225, 4.9 MB) was fetched, measured and **refused**: 21 descriptions, 16 items, four still wrong after every rule (`in connection`, `in rods`, `tension`, `Rubber joints` as the answer to "Which tool is used to ...?"). Lower precision than the seven manuals refused in round 18 and the ten in round 20. **Shop Information goes 55 -> 42**: 16 mixed items out, 3 good ones in -- the first round that had to apply the refusal standard to a source already in the corpus. | `ROUND-23-REPORT.md`, `rebuild-corpus.py` |
| A local vein measured and refused | Webster's 1913 defines 1,463 things by what they are for, which looked like a large Shop Information source already in the repository. The useful subset -- headword in the manuals' vocabulary and a definition opening with a tool-class noun -- is 49 entries, contaminated by etymology (`brake ... an instrument for breaking flax` is about the word's German origin) and by arcana (`odontograph`, `mangle`, `manometer`). Items built from it would quote etymology as claims about tools. | `ROUND-23-REPORT.md` |

Corpus after round 23: 1,949 WK, 1,565 EI, 1,676 PC, 301 GS, **42 SI**, **77 AI** = **5,610
active items**, 69 evidence records, 7,559 citations, every provenance invariant a zero. Pack
`core-asvab` was rebuilt and reinstalled at **v4** (5,610 items, 55 sources, content hash
`sha256:844832a6...`), because a corpus rebuild changes item identities and leaves the previous
pack's membership describing items that no longer exist.

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; 1,719 Rust tests across
109 binaries, 211 frontend tests, 36 E2E, packaged live-fire passed against artifact
`7453448f971c210183e087b79726f93fee45622ec954d34a8801e1303fde45cf`. **All 29 mutations caught.**
Verdict unchanged: `CONDITIONAL_EXTERNAL_GATES`.

Still open: **SI 42 and AI 77 remain thin**, and this round lowered SI to raise its precision; the
sources that would raise both have been measured (Rose refused, the other Gutenberg mechanics
books yield 0-6 descriptions each, the microfiche manuals stay refused), leaving the
noun-complement family (`serves as` 94, `acts as` 139, `is provided with` 87) as the next lever --
it needs a third question frame and a link field on the purpose. **One known bad item** survives
every rule (`ita size` in *Tools and Their Uses*, subject the steel shank): catching it needs a
misreading test on short tokens that the probes measured as too blunt for prose, so it is recorded
the way the two rare Word Knowledge distractors are. The plan still orders by prerequisite without
tracking mastery per objective, and General Science is still one work.

### Rounds 24-25

| Area | What it delivered | Evidence |
|---|---|---|
| The relation's third form | The manuals state the relation three ways and the reader knew two. `relation-shapes.py` counts the third -- `serves as` 94, `acts as` 139, `is used as` 33 across the Shop and Auto sources, more than any other phrase -- and none of it was readable because those complements are *nouns*: `The bimetallic strip serves as one of the contact points.` A purpose now carries the form it was stated in (`Link::To`, `Link::For`, `Link::As`) and the question follows it. **Auto Information goes 77 -> 94 items.** | `.agent/evidence/EP-007/content-corpus/ROUND-24-25-REPORT.md` |
| Three properties that keep it honest | The frame is the source's: a `serves as` description is **not** asked as a Shop Information question ("Which tool serves as a switch?" offers a component as a tool -- the defect round 23 refused a source for), it is asked the other way round. The options match the question: a `serve as` prompt takes noun phrases and a `used for` prompt takes infinitives, and the verifier refuses an item that mixes them, because offering "a thrust bearing" beside "to prevent leakage" tells the learner which is the odd one out. And a purpose holds no semicolon: it always joins two clauses. | `purposes.rs`, `mutation-round18.py` |
| A mutation that outlived its run | The harness disables one rule at a time and restores the file in a `finally`. A run that is **killed** never reaches it, and the mutation stays: two `if false &&` guards sat in `purposes.rs` across a round boundary, announcing themselves only as two tests failing for reasons that made no sense (`Both hands` accepted as a tool name) while the rule looked present in the file. The harness now refuses to run when a replacement is present *instead of* the text it replaced -- and the first version of that guard was wrong, because several mutations delete one line from a pair and the replacement is a substring of the original. | `mutation-round18.py` |
| A probe that was wrong for the new frame | Two `check-corpus.py` invariants were narrower than the corpus after this round: an Auto Information stem is now either `What ... used for?` or `What ... serve as?`, and the option rule checks the options are in the form *the question asks in* rather than that they are infinitives. Third time a probe has been the thing at fault (the capitals rule of round 18, the Word Knowledge source count of round 13): a probe encodes the corpus as it was when it was written. | `check-corpus.py` |

Corpus after round 25: 1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 42 SI, **94 AI** = **5,627 active
items**, every provenance invariant a zero, 25/25 Gutenberg works re-downloading to their cited
bytes. Pack `core-asvab` rebuilt and reinstalled at **v5** (5,627 items, 55 sources).

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; 1,721 Rust tests across
109 binaries, 211 frontend tests, 36 E2E, packaged live-fire passed against artifact
`028bb9240678de2a2cf2a069bb03ea4a6dfe8f99883c1d629649d078a4c793f9`. **All 32 mutations caught.**
Verdict unchanged: `CONDITIONAL_EXTERNAL_GATES`.

Still open: **SI 42 remains the thin bank**, and the noun frame did not help it by design -- the
descriptions it reads are about components, and a Shop Information item asks which tool does
something; sources that would raise SI have been measured and refused across four rounds. **AI's
new frame has its own noise floor**: `What does the Slack serve as?` (a property) and `What do
the Hangers frame members serve as?` (a heading run-in) reached the bank and are recorded as
known, the way the steel-shank item is. The plan orders by prerequisite without tracking mastery
per objective, General Science is still one work, and the Word Knowledge rare-distractor gap is
unchanged.

### Rounds 26-27

| Area | What it delivered | Evidence |
|---|---|---|
| Mastery at the curriculum's grain | `AttemptRepo::objective_stats` joins each attempt through the item it was on to the objective that item teaches -- an attempt records the question, the question records the objective, so it is read rather than maintained. `Services::objective_mastery` reports attempts, correct, the same Laplace estimate the subtest mastery uses (no evidence is 0.5, not 0), and `waiting_on`: the prerequisites this learner has not met. | `.agent/evidence/EP-007/content-corpus/ROUND-26-27-REPORT.md` |
| A threshold in one place, and a plan that names an objective | `PREREQUISITE_MET = 0.6`: no attempts gives 0.5, one correct out of one gives 0.67, one wrong gives 0.33 -- so an untouched prerequisite is unmet rather than assumed known. Each drill carries the weakest objective whose prerequisites are met (ties broken by fewer attempts, then id) and its reason says which and why. An objective whose prerequisite is unmet is not offered. Read end to end from the installed pack; no pack, no objective. | `service.rs`, `TodayView.tsx` |
| The defect the live readback found | `examples/plan_report.rs` opens a *copy* of the installation's own database and prints the plan. Its first run sent the learner to `AO` -- Assembling Objects -- for which this installation has neither items nor templates: a drill that could not start. Unit, view and E2E tests all passed; only reading a real store showed it. Only servable subtests are now handed to the planner, and the filter is in the service rather than on the planner's output so the freed minutes are not left allocated to a drill that is not in the plan. | `plan_report.rs`, `service.rs` |

Corpus unchanged at **5,627 active items** (1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 42 SI, 94 AI);
pack `core-asvab v5` active; every provenance invariant a zero.

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; **all 35 mutations
caught**, including this round's three (the prerequisite threshold, the objective focus and the
servable filter). Verdict unchanged: `CONDITIONAL_EXTERNAL_GATES`.

Two service tests failed when the servable filter landed, and that was the filter working: they
wanted a plan with SI and AI in it while their fixture store held nothing but factory-generated AR
items -- the same assumption, in miniature, that the readback caught in the product. The fixture
now ingests a Shop and an Auto Information manual through the real pipeline.

Still open: **the curriculum's grain stops at the plan** -- mastery is read per objective and the
practice surface still chooses items by subtest, so selecting the next item from the objective the
plan named is the next step; SI 42 remains the thin bank; AI's noun frame carries a known noise
floor; General Science is one work; the Word Knowledge rare-distractor gap is unchanged.

### Round 28

| Area | What it delivered | Evidence |
|---|---|---|
| The plan's objective reaches the item | `ContentItemRepo::servable_in(subtest, Option<&str>)` narrows the servable read with one predicate in one SQL string (`AND (?2 IS NULL OR objective_id = ?2)`), so the provenance rules below it cannot drift between the narrow and the wide read; `servable` delegates with `None`. `ContentPipeline::next_item_for(subtest, objective_id, seen)` prefers the named objective's items and falls back to the whole subtest when the objective holds none -- an unbuilt objective is a content gap, and serving nothing would turn it into a blocked learner. The rotation rule moved into a free `pick` helper so both reads rotate identically. | `crates/vector-persistence/src/content.rs`, `crates/vector-application/src/content.rs` |
| From the plan into the session | The command takes an optional `objective_id`; `VectorClient.contentNext(subtest, seen, objectiveId?)` and `usePracticeItems(..., objectiveId)` pass it on every fetch; `PracticeContentView` takes a `PracticeRequest`, says which objective it is serving, and clears it when a subtest is chosen by hand; `TodayView` renders a per-drill control that starts the session the plan asked for (only when navigation is supplied, because a button that went nowhere is worse than a label); `Shell` carries the request into the view and announces it in the polite live region. | `commands.rs`, `client.ts`, `usePracticeItems.ts`, `PracticeContentView.tsx`, `TodayView.tsx`, `App.tsx` |
| Read back from the installation's own store | `examples/plan_report.rs` now prints what a session for each named objective is actually served. Against a copy of the real database: SI -> `OBJ-SI-TOOLS-01` item, EI -> `OBJ-EI-TERMINOLOGY-01`, GS -> `OBJ-GS-EXPLAIN-01`, PC -> `OBJ-PC-DETAIL-01`, WK -> `OBJ-WK-SYNONYM-01`, each served item carrying the objective the plan named. | `.agent/evidence/EP-007/content-corpus/ROUND-28-REPORT.md` |
| Proof the rule is load-bearing | The mutation harness grew vitest and Playwright runners and now catches **all 40** mutations: narrowing ignored, fallback removed, objective not sent to the backend, request not carried into the view, and the built bundle serving the subtest's first item instead of the objective's. The Playwright runner rebuilds the bundle first, because a spec run against a stale `dist` describes code that no longer exists. Two harness defects were fixed on the way: a run now refuses to be judged alongside another test command, and it restores the tree byte-exactly -- its text-mode restore had rewritten line endings and made the format gate fail on files nobody had edited. | `scripts/probes/mutation-round18.py`, `mutation-round28.log` |

Corpus unchanged at **5,627 active items** (1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 42 SI, 94 AI);
pack `core-asvab v5` active; every provenance invariant a zero.

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; artifact digest
`6983c707fafa8ac2f60e1c87fc35636cadf27f6a0fd78a08fd91caaba125f573`, candidate epoch 16, verdict
`CONDITIONAL_EXTERNAL_GATES`. Tests: 70 Rust binaries / 949 passed (`cargo test --workspace`), 217
frontend unit tests in 12 files, 9 integration, 37 Playwright, 0 failures.

Still open, measured rather than assumed: the narrowing is a **no-op on the ingested subtests today**
because the curriculum declares one objective per ingested subtest and every item of that subtest
carries it (read back per subtest/objective from the store); a plan **cannot name an objective for a
generated subtest** because the pack's curriculum declares none of the factory's 22 AR/MK/MC
objectives; practising a generated subtest on a real installation still takes one explicit "Prepare
40 ... questions" click. SI 42 remains thin, AI carries its known noise floor, General Science is one
work, and the Word Knowledge rare-distractor gap is unchanged.

### Round 29

| Area | What it delivered | Evidence |
|---|---|---|
| The computable subtests are in the pack | `examples/generate_corpus.rs` runs the application's own verify-store-activate pipeline with fixed per-subtest seeds, and `rebuild-corpus.py` calls it for AR/MK/MC after the ingested subtests, so a documented rebuild reproduces the corpus the pack is built from. The pack `core-asvab v6` now carries 6,961 items, so a plan can name an objective for every subtest it schedules and a fresh installation serves one without pressing "Prepare 40 questions". | `generate_corpus.rs`, `rebuild-corpus.py`, `packs/pack-v6.json` |
| A defect the readback caught | `check-corpus.py` found 126 questions asked twice in one subtest: the generated path refused only a repeated *content hash*, so the same question with its wrong answers shuffled was stored again -- the padding the ingestion loops already refuse. `generate_and_activate` now applies the question identity too; the rebuild stores 1,334 distinct questions instead of 1,500 rows with 166 repeats. The probe also gained an independent re-derivation of every executable proof: **1,334 of 1,334 recompute to the option they mark correct**, evaluated in Python rather than trusted from the verifier. | `content.rs`, `check-corpus.py` |
| Every claimed pack transition, on real state (DOD-035) | install v6 over v5 (upgrade), rollback to v5 (1,334 items stored but withdrawn in one statement), activate v6 again, with the database hash at each step. **The transition back did not work**: reinstalling the same signed pack reported success and changed nothing, because `install` deliberately never undoes a rollback -- right as a safety property, but on its own it makes a rollback a one-way door. `pack-activate` is now the way home, it supersedes rather than quarantines the version it replaces, and a test pins both halves. | `packs/state-*.log`, `packs/state-*.sha256`, `repo.rs`, `packs.rs`, `COMMANDS.md` |
| A licence policy had to admit the computable subtests' citation | `pack-install` refused v6 whole: the construct record's licence ("Facts-only; item text is original work") was outside the permitted list. Named in it with the reasoning attached -- the page asserts all rights reserved, so relabelling it public domain would be the dishonest fix; what keeps such a pack honest is the executable proof the installer recomputes and the vault record whose bytes were hashed. | `packs.rs` |
| Clean room (DOD-002, DOD-023) | A fresh `git clone` of the committed tree ran the README's five published commands in order -- install, preflight, generated-pack validation, the full 19-gate sweep, evidence-archive verification -- and all five exited 0. Lockfile digests, tool versions and both artifacts' digests recorded; the clean-room build differs in bytes from the working copy's, which is why no gate asserts digest equality. | `.agent/evidence/EP-009/cleanroom/` |
| Zero state and the golden path (DOD-034) | On a directory that did not exist, the exact release artifact (`sha256 6983c707…`) created and migrated its database, passed its own 19 checks, took the signed v6 pack through the documented command, and completed the golden path: learner, 9-drill plan, one session per named objective, attempts stored once, analytics and mastery read back, provenance re-checked. `examples/golden_path.rs` is new and the soak found a defect in it within minutes (attempt ids did not name the learner, so a repeated run looked like a duplicate). The machine is the developer's, so the clause stays PARTIAL with the virgin-OS residual named. | `.agent/evidence/EP-009/zero-state/`, `golden_path.rs` |
| Abbreviated soak (DOD-038) | `scripts/probes/soak.py`: packaged self-check plus a full golden path per iteration against a copy of the installation's store, with a heartbeat, an integrity check and a row census. 112 iterations over 12.02 minutes, 896 attempts added, 0 failures, integrity ok at every heartbeat; the trial was re-run after the final formatting pass changed the artifact, because old evidence does not prove changed bytes. The report labels itself an abbreviated trial and `release-state.py` reads that record rather than asserting a duration. | `.agent/evidence/EP-009/soak/` |

Corpus **6,961 active items** (WK 1,949 · PC 1,676 · EI 1,565 · AR 466 · MC 447 · MK 421 · GS 301 ·
AI 94 · SI 42), 70 vault records, 8,910 citations, every provenance invariant a zero; pack
`core-asvab v6` active.

Gates: `sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; **all 42 mutations caught**
across cargo, vitest and Playwright; DoD accounting **30 PASS / 11 PARTIAL / 1 EXTERNAL_REQUIRED**
(from 27/11/2/1/1). Verdict unchanged: `CONDITIONAL_EXTERNAL_GATES`.

Next in-repo work, in the order the clauses need it: DOD-013 (runtime canary in a critical proof),
DOD-031 and DOD-040 (the dependency-edge and change-invalidation graphs are still templates),
DOD-036 (RPO/RTO/MTTR unmeasured), the npm advisory state (the gate covers Rust advisories and npm
licences, not npm advisories), and the three open requirements that are work here rather than
external gates: REQ-014, REQ-032, REQ-037.

### Round 30

| Area | What it delivered | Evidence |
|---|---|---|
| The sweep no longer lets one failure hide the rest (DOD-031) | `scripts/verify.sh` runs every gate and records a gate whose declared prerequisite did not pass as `BLOCKED_PREREQUISITE` with the gate that blocked it, exiting non-zero if anything failed or was blocked -- so a reader can tell "this gate failed" from "we never got there". Demonstrated in this round: a formatting failure left all twenty other gates running and recorded. `scripts/dependency-graph.py` derives the graph from the registry, the 484-row accounting and the sweep's own results, and refuses an unnamed blocker, a false gate edge, a passing row with an incoming edge, and any cycle. | `verify.sh`, `dependency-graph.py`, `DEPENDENCY_BLOCKER_GRAPH.json` |
| What a candidate's changes invalidate (DOD-040) | `scripts/change-invalidation.py` records each epoch against its commit and artifact digest, diffs the previous epoch against this one (committed and uncommitted), maps changed paths to surfaces and gates, closes the rerun list over the gate edges, and refuses a changed path no gate covers. It caught a bug in itself on the first run: stripping porcelain output shifted the first path by a character, which the coverage check reported. | `change-invalidation.py`, `CHANGE_INVALIDATION_GRAPH.md` |
| An unpredictable value through the real path (DOD-013) | `scripts/probes/canary-proof.py` draws a learner name and target score from the OS CSPRNG at run time, propagates both through the application's study path, reads them back with a separate SQLite connection, and requires the negative control -- the same canary with one character changed -- to find nothing. The canary enters at the command boundary because no window driver exists here (DOD-004). | `canary-proof.py`, `.agent/evidence/EP-009/canary/` |
| Recovery objectives, and the gap they found (DOD-036) | A truncated, a corrupted and a deleted store are each restored from a verified archive and reconciled to the pre-fault rows and bytes. The first run found that a **truncated store could not be restored at all**: the documented path needs a handle on the live database, and SQLite will not open a malformed one. `restore_verified_at` now restores from the path with the same archive checks and no connection to the damaged file, the CLI falls back to it, and a test plus a mutation pin it. RTO 7.85 s against a declared 60 s. | `backup.rs`, `main.rs`, `ep003_acceptance.rs`, `.agent/evidence/EP-009/recovery/` |
| The mutation harness was misclassifying mutations | Two Rust mutations from round 29 sat in the vitest list and were "caught" by a runner that never loaded them. Entries now live in the list whose runner can execute them, `test_passes` refuses a crate that is not a workspace member or a filter that matched nothing (counting passed *and* failed, because a caught mutation reports "0 passed; 1 failed"), and `vitest_passes` refuses a missing file or an empty selection. **All 43 caught.** | `mutation-round18.py`, `mutation-round30.log` |

DoD accounting moved from 30 PASS / 11 PARTIAL / 1 EXTERNAL_REQUIRED to **34 PASS / 7 PARTIAL / 1
EXTERNAL_REQUIRED**; gates from 19 to **21**. Three accounting rows were narrowed rather than left
stale: `E2E-011` (the cache-free half is executed; the foreign machine is what remains), `SUP-004`
(two clean environments produce different bytes for the same source -- recorded as a measurement,
so the claim stays blocked rather than relabelled), and `E2E-018` (abbreviated trial recorded, full
duration still missing). Corpus unchanged at 6,961 active items; pack `core-asvab v6` active;
artifact digest `f75d5bc89e0c1a96…`, epoch 19; verdict `CONDITIONAL_EXTERNAL_GATES`.

Remaining in-repo: the npm advisory state (`pnpm audit --prod` is clean; dev tooling carries 1
critical / 2 high / 5 moderate, and the gate covers Rust advisories and npm licences rather than npm
advisories) and the three open requirements that are work here -- REQ-014 (local GGUF model),
REQ-032 (`gh` pull-request lane), REQ-037 (signature verification for updates).

### Round 31

| Area | What it delivered | Evidence |
|---|---|---|
| The last in-repo security finding | `pnpm audit` reported 1 critical / 2 high / 5 moderate, all in the frontend toolchain (esbuild's dev server accepting requests from any website; two vite path-traversal and NTLM-disclosure advisories; vitest's redirect-mock file read; playwright). None ships -- the artifact is a Rust binary with a bundled static frontend and `pnpm audit --prod` was already clean -- but the repository is public and the count was real. Upgraded vite 5.4 -> **8.3.0**, vitest 2.1.9 -> **5.0.1**, @playwright/test 1.49.1 -> **1.63.0**, @vitejs/plugin-react 4 -> **6.1.1**; esbuild is no longer installed at all, because vite 8 declares it an optional peer dependency -- which is why the critical advisory is gone rather than patched in place. | `package.json`, `apps/desktop/package.json`, `pnpm-lock.yaml`, `.agent/evidence/EP-009/npm-advisories.json` |
| Re-verified, not assumed | Three majors of build tool and two of the test runner: `pnpm typecheck` exit 0, 217 unit tests in 12 files, 9 integration tests, vite 8.3.0 build exit 0, 37 Playwright tests on 1.63.0 (browser re-installed), and the full sweep -- 21 gates, 0 failed, 0 blocked. `pnpm audit` reports no known vulnerabilities for the full tree and for `--prod`. | `.agent/evidence/EP-009/ROUND-31-REPORT.md`, `verify-round31.log` |
| The advisory check is now part of the accounting | The check contacts the registry, so it stays a documented manual command rather than a sweep gate (the gate covers Rust advisories and npm licences); its result is written to `.agent/evidence/EP-009/npm-advisories.json` and `release-state.py` reads that file into the DOD-021 record, so the accounting states the advisory position and the versions it was measured against. | `release-state.py`, `COMMANDS.md` |
| A declaration that survives its prose | The dependency graph refused three rows whose rewritten reasons no longer named what blocks them -- exactly what DOD-031's graph exists to catch. A blocked reason may now name its blocker explicitly as `blocked-by: <kind>:<name>`, and `E2E-011`, `SUP-004` and `E2E-018` use it. | `dependency-graph.py`, `COMPLETE_TEST_ACCOUNTING.csv` |

DoD 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED; 21 gates exit 0; npm advisories **0** (from 1
critical / 2 high / 5 moderate); artifact digest `23f8c2c5…`, epoch 21. Corpus unchanged at 6,961
active items; pack `core-asvab v6` active; verdict `CONDITIONAL_EXTERNAL_GATES`.

Remaining in-repo: the three open requirements that are work here -- REQ-037 (signature verification
for updates; the certificate is external), REQ-032 (`gh` pull-request lane), REQ-014 (local GGUF
model).

### Round 32

| Area | What it delivered | Evidence |
|---|---|---|
| The update path's missing half (REQ-037) | `crates/vector-platform/src/update.rs`: an Ed25519-signed update manifest verified in a deliberate order -- format and application, then the signer against the key this installation pins (a valid signature by an untrusted key is refused as *that*, not as a bad signature), then the signature over a length-prefixed payload rebuilt from the manifest's current fields, then the version relation because a version mismatch is not an attack, then the artifact by SHA-256 and length before anything is written. `stage` verifies the copy it stages; `apply_staged` moves the previous artifact aside and restores it if the rename fails; `rollback` puts it back. A running executable is never replaced: applying is a separate step from staging. | `update.rs`, `tools/vector-tools/src/update.rs`, `COMMANDS.md` |
| What the end-to-end run found | The unit test used a manifest file name that matched the installed artifact name, so it passed. Against real files the first run applied the update to the *published* name (`vector-desktop-0.2.0.exe`) instead of the installed one and then could not roll back, having looked for the backup beside the installed file. Both are design errors -- the published file name and the installed file name are different things, and a rollback must be able to find what an apply displaced. `apply_staged` now takes the installed target, derives the backup from it, returns `None` for a first install, and `rollback` takes the same target. | `.agent/evidence/EP-009/update/e2e-transcript.md` |
| Proven refusals, not just a happy path | Against real files: honest update exit 0; untrusted signer exit 1 (naming both keys); edited manifest exit 1 (*"Verification equation was not satisfied"*); tampered artifact exit 1 (naming both digests); offer not newer exit 1; stage and apply exit 0 with the target holding the published bytes and the backup holding the old ones; rollback exit 0 with the target restored and the backup consumed. Five unit tests cover the same rules plus six separate edits to a signed manifest and the first-install case, and the mutation `update-signature-not-checked` is caught. | `.agent/evidence/EP-009/update/`, `.agent/evidence/EP-009/ROUND-32-REPORT.md` |

`REQ-037` moves from `PARTIAL_DIGEST_VERIFY_AND_TAMPER_DETECTION_DONE_SIGNATURE_PENDING` to
`DONE_SIGNATURE_HASH_ATOMIC_STAGE_ROLLBACK_CERTIFICATE_EXTERNAL_GATE`; open requirements fall from
6 to **5**. Deliberately not implemented and named: downloading (the manifest carries the digest and
size; fetching is the operator's step, and the crate takes no network dependency) and the
certificate (`REQ-036`'s external gate).

DoD unchanged at 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED; 21 gates exit 0; **44 mutations
caught**; artifact digest `6eaa3027…`, epoch 22. Corpus unchanged at 6,961 active items; pack
`core-asvab v6` active; verdict `CONDITIONAL_EXTERNAL_GATES`.

Remaining in-repo: REQ-032 (`gh` pull-request lane) and REQ-014 (local GGUF model).

### Round 33

| Area | What it delivered | Evidence |
|---|---|---|
| The pull-request lane (REQ-032) | `crates/vector-platform/src/gh.rs`: the official `gh` client invoked through the platform's shell-free `ProcessSpec` (an argument vector, never a shell), an approver required and refused before any process runs (`None`, `""` and `"   "` are all `NotApproved`), and a command builder that can only express `pr create` and `pr view` -- a test asserts no command it can build contains `merge`, `close` or `delete`. The child environment comes from the platform's allowlist, so `GH_TOKEN`/`GITHUB_TOKEN` never reach `gh`: the lane runs on the session `gh auth login` established and cannot authenticate by itself. | `gh.rs`, `tools/vector-tools/src/main.rs`, `COMMANDS.md` |
| A defect the first real run found | The lane failed with *"To get started with GitHub CLI, please run: gh auth login"* because the platform allowlist carries `HOME` and `USERPROFILE` but not `APPDATA`, which is where `gh` keeps its session on Windows. The lane now forwards the configuration-location variables (`APPDATA`, `LOCALAPPDATA`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`) explicitly rather than widening the allowlist for every child process, and a test pins that they are forwarded while the token variables are not. | `gh.rs`, `.agent/evidence/EP-009/gh-lane/open.json` |
| Driven against the real repository | `repair gh-open` opened **pull request 22** on `dominator509/project-vector-asvab` (exit 0, `"merged": false` in the lane's own report); `gh pr view 22 --json …` read it back independently as `OPEN`, `mergedAt: null`; `gh pr diff 22 --name-only` showed four files; `gh pr close 22` closed it unmerged and the branch was deleted (`gh api …/branches/…` → 404). The default branch is unchanged by any of it. | `.agent/evidence/EP-009/gh-lane/`, `ROUND-33-REPORT.md` |

`REQ-032` moves to `DONE_OFFICIAL_GH_LANE_EXPLICIT_APPROVAL_NO_AUTO_MERGE`; open requirements fall
from 5 to **4** -- `REQ-036`, `REQ-038` and `REQ-060` (external) and `REQ-014` (a local GGUF model,
which is work here). DoD unchanged at 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED; 21 gates exit 0;
**46 mutations caught**; verdict `CONDITIONAL_EXTERNAL_GATES`.

Recorded rather than chased: GitHub's Dependabot count on the default branch is now **2 moderate**
(from 17 before round 31's toolchain upgrade) while `pnpm audit` and `cargo deny check` both report
nothing. The two views disagree about the same tree, and the difference is worth a look before the
next release rather than being assumed away.

Remaining in-repo: REQ-014 (local GGUF model).


