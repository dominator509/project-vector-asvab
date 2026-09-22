"""Measure a class-noun definition rule on the tool and automotive manuals.

The manuals define things in one dominant shape:

    A wrench is a basic tool that is used to exert a twisting force on bolt heads.
    The tang is the end of the file that fits into the handle.

The predicate names a class and then says what the thing does. Requiring that shape
is a sharper filter than requiring a content word to overlap the dictionary, which
rejected `mallet` and `shank` on wording alone and accepted `what` and `he`.

Usage:
    python3 scripts/probes/class-noun-probe.py <file.txt> [...] [--show N]
"""

import re
import sys

CLASSES = (
    "tool|tools|device|instrument|implement|part|parts|unit|system|assembly|"
    "machine|machinery|engine|motor|valve|pump|gauge|meter|wrench|hammer|file|"
    "drill|saw|chisel|punch|tap|die|gear|shaft|bearing|spring|circuit|"
    "component|mechanism|process|method|material|metal|substance|measure|"
    "force|energy|power|mixture|compound|element|operation|piece|length|"
    "surface|angle|point|edge|shape|form|kind|type|arrangement|movement|"
    "movements|size|amount|quantity|structure|framework|support|connection|"
    "joint|seal|gasket|filter|hose|line|pipe|tube|rod|lever|pulley|wheel|"
    "belt|chain|clutch|brake|transmission|axle|cylinder|piston|crankshaft|"
    "carburetor|battery|alternator|generator|starter|ignition|fuel|coolant|"
    "lubricant|oil|grease|air|water|gas|pressure|temperature|speed|torque"
)

CLASS_NOUN = re.compile(
    r"(?:^|(?<=[.!?] ))"
    r"(?P<term>(?:A |An |The )?[A-Za-z][A-Za-z0-9'’\-]*"
    r"(?: [A-Za-z][A-Za-z0-9'’\-]*){0,2}) "
    r"is (?:a|an|the) (?:[a-z\-]+ ){0,2}(?P<klass>" + CLASSES + r")"
    r"(?: (?:that|which|who|used|for|to|with|of|when|and|in|on|by|or)\b)"
    r"(?P<definition>[^.]{10,220})\.",
    re.MULTILINE | re.IGNORECASE,
)

FUNCTION_WORDS = {
    "in", "on", "at", "by", "then", "here", "there", "perhaps", "never",
    "using", "since", "wherever", "if", "and", "or", "but", "as", "of", "to",
    "this", "that", "these", "those", "which", "it", "they", "what", "one",
    "each", "he", "she", "you", "we", "i", "such", "his", "her", "their",
    "our", "your", "my", "its",
}


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


def mine(text: str, min_words: int = 8) -> list[tuple[str, str, str]]:
    out: list[tuple[str, str, str]] = []
    seen: set[str] = set()
    for match in CLASS_NOUN.finditer(unwrap(text)):
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
        out.append((term, match.group("klass").lower(), definition))
    return out


def main(argv: list[str]) -> int:
    show = 10
    if "--show" in argv:
        index = argv.index("--show")
        show = int(argv[index + 1])
        argv = argv[:index] + argv[index + 2 :]
    if not argv:
        print(__doc__)
        return 2
    for path in argv:
        with open(path, encoding="utf-8", errors="replace") as handle:
            found = mine(handle.read())
        print(f"== {path}: {len(found)} definition(s)")
        for term, klass, definition in found[:show]:
            print(f"   {term:<20} [{klass}] is a … {definition[:95]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
