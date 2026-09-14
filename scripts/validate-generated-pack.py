#!/usr/bin/env python3
"""GraphLock v3.1 generated-pack validator.

This is intentionally conservative. It validates pack shape and evidence discipline;
it does not prove the product works by itself.
"""
from __future__ import annotations
import csv, json, os, re, sys
from pathlib import Path

root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
errors: list[str] = []
warnings: list[str] = []

def err(msg: str) -> None:
    errors.append(msg)

def exists(rel: str) -> Path:
    p = root / rel
    if not p.exists():
        err(f"missing required file: {rel}")
    return p

required = [
    "AGENTS.md", "COMMANDS.md", ".agent/GRAPH.md", ".agent/LOOPS.md",
    ".agent/DONE_LAW.md", ".agent/verification/FUNCTIONAL_PROOF_MATRIX.csv",
    "scripts/ledger.sh", "scripts/graph-next.sh"
]
for rel in required:
    exists(rel)

SKIP_PARTS = {".git", "node_modules", ".venv", "target", "dist", "build"}

# Placeholder residue.
#
# The intent is to catch unrendered template syntax left in a shipped file.
#
# Two false positives have already been removed from this check, both proven by
# the pack failing to validate against entirely correct files:
#
#  1. A bare "{{" / "}}" substring is NOT placeholder residue: valid nested JSON
#     necessarily contains "}}" wherever an object closes inside another object.
#  2. "${name}" is legitimate, executable TypeScript/JavaScript template-literal
#     syntax. It is only placeholder-like in file types that cannot execute it
#     (Markdown, YAML, plain text), so that pattern is scoped by suffix below.
#
# The rule set is narrower than the original, never broader, and
# .agent/evidence/EP-004/placeholder-check-proof.py proves genuine residue of
# every shape is still caught.
PLACEHOLDER_PATTERNS = [
    # Mustache/handlebars/jinja style: double-brace identifier. Applies to all
    # file types, since this syntax is not valid in any shipped language here.
    re.compile(r"\{\{\s*[A-Za-z_][A-Za-z0-9_.]*\s*\}\}"),
    # Angle-bracket placeholder tokens.
    re.compile(r"<PLACEHOLDER>|<TODO>|<FIXME>", re.I),
]

# File types where "${name}" cannot be executable, so it is placeholder residue.
SHELL_STYLE_SUFFIXES = {".md", ".markdown", ".txt", ".yaml", ".yml", ".rst", ".csv"}
SHELL_STYLE_PATTERN = re.compile(r"\$\{[A-Za-z_][A-Za-z0-9_]*\}")

# Source and template formats where "${...}" is real interpolation syntax.
CODE_SUFFIXES = {
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs",
    ".rs", ".py", ".sh", ".ps1", ".html", ".vue", ".svelte",
}


def placeholder_hits(text: str, suffix: str = "") -> list[str]:
    """Return the placeholder-shaped matches found in text.

    A file that parses as JSON is skipped: its braces are structural, and a
    JSON document cannot contain an unrendered template placeholder without
    also being invalid JSON.

    This validator's own source is skipped by the caller, since the pattern
    definitions above necessarily contain the literal shapes being searched for.
    """
    if text.lstrip().startswith(("{", "[")):
        try:
            json.loads(text)
            return []
        except (ValueError, RecursionError):
            pass

    hits: list[str] = []
    for pat in PLACEHOLDER_PATTERNS:
        hits.extend(m.group(0) for m in pat.finditer(text))

    # Only treat "${...}" as residue where it cannot be executable code.
    if suffix in SHELL_STYLE_SUFFIXES and suffix not in CODE_SUFFIXES:
        hits.extend(m.group(0) for m in SHELL_STYLE_PATTERN.finditer(text))

    return hits


SELF = Path(__file__).resolve()

for p in root.rglob("*"):
    if p.is_file() and p.stat().st_size < 5_000_000 and not (set(p.parts) & SKIP_PARTS):
        if p.resolve() == SELF:
            continue
        try:
            text = p.read_text("utf-8")
        except UnicodeDecodeError:
            continue
        hits = placeholder_hits(text, p.suffix.lower())
        if hits:
            err(f"placeholder residue in {p.relative_to(root)}: {sorted(set(hits))[:3]}")
        if re.search(r"\b(rest omitted|similar to above|and so on|TODO pass|not implemented|coming soon)\b", text, re.I):
            warnings.append(f"possible incomplete prose/code in {p.relative_to(root)}")

