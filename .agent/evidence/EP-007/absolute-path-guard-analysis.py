"""Why does MUT-I9 (absolute archive entries) survive?

If the absolute-path guard is removed and the traversal tests still pass, either
(a) the guard is redundant because another check catches absolute paths, or
(b) the test suite never exercises the absolute case. This distinguishes them.
"""

def normalize(path: str) -> str:
    unified = path.replace("\\", "/")
    out = []
    for segment in unified.split("/"):
        if segment in ("", "."):
            continue
        out.append(segment)
    return "/".join(out)


ROOT = "content"

cases = ["/etc/passwd", "\\windows\\system32", "C:\\Windows\\system32", "content/lesson.md"]

for entry in cases:
    norm = normalize(entry)
    has_dotdot = any(s == ".." for s in norm.split("/"))
    starts_sep = entry.startswith("/") or entry.startswith("\\")
    drive_qualified = len(entry) >= 2 and entry[1] == ":"

    within = norm == ROOT or norm.startswith(ROOT + "/")

    print(
        f"{entry!r:28} normalized={norm!r:24} "
        f"has_dotdot={has_dotdot} starts_sep={starts_sep} "
        f"drive={drive_qualified} within_root={within}"
    )
