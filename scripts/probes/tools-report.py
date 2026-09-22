"""Print a Shop Information ingestion report.

Usage:
    python3 scripts/probes/tools-report.py <ingest-tools report.json>
"""

import json
import sys


def main(path: str) -> int:
    with open(path, encoding="utf-8") as handle:
        report = json.load(handle)

    print(f"database        : {report['database']}")
    print(f"subtest         : {report['subtest']}")
    print(f"dictionary      : {report['dictionary']['entries']:,} entries")
    print(f"requested/manual: {report['requested_per_manual']}  seed={report['seed']}")
    print(f"elapsed         : {report['elapsed_ms']} ms")
    print()
    for manual in report["manuals"]:
        print(
            f"  {manual['archive_id']:<26} statements={manual['statements']:<4} "
            f"built={manual['built']:<4} activated={manual['activated']:<4} "
            f"rejected={manual['rejected']}  {manual['title'][:46]}"
        )
        for reason in manual["rejection_reasons"]:
            print(f"          refused: {reason}")
        if manual["skipped"]:
            print(f"          skipped: {manual['skipped']}")

    totals = report["totals"]
    print()
    print(
        "totals: {activated} activated, {already_present} already present, "
        "{built} built, {rejected} refused, {statements} descriptions".format(**totals)
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
