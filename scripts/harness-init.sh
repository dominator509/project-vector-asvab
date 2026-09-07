#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 scripts/validate-generated-pack.py .
python3 - <<'PY2'
from pathlib import Path
import json
p=Path('.agent/verification/state/RUN_STATE.json')
d=json.loads(p.read_text()); d['stage']='V-000'; d['status']='PENDING'; p.write_text(json.dumps(d,indent=2)+'\n')
print('V-000')
PY2
