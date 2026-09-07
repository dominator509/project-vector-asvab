# Testing

Unit tests prove pure logic; integration uses real SQLite and real parsers; desktop E2E uses the actual app/artifact for ship-critical flows. Mocks can isolate external services in development but cannot establish a final claim that an integration works.

Critical properties: deterministic mastery replay; idempotent attempt submission; valid adaptive-plan termination; CAT no-backtrack invariant; monotonic timing; backup semantic round trip; content-manifest completeness; readiness cannot serialize as official score; provider environment secret-canary scrub; MCP text cannot elevate tool scope.

Failure injection: kill during answer save/migration/backup/content activation/update; disk full/read-only; malformed/corrupt packs; provider timeout/cancel/broken stream; missing client; MCP disconnect; clock shift; sleep/resume; gh auth expiry.

Every ACTIVE question must have one proven answer, objective, source lineage, review state and hash. Release uses exact artifact + full 484-registry and 42-DOD accounting.
