#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 - <<'PY2'
from pathlib import Path
import re
text=Path('.agent/GRAPH.md').read_text()
block=re.search(r'GRAPH-TABLE-BEGIN\n(.*?)\nGRAPH-TABLE-END',text,re.S).group(1)
nodes=[]
for line in block.splitlines():
 m=re.fullmatch(r'NODE\s+(\S+)\s+DEPS\s+(.+)',line.strip())
 if m: nodes.append((m.group(1),[] if m.group(2)=='-' else m.group(2).split(',')))
ledger=Path('.agent/state/LEDGER.md').read_text() if Path('.agent/state/LEDGER.md').exists() else ''
done=set(re.findall(r'\|\s*(EP-\d+)\s*\|\s*NODE_DONE\s*\|',ledger))
ready=[n for n,deps in nodes if n not in done and all(d in done for d in deps)]
print(ready[0] if ready else 'NO_READY_NODE')
PY2
