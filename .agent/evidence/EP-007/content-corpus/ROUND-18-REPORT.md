# Study corpus round 18: a corpus of questions, and the three defects that were hiding in it

Node: EP-007. Requirements touched: REQ-022, REQ-023, REQ-048, REQ-056.

## What this round adds

**Auto Information.** The Shop Information reader from round 17 read one side of a manual's
description -- `Screw extractors are used to remove broken screws ...` became *which tool is
used to remove broken screws?* This round reads the other side: `What are the screw
extractors used for?`, with the functions of the manual's other components as the wrong
answers. Two Army manuals supply it, *TM 9-8000 Principles of Automotive Vehicles* (1985) and
*TM 9-2700*, both Internet Archive scans recorded in the vault with their own URL and digest.

**More of the relation.** The reader only knew `is used to` and `is used for`. The same
manuals state it as `is designed to`, `is intended to` and `serves to`, and each shape it
cannot read is content already in the corpus that never becomes an item.
`scripts/probes/description-shapes.py` counted them before any rule was written: in TM 9-8000
alone, beside 134 sentences written as `is used to`, the manual writes `is designed to` 66
times and `serves to` 23. Reading them took the two automotive manuals from 46 descriptions to
78. `serves as` is deliberately not read: its complement is a noun, and the frames this module
builds take a purpose clause.

**One question, asked once.** `ContentItemRepo::question_exists` identifies a question by what
a learner answers -- the stem, the correct answer, and the passage if there is one -- and every
ingest path now asks it before storing. That is what turned this round from "add content" into
"find out what the content actually was" (see below).

## Three defects, in the order they were found

### 1. A rule that let adjectives through

The miner refuses a purpose that does not start with a verb, and the first version of that rule
refused only what was *obviously* not one: a function word, an adverb, a hyphenated participle,
a noun from a hand-written list. Applied to the shop manuals it accepted

```
Six and eight point wrenches are used for heavy, for medium, and for light duty only.
```

and built **Which tool is used to heavy, for medium, and for light duty only?** -- an item with
no answer. English has no morphological test that separates `heavy` from `cut`, or the
attributive `cutting tools` from the gerund `cutting metal`, so the rule is now a list of the
verbs these manuals use. The list was not guessed: `scripts/probes/purpose-verbs.py` prints
every distinct word a purpose opens with across the sources, and the 205 entries are that
output, read.

Recall is what paid for the precision, and the measurement says the price was small: across
the four manuals the permissive rule yielded 115 descriptions and the list yields 103 of them,
with the twelve dropped being the ones with no verb.

### 2. A refused item left a row behind

Storing an item is a sequence -- insert a draft, cite the source, record the verifier's hash,
walk it to content-reviewed, activate it -- and the row exists from the first step. A failure
in the middle therefore left a draft in the store permanently. An ingestion naming a source the
vault had not recorded produced **six drafts**, and because a draft is a row, the new question
check read them as questions already asked: the run reported six items already present and
activated none, while its own rejection list showed every item refused.

Found by a test that insisted an unrecorded source must fail. Fixed with
`ContentItemRepo::discard_draft` (guarded to `state = 'draft'`, because an active or
quarantined item is a record somebody depends on) and `ContentPipeline::finish_item`, which
wraps the tail of all six store paths. The test now asserts the store is empty afterwards, not
merely that nothing is servable.

### 3. The Electronics Information bank was 89% the same question

The corpus was reported as 3,668 Electronics Information items. Ingesting it again under the
question check produced 3,668 items of which **3,259 were already asked**: the same NEETS
glossary definition, with the same correct term, and the same four options in a different
order. A learner practising Electronics Information would meet one definition up to seven
times and could reasonably conclude the bank was broken.

The bank is 414 questions, and that is the whole of what these glossaries state in a shape an
item can be built from. Asked for 3,000 items per module the builder produces about 10,000
candidates, and 414 of them are questions: a run with a different seed found five more than the
400-per-module run had, and a rebuild from an empty store produces all 414 and then stores
nothing new on a repeat.

The same rule removed 51 duplicate Word Knowledge questions and one Paragraph Comprehension
question, and it keeps the duplicates that are not duplicates: `Choose the word that most
nearly means the same as irenic` has more than one true answer, and the 59 Word Knowledge stems
with a *different* correct answer are 59 questions.

### 4. A citation that could not be re-checked

