"""Download the plain-text OCR of Internet Archive items into the source directory.

The corpus cites Internet Archive scans, and the citation is checkable only if the bytes
are the ones the archive serves: this fetches the item's own `_djvu.txt` derivative and
records its digest, and the ingester records the same digest in the evidence vault.

Usage:
    python3 scripts/probes/fetch-archive-text.py <archive-id>[:<file-name>] [...]
    python3 scripts/probes/fetch-archive-text.py --into sources/federal <id> [...]
"""

import hashlib
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_INTO = ROOT / "sources/federal"


def fetch(identifier: str, into: Path) -> tuple[str, str] | None:
    url = f"https://archive.org/download/{identifier}/{identifier}_djvu.txt"
    request = urllib.request.Request(url, headers={"User-Agent": "project-vector/0.1"})
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            body = response.read()
    except (urllib.error.HTTPError, urllib.error.URLError) as error:
        print(f"{identifier:<28} FAILED: {error}")
        return None
    if not body.strip():
        print(f"{identifier:<28} FAILED: empty body")
        return None
    digest = hashlib.sha256(body).hexdigest()
    path = into / f"{identifier}.txt"
    path.write_bytes(body)
    return digest, str(path)


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    into = DEFAULT_INTO
    if argv[:1] == ["--into"]:
        into = Path(argv[1])
        argv = argv[2:]
    if not argv:
        print(__doc__)
        return 2
    into.mkdir(parents=True, exist_ok=True)
    failures = 0
    for identifier in argv:
        result = fetch(identifier, into)
        if result is None:
            failures += 1
            continue
        digest, path = result
        print(f"{identifier:<28} sha256={digest} {path}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
