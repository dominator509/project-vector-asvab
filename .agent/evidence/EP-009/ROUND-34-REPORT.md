# Round 34: the local GGUF model, live (REQ-014)

Node: EP-009 (release lanes). Requirements touched: REQ-014 (llama.cpp GGUF with optional
user-selected unfiltered model).

`REQ-014` read `PARTIAL_TRANSPORT_REGISTERED_MODEL_NOT_LAUNCHED`: the `local_llama` lane existed in
the transport registry, and no model had ever been launched behind it. This round launches one and
proves the product sees it.

## What was launched

| | |
|---|---|
| Runtime | llama.cpp **b11136**, asset `llama-b11136-bin-win-cpu-x64.zip`, MIT, `llama-server.exe` sha256 `48927d3b0c2391bac96a181a3b55e55f286812f162b7dd42464a5c38188ff7b4` |
| Model | `qwen2.5-0.5b-instruct-q4_k_m.gguf` from `Qwen/Qwen2.5-0.5B-Instruct-GGUF`, **Apache-2.0** (read from the repository's own metadata through the Hugging Face API *before* the download), 491,400,032 bytes, sha256 `74a4da8c9fbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db` |
| Launch | `llama-server.exe -m qwen2.5-0.5b-instruct-q4_k_m.gguf --host 127.0.0.1 --port 8099 --ctx-size 2048 --threads 4` |
| Answer | *"In one sentence: what does an ohmmeter measure?"* → **"An ohmmeter measures resistance in electrical circuits."** in 0.67 s, 11 completion tokens |

The completion was read straight from the server's OpenAI-compatible endpoint
(`POST /v1/chat/completions`), request and response both in the evidence directory with the usage
block, so the answer is the model's and not a summary of it.

## What the product reports

`VECTOR_LLAMA_ENDPOINT=http://127.0.0.1:8099 cargo run -p vector-tools -- provider probe --all-configured`:

```
health: { "state": "healthy" },  id: "local_llama",  billing: "None",  policyRefusal: null
routing: localOnly -> local_llama,  remoteAllowed -> local_llama
```

and with the server stopped:

```
health: { "state": "unavailable",
          "reason": "no llama.cpp server at http://127.0.0.1:8099 (connection timed out);
                     set VECTOR_LLAMA_ENDPOINT to a running llama.cpp server to use the local lane" }
```

That is the first time this lane has been healthy in the project's evidence, and the negative case
is recorded beside it so the health is a measurement rather than a constant.

## What this cost, and what it caught

**A duplicate probe, deleted.** I first wrote a new local-model probe in `vector-llm` with its own
environment variable, then found `probe_local_llama()` already in `tools/vector-tools/src/transports.rs`
doing the same raw-TCP `/health` exchange against `VECTOR_LLAMA_ENDPOINT`. The new module was
removed and the existing path used instead. A second mechanism for one question is drift, and the
evidence for it would have been evidence about my code rather than about the product.

**Two gate failures, both real, both fixed without weakening anything:**

- `many_persisted_attempts_stay_within_budget` failed during the full sweep. Run alone it measures
  **1.08x growth** over 2,000 durable inserts; inside the suite it tripped its 5x threshold because
  the first two hundred and fifty inserts competed with nine sibling suites on the same disk and the
  last two hundred and fifty did not. The threshold was not the problem and lowering it would have
  been: the ten timing-sensitive tests in that file now take a shared lock for the part they time,
  so the measurement gets the machine to itself.
- `change-invalidation` refused `AI_TRANSPORTS.md` as *"a changed path no gate would notice"*,
  because its surface table named `COMMANDS.md` and `README.md` individually. The table now matches
  root documents by suffix — and only at the root, so a `.md` file inside a crate stays what it is,
  source that happens to be prose.

## State

`REQ-014` moves to `DONE_GGUF_MODEL_LIVEFIRED_TRANSPORT_HEALTHY`; open requirements fall from 4 to
**3**, all of them needing something this repository cannot supply: `REQ-036` (a code-signing
certificate), `REQ-038` (a person using a screen reader), `REQ-060` (trademark clearance).

Named rather than implied, in `AI_TRANSPORTS.md`: the lane has no screen for choosing the model file
(the endpoint is the configuration and the file is the server's argument), and no tutor surface
sends a lesson question through it yet. Routing already prefers it for both local-only and
remote-allowed requests.

DoD unchanged at 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED; 21 gates exit 0 after the two fixes;
verdict `CONDITIONAL_EXTERNAL_GATES`. Corpus unchanged at 6,961 active items; pack `core-asvab v6`
active.