Ten Paragraph Comprehension items cited `https://www.gutenberg.org/ebooks/2009` for the text of
`sources/gutenberg/darwin-origin.txt`. The file is Project Gutenberg **#1228**; #2009 is a
different file of the same book. The provenance check passed anyway, because it re-downloads
the works named in the *manifest*, and the manifest had been corrected while the vault record
and the items citing it had not.

The stale record is deleted by `scripts/rebuild-corpus.py` -- a vault record whose URL does not
resolve to the bytes recorded for it is not evidence -- and the work is re-ingested from the
manifest, so the citation and the check now come from one file. 17/17 works re-download to the
bytes they are cited as.

## Corpus

| Subtest | Items | Source |
|---|---|---|
| WK | 1,949 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 414 | NEETS module glossaries (10 modules) |
| PC | 1,050 | 17 public-domain works, passage quoted |
| GS | 301 | *The book of wonders*, question and answer |
| SI | 36 | *Tools and Their Uses*, *TM 11-453 Shop Work* |
| AI | 59 | *TM 9-8000*, *TM 9-2700* |
| AR / MK / MC | generated on demand | computable templates with executable proofs |
| **Total stored** | **3,809** | |

**The total went down, from 7,111 to 3,809, and that is the round's most important number.** Of
the 7,111 items the corpus held, **3,305 were questions it had already asked** -- 3,254 of them
in Electronics Information and 51 in Word Knowledge -- and three were added where the corrected
Darwin citation produced items the wrong citation had refused. Every stored item is now a
distinct question, so 3,809 is the first honest count this corpus has had. A smaller corpus that
asks 3,809 different things is worth more to a learner than a larger one that asks 414 things
repeatedly.

33 evidence records, 5,758 citations, 15,236 review rows. Every provenance invariant is a
zero: one source cited per item (two for Word Knowledge, whose synonym pairs are corroborated
by the dictionary as well as the thesaurus), no duplicate content hashes, no question asked
twice within a subtest, no item with scan damage in its stem or options, no PC item without a
passage, and no option that is not the shape its subtest asks for.

## The rules this round added, and how each was proved

`scripts/probes/mutation-round18.py` disables one rule at a time in the source and requires the
test that is supposed to catch it to fail. **All 16 mutations are caught.** A rule enforced in
two places is disabled in both, because disabling one site and watching the test still pass
would say nothing about the rule.

| Rule | The learner-visible defect it refuses |
|---|---|
| closed verb list | `Which tool is used to heavy, for medium, and for light duty only?` |
| the connector follows the source's form | `Which tool is used to gripping, reaching places ...?` (the source wrote `for gripping`) |
| a purpose holds no full stop | `... cut this fuzz from the wood. "Sandpaper" consists of small particles ...` |
| the stop survives a closing quote | `... reinforce the piston-pin "bosses." the radial-engine piston varies only slightly.` |
| Latin letters only | `Oй filters`, `Tһe bulkhead receptacle`, `fluid pressure юг the lubrication system` |
| a class is not a tool | `the more permanent type`, `Whatever method`, `Both hands`, `catch trough then` |
| a possessive names a document | `automotive manufacturers' requirements` |
| scan damage | `be- tween`, `EVAPORATOR CORE CAPILLARY TUBE`, `of to candlepower`, `gallons liters)` |
| a name is not ended by its own first word | `Inside micrometers` truncated to nothing |
| Auto Information options are parallel | `to joining the towing vehicle and the trailer` |
| one question per stem and answer | 3,254 questions the corpus already asked |
| a refused item leaves no draft | six drafts from one refused ingestion |
| a lost space in prose | `damagingthe`, `ina vise`, `riveted,or` |
| a lost space inside a compound | `engine-todrive train clearance` |
| damage at the dictionary: no repair by guessing | a joined word is refused, never silently split |

## Sources surveyed and refused

Seven Internet Archive NAVEDTRA rate training manuals were downloaded
(`scripts/probes/fetch-archive-text.py`), measured and **refused**: *Construction Mechanic 3 &
2*, *Aviation Machinist's Mate R 1 & C*, *Fluid Power*, *Aviation Structural Mechanic E 3 & 2*,
*Machinery Repairman 3 & 2*, *Patternmaker 3 and 2*, *Construction Electrician 3 & 2*. Between
them they offer 65 descriptions, and the survivors after the rules above are poor:

```
Special equipment        -> detect the amount of unbalance
Relatively large amounts -> form some of the cutting tools used in the machine shop
Single-acting cylinders similar -> exert force in only one direction
penetrating oil mixed    -> lubricate the shaft since ordir.aty lubricant would rapidly burn off
```

