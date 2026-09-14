#!/usr/bin/env python3
"""Negative proof for the anti-gaming review schema enforcement.

The validator previously checked only `verdict == "PASS"`, which let five
reviews satisfy the gate while missing 19 of the 20 fields the shipped schema
requires. This script proves the strengthened check is not merely present but
load-bearing: it builds disposable packs containing deliberately defective
reviews and confirms the validator fails on each.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
VALIDATOR = REPO / "scripts" / "validate-generated-pack.py"

REQUIRED = json.loads(
    (REPO / "schemas" / "anti-gaming-review.schema.json").read_text("utf-8")
)["required"]


def compliant_review() -> dict:
    """A review that satisfies every required field."""
    review: dict = {
        "node_id": "EP-900",
        "requirement_ids": ["REQ-001"],
        "status_requested": "DONE_VERIFIED",
        "forbidden_patterns_scanned": True,
        "verdict": "PASS",
        "reason": "synthetic review used for validator proof",
    }
    for field in REQUIRED:
        if field in review:
            continue
        # Required fields are lists or scalars; the schema's minItems lists need
        # at least one entry.
        review[field] = ["placeholder entry"]
    return review


CASES = [
    ("compliant review must PASS", lambda r: r, False),
    (
        "review missing required fields must FAIL",
        lambda r: {"node_id": "EP-900", "verdict": "PASS", "reason": "thin"},
        True,
    ),
    (
        "review with an empty non-empty-list field must FAIL",
        lambda r: {**r, "mutation_checks": []},
        True,
    ),
    (
        "review without forbidden_patterns_scanned must FAIL",
        lambda r: {k: v for k, v in r.items() if k != "forbidden_patterns_scanned"},
        True,
    ),
    (
        "review with a blank reason must FAIL",
        lambda r: {**r, "reason": "   "},
        True,
    ),
    (
        "review declaring the wrong node must FAIL",
        lambda r: {**r, "node_id": "EP-901"},
        True,
    ),
    (
        "review whose verdict is FAIL must FAIL",
        lambda r: {**r, "verdict": "FAIL"},
        True,
    ),
]


def build_pack(root: Path, review: dict) -> None:
    """Copy the minimum structure the validator requires into a temp pack."""
    for rel in [
        "AGENTS.md",
        "COMMANDS.md",
        ".agent/GRAPH.md",
        ".agent/LOOPS.md",
        ".agent/DONE_LAW.md",
        ".agent/verification/FUNCTIONAL_PROOF_MATRIX.csv",
        "scripts/ledger.sh",
        "scripts/graph-next.sh",
        "schemas/anti-gaming-review.schema.json",
    ]:
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        source = REPO / rel
        if source.exists():
            shutil.copy2(source, target)
        else:
            target.write_text("", encoding="utf-8")

    ledger = root / ".agent/state/LEDGER.md"
    ledger.parent.mkdir(parents=True, exist_ok=True)
    ledger.write_text(
        "# Ledger\n\n| Node | Status | Timestamp | Evidence Directory |\n"
        "|---|---|---|---|\n"
        "| EP-900 | NODE_DONE | 2026-09-10T00:00:00Z | .agent/evidence/EP-900 |\n",
        encoding="utf-8",
    )

    review_dir = root / ".agent/evidence/EP-900"
    review_dir.mkdir(parents=True, exist_ok=True)
    (review_dir / "anti_gaming_review.json").write_text(
        json.dumps(review, indent=2), encoding="utf-8"
    )


def main() -> int:
    failures: list[str] = []

    for name, transform, must_fail in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            build_pack(root, transform(compliant_review()))

            proc = subprocess.run(
                [sys.executable, str(VALIDATOR), str(root)],
                capture_output=True,
                text=True,
                check=False,
            )
            output = proc.stdout + proc.stderr
            # Only anti-gaming failures matter here; the synthetic pack is
            # intentionally incomplete elsewhere.
            flagged = "anti_gaming_review" in output and proc.returncode != 0

            ok = flagged if must_fail else not flagged
            status = "PASS" if ok else "FAIL"
            print(f"[{status}] {name}")
            if not ok:
                failures.append(f"{name}: expected must_fail={must_fail}, flagged={flagged}")
                print(f"        validator said: {output.strip()[:300]}")

    print()
    if failures:
        print(f"NEGATIVE PROOF FAILED ({len(failures)} case(s)):")
        for item in failures:
            print(f"  - {item}")
        return 1

    print(f"NEGATIVE PROOF PASSED: {len(CASES)}/{len(CASES)} cases behaved as required.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
