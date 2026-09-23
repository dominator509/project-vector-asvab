"""Drive the packaged application through a real study journey, in its own process.

DOD-004's residual was that the Playwright suite runs against the built frontend bundle with a
stubbed backend, not inside the packaged executable. This probe closes that gap for the journey
that matters: it starts the release binary under `tauri-driver`, drives its WebView2 with the
WebDriver protocol, and reads the effect out of the application's own database with a separate
process.

What that proves that the live-fire marker does not: the packaged window is not merely reachable
(it writes a marker through IPC), it is *drivable* -- a learner can be created, a plan read, a
question served from the installed pack, answered, and the attempt stored -- with the real Rust
command layer behind every step and no stub anywhere.

No test-runner dependency: the WebDriver protocol is JSON over HTTP, and this speaks it directly
with `urllib`, for the same reason the local-model probe speaks HTTP to llama.cpp rather than
adding a client crate.

**Measured in round 35, and why this probe is not a gate.** With `tauri-driver 3.0.0-alpha.0`
started against `msedgedriver 153.0.4234.48` -- the version matching the installed WebView2 runtime
-- a session is created with one window handle and `GET /source` returns
`<html><head></head><body></body></html>` (39 characters), unchanged at every sample over thirty
seconds. The packaged window is reachable; its WebView2 document is not exposed to the driver in
this environment, so the journey cannot be driven here. The attempt and its measurement are in
`.agent/evidence/EP-009/packaged-journey/ATTEMPT.md`, and DOD-004 stays PARTIAL with that residual.
On a machine whose driver does expose the document, this is the missing half of the clause: it
creates a learner through the real onboarding form, reads the plan, answers a question served from
the installed pack, and reads the attempt back out of the application's own database.

Usage:
    python3 scripts/probes/packaged-journey.py [--driver http://127.0.0.1:4444]
                                              [--app target/release/vector-desktop.exe]
                                              [--out <report json>] [--keep]

The driver must already be running:
    tauri-driver --native-driver <path to msedgedriver.exe> --port 4444
"""

import argparse
import json
import os
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CANARY = "webdriver-journey"


class WebDriverError(RuntimeError):
    pass


