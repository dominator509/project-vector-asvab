"""Trace which workspace dependency pulls in the vulnerable crates.

cargo-audit reports two vulnerabilities. Whether each one actually affects the
shipped product depends on reachability, so this resolves the reverse dependency
chains from the lockfile.
"""

import re
from pathlib import Path

TARGETS = ["rsa", "sqlx"]

text = Path("Cargo.lock").read_text(encoding="utf-8")
blocks = text.split("[[package]]")

parsed = []
for block in blocks:
    name = re.search(r'name = "([^"]+)"', block)
    if not name:
        continue
    version = re.search(r'version = "([^"]+)"', block)
    deps = re.findall(r'^ "([^" ]+)', block, re.M)
    parsed.append((name.group(1), version.group(1) if version else "?", deps))

by_name = {name: (version, deps) for name, version, deps in parsed}

for target in TARGETS:
    print(f"=== who depends on {target}? ===")
    found = False
    for name, version, deps in parsed:
        if target in deps:
            print(f"  {name} {version}")
            found = True
    if not found:
        print("  (nobody in the lockfile lists it as a direct dependency)")
    print()

print("=== packages reachable upward from each target ===")
for target in TARGETS:
    print(f"{target}:")
    frontier = {target}
    seen = set()
    for _ in range(12):
        next_frontier = set()
        for name, version, deps in parsed:
            if name in seen:
                continue
            if any(d in frontier for d in deps) or name in frontier:
                if name != target:
                    print(f"  <- {name} {version}")
                seen.add(name)
                next_frontier.add(name)
        if not next_frontier:
            break
        frontier = next_frontier
    print()
