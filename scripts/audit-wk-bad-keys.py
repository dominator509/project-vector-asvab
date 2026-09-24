#!/usr/bin/env python3
"""Audit every Word Knowledge item in a built pack for bad answer keys.

This is the audit behind #9's "fix bad keys" item. It is a real, re-runnable
check rather than a one-line claim: it reads the built pack the application
serves, not the generator's in-memory items, and it refuses to pass on a pack
where an item's answer key does not point at a real option.

Usage:
    python3 scripts/audit-wk-bad-keys.py [path/to/pack.json]

Default pack is the EP-007 content corpus pack the release gate consumes.

Checks, per WK item:
  * correct_index is in range for the options that exist;
  * the item has four distinct options (no duplicate option text);
  * the explanation is present and non-empty;
  * the item names an objective_id and at least one cited source.

Exit code 0 when every item is clean, 1 otherwise. The counts are printed so a
reviewer can compare them against the ticket rather than trust them.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

DEFAULT_PACK = Path(".agent/evidence/EP-007/content-corpus/packs/pack-v6.json")


def audit(pack_path: Path) -> int:
    pack = json.loads(pack_path.read_text(encoding="utf-8"))
    items = [item for item in pack.get("items", []) if item.get("subtest") == "WK"]

    bad_index: list[str] = []
    duplicate_options: list[str] = []
    missing_explanation: list[str] = []
    missing_objective: list[str] = []
    missing_source: list[str] = []

    for item in items:
        item_id = str(item.get("id", "<no-id>"))
        options = item.get("options") or []
        correct_index = item.get("correct_index")

        if not isinstance(correct_index, int) or not (0 <= correct_index < len(options)):
            bad_index.append(item_id)

        # Four options, four distinct strings. Two identical options make the
        # item unanswerable no matter where the key points.
        if len(options) != 4 or len(set(options)) != len(options):
            duplicate_options.append(item_id)

        if not str(item.get("explanation", "")).strip():
            missing_explanation.append(item_id)

        if not str(item.get("objective_id", "")).strip():
            missing_objective.append(item_id)

        if not item.get("sources"):
            missing_source.append(item_id)

    total = len(items)
    clean = total - len(
        set(bad_index)
        | set(duplicate_options)
        | set(missing_explanation)
        | set(missing_objective)
        | set(missing_source)
    )

    print(f"pack        : {pack_path}")
    print(f"WK items    : {total}")
    print(f"clean items : {clean}/{total}")
    print(f"correct_index out of range : {len(bad_index)}")
    print(f"fewer than 4 distinct opts : {len(duplicate_options)}")
    print(f"missing/blank explanation  : {len(missing_explanation)}")
    print(f"missing objective_id       : {len(missing_objective)}")
    print(f"missing cited source       : {len(missing_source)}")

    for label, ids in (
        ("bad_index", bad_index),
        ("duplicate_options", duplicate_options),
        ("missing_explanation", missing_explanation),
        ("missing_objective", missing_objective),
        ("missing_source", missing_source),
    ):
        for item_id in ids[:10]:
            print(f"  DEFECT {label}: {item_id}")

    return 0 if clean == total and total > 0 else 1


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_PACK
    if not path.exists():
        print(f"pack not found: {path}", file=sys.stderr)
        return 2
    return audit(path)


if __name__ == "__main__":
    raise SystemExit(main())
