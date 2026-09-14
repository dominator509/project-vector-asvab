"""Independent live-fire driver for EP-003.

Uses the standard-library sqlite3 module rather than the project's own Rust
code, so the readback is an independent check of the on-disk result rather
than the same code path that wrote it.
"""

import sqlite3
import sys

db = sys.argv[1]
action = sys.argv[2]

conn = sqlite3.connect(db)
conn.execute("PRAGMA foreign_keys=ON")

if action == "seed":
    conn.execute(
        "INSERT OR IGNORE INTO learner_profile (id,name,target_score) VALUES ('L1','Ada',72)"
    )
    conn.executemany(
        "INSERT INTO attempts (id,learner_id,subtest,question_id,correct,latency_ms,created_at)"
        " VALUES (?,?,?,?,?,?,?)",
        [
            ("a1", "L1", "AR", "q1", 1, 1000, "2026-09-10T00:00:00Z"),
            ("a2", "L1", "AR", "q2", 0, 2000, "2026-09-10T00:01:00Z"),
        ],
    )
    conn.commit()
    print("attempts:", conn.execute("SELECT COUNT(*) FROM attempts").fetchone()[0])

elif action == "delete":
    conn.execute("DELETE FROM attempts")
    conn.commit()
    print("attempts after delete:", conn.execute("SELECT COUNT(*) FROM attempts").fetchone()[0])

elif action == "read":
    print("attempts:", conn.execute("SELECT COUNT(*) FROM attempts").fetchone()[0])
    print("row a1:", conn.execute("SELECT subtest,correct,latency_ms FROM attempts WHERE id='a1'").fetchone())
    print("tables:", sorted(r[0] for r in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")))
    print("migrations:", conn.execute("SELECT version FROM schema_migrations ORDER BY version").fetchall())

elif action == "fk":
    # An orphan attempt must be rejected by the real schema.
    try:
        conn.execute(
            "INSERT INTO attempts (id,learner_id,subtest,question_id,correct,latency_ms,created_at)"
            " VALUES ('orphan','nobody','AR','q',1,1,'2026-09-10T00:00:00Z')"
        )
        conn.commit()
        print("FK: NOT ENFORCED (bad)")
    except sqlite3.IntegrityError as e:
        print("FK enforced:", e)

conn.close()
