"""Fetch the whole study-corpus source tree, and record how to re-derive every byte.

The corpus is ingested, not authored: `scripts/rebuild-corpus.py` rebuilds it from
`sources/`, and `sources/` is gitignored (`.gitignore:54`) because the tree is tens of
megabytes of third-party prose. That is the right call for the repository and the wrong
one for reproducibility on its own -- it left the rebuild dependent on a directory nobody
had a script to create, so the banks could not be regenerated on a fresh checkout and the
defects they carry could not be fixed at their source.

This is that script. It fetches, from each work's canonical public distribution:

  * the Project Gutenberg works -- Paragraph Comprehension's manifest, the Shop and Auto
    Information manuals, and the 1913 Webster dictionary;
  * the Moby Thesaurus II data file (Grady Ward, public domain), which PG #3202 links
    from its own page as `files/mthesaur.txt`;
  * the Internet Archive scans -- the 24 NEETS modules (Electronics Information) and the
    four federal tool/vehicle manuals.

and writes `sources/MANIFEST.sha256.json`: one record per file with its canonical URL,
its SHA-256 and its size, so a reviewer can re-fetch and compare. A rebuild is then:

    python3 scripts/fetch-sources.py          # ~2-4 minutes, ~250 MB
    python3 scripts/rebuild-corpus.py <db>    # regenerates the corpus

Every Gutenberg file is checked the way `scripts/probes/verify-gutenberg-provenance.py`
checks it -- the download must carry a `<title> [eBook #<id>]</title>` header naming the
id it was asked for -- so a wrong number cannot become an authoritative-looking citation.

Usage:
    python3 scripts/fetch-sources.py [--into sources] [--only gutenberg,archive,thesaurus]
"""

import argparse
import hashlib
import json
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
GUTENBERG = "https://www.gutenberg.org/cache/epub/{id}/pg{id}.txt"
USER_AGENT = {"User-Agent": "project-vector/0.1 (corpus source fetch)"}
EBOOK = re.compile(r"\[eBook #(\d+)\]")
TITLE = re.compile(r"^Title:\s*(.+)$", re.MULTILINE)

# The Paragraph Comprehension manifest: fetched with the corpus, cited by the ingester and
# re-checked by the provenance probe, so the citation and the file cannot drift apart.
PC_MANIFEST = ROOT / ".agent/evidence/EP-007/content-corpus/gutenberg-manifest.txt"

# Shop Information and Auto Information manuals, as `<source>:<id>:<title>=<path>`.
# Written down rather than guessed from the id, because an all-digit id is a Gutenberg
# ebook number *and* a plausible Internet Archive identifier.
SHOP_MANUALS = [
    ("archive", "micro_IA41153156_0308", "Tools and Their Uses (US Army, 1971)", "sources/federal/tools-and-uses.txt"),
    ("archive", "TM11-453", "TM 11-453 Shop Work (US Army)", "sources/federal/army-shop-work.txt"),
    ("gutenberg", "76925", "Elementary Lathe Practice (T. J. Palmateer)", "sources/gutenberg/palmateer-lathe-practice.txt"),
    ("gutenberg", "57120", "The Economy of Workshop Manipulation (John Richards)", "sources/gutenberg/richards-workshop-manipulation.txt"),
    ("gutenberg", "69061", "Precision Locating and Dividing Methods (Anonymous)", "sources/gutenberg/precision-locating-dividing.txt"),
    ("gutenberg", "39791", "Farm Mechanics (Herbert A. Shearer)", "sources/gutenberg/shearer-farm-mechanics.txt"),
    ("gutenberg", "28553", "How it Works (Archibald Williams)", "sources/gutenberg/williams-how-it-works.txt"),
]

