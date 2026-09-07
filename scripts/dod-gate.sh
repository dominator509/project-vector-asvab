#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 - <<'PY2'
import csv, pathlib
rows=list(csv.DictReader(open('.agent/verification/DOD_REGISTRY.csv',encoding='utf-8')))
if len(rows)!=42: raise SystemExit('DoD registry count mismatch')
state=pathlib.Path('.agent/verification/state/DOD_STATUS.jsonl')
if not state.exists() or not state.read_text().strip():
 print('DoD execution evidence is empty')
 raise SystemExit(3)
print('DoD rows present; validate executed statuses in V-021')
PY2
