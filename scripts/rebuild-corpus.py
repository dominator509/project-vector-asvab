"""Rebuild the study corpus from its sources, in one reproducible run.

The corpus is ingested, not authored: every item is built from a public-domain source and
cited to it, and the evidence vault records each source's URL, licence and digest. Rebuilding
is therefore a defined operation -- delete the items, ingest each subtest from its sources in
a fixed order with a fixed seed, and write the same reports the round logs carry -- and one
command that does it is worth more than six commands remembered.

Three subtests are the exception, and they are the reason this script also *generates*: the
computable ones (Arithmetic Reasoning, Mathematics Knowledge, Mechanical Comprehension) have
templates rather than sources, so their items exist only when something asks the factory for
them. Generated through the same pipeline the application uses, with fixed seeds, so the same
command produces the same items -- and the content pack carries them, which is what lets a
study plan name an objective for those subtests and a fresh installation serve one without
first pressing "prepare questions".

It exists because three things about the ingestions are easy to get wrong by hand:

* **the sources.** The Paragraph Comprehension manifest is the *same file* the provenance
  check re-downloads from, so the citation and the check cannot drift apart; the Electronics
  Information modules and the Shop and Auto manuals are listed here in the order they were
  measured;
* **the order.** A builder's draws depend on the order its sources are ingested in, because
  each module is seeded with its index. Two orders produce two corpora -- the same 24 modules
  came out as 1,566 Electronics Information questions under one shell's ordering and 1,508
  under another's -- so the order lives here, sorted, rather than in whatever the caller's
  glob happened to return;
* **the vault.** A record whose URL does not resolve to the bytes it was recorded for is not
  evidence, and this script removes the one the corpus had: `darwin-origin.txt` is Project
  Gutenberg #1228, and ten Paragraph Comprehension items cited #2009, which is a different
  file of the same book. `verify-gutenberg-provenance.py` re-downloads every work and reports
  OK, MISMATCH or MISSING per work; run it after this script.

An item a learner has attempted is never deleted: attempts are the learner's own record, and a
rebuild that would touch one stops instead.

Usage:
    python3 scripts/rebuild-corpus.py <database> [--only WK,EI,PC] [--dry-run]
"""

import argparse
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

INGESTED_SUBTESTS = ("WK", "EI", "PC", "GS", "SI", "AI")

# The subtests the factory makes rather than a source, and how many items each is asked for. Five
# hundred per subtest is what the observations asked for: the factory draws uniformly over its
# templates, and the store asks each question once, so a thin objective stays thin -- the fraction
# template produced 2 distinct items from 100 draws and 10 from 400. Five hundred leaves every
# objective with enough distinct items for a ten-question session.
GENERATED_SUBTESTS = (("AR", 500), ("MK", 500), ("MC", 500))
GENERATE_SEED = "20260923"

SUBTESTS = INGESTED_SUBTESTS + tuple(name for name, _ in GENERATED_SUBTESTS)

# The Shop Information sources, as `<source>:<id>:<title>=<path>`, where `<source>` is
# `archive` or `gutenberg`. Written down rather than guessed from the id, because an all-digit
# id is a Gutenberg ebook number *and* a plausible Internet Archive identifier -- the ambiguity
# that let ten Paragraph Comprehension items cite #2009 for the file that is #1228.
SHOP_MANUALS = [
    "archive:micro_IA41153156_0308:Tools and Their Uses (US Army, 1971)"
    "=sources/federal/tools-and-uses.txt",
    "archive:TM11-453:TM 11-453 Shop Work (US Army)=sources/federal/army-shop-work.txt",
    # `Modern Machine-Shop Practice` (Rose) is fetched and measured and **not used**: of the 21
    # descriptions its 4.9 MB of treatise prose yields, 10 became items and four of those are
    # visibly wrong -- `in connection`, `in rods`, `tension`, `Rubber joints` as the correct
    # answer to "Which tool is used to ...?" -- because the miner reads mid-paragraph clauses
    # (`... and are usually made from what is known as combination rubber`, `... they are fitted
    # to the ash-pits`). Its precision is lower than the seven manuals refused in round 18 and
    # the ten refused in round 20, and a source refused at higher precision cannot be kept at
    # this one. Round 23's measurement is in that round's report.
    "gutenberg:76925:Elementary Lathe Practice (T. J. Palmateer)"
    "=sources/gutenberg/palmateer-lathe-practice.txt",
    "gutenberg:57120:The Economy of Workshop Manipulation (John Richards)"
    "=sources/gutenberg/richards-workshop-manipulation.txt",
    "gutenberg:69061:Precision Locating and Dividing Methods (Anonymous)"
    "=sources/gutenberg/precision-locating-dividing.txt",
    "gutenberg:39791:Farm Mechanics: Machinery and Its Use to Save Hand Labor on the Farm"
    " (Herbert A. Shearer)=sources/gutenberg/shearer-farm-mechanics.txt",
    "gutenberg:28553:How it Works (Archibald Williams)"
    "=sources/gutenberg/williams-how-it-works.txt",
]

