"""Diagnose the hanging property test.

Suspicion: the xorshift PRNG in the fuzz suite can reach a fixed point where
it returns the same value forever, so a rejection loop never terminates. This
reproduces the generator in Python and checks for cycles.
"""

MASK = (1 << 64) - 1


def next_u64(state: int) -> int:
    x = state
    x ^= (x << 13) & MASK
    x ^= x >> 7
    x ^= (x << 17) & MASK
    return x & MASK


# Check for a fixed point: a state that maps to itself would loop forever.
fixed_points = []
state = 1
seen = set()
for _ in range(200000):
    state = next_u64(state)
    if state == 0:
        fixed_points.append("reached 0 (absorbing state)")
        break
    if state in seen:
        fixed_points.append(f"cycle detected at {state}")
        break
    seen.add(state)

print("issues:", fixed_points if fixed_points else "none in first 200k draws")

# The real hazard for a rejection loop: next_u32(bound) using modulo is fine,
# but if bound is 0 the helper returns 0 immediately, which is also fine.
# Check the alphabet index path: next_u32(len) must always be < len.
for seed in [0xA11CE, 0xBEEF, 0xF00D, 0xCAFE, 0x1234, 0x5678, 0x9ABC,
             0xD00D, 0xFEED, 0xABCD, 0x1357]:
    s = seed
    bad = 0
    for _ in range(10000):
        s = next_u64(s)
        idx = s % 42
        if idx >= 42:
            bad += 1
    print(f"seed {seed:#x}: out-of-range indices = {bad}")

print()
print("Conclusion: if no cycle or absorbing state is reported above, the hang is")
print("not in the PRNG and must be in a loop that depends on generated input.")
