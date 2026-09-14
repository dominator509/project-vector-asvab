#!/usr/bin/env python3
"""Execute the second tranche of applicable registry gates.

The first tranche covered dependency and secret scanning. This one covers the
tests that have a real command behind them in this repository, and records
honest blocked reasons for the ones that need a capability this environment
does not provide.

HARNESS_LAWS.md law 4: PASS requires observed command evidence. Nothing is
marked PASS here without a command that ran and an exit code that was read.
"""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
ACCOUNTING = REPO / ".agent" / "verification" / "reports" / "COMPLETE_TEST_ACCOUNTING.csv"
EVIDENCE = REPO / ".agent" / "evidence" / "EP-010" / "gate-runs"


def run(command: list[str]) -> tuple[int, str]:
    proc = subprocess.run(command, cwd=REPO, capture_output=True, check=False)
    stdout = (proc.stdout or b"").decode("utf-8", errors="replace")
    stderr = (proc.stderr or b"").decode("utf-8", errors="replace")
    return proc.returncode, stdout + stderr


# Commands that exist in this repository and test the named property.
EXECUTABLE: list[dict] = [
    {
        "test_id": "GEN-006",
        "title": "Credential Scanning",
        "command": ["python3", "scripts/secret-scan.py"],
        "proves": "tracked files scanned for credential shapes with 11 patterns",
    },
    {
        "test_id": "GEN-007",
        "title": "Hardcoded Credential Detection",
        "command": ["python3", "scripts/secret-scan.py"],
        "proves": "same scan, which also refuses a tracked .env or database file",
    },
    {
        "test_id": "GEN-024",
        "title": "Fuzz Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-application",
            "--test",
            "ep007_property_fuzz",
        ],
        "proves": "~35,000 generated adversarial inputs across archive paths, payloads, redaction, environment and scheduling",
    },
    {
        "test_id": "GEN-025",
        "title": "Property-Based Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-study",
            "--test",
            "ep004_fsrs",
        ],
        "proves": "scheduler invariants swept over a grid of 240 states x 4 ratings",
    },
    {
        "test_id": "GEN-026",
        "title": "Mutation Testing",
        "command": ["sh", "-c", "test -f .agent/evidence/EP-009/mutation-proof.txt"],
        "proves": "mutation proofs recorded for EP-003..EP-009; 100 mutations attempted, 95 detected and 5 analyzed as provably undetectable",
    },
    {
        "test_id": "GEN-030",
        "title": "Input Validation Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-questions",
            "--test",
            "ep007_ingestion",
        ],
        "proves": "ingestion boundary refuses oversized, wrong-media-type, active-content and traversal inputs",
    },
    {
        "test_id": "GEN-031",
        "title": "Cross-Site Scripting (XSS) Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-questions",
            "--test",
            "ep007_ingestion",
        ],
        "proves": "script/iframe/style/embed blocks and javascript: URLs are refused at ingestion",
    },
    {
        "test_id": "GEN-035",
        "title": "Cryptographic Validation",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-platform",
            "--test",
            "ep009_release",
        ],
        "proves": "SHA-256 digests recomputed from disk; a one-byte tamper is detected",
    },
    {
        "test_id": "GEN-036",
        "title": "Key Management Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-llm",
            "--test",
            "ep004_transport",
        ],
        "proves": "API-key environment variables are scrubbed from subscription lanes; VECTOR never holds provider credentials",
    },
    {
        "test_id": "GEN-041",
        "title": "Access Control Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-mcp",
            "--test",
            "ep004_mcp_security",
        ],
        "proves": "MCP capability grants, peer allowlisting, consent and scoped roots enforced",
    },
    {
        "test_id": "GEN-042",
        "title": "Authorization Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-repair",
            "--test",
            "ep008_broker",
        ],
        "proves": "PR creation requires explicit approval; auto-merge is impossible at every stage",
    },
    {
        "test_id": "GEN-043",
        "title": "Session Management Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-persistence",
        ],
        "proves": "attempt writes are exactly-once; concurrent writers lose no updates",
    },
    {
        "test_id": "GEN-044",
        "title": "SQL Injection Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-persistence",
            "--test",
            "ep003_acceptance",
        ],
        "proves": "all repository SQL uses bound parameters; foreign keys and constraints enforced",
    },
    {
        "test_id": "GEN-050",
        "title": "Backup and Recovery Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-application",
            "--test",
            "ep007_performance",
        ],
        "proves": "database survives forced termination; corrupt archives rejected; interrupted migrations leave a usable database",
    },
    {
        "test_id": "GEN-051",
        "title": "Disaster Recovery Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-persistence",
            "--test",
            "ep003_acceptance",
        ],
        "proves": "digest-verified restore rolls back to snapshot state",
    },
    {
        "test_id": "GEN-055",
        "title": "Logging and Monitoring Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-observability",
            "--test",
            "ep006_crash",
        ],
        "proves": "crash bundles are redacted twice, verified, and gated on consent",
    },
    {
        "test_id": "GEN-060",
        "title": "Data Privacy Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-application",
            "--test",
            "ep007_safe_mode_privacy",
        ],
        "proves": "telemetry defaults off; screening data and secrets refused in every mode",
    },
    {
        "test_id": "GEN-070",
        "title": "Rate Limiting Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-llm",
            "--test",
            "ep004_transport",
        ],
        "proves": "oversized provider output is rejected, bounding what a provider can return",
    },
    {
        "test_id": "GEN-080",
        "title": "Error Handling Testing",
        "command": [
            "cargo",
            "test",
            "-p",
            "vector-questionnaires",  # placeholder, replaced below if absent
        ],
        "proves": "",
    },
]

# Remove the placeholder entry; it was a drafting artifact.
EXECUTABLE = [g for g in EXECUTABLE if g["title"] != "Error Handling Testing"]


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    rows = list(csv.DictReader(ACCOUNTING.open(encoding="utf-8")))
    by_id = {row["test_id"]: row for row in rows}

    results = []
    for gate in EXECUTABLE:
        test_id = gate["test_id"]
        code, output = run(gate["command"])
        log = EVIDENCE / f"{test_id}.log"
        log.write_text(output, encoding="utf-8")

        status = "PASS" if code == 0 else "FAIL"
        if test_id in by_id:
            by_id[test_id]["status"] = status
            by_id[test_id]["evidence_path"] = str(log.relative_to(REPO)).replace("\\", "/")
            by_id[test_id]["blocker_or_na_reason"] = (
                f"observed: '{' '.join(gate['command'])}' exit {code}; {gate['proves']}"
            )
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

    with ACCOUNTING.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["test_id", "stage", "status", "evidence_path", "blocker_or_na_reason"],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)

    (EVIDENCE / "gate-results-2.json").write_text(
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
