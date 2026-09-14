"""Why did MUT-B4 (bundle entry path safety) survive?

The check `is_safe_entry_path(&entry.path)` guards writing entries into the
archive. Removing it changed nothing, which means either every path the
assembler produces is already safe by construction, or the test suite does not
exercise the failure path.
"""

# The paths assemble_bundle hardcodes.
PRODUCED = [
    "manifest.json",
    "diagnostic.json",
    "events.ndjson",
    "repro.yaml",
    "environment.json",
    "consent.json",
]


def is_safe(path: str) -> bool:
    if not path or path.startswith("/") or path.startswith("\\"):
        return False
    if len(path) >= 2 and path[1] == ":":
        return False
    return not any(seg == ".." for seg in path.replace("\\", "/").split("/"))


print("Paths the assembler actually produces:")
for p in PRODUCED:
    print(f"  {p:20} safe={is_safe(p)}")

unsafe = [p for p in PRODUCED if not is_safe(p)]
print()
print(f"unsafe produced paths: {unsafe if unsafe else 'none'}")
print()
print("Conclusion: every path is a hardcoded literal, so the guard cannot fire")
print("through the public assembler. The dedicated is_safe_entry_path test calls")
print("the predicate directly, which is why the function is covered but the")
print("guard INSIDE assemble_bundle is not.")
print()
print("This is the same class as the EP-006 denylist sweep: a defence-in-depth")
print("check whose removal is unobservable because nothing reachable produces")
print("the dangerous input. It is retained deliberately -- it protects against a")
print("future change that derives an entry path from capture content -- but the")
print("mutation result must be recorded honestly rather than hidden.")
