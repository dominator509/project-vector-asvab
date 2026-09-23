# Study corpus round 29: the corpus the pack ships, and the way back from a rollback

Node: EP-007 (content), with EP-009 (release lanes). Requirements touched: REQ-003 (adaptive
plan), REQ-014, REQ-022 (original item + proof), REQ-023 (signed, versioned packs), REQ-037
(update/rollback), REQ-048, REQ-056 (per-item provenance).

This round closes the two PENDING release clauses (DOD-034, DOD-035), closes DOD-002 and DOD-023
with a real clean-room run, and makes the objective mechanism of round 28 real in production.

## 1. The computable subtests are in the corpus, not generated on demand

Round 28 made practice follow the objective the plan names. On the installation, that mechanism
was a no-op for the three computable subtests: the plan could not name an objective for them
(none was declared), and a fresh installation held no Arithmetic Reasoning, Mathematics Knowledge
or Mechanical Comprehension items at all -- the first session had to press "Prepare 40 questions".

The pack now carries them. `crates/vector-application/examples/generate_corpus.rs` runs the
application's own pipeline (`ContentPipeline::generate_and_activate`: verify, then store, then
activate) with fixed per-subtest seeds, and `scripts/rebuild-corpus.py` calls it for the computed
subtests after the ingested ones, so a documented rebuild reproduces the corpus the pack is built
from:

```
$ python3 scripts/rebuild-corpus.py <db> --only AR,MK,MC
items after rebuild:
  AR   466   MK   421   MC   447   total 1334
```

**The corpus readback then caught a defect in the generated path.** `check-corpus.py` reported
`questions asked twice within one subtest: 126`: the factory draws its parameters at random, and
the store only refused an item whose *content hash* it already held, so the same question with its
wrong answers shuffled was stored twice. That is the padding the ingestion loops already refuse --
one Electronics Information module asked its definitions up to seven times each, and 3,668 items
carried 414 distinct questions between them. The generated path now applies the same question
identity check (`generate_and_activate`, covered by
`a_generated_batch_asks_each_question_once`), and the rebuild stores 1,334 distinct questions
instead of 1,500 rows with 166 repeats.

## 2. Corpus, and the invariants over it

| | |
|---|---|
| Active items | **6,961** -- WK 1,949 · PC 1,676 · EI 1,565 · AR 466 · MC 447 · MK 421 · GS 301 · AI 94 · SI 42 |
| Vault records | 70, with 8,910 item citations |
| Invariants | every one a zero: provenance per item, no duplicate content hashes, no question asked twice in a subtest, no scan damage, no item without an objective, PC passage rules, GS/SI/AI form rules |
| Proofs re-derived independently | **1,334 of 1,334 computable items recompute to the option they mark correct**, 0 unreadable, 0 disagreements -- `check-corpus.py` now evaluates each stored proof expression itself rather than trusting the verifier that produced it |

`check-corpus.py` grew that independent re-derivation this round (it also crashed on the first
generated item it met, assuming every proof was `SourceBacked`; the corpus legitimately holds two
kinds, and the probe was narrower than the corpus).

## 3. The pack, and every transition it claims (DOD-035)

`core-asvab v6`: 6,961 items, 56 sources, signed by the installation's own key
(`86b0ed0e…`), 28 declared objectives over 9 subtests with prerequisites in learning order.

Each step below ran through the documented commands against the installation's own store, with the
database hash recorded at every step (`packs/state-*.sha256`):

| Step | Command | State afterwards |
|---|---|---|
| before | `pack-list` / `count-items.py` | v5 active, 5,627 servable |
| upgrade | `pack-build` v6, then `pack-install` | v6 active, **6,961 servable** |
| rollback | `pack-rollback --name core-asvab` | v5 active; v6 quarantined; **AR 466 / MK 421 / MC 447 stored but 0 servable** -- the rollback withdraws its items in one statement, and leaves them intact |
| back | `pack-activate --version 6` | v6 active, 6,961 servable; v5 superseded, not quarantined |

