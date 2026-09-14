#!/usr/bin/env python3
"""Reconcile the accounting with the current, post-fix state of every gate.

The FAIL entries recorded earlier came from runs made before the fixes in this
node (the audit disposition and the missing package.json scripts). Re-running
them now and truthfully updating the statuses is the point: an accounting that
preserves stale failures is as misleading as one that invents passes.
"""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
ACCOUNTING = REPO / ".agent" / "verification" / "reports" / "COMPLETE_TEST_ACCOUNTING.csv"
EVIDENCE = REPO / ".agent" / "evidence" / "EP-010" / "gate-runs"

# Tests to re-observe, with the command that proves them now.
RERUN: dict[str, tuple[str, list[str]]] = {
    "GEN-003": ("vulnerability scan of the committed Cargo.lock; the two known advisories are dispositioned with reachability analysis in .cargo/audit.toml and .agent/evidence/EP-010/assess-vulnerabilities.py", ["cargo", "audit"]),
    "GEN-088": ("security test automation: the full verify.sh pipeline runs every gate from scripts", ["sh", "scripts/verify.sh"]),
    "GEN-104": ("vulnerability management: advisories tracked by identifier with documented dispositions", ["cargo", "audit"]),
}


def run(command: list[str]) -> tuple[int, str]:
    proc = subprocess.run(command, cwd=REPO, capture_output=True, check=False)
    return (
        proc.returncode,
        (proc.stdout or b"").decode("utf-8", errors="replace")
        + (proc.stderr or b"").decode("utf-8", errors="replace"),
    )


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    rows = list(csv.DictReader(ACCOUNTING.open(encoding="utf-8")))
    by_id = {row["test_id"]: row for row in rows}

    for test_id, (proves, command) in RERUN.items():
        code, output = run(command)
        log = EVIDENCE / f"{test_id}.log"
        log.write_text(output, encoding="utf-8")
        status = "PASS" if code == 0 else "FAIL"
        by_id[test_id]["status"] = status
        by_id[test_id]["evidence_path"] = str(log.relative_to(REPO)).replace("\\", "/")
        by_id[test_id]["blocker_or_na_reason"] = (
            f"observed: '{' '.join(command)}' exit {code}; {proves}"
        )
        print(f"{test_id}  exit={code}  {status}")

    with ACCOUNTING.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["test_id", "stage", "status", "evidence_path", "blocker_or_na_reason"],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)

    counts = Counter(row["status"] for row in rows)
    print()
    print(f"total {len(rows)}")
    for status, count in sorted(counts.items()):
        print(f"  {status}: {count}")

    (REPO / ".agent" / "verification" / "reports" / "ACCOUNTING_SUMMARY.json").write_text(
        json.dumps(
            {
                "total": len(rows),
                "by_status": dict(sorted(counts.items())),
                "pending": counts.get("PENDING", 0),
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
