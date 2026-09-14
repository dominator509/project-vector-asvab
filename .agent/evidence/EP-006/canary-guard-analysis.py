"""Is the empty-canary guard redundant with the length guard?

MUT-C7 removed the empty check and all tests still passed, which means no test
distinguishes the two guards. This determines whether the empty check is
genuinely unreachable-by-behaviour (harmless defence in depth) or whether a test
is missing.
"""

values = ["", "   ", "  a ", "abc", "abcd"]
for v in values:
    trimmed_empty = v.strip() == ""
    length_ok = len(v) >= 4
    print(
        f"{v!r:8} len={len(v)} trimmed_empty={trimmed_empty} "
        f"passes_length_guard={length_ok}"
    )
