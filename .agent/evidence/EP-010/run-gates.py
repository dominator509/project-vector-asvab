#!/usr/bin/env python3
"""Execute the applicable registry gates and record their observed outcomes.

HARNESS_LAWS.md law 4: "PASS requires observed command evidence and appropriate
artifact/readback proof." This script runs the commands that genuinely apply to
Project VECTOR, captures exit codes and output, and updates the accounting with
PASS or FAIL based on what was actually observed.

Anything it cannot execute stays PENDING or is marked with the reason it is
blocked. Nothing is marked PASS without a command that ran.
"""

from __future__ import annotations

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
REPORTS = REPO / ".agent" / "verification" / "reports"
ACCOUNTING = REPORTS / "COMPLETE_TEST_ACCOUNTING.csv"
EVIDENCE = REPO / ".agent" / "evidence" / "EP-010" / "gate-runs"


# Each entry maps a test ID to a command with observed evidence.
# `expect_zero` records whether a zero exit means PASS.
GATE_COMMANDS: list[dict] = [
    {
        "test_id": "GEN-002",
        "title": "Software Composition Analysis (SCA)",
        "command": ["cargo", "deny", "check", "advisories"],
        "proves": "dependency advisories evaluated against the RustSec database",
    },
    {
        "test_id": "GEN-003",
        "title": "Dependency Scanning",
        "command": ["cargo", "audit"],
        "proves": "vulnerability scan of the committed Cargo.lock",
    },
    {
        "test_id": "GEN-004",
        "title": "License Compliance Scanning",
        "command": ["cargo", "deny", "check", "licenses"],
        "proves": "every Rust dependency license checked against the release policy",
    },
    {
        "test_id": "GEN-005",
        "title": "Secrets Scanning",
        "command": ["python3", "scripts/secret-scan.py"],
        "proves": "tracked files scanned for credential shapes and forbidden files",
    },
    {
        "test_id": "GEN-001",
        "title": "Static Application Security Testing (SAST)",
        "command": ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        "proves": "static analysis of all Rust sources with warnings denied",
    },
    {
        "test_id": "GEN-014",
        "title": "Package/Supply Chain Security",
        "command": ["cargo", "deny", "check", "sources"],
        "proves": "every dependency resolves to the approved registry",
    },
    {
        "test_id": "GEN-013",
        "title": "Dependency Confusion",
        "command": ["cargo", "deny", "check", "bans"],
        "proves": "banned crates and duplicate-version risks evaluated",
    },
]

# Tests that require a capability this environment does not have. Each records
# exactly what is missing, because "blocked" without a reason is not accounting.
BLOCKED: list[dict] = [
    ("GEN-015", "BLOCKED_CAPABILITY", "DAST requires a running deployed service and a scanning tool (e.g. OWASP ZAP) that is not installed"),
    ("GEN-016", "BLOCKED_CAPABILITY", "IAST requires runtime instrumentation agent support from the language runtime"),
    ("GEN-018", "BLOCKED_CAPABILITY", "manual penetration testing requires a human tester; PREFLIGHT PF-017 human lane"),
    ("GEN-020", "BLOCKED_CAPABILITY", "red team testing requires a human team and a deployed environment"),
    ("GEN-021", "BLOCKED_CAPABILITY", "purple team testing requires both offensive and defensive human participants"),
    ("GEN-103", "BLOCKED_PREREQUISITE", "Common Criteria evaluation is an external laboratory certification, not an in-repo test"),
    ("GEN-040", "SKIPPED_NOT_APPLICABLE", "VECTOR ships a local desktop application; it deploys no Kubernetes cluster"),
    ("GEN-064", "SKIPPED_NOT_APPLICABLE", "VECTOR ships no infrastructure-as-code definitions; there is no cloud infrastructure"),
    ("GEN-066", "SKIPPED_NOT_APPLICABLE", "VECTOR is not distributed as a container image"),
    ("E2E-020", "BLOCKED_CAPABILITY", "user acceptance testing requires human participants; PREFLIGHT PF-017 human lane"),
    ("SUP-015", "BLOCKED_CAPABILITY", "assistive-technology validation requires a human using Narrator or NVDA; PREFLIGHT PF-017"),
]


def run(command: list[str]) -> tuple[int, str]:
    """Run a command and capture its output.

    Output is decoded as UTF-8 with replacement and errors are tolerated: these
    tools emit box-drawing characters and, on Windows, occasionally bytes that
    are not valid in the console codepage. A decoding failure must not lose the
    exit code, which is the evidence that actually matters.
    """
    proc = subprocess.run(
        command,
        cwd=REPO,
        capture_output=True,
        check=False,
    )
    stdout = (proc.stdout or b"").decode("utf-8", errors="replace")
    stderr = (proc.stderr or b"").decode("utf-8", errors="replace")
    return proc.returncode, stdout + stderr


def slug(test_id: str) -> str:
    return test_id.replace("/", "-")


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)

    rows = list(csv.DictReader(ACCOUNTING.open(encoding="utf-8")))
    by_id = {row["test_id"]: row for row in rows}

    results: list[dict] = []

    for gate in GATE_COMMANDS:
        test_id = gate["test_id"]
        code, output = run(gate["command"])

        log = EVIDENCE / f"{slug(test_id)}.log"
        log.write_text(output, encoding="utf-8")

        status = "PASS" if code == 0 else "FAIL"
        detail = (
            f"observed: '{' '.join(gate['command'])}' exit {code}; {gate['proves']}. "
            f"Evidence: .agent/evidence/EP-010/gate-runs/{log.name}"
        )

        if test_id in by_id:
            by_id[test_id]["status"] = status
            by_id[test_id]["evidence_path"] = str(log.relative_to(REPO)).replace("\\", "/")
            by_id[test_id]["blocker_or_na_reason"] = detail

        results.append(
            {
                "test_id": test_id,
                "title": gate["title"],
                "command": " ".join(gate["command"]),
                "exit_code": code,
                "status": status,
            }
        )
        print(f"{test_id}  exit={code}  {status}  ({gate['title']})")

    for test_id, status, reason in BLOCKED:
        if test_id in by_id:
            by_id[test_id]["status"] = status
            by_id[test_id]["blocker_or_na_reason"] = reason
        results.append(
            {
                "test_id": test_id,
                "title": "",
                "command": "",
                "exit_code": None,
                "status": status,
                "reason": reason,
            }
        )
        print(f"{test_id}  {status}")

    with ACCOUNTING.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["test_id", "stage", "status", "evidence_path", "blocker_or_na_reason"],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)

    (EVIDENCE / "gate-results.json").write_text(
        json.dumps(results, indent=2), encoding="utf-8"
    )

    from collections import Counter

    counts = Counter(row["status"] for row in rows)
    print()
    print(f"total {len(rows)}")
    for status, count in sorted(counts.items()):
        print(f"  {status}: {count}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
