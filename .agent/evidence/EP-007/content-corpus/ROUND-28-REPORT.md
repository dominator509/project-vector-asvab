# Study corpus round 28: the plan's objective reaches the item the learner is served

Node: EP-007. Requirements touched: REQ-003 (adaptive plan), REQ-048 (content QA), REQ-056
(per-item provenance).

## The gap this round closes

Round 26-27 gave the plan a grain finer than the subtest: each drill names the objective to work
on (`OBJ-SI-TOOLS-01`, `OBJ-EI-TERMINOLOGY-01`, ...), read from the installed pack's curriculum and
chosen as the weakest objective whose prerequisites are met. The practice surface ignored it. The
plan said one thing and the session did another, and the learner had no way to tell which to
believe. Its own report recorded that: *"the curriculum's grain stops at the plan ... selecting the
next item from the objective the plan named is the natural next step."*

This round carries the objective from the plan, through the command boundary, into the read that
picks the item.

## What changed, layer by layer

| Layer | Change |
|---|---|
| `vector-persistence` | `ContentItemRepo::servable_in(subtest, Option<&str>)`; `servable(subtest)` delegates with `None`. The narrowing is one predicate in one SQL string -- `AND (?2 IS NULL OR objective_id = ?2)` -- because the provenance rules below it (active, and not withdrawn by a pack rollback) are the part that must never diverge between the narrow and the wide read, and one string cannot drift from itself. |
| `vector-application` | `ContentPipeline::next_item_for(subtest, objective_id, seen)`; `next_item` delegates with `None`. A free `pick` helper holds the rotation rule (first unseen, lowest id, wrapping when the pool is exhausted) so the two reads cannot rotate differently. |
| Command boundary | `content_next_impl(db, subtest, objective_id, seen)`; the Tauri command takes `objective_id: Option<String>`, so a caller with no objective in hand (direct subtest browsing) behaves exactly as before. |
| Interface | `VectorClient.contentNext(subtest, seen, objectiveId?)`; `usePracticeItems(..., objectiveId)` passes it on every fetch; `PracticeContentView` takes a `PracticeRequest { subtest, objectiveId }` and says which objective it is serving; `TodayView` renders a per-drill control that starts the session the plan asked for; `Shell` carries the request into the view and announces it through the polite live region. |

Two decisions worth recording:

- **An objective with no content falls back to its subtest, not to nothing.** A plan can name an
  objective this installation has no items for -- a pack was rolled back, or the objective is
  declared and unbuilt. Refusing to serve anything turns a content gap into a blocked learner. The
  fallback is scoped to the subtest, so it cannot cross into another skill; the test asserts both
  halves.
- **Choosing a subtest by hand clears the objective.** Keeping another subtest's objective would
  narrow the new subtest's read to an objective it cannot hold.

The drill control is rendered only when a navigation callback is supplied, and a test asserts that
it is absent otherwise: a "Practise" button that went nowhere would be worse than the label it
replaces.

## Live readback against the installation's own store

`crates/vector-application/examples/plan_report.rs` (added in round 26) now prints not just the plan
but what a session for each named objective is actually served. Run against a copy of the real
database -- the installed `core-asvab v5` pack and the 5,627 items ingested into it:

```
what a session for each named objective is served:
  SI   OBJ-SI-TOOLS-01            -> q-05927e38-... [OBJ-SI-TOOLS-01] Which tool is used to cut material such as soft wire and nails?
  EI   OBJ-EI-TERMINOLOGY-01      -> q-00045de5-... [OBJ-EI-TERMINOLOGY-01] Which term means: "The lower extremity of the output waveshape is clampe…
  GS   OBJ-GS-EXPLAIN-01          -> q-00abec4a-... [OBJ-GS-EXPLAIN-01] Why Do I Laugh When Tickled?
  PC   OBJ-PC-DETAIL-01           -> q-000e1600-... [OBJ-PC-DETAIL-01] According to the passage, which of the following is stated?
  WK   OBJ-WK-SYNONYM-01          -> q-0031b249-... [OBJ-WK-SYNONYM-01] Choose the word that most nearly means the same as BLANDISHMENT.
```

Every served item carries the objective the plan named. Command:
`cargo run -q -p vector-application --example plan_report -- <copy of vector.db> r28-probe 45`,
exit 0.

## Tests, and the mutations that prove them load-bearing

New Rust tests (`apps/desktop/src-tauri/tests/content_commands.rs`, driven through the real embedded
migrations and the real ingested/generated corpus):

- `a_named_objective_serves_its_own_items_and_not_a_neighbour` -- reads the objective spread back
  out of the store, then walks *every* objective to exhaustion and asserts each session served
  exactly that objective's items and never repeated one before the pool ran out.
- `an_objective_with_no_content_falls_back_to_the_subtest` -- including that the fallback does not
  cross into another subtest.

