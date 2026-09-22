"""Rebuild the whole study corpus from its sources, in one reproducible run.

The corpus is ingested, not authored: every item is built from a public-domain source and
cited to it, and the evidence vault records each source's URL, licence and digest. Rebuilding
is therefore a defined operation -- delete the items, ingest each subtest from its sources in
a fixed order with a fixed seed, and write the same reports the round logs carry -- and one
command that does it is worth more than six commands remembered.

It exists because two things about the ingestions are easy to get wrong by hand:

* the sources. The Paragraph Comprehension manifest is the *same file* the provenance check
  re-downloads from, so the citation and the check cannot drift apart; the Electronics
  Information modules and the Shop and Auto manuals are listed here in the order they were
  measured;
* the vault. A record whose URL does not resolve to the bytes it was recorded for is not
  evidence, and this script removes the one the corpus had: `darwin-origin.txt` is Project
  Gutenberg #1228, and ten Paragraph Comprehension items cited #2009, which is a different
  file of the same book. `verify-gutenberg-provenance.py` re-downloads every work and
  reports OK, MISMATCH or MISSING per work; run it after this script.

An item a learner has attempted is never deleted: attempts are the learner's own record. If
any exist, the script stops.

Usage:
    python3 scripts/rebuild-corpus.py <database> [--dry-run]
"""

import argparse
import json
import os
import sqlite3
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / ".agent/evidence/EP-007/content-corpus"
SEED = "20260922"
REVIEWER = "content-reviewer"

DICTIONARY = "sources/webster-1913/pg29765.txt"
THESAURUS = "sources/moby-thesaurus/words.txt"
MANIFEST = EVIDENCE / "gutenberg-manifest.txt"

# The Shop Information manuals, as `<archive-id>:<title>=<path>`.
SHOP_MANUALS = [
    "micro_IA41153156_0308:Tools and Their Uses (US Army, 1971)"
    "=sources/federal/tools-and-uses.txt",
    "TM11-453:TM 11-453 Shop Work (US Army)=sources/federal/army-shop-work.txt",
]

# The Auto Information manuals.
AUTO_MANUALS = [
    "TM9-8000:TM 9-8000 Principles of Automotive Vehicles (US Army, 1985)"
    "=sources/federal/tm9-8000.txt",
    "TM9-2700:TM 9-2700 Principles of Automotive Vehicles (US Army)"
    "=sources/federal/tm9-2700.txt",
]

EI_MODULES = sorted(
    f"sources/neets/{name}"
    for name in os.listdir(ROOT / "sources/neets")
    if name.startswith("NEETS_MOD_") and name.endswith(".txt")
)

GENERAL_SCIENCE = "75948=sources/gutenberg/book-of-wonders.txt"

# A vault record whose URL does not resolve to the bytes recorded for it. The entry is the
# corrected form: (superseded URL, why, what replaces it).
SUPERSEDED_SOURCES = [
    (
        "https://www.gutenberg.org/ebooks/2009",
        "the file is Project Gutenberg #1228; #2009 is a different file of the same book",
    ),
]


def run(command: list[str]) -> None:
    print(f"\n$ {' '.join(command)}")
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8", errors="replace")
    if result.returncode != 0:
        raise SystemExit(f"failed with exit {result.returncode}: {' '.join(command)}")


