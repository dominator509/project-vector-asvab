"""Download Project Gutenberg works in the form this corpus cites them.

The provenance check re-downloads each work from `https://www.gutenberg.org/cache/epub/<id>/pg<id>.txt`
and compares the bytes with the local file, so that is the form to fetch here: a work fetched
from a different URL form would be a work the check cannot re-derive.

The ebook number is not a guess. A wrong one leaves a citation that looks authoritative and
cannot be re-checked -- ten Paragraph Comprehension items spent two rounds citing #2009 for a
file that is #1228 -- so the number is read out of the file's own Gutenberg header after the
download and the fetch fails when the two disagree.

Usage:
    python3 scripts/probes/fetch-gutenberg.py <ebook-number>[:<name>] [...]
    python3 scripts/probes/fetch-gutenberg.py --into sources/gutenberg 38015:agricola-de-re-metallica
"""

import hashlib
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_INTO = ROOT / "sources/gutenberg"
URL = "https://www.gutenberg.org/cache/epub/{id}/pg{id}.txt"
USER_AGENT = {"User-Agent": "project-vector/0.1"}
# `Title: The book of wonders` / `Release Date: ... [eBook #75948]`
TITLE = re.compile(r"^Title:\s*(.+)$", re.MULTILINE)
EBOOK = re.compile(r"\[eBook #(\d+)\]")


def fetch(ebook_id: str, name: str, into: Path) -> tuple[str, str, str] | None:
    request = urllib.request.Request(URL.format(id=ebook_id), headers=USER_AGENT)
    try:
        with urllib.request.urlopen(request, timeout=180) as response:
            body = response.read()
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as error:
        print(f"#{ebook_id:<8} {name:<32} FAILED: {error}")
        return None
    text = body.decode("utf-8", "replace")
    found = EBOOK.search(text)
    if found is None:
        print(f"#{ebook_id:<8} {name:<32} FAILED: no Gutenberg header, so the id cannot be checked")
        return None
    if found.group(1) != ebook_id:
        print(
            f"#{ebook_id:<8} {name:<32} FAILED: the file says it is #{found.group(1)}, "
            "so citing it as this number would be a citation nobody can re-check"
        )
        return None
    title_line = TITLE.search(text)
    title = title_line.group(1).strip() if title_line else name
    digest = hashlib.sha256(body).hexdigest()
    path = into / f"{name}.txt"
    path.write_bytes(body)
    return digest, str(path), title


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
    for argument in argv:
        ebook_id, _, name = argument.partition(":")
        if not name:
            print(f"{argument}: needs a name, as <ebook-number>:<name>")
            failures += 1
            continue
        result = fetch(ebook_id, name, into)
        if result is None:
            failures += 1
            continue
        digest, path, title = result
        print(f"#{ebook_id:<8} {name:<32} sha256={digest[:16]}… {title[:44]}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
