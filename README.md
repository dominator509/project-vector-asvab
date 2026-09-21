# Project VECTOR - Local-First Military Aptitude Prep Desktop

GraphLock v3.1 project. The control plane declares 60 product requirements, 15
live-fire outcomes, 11 implementation nodes, 22 release-verification stages, a
484-test registry and a 42-clause Definition of Done.

**Current state.** All 11 nodes are `NODE_DONE` and the verification sweep passes.
The release verdict is **not GO**, and six of the 60 requirements are open. Three
of them need something this repository cannot supply — a code-signing
certificate (`REQ-036`), a person using a screen reader (`REQ-038`), and
trademark clearance (`REQ-060`). The other three are work that can be done here:
an update mechanism with signature verification (`REQ-037`), a local GGUF model
(`REQ-014`), and driving the broker's pull-request lane through `gh` (`REQ-032`).

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


<!-- probe -->