AUTO_MANUALS = [
    ("archive", "tm-9-8000-principles-of-automotive-vehicles-1985", "TM 9-8000 Principles of Automotive Vehicles (US Army, 1985)", "sources/federal/tm9-8000.txt"),
    ("archive", "TM9-2700", "TM 9-2700 Principles of Automotive Vehicles (US Army)", "sources/federal/tm9-2700.txt"),
    ("gutenberg", "56776", "Practical Hand Book of Gas, Oil and Steam Engines (John B. Rathbun)", "sources/gutenberg/rathbun-gas-oil-steam-engines.txt"),
    ("gutenberg", "38415", "Gas-Engines and Producer-Gas Plants (R. E. Mathot)", "sources/gutenberg/mathot-gas-engines.txt"),
    ("gutenberg", "27286", "Gas and Oil Engines, Simply Explained (Walter C. Runciman)", "sources/gutenberg/runciman-gas-oil-engines.txt"),
    ("gutenberg", "59311", "Gas and Petroleum Engines (H. de Graffigny)", "sources/gutenberg/graffigny-gas-petroleum-engines.txt"),
    ("gutenberg", "46094", "The Romance of Modern Mechanism (Archibald Williams)", "sources/gutenberg/williams-modern-mechanism.txt"),
    ("gutenberg", "41160", "The Romance of Modern Invention (Archibald Williams)", "sources/gutenberg/williams-modern-invention.txt"),
    ("gutenberg", "46232", "The Boy's Book of New Inventions (Harry E. Maule)", "sources/gutenberg/maule-boys-book-inventions.txt"),
    ("gutenberg", "55482", "Machines at Work (Mary Elting)", "sources/gutenberg/elting-machines-at-work.txt"),
]

GENERAL_SCIENCE = ("gutenberg", "75948", "The Book of Wonders", "sources/gutenberg/book-of-wonders.txt")

# The NEETS collection: 24 documents in one Internet Archive item, so each file is named.
NEETS_ITEM = "neetsmodules_202003"

# The archive item file-name that backs each federal path, where it differs from the path.
ARCHIVE_FILES = {
    "sources/federal/tools-and-uses.txt": "micro_IA41153156_0308_djvu.txt",
    "sources/federal/army-shop-work.txt": "TM11-453_djvu.txt",
    "sources/federal/tm9-8000.txt": "TM9-8000_Principles_of_automotive_vehicles_1985_djvu.txt",
    "sources/federal/tm9-2700.txt": "TM9-2700_djvu.txt",
}

# The thesaurus is not a Gutenberg *ebook*: PG #3202 is a documentation page whose own
# "Download" section links the 30,259-line data file. The ingester parses that file
# (comma-separated root lines), so this is the URL it must be the bytes of.
THESAURUS_URL = "https://www.gutenberg.org/files/3202/files/mthesaur.txt"
DICTIONARY = ("gutenberg", "29765", "Webster's Unabridged Dictionary", "sources/webster-1913/pg29765.txt")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read(url: str, attempts: int = 4, timeout: int = 180) -> bytes | None:
    request = urllib.request.Request(url, headers=USER_AGENT)
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return response.read()
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as error:
            if attempt + 1 == attempts:
                print(f"    FAILED {url}: {error}")
                return None
            time.sleep(3 * (attempt + 1))
    return None


class Fetcher:
    def __init__(self, into: Path):
        self.into = into
        self.records: list[dict] = []
        self.failures: list[dict] = []

    def _write(self, path: Path, body: bytes, url: str, note: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)
        self.records.append({
            "path": str(path.relative_to(ROOT)),
            "url": url,
            "sha256": sha256(body),
            "bytes": len(body),
            "note": note,
        })

    def gutenberg(self, ebook_id: str, path: str, note: str) -> None:
        url = GUTENBERG.format(id=ebook_id)
        body = read(url)
        if body is None:
            self.failures.append({"url": url, "path": path, "reason": "download failed"})
            return
        text = body.decode("utf-8", "replace")
        found = EBOOK.search(text)
        if found is None or found.group(1) != str(ebook_id):
            why = "no Gutenberg header" if found is None else f"header says #{found.group(1)}"
            print(f"    REJECT #{ebook_id} {Path(path).name}: {why}")
            self.failures.append({"url": url, "path": path, "reason": why})
            return
        title_line = TITLE.search(text)
        self._write(ROOT / path, body, url, title_line.group(1).strip() if title_line else note)
        print(f"    OK #{ebook_id:<8} {Path(path).name:<42} {sha256(body)[:16]}…")

    def archive(self, identifier: str, path: str, note: str, file_name: str | None = None) -> None:
        name = file_name or f"{identifier}_djvu.txt"
        url = f"https://archive.org/download/{identifier}/{quote(name)}"
        body = read(url)
        if body is None or not body.strip():
            self.failures.append({"url": url, "path": path, "reason": "empty body"})
            return
        self._write(ROOT / path, body, url, note)
        print(f"    OK {Path(path).name:<42} {sha256(body)[:16]}…")

    def thesaurus(self, path: str) -> None:
        body = read(THESAURUS_URL, timeout=300)
        if body is None:
            self.failures.append({"url": THESAURUS_URL, "path": path, "reason": "download failed"})
            return
        lines = [line for line in body.decode("utf-8", "replace").splitlines() if line.strip()]
        self._write(ROOT / path, body, THESAURUS_URL, f"Moby Thesaurus II (Grady Ward), {len(lines)} root lines")
        print(f"    OK {Path(path).name:<42} {sha256(body)[:16]}… ({len(lines)} root lines)")


