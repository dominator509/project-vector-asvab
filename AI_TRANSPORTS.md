# AI transport architecture

## ModelTransport
Every adapter exposes: ID, binary/version, auth owner, billing mode, structured output, streaming, tool/MCP ability, web/search ability, privacy class, terms-verified date, automation status and health.

## Hard rules
1. Never open/copy/replay Grok, Codex, Claude, browser-cookie or other provider OAuth credential stores.
2. Invoke only documented provider-owned binaries/protocols or an explicitly enabled documented API.
3. Authentication happens in provider UI/CLI.
4. Scrub API-key environment variables from subscription/native-client lanes unless the user deliberately chose API mode.
5. A signed/datable provider-policy record can disable an adapter when terms become stale or incompatible.
6. Local-only requests cannot route to network providers.
7. Provider output is untrusted and schema/bounds checked.

## Matrix
| Adapter | Status | Transport |
|---|---|---|
| `local_llama` | default-capable | llama.cpp process/server + user-selected GGUF |
| `grok_native` | enabled when official client/login healthy | documented headless CLI or ACP |
| `codex_native` | enabled when official client/login healthy | documented `codex exec`/supported server interfaces |
| `claude_native` | conditional | official Claude Code noninteractive mode only while terms registry permits |
| `gemini_cli_oauth` | disabled | prohibited third-party OAuth bridge |
| `gemini_api` | optional user-paid fallback | documented API only |
| `gemini_notebook_enterprise` | optional licensed | documented enterprise API |

## Local unfiltered mode
A user may choose a less-filtered/unfiltered GGUF. That changes model behavior, not application authority. Evidence, content-provenance, anti-leak, filesystem, MCP and correctness boundaries remain enforced outside the model.
