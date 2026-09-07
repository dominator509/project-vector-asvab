#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 scripts/anti-gaming-scan.py .
python3 - <<'PY2'
from pathlib import Path
required=['Cargo.toml','package.json','src-tauri/tauri.conf.json']
missing=[x for x in required if not Path(x).exists()]
if missing:
 print('repository reality gate: product implementation surfaces absent:', ', '.join(missing))
 raise SystemExit(3)
print('repository reality gate: implementation surfaces present; continue with live tests')
PY2
