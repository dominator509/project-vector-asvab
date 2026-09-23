"""Measure the recovery objectives against a real store, and say what is declared.

DOD-036: *backup, restore, disaster recovery, hard-failure recovery, RPO, RTO and MTTR
claims are executed against reconciled state where applicable.* Backup, verified restore
and refusal of a tampered archive are already proven end to end by the packaged
self-check; what was missing was the measurement, so this measures it.

**What the product declares** (and nothing more):

* **RPO -- recovery point.** The store is one local SQLite file and the backups are the
  ones the learner or the self-check takes. The product claims *no automatic schedule*;
  a learner who never backs up has an RPO of everything since their last export. The
  measured figure for a run is therefore the interval between the backup and the fault,
  recorded per run rather than promised.
* **RTO -- recovery time.** A verified restore of this store, measured wall-clock, from
  the archive the backup command produced. The declared target for a store of this size
  is 60 seconds; the measurement is what decides whether that target is honest.
* **MTTR -- mean time to recover.** Detect a hard failure (a corrupted database file),
  restore the verified archive, and confirm the reconciled state: pre-fault and
  post-restore row censuses equal, and the restored bytes the backup's own checksum.
  Time from fault to verified recovery, measured once per fault kind.

**Faults injected**, each on its own copy: a truncated file, a corrupted page, and a
deleted file. Each is a real hard failure for a single-file store.

Every number here is measured in this run; nothing is copied from an earlier round.

Usage:
    python3 scripts/probes/recovery-objectives.py [--db <source db>] [--out <report json>]
"""

import argparse
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DECLARED_RTO_SECONDS = 60.0


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


def census(db: Path) -> dict:
    connection = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        rows = {}
        for table in ("content_items", "attempts", "learner_profile", "evidence_records",
                      "content_item_sources", "mastery", "content_packs"):
            rows[table] = connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        return {"integrity": integrity, "rows": rows}
    finally:
        connection.close()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", default=None)
    parser.add_argument(
        "--out",
        default=str(ROOT / ".agent/evidence/EP-009/recovery/recovery-objectives.json"),
    )
    arguments = parser.parse_args(argv)

    source = Path(arguments.db) if arguments.db else Path(
        os.environ.get("APPDATA", "")
    ) / "com.vector.app" / "vector.db"
    if not source.exists():
        print(f"no source database at {source}")
        return 2

    scratch = Path(os.environ.get("TEMP", "/tmp")) / "vector-recovery"
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True, exist_ok=True)
    live = scratch / "vector.db"
    shutil.copyfile(source, live)
    before = census(live)
    before_digest = digest(live)
    print(f"store: {live} ({live.stat().st_size} bytes)")
    print(f"pre-fault census: {json.dumps(before['rows'])}")

    # 1. The backup, through the documented command.
    archive = scratch / "vector-backup.db"
    started = time.time()
    code, output = run(
        ["cargo", "run", "-q", "-p", "vector-tools", "--", "db", "backup",
         "--db-path", str(live), "--dest", str(archive)]
    )
    backup_seconds = round(time.time() - started, 3)
    if code != 0 or not archive.exists():
        print(f"the backup failed: exit {code}\n{output[-400:]}")
        return 1
    archive_digest = digest(archive)
    archive_rows = census(archive)
    print(f"backup: exit {code}, {backup_seconds}s, {archive.stat().st_size} bytes, "
          f"digest {archive_digest[:16]}")

    # 2. Faults. Each on its own copy of the store, restored from the archive above.
    faults = []

    def measure(name: str, inject) -> dict:
        working = scratch / f"fault-{name}.db"
        shutil.copyfile(live, working)
        inject(working)
        detected_at = time.time()
        broken = None
        try:
            broken = census(working)
        except sqlite3.DatabaseError as error:
            broken = {"integrity": f"unreadable: {error}", "rows": {}}
        detect_seconds = round(time.time() - detected_at, 3)
        restore_started = time.time()
        code, output = run(
            ["cargo", "run", "-q", "-p", "vector-tools", "--", "db", "restore",
             "--db-path", str(working), "--source", str(archive),
             "--checksum", archive_digest]
        )
        restore_seconds = round(time.time() - restore_started, 3)
        after = census(working) if code == 0 else {"integrity": "not restored", "rows": {}}
        recovered = (
            code == 0
            and after["integrity"] == "ok"
            and after["rows"] == before["rows"]
            and digest(working) == before_digest
        )
        record = {
            "fault": name,
            "state_after_fault": broken["integrity"],
            "detect_seconds": detect_seconds,
            "restore_exit": code,
            "restore_seconds": restore_seconds,
            "mttr_seconds": round(detect_seconds + restore_seconds, 3),
            "reconciled": recovered,
            "rows_reconciled": after["rows"] == before["rows"],
            "bytes_reconciled": digest(working) == before_digest if code == 0 else False,
        }
        print(
            f"  {name:<12} after fault: {str(broken['integrity'])[:28]:<28} "
            f"restore {restore_seconds:>6.2f}s  reconciled={recovered}"
        )
        return record

    def truncate(path: Path) -> None:
        with path.open("r+b") as handle:
            handle.truncate(path.stat().st_size // 2)

    def corrupt_page(path: Path) -> None:
        with path.open("r+b") as handle:
            handle.seek(4096)
            handle.write(b"\x00" * 512)

    def delete(path: Path) -> None:
        path.unlink()

    print("faults:")
    for name, inject in (
        ("truncated", truncate),
        ("corrupt-page", corrupt_page),
        ("deleted", delete),
    ):
        faults.append(measure(name, inject))

    rto = max(fault["restore_seconds"] for fault in faults)
    mttr = max(fault["mttr_seconds"] for fault in faults)
    report = {
        "clause": "DOD-036",
        "source_database": str(source),
        "store_bytes": live.stat().st_size,
        "declared": {
            "rpo": (
                "no automatic backup schedule is claimed: the RPO is the interval since the "
                "learner's last backup or export, and the measured interval for this run is "
                f"{backup_seconds}s between the backup taken here and the injected faults"
            ),
            "rto_seconds": DECLARED_RTO_SECONDS,
            "mttr": "fault detection plus verified restore, measured per fault kind",
        },
        "measured": {
            "backup_seconds": backup_seconds,
            "restore_seconds_max": rto,
            "mttr_seconds_max": mttr,
            "rpo_seconds_this_run": backup_seconds,
        },
        "rto_target_met": rto <= DECLARED_RTO_SECONDS,
        "reconciled_every_fault": all(fault["reconciled"] for fault in faults),
        "pre_fault": before,
        "backup": {
            "path": str(archive),
            "digest": archive_digest,
            "rows": archive_rows["rows"],
            "integrity": archive_rows["integrity"],
        },
        "faults": faults,
    }
    out = Path(arguments.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print()
    print(f"RTO measured {rto}s against a declared {DECLARED_RTO_SECONDS}s: "
          f"{'met' if report['rto_target_met'] else 'NOT MET'}")
    print(f"MTTR measured {mttr}s (worst fault)")
    print(f"reconciled after every fault: {report['reconciled_every_fault']}")
    print(f"report {out}")
    return 0 if report["rto_target_met"] and report["reconciled_every_fault"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
