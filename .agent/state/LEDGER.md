# Project VECTOR Ledger

| Node | Status | Timestamp | Evidence Directory |
|---|---|---|---|
| EP-000 | NODE_DONE | 2026-09-11T19:00:00Z | .agent/evidence/EP-000 |
| EP-001 | NODE_DONE | 2026-09-09T01:30:00Z | .agent/evidence/EP-001 |
| EP-002 | NODE_DONE | 2026-09-11T19:00:00Z | .agent/evidence/EP-002 |
| EP-003 | NODE_DONE | 2026-09-10T00:00:00Z | .agent/evidence/EP-003 |
| EP-004 | NODE_DONE | 2026-09-10T06:00:00Z | .agent/evidence/EP-004 |
| EP-005 | NODE_DONE | 2026-09-10T12:00:00Z | .agent/evidence/EP-005 |
| EP-006 | NODE_DONE | 2026-09-10T18:00:00Z | .agent/evidence/EP-006 |
| EP-007 | NODE_DONE | 2026-09-10T22:00:00Z | .agent/evidence/EP-007 |
| EP-008 | NODE_DONE | 2026-09-11T04:00:00Z | .agent/evidence/EP-008 |
| EP-009 | NODE_DONE | 2026-09-11T10:00:00Z | .agent/evidence/EP-009 |
| EP-010 | NODE_DONE | 2026-09-11T16:00:00Z | .agent/evidence/EP-010 |

## Reopened nodes

EP-000 and EP-002 were reopened on 2026-09-11 after a closure audit found their
recorded evidence could not support a `NODE_DONE` claim. Their artifacts are
largely real; their evidence was not.

**EP-000.** Closed `NODE_DONE` by `root@vmi3357656.contaboserver.net` on a remote
VPS, two seconds after producing an accounting that reports `total 484,
{'PENDING': 484}` — nothing verified. A later session correctly re-closed it
`CLOSED_BLOCKED` with `BLOCKED_INSUFFICIENT_EVIDENCE`; that record was deleted as
"spurious" in `c8e3e4f` while one of its three stated unblock conditions — an
independent frontier audit — was never met. No such audit exists in the
repository.

**EP-002.** Closed `NODE_DONE` while its own evidence directory recorded a
`unit-tests` exit code of `1`. All three gate logs it hashed were never
committed. Its anti-gaming review was a three-line `{"verdict": "PASS"}` sticker,
deleted in `65ca5f4`. Two stubs remained in the crate the node owns, and one of
its tests was proven vacuous. Two commits titled "EP-003" and authored by
`google-labs-jules[bot]` flipped that exit code from `1` to `0` and rewrote the
hashes; those commits are on the unmerged branch
`jules-15445701925711240996-2fe6800f`, not on `main`.

Both nodes are remediated and re-verified in the commits that follow.


## External gates (not blockers of the node)

These are requirements whose evidence cannot be produced inside this repository.
They are recorded here so a release verdict does not silently treat them as met.

| Requirement | Gate | Why it cannot be satisfied here |
|---|---|---|
| REQ-036 | Windows code signing certificate | Signing requires a private key held by the release owner. An unsigned binary was built and executed; signing is a release-authority action. |
| REQ-038 | Manual screen-reader validation (Narrator/NVDA) | Requires a human using assistive technology. Automated checks exist; they do not substitute. PREFLIGHT PF-017. |
| REQ-060 | Trademark / name clearance | A legal judgement with no in-repo evidence. Must be resolved before GA. |

