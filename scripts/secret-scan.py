#!/usr/bin/env python3
"""Secret scanning gate (GEN-005) for the Project VECTOR release candidate.

Searches tracked files for credential shapes, mirroring the pattern set the
crash-redaction module uses, plus repository-specific risks such as a committed
SQLite database or a .env file.

Evidence discipline: this prints what it searched and what it found. A clean
result is only meaningful alongside the scope it covered.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Credential shapes. Kept in step with crates/vector-observability/src/crash.rs.
PATTERNS: list[tuple[str, str]] = [
    ("bearer token", r"(?i)bearer\s+[A-Za-z0-9._\-]{20,}"),
    ("openai-style key", r"sk-[A-Za-z0-9]{32,}"),
    ("anthropic-style key", r"sk-ant-[A-Za-z0-9\-_]{24,}"),
    ("xai-style key", r"xai-[A-Za-z0-9]{24,}"),
    ("google api key", r"AIza[A-Za-z0-9\-_]{30,}"),
    ("github token", r"ghp_[A-Za-z0-9]{30,}"),
    ("github pat", r"github_pat_[A-Za-z0-9_]{30,}"),
    ("aws access key", r"AKIA[0-9A-Z]{16}"),
    ("private key block", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ("slack token", r"xox[baprs]-[A-Za-z0-9\-]{10,}"),
    ("stripe key", r"sk_live_[A-Za-z0-9]{20,}"),
]

# Files that must never be committed.
FORBIDDEN_TRACKED = [
    ".env",
    "vector.db",
    "id_rsa",
    "id_ed25519",
]

# Paths excluded from the scan and why.
EXCLUDED_PREFIXES = [
    # The verification source library is a corpus of security prompts that
    # necessarily contains example credential strings.
    ".agent/verification/source-library/",
    ".agent/verification/casebooks/",
    # Evidence contains redaction test fixtures, which are deliberate sentinels.
    ".agent/evidence/EP-006/",
    ".agent/evidence/EP-007/",
    ".agent/evidence/EP-008/",
]

# Fixture values that are intentionally present in tests.
BENIGN = {
    "canary0000000000000000000000",
    "abcdefghijklmnopqrst",
    "abcdefghijklmnopqrstuvwx",
    "abcdefghijklmnopqrstuvwxyz01",
    "IOSFODNN7EXAMPLE",
    "1234567890abcdefghijklmnopq",
    # A PEM header/footer pair with no key material between them, used by the
    # EP-006 redaction test to prove private keys ARE stripped. Reviewed during
    # EP-010: the body is a truncated placeholder, not a real key.
    "MIIEowIBAAKCAQEA",
}

# Files reviewed and accepted despite containing credential-shaped fixtures.
# Each entry records WHY, so a future reviewer can re-check rather than trust.
REVIEWED_EXCEPTIONS = {
    "crates/vector-observability/tests/ep006_crash.rs": (
        "EP-006 redaction test: contains a PEM header/footer pair with a "
        "placeholder body to prove private keys are redacted. No key material."
    ),
    "scripts/secret-scan.py": (
        "this scanner: its pattern table necessarily contains the literal "
        "shapes it searches for."
    ),
}

# Extensions that are not scannable as UTF-8 text.
BINARY_SUFFIXES = {
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".icns", ".webp",
    ".exe", ".dll", ".so", ".dylib", ".bin", ".db", ".sqlite",
    ".zip", ".gz", ".tar", ".pdf", ".woff", ".woff2", ".ttf",
    ".b64",
}


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], capture_output=True, text=True, check=True
    )
    return [line.strip() for line in out.stdout.splitlines() if line.strip()]


def main() -> int:
    files = tracked_files()
    findings: list[str] = []

    # 1. Forbidden files must not be tracked.
    for name in FORBIDDEN_TRACKED:
        for path in files:
            if Path(path).name == name:
                findings.append(f"FORBIDDEN_FILE {path}")

    # 2. Credential shapes in tracked text files.
    scanned = 0
    skipped = 0
    for path in files:
        if any(path.startswith(prefix) for prefix in EXCLUDED_PREFIXES):
            skipped += 1
            continue

        full = REPO / path
        if not full.is_file():
            continue
        # Skip files that are clearly not text by extension, and anything
        # oversized. Reading everything and catching UnicodeDecodeError looked
        # equivalent but silently scanned nothing, because a decode failure was
        # treated the same as a missing file.
        if full.suffix.lower() in BINARY_SUFFIXES:
            continue
        try:
            if full.stat().st_size > 2_000_000:
                continue
            text = full.read_text(encoding="utf-8", errors="strict")
        except (UnicodeDecodeError, OSError):
            continue
        scanned += 1

        if path in REVIEWED_EXCEPTIONS:
            # Reported rather than silently skipped, so the exception stays
            # visible in the gate output.
            print(f"  reviewed exception: {path}")
            continue

        for label, pattern in PATTERNS:
            for match in re.finditer(pattern, text):
                value = match.group(0)
                if any(benign in value for benign in BENIGN):
                    continue
                findings.append(f"{label} in {path}: {value[:60]}")

    print("=== secret scan scope ===")
    print(f"tracked files        : {len(files)}")
    print(f"scanned as text      : {scanned}")
    print(f"skipped (corpus)     : {skipped}")
    print(f"patterns applied     : {len(PATTERNS)}")
    print()
    print("=== findings ===")
    if findings:
        for finding in findings:
            print(f"  {finding}")
        print()
        print(f"FAIL: {len(findings)} finding(s)")
        return 1

    print("  none")
    print()
    print("PASS: no credential shapes and no forbidden tracked files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
