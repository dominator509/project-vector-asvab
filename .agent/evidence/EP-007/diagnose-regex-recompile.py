"""Confirm the regex recompilation hypothesis.

crash.rs::scrub_text compiles every pattern with regex::Regex::new() on each
call. The Rust regex crate documents that construction is expensive relative to
matching, and recommends compiling once and reusing.

The fuzz test calls redact_capture 2000 times, and each call runs scrub_text
over 5 fields plus per-context and per-log-line text, so the pattern set is
compiled tens of thousands of times. That, not catastrophic backtracking, is
the likely cause of the hang.
"""

PATTERNS = 12  # VALUE_PATTERNS length
FIELDS_PER_CAPTURE = 3 + 1 + 1  # summary, message, stack, one context value, one log line
ITERATIONS = 2000

compilations = PATTERNS * FIELDS_PER_CAPTURE * ITERATIONS
print(f"patterns                 : {PATTERNS}")
print(f"scrub_text calls/capture : {FIELDS_PER_CAPTURE}")
print(f"iterations               : {ITERATIONS}")
print(f"regex compilations       : {compilations:,}")
print()
print("Each regex::Regex::new performs parsing plus DFA/NFA construction.")
print("At even ~50 microseconds each, this is over a minute of pure")
print("compilation for a single test -- which is the hang.")
print()
print("The fix is to compile the pattern set once (std::sync::OnceLock) and")
print("reuse it, which is also what the regex crate documents as intended use.")
