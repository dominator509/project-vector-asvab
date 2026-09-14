# Decisions

| ADR | Decision | Why |
|---|---|---|
| ADR-001 | Tauri 2 + React/TS + Rust | local footprint, strong native core, accessible web UI |
| ADR-002 | SQLite canonical state | zero-admin transactions, robust backup, public-domain engine |
| ADR-003 | Local-first core/no account | privacy, resilience, low recurring cost |
| ADR-004 | Original question factory | test integrity + ownable commercial corpus |
| ADR-005 | Native-client transporter | use subscription economics without token theft/replay |
| ADR-006 | Gemini CLI OAuth bridge disabled | current Google terms prohibit that bridge |
| ADR-007 | Evidence Vault canonical | keeps cloud notebooks/models replaceable |
| ADR-008 | MCP client + server | agent/writing-tool interoperability |
| ADR-009 | Repair produces reviewed PR | reversible, testable, auditable; no self-modifying installed app |
| ADR-010 | No precise predicted score before calibration | prevent pseudo-scientific claims |
| ADR-011 | SaaS via future disabled SyncPort | preserve local product architecture |
| ADR-012 | GRAPH.md declares the full EP-000..EP-010 chain, not a truncated prefix | graph truncation orphaned EP-003..EP-010 and made graph-next report NO_READY_NODE while execplans/specs existed; ROADMAP.md and `.agent/execplans/` both define the 11-node chain, so the registry must match them; a truncated graph silently converts "not yet built" into "nothing to build" |

## ADR-012 — Detail

**Context.** `ROADMAP.md` and `.agent/execplans/` define eleven nodes (EP-000 discovery →
EP-010 production readiness). Commit `fe41478` declared all eleven rows in `.agent/GRAPH.md`.
Commit `089f4c6` re-declared them after a revert, noting the guard should prevent cumulative-delta
reverts. At the time of this ADR the committed `GRAPH.md` contains only EP-000, EP-001, EP-002, so
`scripts/graph-next.sh` returns `NO_READY_NODE` while nine execplans remain unimplemented.

**Decision.** Restore the canonical eleven-node chain with strict linear dependencies
(EP-00N DEPS EP-00N-1). The execplans, not the truncated table, are authoritative for scope.

**Consequences.** `graph-next.sh` resumes returning a single READY node (EP-003). Only that node may
be implemented. The restoration is a control-plane change only: it adds no production behavior and
claims no node status. EP-003 remains `PENDING` until it independently satisfies `.agent/DONE_LAW.md`.

**Note on stranded work.** Commit `3164c80` (branch `jules-15445701925711240996-2fe6800f`) claims
EP-003 `NODE_DONE` and is not an ancestor of `main`. It is not merged or credited by this ADR. Its
claims are verified independently under EP-003 acceptance; unverified claims carry no authority.