# The Auto Information sources, in the same form.
AUTO_MANUALS = [
    "archive:TM9-8000:TM 9-8000 Principles of Automotive Vehicles (US Army, 1985)"
    "=sources/federal/tm9-8000.txt",
    "archive:TM9-2700:TM 9-2700 Principles of Automotive Vehicles (US Army)"
    "=sources/federal/tm9-2700.txt",
    "gutenberg:56776:Practical Hand Book of Gas, Oil and Steam Engines (John B. Rathbun)"
    "=sources/gutenberg/rathbun-gas-oil-steam-engines.txt",
    "gutenberg:38415:Gas-Engines and Producer-Gas Plants (R. E. Mathot)"
    "=sources/gutenberg/mathot-gas-engines.txt",
    "gutenberg:27286:Gas and Oil Engines, Simply Explained (Walter C. Runciman)"
    "=sources/gutenberg/runciman-gas-oil-engines.txt",
    "gutenberg:59311:Gas and Petroleum Engines (H. de Graffigny)"
    "=sources/gutenberg/graffigny-gas-petroleum-engines.txt",
    "gutenberg:46094:The Romance of Modern Mechanism (Archibald Williams)"
    "=sources/gutenberg/williams-modern-mechanism.txt",
    "gutenberg:41160:The Romance of Modern Invention (Archibald Williams)"
    "=sources/gutenberg/williams-modern-invention.txt",
    "gutenberg:46232:The Boy's Book of New Inventions (Harry E. Maule)"
    "=sources/gutenberg/maule-boys-book-inventions.txt",
    "gutenberg:55482:Machines at Work (Mary Elting)=sources/gutenberg/elting-machines-at-work.txt",
]

GENERAL_SCIENCE = "75948=sources/gutenberg/book-of-wonders.txt"

# Vault records whose URL does not resolve to the bytes recorded for them, and why.
SUPERSEDED_SOURCES = [
    (
        "https://www.gutenberg.org/ebooks/2009",
        "the file is Project Gutenberg #1228; #2009 is a different file of the same book",
    ),
]


def neets_modules() -> list[str]:
    """Every NEETS module, in a fixed order.

    Sorted, and sorted here rather than by the caller: a module's seed is the run's seed plus
    the module's index, so the order decides which questions the builder's attempt budget
    reaches.
    """
    return sorted(
        f"sources/neets/{name}"
        for name in os.listdir(ROOT / "sources/neets")
        if name.startswith("NEETS_MOD_") and name.endswith(".txt")
    )


def gutenberg_works() -> list[str]:
    lines = [
        line.strip()
        for line in MANIFEST.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    ]
    if not lines:
        raise SystemExit(f"no works in {MANIFEST}")
    return lines


def repeated(flag: str, values: list[str]) -> list[str]:
    return [item for value in values for item in (flag, value)]


def steps(tools: list[str], db: str) -> list[tuple[str, list[str]]]:
    """The ingestion commands, one per subtest, in the order they must run."""
    word_knowledge = tools + [
        "ingest-wk",
        "--db", db,
        "--thesaurus", THESAURUS,
        "--dictionary", DICTIONARY,
        "--count", "2000",
        "--seed", SEED,
        "--reviewer", REVIEWER,
        "--out", str(EVIDENCE / "ingest-wk.json"),
    ]

    # The count is attempts, not items, and it is deliberately far larger than the bank. The
    # builder draws a definition per attempt, and at the 400 the earlier rounds used it built
    # 3,668 items that asked 409 distinct questions between them -- the same definition up to
    # seven times with its wrong answers shuffled. The store asks each question once, and the
    # attempts are what find the rest: 3,000 per module covers the whole of what these
    # glossaries state in a shape an item can be built from.
    electronics = tools + [
        "ingest-ei",
        "--db", db,
        "--dictionary", DICTIONARY,
        "--count", "3000",
        "--min-definition-words", "5",
        "--seed", SEED,
        "--reviewer", REVIEWER,
        "--out", str(EVIDENCE / "ingest-ei.json"),
    ] + repeated("--module", neets_modules())

    paragraphs = tools + [
        "ingest-pc",
        "--db", db,
        "--count", "400",
        "--seed", SEED,
        "--reviewer", REVIEWER,
        # Vocabulary-in-context items need a source-backed meaning, so the same
        # Webster's the vocabulary builder draws from is passed here. Without it the
        # run has no vocabulary items -- an honest gap, but a PC corpus that should
        # carry all four CAT kinds would be missing one.
        "--dictionary", DICTIONARY,
        "--out", str(EVIDENCE / "ingest-pc.json"),
    ] + repeated("--work", gutenberg_works())

    science = tools + [
        "ingest-facts",
        "--db", db,
        "--subtest", "GS",
        "--work", GENERAL_SCIENCE,
        "--count", "500",
        "--seed", SEED,
        "--reviewer", REVIEWER,
        "--out", str(EVIDENCE / "ingest-facts-gs.json"),
    ]

    # The counts are the ones the rounds that introduced them recorded: 200 attempts per shop
    # manual, 300 per automotive manual, which is more attempts than either manual has
    # descriptions.
    shop = tools + [
        "ingest-tools",
        "--db", db,
        "--subtest", "SI",
        "--ask", "tools",
        "--dictionary", DICTIONARY,
        "--count", "200",
        "--seed", SEED,
        "--reviewer", REVIEWER,
        "--out", str(EVIDENCE / "ingest-tools-si.json"),
    ] + repeated("--work", SHOP_MANUALS)

    auto = tools + [
        "ingest-tools",
        "--db", db,
        "--subtest", "AI",
        "--ask", "functions",
        "--dictionary", DICTIONARY,
        "--count", "300",
        "--seed", SEED,
        "--reviewer", REVIEWER,
        "--out", str(EVIDENCE / "ingest-tools-ai.json"),
    ] + repeated("--work", AUTO_MANUALS)

    return [
        ("WK", word_knowledge),
        ("EI", electronics),
        ("PC", paragraphs),
        ("GS", science),
        ("SI", shop),
        ("AI", auto),
    ]


