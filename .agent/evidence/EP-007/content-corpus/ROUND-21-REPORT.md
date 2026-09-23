# Study corpus round 21: what a pack teaches, in the product

Node: EP-007. Requirements touched: REQ-003, REQ-023, REQ-032, REQ-048, REQ-056.

## What this round adds

Round 20 gave the pack format the curriculum graph and the calibration metadata
`CONTENT_PACK_SPEC.md` has always listed, and recorded honestly that neither was *visible*:
they travelled in the pack and the installer verified them, and no learner or reviewer could
see them. This round closes that.

**The content manager shows what a pack teaches.** `InstalledPackDto` gains one entry per
objective the pack's manifest declares -- its title, its subtest, its prerequisites, and the
calibration it carries -- and the packs panel renders them under the pack's row:

```
What core-asvab v3 teaches
  Auto Information: what a component is for (AI) -- after OBJ-SI-TOOLS-01
      -- 53% expected correct, declared, no responses recorded
  Shop Information: which tool performs a purpose (SI) -- 53% expected correct, declared, no responses recorded
```

The `responses` field is what the sentence turns on. A difficulty figure with no responses
behind it is a declaration made from the material, and the panel says "declared, no responses
recorded" rather than printing a percentage that reads like an observation. A pack whose
manifest predates the curriculum says so instead of rendering an empty list as if it were one.

**The plan follows the curriculum.** `Services::study_plan` reads the active pack's graph and
moves a drill that teaches a prerequisite ahead of the drill that requires it, appending the
reason to the drill's own: `... ; it is the prerequisite for AI in this pack's curriculum`. Two
things make this honest rather than clever:

* the order is the *pack's*, not this layer's opinion of the subject. A device with no pack
  installed has no declared curriculum, so its plan is unchanged;
* the move is conservative. It only pulls a prerequisite earlier, it only when both subtests
  are already planned, and it never replaces the planner's own reason -- a learner with fifteen
  minutes should not have their session stretched to reach something they will meet tomorrow,
  and the planner's judgement about *why* and *how long* is still true after the reordering.

## The defect the work found

`objectives_from_manifest` first parsed the registry's stored manifest as a `PackDocument`.
The installer stores the pack's *payload* -- the bytes the signature covers, without the
signature itself -- so every pack listed zero objectives and the panel would have shown "this
pack declares no curriculum" for a pack that declared six. The integration test caught it on
its first run (`left: 0, right: 4`), which is what the test was for: the field would otherwise
have been populated by nothing and rendered as a blank list nobody could distinguish from an
old pack.

## Verification

`sh scripts/verify.sh` exits 0. All 19 gates recorded exit 0 in
`.agent/verification/state/gate-results.jsonl`. Test accounting from that run
(`verify-round21.log`): **1,717 Rust tests across 109 binaries, 0 failed**; 211 frontend unit
tests and 9 more in the desktop suite; 36 end-to-end tests and 6 more in the packaged suite;
the packaged desktop live-fire passed against artifact digest
`f14cd345df8568de02059b2fe0249dad53f44a12d20442b1d743cecd84504b38`.

Release verdict unchanged at `CONDITIONAL_EXTERNAL_GATES`: 42 DoD clauses, 27 PASS, 11 PARTIAL,
2 PENDING, 1 EXTERNAL_REQUIRED, 1 DEFERRED_LONG_RUNNING, 0 FAIL.

The rules added this round are mutation-proved like the rest: `mutation-round18.py` exits 0 with
**all 26 mutations caught**, including the two that carry this round's behaviour -- removing the
prerequisite ordering fails `a_plan_follows_the_curriculum_the_installed_pack_declares`, and
emptying `objectives_from_manifest` fails `an_installed_pack_reports_what_it_teaches`.

The panel is covered at three levels, because each level can fail on its own: a Rust test that
an installed pack reports the graph and the calibration it was built with, a view test that the
panel renders the prerequisite edge and distinguishes a declared figure from a measured one,
and an end-to-end test in the built bundle that the same holds through the stub boundary.

## What is still not done

- **SI 55 and AI 75 remain thin** for a subtest that asks sixteen questions in a sitting. Every
  source measured across three rounds is either in the corpus or refused with numbers.
- **The plan orders by prerequisite but does not yet track mastery per objective.** Attempts
  are recorded per item and items carry an objective id, so per-objective mastery is derivable;
  the plan itself still reasons about subtests, and the curriculum's finer grain is used only
  for ordering.
- **General Science is one work** (301 questions): only *The book of wonders* in the corpus has
  the question-and-answer shape `ingest-facts` reads.
- The noun-head check for the residual generic names, the two rare Word Knowledge distractors,
  and the calibration of anything by real responses (there are still no learners) are unchanged.
