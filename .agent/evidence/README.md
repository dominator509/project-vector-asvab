# Evidence corpus

This directory is the verification record for Project VECTOR. It is what makes a
claim in `.agent/state/LEDGER.md` checkable rather than asserted.

## What is tracked, and what is not

`*.log` is git-ignored (see the root `.gitignore`). The raw output of every gate
run is therefore *not* committed directly. What is committed alongside it is:

| Suffix | Meaning |
| --- | --- |
| `<gate>.log.sha256` | SHA-256 of the `.log` file as it existed when the gate was run |
| `<gate>.exitcode` | The exit code the gate actually returned |
| `*.json`, `*.md`, `ledger.md`, `summary.json` | The human- and machine-readable findings |

The intent is that a small, stable digest is committed while multi-megabyte,
machine-specific log text stays out of the history. That works only as long as
the log it digests still exists somewhere: a digest with no artifact is an
attestation nobody can re-derive.

## `logs.tar.gz` — the epoch archives

Each closed epoch keeps a compressed archive of every log it produced:

```
.agent/evidence/EP-010/logs.tar.gz
```

An archive is written once when an epoch closes and is never regenerated, which
is why it compresses so well (220 logs, 11.1 MB raw, 1.13 MB across all ten
archives) and why it does not churn. It is a snapshot, not a build output.

Each archive contains its logs at the epoch-relative paths they originally had,
plus a `MANIFEST.txt` whose lines are:

```
<sha256 of the log>  <sha256 committed in the repository, or UNATTESTED>  <path>
```

`UNATTESTED` marks logs that never had a committed `.log.sha256` — most of the
per-gate logs under `EP-010/gate-runs/`, which are supporting detail rather than
primary evidence. Every one of the 107 logs that *does* have a committed digest
is present in an archive and matches it.

## Restoring and verifying

Extract one epoch, or all of them:

```sh
for a in .agent/evidence/EP-*/logs.tar.gz; do
  tar -xzf "$a" -C "$(dirname "$a")"
done
```

Then verify the restored logs against the digests committed in the repository:

```sh
python3 scripts/verify-evidence-archive.py
```

That script extracts every archive to a temporary directory, refuses any member
path that is absolute or escapes the archive, and re-hashes each restored log
against its committed `.log.sha256`. It exits non-zero on any missing log or
digest mismatch. Running it against a fresh clone is the proof that a workspace
can be discarded and fully reconstituted from GitHub.

## Known defect

`EP-009/livefire.log.sha256` has no sibling `EP-009/livefire.exitcode`. The log
is attested and present in `EP-009/logs.tar.gz`, but the exit code it returned
was never captured, so the *result* of that run cannot be checked from the
evidence alone. This is recorded rather than repaired: writing an exit code
after the fact, without a re-run that observes one, would manufacture evidence.
Re-running the EP-009 live-fire lane is the honest fix.
