"""Measure two structures that suit a factual subtest better than definitions do.

1. Purpose sentences: `A wrench is used to exert a twisting force on bolt heads.`
   This is the shape the tool manuals actually use, and it maps directly onto a
   Shop Information question ("which tool is used to ...?").

2. Question and answer pairs: some public-domain science books are written as
   questions with answers, which is the shape of a General Science item.

Usage:
    python3 scripts/probes/purpose-probe.py <file.txt> [...] [--show N]
"""

import re
import sys

FUNCTION_WORDS = {
    "in", "on", "at", "by", "then", "here", "there", "perhaps", "never",
    "using", "since", "wherever", "if", "and", "or", "but", "as", "of", "to",
    "this", "that", "these", "those", "which", "it", "they", "what", "one",
    "each", "he", "she", "you", "we", "i", "such", "his", "her", "their",
    "our", "your", "my", "its", "the", "a", "an", "is", "are",
}

PURPOSE = re.compile(
    r"(?:^|(?<=[.!?] ))(?:A |An |The )?"
    r"(?P<tool>[A-Za-z][A-Za-z0-9'’\-]*(?: [A-Za-z][A-Za-z0-9'’\-]*){0,2}) "
    r"(?:is|are) used (?:to|for|as) (?P<purpose>[^.]{15,200})\.",
    re.MULTILINE,
)

QUESTION = re.compile(r"(?m)^\s*(?P<question>(?:Why|What|How|When|Where|Which)\b[^?\n]{10,150}\?)")


def unwrap(text: str) -> str:
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


def purposes(text: str) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    seen: set[str] = set()
    for match in PURPOSE.finditer(unwrap(text)):
        tool = match.group("tool").strip().lower()
        purpose = " ".join(match.group("purpose").split())
        if any(word in FUNCTION_WORDS for word in tool.split()):
            continue
        if len(purpose.split()) < 5 or re.search(r"\b[a-z] \b", purpose):
            continue
        if tool in seen:
            continue
        seen.add(tool)
        out.append((tool, purpose))
    return out


def main(argv: list[str]) -> int:
    show = 6
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
        found = purposes(text)
        questions = QUESTION.findall(unwrap(text))
        print(f"== {path}")
        print(f"   purpose sentences: {len(found)}   questions: {len(questions)}")
        for tool, purpose in found[:show]:
            print(f"     {tool:<22} -> {purpose[:90]}")
        for question in questions[:show]:
            print(f"     Q: {question[:100]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
