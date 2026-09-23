# Project VECTOR - Local-First Military Aptitude Prep Desktop

GraphLock v3.1 project. The control plane declares 60 product requirements, 15
live-fire outcomes, 11 implementation nodes, 22 release-verification stages, a
484-test registry and a 42-clause Definition of Done.

**Current state.** All 11 nodes are `NODE_DONE` and the verification sweep passes.
The release verdict is **not GO**, and five of the 60 requirements are open. Three
of them need something this repository cannot supply — a code-signing
certificate (`REQ-036`), a person using a screen reader (`REQ-038`), and
trademark clearance (`REQ-060`). Two more are work that can be done here: a local
GGUF model (`REQ-014`) and driving the broker's pull-request lane through `gh`
(`REQ-032`). Update signing and staging (`REQ-037`) is implemented: the manifest
is Ed25519-signed and the artifact is verified by digest and length before
anything is written, the transition stages beside the installation and moves the
previous artifact aside, and rolling back puts it there again. The certificate a
signed distribution needs is `REQ-036`'s external gate.

Read the generated state rather than this summary; each file below is produced by
`python3 scripts/release-state.py` and is never edited by hand.

| Question | File |
|---|---|
| What is the release verdict, and why? | `.agent/verification/reports/RELEASE_GATE.json` |
| Which artifact was verified? | `.agent/verification/state/RUN_MANIFEST.json` |
| Which requirement is not done? | `.agent/verification/REQUIREMENT_TRACEABILITY.csv` |
| Which DoD clause is not PASS? | `.agent/verification/state/DOD_STATUS.jsonl` |
| What is left, and who can do it? | `.agent/verification/reports/RESIDUAL_RISK_AND_EXTERNAL_GATES.md` |
| What evidence exists, with hashes? | `.agent/verification/state/EVIDENCE_INDEX.json` |

A green sweep means the project's own gates passed on one artifact. It does not
mean the product is releasable, and the two claims are deliberately kept in
different files.

Start with `HOW_TO_USE.md`, then `PROJECT_BRIEF.md`, `ARCHITECTURE.md`,
`AI_TRANSPORTS.md`, `QUESTION_FACTORY.md`, `SECURITY.md`, and `AGENTS.md`.

## GraphLock execution pack

Start with `HOW_TO_USE.md`, `AGENTS.md`, `COMMANDS.md`, and `.agent/GRAPH.md`.

## Restoring a working copy

A clone contains everything needed to rebuild the project: the source, both
lockfiles, the pinned toolchains (`rust-toolchain.toml`, `.nvmrc` and
`packageManager`), the icons, the migrations, and the evidence record. Nothing
required to build exists only on one machine.

Prerequisites a clone cannot supply, because they belong to the machine:

| Prerequisite | Why |
|---|---|
| `git`, Python 3, `rustup`, Node 24 | `scripts/preflight.sh` prints `ABSENT` for any that are missing |
| pnpm, via `corepack enable` | pinned by `packageManager` in `package.json` |
| MSVC build tools and the WebView2 runtime (Windows) | Tauri links against both |

Then, from the repository root, in this order:

```sh
sh scripts/install.sh                          # pinned helpers, then pnpm install --frozen-lockfile
sh scripts/preflight.sh                        # toolchain and prerequisite report
python3 scripts/validate-generated-pack.py .   # generated-pack shape
sh scripts/verify.sh                           # the full sweep, 19 gates
python3 scripts/verify-evidence-archive.py     # re-derive the committed evidence digests
```

`install.sh` must run before `verify.sh`: it provisions the dependency trees, the
cargo registry cache, and the pinned `cargo-audit` and `cargo-deny` subcommands
that the security gates invoke — none of which the Rust toolchain supplies on its
own. `preflight.sh` deliberately depends on none of that: it reports which
machine-level tools are present and is honest on a bare clone, so it is safe to run
before the install. The last command is what proves the evidence record survived
the copy — it extracts each `.agent/evidence/EP-XXX/logs.tar.gz` and checks the
restored logs against the `.log.sha256` digests committed alongside them.

Driving the broker's pull-request lane additionally needs `gh auth login` with the
`workflow` scope. Nothing else in the build does.

### What is deliberately not in the repository

These are excluded by the rules in `.gitignore`. None is needed to rebuild, and
none is a loss:

| Excluded | Why, and how it comes back |
|---|---|
| Gate logs (`*.log`) | Large, machine-specific, rewritten on every run. Each closed epoch's logs are preserved in `.agent/evidence/EP-XXX/logs.tar.gz`, which is what keeps the committed `.log.sha256` digests re-derivable. |
| `target/`, `node_modules/`, `apps/desktop/dist/` | Build output and dependency trees. `scripts/install.sh` and the build recreate them. |
| Installers (`*.msi`, `*.exe`) | Rebuilt from source. A Windows build is not bit-reproducible, so a rebuild matches in behaviour rather than byte for byte; CI keeps the bytes it built as a 90-day Actions artifact. |
| `vector.db` | The runtime learner store, not source. Carrying one learner's history into every clone is exactly why it is excluded. |
| `.env`, provider credentials | Never committed. Provider sign-in lives in the provider CLIs' own configuration directories, so this repository neither holds nor needs those secrets. |

## Licence

**Proprietary and confidential. All rights reserved.** No licence, express or
implied, is granted to this repository or anything in it — not to read it, copy
it, modify it, redistribute it, or offer it, or anything derived from it, as a
service. The terms are in `LICENSE`.

Third-party components remain under their own licences; see
`OPEN_SOURCE_LICENSES.md` and `.agent/evidence/EP-009/THIRD_PARTY_NOTICES.md`.
Nothing in those documents grants any right in VECTOR's own code.

Contributions are accepted only under a copyright assignment or an equivalent
grant; see `CONTRIBUTING.md`.

