# SPEC-002 Data Model

SQLite is the canonical local operational store. Migrations are monotonic and reversible when feasible. Tables are partitioned by logical ownership, not by provider. Raw research is quarantined from approved content. Sensitive local profile fields are minimized. Crash payloads are separate, redacted, consent-governed records.

Required persistence tests cover migration from N-1, backup/restore, crash interruption, concurrent readers with serialized writers, foreign-key integrity, content-pack rollback, and exact readback after restart.