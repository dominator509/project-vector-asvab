# Study corpus round 24–25: the relation's third form, and a mutation that outlived its run

Node: EP-007. Requirements touched: REQ-022, REQ-023, REQ-048, REQ-056.

## What these rounds add

**The noun-complement form.** The manuals state the relation three ways, and the reader knew
two. `scripts/probes/relation-shapes.py` counts the third: `serves as` 94 times, `acts as` 139
and `is used as` 33 across the Shop and Auto sources -- more than any other single phrase --
and none of it was readable, because those complements are *nouns*:

```
The bimetallic strip serves as one of the contact points.
The throttle return dashpot serves as a damper to keep the throttle from closing too quickly.
A one-way valve acts as a check against return flow.
```

A purpose now carries the form it was stated in (`Link::To`, `Link::For`, `Link::As`), and the
question follows it: `What does the bimetallic strip serve as?` with the complement as the
answer, rather than "Which tool is used to one of the contact points?". **Auto Information goes
from 77 to 94 items**, seventeen of them asked in the new frame.

Three things make that honest rather than merely larger:

* **the frame is the source's.** A `serves as` description is *not* asked as a Shop Information
  question -- "Which tool serves as a switch?" would offer a component as a tool, which is
  exactly the defect round 23 refused a source for. Those descriptions are asked the other way
  round, where they are exactly right.
* **the options match the question.** An item whose prompt says `serve as` takes noun-phrase
  options and one whose prompt says `used for` takes infinitives, and the verifier refuses an
  item that mixes them: offering "a thrust bearing" beside "to prevent leakage" tells the learner
  which option is the odd one out. This is round 18's parallelism rule, now applied to the link.
* **a purpose holds no semicolon.** `the pressure valve also serves as a safety valve to relieve
  extra pressure within the system; the vacuum valve opens only when the pressure drops` is two
  sentences, and the first half is the item while the second is the next one. A semicolon is
  always two clauses, so it is refused on both paths.

## The defect these rounds found: a mutation that outlived its run

The mutation harness disables one rule at a time and restores the file in a `finally` block. A
run that is **killed** -- by a timeout, by an interrupt, by the harness ending the turn -- never
reaches that block, and the mutation it was applying stays in the source. That happened here:
two `if false &&` guards sat in `purposes.rs` across a round boundary, and they announced
themselves only as two tests failing for reasons that made no sense (`Both hands` accepted as a
tool name, `same device` accepted as a name) while the rule they disabled looked present in the
file.

The harness now refuses to run when a mutation's replacement is present *instead of* the text it
was applied to:

```
a previous run left a mutation applied; restore the source before mutating:
  purposes.rs: if false
```

The distinction matters and the first version of the guard got it wrong: several mutations delete
one line from a pair, so the replacement is a *substring* of the text they were applied to, and a
guard that only looks for the replacement fires on a clean tree.

## Corpus

| Subtest | Round 23 | Round 25 | Sources |
|---|---|---|---|
| WK | 1,949 | 1,949 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 1,565 | 1,565 | NEETS glossaries, all 24 modules |
| PC | 1,676 | 1,676 | 25 public-domain works, passage quoted |
| GS | 301 | 301 | *The book of wonders* |
| SI | 42 | 42 | *Tools and Their Uses*, *TM 11-453*, *Farm Mechanics* |
| AI | 77 | **94** | *TM 9-8000*, *TM 9-2700*, Rathbun's engine handbook |
| **Total** | **5,610** | **5,627** | |

Pack `core-asvab` rebuilt and reinstalled at **v5** (5,627 items, 55 sources, 11.5 MB).

`check-corpus.py` gained no invariant this round, but two of its existing ones were **wrong for
the new frame** and were corrected rather than left reporting the corpus as broken: an Auto
Information stem is now either `What ... used for?` or `What ... serve as?`, and the option rule
checks that the options are in the form *the question asks in* rather than that they are
infinitives. That is the third time a probe has been the thing at fault (the capitals rule of
round 18, the Word Knowledge source count of round 13), and the pattern is worth stating: a
probe encodes the corpus as it was when it was written.

## Gates

`sh scripts/verify.sh` exits 0. All 19 gates recorded exit 0 in
`.agent/verification/state/gate-results.jsonl`. Test accounting from that run
(`verify-round25.log`): **1,721 Rust tests across 109 binaries, 0 failed**; 211 frontend unit
tests and 9 more in the desktop suite; 36 end-to-end tests and 6 more in the packaged suite; the
packaged desktop live-fire passed against artifact digest
`028bb9240678de2a2cf2a069bb03ea4a6dfe8f99883c1d629649d078a4c793f9`.

Release verdict unchanged at `CONDITIONAL_EXTERNAL_GATES`: 42 DoD clauses, 27 PASS, 11 PARTIAL,
2 PENDING, 1 EXTERNAL_REQUIRED, 1 DEFERRED_LONG_RUNNING, 0 FAIL.

`python3 scripts/probes/mutation-round18.py` exits 0 with **all 32 mutations caught**, including
this round's three: the noun frame, the option-form check and the semicolon rule.

## What is still not done

- **SI 42 remains the thin bank.** The noun frame did not help it, by design: the descriptions it
  reads are about components, and a Shop Information item asks which tool does something.
  Sources that would raise SI have been measured and refused across four rounds.
- **AI's new frame has its own noise floor.** `What does the Slack serve as?` and `What do the
  Hangers frame members serve as?` reached the bank: a property and a heading run-in, both with
  plausible-looking complements. They are recorded as known, the way the steel-shank item is.
- **The plan orders by prerequisite but does not track mastery per objective**, General Science is
  still one work, and the Word Knowledge rare-distractor gap is unchanged.
