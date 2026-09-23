"""Write the corpus's curriculum graph and calibration metadata for a content pack.

A pack declares what it teaches (a graph, not a set of labels) and what it claims about how
hard each objective is (with the evidence the claim rests on). Both are authored data, and
this writes the corpus's own: one node per objective its items use, prerequisites following the
order a learner meets the material in, and a calibration entry per objective.

**What the calibration honestly is.** No learner has answered these items yet, so every entry
carries `responses: 0` and says so in its basis. The number it does carry is derived from the
items' own difficulty scale -- a logit-like value the generator or the source assigned -- which
is a declaration about the material, not a measurement of learners. The field that separates
the two is `responses`, and it is zero on purpose: a pack that claimed measured difficulty it
had never measured would be the same class of lie as a citation nobody can re-check.

Usage:
    python3 scripts/probes/build-curriculum.py <db> [--out <directory>]
"""

import json
import math
import os
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / ".agent/evidence/EP-007/content-corpus"

# What each objective is, in a learner's words. Objective ids are stable and human-readable
# already (`OBJ-SI-TOOLS-01`), so a missing entry falls back to the id rather than being
# invented here.
TITLES = {
    "OBJ-SI-TOOLS-01": "Shop Information: which tool performs a purpose",
    "OBJ-AI-FUNCTION-01": "Auto Information: what a component is for",
    "OBJ-EI-TERMINOLOGY-01": "Electronics Information: the term a definition names",
    "OBJ-WK-SYNONYM-01": "Word Knowledge: the word that means the same",
    "OBJ-PC-DETAIL-01": "Paragraph Comprehension: the detail the passage states",
    "OBJ-GS-EXPLAIN-01": "General Science: the explanation a scientific work gives",
}

# What has to come first. Only relations the corpus can defend: shop knowledge before
# automotive, and the arithmetic objectives in the order the factory introduces them.
PREREQUISITES = {
    "OBJ-AI-FUNCTION-01": ["OBJ-SI-TOOLS-01"],
    "OBJ-AR-PERCENT-01": ["OBJ-AR-RATE-01"],
    "OBJ-AR-PROPORTION-01": ["OBJ-AR-PERCENT-01"],
    "OBJ-AR-AVERAGE-01": ["OBJ-AR-PROPORTION-01"],
    "OBJ-AR-INTEREST-01": ["OBJ-AR-PERCENT-01"],
    "OBJ-AR-GEOMETRY-01": ["OBJ-AR-PROPORTION-01"],
    "OBJ-AR-GEOMETRY-02": ["OBJ-AR-GEOMETRY-01"],
    "OBJ-AR-RATE-02": ["OBJ-AR-RATE-01"],
    "OBJ-MK-ALGEBRA-02": ["OBJ-MK-ALGEBRA-01"],
}


def expected_correct(difficulty: float) -> float:
    """Turn the items' difficulty scale into the share of learners expected to be right.

    The scale is centred on zero and logit-like, so its logistic is a share between 0 and 1.
    This is a conversion, not a measurement: nothing here has been observed.
    """
    return 1.0 / (1.0 + math.exp(-difficulty))


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not argv:
        print(__doc__)
        return 2
    database = argv[0]
    out = DEFAULT_OUT
    if "--out" in argv:
        out = Path(argv[argv.index("--out") + 1])
    if not os.path.exists(database):
        print(f"no such database: {database}")
        return 2

    connection = sqlite3.connect(database)
    rows = connection.execute(
        "SELECT subtest, objective_id, COUNT(*), AVG(difficulty) FROM content_items "
        "WHERE state = 'active' GROUP BY subtest, objective_id ORDER BY subtest, objective_id"
    ).fetchall()
    connection.close()
    if not rows:
        print("the store holds no active items, so there is no curriculum to write")
        return 1

    nodes = []
    calibration = []
    for subtest, objective_id, count, mean_difficulty in rows:
        prerequisites = [
            prerequisite
            for prerequisite in PREREQUISITES.get(objective_id, [])
            # A prerequisite the corpus does not teach is not a prerequisite it can declare:
            # the pack verifier refuses a graph that names an objective it does not carry.
            if any(row[1] == prerequisite for row in rows)
        ]
        nodes.append(
            {
                "objective_id": objective_id,
                "subtest": subtest,
                "title": TITLES.get(objective_id, objective_id),
                "prerequisites": prerequisites,
            }
        )
        calibration.append(
            {
                "objective_id": objective_id,
                "expected_correct": round(expected_correct(mean_difficulty or 0.0), 4),
                "responses": 0,
                "basis": (
                    f"declared from the difficulty scale of {count} item(s); "
                    "no learner responses recorded"
                ),
            }
        )

    out.mkdir(parents=True, exist_ok=True)
    curriculum_path = out / "pack-curriculum.json"
    calibration_path = out / "pack-calibration.json"
    curriculum_path.write_text(json.dumps(nodes, indent=2) + "\n", encoding="utf-8")
    calibration_path.write_text(json.dumps(calibration, indent=2) + "\n", encoding="utf-8")

    print(f"{len(nodes)} objective(s) over {len(set(row[0] for row in rows))} subtest(s)")
    for node in nodes:
        prerequisite = ",".join(node["prerequisites"]) or "-"
        print(f"  {node['subtest']:<4} {node['objective_id']:<26} after {prerequisite}")
    print(f"wrote {curriculum_path}")
    print(f"wrote {calibration_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
