# Environment

Developer baseline: Git, Python 3 for GraphLock, pinned Rust/Cargo, pinned Node/pnpm, Tauri/WebView2 platform prerequisites. Optional: llama.cpp, Grok CLI, Codex CLI, Claude Code, gh, NVDA and signing tools.

Runtime data is separated into DB, content, backups, logs, crashes, model metadata, cache and exports using OS app-data APIs. Provider-owned credential directories are outside VECTOR and never read.

Network modes: Offline (core only); Source Update (approved sources); Provider (selected official client); Repair (separate consent and restricted repository networking).