**The transition back did not work, and that is the finding.** Reinstalling the same signed v6 pack
reported success and changed nothing: `install` deliberately leaves a registered pack's status
alone so a stray reinstall cannot undo a deliberate rollback -- the right safety property, but on
its own it makes a rollback a one-way door with no supported way home. There was no command to
activate a version the installation already holds. There is now (`pack-activate`, documented in
COMMANDS.md), it supersedes rather than quarantines the version it replaces so the transition can
be made again in either direction, and
`an_activated_version_returns_to_service_and_a_reinstall_does_not` pins both halves: the reinstall
stays inert, the activation moves the content.

Two further findings from the same work:

- **A pack's licence policy had to admit the computable subtests' own citation.** `pack-install`
  refused v6 whole: *source … carries licence "Facts-only; item text is original work", which the
  corpus does not permit*. The permitted list is the prohibited-source scan, and the honest fix is
  to name that licence in it with the reasoning attached (the page the citation names asserts all
  rights reserved; the items are the project's own; what keeps such a pack honest is the executable
  proof the installer recomputes and the vault record whose bytes were hashed), not to relabel the
  source as public domain.
- **The plan now names an objective for every subtest it schedules.** The live readback
  (`examples/plan_report.rs`, a copy of the real store):

```
  SI 3 min OBJ-SI-TOOLS-01     AI 3 min -            AR 3 min OBJ-AR-RATE-01
  EI 3 min OBJ-EI-TERMINOLOGY-01   GS 3 min OBJ-GS-EXPLAIN-01
  MC 3 min OBJ-MC-ADVANTAGE-01     MK 3 min OBJ-MK-ALGEBRA-01
  PC 3 min OBJ-PC-DETAIL-01        WK 6 min OBJ-WK-SYNONYM-01
what a session for each named objective is served:
  AR OBJ-AR-RATE-01 -> q-0d78876a-… [OBJ-AR-RATE-01] A printer produces 11 pages per minute…
  MC OBJ-MC-ADVANTAGE-01 -> q-036d5768-… [OBJ-MC-ADVANTAGE-01] The effort arm of a lever is 54 feet…
  MK OBJ-MK-ALGEBRA-01 -> q-0179ef29-… [OBJ-MK-ALGEBRA-01] If 6x + 18 = 36, what is the value of x?
```

## 4. Clean room (DOD-002, DOD-023)

A fresh `git clone` of the committed tree (same SHA), with no `target/` and no `node_modules/`, ran
the README's published commands in the published order: `install.sh`, `preflight.sh`,
`validate-generated-pack.py .`, `verify.sh`, `verify-evidence-archive.py` -- **all five exit 0**,
including the full 19-gate sweep inside the clone (unit, integration, E2E, security,
dependency-audit, MCP, provider, build, smoke, live-fire).

The clone's artifact digest is `fd716afa…` where the working copy's is `6983c707…`: the same source,
different bytes, which is the documented fact that a Windows build is not bit-reproducible and why
no gate asserts digest equality. Lockfile digests, tool versions and the manifest are recorded in
`.agent/evidence/EP-009/cleanroom/`.

## 5. Zero state (DOD-034)

