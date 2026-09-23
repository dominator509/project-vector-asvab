# Round 35: can the packaged application be driven? (DOD-004)

Node: EP-009 (release lanes). Clause touched: DOD-004.

DOD-004's residual is that the Playwright suite runs against the built frontend bundle with a
stubbed command boundary, not inside the packaged executable. This round went after that residual
with the documented tooling, and came back with a measurement rather than a claim.

## What was set up

| | |
|---|---|
| Driver | `tauri-driver 3.0.0-alpha.0` (crates.io, Apache-2.0 OR MIT), installed from source in 54 s, started with `--native-driver` |
| Native driver | `msedgedriver 153.0.4234.48`, downloaded from Microsoft to match the installed WebView2 runtime **153.0.4234.48** exactly -- the version-match the Tauri documentation calls out as the cause of hangs when it is wrong |
| Client | `scripts/probes/packaged-journey.py`, which speaks the WebDriver protocol over `urllib` rather than adding a test-runner dependency, for the same reason the local-model probe speaks HTTP to llama.cpp |

The driver reported `"msedgedriver ready for new sessions"` and created sessions against the release
binary.

## What it measured

```
session   created, one window handle
source    <html><head></head><body></body></html>   (39 characters)
polling   every 3 s for 30 s: unchanged at every sample
```

So: **the packaged window is reachable and its document is not exposed.** The driver attaches, the
window handle exists, `GET /source` answers -- with an empty document that never fills in. A user
journey cannot be driven inside the packaged process in this environment.

## What that does and does not change

- The packaged process *does* reach the Rust command layer, and that is already proven: launching
  the release binary writes a `ui_ready` marker through the real IPC path and a separate process
  reads it back out of the application database (the `live-fire` gate).
- The browser suite's claim is different and stays as it was: it drives the built bundle with a
  stubbed command boundary.
- **DOD-004 stays PARTIAL**, with the measurement above as its evidence rather than a vague
  "not done".

The probe ships anyway, and says in its own docstring that it measured an empty document here: it
is the tool the clause needs, it fails loudly with a report instead of pretending, and on a machine
whose driver does expose the document it is the missing half -- create a learner through the real
onboarding form, read the plan, answer a question served from the installed pack, and read the
attempt back out of the application's own database, with the real Rust layer behind every step and
no stub anywhere.

## State

DoD unchanged at 34 PASS / 7 PARTIAL / 1 EXTERNAL_REQUIRED (DOD-004's record now carries the
measurement); 3 requirements open, all external. Corpus unchanged at 6,961 active items; pack
`core-asvab v6` active; verdict `CONDITIONAL_EXTERNAL_GATES`.

What is left is the closure accounting: the seven PARTIAL clauses and one EXTERNAL_REQUIRED clause
each with a named owner, and the statement of what this repository can and cannot prove.
