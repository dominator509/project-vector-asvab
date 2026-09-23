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
| Dependency audit | `sh scripts/dependency-audit.sh` (Rust advisories via `cargo deny`, npm licences via the SBOM -- both local) |
| npm advisory check | `pnpm audit` (contacts the registry, so it is a documented manual check rather than a sweep gate; the result is recorded in `.agent/evidence/EP-009/npm-advisories.json`, which the release accounting reads) |
| Smoke exact artifact | `sh scripts/smoke-test.sh` |
| Live-fire | `sh scripts/live-fire.sh` |
| Packaged desktop live-fire | `python3 scripts/desktop-live-fire.py --report .agent/evidence/EP-001/desktop-live-fire.json` |
| Runtime canary proof | `python3 scripts/probes/canary-proof.py` (a learner name and target drawn from the operating system's CSPRNG at run time, propagated through the study path and read back by a separate connection, with a negative control) |
| Recovery objectives | `python3 scripts/probes/recovery-objectives.py` (injects truncated, corrupted and deleted stores, restores each from a verified archive, and measures RTO/MTTR against the declared target) |
| Bounded soak | `python3 scripts/probes/soak.py --minutes <n>` (packaged self-check plus a golden path per iteration, with a heartbeat, integrity check and row census; labels itself an abbreviated trial) |
| Full verification | `sh scripts/verify.sh` (runs every gate and continues past a failure, marking only the gates that consume a failed artifact as `BLOCKED_PREREQUISITE`; exits non-zero if anything failed or was blocked) |
| Dependency blocker graph | `python3 scripts/dependency-graph.py` (derives which prerequisite or capability each blocked test waits on, and refuses a blanket block) |
| Change invalidation graph | `python3 scripts/change-invalidation.py` (diffs the previous epoch's commit against this candidate and refuses a changed path no gate covers) |
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
| Content ingestion (Paragraph Comprehension) | `cargo run -p vector-tools -- content ingest-pc --db <db> --work <gutenberg ebook number>=<text file> [--work ...]` |
| Content ingestion (Shop / Auto Information sources) | `cargo run -p vector-tools -- content ingest-tools --db <db> --subtest <SI\|AI> --ask <tools\|functions> --dictionary <webster pg29765.txt> --work <source>:<id>:<title>=<text file> [--work ...]`, where `<source>` is `archive` or `gutenberg` and decides the page the citation names |
| Content ingestion (factual: GS, SI, AI) | `cargo run -p vector-tools -- content ingest-facts --db <db> --subtest <GS\|SI\|AI> --dictionary <webster pg29765.txt> --work <gutenberg ebook number>=<text file> [--work ...]` |
| Generate the computable subtests into the corpus | `cargo run -p vector-application --example generate_corpus -- <db> <SUBTEST=count>... [seed]` (Arithmetic Reasoning, Mathematics Knowledge and Mechanical Comprehension have templates rather than sources; this runs the application's own verify-then-store pipeline with fixed seeds, so the pack carries them and a plan can name their objectives) |
| Paragraph Comprehension corpus probe | `cargo run -p vector-questions --example ingest_pc -- <label>=<text file> [<label>=<text file> ...] [count]` |
| Factual corpus probe | `cargo run -p vector-questions --example ingest_facts -- <label>=<text file> [...] [count]` |
| Shop / Auto Information corpus probe | `cargo run -p vector-questions --example ingest_tools -- [names] [tools\|functions] <label>=<file.txt> [...] [count]` |
| Download a Project Gutenberg work | `python3 scripts/probes/fetch-gutenberg.py <ebook-number>:<name> [...]` (reads the id out of the file's own header and refuses a download whose number disagrees) |
| List an Internet Archive item | `python3 scripts/probes/fetch-archive-text.py --list <archive id>` |
| Generated item sampler | `cargo run -p vector-questions --example mc_probe` |
| Factual ingestion report | `python3 scripts/probes/facts-report.py <ingest-facts report.json>` |
| Definition / purpose mining probes | `python3 scripts/probes/purpose-probe.py <work.txt>` (also `definition-rule-probe.py`, `class-noun-probe.py`, `corroboration-probe.py`, `mine-definitions.py`) |
| Corpus readback | `python3 scripts/probes/check-corpus.py <db>` |
| Corpus counts per subtest | `python3 scripts/probes/count-items.py <db>` |
| Ingestion report rendered | `python3 scripts/probes/pc-report.py <ingest report.json>` |
| Source URL provenance | `python3 scripts/probes/verify-gutenberg-provenance.py --manifest <id=path manifest>` |
| Passage containment audit | `cargo run -p vector-questions --example pc_passage_audit -- <label>=<work.txt> [...] [limit]` |
| Apparatus survey of a work | `python3 scripts/probes/survey-apparatus.py <work.txt> [...]` |
| Rebuild a corpus subtest | `python3 scripts/probes/rebuild-pc-corpus.py <db> --subtest PC` (refuses to delete an item a learner has attempted) |
| Rebuild the whole corpus | `python3 scripts/rebuild-corpus.py <db>` (deletes the items, prunes a vault record whose URL does not resolve to its bytes, then re-ingests every ingested subtest from its sources, generates the computable ones, and writes the round's reports; `--dry-run` reports first) |
| Learner-facing damage check | `python3 scripts/probes/foreign-letters.py <db>` (letters from another alphabet and undecodable bytes) |
| Description-shape survey | `python3 scripts/probes/description-shapes.py <text file> [...]` (counts the `X is designed to Y` shapes the reader cannot read yet) |
| Relation-shape survey | `python3 scripts/probes/relation-shapes.py <text file> [...]` (every verb phrase the sources state the relation with, including the ones the reader cannot read) |
| Purpose-opening survey | `python3 scripts/probes/purpose-verbs.py <text file> [...]` (every distinct word a purpose opens with, which is how the verb list was written) |
| Download an Internet Archive text | `python3 scripts/probes/fetch-archive-text.py <archive id> [...]` (fetches the item's own `_djvu.txt` and prints its digest) |
| Mutation proof for the content rules | `python3 scripts/probes/mutation-round18.py` (disables one rule at a time and requires its test to fail) |
| Generate a pack signing key | `cargo run -p vector-tools -- pack-keygen --key <key file>` (refuses to overwrite; keep it out of the repository) |
| Build a signed content pack | `cargo run -p vector-tools -- pack-build --db <db> --out <pack file> --name core-asvab --version 1 --key <key file> [--curriculum <nodes.json>] [--calibration <entries.json>]` |
| Write the corpus curriculum | `python3 scripts/probes/build-curriculum.py <db>` (one node per objective, prerequisites in learning order, and a calibration entry per objective that says plainly it rests on no responses) |
| Install a signed content pack | `cargo run -p vector-tools -- pack-install --db <db> --pack <pack file> --trusted-signer <public key hex> --app-version 0.1.0` |
| List installed packs | `cargo run -p vector-tools -- pack-list --db <db>` |
| Roll a pack back | `cargo run -p vector-tools -- pack-rollback --db <db> --name core-asvab` |
| Generate the update signing key | `cargo run -p vector-tools -- update update-keygen --key <key file>` (refuses to overwrite; keep it out of the repository) |
| Build an update manifest | `cargo run -p vector-tools -- update update-manifest --artifact <file> --version 0.2.0 --min-running 0.1.0 [--notes-url <url>] --out <manifest.json>` (the digest and size come from the file itself) |
| Sign an update manifest | `cargo run -p vector-tools -- update update-sign --manifest <manifest.json> --key <key file> --out <signed.json>` |
| Verify an update | `cargo run -p vector-tools -- update update-verify --signed <signed.json> --artifact <file> --trusted-signer <public key hex> --running 0.1.0` (refuses an untrusted signer, an edited manifest, a digest or size mismatch, and an offer that is not newer) |
| Stage and apply an update | `cargo run -p vector-tools -- update update-stage --signed <signed.json> --artifact <file> --trusted-signer <hex> --running 0.1.0 --install-dir <dir> --target <dir>/vector-desktop.exe` (stages beside the installation, moves the previous artifact aside; a running executable is never replaced) |
| Roll an update back | `cargo run -p vector-tools -- update update-rollback --target <dir>/vector-desktop.exe` |
| Activate a registered pack version | `cargo run -p vector-tools -- pack-activate --db <db> --name core-asvab --version 6` (the way back from a rollback; `pack-install` deliberately never undoes a rollback, and this supersedes the active version rather than quarantining it) |
| OCR residue check | `python3 scripts/probes/check-ocr-residue.py <db>` |
| OCR detector probe | `cargo run -p vector-questions --example ocr_probe -- <webster pg29765.txt> <token>` |

## Adapter parity
After changing commands, run `python3 scripts/validate-generated-pack.py .` and ensure every platform adapter points back to AGENTS.md rather than duplicating a conflicting command surface.

## Forbidden commands
Interactive REPLs/editors/pagers during autonomous execution; foreground watch servers; forced pushes; shared-history rewrites; deleting user data; destructive database operations outside a tested migration/backup path; production destructive tests without explicit authorization; commands copied from untrusted model output without repository verification.

Failure recovery is defined in `.agent/LOOPS.md`.
