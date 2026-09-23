"""Work out what this candidate's changes invalidate, and refuse a change nothing covers.

DOD-040: *any code, dependency, schema, configuration, build, test-oracle or artifact
change invalidates and reruns every affected downstream result.* Required evidence is the
change invalidation graph, the prior and new epoch IDs, the rerun list, and the current
evidence hashes -- which is exactly what this writes.

**How the invalidation is derived.** The epoch ledger (`EPOCHS.jsonl`) records which
commit and artifact each candidate epoch described. This script diffs the previous
epoch's commit against the current one *and* lists the working tree's uncommitted
changes, because a sweep on a dirty tree is verifying bytes no commit describes yet --
the state this repository was in for several rounds. Each changed path is mapped to a
surface, each surface to the gates that exercise it, and the rerun list is that set
closed over the gate edges the sweep declares.

**What it refuses.** A changed path under a production root that no surface covers: that
is a change no gate would notice, which is the failure the clause exists to prevent. It
also refuses when its gate-edge table disagrees with `scripts/verify.sh`, so the two
cannot drift apart while both claiming to describe the same pipeline.

Usage:
    python3 scripts/change-invalidation.py [--json <path>] [--md <path>]
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / ".agent/verification/state"
EPOCHS = STATE / "EPOCHS.jsonl"
MANIFEST = STATE / "RUN_MANIFEST.json"
VERIFY = ROOT / "scripts/verify.sh"

# Changed path prefix -> (surface, gates that exercise it). Order matters: the first match
# wins, so the more specific prefixes come first.
SURFACES: list[tuple[str, str, list[str]]] = [
    ("crates/vector-questions/", "content-generation", ["test-unit", "test-integration", "lint"]),
    ("crates/vector-application/", "application", ["test-unit", "test-integration", "lint"]),
    ("crates/vector-persistence/", "persistence", ["test-unit", "test-integration", "lint"]),
    ("crates/", "crate", ["test-unit", "test-integration", "lint"]),
    ("apps/desktop/src-tauri/", "desktop-shell", ["test-integration", "build"]),
    ("apps/desktop/src/", "interface", ["typecheck", "test-unit", "test-e2e"]),
    ("apps/desktop/e2e/", "browser-tests", ["test-e2e", "smoke-test"]),
    ("apps/desktop/", "interface", ["typecheck", "test-unit", "test-e2e"]),
    ("migrations/", "schema", ["test-integration", "build"]),
    ("tools/", "tooling", ["test-unit", "generated-pack"]),
    ("scripts/verify.sh", "harness", ["release-state", "dependency-graph"]),
    ("scripts/", "harness", ["anti-gaming-scan"]),
    ("Cargo.toml", "dependencies", ["dependency-audit", "build"]),
    ("Cargo.lock", "dependencies", ["dependency-audit", "build"]),
    ("pnpm-lock.yaml", "dependencies", ["dependency-audit", "build"]),
    ("package.json", "dependencies", ["dependency-audit", "typecheck"]),
    # Every operating document at the root, not a list of the two that existed when this table
    # was written: round 34 added a paragraph to AI_TRANSPORTS.md and the coverage check refused
    # it, which is the check working -- a documentation change no surface names is a change
    # nothing would notice.
    ("docs/", "documentation", []),
    ("COMMANDS.md", "documentation", []),
    ("README.md", "documentation", []),
    ("AGENTS.md", "control-plane", []),
    ("*.md", "documentation", []),
    (".agent/verification/", "accounting", ["release-state", "dependency-graph"]),
    (".agent/evidence/", "evidence", []),
    (".agent/state/", "ledger", []),
    (".agent/", "control-plane", []),
    ("AGENTS.md", "control-plane", []),
]

# Gates that consume the built artifact, and the gates that consume the identity. Mirrors
# `blocked_by` in scripts/verify.sh; the check below fails if they disagree.
GATE_EDGES: dict[str, list[str]] = {
    "artifact-identity": ["build"],
    "proof-matrix-stamp": ["artifact-identity"],
    "smoke-test": ["build"],
    "live-fire": ["build"],
    "release-state": ["artifact-identity"],
    "dependency-graph": ["release-state"],
    "change-invalidation": ["release-state"],
}

# Surfaces whose change means the artifact's bytes change, and therefore everything
# downstream of the build.
ARTIFACT_SURFACES = {
    "content-generation",
    "application",
    "persistence",
    "crate",
    "desktop-shell",
    "interface",
    "schema",
    "tooling",
    "dependencies",
}


def git(*arguments: str) -> str:
    """Run git and return its stdout, stripped of trailing newlines only.

    Not `strip()`: porcelain output *starts* with a status column, and a leading space is
    part of the format (`' M path'` is "modified, not staged"). Stripping it shifted every
    path on the first line by one character -- which this script's own coverage check
    caught, as a changed path (`.agent/...`) no surface covered.
    """
    result = subprocess.run(
        ["git", *arguments],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        return ""
    return result.stdout.rstrip("\n")


def digest(path: Path) -> str | None:
    if not path.exists():
        return None
    return hashlib.sha256(path.read_bytes()).hexdigest()


def declared_edges() -> dict[str, list[str]]:
    """The gate edges as `scripts/verify.sh` declares them."""
    text = VERIFY.read_text(encoding="utf-8")
    block = re.search(r"blocked_by\(\) \{(.*?)\n\}", text, re.S)
    if not block:
        return {}
    found: dict[str, list[str]] = {}
    for line in block.group(1).splitlines():
        match = re.match(r'\s*([a-z0-9-]+)\)\s*echo\s+"([^"]*)"', line)
        if match:
            found[match.group(1)] = match.group(2).split()
    return found


def surface_for(path: str) -> tuple[str, list[str]] | None:
    """The surface a changed path belongs to.

    A pattern beginning `*.` matches by suffix and only for a path with no directory: an
    operating document at the repository root is documentation, while `crates/…/notes.md` is
    source that happens to be prose, and treating the two the same would let a code change be
    filed as documentation.
    """
    for prefix, surface, gates in SURFACES:
        if prefix.startswith("*."):
            if "/" not in path and path.endswith(prefix[1:]):
                return surface, gates
            continue
        if path == prefix or path.startswith(prefix):
            return surface, gates
    return None


def main(argv: list[str]) -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", default=str(STATE / "CHANGE_INVALIDATION_GRAPH.json"))
    parser.add_argument("--md", default=str(STATE / "CHANGE_INVALIDATION_GRAPH.md"))
    arguments = parser.parse_args(argv)

    if not MANIFEST.exists():
        print("no run manifest; run sh scripts/verify.sh first")
        return 2
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    epoch = manifest.get("candidate_epoch")
    sha = manifest.get("git_sha", "")
    artifact = manifest.get("artifact_digest", "")

    history: list[dict] = []
    if EPOCHS.exists():
        for line in EPOCHS.read_text(encoding="utf-8").splitlines():
            if line.strip():
                history.append(json.loads(line))
    previous = next((row for row in reversed(history) if row.get("git_sha") != sha), None)
    current_row = {
        "epoch": epoch,
        "git_sha": sha,
        "artifact_digest": artifact,
        "recorded_at": datetime.now(timezone.utc).isoformat(),
    }
    if not history or history[-1].get("git_sha") != sha or history[-1].get("epoch") != epoch:
        history.append(current_row)
        EPOCHS.write_text(
            "".join(json.dumps(row) + "\n" for row in history), encoding="utf-8"
        )

    committed: list[str] = []
    if previous and previous.get("git_sha"):
        committed = [
            line
            for line in git("diff", "--name-only", previous["git_sha"], sha).splitlines()
            if line.strip()
        ]
    uncommitted = [
        line[3:].strip()
        for line in git("status", "--porcelain").splitlines()
        if line.strip() and not line.startswith("??")
    ]
    changed = sorted(set(committed) | set(uncommitted))

    violations: list[str] = []
    by_surface: dict[str, dict] = {}
    unmapped: list[str] = []
    for path in changed:
        found = surface_for(path)
        if found is None:
            unmapped.append(path)
            continue
        surface, gates = found
        entry = by_surface.setdefault(surface, {"paths": [], "gates": set()})
        entry["paths"].append(path)
        entry["gates"].update(gates)

    if unmapped:
        violations.append(
            "changed path(s) no surface covers, so no gate would notice them: "
            + ", ".join(unmapped[:5])
        )

    # The rerun list: the gates the changed surfaces name, closed over the gate edges.
    rerun: set[str] = set()
    for entry in by_surface.values():
        rerun.update(entry["gates"])
    if any(surface in ARTIFACT_SURFACES for surface in by_surface):
        rerun.add("build")
    growing = True
    while growing:
        growing = False
        for gate, prerequisites in GATE_EDGES.items():
            if set(prerequisites) & rerun and gate not in rerun:
                rerun.add(gate)
                growing = True

    verify_edges = declared_edges()
    if verify_edges != GATE_EDGES:
        violations.append(
            "the gate edges here disagree with scripts/verify.sh: "
            f"verify.sh declares {verify_edges}, this script declares {GATE_EDGES}"
        )

    evidence_hashes = {
        name: digest(STATE / name)
        for name in (
            "gate-results.jsonl",
            "RUN_MANIFEST.json",
            "DOD_STATUS.jsonl",
            "DEPENDENCY_BLOCKER_GRAPH.json",
        )
    }
    evidence_hashes["artifact_identity"] = digest(
        ROOT / ".agent/evidence/EP-009/artifact_identity.json"
    )

    document = {
        "prior_epoch": (previous or {}).get("epoch"),
        "prior_git_sha": (previous or {}).get("git_sha"),
        "prior_artifact_digest": (previous or {}).get("artifact_digest"),
        "new_epoch": epoch,
        "new_git_sha": sha,
        "new_artifact_digest": artifact,
        "committed_changes": committed,
        "uncommitted_changes": uncommitted,
        "changed_paths": changed,
        "surfaces": {
            surface: {"paths": sorted(entry["paths"]), "gates": sorted(entry["gates"])}
            for surface, entry in sorted(by_surface.items())
        },
        "rerun_list": sorted(rerun),
        "evidence_hashes": evidence_hashes,
        "violations": violations,
    }
    Path(arguments.json).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# Change Invalidation Graph",
        "",
        "Generated by `python3 scripts/change-invalidation.py` from the epoch ledger and this",
        "candidate's diff. DOD-040: a change to code, dependencies, schema, configuration, the",
        "build, a test oracle or the artifact invalidates every downstream result that rested",
        "on the old bytes.",
        "",
        f"- prior epoch: {document['prior_epoch']} at `{str(document['prior_git_sha'])[:8]}`",
        f"- new epoch: {document['new_epoch']} at `{str(sha)[:8]}`",
        f"- artifact: `{artifact}`",
        f"- changed paths: {len(changed)} ({len(committed)} committed, {len(uncommitted)} uncommitted)",
        "",
        "## Surfaces and what they invalidate",
        "",
    ]
    for surface, entry in document["surfaces"].items():
        lines.append(f"- **{surface}** ({len(entry['paths'])} path(s)): gates {', '.join(entry['gates']) or 'none'}")
    lines += [
        "",
        "## Rerun list",
        "",
        ", ".join(f"`{gate}`" for gate in document["rerun_list"]) or "nothing",
        "",
        "## Evidence hashes at this epoch",
        "",
    ]
    for name, value in evidence_hashes.items():
        lines.append(f"- `{name}`: `{value}`")
    lines += ["", "## Violations", ""]
    lines += [f"- {violation}" for violation in violations] or ["- none"]
    lines.append("")
    Path(arguments.md).write_text("\n".join(lines), encoding="utf-8")

    print(
        f"change invalidation: epoch {document['prior_epoch']} -> {document['new_epoch']}, "
        f"{len(changed)} changed path(s) over {len(by_surface)} surface(s)"
    )
    for surface, entry in document["surfaces"].items():
        print(f"  {surface:<20} {len(entry['paths']):>3} path(s) -> {', '.join(entry['gates']) or 'no gate'}")
    print(f"  rerun: {', '.join(document['rerun_list']) or 'nothing'}")
    if violations:
        print()
        for violation in violations:
            print(f"  VIOLATION {violation}")
        return 1
    print("change invalidation: every changed path is covered by a gate")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
