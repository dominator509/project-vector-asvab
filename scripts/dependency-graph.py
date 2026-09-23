"""Derive the dependency blocker graph and refuse a blanket block.

DOD-031: *a failed prerequisite blocks only tests with explicit dependency edges;
independent tests continue to completion.* The clause has two halves and both are
machine-checked here.

**The graph.** Every registered test is a node; so is every gate the sweep recorded, and
every prerequisite or capability a blocked test names. An edge means "this blocked test
is waiting on that node" -- an explicit declaration, not a stage-wide assumption. The
edges are derived from what each blocked row already records in its reason, through the
patterns below, so a test cannot be blocked without saying what blocks it.

**The checks**, any of which makes this exit non-zero:

1. every blocked row has at least one incoming edge, so nothing is blocked by cascade;
2. no passing or not-applicable row has an incoming edge from an unsatisfied node -- a
   graph that blocked an independent test would be the blanket blocking the clause
   forbids;
3. the graph is acyclic;
4. blocked gates in `gate-results.jsonl` name a prerequisite that actually failed, and a
   gate that passed is not recorded as blocked by anything;
5. at least one test ran to completion inside a stage that also holds a blocked sibling,
   which is the continuation the clause asks for, observed rather than asserted.

It writes `.agent/verification/state/DEPENDENCY_BLOCKER_GRAPH.json` and a readable
summary beside it, and prints the per-node blast radius: how many tests each unsatisfied
node actually blocks. A prerequisite that blocks everything is the failure this clause
exists to catch.

Usage:
    python3 scripts/dependency-graph.py [--json <path>] [--md <path>]
"""

import argparse
import csv
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / ".agent/verification/state"
REPORTS = ROOT / ".agent/verification/reports"
REGISTRY = ROOT / ".agent/verification/MASTER_TEST_REGISTRY.csv"
ACCOUNTING = REPORTS / "COMPLETE_TEST_ACCOUNTING.csv"
GATE_RESULTS = STATE / "gate-results.jsonl"

BLOCKED = ("BLOCKED_PREREQUISITE", "BLOCKED_CAPABILITY")

# What a blocked row's reason names, in the order the patterns are tried. Each is a
# declaration about *what kind of thing* the test waits for: a capability no ordering can
# supply, a prerequisite this project has not produced yet, or a failed gate.
PATTERNS = [
    ("human participants", "capability:human-participant"),
    ("human team", "capability:human-participant"),
    ("human tester", "capability:human-participant"),
    ("human reviewer", "capability:human-participant"),
    ("human using Narrator", "capability:human-participant"),
    ("human-reviewed", "capability:human-participant"),
    ("offensive and defensive human", "capability:human-participant"),
    ("external laboratory", "capability:external-laboratory"),
    ("code-signing certificate", "capability:signing-certificate"),
    ("deployment target", "capability:deployment-target"),
    ("deployed service", "capability:deployment-target"),
    ("deployed environment", "capability:deployment-target"),
    ("deployed production-like", "capability:deployment-target"),
    ("deployed target", "capability:deployment-target"),
    ("CI runner", "capability:ci-runner"),
    ("runtime instrumentation", "capability:runtime-instrumentation"),
    ("coverage-guided fuzzer", "capability:fuzzing-toolchain"),
    ("Ghidra", "capability:binary-analysis-toolchain"),
    ("disassembler", "capability:binary-analysis-toolchain"),
    ("OWASP ZAP", "capability:dast-tooling"),
    ("prior released version", "prerequisite:prior-released-version"),
    ("second released version", "prerequisite:prior-released-version"),
    ("prior released version to upgrade from", "prerequisite:prior-released-version"),
    ("extended wall-clock budget", "prerequisite:full-duration-trial"),
    ("no local caches", "capability:cache-free-environment"),
    ("second clean environment", "capability:independent-build-environment"),
]


def classify(reason: str) -> str | None:
    """The node a blocked row is waiting on, or None when its reason names nothing."""
    lowered = reason.lower()
    for needle, node in PATTERNS:
        if needle.lower() in lowered:
            return node
    return None


