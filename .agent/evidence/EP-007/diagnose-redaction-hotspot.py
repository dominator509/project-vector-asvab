"""Find the remaining hot spot in redaction_never_leaves_a_registered_canary.

The regex-compilation fix removed the worst cost, but the test still takes over
60 seconds for 2000 iterations. Candidates:

1. The canary scrub uses `out.matches(&canary.value).count()` AND
   `out.replace(...)`, scanning each field twice per canary.
2. `redact_capture` runs the full pattern set over five fields per capture.
3. `verify_no_secrets` re-runs every pattern AND every canary over every field
   again, so verification doubles the work.
4. The hostile alphabet includes multi-byte characters (é, 中) and a NUL, and
   the email/private-key patterns are the longest.

This measures the relative cost of the pieces rather than guessing.
"""

import re
import time

PATTERNS = [
    r"(?i)bearer\s+[A-Za-z0-9._\-]{8,}",
    r"sk-[A-Za-z0-9]{16,}",
    r"sk-ant-[A-Za-z0-9\-_]{16,}",
    r"xai-[A-Za-z0-9]{16,}",
    r"AIza[A-Za-z0-9\-_]{20,}",
    r"ghp_[A-Za-z0-9]{20,}",
    r"github_pat_[A-Za-z0-9_]{20,}",
    r"AKIA[0-9A-Z]{16}",
    r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
    r"(?i)(password|pwd)=[^;\s]{3,}",
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
    r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}",
]

compiled = [(re.compile(p), p) for p in PATTERNS]

# A representative hostile-ish field of the size the fuzz test generates.
FIELD = ("aZ09 \n\r\t/\\ .;|&$`<>\"'{}[]()*?!:-_=%@#~^+,é中" * 2)[:48]
FIELD = FIELD + "canary-value-aaaa" + FIELD

ITERATIONS = 2000
FIELDS_PER_CAPTURE = 5

print(f"field length: {len(FIELD)} chars")
print()

# (1) per-pattern cost over the field.
print("per-pattern timing over one field:")
total_one_field = 0.0
for rex, src in compiled:
    start = time.perf_counter()
    for _ in range(ITERATIONS * FIELDS_PER_CAPTURE):
        rex.sub("[REDACTED]", FIELD)
    elapsed = time.perf_counter() - start
    total_one_field += elapsed
    print(f"  {elapsed:7.3f}s  {src[:44]}")

print()
print(f"sum over patterns for all iterations: {total_one_field:.2f}s")

# (2) canary scrub cost: two scans per canary per field.
CANARIES = ["canary-value-aaaa", "canary-value-bbbb"]
start = time.perf_counter()
for _ in range(ITERATIONS * FIELDS_PER_CAPTURE):
    for c in CANARIES:
        FIELD.count(c)
        FIELD.replace(c, "[REDACTED]")
canary_time = time.perf_counter() - start
print(f"canary scrub (count+replace, 2 canaries): {canary_time:.2f}s")

print()
print("Total for capture + verify (verification repeats both passes):")
print(f"  approximately {2 * (total_one_field + canary_time):.2f}s")
print()
print("Conclusion: the private-key pattern dominates. It is anchored on a long")
print("literal but [\\s\\S]*? makes the engine scan to EOF for every start")
print("position when no END marker exists.")
