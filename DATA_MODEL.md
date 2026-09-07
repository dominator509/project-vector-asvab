# Data Model Blueprint

SQLite tables are grouped into migrations for learner, study, content/evidence, model provenance, MCP grants/audit, diagnostics/crash, and release metadata. Key IDs are UUIDv7 or deterministic content hashes as appropriate. Attempt events are append-oriented; derived mastery/readiness snapshots are rebuildable. Evidence claims reference source IDs and content hashes. Approved question instances reference generator/template/version, solver result, reviewer status, and content-pack signature. No cloud identity is required for the initial product.
