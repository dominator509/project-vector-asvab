"""Independently read back an ingested corpus from the database.

Deliberately separate from the ingester: it opens the file the ingester wrote and
asks the schema what is there, so a bug that reported success without storing
anything cannot hide behind its own report.
"""

import json
import sqlite3
import sys

db = sys.argv[1]
c = sqlite3.connect(db)

print("=== corpus ===")
for subtest, state, count in c.execute(
    "select subtest, state, count(*) from content_items group by subtest, state"
):
    print(f"  {subtest:<4} {state:<10} {count}")
for table in ("evidence_records", "content_item_sources", "content_item_reviews"):
    print(f"  {table:<22}", c.execute(f"select count(*) from {table}").fetchone()[0])

print()
print("=== provenance invariants over the whole corpus ===")
checks = [
    (
        "active items not citing exactly 2 sources",
        """select count(*) from content_items i where i.state='active'
           and (select count(*) from content_item_sources s where s.item_id=i.id) <> 2""",
    ),
    (
        "items whose generator and verifier hashes agree",
        "select count(*) from content_items where generator_hash = verifier_hash",
    ),
    (
        "active items with no reviewer",
        "select count(*) from content_items where state='active' and trim(reviewer)=''",
    ),
    (
        "active items whose proof is not source_backed",
        "select count(*) from content_items where state='active' and proof_kind <> 'source_backed'",
    ),
    (
        "duplicate content hashes",
        "select count(*) from (select content_hash from content_items group by content_hash having count(*) > 1)",
    ),
    (
        "items whose correct_index misses its options",
        """select count(*) from content_items
           where correct_index >= json_array_length(options_json)""",
    ),
]
for label, sql in checks:
    print(f"  {label:<48} {c.execute(sql).fetchone()[0]}")

print()
print("=== three real items ===")
for stem, options, idx, proof, explanation in c.execute(
    "select stem, options_json, correct_index, proof_json, explanation "
    "from content_items where state='active' order by id limit 3"
):
    print(f"\n  {stem}")
    for position, option in enumerate(json.loads(options)):
        marker = "   <-- correct" if position == idx else ""
        print(f"      {'ABCD'[position]}. {option}{marker}")
    parsed = json.loads(proof)["SourceBacked"]
    print(f"      cited source : {parsed['source_id']}")
    print(f"      rubric       : {parsed['rubric'][:140]}...")
    print(f"      explanation  : {explanation[:120]}...")

print()
print("=== review trail for one item ===")
item_id = c.execute("select id from content_items limit 1").fetchone()[0]
for from_state, to_state, actor, rationale in c.execute(
    "select from_state, to_state, actor, rationale from content_item_reviews "
    "where item_id=? order by created_at, rowid",
    (item_id,),
):
    print(f"  {from_state:<20} -> {to_state:<20} by {actor:<16} {rationale[:50]}")
