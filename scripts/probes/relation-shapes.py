"""Count every relation shape the reader could learn, across the Shop and Auto sources.

Each shape a manual uses and the reader cannot see is content already in the corpus that never
becomes an item. This counts them by hand-written verb list rather than by one regex per shape,
so a shape nobody thought of still shows up in the totals.

Usage:
    python3 scripts/probes/relation-shapes.py <file.txt> [...]
"""

import collections
import re
import sys
from pathlib import Path

# The shapes the reader already reads, then the ones it does not.
SHAPES = [
    # Read today: a bare verb follows the phrase, which is what the question frame needs.
    "is used to",
    "is used for",
    "are used to",
    "are used for",
    "is designed to",
    "are designed to",
    "is intended to",
    "are intended to",
    "serves to",
    "serve to",
    # Not read: either a different verb, or a complement that is not a verb phrase.
    "is used as",
    "are used as",
    "serves as",
    "serve as",
    "acts as",
    "act as",
    "functions as",
    "function as",
    "is employed to",
    "is employed for",
    "are employed to",
    "are employed for",
    "is utilized to",
    "is utilized for",
    "is adapted to",
    "is adapted for",
    "is applied to",
    "is applied for",
    "is fitted with",
    "are fitted with",
    "is arranged to",
    "is arranged so",
    "is provided with",
    "are provided with",
    "is made to",
    "is made so",
    "is so designed",
    "is so arranged",
]


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not argv:
        print(__doc__)
        return 2
    totals: collections.Counter[str] = collections.Counter()
    per_file: dict[str, collections.Counter[str]] = {}
    for path in argv:
        text = Path(path).read_text(encoding="utf-8", errors="replace")
        body = re.sub(r"\s+", " ", text)
        counts = collections.Counter()
        for shape in SHAPES:
            counts[shape] = len(re.findall(r"\b" + re.escape(shape) + r"\b", body, re.I))
        per_file[path] = counts
        totals.update(counts)
        print(f"== {Path(path).name}")
        for shape, count in counts.most_common():
            if count:
                print(f"   {count:>5}  {shape}")
    print("\n== across every source, unread shapes only")
    for shape, count in totals.most_common():
        if count and shape not in {
            "is used to",
            "is used for",
            "are used to",
            "are used for",
            "is designed to",
            "are designed to",
            "is intended to",
            "are intended to",
            "serves to",
            "serve to",
        }:
            print(f"   {count:>5}  {shape}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
