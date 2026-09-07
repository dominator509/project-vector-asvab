# Verification Harness Laws

1. Account for all 484 master registry IDs; never prune irrelevant rows.
2. A blocked capability blocks only tests that actually require it.
3. Required test collection of zero is a failure.
4. PASS requires observed command evidence and appropriate artifact/readback proof.
5. SKIPPED_NOT_APPLICABLE requires V-003 evidence proving the subsystem is absent.
6. Candidate epochs are immutable; a code/config/dependency change creates a new epoch and invalidates affected evidence.
7. The release artifact is hashed once; later gates must use that exact digest.
8. No mocked external provider is sufficient for a real-dependency release claim.
9. Mutation proof demonstrates important tests fail when actual behavior is broken.
10. The final ledger is hash chained and independently validated.
