"""Count the description shapes the miner does not read yet.

The miner reads `X is used to <purpose>` and `X is used for <purpose>`. These manuals state
the same relation in other words -- `The function of the X is to Y`, `X serves to Y`,
`X is designed to Y` -- and each shape it cannot read is content already in the corpus that
never becomes an item. This counts them before any rule is written.

Usage:
    python3 scripts/probes/description-shapes.py <file.txt> [...]
"""

import re
import sys

SHAPES = {
    "is used to/for (read today)": re.compile(
        r"\b(?:is|are) (?:also )?used (?:to|for|as) ", re.IGNORECASE
    ),
    "the function of X is to": re.compile(
        r"\bfunction of (?:the )?[A-Za-z][^.]{0,40}? is to ", re.IGNORECASE
    ),
    "X serves to": re.compile(r"\bserves to ", re.IGNORECASE),
    "X serves as": re.compile(r"\bserves as ", re.IGNORECASE),
    "X is designed to": re.compile(r"\b(?:is|are) designed to ", re.IGNORECASE),
    "X is intended to": re.compile(r"\b(?:is|are) intended to ", re.IGNORECASE),
    "the purpose of X is to": re.compile(
        r"\bpurpose of (?:the )?[A-Za-z][^.]{0,40}? is to ", re.IGNORECASE
    ),
    "X is used when": re.compile(r"\b(?:is|are) used when ", re.IGNORECASE),
}


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not argv:
        print(__doc__)
        return 2
    for path in argv:
        text = open(path, encoding="utf-8", errors="replace").read()
        body = re.sub(r"\s+", " ", text)
        print(f"== {path}")
        for name, pattern in SHAPES.items():
            print(f"   {name:<28} {len(pattern.findall(body))}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
