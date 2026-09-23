"""Run the study path under load for a bounded time, with telemetry.

DOD-038 asks for soak, endurance and stress at "the specified scale", and says an abbreviated
trial is labelled separately and can never be PASS for the full requirement. No scale is specified
anywhere in this repository, so what this produces is an *abbreviated* trial: a bounded run of the
real stack against a real store, with per-iteration heartbeats, latency and size telemetry, so the
claim it supports is "the study path ran for N minutes without an error" and not "the product is
stable for a week".

What it exercises, per iteration:

* the packaged artifact's own self-check -- it creates a database, migrates it, writes a learner
  and two attempts, backs it up, restores it, and refuses a tampered archive;
* a golden path on the accumulating store: a learner, the plan, one session per named objective,
  an attempt each, analytics and mastery read back, provenance re-checked;
* an integrity check and a row census, so a soak that silently corrupts or stops writing is
  visible in the telemetry rather than in a green exit code.

The store is a *copy* of the installation's database, taken once at the start: this writes learner
rows and no test may write to the user's own store. The artifact is the release binary, so the
self-check is the shipped code and not a test build.

Usage:
    python3 scripts/probes/soak.py --minutes 30 [--db <source db>] [--out <report json>]
"""

import argparse
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ARTIFACT = ROOT / "target" / "release" / "vector-desktop.exe"
SCRATCH = Path(os.environ.get("TEMP", "/tmp")) / "vector-soak"


def run(command: list[str], cwd: Path | None = None) -> tuple[int, str]:
    result = subprocess.run(
        command,
        cwd=str(cwd or ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def census(db: Path) -> dict:
    connection = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        tables = {}
        for table in ("content_items", "attempts", "learner_profiles", "evidence_records",
                      "content_item_sources", "learner_mastery"):
            try:
                tables[table] = connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            except sqlite3.Error:
                tables[table] = None
        return {"integrity": integrity, "rows": tables}
    finally:
        connection.close()


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--minutes", type=float, default=30.0)
    parser.add_argument("--db", default=None, help="source database; a copy is used")
    parser.add_argument("--out", default=str(ROOT / ".agent/evidence/EP-009/soak/soak.json"))
    arguments = parser.parse_args(argv)

    if not ARTIFACT.exists():
        print(f"the release artifact is absent at {ARTIFACT}; run `sh scripts/build.sh` first")
        return 4

    source = Path(arguments.db) if arguments.db else Path(
        os.environ.get("APPDATA", "")
    ) / "com.vector.app" / "vector.db"
    if not source.exists():
        print(f"no source database at {source}")
        return 2

    SCRATCH.mkdir(parents=True, exist_ok=True)
    db = SCRATCH / "soak.db"
    shutil.copyfile(source, db)
    report_path = Path(arguments.out)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    heartbeats = report_path.with_suffix(".jsonl")
    heartbeats.write_text("", encoding="utf-8")

    started = time.time()
    deadline = started + arguments.minutes * 60
    iterations = 0
    failures: list[str] = []
    latencies: list[float] = []
    baseline = census(db)
    first_size = db.stat().st_size
    print(f"soak: source {source}")
    print(f"soak: copy   {db} ({first_size} bytes)")
    print(f"soak: budget {arguments.minutes} minute(s)")
    print(f"soak: baseline {json.dumps(baseline)}")

    while time.time() < deadline:
        iterations += 1
        iteration_started = time.time()
        problems: list[str] = []

        # 1. The shipped binary's own check, in its own directory.
        check_dir = SCRATCH / "self-check"
        shutil.rmtree(check_dir, ignore_errors=True)
        check_dir.mkdir(parents=True, exist_ok=True)
        report = check_dir / "self-check.json"
        code, output = run(
            [str(ARTIFACT), "--self-check", "--data-dir", str(check_dir), "--report", str(report)]
        )
        if code != 0:
            problems.append(f"self-check exit {code}: {output[-200:]}")
        elif report.exists():
            verdict = json.loads(report.read_text(encoding="utf-8")).get("verdict")
            if verdict != "pass":
                problems.append(f"self-check verdict {verdict!r}")

        # 2. The golden path against the accumulating store.
        code, output = run(
            [
                "cargo", "run", "-q", "-p", "vector-application", "--example", "golden_path",
                "--", str(db), f"soak-{iterations}", "15",
            ]
        )
        if code != 0:
            problems.append(f"golden path exit {code}: {output[-200:]}")
        elif "golden path complete" not in output:
            problems.append("golden path reported no completion")

        # 3. Integrity and census, so a silent corruption or a stalled writer shows up here.
        state = census(db)
        if state["integrity"] != "ok":
            problems.append(f"integrity {state['integrity']!r}")
        elapsed = time.time() - iteration_started
        latencies.append(elapsed)
        if problems:
            failures.extend(problems)

        heartbeat = {
            "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "iteration": iterations,
            "seconds": round(elapsed, 3),
            "db_bytes": db.stat().st_size,
            "integrity": state["integrity"],
            "attempts": state["rows"]["attempts"],
            "items": state["rows"]["content_items"],
            "problems": problems,
        }
        with heartbeats.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(heartbeat) + "\n")
        print(
            f"  {heartbeat['at']} #{iterations:<4} {elapsed:6.2f}s "
            f"attempts={heartbeat['attempts']:<6} items={heartbeat['items']:<6} "
            f"db={heartbeat['db_bytes']:<10} {'OK' if not problems else 'PROBLEM'}"
        )

    finished = time.time()
    final_state = census(db)
    summary = {
        "trial": "abbreviated",
        "scale_declared_in_repository": None,
        "label": (
            "abbreviated trial: bounded by --minutes, not the full soak DOD-038 asks for; "
            "this can never be PASS for the full requirement"
        ),
        "artifact": str(ARTIFACT),
        "artifact_sha256": __import__("hashlib").sha256(ARTIFACT.read_bytes()).hexdigest(),
        "source_database": str(source),
        "working_copy": str(db),
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(started)),
        "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(finished)),
        "minutes_requested": arguments.minutes,
        "minutes_observed": round((finished - started) / 60.0, 3),
        "iterations": iterations,
        "failures": failures,
        "latency_seconds": {
            "min": round(min(latencies), 3) if latencies else None,
            "mean": round(sum(latencies) / len(latencies), 3) if latencies else None,
            "max": round(max(latencies), 3) if latencies else None,
        },
        "baseline": baseline,
        "final": final_state,
        "db_bytes_start": first_size,
        "db_bytes_end": db.stat().st_size,
        "attempts_added": (
            (final_state["rows"]["attempts"] or 0) - (baseline["rows"]["attempts"] or 0)
        ),
        "heartbeats": str(heartbeats),
        "interrupted": False,
    }
    report_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print()
    print(f"soak: {iterations} iteration(s) over {summary['minutes_observed']} minute(s)")
    print(f"soak: attempts {summary['baseline']['rows']['attempts']} -> "
          f"{summary['final']['rows']['attempts']}, "
          f"db {summary['db_bytes_start']} -> {summary['db_bytes_end']} bytes")
    print(f"soak: failures {len(failures)}")
    for failure in failures[:5]:
        print(f"  {failure}")
    print(f"soak: report {report_path}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
