# V-005

**Objective:** Build from clean checkout with lockfiles; record toolchain, artifact digest, SBOM, provenance, package contents.

**Inputs:** prior-stage evidence, master registry, applicable requirements, candidate artifact/revision.

**Execution:** use only COMMANDS.md and casebook commands materialized by the harness. Continue independent cases after non-cascading blockers.

**Evidence:** command, exit code, environment, artifact digest where applicable, result, blocker reason, cleanup, and hashes.

**Exit:** all cases assigned to this stage are accounted and no PASS lacks required evidence.
