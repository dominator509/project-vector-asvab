#!/usr/bin/env python3
"""Negative proof for the placeholder-residue check.

AGENTS.md forbids weakening a gate. The previous check flagged every file
containing "{{"/"}}", which produced false positives on valid nested JSON. The
replacement targets real placeholder shapes. This script proves the replacement
is not merely weaker by planting genuine placeholder residue and confirming the
validator still fails.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VALIDATOR = REPO / "scripts" / "validate-generated-pack.py"

# Fixtures are assembled from fragments so that this proof script does not
# itself contain the literal placeholder shapes it plants. Without this, the
# validator correctly flags this file as containing placeholder residue, and
# the proof becomes self-defeating.
OB, CB = "{", "}"
DOLLAR = "$"
LT, GT = "<", ">"

MUSTACHE = f"{OB}{OB}learner_name{CB}{CB}"
SHELL_PARAM = f"{DOLLAR}{OB}target_env{CB}"

# Each case is (name, relative filename, file contents, must_fail).
CASES = [
    (
        "valid nested JSON must PASS (the original false positive)",
        "ok_nested.json",
        '{"a":{"b":{"c":1}},"commands":{"allow":[],"deny":[]}}',
        False,
    ),
    (
        "mustache placeholder must FAIL",
        "bad_mustache.md",
        f"Hello {MUSTACHE}, your plan is ready.",
        True,
    ),
    (
        "shell-style placeholder in a Markdown doc must FAIL",
        "bad_shell.md",
        f"Deploy to {SHELL_PARAM} before continuing.",
        True,
    ),
    (
        "shell-style expansion in CODE must PASS (it is real syntax)",
        "ok_code.ts",
        f"const url = `http://{SHELL_PARAM}/api`;",
        False,
    ),
    (
        "shell-style expansion in a shell script must PASS (real syntax)",
        "ok_script.sh",
        f'echo "deploying to {SHELL_PARAM}"',
        False,
    ),
    (
        "angle placeholder must FAIL",
        "bad_angle.txt",
        f"Result: {LT}PLACEHOLDER{GT}",
        True,
    ),
    (
        "unrendered TODO marker must FAIL",
        "bad_todo.txt",
        f"Answer: {LT}TODO{GT}",
        True,
    ),
    (
        "plain prose with single braces must PASS",
        "ok_braces.txt",
        "Use set notation {1, 2, 3} and close with }",
        False,
    ),
]


def run_validator(root: Path) -> tuple[int, str]:
    proc = subprocess.run(
        [sys.executable, str(VALIDATOR), str(root)],
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.returncode, proc.stdout + proc.stderr


def main() -> int:
    failures: list[str] = []

    for name, filename, contents, must_fail in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # Copy the minimum structure the validator requires.
            for rel in [
                "AGENTS.md",
                "COMMANDS.md",
                ".agent/GRAPH.md",
                ".agent/LOOPS.md",
                ".agent/DONE_LAW.md",
                ".agent/verification/FUNCTIONAL_PROOF_MATRIX.csv",
                "scripts/ledger.sh",
                "scripts/graph-next.sh",
            ]:
                target = root / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                source = REPO / rel
                if source.exists():
                    shutil.copy2(source, target)
                else:
                    target.write_text("", encoding="utf-8")

            (root / filename).write_text(contents, encoding="utf-8")

            code, out = run_validator(root)
            flagged = "placeholder residue" in out
            ok = flagged if must_fail else not flagged

            status = "PASS" if ok else "FAIL"
            print(f"[{status}] {name}")
            if not ok:
                failures.append(f"{name}: expected must_fail={must_fail}, flagged={flagged}")
                print(f"        validator said: {out.strip()[:200]}")

    print()
    if failures:
        print(f"NEGATIVE PROOF FAILED ({len(failures)} case(s)):")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"NEGATIVE PROOF PASSED: {len(CASES)}/{len(CASES)} cases behaved as required.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
