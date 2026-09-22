"""Print a factual ingestion report, so a work's contribution is readable.

Usage:
    python3 scripts/probes/facts-report.py <ingest-facts report.json>
"""

import json
import sys


def main(path: str) -> int:
    with open(path, encoding="utf-8") as handle:
        report = json.load(handle)

    print(f"database       : {report['database']}")
    print(f"subtest        : {report['subtest']}")
    print(f"dictionary     : {report['dictionary']['entries']:,} entries")
    print(f"requested/work : {report['requested_per_work']}  seed={report['seed']}")
    print(f"elapsed        : {report['elapsed_ms']} ms")
    print()
    for work in report["works"]:
        print(
            f"  {work['ebook']:>6}  questions={work['questions']:<5} "
            f"built={work['built']:<5} activated={work['activated']:<5} "
            f"rejected={work['rejected']}  {work['title'][:50]}"
        )
        for reason in work["rejection_reasons"]:
            print(f"          refused: {reason}")
        if work["skipped"]:
            print(f"          skipped: {work['skipped']}")

    totals = report["totals"]
    print()
    print(
        "totals: {activated} activated, {already_present} already present, "
        "{built} built, {rejected} refused, {questions} questions".format(**totals)
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
