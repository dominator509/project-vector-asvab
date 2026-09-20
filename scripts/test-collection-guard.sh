#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

# Collection guard (DOD-007): fail when a suite collects nothing, or fewer than
# the count the caller expects.
#
# The script previously required a COUNT argument and nothing in the repository
# passed one: `verify.sh` did not call it and the harness had no invocation. A
# guard that is never invoked is not a guard. With no argument it now discovers
# the collected counts for itself, which is what the gate needs.
#
#   sh scripts/test-collection-guard.sh          # discover, require more than zero
#   sh scripts/test-collection-guard.sh 1200     # require at least 1200
#
# The count is real collection, not a static count of source declarations: the
# Rust side lists what the test harness would run, and the TypeScript side reads
# the result file the last `vitest run` wrote.

if [ "$#" -gt 1 ]; then
  echo "usage: sh scripts/test-collection-guard.sh [MINIMUM_COUNT]" >&2
  exit 2
fi

minimum=${1:-1}

python3 - "$minimum" <<'PY2'
import json
import pathlib
import re
import shutil
import subprocess
import sys

minimum = int(sys.argv[1])
root = pathlib.Path('.').resolve()
problems = []

# Windows resolves the pnpm launcher as `corepack.cmd`, which subprocess cannot
# find by bare name. `shutil.which` consults PATHEXT, so it finds the real
# launcher on every platform.
def launcher() -> list[str] | None:
    corepack = shutil.which('corepack')
    if corepack:
        return [corepack, 'pnpm']
    pnpm = shutil.which('pnpm')
    if pnpm:
        return [pnpm]
    return None


# --- Rust: ask the test harness what it would run -------------------------
cargo = shutil.which('cargo')
if cargo is None:
    problems.append('cargo is not on PATH')
    listing = None
else:
    listing = subprocess.run(
        [cargo, 'test', '--workspace', '--', '--list'],
        capture_output=True,
        text=True,
        check=False,
    )

rust_tests = 0
if listing is not None:
    # `cargo test --list` prints "<path>: test" per collected case, plus a
    # trailing "N tests, 0 benchmarks" summary that must not count as a case.
    rust_tests = len(re.findall(r': test$', listing.stdout, re.MULTILINE))
    if listing.returncode != 0:
        tail = (listing.stderr.strip().splitlines() or ['no output'])[-1]
        problems.append(f'cargo test --list failed: {tail}')
    elif rust_tests == 0:
        problems.append('the Rust workspace collected zero tests')

# --- TypeScript: collect, without executing --------------------------------
#
# `vitest list` enumerates the cases the config would run. This replaced reading
# Vite's cached results file, which holds durations rather than a test count and
# so reported zero however many tests had run.
ts_tests = 0
pnpm = launcher()
if pnpm is None:
    problems.append('neither corepack nor pnpm is on PATH')
else:
    desktop = root / 'apps' / 'desktop'
    listing = subprocess.run(
        [*pnpm, 'exec', 'vitest', 'list', '--json'],
        cwd=desktop,
        capture_output=True,
        text=True,
        check=False,
    )
    if listing.returncode != 0:
        tail = (listing.stderr.strip().splitlines() or ['no output'])[-1]
        problems.append(f'vitest list failed: {tail}')
    else:
        try:
            collected = json.loads(listing.stdout)
            ts_tests = len(collected)
        except ValueError as error:
            problems.append(f'vitest list did not return JSON: {error}')
        else:
            if ts_tests == 0:
                problems.append('the TypeScript suite collected zero tests')

total = rust_tests + ts_tests

if problems:
    print('test collection guard: FAILED')
    for problem in problems:
        print(f'  - {problem}')
    raise SystemExit(1)

if total < minimum:
    print(
        f'test collection guard: FAILED - collected {total}, '
        f'expected at least {minimum}'
    )
    raise SystemExit(1)

print(f'test collection guard: {rust_tests} Rust + {ts_tests} TypeScript = {total} collected')
PY2
