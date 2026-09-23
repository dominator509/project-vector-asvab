"""Download the plain-text OCR of Internet Archive items into the source directory.

The corpus cites Internet Archive scans, and the citation is checkable only if the bytes are
the ones the archive serves: this fetches a file and prints its digest, and the ingester
records the same digest in the evidence vault.

Three forms, because the items differ in shape. A `micro_*` item is one document per
identifier; a collection item such as `neetsmodules_202003` holds twenty-four documents as
twenty-four files, so the file has to be named; and before either, a caller wants to know what
an item holds.

Usage:
    python3 scripts/probes/fetch-archive-text.py --list <archive-id>
    python3 scripts/probes/fetch-archive-text.py <archive-id>
    python3 scripts/probes/fetch-archive-text.py <archive-id>:<file-name> [...]
    python3 scripts/probes/fetch-archive-text.py --into sources/neets <id>:<file> [...]
"""

import hashlib
import json
import sys
import urllib.error
import urllib.request
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_INTO = ROOT / "sources/federal"
USER_AGENT = {"User-Agent": "project-vector/0.1"}


def read(url: str) -> bytes | None:
    request = urllib.request.Request(url, headers=USER_AGENT)
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            return response.read()
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as error:
        print(f"  FAILED: {error}")
        return None


def list_text_files(identifier: str) -> int:
    """Print the item's text derivatives, which are what this corpus can read."""
    body = read(f"https://archive.org/metadata/{identifier}")
    if body is None:
        return 1
    metadata = json.loads(body)
    files = [
        entry["name"]
        for entry in metadata.get("files", [])
        if entry.get("name", "").endswith((".txt", ".txt.gz"))
    ]
    print(f"{identifier}: {len(files)} text file(s)")
    for name in sorted(files):
        print(f"  {name}")
    return 0


def fetch(identifier: str, file_name: str, into: Path) -> tuple[str, str] | None:
    # The archive's own file names contain spaces, which a URL cannot carry.
    url = f"https://archive.org/download/{identifier}/{quote(file_name)}"
    body = read(url)
    if body is None or not body.strip():
        print(f"{file_name:<48} FAILED: empty body")
        return None
    digest = hashlib.sha256(body).hexdigest()
    # The archive names this collection's files with spaces, and a space in a source path is a
    # quoting hazard in every command that follows. The digest is the identity; the name is
    # normalised here so a rebuild's arguments are the same everywhere.
    path = into / file_name.replace(" ", "_")
    path.write_bytes(body)
    return digest, str(path)


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    into = DEFAULT_INTO
    if argv[:1] == ["--into"]:
        into = Path(argv[1])
        argv = argv[2:]
    if argv[:1] == ["--list"]:
        return list_text_files(argv[1]) if len(argv) == 2 else 2
    if not argv:
        print(__doc__)
        return 2
    into.mkdir(parents=True, exist_ok=True)
    failures = 0
    for argument in argv:
        identifier, _, named = argument.partition(":")
        file_name = named or f"{identifier}_djvu.txt"
        result = fetch(identifier, file_name, into)
        if result is None:
            failures += 1
            continue
        digest, path = result
        print(f"{file_name:<48} sha256={digest} {path}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
