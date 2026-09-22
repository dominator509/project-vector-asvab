"""Prototype the definition miner against the real federal texts.

Written before the Rust implementation so the rules are chosen from observed text
rather than guessed, and kept afterwards as the probe that shows what the rules
yield on a new source.

Usage:
    python3 scripts/probes/mine-definitions.py <file.txt> [--show N] [--min-words N]
"""

import re
import sys

# Front matter, headings and apparatus that look like definitions but are not.
REJECT_TERMS = {
    "distribution statement",
    "title",
    "note",
    "caution",
    "warning",
    "applicable training",
    "value",
    "descriptors",
    "identifiers",
    "abstract",
    "report no",
    "pub date",
    "available from",
    "edrs price",
    "institution",
    "spons agency",
    "grading by mail",
}

# A definitional sentence: the term, then "is a/an/the", then the predicate.
DEFINITION = re.compile(
    r"(?P<term>(?:The |A |An )?[A-Za-z][A-Za-z0-9'’\- ]{2,34}?) "
    r"is (?:a|an|the) (?P<definition>[^.]{25,240})\.",
    re.IGNORECASE,
)


def unwrap(text: str) -> str:
    """Join hard-wrapped lines into paragraphs."""
    paragraphs = []
    current: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            if current:
                paragraphs.append(" ".join(current))
                current = []
            continue
        current.append(stripped)
    if current:
        paragraphs.append(" ".join(current))
    return "\n".join(paragraphs)


def clean(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def acceptable(term: str, definition: str, min_words: int) -> bool:
    term = clean(term).lower()
    if term in REJECT_TERMS:
        return False
    words = term.split()
    if not 1 <= len(words) <= 3:
        return False
    # Every word of the term must be a plausible English word: no digits, no
    # stray single letters from OCR, no all-capitals shouting.
    for word in words:
        if not re.fullmatch(r"[a-z][a-z'’\-]*", word):
            return False
    if len(definition.split()) < min_words:
        return False
    # OCR damage: a stray single letter is almost always a lost character.
    if re.search(r"\b[a-z] \b", definition):
        return False
    return True


def mine(text: str, min_words: int) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    seen: set[str] = set()
    for match in DEFINITION.finditer(unwrap(text)):
        raw_term = clean(match.group("term"))
        term = re.sub(r"^(?:the|a|an) ", "", raw_term, flags=re.IGNORECASE)
        definition = clean(match.group("definition"))
        if not acceptable(term, definition, min_words):
            continue
        key = term.lower()
        if key in seen:
            continue
        seen.add(key)
        out.append((term, definition))
    return out


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    path = argv[0]
    show = 10
    min_words = 8
    if "--show" in argv:
        show = int(argv[argv.index("--show") + 1])
    if "--min-words" in argv:
        min_words = int(argv[argv.index("--min-words") + 1])

    with open(path, encoding="utf-8", errors="replace") as handle:
        text = handle.read()
    found = mine(text, min_words)
    print(f"{path}: {len(found)} definition(s) at min_words={min_words}")
    for term, definition in found[:show]:
        print(f"  {term!r}: {definition[:120]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
