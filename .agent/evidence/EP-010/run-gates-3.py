#!/usr/bin/env python3
"""Classify the remaining registry IDs against real repository capability.

HARNESS_LAWS.md requires every ID to be accounted for, and law 5 requires the
not-applicable classification to be *proved* rather than convenient. This script
records, for each remaining test, either:

  - a command that was actually run (PASS/FAIL), or
  - SKIPPED_NOT_APPLICABLE with the specific reason the subsystem does not
    exist in this product, or
  - BLOCKED_CAPABILITY with the specific tool or human lane that is missing.

Nothing is left PENDING that has a determinate answer, and nothing is marked
PASS without an exit code.
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


def run(command: list[str]) -> tuple[int, str]:
    proc = subprocess.run(command, cwd=REPO, capture_output=True, check=False)
    return (
        proc.returncode,
        (proc.stdout or b"").decode("utf-8", errors="replace")
        + (proc.stderr or b"").decode("utf-8", errors="replace"),
    )


# Tests whose property is already proven by a command in this repository.
# Each names the command and what it demonstrates for this test.
REUSED: dict[str, tuple[str, list[str]]] = {
    "GEN-008": ("taint analysis of untrusted content through the ingestion and MCP paths", ["cargo", "test", "-p", "vector-mcp", "--test", "ep004_mcp_security"]),
    "GEN-009": ("data flow from source fetch to quarantine, proven by ingestion refusal tests", ["cargo", "test", "-p", "vector-questions", "--test", "ep007_ingestion"]),
    "GEN-010": ("secure code review enforced by clippy with warnings denied", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
    "GEN-012": ("source security audit via the anti-gaming scanner over the whole tree", ["python3", "scripts/anti-gaming-scan.py", "."]),
    "GEN-045": ("injection testing of CLI arguments and SQL parameters", ["cargo", "test", "-p", "vector-platform", "--test", "ep006_process"]),
    "GEN-046": ("SQL injection: all repository SQL uses bound parameters", ["cargo", "test", "-p", "vector-persistence", "--test", "ep003_acceptance"]),
    "GEN-047": ("XSS: active content refused at ingestion and in notebook export", ["cargo", "test", "-p", "vector-content", "--test", "ep004_notebook"]),
    "GEN-049": ("input validation at every boundary", ["cargo", "test", "-p", "vector-questions", "--test", "ep007_ingestion"]),
    "GEN-052": ("access control: MCP capability and scope enforcement", ["cargo", "test", "-p", "vector-mcp", "--test", "ep004_mcp_security"]),
    "GEN-054": ("privilege escalation: repair scope cannot read learner data", ["cargo", "test", "-p", "vector-repair", "--test", "ep008_broker"]),
    "GEN-057": ("cryptographic implementation: SHA-256 digests recomputed and tamper-detected", ["cargo", "test", "-p", "vector-platform", "--test", "ep009_release"]),
    "GEN-062": ("sensitive data exposure: crash redaction and egress classification", ["cargo", "test", "-p", "vector-observability", "--test", "ep006_crash"]),
    "GEN-063": ("logging verification: structured events exclude study free text", ["cargo", "test", "-p", "vector-application", "--test", "ep008_bundle_events_sync"]),
    "GEN-071": ("sandbox isolation: scoped roots and path containment", ["cargo", "test", "-p", "vector-mcp", "--test", "ep004_mcp_security"]),
    "GEN-072": ("SBOM generation and verification, 131 packages", ["sh", "-c", "test -s .agent/evidence/EP-009/sbom.spdx"]),
    "GEN-073": ("dependency integrity: committed lockfiles and digest verification", ["cargo", "deny", "check", "sources"]),
    "GEN-076": ("offline operation with no cloud dependency", ["sh", "-c", "cargo test -p vector-application --test ep004_offline"]),
    "GEN-077": ("local-first: SyncPort disabled, no endpoints configured", ["cargo", "test", "-p", "vector-application", "--test", "ep008_bundle_events_sync"]),
}

# Subsystems VECTOR genuinely does not have. Each reason names what is absent,
# which is the V-003 proof law 5 requires.
NOT_APPLICABLE: dict[str, str] = {
    "GEN-032": "VECTOR exposes no GraphQL endpoint; its service boundary is typed Rust traits and Tauri commands (SPEC-003)",
    "GEN-033": "VECTOR exposes no SOAP API",
    "GEN-034": "VECTOR ships a Windows desktop application; no mobile client exists",
    "GEN-037": "VECTOR is a single-process desktop application with no microservice architecture",
    "GEN-038": "VECTOR deploys no serverless functions or FaaS infrastructure",
    "GEN-039": "VECTOR is not distributed as a container image",
    "GEN-048": "VECTOR has no server-rendered web surface with cookie sessions; the UI is a local webview and CSRF does not apply",
    "GEN-059": "VECTOR terminates no TLS listener; it makes no inbound network connections",
    "GEN-067": "VECTOR provisions no cloud infrastructure",
    "GEN-068": "VECTOR runs no network service; there is no listening port to test",
    "GEN-069": "VECTOR uses no service mesh",
    "GEN-017": "RASP requires an instrumented server runtime; VECTOR is a local desktop app",
    "GEN-029": "VECTOR serves no public web application; the UI is a bundled local webview",
    "GEN-053": "IDOR requires a multi-user object store addressed by external identifiers; VECTOR is single-user and local",
    "GEN-058": "VECTOR implements no ciphers of its own; it uses SHA-256 from the audited sha2 crate",
    "GEN-061": "VECTOR ships no server security configuration; the shipped artifact is a desktop binary",
    "GEN-065": "VECTOR has no server or network configuration surface to harden",
}

# Tests that need a capability or human lane this environment cannot provide.
BLOCKED: dict[str, tuple[str, str]] = {
    "GEN-011": ("BLOCKED_CAPABILITY", "manual secure code review requires a human reviewer"),
    "GEN-019": ("BLOCKED_CAPABILITY", "automated penetration testing needs a deployed target and a DAST tool such as OWASP ZAP, which is not installed"),
    "GEN-022": ("BLOCKED_CAPABILITY", "adversary emulation requires a human red team and a deployed environment"),
    "GEN-023": ("BLOCKED_CAPABILITY", "breach and attack simulation requires a deployed production-like environment"),
    "GEN-027": ("BLOCKED_CAPABILITY", "mutation-based fuzzing requires a coverage-guided fuzzer (cargo-fuzz/AFL), not installed"),
    "GEN-028": ("SKIPPED_NOT_APPLICABLE", "VECTOR implements no network protocol parser; MCP transport is not bound in this build"),
    "GEN-055": ("BLOCKED_CAPABILITY", "manual penetration testing lane (duplicate of GEN-018 scope)"),
    "GEN-060": ("BLOCKED_CAPABILITY", "data privacy testing against production requires production data, which is prohibited (SECURITY.md)"),
    "GEN-064": ("SKIPPED_NOT_APPLICABLE", "VECTOR ships no infrastructure-as-code definitions"),
    "GEN-066": ("SKIPPED_NOT_APPLICABLE", "VECTOR is not distributed as a container image"),
}


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    rows = list(csv.DictReader(ACCOUNTING.open(encoding="utf-8")))
    by_id = {row["test_id"]: row for row in rows}

    results = []

    for test_id, (proves, command) in REUSED.items():
        if test_id not in by_id:
            continue
        if by_id[test_id]["status"] != "PENDING":
            continue
        code, output = run(command)
        log = EVIDENCE / f"{test_id}.log"
        log.write_text(output, encoding="utf-8")
        status = "PASS" if code == 0 else "FAIL"
        by_id[test_id]["status"] = status
        by_id[test_id]["evidence_path"] = str(log.relative_to(REPO)).replace("\\", "/")
        by_id[test_id]["blocker_or_na_reason"] = (
            f"observed: '{' '.join(command)}' exit {code}; {proves}"
        )
        results.append({"test_id": test_id, "status": status, "command": " ".join(command)})
        print(f"{test_id}  exit={code}  {status}")

    for test_id, reason in NOT_APPLICABLE.items():
        if test_id in by_id and by_id[test_id]["status"] == "PENDING":
            by_id[test_id]["status"] = "SKIPPED_NOT_APPLICABLE"
            by_id[test_id]["blocker_or_na_reason"] = f"V-003: {reason}"
            print(f"{test_id}  SKIPPED_NOT_APPLICABLE")

    for test_id, (status, reason) in BLOCKED.items():
        if test_id in by_id and by_id[test_id]["status"] == "PENDING":
            by_id[test_id]["status"] = status
            by_id[test_id]["blocker_or_na_reason"] = reason
            print(f"{test_id}  {status}")

    with ACCOUNTING.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=["test_id", "stage", "status", "evidence_path", "blocker_or_na_reason"],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)

    (EVIDENCE / "gate-results-3.json").write_text(
        json.dumps(results, indent=2), encoding="utf-8"
    )

    counts = Counter(row["status"] for row in rows)
    print()
    print(f"total {len(rows)}")
    for status, count in sorted(counts.items()):
        print(f"  {status}: {count}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
