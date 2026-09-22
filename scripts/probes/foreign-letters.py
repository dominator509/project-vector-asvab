"""Find learner-facing text that carries letters from another alphabet.

The 1940s and 1970s scans this corpus comes from substitute Cyrillic letters for Latin
ones, and the substitution is sometimes not a homoglyph at all: an archived copy of
TM 9-8000 offers `дeaг-ana rotor-type pumps`, `Oй filters`, `Tһe bulkhead receptacle` and
the purpose `generate fluid pressure юг OIL SENSOR TM the lubrication system`. The miner
normalises the letters that are shape-identical and refuses the rest, so this probe is the
check that the refusal holds.

Two things are not damage and must not be reported as such. A Paragraph Comprehension
passage quotes Project Gutenberg prose verbatim, and that prose carries the accents its
author used -- `coöperation`, `Linnæus`, `François`, `Segrè`, `Chaillô` -- which are Latin
letters and are kept. U+FFFD is never acceptable anywhere: it means a byte could not be
decoded, and a learner cannot read it.

Usage:
    python3 scripts/probes/foreign-letters.py <database.db>
"""

import json
import os
import sqlite3
import sys
import unicodedata

# Latin-1 letters, the œ/Œ ligature, and the quotation marks and dashes this prose uses.
PROSE_EXCEPTIONS = set(
    "ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÑÒÓÔÕÖØŒÙÚÛÜÝß"
    "àáâãäåæçèéêëìíîïñòóôõöøœùúûüýÿ"
    "‘’“”–—…°"
)


def is_damage(character: str) -> bool:
    """Whether a letter is something the corpus must not teach."""
    if character.isascii():
        return False
    if character == "\ufffd":
        return True
    if character in PROSE_EXCEPTIONS:
        return False
    return character.isalpha()


def main(argv: list[str]) -> int:
    # The console is not UTF-8 on Windows, and this probe prints the characters it found.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if len(argv) != 1:
        print(__doc__)
        return 2
    path = argv[0]
    if not os.path.exists(path):
        print(f"no such database: {path}")
        return 2
    connection = sqlite3.connect(path)
    damaged = 0
    quoted = set()
    total = 0
    for (item_id, subtest, stem, options, explanation) in connection.execute(
        "select id, subtest, stem, options_json, explanation from content_items "
        "where state = 'active' order by subtest, id"
    ):
        total += 1
        fields = {
            "stem": stem or "",
            "options": " ".join(json.loads(options or "[]")),
            "explanation": explanation or "",
        }
        for field, text in fields.items():
            foreign = {c for c in text if is_damage(c)}
            if foreign:
                damaged += 1
                named = ", ".join(
                    f"U+{ord(c):04X} {unicodedata.name(c, 'unnamed')}"
                    for c in sorted(foreign)
                )
                print(f"DAMAGE {item_id} [{subtest}] {field}: {named}")
                print(f"    {text[:120]}")
            quoted |= {c for c in text if c.isalpha() and not c.isascii()} - foreign
    for character in sorted(quoted):
        print(
            f"quoted prose: U+{ord(character):04X} {unicodedata.name(character, 'unnamed')}"
        )
    print(f"\n{total} active item(s) read, {damaged} field(s) with unreadable text")
    return 1 if damaged else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
