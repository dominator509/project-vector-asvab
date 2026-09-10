# DEFINITION_OF_DONE.md

## Completion Checklist
1. All unit tests must pass.
2. Requirements mapped in `FUNCTIONAL_PROOF_MATRIX.csv` must be recorded as `DONE_VERIFIED` with a valid `evidence_path`.
3. Requirements mapped in `REQUIREMENT_TRACEABILITY.csv` must be recorded as `DONE`.
4. Preflight script (`scripts/preflight.sh`) must pass cleanly.
5. `validate-generated-pack.py` must pass cleanly.
6. The DoD registry scan (`scripts/dod-gate.sh`) must run successfully.
7. Real evidence must be recorded under `.agent/evidence/<node>/`.
8. The ledger (`.agent/state/LEDGER.md`) must be appended with `NODE_DONE`.
9. The graph registry (`.agent/GRAPH.md`) must point to the new node explicitly.

## Anti-patterns
- Generating dummy test data in production code.
- Implementing non-owned requirements.
- Masking failures or bypassing checks.
- Editing requirements without consensus.

## Why
This file defines exactly what DONE means per node class to enforce high quality and maintain strict execution discipline across the board. Anti-patterns void work because they circumvent verification or create technical debt.
