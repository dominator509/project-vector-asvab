# DOD-004: driving the packaged application

```json
{
  "clause": "DOD-004",
  "question": "can the packaged application be driven through a real study journey from outside?",
  "setup": {
    "driver": "tauri-driver 3.0.0-alpha.0 (crates.io, Apache-2.0 OR MIT), started with --native-driver",
    "native_driver": "msedgedriver 153.0.4234.48, matching the installed WebView2 runtime 153.0.4234.48",
    "application": "target/release/vector-desktop.exe (the release artifact)",
    "driver_status": {
      "ready": true,
      "message": "msedgedriver ready for new sessions."
    }
  },
  "measured": {
    "session": "created, one window handle",
    "page_source": "<html><head></head><body></body></html> (39 characters)",
    "polling": "every 3 seconds for 30 seconds: unchanged at every sample",
    "conclusion": "the packaged window is reachable and the WebView2 document is not exposed to the driver in this environment, so a user journey cannot be driven inside the packaged process here"
  },
  "what_is_still_proven_about_the_packaged_process": {
    "evidence": ".agent/evidence/EP-001/desktop-live-fire.json (sweep gate `live-fire`)",
    "detail": "launching the packaged executable writes a ui_ready marker through the real IPC command layer, and a separate process reads it back from the application database, so the packaged webview does reach the Rust commands"
  },
  "what_the_browser_suite_covers_instead": "the Playwright suite drives the built frontend bundle with a stubbed command boundary, which is a different claim from driving the packaged process and is recorded as such",
  "carried_forward": "scripts/probes/packaged-journey.py is the tool for this clause: it creates a learner, reads the plan, answers a question and reads the attempt back, and it fails loudly with a report when the driver does not expose the DOM (this run). On a machine whose driver does, it is the missing half of DOD-004"
}
```
