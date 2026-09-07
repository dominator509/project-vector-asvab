#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 - <<'PY2'
import json
from pathlib import Path
state=json.loads(Path('.agent/verification/state/RUN_STATE.json').read_text())
print(state.get('stage','V-000'))
PY2
