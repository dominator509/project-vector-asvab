# Round 31: the last in-repo security finding, and the toolchain that carried it

Node: EP-009 (release lanes). Requirements touched: REQ-041 (SBOM and licence release gate),
REQ-039/REQ-040 (harness and diagnostics).

## The finding

`pnpm audit` reported **1 critical / 2 high / 5 moderate**, all in the frontend toolchain:

| Package | Vulnerable | Patched | What it is |
|---|---|---|---|
| esbuild | ≤ 0.24.2 | ≥ 0.25.0 | the critical one: the dev server accepts requests from any website |
| vite | ≤ 6.4.2 | ≥ 6.4.3 | two advisories: optimized-deps path traversal, and NTLMv2 hash disclosure through UNC path handling on Windows |
| vitest / @vitest/mocker | ≥ 2.1.0 < 4.1.11 | ≥ 4.1.11 | path traversal / arbitrary file read through a redirect mock |
| playwright | < 1.55.1 | ≥ 1.55.1 | high |

None of them ships: the artifact is a Rust binary with a bundled static frontend, and
`pnpm audit --prod` was already clean. They were still real, and the repository is public, so the
Dependabot count on the default branch was real too.

## The upgrade, and what it cost

| | before | after |
|---|---|---|
| vite | 5.4.x | **8.3.0** |
| vitest | 2.1.9 | **5.0.1** |
| @playwright/test | 1.49.1 | **1.63.0** |
| @vitejs/plugin-react | 4.x | **6.1.1** |
| esbuild | 0.24.x (critical) | **not installed**: vite 8 declares it an optional peer dependency and no longer pulls it in, which is why that advisory is gone rather than patched in place |

Three major versions of the build tool and two of the test runner. Everything was re-verified
rather than assumed: `pnpm typecheck` exit 0, **217 unit tests in 12 files**, **9 integration
tests**, `vite 8.3.0` build exit 0, **37 Playwright tests on 1.63.0** (the browser was re-installed
for the new version), and then the full sweep: 21 gates, 0 failed, 0 blocked. `pnpm audit` reports
**no known vulnerabilities** for the full tree and for `--prod`.

The check is networked, so it is documented as a manual check in COMMANDS.md rather than added to
the sweep: the local gate covers Rust advisories (`cargo deny`) and npm licences (SBOM). Its result
is written to `.agent/evidence/EP-009/npm-advisories.json`, and `release-state.py` reads that file
into the DOD-021 record -- so the accounting states the advisory position and the versions it was
measured against, and a stale audit cannot pass as a current one.

## What the sweep caught on the way

Two things, both worth recording because both were the harness doing its job:

- **The dependency graph refused three rows.** Rewriting the `blocked_or_na_reason` of `E2E-011`,
  `SUP-004` and `E2E-018` to describe what has since been executed removed the prose the classifier
  reads, so three blocked rows no longer named what blocks them -- which is precisely what
  DOD-031's graph exists to refuse. The fix is not a new pattern: a reason may now name its blocker
  explicitly as `blocked-by: <kind>:<name>`, and those three use it. A declaration that survives
  its prose being rewritten.
- **The change-invalidation graph re-ran the right things.** The lockfile change mapped to the
  `dependencies` surface and pulled `dependency-audit`, `build` and everything downstream of the
  artifact into the rerun list, which is why the sweep that followed the upgrade was the one whose
  evidence counts.

## Where this leaves the project

| | round 30 | round 31 |
|---|---|---|
| DoD clauses | 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED | **34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED** (DOD-021's record now carries the advisory position) |
| Gates | 21 exit 0 | **21 exit 0** |
| npm advisories | 1 critical / 2 high / 5 moderate | **0** |
| Artifact | `f75d5bc8…` (epoch 19) | `23f8c2c5…` (epoch 21) |

Corpus unchanged at 6,961 active items; pack `core-asvab v6` active; every provenance invariant a
zero. Verdict `CONDITIONAL_EXTERNAL_GATES`.

What is left in-repo is the three open requirements that are work here rather than external gates:
REQ-037 (signature verification for updates -- the certificate itself is external), REQ-032 (`gh`
pull-request lane), REQ-014 (local GGUF model).
