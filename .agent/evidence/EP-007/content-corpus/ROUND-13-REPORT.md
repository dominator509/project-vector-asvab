# Study corpus round 13: Paragraph Comprehension, and the passage the store was missing

Node: EP-007 (the content item store). Requirements touched: REQ-020, REQ-022,
REQ-023, REQ-048, REQ-053, REQ-056.

This file records what was run and what was observed. It is written after the work,
from the commands' own output, and it names the defects the work uncovered rather
than only the results.

## What this round adds

1. **Paragraph Comprehension ingestion.** 17 public-domain works, 1,047 active
   items, each citing the work it quotes and carrying the passage it is about.
2. **The passage column.** `migrations/005_content_passage.sql`. Before it, a PC
   item could be stored and served with no passage at all: the question asks about
   a text and the text had nowhere to live. The store now refuses that state.
3. **Reachability.** The practice surface offered only the two generatable
   subtests, so the ingested corpus could not be practised at all. It now offers
   every subtest the device can serve, and never offers to generate one the factory
   cannot produce.

## The defects this round found

| Found by | Defect | Fix |
|---|---|---|
| `Text::contains_passage` refusing 3 items | The passage builder split a paragraph into sentences and rejoined them with a space. Where the split was not at a space, the learner was shown text the work does not contain: `the cap of liberty.” An ancient` came back `night? ” “Where`, and `Penna. R.R.` came back `Penna. R. R.`. | Passages are cut from the paragraph by character span (`sentence_spans`), so they are the source's own text by construction. 0 of 4,000+ built items now fail containment. |
| The same check | Sentence terminators inside initials and abbreviations were treated as boundaries (`R.R.` split into `R.`/`R.`). | A terminator must be followed by whitespace (possibly after a closing mark), the token before it must not be a known abbreviation or a single letter. |
| Corpus scan | An index of illustrations was built into an item as a "passage" about air locks, with other index entries as its options. | `parse_gutenberg` drops a paragraph that refers to illustrations three or more times. |
| `verify-gutenberg-provenance.py` | `darwin-origin.txt` cites Project Gutenberg **#2009**, which serves a *different edition* of the same book. The items' provenance claim was false while looking entirely plausible. | The correct number is **#1228**. The 13 affected items were deleted and re-ingested; all 17 works now re-download to the bytes they are cited as. |
| The E2E suite | 4 tests had been failing since the corpus replaced the sample literals: they asserted option text from `sample.ts`, which the view no longer serves. | Proven pre-existing by running the same specs at `1ae9dce` in a worktree. Fixed by giving the specs a shared stub of the Tauri bridge, and by reading the correct option and item id from the page instead of hard-coding them. |

The first and fourth are the kind that matter most: both produced items that looked
authoritative and were wrong in a way only an independent check could see.

## Corpus read back from the store

Command: `python3 scripts/probes/check-corpus.py "<db>"`

```
=== corpus ===
  EI   active     3668
  PC   active     1047
  WK   active     2000
  evidence_records       29
  content_item_sources   8715
  content_item_reviews   26860

=== provenance invariants over the whole corpus ===
  WK active items not citing exactly 2 source(s): 0
  EI active items not citing exactly 1 source(s): 0
  PC active items not citing exactly 1 source(s): 0
  items whose generator and verifier hashes agree  0
  active items with no reviewer                    0
  active items whose proof is not source_backed    0
  duplicate content hashes                         0
  items whose correct_index misses its options     0
  active items with no objective                   0
  PC items with no passage                         0
  PC items whose passage is under 40 words         0
  PC items whose correct option is not in the passage 0
```

Database: `C:\Users\domin\AppData\Roaming\com.vector.app\vector.db`
Size 26,099,712 bytes, SHA-256
`9beaf1c2731936617eca25f70e5790808b294d6665d353a139f3b02b75801376`.

## Provenance of the sources

Command:
`python3 scripts/probes/verify-gutenberg-provenance.py --manifest .agent/evidence/EP-007/content-corpus/gutenberg-manifest.txt`

Result: `17/17 work(s) re-download to the bytes they are cited as`.

Each work was re-fetched from `https://www.gutenberg.org/cache/epub/<id>/pg<id>.txt`
and its SHA-256 compared with the local file. Full log:
`gutenberg-provenance.log`.

Ingestion reports, per work and per NEETS module, with the seed and the elapsed
time: `ingest-wk.json`, `ingest-ei.json`, `ingest-pc.json`, `ingest-pc-darwin.json`.
The 17-work scan of the paragraph builder, before ingestion, is
`pc-corpus-scan.txt`.

## Gate results

Run after all changes, with `CI=1 NO_COLOR=1`. Logs and exit codes:
`.agent/evidence/EP-007/content-corpus/gates/`.

| Gate | Command | Exit |
|---|---|---|
| Format check | `sh scripts/format-check.sh` | 0 |
| Lint | `sh scripts/lint.sh` | 0 |
| Typecheck | `sh scripts/typecheck.sh` | 0 |
| Unit tests | `sh scripts/test-unit.sh` | 0 |
| Integration tests | `sh scripts/test-integration.sh` | 0 |
| E2E tests | `sh scripts/test-e2e.sh` | 0 |
| Generated-pack shape | `python3 scripts/validate-generated-pack.py .` | 0 |
| Anti-gaming scan | `python3 scripts/anti-gaming-scan.py .` | 0 |
| Packaged build | `sh scripts/build.sh` | 0 |
| Packaged live-fire | `python3 scripts/desktop-live-fire.py --report .agent/evidence/EP-001/desktop-live-fire.json` | 0 |

Counts: 817 Rust tests passed across 66 test binaries, 0 failed; 205 frontend unit
tests passed; 34 E2E tests passed. The packaged live-fire launched
`target/release/vector-desktop.exe`, observed the webview mount, and read the
`ui_ready` marker back out of the database in a separate process (markers 20 → 21,
`"verdict": "pass"`).

Mutation proofs: `mutation-proofs.md` in this directory.

## What is still not right

- **Word Knowledge distractor plausibility.** Moby's associations are not all
  synonyms, and the dictionary filter reduces the rate rather than eliminating it:
  `unendowed` and `becharm` still reach items. Closing this needs a word-frequency
  source so rare headwords can be excluded. Recorded, not fixed.
- **`Pride and Prejudice` contributes nothing.** Its sentences are longer than
  `MAX_CLAUSE_WORDS` (26), so no paragraph fits both the passage band and the option
  band. Reported by the ingester as skipped with that reason rather than silently.
- **PC items are detail-location, not inference.** Every item asks which statement
  the passage makes, and the distractors alter one quantity or name. That is a real
  Paragraph Comprehension skill and it is honestly labelled, but it is narrower than
  the subtest, which also asks what a passage implies.
- **Federal sources for GS, MC, AI and SI are not ingested.** The DOE and AEC works
  used here for PC are government publications and will serve those subtests too,
  but the subtests themselves have no items yet.
