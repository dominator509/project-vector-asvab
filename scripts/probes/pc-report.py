"""Print a Paragraph Comprehension ingestion report, including refusal reasons.

The JSON report is the evidence record; this renders it for reading, so the
reasons a work contributed nothing or had items refused are visible without
opening the file.
"""

import json
import sys


def main(path: str) -> int:
    with open(path, encoding="utf-8") as handle:
        report = json.load(handle)

    print(f"database        : {report['database']}")
    print(f"requested/work  : {report['requested_per_work']}  seed={report['seed']}")
    print(f"elapsed         : {report['elapsed_ms']} ms")
    print()
    for work in sorted(report["works"], key=lambda w: -w["activated"]):
        line = (
            f"  {work['ebook']:>6}  activated={work['activated']:<4} "
            f"built={work['built']:<4} paragraphs={work['paragraphs']:<5} "
            f"rejected={work['rejected']}  {work['title'][:52]}"
        )
        print(line)
        for reason in work["rejection_reasons"]:
            print(f"          refused: {reason}")
        if work["skipped"]:
            print(f"          skipped: {work['skipped']}")

    totals = report["totals"]
    print()
    print(
        "totals: {activated} activated, {already_present} already present, "
        "{built} built, {rejected} refused, {paragraphs} paragraphs".format(**totals)
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
