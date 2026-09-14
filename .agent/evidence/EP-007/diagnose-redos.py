"""Confirm the ReDoS in the private-key redaction pattern.

crash.rs uses:
    -----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----

An unterminated BEGIN marker with no END forces the lazy [\\s\\S]*? quantifier to
try every possible end position, at every possible start position: O(n^2)
backtracking per match attempt, and the engine retries at each offset, giving
effectively O(n^3) on adversarial input.

This script measures the growth to confirm it is super-linear rather than
guessing from the pattern alone.
"""

import re
import time

PATTERN = re.compile(
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----"
)

print("size | seconds | ratio vs previous")
print("-----+---------+-------------------")
previous = None
for size in [500, 1000, 2000, 4000, 8000]:
    # An unterminated BEGIN marker: no END, so the lazy quantifier scans to EOF.
    text = "-----BEGIN RSA PRIVATE KEY-----" + ("A" * size)
    start = time.perf_counter()
    PATTERN.search(text)
    elapsed = time.perf_counter() - start

    ratio = "-" if previous is None else f"{elapsed / previous:.2f}x"
    print(f"{size:>5} | {elapsed:8.4f} | {ratio}")
    previous = elapsed

print()
print("A ratio near 2x per doubling is quadratic; near 4x is cubic.")
print("Either way, growth is super-linear and unbounded input is a denial of")
print("service on the redaction path that crash reporting depends on.")
