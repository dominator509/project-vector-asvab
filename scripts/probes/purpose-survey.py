"""Measure purpose statements across the shop and automotive candidates.

The earlier probe required a purpose sentence to begin with its subject, which found
30 in the tool manual. The descriptions in these manuals usually follow a heading --
`SCREW AND TAP EXTRACTORS  Screw extractors are used to remove broken screws ...` --
so the subject sits mid-line and the sentence-initial rule misses most of them.

This probe takes the looser shape and reports what survives a few obvious filters, so
the size of a Shop Information bank can be judged from counts rather than from hope.

Usage:
    python3 scripts/probes/purpose-survey.py <file.txt> [...]
"""

import re
import sys

# Words that mean the "subject" is a figure reference or an aside, not a tool.
NOT_A_SUBJECT = {
    "figure",
    "fig",
    "chapter",
    "table",
    "page",
    "it",
    "they",
    "this",
    "that",
    "these",
    "those",
    "which",
    "one",
    "some",
    "many",
    "most",
    "they",
    "you",
    "we",
    "he",
    "she",
}

PURPOSE = re.compile(
    r"(?P<tool>[A-Za-z][A-Za-z'’\-]*(?: [A-Za-z][A-Za-z'’\-]*){0,3}?) "
    r"(?:are|is) (?:also )?(?:used|employed|designed|intended) (?:for|to|as) "
    r"(?P<purpose>[^.]{12,200})\.",
    re.IGNORECASE,
)

VERB = r"(?:used|serves|serving|designed|intended|employed|provides|makes|allows|cuts|holds|measures|removes|drives|lifts)"
SERVES = re.compile(
    r"(?P<tool>[A-Za-z][A-Za-z'’\-]*(?: [A-Za-z][A-Za-z'’\-]*){0,3}?) "
    r"(?:serves to|serves as|is designed to|is intended to|is used to|is used for) "
    r"(?P<purpose>[^.]{12,200})\.",
    re.IGNORECASE,
)


def clean(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def acceptable(tool: str, purpose: str) -> bool:
    tool = clean(tool)
    purpose = clean(purpose)
    words = tool.lower().split()
    if not words or len(words) > 3:
        return False
    if any(word in NOT_A_SUBJECT for word in words):
        return False
    # OCR noise: a stray digit or single letter in the subject.
    if re.search(r"\d", tool) or re.search(r"\b[a-z] \b", tool):
        return False
    if len(purpose.split()) < 5:
        return False
    if re.search(r"\b[a-z] \b", purpose):
        return False
    return True


def survey(path: str) -> tuple[int, int, list[tuple[str, str]]]:
    text = open(path, encoding="utf-8", errors="replace").read()
    body = re.sub(r"\s+", " ", text)
    found: list[tuple[str, str]] = []
    seen: set[str] = set()
    for pattern in (PURPOSE, SERVES):
        for match in pattern.finditer(body):
            tool = clean(match.group("tool"))
            purpose = clean(match.group("purpose"))
            if not acceptable(tool, purpose):
                continue
            key = tool.lower()
            if key in seen:
                continue
            seen.add(key)
            found.append((tool, purpose))
    raw = len(PURPOSE.findall(body)) + len(SERVES.findall(body))
    return raw, len(found), found


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__)
        return 2
    for path in argv:
        raw, kept, found = survey(path)
        print(f"== {path}")
        print(f"   candidate statements: {raw}   after filters: {kept}")
        for tool, purpose in found[:6]:
            print(f"     {tool:<26} -> {purpose[:90]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
