# Study corpus round 30: closing the four in-repo evidence clauses

Node: EP-007 (content), EP-009 (release lanes). Requirements touched: REQ-003, REQ-022, REQ-036,
REQ-037, REQ-048, REQ-056, plus the harness requirements REQ-039/REQ-040.

Round 29 closed the two PENDING release clauses. This round closes the four remaining clauses that
were *in-repo work rather than missing capability*: DOD-013 (runtime canary), DOD-031 (dependency
edges and cascade avoidance), DOD-036 (recovery objectives) and DOD-040 (change invalidation). Two
of them turned up real product defects.

## 1. The sweep no longer lets one failure hide the rest (DOD-031)

The sweep stopped at the first failing gate. That is not what DOD-031 asks for: a failed gate is a
prerequisite for the gates that consume *its artifact* and for nothing else, and stopping meant
eighteen gates reported nothing at all, so a reader could not tell "this gate failed" from "we
never got there".

`scripts/verify.sh` now runs every gate, and a gate whose declared prerequisite did not pass is
recorded as `BLOCKED_PREREQUISITE` with the gate that blocked it (`"exit": null` -- not a pass,
not a failure). The sweep exits non-zero if anything failed or was blocked. `release-state.py`
reads `null` as "not run in this sweep", which is the distinction its own docstring already made.

Demonstrated in this round by accident and then on purpose: a formatting failure in the first run
left all twenty other gates running and recorded, which is exactly the continuation the clause
wants. `scripts/dependency-graph.py` derives the graph from the test registry, the 484-row
accounting and the sweep's own results, and refuses:

- a blocked row whose reason names no blocker (`GEN-011 is BLOCKED_CAPABILITY and its reason names
  no prerequisite or capability: 'the toolchain was in a bad mood'` -- the negative proof);
- a gate recorded as blocked by a gate that passed (`gate smoke-test is recorded as blocked by
  format-check, which passed` -- the second negative proof);
- any passing row with an incoming edge, and any cycle.

Measured over the real accounting: **484 tests, 26 explicit edges over 12 unsatisfied nodes, the
widest blocking 7 tests, and 77 tests that ran to completion inside a stage holding a blocked
sibling** -- so cascade avoidance is observed, not asserted.

## 2. What this candidate's changes invalidate (DOD-040)

`scripts/change-invalidation.py` records each candidate epoch against its commit and artifact
digest, diffs the previous epoch against this one (committed *and* uncommitted -- several rounds
verified bytes no commit described), maps every changed path to a surface and that surface to the
gates that exercise it, and closes the rerun list over the gate edges. It refuses a changed path no
surface covers. Its own first run caught a bug in itself: `git()` stripped the leading space of
porcelain output, shifting the first path by one character, which the coverage check reported as a
changed path (`.agent/...`) no surface covered.

At this epoch: **prior epoch 17 (a1de4c02) -> epoch 19 (21c6069), 55 changed paths over 8
surfaces, 15 gates on the rerun list, no violations.** The gate edges it uses are checked against
`scripts/verify.sh` on every run, so the two cannot drift apart while both claim to describe the
same pipeline.

## 3. An unpredictable value through the real path (DOD-013)

`scripts/probes/canary-proof.py` draws a learner name from the operating system's CSPRNG and a
target score from the same source at run time, propagates both through the application's own study
path against a copy of the installation's store, and reads them back with a separate SQLite
connection:

```
canary: canary-e1444bf3951a95a6 target=79 minutes=48 (decoy canary-e1444bf3951a95a5)
propagation: 8 session(s), attempts stored 8 row(s) over 8 subtest(s)
observation: {"found": 1, "name": "canary-e1444bf3951a95a6", "target_score": 79,
              "attempts": 8, "mastery_rows": 8}
negative control: {"found": 0}
```

The negative control is the part that makes it a proof: the same canary with one character changed
must find nothing, or an observation that always passes would look identical to one that reads the
value. Storing mastery also had to become part of the golden path for the canary to have something
to read -- an estimate computed and never stored is not something a later session can rely on.

