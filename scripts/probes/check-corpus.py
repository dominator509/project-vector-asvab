"""Independently read back an ingested corpus from the database.

Deliberately separate from the ingester: it opens the file the ingester wrote and
asks the schema what is there, so a bug that reported success without storing
anything cannot hide behind its own report.
"""

import json
import re
import sqlite3
import sys
import unicodedata

db = sys.argv[1]
c = sqlite3.connect(db)

# Letters a Paragraph Comprehension passage legitimately quotes: Latin-1 and the œ ligature.
PROSE_EXCEPTIONS = set(
    "ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÑÒÓÔÕÖØŒÙÚÛÜÝß"
    "àáâãäåæçèéêëìíîïñòóôõöøœùúûüýÿ"
    "‘’“”–—…°"
)


def scan_damage(subtest: str, stem: str, options_json: str) -> int:
    """Whether a learner-facing field carries damage the scan left in it.

    Written independently of the Rust rules that refuse this text at ingestion, so the check
    is a readback rather than a restatement: if the two disagree, one of them is wrong.

    The capitals rule applies to the subtests whose sentences a program framed from a manual's
    prose. In Electronics Information the capitals *are* the source's typography -- a NEETS
    glossary prints its terms as `THERMOCOUPLE` -- and in the quoted subtests the capitals
    belong to the work being quoted.
    """
    text = (stem or "") + " " + " ".join(json.loads(options_json or "[]"))
    if "\ufffd" in text:
        return 1
    for character in text:
        if (
            character.isalpha()
            and not character.isascii()
            and character not in PROSE_EXCEPTIONS
        ):
            return 1
    if subtest not in ("SI", "AI"):
        return 0
    # A word the scan broke: a letter, a hyphen, a space, a lower-case letter.
    if re.search(r"[A-Za-z]- [a-z]", text):
        return 1
    # A run of capitals, which is a label from a drawing rather than a sentence.
    if re.search(r"\b[A-Z]{3,}\s+[A-Z]{3,}\b", text):
        return 1
    # A bracket opened and never closed, which is text the scan dropped.
    if text.count("(") != text.count(")") or text.count("[") != text.count("]"):
        return 1
    return 0


c.create_function("scan_damage", 3, scan_damage)

print("=== corpus ===")
for subtest, state, count in c.execute(
    "select subtest, state, count(*) from content_items group by subtest, state"
):
    print(f"  {subtest:<4} {state:<10} {count}")
for table in ("evidence_records", "content_item_sources", "content_item_reviews"):
    print(f"  {table:<22}", c.execute(f"select count(*) from {table}").fetchone()[0])

print()
print("=== provenance invariants over the whole corpus ===")

# How many sources each subtest's items must cite. A Word Knowledge item rests on
# a thesaurus line corroborated by a dictionary, so it cites two; an Electronics
# Information item rests on a single NEETS module glossary, and a Paragraph
# Comprehension item on a single public-domain work whose passage it quotes, so
# both cite one. Asserting a flat "2" here reported 4,000 EI items as broken when
# the corpus was correct -- the probe was wrong, not the data.
EXPECTED_SOURCES = {"WK": 2, "EI": 1, "PC": 1, "GS": 1, "SI": 1, "AI": 1}

bad_citations = 0
for subtest, expected in EXPECTED_SOURCES.items():
    offenders = c.execute(
        """select count(*) from content_items i where i.subtest=? and i.state='active'
           and (select count(*) from content_item_sources s where s.item_id=i.id) <> ?""",
        (subtest, expected),
    ).fetchone()[0]
    print(f"  {subtest} active items not citing exactly {expected} source(s): {offenders}")
    bad_citations += offenders

