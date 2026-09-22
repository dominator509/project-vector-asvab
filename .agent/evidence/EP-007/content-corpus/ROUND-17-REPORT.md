# Study corpus round 17: Shop Information, and an OCR check that was doing nothing

Node: EP-007. Requirements touched: REQ-022, REQ-023, REQ-048, REQ-056.

## What this round adds

**Shop Information.** `crates/vector-questions/src/purposes.rs` reads a third shape of
public-domain text: a tool manual describing what each tool is for.

```text
SCREW AND TAP EXTRACTORS
Screw extractors are used to remove broken screws without damaging the surrounding material.
Long-nose pliers are used for gripping, reaching places not readily accessible to the hand.
```

The question has to be *formed* rather than quoted -- "Which tool is used to remove broken
screws ...?" -- with the purpose clause taken verbatim from the manual and the distractors
drawn from the same manual's other tools, each of which is described doing something else.
`content ingest-tools` wires it through the pipeline, the vault and the command boundary.

**30 items from two US government manuals**: *Tools and Their Uses* (NAVEDTRA rate training
manual, 1971) and *TM 11-453 Shop Work* (US Army Signal Corps, 1942), both Internet Archive
scans recorded in the vault with their own URL and digest.

## The check that was doing nothing, and the check that was doing harm

This round's most useful result is about a safeguard that appeared to work.

The corpus's OCR check, `looks_like_misreading`, asks whether a token is one edit away from
a dictionary word. On the factual and shop paths it was being handed **whole sentences and
whole options**, and it answers a question about a *word*. The result was a check that could
not fail: it was a no-op that looked like diligence. It was found by a test that insisted a
misread tool must not reach an option -- and `inuide micrometer` did.

Fixing the granularity immediately showed the opposite failure. Applied word by word to
*The book of wonders* it refused **226 of 301 items**, and every word it printed was ordinary
English: `produced`, `caused`, `applied`, `began`, `countries`, `today`, `oldest`,
`remarkably`. Those are one edit from their own base forms -- `produced` is `produce` plus a
deletion, because the inflection helper does not know that `-ed` follows a silent `e`. A
filter that rejects the language is worse than no filter: it removes real content while
looking like rigour.

So the check now runs where its precision was actually measured, and nowhere else:

| Path | Check | Why |
|---|---|---|
| EI (NEETS glossaries) | `looks_like_misreading`, on terms and definitions | short technical vocabulary, where it was designed and where it caught `eleetron` and `LILTER` |
| SI (tool manuals) | every word of every **option** must be a dictionary word, inflections resolved | a tool name is one to three common nouns, so an unknown word is genuinely suspect -- `inuide micrometer` is refused |
| GS (science book) | none | measured against this very book: 226 of 301 items refused, all of them ordinary words. The work is a proofread reprint, not a scan |

Consequences, read back from the store: GS is **301 items** (it had been cut to 209 by the
over-eager rule), SI is **31**, and `scripts/probes/check-options.py` reports **0 options
containing scan damage** across all 7,047 items.

That probe also reports two Word Knowledge options containing a rare word (`unendowed`,
`becharm`) as a *separate* category. That is the oldest open gap in this corpus -- Moby's
associations are not all synonyms -- and it is not scan damage; the two are counted apart so
they are never confused for one another.

## Corpus

| Subtest | Items | Source |
|---|---|---|
| WK | 2,000 | Moby Thesaurus corroborated by Webster's 1913 |
| EI | 3,668 | NEETS module glossaries |
| PC | 1,047 | 17 public-domain works, passage quoted |
| GS | 301 | *The book of wonders*, question and answer |
| SI | 30 | *Tools and Their Uses*, *TM 11-453 Shop Work* |
| AR / MK / MC | generated on demand | computable templates with executable proofs |
| **Total stored** | **7,047** | |

31 evidence records, 9,047 citations, 28,188 review rows, every provenance invariant
zero-offender including the two added for SI this round.

## Sources surveyed and refused

`Machinery's Reference Series` booklets 3, 11 and 12 (1908-1916) were downloaded and
measured, and refused. Their scans mangle tool names into things like `MaWng Oonoave
Forming`, `Drill Jiers Drill` and `punch does not`, and the engine's filters -- interior
capitals, clause words, figure references -- reject the damage but the survivors are still
poor. Three candidate statements from booklet 3, four from booklet 12, and booklet 11's 23
were read and judged unusable. Shipping them would have put visibly broken options in front
of learners to make a count look better.

**Auto Information still has no items.** Round 14's finding stands and was re-confirmed:
the Navy automotive manual's review questions refer to figures that are not in the item, and
the Project Gutenberg automotive texts are essayistic (0-4 usable statements each).

## Gates

All eight exit 0: format-check, lint, typecheck, unit, integration, e2e, generated-pack,
anti-gaming. 906 Rust tests across 70 binaries, 0 failed; 210 frontend tests; 36 E2E.
Evidence for this round in this directory: `ingest-tools-si.json`, `check-options.py` output
in the round log, `check-corpus.log`, `count-items.log`.

## What is still not done

- **Auto Information has no items**, as above.
- **Shop Information is 30 items**, thin for a subtest with sixteen questions in a sitting.
  A larger bank needs a manual that lists tools systematically rather than describing them in
  prose.
- **The Word Knowledge distractor gap**: Moby's associations are not all synonyms, and the
  dictionary filter reduces the rate instead of eliminating it.
- **The pack schema** still carries neither the curriculum graph nor the calibration metadata
  that `CONTENT_PACK_SPEC.md` lists.
