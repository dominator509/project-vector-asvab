"""List the distinct first words of the purposes these manuals state.

The purpose miner's verb test needs to know which words begin a purpose. Rather than guess,
this prints every distinct first word the corpus produces after the other filters have run,
so the verb list can be written from the data.

Usage:
    python3 scripts/probes/purpose-verbs.py <file.txt> [...]
"""

import re
import sys
from collections import Counter

PURPOSE = re.compile(
    r"(?:are|is) (?:also )?(?:used|employed|designed|intended) (?:for|to|as) ([^.]{12,200})\.",
    re.IGNORECASE,
)


def main(argv: list[str]) -> int:
    words: Counter[str] = Counter()
    for path in argv:
        text = open(path, encoding="utf-8", errors="replace").read()
        body = re.sub(r"\s+", " ", text)
        for purpose in PURPOSE.findall(body):
            first = purpose.split()[0]
            first = re.sub(r"^[^A-Za-z]+", "", first).lower()
            if first:
                words[first] += 1
    print(f"{len(words)} distinct first words\n")
    for word, count in words.most_common():
        print(f"  {count:>5}  {word}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