checks = [
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
    (
        "active items with no objective",
        "select count(*) from content_items where state='active' and trim(objective_id)=''",
    ),
    # A Paragraph Comprehension item is a passage plus a question about it, so one
    # without a passage is unanswerable. The schema refuses it; this asks the store
    # directly rather than trusting the schema to have been applied.
    (
        "PC items with no passage",
        "select count(*) from content_items where subtest='PC' and (passage is null or trim(passage)='')",
    ),
    (
        "PC items whose passage is under 40 words",
        """select count(*) from content_items where subtest='PC'
           and length(trim(passage)) - length(replace(trim(passage), ' ', '')) + 1 < 40""",
    ),
    (
        "PC items whose correct option is not in the passage",
        """select count(*) from content_items where subtest='PC'
           and instr(lower(passage),
                     lower(json_extract(options_json, '$[' || correct_index || ']'))) = 0""",
    ),
    # A factual item's stem is the source's question, and it has no passage: the
    # question is self-contained, and a passage would claim it needs one to read.
    (
        "GS items whose stem is not a question",
        """select count(*) from content_items where subtest='GS'
           and substr(trim(stem), -1) <> '?'""",
    ),
    (
        "GS items carrying a passage they do not need",
        "select count(*) from content_items where subtest='GS' and passage is not null",
    ),
    # Every option of a factual item is an answer the source gives, so all four are
    # substantive statements rather than filler.
    (
        "GS items with an option shorter than 5 words",
        """select count(*) from content_items where subtest='GS' and exists (
             select 1 from json_each(content_items.options_json)
             where length(trim(value)) - length(replace(trim(value), ' ', '')) + 1 < 5
           )""",
    ),
    # A shop item asks what a tool is for, and offers tools rather than sentences: every
    # option should be a short name, and none should be a sentence about a tool.
    (
        "SI items whose stem is not a tool-purpose question",
        """select count(*) from content_items where subtest='SI'
           and stem not like 'Which tool is used%'""",
    ),
    (
        "SI items with an option longer than 4 words",
        """select count(*) from content_items where subtest='SI' and exists (
             select 1 from json_each(content_items.options_json)
             where length(trim(value)) - length(replace(trim(value), ' ', '')) + 1 > 4
           )""",
    ),
    # An Auto Information item is the other way round: the stem names a component and the
    # options are functions, so every option is written as the infinitive the manuals use
    # (`to prevent leakage ...`) and no option is a bare noun phrase.
    (
        "AI items whose stem is not a component-function question",
        """select count(*) from content_items where subtest='AI'
           and (stem not like 'What %' or stem not like '% used for?')""",
    ),
    (
        "AI items with an option that is not an infinitive",
        """select count(*) from content_items where subtest='AI' and exists (
             select 1 from json_each(content_items.options_json)
             where value not like 'to %'
           )""",
    ),
    (
        "AI items with an option shorter than 4 words",
        """select count(*) from content_items where subtest='AI' and exists (
             select 1 from json_each(content_items.options_json)
             where length(trim(value)) - length(replace(trim(value), ' ', '')) + 1 < 4
           )""",
    ),
    # Two editions of one manual state the same thing in different words, and a builder that
    # draws different wrong answers for the same question asks it again. The store identifies a
    # question by the stem, the correct answer and the passage, so a word with two synonyms is
    # two questions (`irenic` asked against `pacific` and against `peaceful`) and a Paragraph
    # Comprehension stem that is generic is made a question by its passage.
    (
        "questions asked twice within one subtest",
        """select count(*) from (
             select subtest,
                    lower(trim(stem)),
                    lower(trim(coalesce(json_extract(options_json,
                          '$[' || correct_index || ']'), ''))),
                    lower(trim(coalesce(passage, '')))
             from content_items where state='active'
             group by 1, 2, 3, 4 having count(*) > 1
           )""",
    ),
    # Damage these scans leave in a learner-facing sentence: a broken word (`be- tween`), a
    # drawing's label read in (`EVAPORATOR CORE CAPILLARY TUBE`), a bracket never closed, a
    # letter from another alphabet, and a letter that could not be decoded at all.
    #
    # The capitals rule is scoped to the items a *program* framed from a manual's sentence. A
    # NEETS glossary prints its terms in capitals (`THERMOCOUPLE`), so an unscoped rule reported
    # 3,670 perfectly good Electronics Information options as damaged -- the probe was wrong,
    # not the corpus.
    (
        "items carrying scan damage in stem or options",
        """select count(*) from content_items
           where scan_damage(subtest, stem, options_json) = 1""",
    ),
]
for label, sql in checks:
    print(f"  {label:<48} {c.execute(sql).fetchone()[0]}")

print()
print("=== citations per subtest ===")
for subtest, cite_count, items in c.execute(
    """select i.subtest, count(s.item_id), count(distinct i.id)
       from content_items i left join content_item_sources s on s.item_id = i.id
       group by i.subtest order by i.subtest"""
):
    print(f"  {subtest:<4} {items} item(s), {cite_count} citation(s)")

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

