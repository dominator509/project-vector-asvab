# Capability Matrix

| Capability | Default | Proof before use | Degradation |
|---|---|---|---|
| Offline study/data | REQUIRED | local E2E | release blocked if absent |
| llama.cpp local inference | OPTIONAL-BUILT-IN-ADAPTER | real local completion | deterministic study tools remain |
| xAI Grok native client | OPTIONAL | installed official client + supported auth + terms gate | adapter disabled |
| OpenAI Codex native client | OPTIONAL | installed official client + ChatGPT/native auth + process test | adapter disabled |
| Anthropic Claude native client | OPTIONAL-CONDITIONAL | current terms/capability review + official client probe | adapter disabled |
| Gemini CLI OAuth bridge | DISALLOWED | policy gate intentionally rejects | use local/provider alternatives |
| Gemini Notebook companion | OPTIONAL | export path or authorized Enterprise API contract | evidence vault remains canonical |
| MCP server/client | REQUIRED FEATURE | loopback positive/negative capability test | external-agent connectivity blocked |
| GitHub PR automation | OPTIONAL | gh auth + repo permission + dry-run branch/PR evidence | export repair bundle manually |
| Remote telemetry | OPT-IN | redaction + consent + endpoint proof | local diagnostics only |
