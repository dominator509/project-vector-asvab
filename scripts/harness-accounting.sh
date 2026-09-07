#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

python3 - <<'PY2'
import csv
from collections import Counter
p='.agent/verification/reports/COMPLETE_TEST_ACCOUNTING.csv'
rows=list(csv.DictReader(open(p,encoding='utf-8')))
print('total',len(rows)); print(dict(Counter(r['status'] for r in rows)))
if len(rows)!=484: raise SystemExit('registry accounting must contain exactly 484 rows')
if any(r['status']=='PENDING' for r in rows): raise SystemExit(3)
PY2
