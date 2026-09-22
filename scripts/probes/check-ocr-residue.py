"""Confirm specific OCR misspellings are absent from a corpus."""

import sqlite3
import sys

db = sys.argv[1]
c = sqlite3.connect(db)

# Tokens the scan rendered wrongly, each found in real items before the term
# check was added.
SUSPECT = ["LILTER", "eleetron", "eurrent", "eonduct", "resistanee", "reeiproeal"]

print("=== OCR misspellings in served text ===")
for token in SUSPECT:
    rows = c.execute(
        "select count(*) from content_items "
        "where stem like ? or options_json like ? or explanation like ?",
        (f"%{token}%", f"%{token}%", f"%{token}%"),
    ).fetchone()[0]
    print(f"  {token:<14} {rows}")

total = c.execute(
    "select count(*) from content_items where state='active'"
).fetchone()[0]
print(f"\n  active items checked: {total}")
