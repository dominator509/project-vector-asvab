# ASVAB test specification — reference facts

This file records what the ASVAB program itself publishes about how the test is
built. It exists so that lesson and item design targets the real instrument
rather than an impression of it.

**It is a source ledger, not content.** Every statement below is either a fact
about the test (facts are not copyrightable) or a short quotation attributed to
its publisher. No third-party question text is reproduced here, and none may be
added: this repository is public, and copying a publisher's items into it would
be redistribution. See "Prohibited material" at the end.

Retrieved 2026-09-22. Re-verify before relying on any figure; test composition
changes between programme revisions.

## The ten subtests

Construct descriptions are the ASVAB programme's own one-line definitions of
what each subtest measures.

| Code | Subtest | What it measures | Scored items | Time (min:sec) | Seconds per item |
| --- | --- | --- | ---: | ---: | ---: |
| GS | General Science | knowledge of physical and biological sciences | 15 | 12:00 | 48 |
| AR | Arithmetic Reasoning | ability to solve basic arithmetic word problems | 15 | 55:00 | 220 |
| WK | Word Knowledge | ability to identify the best synonym for a given word | 15 | 9:00 | 36 |
| PC | Paragraph Comprehension | ability to obtain information from written passages | 10 | 27:00 | 162 |
| MK | Mathematics Knowledge | knowledge of high school mathematics principles | 15 | 31:00 | 124 |
| EI | Electronics Information | knowledge of electricity and electronics | 15 | 10:00 | 40 |
| AI | Auto Information | knowledge of automobile technology | 10 | 7:00 | 42 |
| SI | Shop Information | knowledge of tools and shop terminology and practices | 10 | 6:00 | 36 |
| MC | Mechanical Comprehension | knowledge of mechanical and physical principles | 15 | 22:00 | 88 |
| AO | Assembling Objects | ability to determine how an object looks assembled | 15 | 18:00 | 72 |

The published tables also give a second set of time limits that apply when
tryout (unscored, pre-test) items are embedded, roughly doubling each limit —
for example AR at 113 minutes against 55. Tryout items do not count toward the
score. Practice should therefore train to the **scored-question** limits.

## What this implies for lesson design

The seconds-per-item column is the most consequential number in this file, and
it is not uniform. The subtests fall into two clearly different regimes:

- **Recall subtests — 36 to 48 seconds per item** (WK, SI, EI, AI, GS). There is
  no time to reason. A learner who has to *work out* a synonym has already lost.
  These need automaticity training: spaced retrieval to the point of instant
  recognition, not explanation-heavy lessons.
- **Reasoning subtests — 72 to 220 seconds per item** (AR, MK, PC, MC, AO).
  There is room to set up and check work. These reward worked examples,
  misconception repair, and method drilling.

WK is 36 seconds per item and is one of the four subtests that determine the
AFQT score. It is the highest-leverage target in the product and the cheapest to
generate lawfully, because a synonym pair is a fact rather than an authored item.

## CAT-ASVAB administration

The computer-adaptive form selects each item from a pool spanning very easy to
very hard. A correct response is followed by a harder item, an incorrect
response by an easier one, and selection repeats until the subtest ends or time
expires. The programme's stated purpose is higher score precision at shorter
test length than the paper form.

Unanswered items are not ignored: the programme applies a penalty that scores
them as though answered at random. Its published guidance is that the limits are
liberal enough that nearly all examinees finish.

Two design consequences:

1. Adaptive selection means a learner's *sequence* differs from anyone else's,
   so a fixed linear mock cannot reproduce the real experience. Practising fixed
   15-item sets is still useful for content coverage, but it is not a simulation
   of adaptivity and must not be described as one.
2. Because unanswered items are penalised as random, never leaving an item blank
   is correct behaviour and should be taught explicitly.

## AFQT composition

The AFQT score — the number that determines enlistment eligibility — is computed
from four of the ten subtests: Word Knowledge, Paragraph Comprehension,
Arithmetic Reasoning, and Mathematics Knowledge. The remaining six contribute to
line scores for job qualification, not to AFQT.

This is already modelled in `crates/vector-domain/src/mastery.rs`; it is repeated
here because it drives prioritisation: two of the four AFQT subtests (WK, PC) are
verbal and two (AR, MK) are quantitative, so a study plan that ignores verbal
content cannot move the number that matters most.

## Prohibited material

The ASVAB programme publishes an explicit statement on this, and it settles the
question of "leaked" items:

> Beware of websites, social media profiles/posts, or individuals claiming to
> possess actual ASVAB test questions or guaranteeing passing scores through
> paid programs. These claims are false and misleading.

It further states that the Enlistment Testing Program does **not** provide actual
test questions to any third-party test preparation service, does **not** endorse
any paid programme claiming to guarantee passing scores, does **not** recommend
specific commercial prep courses or study guides, and does **not** authorize any
entity to sell "correct answers" to the test.

Consequences for this project, which are binding:

- Content sold as "actual" or "leaked" ASVAB items is either fabricated or
  compromised secure material. Neither may enter the corpus. Fabricated items
  additionally fail on quality: lessons built on them mis-prepare learners.
- Recalled-item "braindump" sites are excluded for the same reason. Recalled
  items are unverifiable, frequently misremembered, and often describe the paper
  form rather than the adaptive one.
- Commercial prep publishers' question banks are copyrighted works and may not
  be copied, paraphrased, or used as the source of a "respun" item.
- The product must never claim to possess real test questions or to guarantee a
  score. `ADR-010` already forbids presenting a precise predicted score; this
  extends the same discipline to content claims.

## Permitted sources

| Source | Publisher | Status | Use |
| --- | --- | --- | --- |
| [Sample questions](https://www.officialasvab.com/applicants/sample-questions/) | ASVAB programme | © All rights reserved | Format and difficulty reference only. Never copied. |
| [The CAT-ASVAB](https://www.officialasvab.com/applicants/cat-asvab/) | ASVAB programme | © All rights reserved | The item counts and time limits above (facts). |
| [Test preparation disclaimer](https://www.officialasvab.com/applicants/asvab-test-preparation-disclaimer/) | ASVAB programme | © All rights reserved | The quoted policy above. |
| [Preparing for the ASVAB](https://www.officialasvab.com/applicants/prepare/) | ASVAB programme | © All rights reserved | Points to the free official practice sources. |
| [NEETS modules](https://archive.org/details/neetsmodules_202003/) | US Navy (NETPDTC) | US government work, public domain | EI, and much of MC and GS content domains. |
| [Project Gutenberg](https://www.gutenberg.org/) | Various, public domain | Public domain | PC passages, WK vocabulary in context. |
| DTIC technical reports, e.g. [AD1116875](https://apps.dtic.mil/sti/pdfs/AD1116875.pdf), [ADA610668](https://apps.dtic.mil/sti/pdfs/ADA610668.pdf) | US Department of Defense | US government work, public domain | Item-construction and calibration methodology. |
| [March2Success](https://www.march2success.com) | US Army, operated by a contractor | Free to users; contractor content is **not** public domain | Link out only. Do not copy. |

Two operational notes recorded so they are not rediscovered:

- **DTIC blocks automated retrieval.** `apps.dtic.mil` returns HTTP 403 to
  non-browser clients. The reports above are public domain but must be
  downloaded manually or through a browser session.
- **March2Success is operated by a commercial contractor**, not published as a
  government work, despite being an Army programme. It is a legitimate place to
  send learners; it is not a source to ingest from.
