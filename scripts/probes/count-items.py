"""Count content items per subtest and state in the databases this round touches.

Used while assembling the corpus, so the counts reported are read from the store
rather than from the ingester's own summary.
"""

import os
import sqlite3
import sys


def report(path: str) -> None:
    print(f"== {path}")
    if not os.path.exists(path):
        print("   missing")
        return
    connection = sqlite3.connect(path)
    try:
        tables = {
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            )
        }
        if "content_items" not in tables:
            print(f"   no content_items; tables={sorted(tables)}")
            return
        total = connection.execute("SELECT COUNT(*) FROM content_items").fetchone()[0]
        print(f"   items: {total}")
        for subtest, state, count in connection.execute(
            "SELECT subtest, state, COUNT(*) FROM content_items "
            "GROUP BY subtest, state ORDER BY subtest, state"
        ):
            print(f"     {subtest:<3} {state:<18} {count}")
        sources = connection.execute("SELECT COUNT(*) FROM evidence_records").fetchone()[0]
        citations = connection.execute(
            "SELECT COUNT(*) FROM content_item_sources"
        ).fetchone()[0]
        print(f"   evidence records: {sources}, citations: {citations}")
        without_passage = connection.execute(
            "SELECT COUNT(*) FROM content_items WHERE subtest = 'PC' AND passage IS NULL"
        ).fetchone()[0]
        print(f"   PC items with no passage: {without_passage}")
    finally:
        connection.close()


if __name__ == "__main__":
    targets = sys.argv[1:]
    if not targets:
        targets = [
            os.path.join(os.path.dirname(__file__), "..", "..", "vector.db"),
            os.path.join(
                os.environ.get("APPDATA", ""), "com.vector.app", "vector.db"
            ),
        ]
    for target in targets:
        report(os.path.abspath(target))
