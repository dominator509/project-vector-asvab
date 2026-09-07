# Crash and repair pipeline

## Local crash envelope
Capture app version/git SHA/artifact hash/content/schema versions; sanitized OS/hardware facts; stack/minidump where supported; bounded redacted structured event ring; DB integrity/migration status; provider adapter versions/status without tokens; replay seed/repro recipe; expected vs observed behavior; file/consent manifest.

Bundle: deterministic archive with `manifest.json`, `diagnostic.json`, `events.ndjson`, `repro.yaml`, `environment.json`, optional dump and consent record.

## Repair broker
1. User reviews what leaves the machine and chooses a coding-agent transporter.
2. Verify repository state and create isolated branch/worktree.
3. Give agent only repository rules, scoped crash evidence and affected-code map.
4. Reproduce before edit when feasible.
5. Fix adds regression proof and runs targeted tests, `verify`, anti-gaming and applicable GraphLock gates.
6. Record diff, commands, exits and evidence hashes.
7. User previews patch.
8. Official `gh` may create issue/PR only after explicit approval. No default auto-merge or production deploy.

Repair prompts/logs are untrusted. Agent cannot read provider credential stores, unrelated home paths or learner production DBs.
