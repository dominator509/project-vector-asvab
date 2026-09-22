"""Check that every option of every item is free of a known misreading.

The corpus's dictionary check exists because a scanner's error once became a wrong
answer: `inuide micrometer` for `inside micrometer`. It has been revised twice since --
once because passing a whole sentence to a per-word test made it a no-op, and once
because the per-word test refused ordinary English (`produced`, `today`) when applied to
prose. This reads the stored corpus rather than the ingester's report, so what is checked
is what a learner would be served.

Usage:
    python3 scripts/probes/check-options.py <db> [word ...]
"""

import json
import os
import sqlite3
import sys

# Words the scanners in this corpus are known to have produced. An option containing one
# is a wrong answer that looks like a word, which is worse than a missing item.
SCAN_DAMAGE = ["inuide", "eleetron", "eurrent", "eonduct", "lilter"]

# Not scan damage. Moby's associations are not all synonyms, so a rare word can reach a
# Word Knowledge distractor (`becharm` for `bewitch`). That is a different defect with a
# different fix -- a word-frequency source -- and it is reported separately so the two are
# never confused for one another.
RARE_WORD_DISTRACTORS = ["becharm", "unendowed"]


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    path = argv[0]
    if not os.path.exists(path):
        print(f"missing: {path}")
        return 1

    connection = sqlite3.connect(path)
    try:
        total = 0
        damage: list[tuple[str, str, str]] = []
        rare: list[tuple[str, str, str]] = []
        for item_id, subtest, options_json in connection.execute(
            "SELECT id, subtest, options_json FROM content_items"
        ):
            total += 1
            for option in json.loads(options_json):
                lowered = option.lower()
                if any(word in lowered for word in SCAN_DAMAGE):
                    damage.append((item_id, subtest, option))
                elif any(word in lowered for word in RARE_WORD_DISTRACTORS):
                    rare.append((item_id, subtest, option))
        print(f"checked {total} item(s)")
        for item_id, subtest, option in damage[:20]:
            print(f"  SCAN DAMAGE {subtest} {item_id}: {option}")
        for item_id, subtest, option in rare[:5]:
            print(f"  rare distractor (known gap) {subtest} {item_id}: {option}")
        print(f"options containing scan damage      : {len(damage)}")
        print(f"options containing a rare distractor: {len(rare)}")
        return 0 if not damage else 1
    finally:
        connection.close()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