def pc_works() -> list[tuple[str, str]]:
    """The Paragraph Comprehension manifest, as (ebook-id, path) pairs."""
    pairs = []
    for raw in PC_MANIFEST.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        ebook_id, _, path = line.partition("=")
        if ebook_id.strip() and path.strip():
            pairs.append((ebook_id.strip(), path.strip()))
    return pairs


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--into", default="sources")
    parser.add_argument("--only", default="", help="comma list of: gutenberg,archive,thesaurus")
    arguments = parser.parse_args(argv)
    only = {name.strip() for name in arguments.only.split(",") if name.strip()}
    want = (lambda name: not only or name in only)

    fetcher = Fetcher(ROOT / arguments.into)

    if want("dictionary") or want("gutenberg"):
        print("== Project Gutenberg: Paragraph Comprehension manifest ==")
        for ebook_id, path in pc_works():
            fetcher.gutenberg(ebook_id, path, Path(path).stem)
        print("\n== Project Gutenberg: Shop / Auto manuals, science, dictionary ==")
        for family, ident, note, path in (SHOP_MANUALS + AUTO_MANUALS + [GENERAL_SCIENCE, DICTIONARY]):
            if family == "gutenberg":
                fetcher.gutenberg(ident, path, note)

    if want("archive"):
        print("\n== Internet Archive: federal manuals ==")
        for _, ident, note, path in (SHOP_MANUALS + AUTO_MANUALS):
            if _ == "archive":
                fetcher.archive(ident, path, note, ARCHIVE_FILES.get(path))
        print("\n== Internet Archive: NEETS modules (24) ==")
        meta = read(f"https://archive.org/metadata/{NEETS_ITEM}")
        if meta is None:
            fetcher.failures.append({"url": f"https://archive.org/metadata/{NEETS_ITEM}", "path": "sources/neets/", "reason": "metadata failed"})
        else:
            files = sorted(
                entry["name"] for entry in json.loads(meta).get("files", [])
                if entry.get("name", "").startswith("NEETS MOD") and entry.get("name", "").endswith("_djvu.txt")
            )
            for name in files:
                path = f"sources/neets/{name.replace(' ', '_')}"
                fetcher.archive(NEETS_ITEM, path, f"NEETS module: {name}", name)

    if want("thesaurus"):
        print("\n== Moby Thesaurus II ==")
        fetcher.thesaurus("sources/moby-thesaurus/words.txt")

    manifest = {
        "generated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "records": sorted(fetcher.records, key=lambda record: record["path"]),
        "failures": fetcher.failures,
    }
    # The tree itself is gitignored, so the manifest -- the record of what the build fetched --
    # goes to the tracked evidence directory, and a copy is left beside the sources.
    out = ROOT / ".agent/evidence/EP-007/content-corpus/source-manifest/MANIFEST.sha256.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    beside = ROOT / arguments.into / "MANIFEST.sha256.json"
    if beside.parent.exists():
        beside.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"\nwrote {out}: {len(fetcher.records)} file(s), {len(fetcher.failures)} failure(s)")
    return 1 if fetcher.failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
