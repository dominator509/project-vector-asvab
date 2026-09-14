"""Summarize the cargo-audit findings for the EP-010 release gate."""

import json
import sys
from collections import Counter

path = sys.argv[1]
data = json.load(open(path, encoding="utf-8"))

print("=== VULNERABILITIES ===")
for item in data["vulnerabilities"]["list"]:
    advisory = item["advisory"]
    package = item["package"]
    print(f"{advisory['id']}  {advisory['package']} {package['version']}")
    print(f"  title    : {advisory['title']}")
    print(f"  severity : {advisory.get('severity')}")
    print(f"  patched  : {advisory.get('patched_versions')}")
    print(f"  url      : {advisory.get('url')}")
    # The dependency path shows whether the crate is actually reachable.
    chain = item.get("path") or []
    print(f"  path     : {' -> '.join(str(p) for p in chain[:8])}")
    print()

print("=== WARNING KINDS ===")
kinds = Counter()
warnings = data.get("warnings", {})
# cargo-audit emits warnings either as a dict keyed by advisory id, or as a
# list when there are no warnings at all.
warning_items = warnings.values() if isinstance(warnings, dict) else warnings
for warning in warning_items:
    advisory = warning["advisory"]
    kind = advisory.get("informational") or "unmaintained"
    kinds[kind] += 1
for kind, count in kinds.most_common():
    print(f"  {kind}: {count}")

print()
print("=== UNMAINTAINED / UNSOUND CRATES ===")
seen = set()
for warning in warning_items:
    advisory = warning["advisory"]
    key = (advisory["package"], warning["package"]["version"])
    if key in seen:
        continue
    seen.add(key)
    print(f"  {advisory['package']} {warning['package']['version']}: {advisory['title']}")
