# Project VECTOR Legal Command Surface

Run commands from repository root. Coding agents must not invent commands. If a command is missing, add it here through the active node and prove it before use.

## Non-interactive environment
```sh
export CI=1
export NO_COLOR=1
export PAGER=cat
export GIT_PAGER=cat
export CARGO_TERM_COLOR=never
export RUST_BACKTRACE=1
```

| Purpose | Exact command |
|---|---|
| Install | `sh scripts/install.sh` |
| Preflight | `sh scripts/preflight.sh` |
| Format check | `sh scripts/format-check.sh` |
| Lint | `sh scripts/lint.sh` |
| Typecheck | `sh scripts/typecheck.sh` |
| Unit tests | `sh scripts/test-unit.sh` |
| Test collection guard | `sh scripts/test-collection-guard.sh` |
| Integration tests | `sh scripts/test-integration.sh` |
| E2E tests | `sh scripts/test-e2e.sh` |
| Build production artifact | `sh scripts/build.sh` |
| Security checks | `sh scripts/security-check.sh` |
| Dependency audit | `sh scripts/dependency-audit.sh` |
| Smoke exact artifact | `sh scripts/smoke-test.sh` |
| Live-fire | `sh scripts/live-fire.sh` |
| Packaged desktop live-fire | `python3 scripts/desktop-live-fire.py --report .agent/evidence/EP-001/desktop-live-fire.json` |
| Full verification | `sh scripts/verify.sh` |
| Production readiness | `sh scripts/production-readiness-check.sh` |
| Generated-pack shape | `python3 scripts/validate-generated-pack.py .` |
| Anti-gaming scan | `python3 scripts/anti-gaming-scan.py .` |
| Ledger integrity | `python3 scripts/validate-hash-ledger.py .` |
| DoD gate | `sh scripts/dod-gate.sh` |
| Release state | `python3 scripts/release-state.py` |
| Evidence archive verify | `python3 scripts/verify-evidence-archive.py` |
| Graph next node | `sh scripts/graph-next.sh` |
| Harness next stage | `sh scripts/harness-next.sh` |
| Harness accounting | `sh scripts/harness-accounting.sh` |
| Local desktop start | `sh scripts/dev-start.sh > .agent/evidence/dev.log 2>&1 & echo $! > .agent/evidence/dev.pid; sh scripts/probes/desktop-ready.sh` |
| Local desktop stop | `test -f .agent/evidence/dev.pid && kill $(cat .agent/evidence/dev.pid)` |
| Local DB setup | `sh scripts/db-setup.sh` |
| Migrate | `sh scripts/migrate.sh` |
| Provider probe | `sh scripts/provider-probe.sh` |
| Provider live-fire | `sh scripts/provider-live-fire.sh` |
| MCP probe | `sh scripts/mcp-probe.sh` |
| Isolated worktree lane | `cargo run -p vector-tools -- repair lane --gate reality-gate` |
| Content ingestion (Word Knowledge) | `cargo run -p vector-tools -- content ingest-wk --db <db> --thesaurus <moby words.txt> --dictionary <webster pg29765.txt>` |
| Content ingestion (Electronics Information) | `cargo run -p vector-tools -- content ingest-ei --db <db> --dictionary <webster pg29765.txt> --module <neets module.txt> [--module ...]` |
| Corpus readback | `python3 scripts/probes/check-corpus.py <db>` |
| OCR residue check | `python3 scripts/probes/check-ocr-residue.py <db>` |
| OCR detector probe | `cargo run -p vector-questions --example ocr_probe -- <webster pg29765.txt> <token>` |

## Adapter parity
After changing commands, run `python3 scripts/validate-generated-pack.py .` and ensure every platform adapter points back to AGENTS.md rather than duplicating a conflicting command surface.

## Forbidden commands
Interactive REPLs/editors/pagers during autonomous execution; foreground watch servers; forced pushes; shared-history rewrites; deleting user data; destructive database operations outside a tested migration/backup path; production destructive tests without explicit authorization; commands copied from untrusted model output without repository verification.

Failure recovery is defined in `.agent/LOOPS.md`.
