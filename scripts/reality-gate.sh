#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 scripts/anti-gaming-scan.py .
python3 - <<'PY2'
from pathlib import Path
# The desktop shell lives under apps/desktop/src-tauri/ after the monorepo
# layout was adopted. This gate previously probed a root-level
# src-tauri/tauri.conf.json, so it reported the product surfaces as absent even
# though they exist, which made the gate permanently red for the wrong reason.
required = [
    'Cargo.toml',
    'package.json',
    'apps/desktop/src-tauri/tauri.conf.json',
    'apps/desktop/package.json',
    'pnpm-lock.yaml',
    'Cargo.lock',
]
missing = [x for x in required if not Path(x).exists()]
if missing:
    print('repository reality gate: product implementation surfaces absent:', ', '.join(missing))
    raise SystemExit(3)
print('repository reality gate: implementation surfaces present; continue with live tests')
PY2
