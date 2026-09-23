# Study corpus round 20: more sources for the thin banks, and the curriculum a pack declares

Node: EP-007. Requirements touched: REQ-022, REQ-023, REQ-032, REQ-048, REQ-056.

## What this round adds

**More sources for the two thin banks.** Shop Information goes from 37 to **55** and Auto
Information from 70 to **75**, from a vein the corpus had not tried: pre-1929 technical books
on Project Gutenberg, which are public domain and write the relation this reader reads. Modern
Machine-Shop Practice (Rose), Farm Mechanics (Shearer), How it Works (Williams), Elementary
Lathe Practice, The Economy of Workshop Manipulation, Precision Locating and Dividing Methods,
Practical Hand Book of Gas, Oil and Steam Engines (Rathbun), Gas-Engines and Producer-Gas
Plants (Mathot), Gas and Petroleum Engines (de Graffigny), The Romance of Modern Mechanism,
The Romance of Modern Invention, The Boy's Book of New Inventions and Machines at Work were
fetched and measured; the ones that yield descriptions became sources, and the ones that do
not are recorded as measured-and-unused rather than shipped to make a count.

**A citation that would have been wrong.** `content ingest-tools` cited every source as
`https://archive.org/details/<id>`. Ingesting a Project Gutenberg work through that form would
have recorded a URL that does not resolve to the bytes -- the same class of defect as the ten
items that cited #2009 for the file that is #1228. The work argument is now
`<source>:<id>:<title>=<path>` with `archive` or `gutenberg`, and the source kind decides the
page the citation names. It is written down rather than guessed from the id's shape, because
an all-digit id is a Gutenberg ebook number *and* a plausible Internet Archive identifier.

**The curriculum graph and the calibration metadata.** `CONTENT_PACK_SPEC.md` has listed both
since the pack format was written, and rounds 15 to 19 recorded them as still missing. A pack
now carries:

* `curriculum`: nodes of `objective_id`, `subtest`, `title` and `prerequisites`;
* `calibration`: entries of `objective_id`, `expected_correct`, `responses` and `basis`.

Installation verifies three relations that a pack full of real questions can still get wrong:
every item's objective is declared *and* the node's subtest matches the item's; every
prerequisite names a node, and the relation is acyclic (a cycle means no learning order
satisfies the curriculum); and every objective with items carries exactly one calibration
entry in range. Building without either declares them: one node per objective the items teach,
and a calibration derived from the items' own difficulty scale with `responses: 0`, which is
the field that says plainly the number is a declaration rather than a measurement.

`scripts/probes/build-curriculum.py` writes the corpus's own: six objectives over six subtests,
with the prerequisites the corpus can defend (shop knowledge before automotive) and a
calibration entry per objective that names how many items it rests on and that no learner has
answered any of them. A pack carrying both was built and installed: **5,621 items, 56 sources,
11.5 MB, `signature_valid: true`**.

## The defect this round found: the third instance of one bad source ending a run

Adding *Elementary Lathe Practice* to the Shop Information sources **failed the run**. The
pipeline refuses a source it finds no tools in -- correctly, because a caller who asked for one
manual and got nothing must be told -- and `ingest_tools` propagated that refusal with `?`,
exactly as the Electronics Information loop did before round 19 fixed it. The Shop Information
run aborted after storing **49 items**, and the Auto Information subtest was never ingested at
all: one source that contributes nothing took out both banks.

A source that contributes nothing is now recorded as `skipped` with its reason and the run
continues; a run in which no source produced an item still fails. That is the third path with
this shape (Paragraph Comprehension had it from round 15, Electronics Information from round
19), and the fourth one -- Word Knowledge, where the thesaurus is a single file -- cannot have
it. The pattern is recorded because it is the same mistake in three places: a per-source guard
written for a one-source caller, propagated by a loop that was built to be resilient.

## Corpus

| Subtest | Round 19 | Round 20 | Sources |
|---|---|---|---|
| WK | 1,949 | 1,949 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 1,565 | 1,565 | NEETS glossaries, all 24 modules |
| PC | 1,676 | 1,676 | 25 public-domain works, passage quoted |
| GS | 301 | 301 | *The book of wonders* |
| SI | 37 | **55** | *Tools and Their Uses*, *TM 11-453*, *Modern Machine-Shop Practice*, *Farm Mechanics*, *How it Works* |
| AI | 70 | **75** | *TM 9-8000*, *TM 9-2700*, and four engine manuals |
| **Total** | **5,598** | **5,621** | |

69 evidence records, 7,570 citations. Every provenance invariant is a zero: one source cited
per item (two for Word Knowledge), no duplicate content hashes, no question asked twice within
a subtest, no item with scan damage, no PC item without a passage, and no option that is not the
shape its subtest asks for. `25/25 work(s) re-download to the bytes they are cited as`.

## Gates

`sh scripts/verify.sh` exits 0. All 19 gates recorded exit 0 in
`.agent/verification/state/gate-results.jsonl`. Test accounting from that run
(`verify-round20.log`): **1,713 Rust tests across 109 binaries, 0 failed**; 210 frontend unit
tests and 9 more in the desktop suite; 36 end-to-end tests and 6 more in the packaged suite;
the packaged desktop live-fire passed against artifact digest
`0f2471966669c90f501d5ab6a76afe4a8878a8168efb2bd81628602cca88332f`.

Release verdict unchanged at `CONDITIONAL_EXTERNAL_GATES`: 42 DoD clauses, 27 PASS, 11 PARTIAL,
2 PENDING, 1 EXTERNAL_REQUIRED, 1 DEFERRED_LONG_RUNNING, 0 FAIL.

The corpus probes all exit 0 (`count-items.log`, `check-corpus.log`, `check-options.log`,
`foreign-letters.log`, `gutenberg-provenance.log`).

`python3 scripts/probes/mutation-round18.py` exits 0 with **all 24 mutations caught**: the
nineteen content-reading rules of round 18, round 19's two run-level rules and its named
relation, this round's per-source tolerance and source-kind citation, and the two pack rules
the curriculum and calibration rest on.

## What is still not done

- **SI 55 and AI 75 are still thin** for a subtest that asks sixteen questions in a sitting.
  Every source measured across three rounds is either in the corpus or refused with numbers,
  and what is left for these two banks is breadth of source -- pre-1929 trade manuals that
  describe tools systematically -- rather than a looser filter.
- **The curriculum and calibration are not yet visible in the product.** They travel in the
  pack and the installer verifies them; the content manager does not show what a pack teaches
  or how its objectives are calibrated, and the study planner does not order work by the
  prerequisites the graph now carries. That is the next round's natural work.
- **General Science is one work** (301 questions): only *The book of wonders* in the corpus has
  the question-and-answer shape `ingest-facts` reads.
- The noun-head check for the residual generic names, the two rare Word Knowledge distractors,
  and the calibration of anything by real responses (there are no learners yet) are unchanged.
