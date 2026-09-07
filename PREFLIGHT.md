# Project VECTOR Preflight

Preflight is discovery, not a ceremony. Run `sh scripts/preflight.sh` before implementation and before each release candidate. A failed capability blocks only the node that requires it; unrelated work continues.

| ID | Capability | Probe | Required for | Pass condition | Failure disposition |
|---|---|---|---|---|---|
| PF-001 | Git repository | `git rev-parse --is-inside-work-tree` | all implementation | true | initialize or point executor at the real repository |
| PF-002 | Rust toolchain | `rustc --version` and `cargo --version` | desktop/core | exit 0 | EP-001 blocked; specs/tests may continue |
| PF-003 | Node package manager | `corepack pnpm --version` | React UI | exit 0 | EP-005 blocked |
| PF-004 | Tauri prerequisites | platform probe | desktop packaging | all Windows prerequisites present | packaging blocked, core work continues |
| PF-005 | SQLite | app-integrated SQLite smoke | persistence | create/write/read/close succeeds | EP-003 blocked |
| PF-006 | GitHub CLI | `gh --version` | optional repair PR lane | exit 0 | PR automation disabled; bundle export remains available |
| PF-007 | Codex native client | `codex --version` | optional repair/native LLM lane | exit 0 | that transport disabled |
| PF-008 | Grok native client | `grok --version` | optional xAI lane | exit 0 | that transport disabled |
| PF-009 | Claude native client | `claude --version` | optional Anthropic lane | exit 0 | that transport disabled |
| PF-010 | Local model runtime | configured llama.cpp health probe | local AI | real completion succeeds | AI tutor uses another permitted transport or deterministic study tools |
| PF-011 | MCP loopback | integration test | agent interoperability | initialize/list_tools/call_tool succeeds | MCP feature blocked |
| PF-012 | Network | explicit opt-in probe | research/update/provider lanes | scoped host reachable | offline core remains fully usable |
| PF-013 | Content signing | signature test key | content packs | sign then verify succeeds | release content publishing blocked |
| PF-014 | Clean-room build | disposable checkout | release | install/build/tests pass without local caches | release blocked |
| PF-015 | Windows packaging host | Windows runner or lab | shipping | signed/unsigned test artifact installs and launches | Windows release blocked |
| PF-016 | Hardware lab | defined low/mid/high profiles | performance claims | assigned profiles available | performance claim held until evidence exists |
| PF-017 | Human accessibility validation | human lane | release accessibility claim | validator assigned and evidence captured | claim remains external gate |
| PF-018 | Production active testing authorization | config | destructive/live testing | explicitly false unless separately authorized | never run destructive production tests |

## Rules

- Never turn an unavailable optional provider into an invented credential or private endpoint.
- Never access or replay browser cookies, refresh tokens, OAuth caches, or private service traffic.
- Provider-native transports are feature-gated by current provider terms and native-client capability probes.
- Real ASVAB content is never scraped from protected or leaked test banks.
- Preflight output is written to `.agent/evidence/PREFLIGHT/` with command, exit code, tool version, timestamp, and redacted environment facts.