def works_from_manifest() -> list[str]:
    lines = [
        line.strip()
        for line in MANIFEST.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    ]
    if not lines:
        raise SystemExit(f"no works in {MANIFEST}")
    return lines


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database")
    parser.add_argument("--dry-run", action="store_true")
    arguments = parser.parse_args(argv)

    database = Path(arguments.database)
    if not database.exists():
        print(f"no such database: {database}")
        return 2

    connection = sqlite3.connect(database)
    connection.execute("PRAGMA foreign_keys = ON")
    attempts = connection.execute("SELECT COUNT(*) FROM attempts").fetchone()[0]
    if attempts:
        print(f"{attempts} attempt(s) reference items; rebuilding would delete a learner's record")
        return 1
    before = connection.execute("SELECT COUNT(*) FROM content_items").fetchone()[0]
    print(f"database: {database}")
    print(f"items before: {before}")

    if arguments.dry_run:
        print("dry run: nothing deleted, nothing ingested")
        return 0

    # 1. The items, and the vault records that are not evidence.
    connection.execute("DELETE FROM content_items")
    for url, why in SUPERSEDED_SOURCES:
        removed = connection.execute(
            "DELETE FROM evidence_records WHERE url = ?", (url,)
        ).rowcount
        if removed:
            print(f"removed {removed} vault record(s) for {url}: {why}")
    connection.commit()
    left = connection.execute("SELECT COUNT(*) FROM content_items").fetchone()[0]
    print(f"items after delete: {left}")
    connection.close()

    tools = ["cargo", "run", "-q", "-p", "vector-tools", "--", "content"]
    db = str(database)

    # 2. Word Knowledge: the thesaurus, corroborated by the dictionary.
    run(
        tools
        + [
            "ingest-wk",
            "--db", db,
            "--thesaurus", THESAURUS,
            "--dictionary", DICTIONARY,
            "--count", "2000",
            "--seed", SEED,
            "--reviewer", REVIEWER,
            "--out", str(EVIDENCE / "ingest-wk.json"),
        ]
    )

    # 3. Electronics Information: the NEETS module glossaries.
    #
    # The count is attempts, not items, and it is deliberately far larger than the bank: the
    # builder draws a definition per attempt, and at the 400 the earlier rounds used it built
    # 3,668 items that asked 409 distinct questions between them -- the same definition up to
    # seven times with its wrong answers shuffled. The store now asks each question once, and
    # the attempts are what find the rest: 3,000 per module builds about 10,000 items, asks 414
    # questions across the ten modules, and then saturates, which is the whole of what these
    # glossaries state in the shape an item can be built from.
    run(
        tools
        + [
            "ingest-ei",
            "--db", db,
            "--dictionary", DICTIONARY,
            "--count", "3000",
            "--min-definition-words", "5",
            "--seed", SEED,
            "--reviewer", REVIEWER,
            "--out", str(EVIDENCE / "ingest-ei.json"),
        ]
        + [item for module in EI_MODULES for item in ("--module", module)]
    )

    # 4. Paragraph Comprehension: every work in the manifest, one run for all of them.
    run(
        tools
        + [
            "ingest-pc",
            "--db", db,
            "--count", "400",
            "--seed", SEED,
            "--reviewer", REVIEWER,
            "--out", str(EVIDENCE / "ingest-pc.json"),
        ]
        + [item for work in works_from_manifest() for item in ("--work", work)]
    )

    # 5. General Science: the question-and-answer work.
    run(
        tools
        + [
            "ingest-facts",
            "--db", db,
            "--subtest", "GS",
            "--work", GENERAL_SCIENCE,
            "--count", "500",
            "--seed", SEED,
            "--reviewer", REVIEWER,
            "--out", str(EVIDENCE / "ingest-facts-gs.json"),
        ]
    )

    # 6. Shop Information, then Auto Information, from the component manuals. The counts are
    # the ones the rounds that introduced them recorded: 200 attempts per shop manual, 300 per
    # automotive manual, which is more attempts than either manual has descriptions.
    for subtest, ask, manuals, count, report in (
        ("SI", "tools", SHOP_MANUALS, "200", "ingest-tools-si.json"),
        ("AI", "functions", AUTO_MANUALS, "300", "ingest-tools-ai.json"),
    ):
        run(
            tools
            + [
                "ingest-tools",
                "--db", db,
                "--subtest", subtest,
                "--ask", ask,
                "--dictionary", DICTIONARY,
                "--count", count,
                "--seed", SEED,
                "--reviewer", REVIEWER,
                "--out", str(EVIDENCE / report),
            ]
            + [item for manual in manuals for item in ("--work", manual)]
        )

    # 7. Read the result back out of the store rather than out of the reports.
    connection = sqlite3.connect(database)
    after = connection.execute(
        "SELECT subtest, COUNT(*) FROM content_items WHERE state = 'active' "
        "GROUP BY subtest ORDER BY subtest"
    ).fetchall()
    connection.close()
    total = sum(count for _, count in after)
    print("\nitems after rebuild:")
    for subtest, count in after:
        print(f"  {subtest:<4} {count}")
    print(f"  total {total}")
    if total == 0:
        print("the rebuild stored nothing")
        return 1
    if total > before:
        print(f"note: the rebuild stored more than before ({before}), because the reader was widened")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
