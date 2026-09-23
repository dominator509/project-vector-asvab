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

## The local lane, as it actually runs

The `local_llama` lane is a llama.cpp server on loopback plus a user-selected GGUF file. Nothing in
the product starts or manages the server: the endpoint is the configuration, and `VECTOR_LLAMA_ENDPOINT`
says where it is (default `http://127.0.0.1:8080`). The provider probe asks `/health` over a socket
and reports the lane healthy while a server answers, unavailable with the reason once it stops.

Verified on this machine (round 34, `.agent/evidence/EP-009/local-model/`): llama.cpp b11136 (MIT)
with `qwen2.5-0.5b-instruct-q4_k_m.gguf` from `Qwen/Qwen2.5-0.5B-Instruct-GGUF` (Apache-2.0),
answering *"An ohmmeter measures resistance in electrical circuits."* to a study question in 0.67 s.
The probe reported `healthy` with the server up and `unavailable` with the reason -- *"no llama.cpp
server at http://127.0.0.1:8099 (connection timed out)"* -- once it stopped.

What the lane does not have yet: a screen for choosing the model file (the endpoint is the
configuration and the file is the server's argument), and a tutor surface that sends a lesson
question through it. The routing decision already prefers it for both local-only and remote-allowed
requests.

## Local unfiltered mode
A user may choose a less-filtered/unfiltered GGUF. That changes model behavior, not application authority. Evidence, content-provenance, anti-leak, filesystem, MCP and correctness boundaries remain enforced outside the model.
