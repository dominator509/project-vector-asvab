# Study corpus round 19: more than three times the Electronics Information bank, and half again the corpus

Node: EP-007. Requirements touched: REQ-022, REQ-023, REQ-048, REQ-056.

## What this round adds

**The rest of NEETS.** The Electronics Information bank was built from ten of the Navy
Electricity and Electronics Training Series' twenty-four modules, because the ten were what an
earlier round had downloaded. The whole collection is one Internet Archive item with
twenty-four text files, and `scripts/probes/fetch-archive-text.py --list` now lists them: the
other fourteen are downloaded, and the bank goes from **414 to 1,565 questions**.

**Eight more works for Paragraph Comprehension.** The manifest holds seventeen Project
Gutenberg works; eight more technical and natural-history works were added -- Agricola's *De Re
Metallica*, Whewell's *History of the Inductive Sciences*, Candolle's *Origin of Cultivated
Plants*, Thorndike's *A History of Magic and Experimental Science*, two volumes of Kirby and
Spence's *Introduction to Entomology*, and two volumes of Pliny's *Natural History* -- taking
the bank from **1,050 to 1,676 questions**. Their ebook numbers are read out of each file's own
Gutenberg header by the new `scripts/probes/fetch-gutenberg.py`, which refuses a download whose
header disagrees with the number it was asked for. That check exists because of round 18's
finding: ten items spent two rounds citing #2009 for a file that is #1228.

**The relation these manuals name instead of stating.** `The purpose of the piston skirt is to
keep the piston from rocking in the cylinder` puts the subject *after* the phrase, so reading
what precedes the verb names `purpose`. `scripts/probes/description-shapes.py` counted the
shape before it was read: nineteen sentences in TM 9-8000, and it is the form the older
manuals prefer. Reading `function of X is to Y` and `purpose of X is to Y` takes the two
automotive manuals from 78 descriptions to 97 and the Auto Information bank from **59 to 70
questions** -- `hair spring`, `engine lubrication`, `piston skirt`, `air-over-hydraulic
suspension system` among them.

## The defect this round found: a thin module could end the whole run

Ingesting the twenty-four modules in one command **failed**, and the failure is the interesting
part. The pipeline refuses a module that builds no items -- correctly, because a caller who
asked for one module and got nothing must be told -- and the loop propagated that refusal with
`?`. Module 24's glossary holds eight entries and none survives the filters, so the run aborted
on the last module **after storing 1,152 items from the other twenty-three**, and never wrote
its report: the work was done and the evidence for it was lost.

