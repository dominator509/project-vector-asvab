# Commercial dependency strategy

This document covers **third-party** components and the terms under which
VECTOR may use them. VECTOR itself is not open source: it is proprietary and
confidential, and no licence is granted to it — see `LICENSE`. Nothing here is
a grant of any right in VECTOR's own code.

| Component | Role | Research posture | Release rule |
|---|---|---|---|
| Tauri | desktop shell | MIT/Apache-2.0 code | exact pin; do not reuse restricted logo assets |
| SQLite | database | public domain | record provenance/build |
| llama.cpp | GGUF runtime | MIT | exact pin + notices |
| fsrs-rs | review scheduling | BSD-3-Clause candidate | exact pin |
| MCP Rust SDK | MCP | Apache-2.0 | exact pin/protocol conformance |
| GitHub CLI | PR transporter | MIT | execute official binary; auth remains gh-owned |
| Codex repository | coding transport | Apache-2.0 code; service terms separate | official client only |
| Model weights | local inference | model-specific | separate license/redistribution review |

Do not use a GPL/AGPL LMS as a shortcut without an explicit distribution-license decision. Do not confuse a repository code license with permission to redistribute trademarks, model weights, or datasets/content.
