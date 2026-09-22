# Study corpus round 14: General Science, Mechanical Comprehension, and what the other subtests still lack

Node: EP-007 (the content item store). Requirements touched: REQ-020, REQ-022,
REQ-023, REQ-048, REQ-056.

## What this round adds

1. **Mechanical Comprehension, generated rather than quoted.** Eight templates in
   `crates/vector-questions/src/factory.rs`: lever effort, gear ratio, block and
   tackle, wheel and axle, piston pressure, hydraulic jack, inclined plane, and
   mechanical advantage. Each answer is arithmetic over the machine's own geometry,
   so each carries an executable proof of the same kind an Arithmetic Reasoning item
   carries. `verify` recomputes the expression and refuses a disagreement.

2. **General Science from a question-and-answer work.** A new module,
   `crates/vector-questions/src/facts.rs`, reads a public-domain science book's own
   questions and answers. The question is the stem, the answer's first sentence is
   the correct option quoted verbatim, and the distractors are the same book's
   answers to other questions — each one a true statement about something else.
   `content ingest-facts` wires it through the pipeline, the evidence vault and the
   command boundary.

3. **Mechanical Comprehension is reachable in the interface.** It joins Arithmetic
   Reasoning and Mathematics Knowledge as a subtest the practice surface offers
   because the factory can serve it.

## Corpus after this round

Command: `python3 scripts/probes/count-items.py "<db>"`, full readback in
`check-corpus.log`.

| Subtest | Items | Source of items |
|---|---|---|
| WK | 2,000 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 3,668 | NEETS module glossaries |
| PC | 1,047 | 17 public-domain works, passage quoted |
| GS | 301 | *The book of wonders*, questions and answers |
| AR / MK / MC | generated on demand | computable templates with executable proofs |
| **Total stored** | **7,016** | |

29 evidence records, 9,016 citations, 28,064 review rows. Every provenance invariant
the probe checks is zero-offender, including the three added for GS this round: every
stem ends in a question mark, no GS item claims a passage it does not need, and no
option is shorter than five words.

Database: `C:\Users\domin\AppData\Roaming\com.vector.app\vector.db`, 26,767,360 bytes,
SHA-256 `745803ba30c331345c78920614e81573e063bdde890a7ccb4e13db0cabddbb73`.

`ingest-facts` report: `ingest-facts-gs.json`, rendered by
`python3 scripts/probes/facts-report.py`. *The book of wonders* asks 450 questions,
301 of which have an answer in the five-to-thirty word band the options need;
0 refused.

## Mechanical Comprehension reach

`cargo test -p vector-questions --test factory` now runs every generation property
over MC as well as AR and MK: 20 tests. `factory::generate_many("MC", 200, seed)`
returns 200 items, all distinct, and all eight templates are exercised. Sample items
are printed by `cargo run -p vector-questions --example mc_probe`.

The factory's properties are the evidence that matters here: every option is a
positive integer, four options are distinct, each distractor carries a named
misconception, the same seed reproduces the same item, and `verify` recomputes the
proof. The `mc_reach` measurement was a scratch example and has been removed; the
same properties are asserted by the test suite.

## What this round did not achieve

**Shop Information and Auto Information have no items.** This is the honest state,
and the reconnaissance behind it is recorded so the next round does not repeat it:

| Source | Shape found | Yield |
|---|---|---|
| *Tools and Their Uses* (NAVEDTRA, 1971) | 30 sentences of the form `X is used to <purpose>` | ~30 SI items, not yet mined |
| *Construction Mechanic 1 & C* (NAVEDTRA) | 66 review questions, most referring to figures in the manual | unusable without the figures |
| *Basic Machines* (NAVEDTRA 14037) | 70 review questions of the same kind | unusable without the figures |
| *Motor-car principles*, *Tractor principles*, *Farm engines*, *Mechanical devices in the home* (Project Gutenberg) | essayistic explanatory prose | 0–2 structured statements per work |
| ERIC vocational curricula | richer question sets | **not used**: their copyright status is not clearly federal, and the corpus admits only works whose terms are established |

Definition mining was tried and measured before being abandoned: a sentence-initial
`<term> is a <class> that …` rule yields 2–5 usable definitions per manual, and
corroborating them against Webster's entry for the term rejected good pairs
(`mallet`, `shank`) while accepting bad ones (`what`, `he`). The measurements are in
`definition-rule-probe.py`, `class-noun-probe.py`, `corroboration-probe.py` and
`purpose-probe.py`. A tool-oriented miner over the 30 purpose sentences is the
cheapest next step for SI; AI needs a source with structure that has not been found
yet.

**Content packs are still unpopulated.** `content_packs` has publish, activate and
rollback and nothing writes to it. Every item this round created is `active` in the
store with per-item provenance, which is what REQ-056 requires, but the pack surface
the objective names (install/quarantine) has only its quarantine half exercised.

**The 1910s science book is a period source.** Its answers are period answers. The
item's citation names the work and the URL resolves to the bytes, so a reviewer can
see where a claim comes from, but nothing in the pipeline can establish that a 1913
popular-science explanation is current. That limitation is stated in the module's
own documentation rather than left for a reader to discover.

## Gate results

Logs and exit codes: `gates/`.

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

Counts: 843 Rust tests across 68 binaries, 0 failed; 205 frontend unit tests; 34 E2E
tests. New tests this round: `facts` 17, `fact_ingestion` 8, plus MC coverage folded
into the factory's existing generation properties and one command-boundary test.
