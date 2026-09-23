# Clean-room run: the published commands, executed in a clone (DOD-002, DOD-023)

Rule under test.

- **DOD-002**: *the repository builds from a clean checkout using the committed frozen/locked
  dependency files and declared toolchain.*
- **DOD-023**: *README commands, examples, quickstarts, install, upgrade, deployment, rollback and
  operator instructions are executed exactly as published in clean environments.*

## What was done

A fresh clone of the repository at commit `a1de4c02117c4c0ae6d12a91ae078190b2208487`, with no
build cache (`target/`), no `node_modules/` and no evidence residue — `git clone` produces exactly
the committed tree — and then the five commands the README publishes under *Restoring a working
copy*, in the published order, with the published flags and no others:

```
git clone <repository> C:\tmp\vector-cleanroom
sh scripts/install.sh                          # exit 0
sh scripts/preflight.sh                        # exit 0
python3 scripts/validate-generated-pack.py .   # exit 0
sh scripts/verify.sh                           # exit 0
python3 scripts/verify-evidence-archive.py     # exit 0
```

The reproduced command text, with the environment it ran in:

```powershell
$dir = Join-Path $env:TEMP "vector-cleanroom"
Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
git clone --quiet <repository> $dir
cd $dir
sh -c "sh scripts/install.sh"
sh -c "sh scripts/preflight.sh"
sh -c "python3 scripts/validate-generated-pack.py ."
sh -c "sh scripts/verify.sh"
sh -c "python3 scripts/verify-evidence-archive.py"
```

## Result

| Command | Exit | Log |
|---|---|---|
| `install.sh` | 0 | `install.log` |
| `preflight.sh` | 0 | `preflight.log` |
| `validate-generated-pack.py .` | 0 | `generated-pack.log` |
| `verify.sh` | 0 | `verify.log` |
| `verify-evidence-archive.py` | 0 | `evidence-archive.log` |

Machine-readable: `exit-codes.json`.

The clean-room `verify.sh` ran the same 19 gates and recorded exit 0 for every one of them
(`.agent/verification/state/gate-results.jsonl` inside the clone), including the unit, integration,
E2E, security, dependency-audit, MCP and provider gates, the build, the packaged smoke test and the
packaged live-fire. Its run manifest reports `status: VERIFIED`, `candidate_epoch: 17`, `git_sha:
a1de4c02…`, and its artifact digest is
`fd716afa1a6d4f244c444d5d688111ac7c5db1b1df81ae8c97251ca3a1c5d6ed` — **different bytes from the
working repository's `6983c707…` for the same source**, which is the documented fact that a Windows
build is not bit-reproducible and why no gate asserts digest equality across builds.

Prerequisites the README declares, as `preflight.sh` observed them in the clone: git 2.55.0.windows.2,
Python 3.14.4, rustc 1.97.1, cargo 1.97.1, node v24.14.1, corepack 0.34.6. `install.sh` reported that
the two pinned security subcommands it provisions (`cargo-audit 0.22.2`, `cargo-deny 0.19.9`) were
already present at those versions, and pnpm resolved the committed lockfile without changing it
(`Lockfile is up to date, resolution step is skipped`; 193 packages, `Done in 6.6s using pnpm
v10.34.5`).

`verify-evidence-archive.py` re-derived **107 of 107 committed evidence digests from 10 epoch
archives, 0 missing, 0 mismatched** — the evidence record survived the copy.

## What this does and does not close

- **DOD-002** is satisfied for the repository: a clean checkout builds and passes the gates from the
  committed lockfiles with the declared toolchain.
- **DOD-023** is satisfied for the install, preflight, validation and verification instructions. The
  content-pack install, upgrade and rollback instructions are executed against realistic persistent
  state in the same round; that evidence is `../EP-007/content-corpus/ROUND-29-REPORT.md`.
- The clone is a second directory on the **same machine**, not a second operating system. The
  clean-VM and declared-hardware lanes are DOD-005 and DOD-022, which remain PARTIAL and cannot be
  provisioned from here; this run does not claim them.
