#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

# Dependency advisory scan.
#
# Two advisories are known and are dispositioned in `.cargo/audit.toml` with
# reachability analysis recorded in
# `.agent/evidence/EP-010/assess-vulnerabilities.py`:
#
#   RUSTSEC-2023-0071 (rsa)  - reached only via sqlx-mysql; VECTOR speaks SQLite
#                              only (ADR-002), so the code path is not exercised.
#   RUSTSEC-2024-0363 (sqlx) - affects the PostgreSQL and MySQL binary
#                              protocols; VECTOR uses the SQLite driver. The fix
#                              (sqlx 0.8.1) is currently blocked by a
#                              libsqlite3-sys version conflict with rusqlite.
#
# Ignoring them by identifier rather than by disabling the check keeps every
# OTHER advisory fatal, which is the property that matters.
cargo audit

# Static policy check over the whole dependency graph: advisories, licenses,
# banned crates and registry provenance.
cargo deny check

# Credential and forbidden-file scan over tracked files.
python3 scripts/secret-scan.py

# npm ecosystem.
corepack pnpm audit --prod
