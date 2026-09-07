#!/usr/bin/env python3
"""Validate GraphLock v3.1 hash-chained JSONL ledger."""
from __future__ import annotations
import hashlib, json, sys
from pathlib import Path

path = Path(sys.argv[1] if len(sys.argv) > 1 else '.agent/state/LEDGER.jsonl')
if not path.exists():
    print(f'hash-ledger validation: missing {path}', file=sys.stderr)
    sys.exit(1)
prev = ''
line_no = 0
for raw in path.read_text('utf-8').splitlines():
    line_no += 1
    obj = json.loads(raw)
    required = ['event_id','timestamp_utc','agent_id','mode','node_id','event','previous_event_hash','event_hash']
    for key in required:
        if key not in obj:
            print(f'line {line_no}: missing {key}', file=sys.stderr); sys.exit(1)
    if obj['previous_event_hash'] != prev:
        print(f'line {line_no}: previous_event_hash mismatch', file=sys.stderr); sys.exit(1)
    claimed = obj['event_hash']
    tmp = dict(obj); tmp['event_hash'] = ''
    canonical = json.dumps(tmp, sort_keys=True, separators=(',',':')).encode('utf-8')
    actual = hashlib.sha256(canonical).hexdigest()
    if actual != claimed:
        print(f'line {line_no}: event_hash mismatch', file=sys.stderr); sys.exit(1)
    prev = claimed
print('hash-ledger validation: ok')