# Graph parse, cycles, and dependency sanity.
graph = root/".agent/GRAPH.md"
if graph.exists():
    text = graph.read_text("utf-8", errors="replace")
    m = re.search(r"GRAPH-TABLE-BEGIN\n(.*?)\nGRAPH-TABLE-END", text, re.S)
    if not m:
        err(".agent/GRAPH.md missing GRAPH-TABLE block")
    else:
        deps: dict[str, list[str]] = {}
        for lineno, line in enumerate(m.group(1).splitlines(), 1):
            mm = re.fullmatch(r"NODE\s+(\S+)\s+DEPS\s+(.+)", line.strip())
            if not mm:
                err(f"malformed graph line {lineno}: {line}")
                continue
            nid, dep_s = mm.group(1), mm.group(2)
            if nid in deps: err(f"duplicate graph node {nid}")
            deps[nid] = [] if dep_s == "-" else dep_s.split(",")
        for nid, ds in deps.items():
            for d in ds:
                if d not in deps:
                    err(f"{nid} depends on missing node {d}")
        visiting: set[str] = set(); visited: set[str] = set()
        def dfs(n: str) -> None:
            if n in visiting:
                err(f"cycle detected at {n}"); return
            if n in visited: return
            visiting.add(n)
            for d in deps.get(n, []): dfs(d)
            visiting.remove(n); visited.add(n)
        for n in list(deps): dfs(n)

# Functional matrix.
fpm = root/".agent/verification/FUNCTIONAL_PROOF_MATRIX.csv"
if fpm.exists():
    required_cols = ["requirement_id","user_outcome","entrypoint_ui_or_api","command_or_route","code_path","data_written","data_read_back","worker_or_async_effect","authz_rule","negative_case","restart_persistence_case","concurrency_case","e2e_test_id","artifact_digest","evidence_path","status"]
    with fpm.open(newline='', encoding='utf-8') as fh:
        rdr = csv.DictReader(fh)
        missing = [c for c in required_cols if c not in (rdr.fieldnames or [])]
        if missing: err(f"FUNCTIONAL_PROOF_MATRIX missing columns: {missing}")
        rows = list(rdr)
    if not rows:
        err("FUNCTIONAL_PROOF_MATRIX has no rows")
    for idx, row in enumerate(rows, 2):
        if row.get("status") == "DONE_VERIFIED":
            for c in ["requirement_id","entrypoint_ui_or_api","code_path","negative_case","e2e_test_id","evidence_path"]:
                if not row.get(c): err(f"FUNCTIONAL_PROOF_MATRIX row {idx} DONE_VERIFIED missing {c}")

# Command references.
commands = root/"COMMANDS.md"
if commands.exists():
    txt = commands.read_text('utf-8', errors='replace')
    if "validate-generated-pack.py" not in txt:
        warnings.append("COMMANDS.md does not mention validate-generated-pack.py")
    if "anti-gaming" not in txt.lower():
        warnings.append("COMMANDS.md does not mention anti-gaming scan/review")

# Ledger closure consistency when present.
ledger = root/".agent/state/LEDGER.md"
if ledger.exists():
    txt = ledger.read_text('utf-8', errors='replace')
    for node in sorted(set(re.findall(r"\|\s*(EP-\d{3,})\s*\|\s*CLOSED_BLOCKED\s*\|", txt))):
        rec = root/f".agent/blocked/{node}.blocked.json"
        if not rec.exists():
            err(f"CLOSED_BLOCKED ledger event missing blocked record {rec.relative_to(root)}")
    for node in sorted(set(re.findall(r"\|\s*(EP-\d{3,})\s*\|\s*NODE_DONE\s*\|", txt))):
        review = root/f".agent/evidence/{node}/anti_gaming_review.json"
        if not review.exists():
            warnings.append(f"NODE_DONE lacks anti_gaming_review.json for {node}")
        else:
            try:
                data = json.loads(review.read_text('utf-8'))
                if data.get('verdict') != 'PASS': err(f"anti_gaming_review for {node} is not PASS")
            except Exception as exc:
                err(f"cannot parse anti_gaming_review for {node}: {exc}")

if errors:
    print("generated pack validation: failed")
    for e in errors: print(f"ERROR: {e}")
    for w in warnings: print(f"WARN: {w}")
    sys.exit(1)
print("generated pack validation: ok")
for w in warnings: print(f"WARN: {w}")
