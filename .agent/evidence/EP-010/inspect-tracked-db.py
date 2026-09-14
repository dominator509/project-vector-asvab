"""Inspect the tracked vector.db to assess what it exposes.

A SQLite database is runtime state, not source. If it is tracked in the
repository, it could carry a learner's real data into every clone.
"""

import sqlite3
from pathlib import Path

path = Path("vector.db")
print(f"file        : {path}")
print(f"bytes       : {path.stat().st_size}")
print()

conn = sqlite3.connect(path)
tables = [
    row[0]
    for row in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    )
]
print(f"tables      : {tables}")
print()

total_rows = 0
for table in tables:
    count = conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
    total_rows += count
    print(f"  {table:24} {count} row(s)")

print()
print(f"total data rows: {total_rows}")

# Show any learner-identifying content, which would be the actual risk.
for table in tables:
    if table in ("learner_profile", "attempts", "mastery", "evidence_records"):
        rows = conn.execute(f"SELECT * FROM {table} LIMIT 3").fetchall()
        if rows:
            print()
            print(f"sample from {table}: {rows}")

conn.close()

print()
if total_rows <= 2:
    print("Assessment: the database contains only default seed rows, so no")
    print("learner data is exposed. It is still runtime state and should not be")
    print("tracked, because the next commit would capture real data.")
else:
    print("Assessment: the database contains data beyond defaults and must be")
    print("treated as a potential data exposure.")
