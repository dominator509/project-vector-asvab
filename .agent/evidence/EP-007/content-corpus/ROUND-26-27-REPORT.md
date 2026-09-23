# Study corpus round 26-27: the curriculum's grain, and a plan that promised an empty drill

Node: EP-007. Requirements touched: REQ-003, REQ-023, REQ-048, REQ-056.

## What these rounds add

Rounds 20 and 21 gave the pack a curriculum graph and made the plan follow its *ordering*. The
graph's grain was still unused: the plan reasoned about subtests, and "Shop Information" is not
one skill. This round reads it at the grain the content declares.

**Mastery per objective.** `AttemptRepo::objective_stats` joins each attempt through the item it
was on to the objective that item teaches -- an attempt records the question, the question records
the objective, so this is read rather than maintained. `Services::objective_mastery` reports, per
objective the installed pack declares: attempts, correct, the same Laplace estimate the subtest
mastery uses (`(correct + 1) / (attempts + 2)`, so no evidence is 0.5 rather than 0), and
`waiting_on` -- the prerequisites this learner has not met.

**A threshold, in one place.** `PREREQUISITE_MET = 0.6` against that estimate: no attempts gives
0.5, one correct answer out of one gives 0.67, one wrong out of one gives 0.33. So an untouched
prerequisite is *unmet* rather than assumed known, one correct answer is enough to move past it,
and one wrong answer is not. The number is a named constant with that reasoning attached, because
it is a judgement and a reader should be able to disagree with it.

**The plan names the objective.** Each drill carries the objective to work on -- the weakest whose
prerequisites are met, ties broken by fewer attempts and then by id so two plans from the same
evidence agree -- and its reason says which and why:

```
SI     4 min  objective OBJ-SI-TOOLS-01
      weakness; it is the prerequisite for AI in this pack's curriculum; weakest objective with
      its prerequisites met is OBJ-SI-TOOLS-01 (0.50 over 0 attempt(s))
AI     4 min  objective -
      weakness
```

An objective whose prerequisite is unmet is *not offered*: the curriculum says it comes later, and
offering it now is the thing the graph exists to prevent. It is read end to end from the pack: the
objectives, their subtests and their prerequisites all come from the installed manifest, and a
device with no pack names no objective at all.

## The defect the live readback found

The service tests prove behaviour against a fixture. To check the real thing, round 26 added
`crates/vector-application/examples/plan_report.rs`, which opens a *copy* of the installation's
own database -- the pack it installed, the corpus that was ingested into it -- creates a learner
and prints the plan. Its first run produced this:

```
plan for 45 minute(s):
  SI     4 min  objective OBJ-SI-TOOLS-01   weakness; ...
  AI     4 min  objective -                 weakness
  AO     4 min  objective -                 weakness          <-- nothing to serve
  AR     4 min  objective -                 weakness
  ...
```

**AO is Assembling Objects, and this installation has neither items nor templates for it.** The
subtest exists in the domain, the planner ranks every subtest in the domain, and the plan sent the
learner to a drill that could not start. It is exactly the class of defect the project's reality
law is about: the unit tests, the view tests and the E2E all passed, and the thing was visible
only by reading what a real store produces.

The fix is in the service rather than on the planner's output: only subtests this installation can
serve are handed to the planner (`factory::templates_for` non-empty, or items already stored).
Filtering afterwards would leave the freed minutes allocated to a drill that is not in the plan,
and the interface checks that a plan's total equals its drills. The readback now ends:

```
  SI     4 min  objective OBJ-SI-TOOLS-01   ...
  AI     4 min  objective -                 ...
  AR     4 min  objective -                 weakness
  ...
```

with no AO, and the second run of the readback is part of this round's evidence.

Two of the service tests then failed, and that was the filter working: they wanted a plan with SI
and AI in it, and their fixture store held nothing but factory-generated Arithmetic Reasoning
items -- the same assumption, in miniature, that the readback caught in the product. The fixture
now ingests a Shop and an Auto Information manual through the real pipeline.

## Corpus

Unchanged at **5,627 active items** (1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 42 SI, 94 AI), pack
`core-asvab v5` active. This round read the corpus at a finer grain rather than adding to it.

## Gates

`sh scripts/verify.sh` exits 0. Test accounting is in the round's ledger entry.
`python3 scripts/probes/mutation-round18.py` exits 0 with **all 35 mutations caught**, including
this round's three: the prerequisite threshold, the objective focus and the servable filter.

## What is still not done

- **SI 42 remains the thin bank**, and AI's new frame carries a known noise floor (`What does the
  Slack serve as?`); both are recorded rather than papered over.
- **The curriculum's grain stops at the plan.** Mastery is now read per objective, and the
  *practice* surface still chooses items by subtest; selecting the next item from the objective the
  plan named is the natural next step.
- General Science is one work, and the Word Knowledge rare-distractor gap is unchanged.