Their microfiche scans also write `reiatively`, `chu-k` and `ordir.aty`, and their sentences
run into one another without a full stop where one belongs (`... can be the same, v-belt drives
are commonly used with small, low-pressure, motor-driven compressors`). The rules refuse the
damage they can name, but what survives is a name that names nothing. Shipping them would put
visibly broken options in front of learners to make a count look better.

*Automotive Engine Maintenance and Repair* (Marine Corps Institute, ERIC ED 253 045) was
downloaded and **not used**, for the reason round 14 recorded: ERIC-hosted vocational curricula
are not clearly federal works. It yields one usable description in any case.

## Tooling this round added

| Tool | What it answers |
|---|---|
| `scripts/rebuild-corpus.py` | Rebuilds every subtest from its sources in one run, prunes the vault record that is not evidence, and writes the round's reports. `--dry-run` reports first. |
| `scripts/probes/fetch-archive-text.py` | Downloads an Internet Archive item's own `_djvu.txt` and prints its digest. |
| `scripts/probes/purpose-verbs.py` | Every distinct word a purpose opens with, which is how the verb list was written. |
| `scripts/probes/description-shapes.py` | How many descriptions each unread shape holds. |
| `scripts/probes/foreign-letters.py` | Learner-facing text carrying a letter from another alphabet or an undecodable byte. |
| `scripts/probes/mutation-round18.py` | The 16 mutation proofs. |

`scripts/probes/check-corpus.py` gained four invariants: the Auto Information stem and option
shapes, the one-question-per-stem rule, and an independent (Python) implementation of the scan
damage test. That implementation first reported 3,681 damaged items; 3,670 of them were
Electronics Information options like `THERMOCOUPLE`, which is the NEETS glossary's own
typography. The capitals rule is now scoped to the subtests a program framed from a manual's
sentence -- the probe was wrong, not the corpus.

## Gates

`sh scripts/verify.sh` exits 0. All 19 gates recorded exit 0 in
`.agent/verification/state/gate-results.jsonl`: generated-pack, anti-gaming-scan,
format-check, lint, typecheck, test-unit, test-collection-guard, test-integration, test-e2e,
security-check, dependency-audit, mcp-probe, provider-probe, build, artifact-identity,
proof-matrix-stamp, smoke-test, live-fire, release-state.

Test accounting from that run (`.agent/evidence/EP-007/content-corpus/verify-round18.log`):
**1,700 Rust tests across 109 binaries, 0 failed**; 210 frontend unit tests and 9 more in the
desktop suite; 36 end-to-end tests and 6 more in the packaged suite; the packaged desktop
live-fire passed against the artifact digest
`50d6a5a4216458c88fba7ae59f316a49990b33d185e5bfb4f1bd3bc070714ae1`.

Release verdict unchanged at `CONDITIONAL_EXTERNAL_GATES`: 42 DoD clauses, 27 PASS, 11 PARTIAL,
2 PENDING, 1 EXTERNAL_REQUIRED, 1 DEFERRED_LONG_RUNNING, 0 FAIL.

The corpus probes all exit 0 (`count-items.log`, `check-corpus.log`, `check-options.log`,
`foreign-letters.log`, `gutenberg-provenance.log`). `check-options.py` reports 0 options
containing scan damage and the two known rare Word Knowledge distractors (`unendowed`,
`becharm`), which it counts apart so the two are never confused for one another.

`python3 scripts/probes/mutation-round18.py` exits 0 with all 16 mutations caught.

## What is still not done

- **The bank sizes.** 414 Electronics Information and 1,949 Word Knowledge questions are what
  the sources support; 36 Shop and 59 Auto Information items are thin for a subtest that asks
  sixteen questions in a sitting. The seven refused manuals above are the measured evidence
  that more items need a better source, not a looser filter.
- **A noun-head check.** Six names in the Auto Information bank are still generic or
  fragmentary (`system`, `transfer`, `cycle piston-type engine`, `two resistances`,
  `Oil returning`, `system radius rods`). Refusing them needs Webster's part-of-speech markers,
  which the dictionary parser reads but does not expose; that is a change to `Dictionary` with
  its own measurement, not a rule to bolt on here.
- **The Word Knowledge distractor gap**: two options contain a rare word (`unendowed`,
  `becharm`), which Moby's associations supply and a word-frequency source would catch.
- **The pack schema** still carries neither the curriculum graph nor the calibration metadata
  that `CONTENT_PACK_SPEC.md` lists.
