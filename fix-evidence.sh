#!/bin/bash
# 1. Regenerate EP-002 and EP-003 evidences.
mkdir -p .agent/evidence/EP-002 .agent/evidence/EP-003 .agent/evidence/EP-000 .agent/evidence/EP-001

sh scripts/test-unit.sh > .agent/evidence/EP-002/unit-tests.log 2>&1
echo $? > .agent/evidence/EP-002/unit-tests.exitcode
cd .agent/evidence/EP-002 && sha256sum unit-tests.log > unit-tests.log.sha256 && cd -

python3 scripts/anti-gaming-scan.py . > .agent/evidence/EP-002/anti-gaming-scan.log 2>&1
echo $? > .agent/evidence/EP-002/anti-gaming-scan.exitcode
cd .agent/evidence/EP-002 && sha256sum anti-gaming-scan.log > anti-gaming-scan.log.sha256 && cd -

sh scripts/dod-gate.sh > .agent/evidence/EP-002/dod-gate.log 2>&1
echo $? > .agent/evidence/EP-002/dod-gate.exitcode
cd .agent/evidence/EP-002 && sha256sum dod-gate.log > dod-gate.log.sha256 && cd -

# Fix EP-001/EP-000 claim files
sh scripts/dod-gate.sh > .agent/evidence/EP-000/dod-gate.log 2>&1
cd .agent/evidence/EP-000 && sha256sum dod-gate.log > dod-gate.log.sha256 && cd -

sh scripts/harness-accounting.sh > .agent/evidence/EP-000/accounting.log 2>&1
cd .agent/evidence/EP-000 && sha256sum accounting.log > accounting.log.sha256 && cd -

python3 scripts/validate-generated-pack.py . > .agent/evidence/EP-000/pack_validation.log 2>&1
cd .agent/evidence/EP-000 && sha256sum pack_validation.log > pack_validation.log.sha256 && cd -

sh scripts/preflight.sh > .agent/evidence/PREFLIGHT/preflight.log 2>&1
cd .agent/evidence/PREFLIGHT && sha256sum preflight.log > preflight.log.sha256 && cd -

rm -f .agent/evidence/EP-001/status.txt .agent/evidence/PREFLIGHT/pack.txt .agent/evidence/EP-000/dod-gate.txt .agent/evidence/EP-000/accounting.txt .agent/evidence/EP-000/pack_validation.txt .agent/evidence/EP-000/status.txt .agent/evidence/EP-000/ledger.md