def load_csv(path: Path) -> list[dict]:
    with path.open(encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", default=str(STATE / "DEPENDENCY_BLOCKER_GRAPH.json"))
    parser.add_argument("--md", default=str(STATE / "DEPENDENCY_BLOCKER_GRAPH.md"))
    arguments = parser.parse_args(argv)

    registry = {row["test_id"]: row for row in load_csv(REGISTRY)}
    accounting = load_csv(ACCOUNTING)
    if len(registry) != len(accounting):
        print(
            f"the registry holds {len(registry)} id(s) and the accounting {len(accounting)}; "
            "accounting every registered test is the point of the accounting"
        )
        return 2

    gates: dict[str, dict] = {}
    if GATE_RESULTS.exists():
        for line in GATE_RESULTS.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                row = json.loads(line)
                gates[row["gate"]] = row

    nodes: dict[str, dict] = {}
    edges: list[dict] = []
    violations: list[str] = []

    for test_id, row in sorted(registry.items()):
        nodes[f"test:{test_id}"] = {
            "kind": "test",
            "stage": row["default_stage"],
            "title": row["title"],
            "must_account": row["must_account"],
        }
    for name, row in sorted(gates.items()):
        nodes[f"gate:{name}"] = {
            "kind": "gate",
            "exit": row.get("exit"),
            "status": row.get("status", "RAN" if row.get("exit") == 0 else "FAILED"),
        }

    unsatisfied: set[str] = set()
    for row in accounting:
        test_id = row["test_id"]
        status = row["status"]
        node = f"test:{test_id}"
        nodes[node]["status"] = status
        if status not in BLOCKED:
            continue
        reason = row.get("blocker_or_na_reason", "")
        target = classify(reason)
        if target is None:
            violations.append(
                f"{test_id} is {status} and its reason names no prerequisite or capability: "
                f"{reason[:80]!r}"
            )
            continue
        unsatisfied.add(target)
        nodes.setdefault(target, {"kind": target.split(":", 1)[0]})
        edges.append(
            {
                "from": target,
                "to": node,
                "why": reason[:160],
                "recorded_status": status,
            }
        )

    # Gate-level edges, declared by `blocked_by` in scripts/verify.sh and recorded in the
    # sweep's own results.
    for name, row in sorted(gates.items()):
        prerequisite = row.get("blocked_by")
        if not prerequisite:
            continue
        node = f"gate:{name}"
        if f"gate:{prerequisite}" not in nodes:
            violations.append(f"gate {name} is blocked by {prerequisite}, which did not run")
            continue
        unsatisfied.add(f"gate:{prerequisite}")
        edges.append(
            {
                "from": f"gate:{prerequisite}",
                "to": node,
                "why": "the sweep recorded this gate as blocked by that one",
                "recorded_status": "BLOCKED_PREREQUISITE",
            }
        )

    unsatisfied = {node for node in unsatisfied if node in nodes}

    # Check 1: no blocked row without an edge.
    for row in accounting:
        if row["status"] in BLOCKED:
            target = f"test:{row['test_id']}"
            if not any(edge["to"] == target for edge in edges):
                # Already reported by classify(); keep the check explicit so the clause's
                # rule is visible in one place.
                if not any("names no prerequisite" in v and row["test_id"] in v for v in violations):
                    violations.append(f"{row['test_id']} is blocked with no edge to a blocker")

    # Check 2: nothing independent is blocked by cascade.
    for edge in edges:
        target = nodes.get(edge["to"], {})
        status = target.get("status")
        if status and status not in BLOCKED:
            violations.append(
                f"{edge['to']} has a blocker but its recorded status is {status}: "
                "an independent row must not be blocked"
            )

    # Check 3: acyclic.
    adjacency: dict[str, list[str]] = {}
    for edge in edges:
        adjacency.setdefault(edge["from"], []).append(edge["to"])
    visiting: set[str] = set()
    visited: set[str] = set()

    def walk(node: str, path: list[str]) -> None:
        if node in visited:
            return
        if node in visiting:
            violations.append("cycle in the blocker graph: " + " -> ".join(path + [node]))
            return
        visiting.add(node)
        for neighbour in adjacency.get(node, []):
            walk(neighbour, path + [node])
        visiting.discard(node)
        visited.add(node)

    for node in list(adjacency):
        walk(node, [])

    # Check 4: a blocked gate names a gate that actually failed, and a passing gate is not
    # recorded as blocked.
    for name, row in sorted(gates.items()):
        prerequisite = row.get("blocked_by")
        if not prerequisite:
            if row.get("exit") is None and row.get("status") != "BLOCKED_PREREQUISITE":
                violations.append(f"gate {name} recorded no exit and no blocking prerequisite")
            continue
        source = gates.get(prerequisite)
        if source is None:
            violations.append(f"gate {name} names {prerequisite}, which is not in the results")
        elif source.get("exit") == 0:
            violations.append(
                f"gate {name} is recorded as blocked by {prerequisite}, which passed"
            )

    # Check 5: an independent test ran to completion inside a stage that holds a blocked one.
    blocked_stages = {
        row["stage"] for row in accounting if row["status"] in BLOCKED
    }
    continuation = [
        row["test_id"]
        for row in accounting
        if row["status"] == "PASS"
        and next(
            (r["default_stage"] for r in registry.values() if r["test_id"] == row["test_id"]),
            None,
        )
        in blocked_stages
    ]
    if blocked_stages and not continuation:
        violations.append(
            "no test continued to completion in a stage that holds a blocked row: cascade "
            "avoidance is asserted, not observed"
        )

    blast_radius = {
        node: sorted(edge["to"] for edge in edges if edge["from"] == node)
        for node in sorted(unsatisfied)
    }

    document = {
        "generated_from": {
            "registry": str(REGISTRY.relative_to(ROOT)),
            "accounting": str(ACCOUNTING.relative_to(ROOT)),
            "gate_results": str(GATE_RESULTS.relative_to(ROOT)),
        },
        "counts": {
            "tests": len(registry),
            "blocked": sum(1 for row in accounting if row["status"] in BLOCKED),
            "edges": len(edges),
            "unsatisfied_nodes": len(unsatisfied),
        },
        "unsatisfied_nodes": sorted(unsatisfied),
        "blast_radius": blast_radius,
        "nodes": [{"id": node, **body} for node, body in sorted(nodes.items())],
        "edges": edges,
        "cascade_avoidance": {
            "stages_holding_a_blocked_row": sorted(blocked_stages),
            "tests_that_continued": sorted(continuation),
        },
        "violations": violations,
    }
    Path(arguments.json).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Dependency Blocker Graph",
        "",
        "Generated by `python3 scripts/dependency-graph.py` from the test registry, the",
        "accounting, and the sweep's own gate results. DOD-031: a failed prerequisite blocks",
        "only tests with explicit dependency edges, and independent tests continue.",
        "",
        f"- registered tests: {len(registry)}",
        f"- blocked rows: {document['counts']['blocked']}",
        f"- edges: {len(edges)}",
        f"- unsatisfied nodes: {len(unsatisfied)}",
        "",
        "## Blast radius",
        "",
        "How many tests each unsatisfied node blocks. A node that blocked everything would be",
        "the blanket blocking this clause forbids.",
        "",
    ]
    for node, targets in blast_radius.items():
        lines.append(f"- `{node}` blocks {len(targets)}: {', '.join(t.split(':', 1)[1] for t in targets)}")
    lines += [
        "",
        "## Continuation",
        "",
        "Tests that ran to completion in a stage that also holds a blocked row: "
        + (", ".join(document["cascade_avoidance"]["tests_that_continued"]) or "none"),
        "",
        "## Gate edges",
        "",
    ]
    for edge in edges:
        if edge["from"].startswith("gate:"):
            lines.append(f"- `{edge['from']}` -> `{edge['to']}`")
    lines += ["", "## Violations", ""]
    lines += [f"- {violation}" for violation in violations] or ["- none"]
    lines.append("")
    Path(arguments.md).write_text("\n".join(lines), encoding="utf-8")

    print(f"dependency graph: {len(registry)} test(s), {len(edges)} edge(s), "
          f"{len(unsatisfied)} unsatisfied node(s)")
    for node, targets in blast_radius.items():
        print(f"  {node:<45} blocks {len(targets):>3} test(s)")
    print(f"  continuation observed in: {', '.join(document['cascade_avoidance']['tests_that_continued'][:6])}"
          f"{' …' if len(document['cascade_avoidance']['tests_that_continued']) > 6 else ''}")
    if violations:
        print()
        for violation in violations:
            print(f"  VIOLATION {violation}")
        print(f"dependency graph: {len(violations)} violation(s)")
        return 1
    print("dependency graph: no blanket block; every blocked row names its blocker")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
