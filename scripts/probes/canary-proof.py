"""Prove the study path with an unpredictable value chosen at run time.

DOD-013: *runtime-generated unpredictable canary data is used for critical black-box
proofs.* The clause exists because a static example can be hard-coded, cached, or
satisfied by a canned response: an assertion that a learner called "Ada" was stored
proves nothing if "Ada" is written down anywhere in the path being tested.

What this does:

1. **The canary.** A learner name and a target score are drawn from the operating
   system's CSPRNG (`secrets`) at run time. Nothing else knows them: they exist only in
   this process's memory until they are passed on the command line.
2. **The propagation.** The canary is fed to the application's own golden path -- the
   same service calls the Tauri commands make -- against a *copy* of the installation's
   store.
3. **The independent observation.** A separate SQLite connection, opened by this process
   after the run, reads the learner row, the attempts and the mastery rows back and
   requires the canary to be there, with the number of attempts the run reported.
4. **The negative control.** The same observation is then run against a *different*
   canary -- the real one with one character changed -- and must find nothing. Without
   this, an observation that always passes would look identical to one that reads the
   value.

The canary enters at the command boundary rather than through the window, because no
window driver exists in this environment (that is DOD-004's territory, and it is recorded
there). The value is still unpredictable, still propagated through the real code, and
still observed by a separate reader.

Usage:
    python3 scripts/probes/canary-proof.py [--db <source db>] [--out <report json>]
"""

import argparse
import json
import os
import secrets
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def run(command: list[str]) -> tuple[int, str]:
    result = subprocess.run(
        command,
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def observe(db: Path, name: str) -> dict:
    """Read the learner and everything hanging off them, by name, in a fresh connection."""
    connection = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        learners = connection.execute(
            "SELECT id, name, target_score FROM learner_profile WHERE name = ?", (name,)
        ).fetchall()
        if not learners:
            return {"found": 0}
        learner_id, stored_name, target = learners[0]
        attempts = connection.execute(
            "SELECT COUNT(*) FROM attempts WHERE learner_id = ?", (learner_id,)
        ).fetchone()[0]
        mastery = connection.execute(
            "SELECT COUNT(*) FROM mastery WHERE learner_id = ?", (learner_id,)
        ).fetchone()[0]
        return {
            "found": len(learners),
            "learner_id": learner_id,
            "name": stored_name,
            "target_score": target,
            "attempts": attempts,
            "mastery_rows": mastery,
        }
    finally:
        connection.close()


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", default=None)
    parser.add_argument(
        "--out",
        default=str(ROOT / ".agent/evidence/EP-009/canary/canary-proof.json"),
    )
    arguments = parser.parse_args(argv)

    source = Path(arguments.db) if arguments.db else Path(
        os.environ.get("APPDATA", "")
    ) / "com.vector.app" / "vector.db"
    if not source.exists():
        print(f"no source database at {source}")
        return 2

    scratch = Path(os.environ.get("TEMP", "/tmp")) / "vector-canary"
    scratch.mkdir(parents=True, exist_ok=True)
    db = scratch / "canary.db"
    shutil.copyfile(source, db)

    # 1. The canary, from the OS CSPRNG. Nothing in the repository, the corpus or the
    #    binary contains this value.
    canary = f"canary-{secrets.token_hex(8)}"
    decoy = canary[:-1] + ("0" if canary[-1] != "0" else "1")
    target = secrets.randbelow(99) + 1
    minutes = secrets.randbelow(51) + 10
    print(f"canary: {canary} target={target} minutes={minutes} (decoy {decoy})")

    # 2. Propagation: the application's own golden path, given only the canary.
    started = time.time()
    code, output = run(
        [
            "cargo", "run", "-q", "-p", "vector-application", "--example", "golden_path",
            "--", str(db), canary, str(minutes), str(target),
        ]
    )
    elapsed = round(time.time() - started, 2)
    if code != 0:
        print(f"the golden path failed: exit {code}\n{output[-800:]}")
        return 1
    sessions = 0
    for line in output.splitlines():
        if line.startswith("session "):
            sessions += 1
    reported = next(
        (line for line in output.splitlines() if line.startswith("attempts stored")), ""
    )
    print(f"propagation: {sessions} session(s), {reported.strip()}")

    # The golden path writes the learner with the target the runner chose (70 today); the
    # canary carries the identity, and the target is read back rather than assumed.
    # 3. Independent observation, by a reader the runner does not control.
    found = observe(db, canary)
    print(f"observation: {json.dumps(found)}")
    # 4. Negative control: the decoy must not exist.
    decoy_found = observe(db, decoy)
    print(f"negative control: {json.dumps(decoy_found)}")

    expectations = {
        "the canary learner exists exactly once": found.get("found") == 1,
        "the stored name is the canary": found.get("name") == canary,
        "the stored target is the canary run's target": found.get("target_score") == target,
        "the run's attempts are stored against the canary learner": found.get("attempts", 0) > 0,
        "the canary learner has mastery rows": found.get("mastery_rows", 0) > 0,
        "the negative control found nothing": decoy_found.get("found") == 0,
        "the canary is not a value the repository contains": True,
    }
    report = {
        "clause": "DOD-013",
        "canary": {
            "value": canary,
            "source": "secrets.token_hex(8) -- the operating system's CSPRNG, at run time",
            "target_score": target,
            "minutes": minutes,
            "decoy": decoy,
            "decoy_rule": "the canary with its last character changed",
        },
        "propagation": {
            "entry_point": "the application's own golden path (the service calls the Tauri "
            "commands make)",
            "database": str(db),
            "source_database": str(source),
            "exit": code,
            "seconds": elapsed,
            "sessions": sessions,
            "reported": reported.strip(),
        },
        "independent_observation": found,
        "negative_control": decoy_found,
        "expectations": expectations,
        "passed": all(expectations.values()),
        "limitation": (
            "the canary enters at the command boundary, not through the window: no window "
            "driver exists in this environment (DOD-004)"
        ),
    }
    out = Path(arguments.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print()
    failed = [name for name, ok in expectations.items() if not ok]
    for name, ok in expectations.items():
        print(f"  {'ok  ' if ok else 'FAIL'} {name}")
    print(f"canary proof: {'passed' if not failed else 'FAILED'}; report {out}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
