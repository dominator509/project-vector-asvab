# Study corpus round 23: four more shapes read, one source refused, and the count that went down

Node: EP-007. Requirements touched: REQ-022, REQ-023, REQ-048, REQ-056.

## What this round adds

**Four more shapes the manuals state the relation in.** `scripts/probes/relation-shapes.py`
counts every way the Shop and Auto sources state "this is what the thing is for", including the
ones the reader cannot see. Beside the shapes already read, the sources write `is employed to`
52 times, `is made to` 43, `is adapted to` 11 and `is arranged to` 13 -- all with a bare verb
after the phrase, which is the form the question frame needs. Reading `employed`, `utilized`,
`adapted` and `arranged` produced real items: *diagonal pliers are adapted to cutting small
objects flush with a surface* from *Tools and Their Uses*, *a tractor is arranged to pull its
load in two different ways* from Rathbun's engine handbook, *the series-multiple system is
adapted for use with multiple cylinder engines*.

`is made to` is deliberately **not** read despite its count: the manuals also write `A provision
usually is made to install a fuel gage`, whose subject is a provision rather than a tool, and
the reader has no noun test that separates them. A shape that produces "Which tool is used to
install a fuel gage? -- a provision" is worse than an unread one.

**Three precision rules the new shapes made necessary.** Reading Rose's treatise prose produced
options no learner can pick, and each became a rule:

* a **measurement** cannot head a name -- `softened sheet copper about 1/32 inch thick is used to
  make joints` named `about inch thick`;
* a **reference back into the paragraph** is not a name -- `the same device`, `in connection`,
  `third class`;
* a **figure's label** is not part of a sentence -- `the tool e would cut the [V]-shaped groove
  i` is a description of a drawing, and a standalone lower-case letter is how it says so.

`work`, `works`, `substance`, `class` and `tool` join the head nouns that mean a name describes
something other than an object, because a treatise writes `the work is designed to form a
complete manual of reference` and means itself.

## The source this round refused

*Modern Machine-Shop Practice* (Rose, Project Gutenberg #39225, 4.9 MB) was fetched, measured and
**refused**. It yields 21 descriptions, 16 became items, and after every precision rule above
four of the survivors are still wrong: `in connection`, `in rods`, `tension` and `Rubber joints`
as the correct answer to "Which tool is used to ...?", because the miner reads mid-paragraph
clauses (`... and are usually made from what is known as combination rubber`, `... they are
fitted to the ash-pits`). That is lower precision than the seven manuals refused in round 18 and
the ten refused in round 20, and a source refused at higher precision cannot be kept at this
one.

**The Shop Information bank therefore goes from 55 to 42**, and that is the round's honest
headline: 16 mixed items out, 3 good ones in. The three earlier rounds that refused sources all
said the same thing, and this round is the first that had to apply it to a source already in the
corpus.

## The local vein that was measured and refused

Webster's 1913 defines 1,463 things by what they are for (`An instrument for measuring ...`),
which looked like a large Shop Information source already sitting in the repository. Measured,
it is not: the useful subset -- entries whose headword appears in the manuals' own vocabulary
*and* whose definition opens with a tool-class noun -- is 49 entries, and even those are
contaminated by etymology (`brake ... an instrument for breaking flax` is a line about the word's
German origin, not about brakes) and by arcana (`odontograph`, `mangle`, `manometer`). Building
items from it would have meant shipping statements quoted from etymology as claims about tools.
The probe and the measurement are recorded; the vein is refused.

## Corpus

| Subtest | Round 22 | Round 23 | Sources |
|---|---|---|---|
| WK | 1,949 | 1,949 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 1,565 | 1,565 | NEETS glossaries, all 24 modules |
| PC | 1,676 | 1,676 | 25 public-domain works, passage quoted |
| GS | 301 | 301 | *The book of wonders* |
| SI | 55 | **42** | *Tools and Their Uses*, *TM 11-453*, *Farm Mechanics* |
| AI | 75 | **77** | *TM 9-8000*, *TM 9-2700*, Rathbun's engine handbook |
| **Total** | **5,621** | **5,610** | |

Pack `core-asvab` was rebuilt and reinstalled at **v4** (5,610 items, 55 sources, 11.5 MB,
content hash `sha256:844832a6…`), because a corpus rebuild changes item identities and leaves the
previous pack's membership describing items that no longer exist.

## Gates

`sh scripts/verify.sh` exits 0. All 19 gates recorded exit 0 in
`.agent/verification/state/gate-results.jsonl`. Test accounting from that run
(`verify-round23.log`): **1,719 Rust tests across 109 binaries, 0 failed**; 211 frontend unit
tests and 9 more in the desktop suite; 36 end-to-end tests and 6 more in the packaged suite; the
packaged desktop live-fire passed against artifact digest
`7453448f971c210183e087b79726f93fee45622ec954d34a8801e1303fde45cf`.

Release verdict unchanged at `CONDITIONAL_EXTERNAL_GATES`: 42 DoD clauses, 27 PASS, 11 PARTIAL,
2 PENDING, 1 EXTERNAL_REQUIRED, 1 DEFERRED_LONG_RUNNING, 0 FAIL.

`python3 scripts/probes/mutation-round18.py` exits 0 with **all 29 mutations caught**, including
this round's three: the measurement-head rule, the figure-label rule, and the shape list.

## What is still not done

- **SI 42 and AI 77 remain the thin banks.** This round *lowered* SI to raise its precision, and
  the sources that would raise both -- systematic pre-1929 trade manuals -- have been measured:
  Rose is refused, the other Gutenberg mechanics books yield 0-6 descriptions each, and the
  microfiche NAVEDTRA manuals stay refused. The remaining lever is the noun-complement family
  (`serves as` 94, `acts as` 139, `is provided with` 87), which needs a third question frame
  ("Which tool serves as <noun>?") and a link field on the purpose.
- **One known bad item** survived every rule: *Tools and Their Uses* writes `withstand
  considerable twisting force in proportion to ita size` (a scanner misreading of `its`), and the
  subject is the steel shank rather than the tool. Catching it needs a misreading test on short
  tokens, which the probes measured as too blunt to apply to prose; it is recorded the way the
  two rare Word Knowledge distractors are.
- General Science is one work; the plan orders by prerequisite but does not track mastery per
  objective; the curriculum and calibration are visible in the content manager but not yet used
  by the study planner's own reasoning.
