# How to use

1. Extract into a new repository root.
2. Read PROJECT_BRIEF, RESEARCH, ARCHITECTURE, AI_TRANSPORTS, QUESTION_FACTORY, SECURITY and AGENTS.
3. Run `python3 scripts/validate-generated-pack.py .`.
4. Run `sh scripts/preflight.sh`.
5. Run `sh scripts/graph-next.sh`; execute only its eligible EP node from `.agent/execplans/`.
6. EP-000 re-verifies current ASVAB facts, provider terms, dependency versions and licenses before dependency installation.
7. Do not skip to UI/AI: domain, evidence and content correctness are foundational.
8. EP-010 freezes candidate SHA/artifact digest and runs V-000..V-021. GO requires real internal and mandatory external evidence.

Provider login is always done in official provider clients. VECTOR detects their documented capability but never copies OAuth state. Gemini CLI OAuth relay stays disabled.

A user-selected unfiltered local GGUF is permitted, but the model does not gain authority to bypass source, correctness, privacy, filesystem, MCP or controlled-question policies.
