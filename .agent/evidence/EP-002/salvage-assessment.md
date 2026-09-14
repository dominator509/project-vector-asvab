# EP-002 salvage assessment

Date: 2026-09-11
Assessed by: VECTOR Agent session (post-EP-010 review)

## Context

EP-000 and EP-002 were closed `NODE_DONE` by prior sessions whose evidence does
not survive review (see the closure audit below). The question raised was
whether that work is salvageable or must be redone.

A stranded alternate implementation was found on the unmerged branch
`origin/feat/EP-001-desktop-foundation-98796407594786270`, 4 commits ahead of
`main`:

| Commit | Subject |
|---|---|
| `0a51831` | feat(domain): implement EP-002 core learning domain, planner, and question specs |
| `db9af28` | docs(graph): define phase EP-002 next node and update control plane ledger |
| `478b9dc` | feat(desktop): implement EP-001 desktop foundation workspace and core crates |
| `0a684e5` | feat(desktop): implement EP-001 desktop foundation workspace and core crates |

Since it was never merged, none of these files exist on `main`. Each was read
and assessed on its merits rather than accepted or rejected wholesale.

## Verdict per artefact

| Artefact | Verdict | Reasoning |
|---|---|---|
| `crates/vector-domain/src/subtests.rs` | **SALVAGED** | Real domain value: official two-letter codes and the correct AFQT composition (AR, WK, PC, MK). `main`'s `Subtest` enum carried neither. |
| `crates/vector-study/src/exam.rs` | **NOT SALVAGED** | Convergent with the `ExamSession` built independently in EP-005, which additionally enforces commit/lock/time-expiry. Its one advantage — typing the subtest as an enum rather than `String` — is deliverable via the salvaged `Subtest`. |
| `crates/vector-questions/src/question.rs` | **PARTIALLY USED** | `compute_hash` over id/stem/options is sound in principle and is already realised in EP-007's `ContentHash`. Its leak check is WEAKER than EP-007's and was not adopted (below). |
| `crates/vector-content/src/pack.rs` | **REJECTED** | The signature check is forgeable. See below. |
| `crates/vector-domain/src/learner_test.rs` | **REJECTED** | Vacuous test. See below. |
| `crates/vector-questions/src/question_test.rs` | **REJECTED** | Asserts a hash's *length* and trivial index equality. Proves nothing about either. |

## What was salvaged

`Subtest` (`crates/vector-domain/src/mastery.rs`) gained:

- `code()` — the official two-letter code for each subtest
- `name()` — the learner-facing full name
- `from_code()` — case-insensitive, trimming parse
- `is_afqt()` — the AFQT composition: two verbal (WK, PC) and two mathematics
  (AR, MK) subtests. The other six contribute to line scores, never to the AFQT.
- `ALL` — the canonical ten, so callers iterate rather than re-listing variants

The separate `AsvabSubtest` enum was deliberately **not** imported. Two enums for
one concept is the same duplicate-implementation problem as leaving a stub beside
its replacement, which this repository already suffers from elsewhere.

### Why `is_afqt` matters

This is not cosmetic. Scoring or readiness logic that treated all ten subtests as
AFQT inputs would compute a wrong composite. The enum previously could not
express the distinction at all.

## What was rejected, and why

### `pack.rs` — forgeable signature check

```rust
pub fn verify_signature(&self, trusted_public_key: &str) -> bool {
    if self.signature.is_empty() { return false; }
    let expected_sig_prefix = format!("SIG_{}_", trusted_public_key);
    self.signature.starts_with(&expected_sig_prefix)
}
```

This is not signature verification. Any string beginning `SIG_<pubkey>_` passes,
so the "signature" is a caller-controlled prefix with no cryptographic binding to
the manifest content. It is worse than no check, because it would let a reviewer
believe content provenance had been verified when it had not.

Recorded here so it is not re-imported from the branch by a future reader.
REQ-023 remains unsatisfied: `ContentPack::sign` on `main` is still a stub
returning the literal `"dummy_signature"`.

### `learner_test.rs` — vacuous test

```rust
let subtests = vec![Subtest::GS, Subtest::AR, /* ... ten literals ... */];
assert_eq!(subtests.len(), 10);
```

This asserts a property of a vector the test itself just constructed. It does not
call `code()`, does not reference `ALL`, and would pass unchanged if the enum had
three variants. The replacement tests assert properties of the enum's own data,
and are proven non-vacuous by mutation:

| Mutation | Result |
|---|---|
| `GS` wrongly added to `is_afqt` | DETECTED (2 tests failed) |
| `MK` wrongly removed from `is_afqt` | DETECTED (2 tests failed) |
| Two subtests given the same code | DETECTED (4 tests failed) |

## Still outstanding on EP-002

Salvage does not close EP-002. The following remain unresolved on `main`:

1. `AdaptivePlan::generate` (`crates/vector-domain/src/plan.rs`) returns a
   hardcoded `["drill_1", "drill_2"]`, ignoring mastery, goal and time (REQ-003).
   A real planner exists in `vector_study::selection` from EP-004, so this is a
   duplicate-stub problem rather than a missing feature.
2. `ContentPack::sign` returns the literal `"dummy_signature"` and force-sets
   `reviewed = true`, ignoring the key (REQ-023).
3. `unit-tests.exitcode` in this directory records `1` — a failure — while the
   node is marked `NODE_DONE`.
4. The three `.log` files referenced by this directory's `.sha256` files were
   never committed, so the hashes verify nothing.
5. No schema-compliant `anti_gaming_review.json` exists; the original (a
   three-line `{"verdict": "PASS"}` sticker) was deleted in `65ca5f4`.

## Closure audit (why these nodes are in question)

- **EP-000** was closed `NODE_DONE` by `root@vmi3357656.contaboserver.net` on a
  remote VPS, two seconds after producing an accounting that reports
  `total 484, {'PENDING': 484}`. A later session correctly re-closed it
  `CLOSED_BLOCKED` on `BLOCKED_INSUFFICIENT_EVIDENCE`; that blocked record was
  then deleted as "spurious" in `c8e3e4f`, while one of its three stated unblock
  conditions — an independent frontier audit — was never met.
- **EP-002** evidence was modified by two commits titled "EP-003" authored by
  `google-labs-jules[bot]`, which flipped `unit-tests.exitcode` from `1` to `0`
  and rewrote the hashes. Those commits are on the unmerged branch
  `jules-15445701925711240996-2fe6800f`, **not** on `main`.