The Paragraph Comprehension path had already learned this lesson in round 15 ("a work that
parses badly cannot take the rest down with it"), and the Electronics path carried the same
promise in its own doc comment while the loop broke it. A module that contributes nothing is
now recorded as `skipped` with its reason and the run continues; the run still fails when *no*
module contributed anything, which is the guard that keeps a broken scan from reporting a
successful empty ingestion. Both behaviours are tested, and both mutations are caught.

## The other defect: ingestion had become quadratic

The question check added in round 18 runs once per *built candidate*, and a builder produces
far more candidates than items: the Electronics path builds about 10,000 candidates per run to
find a few hundred questions. Without an index that predicate scanned `content_items` every
time, and the 24-module ingestion had not finished after ten minutes where the same work
without the check took about three.

`migrations/008_content_question_index.sql` adds `(subtest, LOWER(TRIM(stem)))` -- the
expression, not `stem`, because the comparison is case- and space-insensitive by design -- and
the application embeds it with the other seven migrations. It is a migration rather than a
setup step because the corpus on a user's machine needs it too.

## Corpus

| Subtest | Round 18 | Round 19 | Source |
|---|---|---|---|
| WK | 1,949 | 1,949 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 414 | **1,565** | NEETS module glossaries, all 24 modules |
| PC | 1,050 | **1,676** | 25 public-domain works, passage quoted |
| GS | 301 | 301 | *The book of wonders*, question and answer |
| SI | 36 | 37 | *Tools and Their Uses*, *TM 11-453 Shop Work* |
| AI | 59 | 70 | *TM 9-8000*, *TM 9-2700* |
| **Total** | **3,809** | **5,598** | |

55 evidence records, 7,547 citations. Every provenance invariant is a zero: one source cited
per item (two for Word Knowledge), no duplicate content hashes, **no question asked twice within
a subtest**, no item with scan damage in its stem or options, no PC item without a passage or
whose correct option is not in it, and no option that is not the shape its subtest asks for.
`25/25 work(s) re-download to the bytes they are cited as`.

## Sources surveyed and refused

Ten Army ordnance maintenance manuals were downloaded and measured for the Auto Information
shape and **refused**: *TM 9-1765B* (engine, power train, braking and steering), *TM 9-1828A*
(fuel pumps), *TM 9-1829A* (speedometers and tachometers), *TM 9-1729A* (engines, cooling and
fuel systems), *TM 9-1750* (power train unit), *TM 9-1765A* (axles and propeller shafts),
*TM 9-1727C* (Hydra-Matic transmission), *TM 9-1868* (tire repair) and *TM 9-1710C* (chassis and
body). Between them they hold **two** sentences of the shape this corpus builds items from.
They are repair *procedures* -- `Remove the four bolts`, `Install the gasket` -- and the
relation `X is used to Y` belongs to the principles manuals, not to them.

*Machinery's Reference* booklets 1 and 7 (already downloaded) were measured again and hold
three descriptions each. The ERIC microfiche manuals refused in round 18 stay refused.

## Tooling this round added

| Tool | What it answers |
|---|---|
| `scripts/probes/fetch-archive-text.py --list` | What an Internet Archive item holds, which is how the other fourteen NEETS modules were found. |
| `scripts/probes/fetch-gutenberg.py` | Downloads a Project Gutenberg work in the form the provenance check re-derives, and refuses it when the file's own header names a different ebook number. |
| `scripts/rebuild-corpus.py --only <subtests>` | Rebuilds one subtest, with the source order and the seeds fixed in the script rather than taken from the caller's shell. The order matters: the same 24 modules produced 1,566 questions under one shell's ordering and 1,508 under another's, because each module's seed is its index. |

## Gates

`sh scripts/verify.sh` exits 0. All 19 gates recorded exit 0 in
`.agent/verification/state/gate-results.jsonl`: generated-pack, anti-gaming-scan, format-check,
lint, typecheck, test-unit, test-collection-guard, test-integration, test-e2e, security-check,
dependency-audit, mcp-probe, provider-probe, build, artifact-identity, proof-matrix-stamp,
smoke-test, live-fire, release-state.

Test accounting from that run (`.agent/evidence/EP-007/content-corpus/verify-round19.log`):
**1,703 Rust tests across 109 binaries, 0 failed**; 210 frontend unit tests and 9 more in the
desktop suite; 36 end-to-end tests and 6 more in the packaged suite; the packaged desktop
live-fire passed against the artifact digest
`36c47ec7d18a55dfe11eef7dc975604a38dcdc8d4cfd65c523f37e07c4709c7e`.

Release verdict unchanged at `CONDITIONAL_EXTERNAL_GATES`: 42 DoD clauses, 27 PASS, 11 PARTIAL,
2 PENDING, 1 EXTERNAL_REQUIRED, 1 DEFERRED_LONG_RUNNING, 0 FAIL.

The corpus probes all exit 0 (`count-items.log`, `check-corpus.log`, `check-options.log`,
`foreign-letters.log`, `gutenberg-provenance.log`). `check-options.py` reports 0 options
containing scan damage and the two known rare Word Knowledge distractors (`unendowed`,
`becharm`), which it counts apart so the two are never confused for one another.

`python3 scripts/probes/mutation-round18.py` exits 0 with **all 20 mutations caught**.

## What is still not done

- **Shop and Auto Information remain thin** (37 and 70). Every source measured for them this
  round or last has been refused with numbers rather than shipped, and what is left is breadth
  of source rather than a looser filter.
- **General Science is one work** (301 questions). Only *The book of wonders* in the corpus has
  the question-and-answer shape `ingest-facts` reads; the seventeen other Gutenberg works hold
  0-2 questions each, and the eight added this round hold none. The next step is finding works
  of that shape rather than more prose.
- **A noun-head check** for the residual generic names (`system`, `transfer`,
  `cycle piston-type engine`) still needs Webster's part-of-speech markers exposed through
  `Dictionary`.
- The Word Knowledge rare-distractor gap (two options: `unendowed`, `becharm`) is unchanged,
  and the pack schema still lacks the curriculum graph and calibration metadata.