def run(command: list[str]) -> None:
    print(f"\n$ {' '.join(command)}")
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8", errors="replace")
    if result.returncode != 0:
        raise SystemExit(f"failed with exit {result.returncode}: {' '.join(command)}")


def counts(connection: sqlite3.Connection, subtests: tuple[str, ...]) -> dict[str, int]:
    placeholders = ",".join("?" for _ in subtests)
    rows = connection.execute(
        "SELECT subtest, COUNT(*) FROM content_items WHERE state = 'active' "
        f"AND subtest IN ({placeholders}) GROUP BY subtest ORDER BY subtest",
        subtests,
    ).fetchall()
    return dict(rows)


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--only",
        default="",
        help="rebuild only these subtests, comma-separated: " + ", ".join(SUBTESTS),
    )
    arguments = parser.parse_args(argv)

    only = tuple(name.strip().upper() for name in arguments.only.split(",") if name.strip())
    unknown = [name for name in only if name not in SUBTESTS]
    if unknown:
        print(f"--only names unknown subtest(s): {', '.join(unknown)}")
        return 2
    selected = only or SUBTESTS

    database = Path(arguments.database)
    if not database.exists():
        print(f"no such database: {database}")
        return 2

    connection = sqlite3.connect(database)
    connection.execute("PRAGMA foreign_keys = ON")
    placeholders = ",".join("?" for _ in selected)
    attempted = connection.execute(
        "SELECT COUNT(*) FROM attempts a JOIN content_items i ON i.id = a.question_id "
        f"WHERE i.subtest IN ({placeholders})",
        selected,
    ).fetchone()[0]
    if attempted:
        print(
            f"{attempted} attempt(s) reference items in {', '.join(selected)}; the rebuild "
            "would delete a learner's own record"
        )
        return 1

    before = counts(connection, selected)
    print(f"database: {database}")
    print(f"subtests: {', '.join(selected)}")
    print(f"items before: {before}")

    if arguments.dry_run:
        print("dry run: nothing deleted, nothing ingested")
        return 0

    # 1. The selected items, and the vault records that are not evidence.
    connection.execute(
        f"DELETE FROM content_items WHERE subtest IN ({placeholders})", selected
    )
    for url, why in SUPERSEDED_SOURCES:
        removed = connection.execute(
            "DELETE FROM evidence_records WHERE url = ?", (url,)
        ).rowcount
        if removed:
            print(f"removed {removed} vault record(s) for {url}: {why}")
    connection.commit()
    print(f"items after delete: {counts(connection, selected)}")
    connection.close()

    # 2. Ingest, in the fixed order.
    tools = ["cargo", "run", "-q", "-p", "vector-tools", "--", "content"]
    for subtest, command in steps(tools, str(database)):
        if subtest in selected:
            run(command)

    # 2b. Generate the computable subtests through the application's own pipeline. One command
    # for all of them, so the per-subtest seeds stay what they are -- the seed is derived from
    # the subtest name there, and running them one at a time would not change it.
    wanted = [(name, count) for name, count in GENERATED_SUBTESTS if name in selected]
    if wanted:
        run(
            ["cargo", "run", "-q", "-p", "vector-application", "--example", "generate_corpus",
             "--", str(database)]
            + [f"{name}={count}" for name, count in wanted]
            + [GENERATE_SEED]
        )

    # 3. Read the result back out of the store rather than out of the reports.
    connection = sqlite3.connect(database)
    after = counts(connection, selected)
    connection.close()
    total = sum(after.values())
    print("\nitems after rebuild:")
    for subtest in selected:
        print(f"  {subtest:<4} {after.get(subtest, 0)}")
    print(f"  total {total}")
    if total == 0:
        print("the rebuild stored nothing")
        return 1
    if any(after.get(name, 0) < before.get(name, 0) for name in selected):
        print(
            "note: a subtest stores fewer items than before, which is what the question "
            "identity does when a bank was asking the same question repeatedly"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
