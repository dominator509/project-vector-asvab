# Release review checklist

A short list for whoever reviews a VECTOR release candidate. It exists because the
evidence a release produces is spread across several files, and a reviewer who reads
one of them has read a part.

1. **Which artifact?** `.agent/verification/state/RUN_MANIFEST.json` names the commit,
   the candidate epoch and the artifact digest this sweep verified.
2. **Did the gates pass?** `.agent/verification/state/gate-results.jsonl` records one
   exit code per gate. A gate recorded as `BLOCKED_PREREQUISITE` did not run: it
   consumes an artifact a failed gate would have produced.
3. **What is not done?** `.agent/verification/state/DOD_STATUS.jsonl` is the clause
   accounting, and `RESIDUAL_RISK_AND_EXTERNAL_GATES.md` says which of the open ones
   can be closed here and which need someone this repository cannot supply.
4. **Is the corpus sound?** `python3 scripts/probes/check-corpus.py <db>` prints the
   provenance invariants, the count of questions asked twice in one subtest, and a
   re-derivation of every executable proof. Every count should be zero.
5. **Is a rule load-bearing?** `python3 scripts/probes/mutation-round18.py` disables
   each rule in turn and requires the test that depends on it to fail. Run it alone.
6. **What does this candidate invalidate?** `CHANGE_INVALIDATION_GRAPH.md` maps the
   changed paths to the surfaces and gates they affect, with the rerun list.