class WebDriver:
    """The subset of the WebDriver protocol this journey needs."""

    def __init__(self, endpoint: str):
        self.endpoint = endpoint.rstrip("/")
        self.session: str | None = None

    def _call(self, method: str, path: str, body: dict | None = None) -> dict:
        url = f"{self.endpoint}{path}"
        data = json.dumps(body).encode("utf-8") if body is not None else None
        request = urllib.request.Request(url, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                payload = json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", errors="replace")
            raise WebDriverError(f"{method} {path} -> {error.code}: {detail[:400]}") from error
        except urllib.error.URLError as error:
            raise WebDriverError(f"{method} {path} -> {error}") from error
        value = payload.get("value")
        if isinstance(value, dict) and value.get("error"):
            raise WebDriverError(f"{method} {path} -> {value.get('error')}: {value.get('message')}")
        return payload

    def start(self, application: Path) -> None:
        payload = self._call(
            "POST",
            "/session",
            {
                "capabilities": {
                    "alwaysMatch": {
                        "tauri:options": {"application": str(application)},
                    }
                }
            },
        )
        session_id = payload.get("value", {}).get("sessionId") or payload.get("sessionId")
        if not session_id:
            raise WebDriverError(f"the driver created no session: {json.dumps(payload)[:300]}")
        self.session = session_id

    def stop(self) -> None:
        if self.session:
            try:
                self._call("DELETE", f"/session/{self.session}")
            except WebDriverError:
                pass
            self.session = None

    def find(self, selector: str, timeout: float = 25.0) -> str:
        """Wait for one element and return its id."""
        deadline = time.time() + timeout
        last = ""
        while time.time() < deadline:
            try:
                payload = self._call(
                    "POST",
                    f"/session/{self.session}/element",
                    {"using": "css selector", "value": selector},
                )
                element = payload.get("value", {}).get("element-6066-11e4-a52e-4f735466cecf")
                if element:
                    return element
                last = json.dumps(payload)[:200]
            except WebDriverError as error:
                last = str(error)
            time.sleep(0.4)
        raise WebDriverError(f"no element matched {selector!r} within {timeout}s: {last}")

    def click(self, selector: str) -> None:
        element = self.find(selector)
        self._call("POST", f"/session/{self.session}/element/{element}/click", {})

    def click_xpath(self, expression: str, timeout: float = 25.0) -> None:
        """Click by XPath, for controls that carry only their accessible text."""
        deadline = time.time() + timeout
        last = ""
        while time.time() < deadline:
            try:
                payload = self._call(
                    "POST",
                    f"/session/{self.session}/element",
                    {"using": "xpath", "value": expression},
                )
                element = payload.get("value", {}).get("element-6066-11e4-a52e-4f735466cecf")
                if element:
                    self._call("POST", f"/session/{self.session}/element/{element}/click", {})
                    return
                last = json.dumps(payload)[:200]
            except WebDriverError as error:
                last = str(error)
            time.sleep(0.4)
        raise WebDriverError(f"no element matched {expression!r} within {timeout}s: {last}")

    def type(self, selector: str, text: str) -> None:
        element = self.find(selector)
        self._call(
            "POST",
            f"/session/{self.session}/element/{element}/value",
            {"text": text, "value": list(text)},
        )

    def text_of(self, selector: str) -> str:
        element = self.find(selector)
        payload = self._call("GET", f"/session/{self.session}/element/{element}/text")
        return str(payload.get("value", ""))

    def wait_for_text(self, selector: str, expected: str, timeout: float = 40.0) -> str:
        deadline = time.time() + timeout
        last = ""
        while time.time() < deadline:
            try:
                last = self.text_of(selector)
                if expected in last:
                    return last
            except WebDriverError:
                pass
            time.sleep(0.4)
        raise WebDriverError(f"{selector!r} never contained {expected!r}; last saw {last!r}")


def app_data_dir() -> Path:
    appdata = os.environ.get("APPDATA")
    if not appdata:
        raise SystemExit("APPDATA is not set, so the application data path is unknown")
    return Path(appdata) / "com.vector.app"


def read_journey(db_path: Path, learner_name: str) -> dict:
    """Read the journey's effect back, in a connection this probe owns."""
    connection = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        rows = connection.execute(
            "SELECT id, name, target_score FROM learner_profile WHERE name = ?", (learner_name,)
        ).fetchall()
        if not rows:
            return {"found": 0}
        learner_id, name, target = rows[0]
        attempts = connection.execute(
            "SELECT COUNT(*) FROM attempts WHERE learner_id = ?", (learner_id,)
        ).fetchone()[0]
        marker = connection.execute(
            "SELECT COUNT(*) FROM health_diagnostics WHERE detail LIKE '%ui_ready%'"
        ).fetchone()[0]
        return {
            "found": len(rows),
            "learner_id": learner_id,
            "name": name,
            "target_score": target,
            "attempts": attempts,
            "ui_ready_markers": marker,
        }
    finally:
        connection.close()


def cleanup(db_path: Path, learner_name: str) -> int:
    """Remove the journey's learner and attempts, so the installation is left as it was found."""
    connection = sqlite3.connect(db_path)
    try:
        ids = [
            row[0]
            for row in connection.execute(
                "SELECT id FROM learner_profile WHERE name = ?", (learner_name,)
            )
        ]
        removed = 0
        for learner_id in ids:
            removed += connection.execute(
                "DELETE FROM attempts WHERE learner_id = ?", (learner_id,)
            ).rowcount
            connection.execute("DELETE FROM mastery WHERE learner_id = ?", (learner_id,))
            connection.execute("DELETE FROM learner_profile WHERE id = ?", (learner_id,))
        connection.commit()
        return removed
    finally:
        connection.close()


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", default="http://127.0.0.1:4444")
    parser.add_argument("--app", default=str(ROOT / "target" / "release" / "vector-desktop.exe"))
    parser.add_argument(
        "--out", default=str(ROOT / ".agent/evidence/EP-009/packaged-journey/journey.json")
    )
    parser.add_argument("--keep", action="store_true", help="leave the learner in place")
    arguments = parser.parse_args(argv)

    application = Path(arguments.app)
    if not application.exists():
        print(f"the release artifact is absent at {application}; run `sh scripts/build.sh` first")
        return 4
    db_path = app_data_dir() / "vector.db"
    if not db_path.exists():
        print(f"no application database at {db_path}")
        return 2

    # A learner name nobody has used, so a previous run cannot be mistaken for this one.
    suffix = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
    ).stdout.strip() or str(int(time.time()))
    learner = f"{CANARY}-{suffix}"
    started = time.time()
    driver = WebDriver(arguments.driver)
    steps: list[dict] = []

    def step(name: str, detail: str) -> None:
        steps.append({"step": name, "detail": detail, "at_seconds": round(time.time() - started, 2)})
        print(f"  {name:<28} {detail}")

    try:
        driver.start(application)
        step("session", f"packaged {application.name} under tauri-driver")

        # The shell renders its heading before any command answers.
        heading = driver.wait_for_text("h1", "Project VECTOR")
        step("window", heading.strip())

        # 1. Create a learner through the real onboarding form.
        driver.click('button[data-view="onboarding"]')
        driver.type('input[type="text"]', learner)
        driver.click('button[type="submit"]')
        step("learner created", learner)
        seen = read_journey(db_path, learner)
        if not seen.get("found"):
            raise WebDriverError("the packaged application stored no learner")
        step("read back", f"learner {seen['learner_id']} target {seen['target_score']}")

        # 2. The plan is computed from the corpus the installation holds.
        driver.click('button[data-view="today"]')
        plan = driver.wait_for_text('[data-testid="today-drills"]', "")
        step("plan", " ".join(plan.split())[:120])

        # 3. A question served from the installed pack, answered in the packaged window.
        driver.click('button[data-view="practice"]')
        question = driver.wait_for_text('[data-testid="practice-corpus-summary"]', "")
        step("practice", question.strip())
        driver.click("label.option-row input")
        # The check-answer control has no test id; it is the button after the options.
        driver.click_xpath("//button[normalize-space()='Check answer']")
        solution = driver.wait_for_text('[data-testid="worked-solution"]', "")
        step("answered", " ".join(solution.split())[:120])

        # 4. The attempt is in the application's own database.
        deadline = time.time() + 30
        after = read_journey(db_path, learner)
        while time.time() < deadline and not after.get("attempts"):
            time.sleep(0.5)
            after = read_journey(db_path, learner)
        step(
            "attempt stored",
            f"{after.get('attempts', 0)} attempt(s), {after.get('ui_ready_markers', 0)} IPC marker(s)",
        )
        if not after.get("attempts"):
            raise WebDriverError("the packaged application stored no attempt")
    except WebDriverError as error:
        print(f"packaged journey failed: {error}")
        report = {
            "passed": False,
            "error": str(error),
            "steps": steps,
            "application": str(application),
            "learners": read_journey(db_path, learner),
        }
        out = Path(arguments.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        driver.stop()
        return 1
    finally:
        driver.stop()

    result = read_journey(db_path, learner)
    removed = 0
    if not arguments.keep:
        removed = cleanup(db_path, learner)
    report = {
        "passed": True,
        "clause": "DOD-004",
        "application": str(application),
        "driver": arguments.driver,
        "learner": learner,
        "readback": result,
        "cleaned_up_attempts": removed,
        "seconds": round(time.time() - started, 2),
        "steps": steps,
        "note": (
            "the journey ran inside the packaged process: every step below the window was the "
            "real Rust command layer, with no stubbed backend anywhere in the path"
        ),
    }
    out = Path(arguments.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print()
    print(f"packaged journey passed in {report['seconds']}s; cleaned up {removed} attempt(s)")
    print(f"report {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
