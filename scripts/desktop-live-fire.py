#!/usr/bin/env python3
"""Launch the packaged desktop artifact and read back what it did.

Why this exists
---------------

`AGENTS.md` §9 (reality law) refuses compilation, screenshots, mocked tests and
a health endpoint as feature proof; `.agent/DONE_LAW.md` requires real artifact
execution with independent readback. Two facts about the packaged desktop
application cannot be established any other way:

1. that the frontend bundle actually loads and mounts inside the real WebView2
   webview, under the Content-Security-Policy in `tauri.conf.json`; and
2. that the webview can reach the Rust command layer over Tauri's IPC bridge.

Both are invisible to `cargo test`: the binary is a different build with a
different runtime, and a webview can render a blank or half-broken page while
every `invoke` fails. So the application writes a marker row through a real
command (`ui_ready`) when it mounts, and this script launches the packaged
executable and waits for that row to appear in the real database at the real
application-data path.

The marker is written by the frontend, stored by SQLite, and read here by a
separate process that shares no state with either. That is the independent
readback.

Usage
-----

    python3 scripts/desktop-live-fire.py [--report PATH] [--timeout SECONDS]

Exit codes
----------

    0  the artifact launched, the webview reached the command layer, and the
       marker was read back from the database
    1  the check ran and failed
    3  Windows-only check, not applicable on this platform
    4  the packaged artifact does not exist; build it first
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ARTIFACT = REPO_ROOT / "target" / "release" / "vector-desktop.exe"
BUNDLE_DIR = REPO_ROOT / "target" / "release" / "bundle"
# Tauri derives the application data directory from the bundle identifier.
IDENTIFIER = "com.vector.app"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def app_data_dir() -> Path:
    appdata = os.environ.get("APPDATA")
    if not appdata:
        raise SystemExit("APPDATA is not set, so the application data path is unknown")
    return Path(appdata) / IDENTIFIER


def count_markers(db_path: Path) -> int:
    """Rows in health_diagnostics written by the webview.

    Opened read-only so the check cannot interfere with a running application,
    and so a typo here can never delete evidence.
    """
    if not db_path.exists():
        return 0
    uri = f"file:{db_path.as_posix()}?mode=ro"
    try:
        with sqlite3.connect(uri, uri=True, timeout=10) as conn:
            row = conn.execute(
                "SELECT COUNT(*) FROM health_diagnostics "
                "WHERE component = 'webview' AND status = 'ready'"
            ).fetchone()
        return int(row[0]) if row else 0
    except sqlite3.Error:
        # The table may not exist yet on a first launch; that is simply zero.
        return 0


def read_latest_marker(db_path: Path) -> dict | None:
    uri = f"file:{db_path.as_posix()}?mode=ro"
    try:
        with sqlite3.connect(uri, uri=True, timeout=10) as conn:
            conn.row_factory = sqlite3.Row
            row = conn.execute(
                "SELECT id, details_json, created_at FROM health_diagnostics "
                "WHERE component = 'webview' AND status = 'ready' "
                "ORDER BY created_at DESC, id DESC LIMIT 1"
            ).fetchone()
    except sqlite3.Error:
        return None
    if row is None:
        return None
    details: dict = {}
    try:
        details = json.loads(row["details_json"] or "{}")
    except json.JSONDecodeError:
        details = {"raw": row["details_json"]}
    return {
        "id": row["id"],
        "created_at": row["created_at"],
        "details": details,
    }


def window_title(pid: int) -> str | None:
    """The main window title, read from the OS rather than from the app."""
    if platform.system() != "Windows":
        return None
    script = (
        "$p = Get-Process -Id %d -ErrorAction SilentlyContinue; "
        "if ($p) { $p.MainWindowTitle }" % pid
    )
    try:
        completed = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    title = completed.stdout.strip()
    return title or None


def build_report() -> dict:
    bundles = []
    if BUNDLE_DIR.exists():
        for path in sorted(BUNDLE_DIR.rglob("*")):
            if path.is_file() and path.suffix.lower() in {".msi", ".exe"}:
                bundles.append(
                    {
                        "path": str(path.relative_to(REPO_ROOT)),
                        "bytes": path.stat().st_size,
                        "sha256": sha256_file(path),
                    }
                )
    return {
        "artifact": str(ARTIFACT.relative_to(REPO_ROOT)),
        "artifact_bytes": ARTIFACT.stat().st_size,
        "artifact_sha256": sha256_file(ARTIFACT),
        "bundles": bundles,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=None)
    parser.add_argument("--timeout", type=float, default=90.0)
    args = parser.parse_args()

    if platform.system() != "Windows":
        print(f"SKIPPED_NOT_APPLICABLE(platform={platform.system()}): "
              "the packaged desktop launch check targets Windows in this environment")
        return 3

    if not ARTIFACT.exists():
        print(f"packaged artifact is absent at {ARTIFACT}; run `sh scripts/build.sh` first")
        return 4

    report = build_report()
    data_dir = app_data_dir()
    db_path = data_dir / "vector.db"
    report["app_data_dir"] = str(data_dir)
    report["database"] = str(db_path)

    before = count_markers(db_path)
    report["markers_before"] = before

    started = time.monotonic()
    process = subprocess.Popen(
        [str(ARTIFACT)],
        cwd=str(REPO_ROOT),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    observed_title: str | None = None
    deadline = time.monotonic() + args.timeout
    try:
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            if observed_title is None:
                observed_title = window_title(process.pid)
            if count_markers(db_path) > before:
                break
            time.sleep(0.5)
    finally:
        elapsed = time.monotonic() - started
        alive = process.poll() is None
        if alive:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=15)
        report["exited_early"] = not alive

    after = count_markers(db_path)
    marker = read_latest_marker(db_path)

    report.update(
        {
            "launch_seconds": round(elapsed, 2),
            "window_title": observed_title,
            "markers_after": after,
            "marker": marker,
        }
    )

    report["webview_reached_command_layer"] = after > before
    report["database_created"] = db_path.exists()

    ok = (
        report["database_created"]
        and report["webview_reached_command_layer"]
        and not report["exited_early"]
    )
    report["verdict"] = "pass" if ok else "fail"

    text = json.dumps(report, indent=2)
    print(text)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text + "\n", encoding="utf-8")

    if not ok:
        if report["exited_early"]:
            print(
                f"artifact exited with code {process.returncode} before the webview "
                "reached the command layer",
                file=sys.stderr,
            )
        elif not report["webview_reached_command_layer"]:
            print(
                "the webview did not record a readiness marker: the interface "
                "either did not mount or could not reach the Rust command layer "
                "(check the Content-Security-Policy and the handler list)",
                file=sys.stderr,
            )
        return 1

    print(
        "live-fire: the packaged artifact launched, the webview mounted, and the "
        f"command layer answered (marker {report['marker']['id']})",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
