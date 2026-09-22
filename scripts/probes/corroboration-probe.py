"""Test whether Webster's 1913 can corroborate a mined definition.

A mined pair is only usable if the sentence really is about the term. Syntax alone
cannot tell: `rifling is a very difficult operation, and great care must be
exercised` is grammatical, sentence-initial, and says nothing about rifling.

The test here is agreement with an independent authority: the mined definition must
share a content word with the dictionary's own entry for the term. `friction is
resistance that one surface offers to its movement` shares `resistance` with the
entry for friction; the rifling sentence shares nothing with the entry for rifling.

Usage:
    python3 scripts/probes/corroboration-probe.py <webster.txt> <source.txt> [...]
"""

import re
import sys

STOPWORDS = {
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "at", "by", "for",
    "with", "from", "as", "is", "are", "was", "were", "be", "been", "being",
    "that", "this", "these", "those", "it", "its", "his", "her", "their",
    "which", "who", "whom", "whose", "when", "where", "while", "not", "no",
    "but", "if", "then", "than", "so", "such", "one", "two", "each", "any",
    "all", "some", "more", "most", "other", "into", "over", "under", "up",
    "down", "out", "off", "about", "upon", "may", "can", "will", "would",
    "shall", "should", "must", "has", "have", "had", "do", "does", "did",
}

FUNCTION_WORDS = STOPWORDS | {
    "in", "on", "at", "by", "then", "here", "there", "perhaps", "never",
    "using", "since", "wherever", "if", "and", "or", "but", "as", "of", "to",
}

SENTENCE_INITIAL = re.compile(
    r"(?:^|(?<=[.!?] ))"
    r"(?P<term>(?:A |An |The )?[A-Za-z][A-Za-z0-9'’\-]*"
    r"(?: [A-Za-z][A-Za-z0-9'’\-]*){0,2}) "
    r"is (?:a|an|the) (?P<definition>[^.]{30,220})\.",
    re.MULTILINE,
)


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


def mine(text: str, min_words: int = 8) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    seen: set[str] = set()
    for match in SENTENCE_INITIAL.finditer(unwrap(text)):
        term = re.sub(
            r"^(a|an|the) ", "", match.group("term").strip(), flags=re.IGNORECASE
        ).lower()
        definition = " ".join(match.group("definition").split())
        words = term.split()
        if not words or any(word in FUNCTION_WORDS for word in words):
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


def webster_index(path: str) -> dict[str, str]:
    """Headword -> its entry text, mirroring `parse_webster` in the Rust crate.

    A headword line is one that starts with a letter and is not a numbered sense;
    the headword itself is the line's leading run of letters, because Webster's
    marks pronunciation inside the word (`Fric"tion`, `Ab"a*ca"`).
    """
    index: dict[str, list[str]] = {}
    headword: str | None = None
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            stripped = line.rstrip()
            if stripped and not stripped[0].isspace() and stripped[0].isalpha():
                letters = re.match(r"[A-Za-z]+", stripped)
                if letters and len(letters.group(0)) >= 2:
                    headword = letters.group(0).lower()
                    index.setdefault(headword, [])
                    continue
            if headword is not None:
                index[headword].append(stripped.lower())
    return {word: " ".join(lines) for word, lines in index.items()}


def content_words(text: str, minimum: int = 5) -> set[str]:
    return {
        word
        for word in re.findall(r"[a-z]{2,}", text.lower())
        if len(word) >= minimum and word not in STOPWORDS
    }


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    dictionary = webster_index(argv[0])
    print(f"dictionary headwords: {len(dictionary):,}")
    total = kept = 0
    for path in argv[1:]:
        with open(path, encoding="utf-8", errors="replace") as handle:
            found = mine(handle.read())
        corroborated = []
        for term, definition in found:
            entry = dictionary.get(term)
            if not entry:
                continue
            if content_words(definition) & content_words(entry):
                corroborated.append((term, definition))
        total += len(found)
        kept += len(corroborated)
        print(
            f"== {path}: mined {len(found)}, corroborated {len(corroborated)} "
            f"({100 * len(corroborated) // max(1, len(found))}%)"
        )
        for term, definition in corroborated[:5]:
            print(f"   {term:<22} {definition[:100]}")
        rejected = [t for t, _ in found if t not in {c[0] for c in corroborated}]
        print(f"   rejected: {', '.join(rejected[:8])}")
    print(f"\ntotal mined {total}, corroborated {kept}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