On a directory that did not exist, the exact release artifact
(`sha256 4ae5cf7820d818c3bfa1fcfdffe755d4bad2dc2e30dc551abccbb243211e98e4`, the digest this
candidate's sweep recorded) ran its own self-check: database created and migrated from nothing, 19
checks, verdict `pass`, latency measured, backup taken, restore verified, tampered archive refused.
The signed v6 pack was then installed into that store through the documented command (6,961 items,
56 sources, 0 already present), and the golden path ran on it:

```
$ cargo run -p vector-application --example golden_path -- <zero-state>/vector.db zero-state 30
learner created   learner-9186a5e5-… name=zero-state target=70
plan              9 drill(s) over 30 minute(s)
session           SI OBJ-SI-TOOLS-01 -> q-05927e38-… answered, attempt stored once
                  … one per named objective, AR through WK …
attempts stored   8 row(s) over 8 subtest(s)
analytics         AR: 1/1 correct, mean 4000 ms … (8 subtests)
objectives with evidence   OBJ-AR-RATE-01 0.67 over 1 attempt(s) … (8 objectives)
provenance        6961 active item(s), 5716 in the objectives served, all cited
```

`examples/golden_path.rs` is new: the outcome the product exists for, run end to end against a real
store -- plan, session per objective, attempt recorded (and proven idempotent by submitting it
twice), analytics read back, mastery moved from 0.50 to 0.67 on one correct answer, provenance
re-checked. The soak found a defect in it within minutes: its attempt ids named the item and the
session index but not the learner, so a second run against the same store looked like a duplicate
submission. An attempt id identifies the *attempt*.

The machine is the developer's. The virgin-OS and hardware lanes are DOD-005 and DOD-022 and are
not provisioned, so DOD-034 is recorded PARTIAL with exactly that residual, not PASS.

## 6. Abbreviated soak (DOD-038)

`scripts/probes/soak.py` runs the packaged self-check plus a full golden path per iteration against
a copy of the installation's store, with a heartbeat line per iteration, an integrity check and a
row census. The trial recorded for this candidate
(`sha256 4ae5cf7820d818c3bfa1fcfdffe755d4bad2dc2e30dc551abccbb243211e98e4`): **112 iterations over
12.02 minutes, 896 attempts added, 0 failures, integrity `ok` at every heartbeat**, per-iteration
wall time min 5.45 s / mean 6.44 s / max 11.21 s. The clause says an abbreviated trial is labelled
separately and can never be PASS for the full requirement, and no scale is declared anywhere in
this repository, so the report labels itself: `"trial": "abbreviated"`, and `release-state.py` reads
that record rather than asserting a duration. An earlier trial in this round ran against the
candidate from before the final formatting pass and was re-run for that reason -- old evidence does
not prove changed bytes.

## 7. Gates

`sh scripts/verify.sh` exits 0 with all 19 gates recorded exit 0; artifact digest
`4ae5cf7820d818c3bfa1fcfdffe755d4bad2dc2e30dc551abccbb243211e98e4`, candidate epoch 17; 42
mutations across cargo, vitest and Playwright, **all 42 caught**; DoD accounting moved from
27 PASS / 11 PARTIAL / 2 PENDING / 1 DEFERRED_LONG_RUNNING / 1 EXTERNAL_REQUIRED to
**30 PASS / 11 PARTIAL / 1 EXTERNAL_REQUIRED**. Verdict unchanged:
`CONDITIONAL_EXTERNAL_GATES`.

The zero-state lane was re-run against this candidate after the artifact changed under it (a
formatting and clippy pass on the sources the binary is built from), because DOD-040's rule is that
a code change invalidates the results that depended on the old bytes.

## 8. What is still not done

- **DOD-034/005/022**: the clean-VM and low/mid/high hardware lanes need machines this one is not.
- **DOD-039**: screen-reader validation by a person, code signing, legal review, name clearance.
- **DOD-013** (runtime canary), **DOD-031** (dependency-edge graph), **DOD-036** (RPO/RTO/MTTR),
  **DOD-040** (change-invalidation graph), **DOD-033** (provisioning adapter): unchanged, and the
  next in-repo work.
- **REQ-014** (local GGUF model launch), **REQ-032** (`gh` pull-request lane), **REQ-037**
  (signature verification for updates): the three open requirements that are work here rather than
  external gates.
- **npm advisories**: `pnpm audit --prod` is clean, but the full audit reports 1 critical / 2 high
  / 5 moderate in dev tooling (vite, vitest, esbuild), and the `dependency-audit` gate covers Rust
  advisories and npm *licences* rather than npm advisories. Real, and next.
- SI remains the thin bank at 42 items, AI keeps its known noise floor, General Science is one work,
  and the Word Knowledge rare-distractor gap is unchanged.
