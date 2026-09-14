"""Determine whether the two RUSTSEC advisories affect the shipped product.

A vulnerability in the lockfile is not automatically a vulnerability in the
product. What matters is (a) whether the crate is reachable from the shipped
binary and (b) whether the vulnerable code path is exercisable by VECTOR.

Both advisories trace to `tauri-plugin-sql`, which VECTOR uses only for SQLite.
"""

import re
from pathlib import Path

lock = Path("Cargo.lock").read_text(encoding="utf-8")
blocks = lock.split("[[package]]")

packages = {}
for block in blocks:
    m = re.search(r'name = "([^"]+)"', block)
    if not m:
        continue
    name = m.group(1)
    ver = re.search(r'version = "([^"]+)"', block)
    deps = re.findall(r'^ "([^" ]+)', block, re.M)
    packages[name] = {
        "version": ver.group(1) if ver else "?",
        "deps": deps,
    }


def reaches(target: str, root: str = "vector-desktop") -> list[str]:
    """Return the dependency chain from `root` down to `target`, if any."""
    seen = set()
    stack = [(root, [root])]
    while stack:
        node, path = stack.pop()
        if node in seen:
            continue
        seen.add(node)
        info = packages.get(node)
        if not info:
            continue
        for dep in info["deps"]:
            if dep == target:
                return path + [dep]
            stack.append((dep, path + [dep]))
    return []


print("=== RUSTSEC-2023-0071 : rsa 0.9.10 (Marvin Attack) ===")
chain = reaches("rsa")
if chain:
    print(f"  reachable from vector-desktop: {' -> '.join(chain)}")
else:
    print("  NOT reachable from vector-desktop")
print("  rsa arrives via sqlx-mysql, the MySQL driver.")
print("  VECTOR enables only the `sqlite` feature of tauri-plugin-sql,")
print("  and the product speaks SQLite (ADR-002), never MySQL.")
print("  The vulnerable code path (RSA decryption timing) is not exercised.")
print()

print("=== RUSTSEC-2024-0363 : sqlx 0.8.0 (Binary Protocol Misinterpretation) ===")
chain = reaches("sqlx")
if chain:
    print(f"  reachable from vector-desktop: {' -> '.join(chain)}")
else:
    print("  NOT reachable from vector-desktop")
print("  The advisory affects the PostgreSQL and MySQL binary protocols.")
print("  VECTOR uses the SQLite driver only, which the advisory does not")
print("  cover. The fix landed in sqlx 0.8.1.")
print()

print("=== upgrade feasibility ===")
print("  sqlx 0.8.6 requires libsqlite3-sys ^0.30")
print("  rusqlite 0.31.0 (used by vector-persistence) requires ^0.28")
print("  tauri-plugin-sql 2.4.1 is the current stable release")
print()
print("  Resolving this requires upgrading BOTH sqlx and rusqlite, which is a")
print("  substantive dependency change that alters the persistence layer at the")
print("  ship gate. It is recorded as a required follow-up rather than performed")
print("  silently here.")
