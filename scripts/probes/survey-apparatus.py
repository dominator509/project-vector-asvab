"""Survey Gutenberg paragraphs for apparatus that is not prose.

Written while deciding what `parse_gutenberg` should filter. It answers two
questions with counts rather than impressions: how often a paragraph is a caption
list (heavy `(illus.)` use) and how often it carries the wiki-style `=bold=`
markup these plain-text editions use for emphasis.
"""

import re
import sys

START = "*** START OF THE PROJECT GUTENBERG"
END = "*** END OF THE PROJECT GUTENBERG"


def body(text: str) -> str:
    start = text.find(START)
    end = text.rfind(END)
    if start == -1 or end == -1 or end <= start:
        return text
    newline = text.find("\n", start)
    return text[newline + 1 : end]


def paragraphs(text: str) -> list[str]:
    out: list[str] = []
    current: list[str] = []
    for line in body(text).splitlines():
        stripped = line.strip()
        if not stripped:
            if current:
                out.append(" ".join(" ".join(current).split()))
                current = []
            continue
        current.append(stripped)
    if current:
        out.append(" ".join(" ".join(current).split()))
    return out


def main(paths: list[str]) -> int:
    for path in paths:
        with open(path, encoding="utf-8", errors="replace") as handle:
            text = handle.read()
        rows = paragraphs(text)
        illus = [p for p in rows if p.count("(illus") >= 3]
        marked = [p for p in rows if p.count("=") >= 2]
        both = [p for p in rows if p.count("(illus") >= 3 or p.count("=") >= 2]
        print(f"== {path}")
        print(f"   paragraphs            : {len(rows)}")
        print(f"   >=3 '(illus' markers  : {len(illus)}")
        print(f"   >=2 '=' markup signs  : {len(marked)}")
        print(f"   filtered by either    : {len(both)}")
        for sample in both[:3]:
            print(f"     e.g. {sample[:120]!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
