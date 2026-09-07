#!/usr/bin/env python3
"""GraphLock v3.1 anti-gaming scan.

Finds patterns that often indicate gate hacking, fake closure, skipped tests,
fixture hardcoding, or catch-and-ignore success. Matches require classification;
severe production matches should block release.
"""
from __future__ import annotations
import argparse, re, sys
from pathlib import Path

PATTERNS = [
    ("TEST_SKIP", re.compile(r"\b(test|it|describe)\.skip\b|pytest\.mark\.skip|pytest\.mark\.xfail|#\[ignore\]", re.I)),
    ("TEST_ONLY_BRANCH", re.compile(r"NODE_ENV\s*={0,2}\s*['\"]test|process\.env\.CI|pytest|vitest|jest", re.I)),
    ("FAKE_SUCCESS", re.compile(r"fakeSuccess|mockSuccess|return\s+true\s*;|success\s*:\s*true", re.I)),
    ("SWALLOWED_ERROR", re.compile(r"except\s+Exception\s*:\s*(pass|return)|catch\s*\([^)]*\)\s*\{\s*(return|/\*)", re.I|re.S)),
    ("PLACEHOLDER", re.compile(r"not implemented|coming soon|TODO pass|placeholder|dummy|stub|simulate|simulation|bypass|force pass", re.I)),
    ("HARDCODED_FIXTURE", re.compile(r"test@example\.com|known-test|fixture|hardcoded", re.I)),
]
SKIP_DIRS = {'.git','node_modules','.venv','venv','target','dist','build','.next','.cache'}
TEXT_SUFFIX = {'.py','.js','.jsx','.ts','.tsx','.rs','.go','.java','.kt','.swift','.rb','.php','.sh','.md','.yaml','.yml','.toml','.json','.txt','.csv'}

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('root', nargs='?', default='.')
    ap.add_argument('--fail-on-match', action='store_true')
    args = ap.parse_args()
    root = Path(args.root).resolve()
    matches = []
    for p in root.rglob('*'):
        if not p.is_file(): continue
        rel_parts = set(p.relative_to(root).parts)
        if rel_parts & SKIP_DIRS: continue
        if p.suffix and p.suffix not in TEXT_SUFFIX: continue
        try: text = p.read_text('utf-8')
        except Exception: continue
        for name, rx in PATTERNS:
            for m in rx.finditer(text):
                line = text.count('\n', 0, m.start()) + 1
                snippet = text[m.start():m.end()].replace('\n',' ')[:160]
                matches.append((name, str(p.relative_to(root)), line, snippet))
    if matches:
        print('anti-gaming scan: matches require classification')
        for name, path, line, snippet in matches:
            print(f'{name}\t{path}:{line}\t{snippet}')
        return 1 if args.fail_on_match else 0
    print('anti-gaming scan: no matches')
    return 0
if __name__ == '__main__':
    raise SystemExit(main())
