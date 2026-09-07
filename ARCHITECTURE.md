# Architecture

## Shape
A Windows-first local desktop modular monolith: Tauri 2 + React/TypeScript front end, Rust application/core, SQLite canonical store, optional out-of-process AI/provider workers. Cloud is adapter-only; local state remains canonical.

## Planned repository
```text
apps/desktop/
crates/vector-domain/
crates/vector-application/
crates/vector-persistence/
crates/vector-study/
crates/vector-content/
crates/vector-questions/
crates/vector-llm/
crates/vector-mcp/
crates/vector-observability/
crates/vector-repair/
crates/vector-platform/
content/
tests/
tools/
```

## Import law
`vector-domain` imports no infrastructure. `vector-application` owns use-case ports and may import domain. Study/questions remain pure where possible. Persistence/LLM/MCP/observability/platform implement ports; pure layers never import them. Desktop is composition root. Repair tooling has a lower privilege profile than learner runtime.

## Architectural invariants
- ARCH-001: An LLM response alone can never establish answer correctness.
- ARCH-002: Provider authentication remains provider-owned; VECTOR never reads provider token stores.
- ARCH-003: Retrieved source/MCP/provider text is untrusted data and cannot grant tool authority.
- ARCH-004: Core study writes commit locally before optional external side effects.
- ARCH-005: Mutable policy/test/composite/job facts have source ID, version and freshness.
- ARCH-006: Every ACTIVE question has immutable answer proof, objective, provenance, review state and content hash.
- ARCH-007: ReadinessEstimate is a distinct type from user-entered/official score data.
- ARCH-008: All imports cross bounded parser/quarantine boundaries.
- ARCH-009: Diagnostics redact at event emission and bundle export.
- ARCH-010: Repair agents run in isolated git worktrees, never against installed learner state.
- ARCH-011: MCP is read-only by default and explicit-consent for writes.
- ARCH-012: AI, indexing, imports and backups are cancellable and off the UI thread.
- ARCH-013: Release identity binds binary, git SHA, migrations, active content packs and SBOM.
- ARCH-014: Future SaaS SyncPort is disabled initially and cannot silently overwrite local study history.

## Key flows
**Practice:** UI -> application use case -> deterministic selector -> content repository -> transactional attempt -> mastery projection/outbox -> UI.

**Tutor:** UI -> evidence retrieval -> privacy classifier -> router -> transport -> schema validation -> source binding -> UI. Tutor never mutates mastery merely by speaking.

**Source update:** fetch/import -> quarantine -> parse -> trust/license -> diff -> validate -> signed candidate -> explicit activation -> immutable version pointer.

**Crash repair:** flight recorder -> redacted repair bundle -> consent -> isolated worktree -> coding-agent transport -> reproduce -> regression test -> fix -> GraphLock -> user patch review -> approved `gh` PR.

## Future SaaS seam
Define stable event IDs and `SyncPort`, but ship disabled. A later cloud tier can provide encrypted multi-device sync/licensing/tutor-team features without rewriting the offline domain.
