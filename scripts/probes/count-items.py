"""Count content items per subtest and state in the databases this round touches.

Used while assembling the corpus, so the counts reported are read from the store
rather than from the ingester's own summary.

Two counts are reported for each subtest, and the difference matters:

  stored     rows in `content_items`, whatever state they are in
  servable   rows a learner can actually be served

A row is servable when it is `active` *and* not withdrawn by a pack rollback: an item
delivered by a pack is served only while that pack is the active version for its name.
Counting only stored rows made a withdrawn corpus look present, which is the kind of
number that reads as success while the product serves nothing.
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
        print(f"   items stored: {total}")
        servable = connection.execute(
            """
            SELECT COUNT(*) FROM content_items
            WHERE state = 'active'
              AND (
                    NOT EXISTS (
                        SELECT 1 FROM content_pack_items m WHERE m.item_id = content_items.id
                    )
                    OR EXISTS (
                        SELECT 1 FROM content_pack_items m
                        JOIN content_packs p ON p.id = m.pack_id
                        WHERE m.item_id = content_items.id AND p.status = 'active'
                    )
                  )
            """
        ).fetchone()[0]
        print(f"   items servable: {servable}")
        if servable != total:
            print(
                "   (the difference is inactive items or items whose pack is not the "
                "active version)"
            )
        for subtest in sorted(
            {row[0] for row in connection.execute("SELECT subtest FROM content_items")}
        ):
            stored = connection.execute(
                "SELECT COUNT(*) FROM content_items WHERE subtest = ?", (subtest,)
            ).fetchone()[0]
            live = connection.execute(
                """
                SELECT COUNT(*) FROM content_items
                WHERE subtest = ? AND state = 'active'
                  AND (
                        NOT EXISTS (
                            SELECT 1 FROM content_pack_items m WHERE m.item_id = content_items.id
                        )
                        OR EXISTS (
                            SELECT 1 FROM content_pack_items m
                            JOIN content_packs p ON p.id = m.pack_id
                            WHERE m.item_id = content_items.id AND p.status = 'active'
                        )
                      )
                """,
                (subtest,),
            ).fetchone()[0]
            print(f"     {subtest:<3} stored {stored:<6} servable {live}")
        sources = connection.execute("SELECT COUNT(*) FROM evidence_records").fetchone()[0]
        citations = connection.execute(
            "SELECT COUNT(*) FROM content_item_sources"
        ).fetchone()[0]
        print(f"   evidence records: {sources}, citations: {citations}")
        if "content_packs" in tables:
            for name, version, status, items in connection.execute(
                "SELECT name, version, status, item_count FROM content_packs "
                "ORDER BY name, version"
            ):
                print(f"   pack {name} v{version}: {status}, {items} item(s)")
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
