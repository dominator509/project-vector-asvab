#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

if [ "$#" -lt 3 ]; then echo "usage: sh scripts/ledger.sh NODE EVENT EVIDENCE_PATH [COMMAND] [EXIT_CODE]" >&2; exit 2; fi
node=$1; event=$2; evidence=$3; command=${4:-}; code=${5:-0}
python3 - "$node" "$event" "$evidence" "$command" "$code" <<'PY2'
from pathlib import Path
import sys,json,hashlib,datetime,subprocess
node,event,evidence,command,code=sys.argv[1:]
p=Path('.agent/state/LEDGER.jsonl'); p.parent.mkdir(parents=True,exist_ok=True)
prev='0'*64
if p.exists() and p.stat().st_size:
 last=json.loads(p.read_text().splitlines()[-1]); prev=last['event_hash']
sha=''
ep=Path(evidence)
if ep.exists() and ep.is_file(): sha=hashlib.sha256(ep.read_bytes()).hexdigest()
try: commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True,stderr=subprocess.DEVNULL).strip()
except Exception: commit='UNPINNED'
e={'event_id':f'{node}-{int(datetime.datetime.now(datetime.timezone.utc).timestamp()*1000)}','timestamp_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'agent_id':'executor','mode':'EXECUTOR','node_id':node,'event':event,'candidate_epoch':0,'commit_sha':commit,'artifact_digest':'','command':command,'exit_code':int(code),'evidence_paths':[evidence],'evidence_sha256':[sha] if sha else [],'previous_event_hash':prev}
canon=json.dumps(e,sort_keys=True,separators=(',',':')).encode(); e['event_hash']=hashlib.sha256(prev.encode()+canon).hexdigest()
with p.open('a') as f: f.write(json.dumps(e,separators=(',',':'))+'\n')
md=Path('.agent/state/LEDGER.md')
with md.open('a') as f: f.write(f'| {node} | {event} | 0 | {evidence} |\n')
print(e['event_hash'])
PY2
