"""Check that each recorded Project Gutenberg URL still yields the bytes we cite.

The evidence vault records a URL, a licence and a SHA-256 for every source an item
cites. That record is only worth something if the URL actually resolves to the
bytes: a wrong ebook number would leave a citation that looks authoritative and
cannot be re-checked.

This script re-downloads each work from the URL form the ingester records and
compares the digest with the local file. It reports, per work:

  OK         the download matches the local file byte for byte
  MISMATCH   the URL resolves but the content differs
  MISSING    the URL does not resolve

Usage:
    python3 scripts/probes/verify-gutenberg-provenance.py <id>=<path> [<id>=<path> ...]
    python3 scripts/probes/verify-gutenberg-provenance.py --manifest <file>

The manifest form reads lines of `<id>=<path>`, which is what the ingestion
invocation is built from, so the two cannot drift apart by transcription.
"""

import hashlib
import sys
import urllib.error
import urllib.request

URL = "https://www.gutenberg.org/cache/epub/{id}/pg{id}.txt"
TIMEOUT_SECONDS = 120


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fetch(ebook_id: str) -> bytes | None:
    request = urllib.request.Request(
        URL.format(id=ebook_id),
        headers={"User-Agent": "project-vector-provenance-check/1.0"},
    )
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            return response.read()
    except (urllib.error.HTTPError, urllib.error.URLError) as error:
        print(f"  {URL.format(id=ebook_id)}: {error}")
        return None


def check(pair: str) -> bool:
    ebook_id, _, path = pair.partition("=")
    if not ebook_id or not path:
        print(f"MALFORMED  {pair!r}: expected <id>=<path>")
        return False
    try:
        with open(path, "rb") as handle:
            local = handle.read()
    except OSError as error:
        print(f"MISSING    {path}: {error}")
        return False

    remote = fetch(ebook_id)
    if remote is None:
        print(f"MISSING    {path}: #{ebook_id} did not resolve")
        return False

    local_hash, remote_hash = digest(local), digest(remote)
    if local_hash != remote_hash:
        print(
            f"MISMATCH   {path}: local {local_hash[:16]}… "
            f"but #{ebook_id} serves {remote_hash[:16]}…"
        )
        return False

    title = ""
    for line in remote.decode("utf-8", errors="replace").splitlines()[:5]:
        if "The Project Gutenberg eBook of " in line:
            title = line.split("The Project Gutenberg eBook of ", 1)[1].strip()
            break
    print(f"OK         #{ebook_id:<6} {local_hash[:16]}…  {title or path}")
    return True


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2

    if argv[0] == "--manifest":
        if len(argv) != 2:
            print("--manifest takes exactly one file")
            return 2
        with open(argv[1], encoding="utf-8") as handle:
            pairs = [
                line.strip()
                for line in handle
                if line.strip() and not line.startswith("#")
            ]
    else:
        pairs = argv

    results = [check(pair) for pair in pairs]
    ok = sum(1 for result in results if result)
    print(f"\n{ok}/{len(results)} work(s) re-download to the bytes they are cited as")
    return 0 if ok == len(results) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
