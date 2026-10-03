# Migration: C:\dev\project-vector-asvab → F:\dev\project-vector-asvab

Performed in one pass, copy-first and verify-before-delete, with the source left intact until the
destination had passed every check.

## What moved, and what was deliberately rebuilt instead

| | |
|---|---|
| Copied | The working tree, `.git` (92 commits of history), `.agent` evidence (including the gate logs that `*.log` ignores), sources, lockfiles, `packs/`, migrations |
| Verified | **2,167 of 2,167 files byte-identical** by SHA-256, compared one for one against the source; 0 missing, 0 differing |
| Carried separately | The shipped artifacts: `target/release/vector-desktop.exe` and both installers (`msi`, `nsis`) |
| Left behind, regenerated | `target/` (**41 GB** of incremental build cache) and `node_modules` — rebuilt from the committed lockfiles and sources on F:, which is what makes the new tree's state verifiable rather than inherited |
| Not moved | The application's own data at `%APPDATA%\com.vector.app` (the database, the pack signing key, backups). The application resolves that path from the operating system, so moving it would have broken the installation rather than migrated it. A copy is kept as a backup outside the repository |

## Verification at the new path

| Check | Result |
|---|---|
| `git rev-parse HEAD` | identical to the source's HEAD before the move |
| `git status --porcelain` | clean, `main`, remote unchanged |
| `pnpm install --frozen-lockfile` | exit 0, lockfile untouched |
| `sh scripts/build.sh` | exit 0 — binary **and** both installers produced under `F:` |
| `sh scripts/verify.sh` | **exit 0, all 21 gates recorded exit 0**, including the packaged smoke and live-fire gates, which launch the artifact from `F:` and read their effect back through IPC into the database |

The artifact digest and candidate epoch moved with the rebuild. That is expected and not a
regression: a Windows build is not bit-reproducible, which is why no gate asserts digest equality
across builds and why the sweep re-derives the identity and stamps the proof matrix every run.

## The remote had moved, and this is worth the reader's attention

The push was rejected: `origin/main` had advanced by seven commits while the migration was being
prepared, and they are not mine. `953978e` (my last push) is their ancestor, so nothing of mine was
lost — but the work in them overlaps with work in this session's history:

| Their commit | What it does |
|---|---|
| `a828807` | make the corpus source tree reproducible from a checkout |
| `1b1f56f` | derive WK difficulty and remove stem-verbatim AR/MK distractors |
| `684a382` | **dedup generated batches on question identity, not content hash** — the same fix this session landed in round 29 (`generated-questions-not-deduplicated`) |
| `67a7763` | expand AR to 14 templates and make the pack hash round-trip stable |
| merges `c595b9d`, `f5a4c6a`, `b74f61a` | pull requests #25, #26, #27 from `ops-hub/*` branches |

The migration commit was therefore rebased by reset onto `origin/main` rather than pushed over it —
no force-push, no merge, no rewritten shared history (AGENTS.md §11). The regenerated state that my
commit carried (manifest, identity, proof matrix, DoD status) was dropped and re-derived by running
the sweep on their revision instead, which is the only honest way to describe *their* artifact.

Two things a human should look at: whether the dedup fix now exists twice in the history (once here
in round 29, once in `684a382`), and whether the parallel `ops-hub` line intends to keep building on
this repository's `main`.

## What is left to do by hand

1. Point the working session at `F:\dev\project-vector-asvab` — the session's own working directory
   was the old path, so relative paths resolve nowhere until it is restarted there.
2. Delete `C:\dev\project-vector-asvab` (see below) once you are satisfied.
3. The scratch directories this session created under `C:\tmp` (the llama.cpp runtime and model, the
   msedgedriver, the clean-room clone, and several scratch databases) are **not** part of the
   project and were not moved. The clean-room clone and the scratch databases are reproducible in
   seconds; the runtime and model are recorded by digest in
   `.agent/evidence/EP-009/local-model/provenance.json` if you want them again.

## What was deleted, and when

The source tree was removed **only after** the checklist above passed on the destination, which
freed **41 GB on a C: drive that had 1.2 GB free** — the migration was as much a rescue as a move.