Limitation, stated in the report and in the accounting: the canary enters at the command boundary
rather than through the window, because no window driver exists in this environment (DOD-004).

## 4. Recovery objectives, measured -- and a real gap they found (DOD-036)

`scripts/probes/recovery-objectives.py` declares the objectives (no automatic backup schedule is
claimed, so the RPO is the interval since the learner's last backup; a 60-second RTO target for a
store of this size), then injects three hard failures on copies of the real store: a truncated
file, a corrupted page, and a deleted file.

The first run found that **a truncated store could not be restored at all**: `restore_verified`
needs a handle on the live database, and SQLite refuses to open a malformed one, so the documented
restore path failed with "database disk image is malformed" before any restore logic ran. A learner
whose file was truncated by a power loss or a bad copy had no way back to their archive.

Fixed in `vector-persistence`: `restore_verified_at` takes the live *path* instead of a handle,
runs the same checks on the archive (digest, integrity, readability), and swaps the file with no
connection to the damaged database, moving the damaged file aside rather than deleting it. The CLI
falls back to it when the live file cannot be opened. `a_truncated_store_is_restored_from_its_
archive` pins it, and the mutation `no-path-based-restore` proves the test depends on it.

Measured after the fix: **truncated, corrupt-page and deleted all reconcile to the pre-fault rows
and bytes; RTO 7.85 s against the declared 60 s, MTTR 7.85 s.**

## 5. The mutation harness was misclassifying mutations

Two Rust mutations added in round 29 were written into the *frontend* list, so they were "caught"
by a vitest runner that never loaded them -- a false pass in the harness whose entire purpose is to
prevent false confidence. Found by the guard this round added and by moving the entries, and three
harness defects were fixed:

- entries are now in the list whose runner can execute them (40 cargo, 2 vitest, 1 Playwright);
- `test_passes` refuses to answer when the crate is not a workspace member or the filter matched no
  test -- counting *both* passed and failed, because a caught mutation makes its test fail and a
  failing binary reports "0 passed; 1 failed" (the first version of this guard stopped the run on
  the very first mutation);
- `vitest_passes` refuses when the file does not exist or no test was selected.

`python3 scripts/probes/mutation-round18.py` now catches **all 43**.

## 6. Accounting

| | before this round | after |
|---|---|---|
| DoD clauses | 30 PASS / 11 PARTIAL / 1 EXTERNAL_REQUIRED | **34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED** |
| Gates | 19 | **21** (dependency-graph, change-invalidation) |
| Mutations caught | 42 (two of them falsely) | **43 (all verified by the runner that can run them)** |

The seven remaining PARTIALs are: DOD-004 (E2E inside the packaged process), DOD-005 and DOD-022
(clean-VM and hardware lanes), DOD-016 (no prior released version), DOD-033 (provisioning adapter),
DOD-034 (the zero-state lane is executed; the foreign machine is not), DOD-038 (an abbreviated trial
by the clause's own rule). DOD-039 stays EXTERNAL_REQUIRED.

Three accounting rows were narrowed rather than left stale: `E2E-011` (clean-room deployment) now
records that the cache-free half is executed and the foreign machine is what remains; `SUP-004`
(reproducible build) records the *measurement* that two clean environments produce different bytes
for the same source, so the claim stays blocked rather than relabelled; `E2E-018` (soak) records the
abbreviated trial and the full duration still missing.

## 7. Corpus and artifact

Unchanged this round: **6,961 active items**, pack `core-asvab v6` active, every provenance
invariant a zero. Artifact digest `f75d5bc89e0c1a96…`, epoch 19, 21 gates exit 0, verdict
`CONDITIONAL_EXTERNAL_GATES`.

## 8. What is left in-repo

- the npm advisory state: `pnpm audit --prod` is clean, dev tooling carries 1 critical / 2 high /
  5 moderate, and the dependency-audit gate covers Rust advisories and npm *licences* rather than
  npm advisories;
- the three open requirements that are work here: REQ-014 (local GGUF model), REQ-032 (`gh`
  pull-request lane), REQ-037 (signature verification for updates).