New interface tests: `usePracticeItems` "asks the backend for the objective the plan named" and
"falls back to the whole subtest when the objective holds nothing" (asserting the objective reached
the *command boundary*, not merely the hook's arguments); `dataViews.test.tsx` "the plan's objective
reaches practice" -- create a learner, click the plan's drill, and assert the session names the
objective and every `content_next` call carried it.

New browser-level test in the built bundle (`e2e/smoke.spec.ts`): the stub holds two Arithmetic
Reasoning items in two objectives, with the *wrong* one first; the spec creates a profile, clicks the
plan's drill, and asserts the question shown is the objective's item. Without the narrowing it would
show the subtest's first item, so the assertion is about the rule and not about ordering luck.

`python3 scripts/probes/mutation-round18.py` now runs **40 mutations across all three runners and
catches all 40**:

| Mutation | Rule it disables | Caught by |
|---|---|---|
| `objective-narrowing-ignored` | a named objective narrows the servable read (`AND (?2 IS NULL OR 1 = 1)`) | `a_named_objective_serves_its_own_items_and_not_a_neighbour` |
| `objective-fallback-removed` | an objective with no content falls back to the whole subtest | `an_objective_with_no_content_falls_back_to_the_subtest` |
| `objective-not-sent-to-the-backend` | the reader asks for the objective the plan named | `the plan's objective reaches practice` (vitest) |
| `objective-not-carried-into-practice` | the shell carries the plan's request into the view | `the plan's objective reaches practice` (vitest) |
| `e2e-stub-serves-the-whole-subtest` | the served item is the objective's, not the subtest's first | `the plan's objective reaches the session it starts` (Playwright, against a freshly built bundle) |

The harness grew a second and third runner for this: cargo, vitest, and Playwright. The Playwright
runner rebuilds the bundle first, because a spec run against a stale `dist` is how a green E2E
describes code that no longer exists.

## Corpus

Unchanged: **5,627 active items** (1,949 WK, 1,565 EI, 1,676 PC, 301 GS, 42 SI, 94 AI), pack
`core-asvab v5` active, 55 sources, every provenance invariant a zero. This round read the corpus at
a finer grain rather than adding to it.

## Gates

`sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0
(`.agent/verification/state/gate-results.jsonl`); artifact digest
`6983c707fafa8ac2f60e1c87fc35636cadf27f6a0fd78a08fd91caaba125f573`, candidate epoch 16, verdict
`CONDITIONAL_EXTERNAL_GATES`. Test accounting this round: 70 Rust test binaries / 949 tests passed
(`cargo test --workspace`), 65 binaries / 902 tests in the unit gate, 217 frontend unit tests in 12
files, 9 frontend integration tests, 37 Playwright tests (36 + this round's), 0 failures anywhere.

## What is still not done

- **The narrowing is currently a no-op on the ingested subtests.** The pack's curriculum declares one
  objective per ingested subtest and every one of that subtest's items carries it, so a narrowed read
  returns the same set as the wide read today (read back from the store: AI 94/`OBJ-AI-FUNCTION-01`,
  EI 1565/`OBJ-EI-TERMINOLOGY-01`, GS 301/`OBJ-GS-EXPLAIN-01`, PC 1676/`OBJ-PC-DETAIL-01`, SI
  42/`OBJ-SI-TOOLS-01`, WK 1949/`OBJ-WK-SYNONYM-01`). The rule is enforced and mutation-proved, and
  it becomes observable the moment a subtest holds two objectives.
- **A plan cannot name an objective for a generated subtest.** The factory assigns objectives to its
  items (`OBJ-AR-RATE-01`, `OBJ-MK-ALGEBRA-02`, `OBJ-MC-LEVER-01`, 22 in total) but the pack's
  curriculum declares none of them, so AR, MK and MC drills carry `objective_id: null` and the
  session stays subtest-wide. Declaring them in the curriculum is the next step and needs a new pack
  version.
- **Practising a generated subtest on a real installation still takes one explicit click.** With
  5,627 items stored, `usePracticeItems` does not auto-generate, so the session shows "Prepare 40 AR
  questions" before it can serve one. Honest, but it is a step the plan implies should already be done.
- SI 42 remains the thin bank; AI's noun frame carries a known noise floor (`What does the Slack
  serve as?`); General Science is one work; the Word Knowledge rare-distractor gap is unchanged.

## A harness hazard, recorded

Two defects in the mutation harness surfaced this round, both of the same kind: a tool that changes
the thing it is measuring.

- **Run it alone.** In this round a concurrent `cargo test` failed
  `a_letter_from_another_alphabet_is_refused` because that rule was mutated at that moment. The
  failure belonged to the harness, not to the code, and cost a wasted investigation. The docstring
  now says so.
- **Put the tree back byte-exactly.** The harness read and wrote in text mode, which normalises CRLF
  to LF on read and expands LF to CRLF on Windows writes, so every mutated file came back with
  different bytes than it went in with. `apps/desktop/src/App.tsx` and `usePracticeItems.ts` came
  back CRLF and the sweep's format gate failed on two files nobody had edited -- a false failure in
  the very gate that is supposed to only fail for a real reason. `apply` now writes the file's own
  line endings and `restore` rewrites the original bytes; a full 40-mutation run now leaves
  `sh scripts/format-check.sh` passing with each file's ending unchanged (App.tsx LF, Rust files
  CRLF, verified byte-wise).
