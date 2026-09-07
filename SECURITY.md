# Security and test-integrity model

Trust boundaries: webview/Rust commands; DB/import-export; app/provider child process; MCP peer; source fetch/Evidence Vault; crash bundle/coding agent/GitHub; updater/release channel.

Required controls: strict Tauri command allowlist/schema; no raw-shell UI API; canonicalized sandbox paths; provider credential isolation; egress privacy classifier; MCP peer scopes and write consent; prompt-injection/tool-confusion defenses; content signatures/hashes; transactional/idempotent attempt writes; update signature/hash + rollback; dependency/license/secret/SBOM gates; double-redaction and secret-canary tests.

Protected/current official ASVAB question text is neither a content source nor a tutor memorization target.

Security tests cover malicious model paths, provider-binary impersonation, argument injection, zip-slip, content tamper, SQL corruption, update rollback attack, XSS in lesson markup, source/MCP prompt injection, crash secret leakage, repair-prompt injection, race/double-submit and stale-policy behavior.
