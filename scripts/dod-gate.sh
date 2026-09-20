#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

# Definition-of-Done gate.
#
# This previously checked only that the registry had 42 rows and that
# `DOD_STATUS.jsonl` was non-empty. That accepts a file whose rows are all
# timestamped days earlier and all point at one evidence directory: it validated
# a shape rather than a result. It now checks that the status was produced for
# the artifact the run manifest names, that every registered clause has exactly
# one row, and that every status comes from the harness taxonomy — then reports
# the clauses that are not PASS, because a silent pass over unfinished work is
# the failure this gate exists to prevent.

python3 - <<'PY2'
import csv
import json
import pathlib

TAXONOMY = {
    'PASS',
    'PARTIAL',
    'FAIL',
    'PENDING',
    'BLOCKED_PREREQUISITE',
    'BLOCKED_CAPABILITY',
    'EXTERNAL_REQUIRED',
    'DEFERRED_LONG_RUNNING',
    'SKIPPED_NOT_APPLICABLE',
}

registry = list(
    csv.DictReader(
        open('.agent/verification/DOD_REGISTRY.csv', encoding='utf-8', newline='')
    )
)
if len(registry) != 42:
    print(f'DoD registry count mismatch: {len(registry)}, expected 42')
    raise SystemExit(3)

status_path = pathlib.Path('.agent/verification/state/DOD_STATUS.jsonl')
if not status_path.exists() or not status_path.read_text().strip():
    print('DoD execution evidence is empty')
    raise SystemExit(3)

rows = [json.loads(line) for line in status_path.read_text().splitlines() if line.strip()]

problems = []

# Every clause accounted for, exactly once.
registered = {r['dod_id'] for r in registry}
recorded = [r['dod_id'] for r in rows]
missing = sorted(registered - set(recorded))
unknown = sorted(set(recorded) - registered)
duplicated = sorted({d for d in recorded if recorded.count(d) > 1})
if missing:
    problems.append(f'clauses with no status row: {", ".join(missing)}')
if unknown:
    problems.append(f'status rows for unregistered clauses: {", ".join(unknown)}')
if duplicated:
    problems.append(f'clauses recorded more than once: {", ".join(duplicated)}')

# Statuses must come from the taxonomy, and each row must name what was checked.
for row in rows:
    if row.get('status') not in TAXONOMY:
        problems.append(
            f"{row.get('dod_id')}: status {row.get('status')!r} is not in the taxonomy"
        )
    if not str(row.get('check', '')).strip():
        problems.append(f"{row.get('dod_id')}: no check recorded")
    if not str(row.get('evidence_path', '')).strip():
        problems.append(f"{row.get('dod_id')}: no evidence path recorded")

# The status must belong to this artifact, not to an earlier one.
manifest_path = pathlib.Path('.agent/verification/state/RUN_MANIFEST.json')
if manifest_path.exists():
    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
    digest = manifest.get('artifact_digest')
    stale = [r['dod_id'] for r in rows if r.get('artifact_digest') != digest]
    if stale:
        problems.append(
            'status was evaluated against a different artifact than the run manifest '
            f'records, for: {", ".join(sorted(stale))}'
        )
else:
    problems.append('no run manifest; run sh scripts/verify.sh first')

if problems:
    print('DoD gate: FAILED')
    for problem in problems:
        print(f'  - {problem}')
    raise SystemExit(3)

counts = {}
for row in rows:
    counts[row['status']] = counts.get(row['status'], 0) + 1

print(f'DoD gate: {len(rows)} clauses evaluated, {dict(sorted(counts.items()))}')

unfinished = [r for r in rows if r['status'] != 'PASS']
if unfinished:
    print(f'DoD gate: {len(unfinished)} clause(s) are not PASS, each with its gap:')
    for row in sorted(unfinished, key=lambda r: r['dod_id']):
        print(f"  {row['dod_id']}  {row['status']:24} {row['check']}")

# A clause recorded FAIL is a defect in the product's current claims, and fails
# the gate. PARTIAL, PENDING, DEFERRED_LONG_RUNNING and EXTERNAL_REQUIRED are
# honest states for work that is not finished; the release verdict carries them
# rather than this gate, because failing on them would make every build red until
# the product is finished, and a permanently red gate is one nobody reads.
failed = [r['dod_id'] for r in rows if r['status'] == 'FAIL']
if failed:
    print(f'DoD gate: FAILED clauses: {", ".join(sorted(failed))}')
    raise SystemExit(3)

print('DoD gate: no clause failed; unfinished clauses are carried by the release verdict')
PY2
