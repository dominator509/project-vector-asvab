"""Remove Paragraph Comprehension items so the corpus can be rebuilt from source.

Used once, when the passage builder was found to be rewriting text: items ingested
before that fix may carry a passage the work does not contain, and a corpus cannot
be repaired by adding corrected items beside the broken ones -- the broken ones stay
servable.

Used again when a recorded source URL was found to point at a different edition of
the same book (`darwin-origin.txt` is Project Gutenberg #1228, not #2009). Those
items had to go as well: re-ingesting the same text produces the same content hash,
so the ingester would report them as already present and leave the wrong citation in
place.

The guard that matters: an item a learner has attempted is not deleted. Attempts
reference items by id and are the learner's own record, so an item with an attempt
against it is left in place and reported. Deleting content is a maintenance action,
not something this script does quietly.

Usage:
    python3 scripts/probes/rebuild-pc-corpus.py <db> [--subtest PC]
    python3 scripts/probes/rebuild-pc-corpus.py <db> --citation-url <substring>
"""

import sqlite3
import sys

SUBTEST = "PC"


def selection(argv: list[str]) -> tuple[str, str, tuple]:
    """Return (description, where-clause, parameters) for what to remove."""
    if len(argv) >= 3 and argv[1] == "--citation-url":
        substring = argv[2]
        return (
            f"items citing a source whose URL contains {substring!r}",
            "EXISTS (SELECT 1 FROM content_item_sources s "
            "  JOIN evidence_records e ON e.id = s.source_id "
            "  WHERE s.item_id = content_items.id AND e.url LIKE ?)",
            (f"%{substring}%",),
        )
    subtest = SUBTEST
    if len(argv) >= 3 and argv[1] == "--subtest":
        subtest = argv[2]
    return (
        f"items of subtest {subtest}",
        "subtest = ?",
        (subtest,),
    )


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    path = argv[0]
    description, where, parameters = selection(argv)

    connection = sqlite3.connect(path)
    connection.execute("PRAGMA foreign_keys = ON")
    try:
        total = connection.execute(
            f"SELECT COUNT(*) FROM content_items WHERE {where}", parameters
        ).fetchone()[0]
        removable = connection.execute(
            f"SELECT COUNT(*) FROM content_items WHERE {where} AND NOT EXISTS ("
            "  SELECT 1 FROM attempts a WHERE a.question_id = content_items.id"
            ")",
            parameters,
        ).fetchone()[0]

        print(f"database        : {path}")
        print(f"selection       : {description}")
        print(f"matching items  : {total}")
        print(f"to remove       : {removable}")

        if (kept := total - removable) > 0:
            print(
                f"refusing to remove {kept} item(s) a learner has attempted; "
                "quarantine them instead"
            )

        connection.execute(
            f"DELETE FROM content_items WHERE {where} AND NOT EXISTS ("
            "  SELECT 1 FROM attempts a WHERE a.question_id = content_items.id"
            ")",
            parameters,
        )
        connection.commit()

        left = connection.execute(
            f"SELECT COUNT(*) FROM content_items WHERE {where}", parameters
        ).fetchone()[0]
        orphans = connection.execute(
            "SELECT COUNT(*) FROM content_item_sources s WHERE NOT EXISTS ("
            "  SELECT 1 FROM content_items ci WHERE ci.id = s.item_id"
            ")"
        ).fetchone()[0]
        print(f"remaining       : {left}")
        print(f"orphan citations: {orphans} (must be 0; the schema cascades)")
        return 0
    finally:
        connection.close()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
