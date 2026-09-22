"""Test a sentence-initial definition rule against the federal manuals.

A definition this miner will accept has to begin a sentence: `<term> is a/an/the
<predicate>.` Definitions that merely contain "is a" are usually the tail of a
previous sentence, and accepting them produced terms like "since C", "Here" and
"wherever there".

Usage:
    python3 scripts/probes/definition-rule-probe.py <file.txt> [...] [--show N]
"""

import re
import sys

STOPWORDS = {
    "this", "that", "these", "those", "there", "here", "which", "it", "they",
    "what", "one", "each", "since", "wherever", "using", "its", "his", "her",
    "their", "our", "your", "my", "he", "she", "you", "we", "i", "such",
}

SENTENCE_INITIAL = re.compile(
    r"(?:^|(?<=[.!?] ))"
    r"(?P<term>(?:A |An |The )?[A-Za-z][A-Za-z0-9'’\-]*"
    r"(?: [A-Za-z][A-Za-z0-9'’\-]*){0,2}) "
    r"is (?:a|an|the) (?P<definition>[^.]{30,220})\.",
    re.MULTILINE,
)

WORD = re.compile(r"^[a-z][a-z'’\-]*(?: [a-z][a-z'’\-]*){0,2}$")


def unwrap(text: str) -> str:
    """Join hard-wrapped lines, which OCR text is full of."""
    paragraphs: list[str] = []
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


def mine(text: str, min_words: int = 8) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    seen: set[str] = set()
    for match in SENTENCE_INITIAL.finditer(unwrap(text)):
        term = re.sub(
            r"^(a|an|the) ", "", match.group("term").strip(), flags=re.IGNORECASE
        ).lower()
        definition = " ".join(match.group("definition").split())
        if term in STOPWORDS or not WORD.fullmatch(term):
            continue
        if len(definition.split()) < min_words:
            continue
        if re.search(r"\b[a-z] \b", definition):
            continue
        if term in seen:
            continue
        seen.add(term)
        out.append((term, definition))
    return out


def main(argv: list[str]) -> int:
    show = 8
    if "--show" in argv:
        index = argv.index("--show")
        show = int(argv[index + 1])
        argv = argv[:index] + argv[index + 2 :]
    if not argv:
        print(__doc__)
        return 2
    for path in argv:
        with open(path, encoding="utf-8", errors="replace") as handle:
            text = handle.read()
        found = mine(text)
        print(f"== {path}: {len(found)} definition(s)")
        for term, definition in found[:show]:
            print(f"   {term:<24} {definition[:110]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
