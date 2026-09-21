#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

# Prerequisite, checked rather than assumed. `cargo deny` is a cargo subcommand,
# not part of the Rust toolchain; the first CI run failed on its sibling
# `cargo audit` with "no such command".
if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "dependency-audit: cargo-deny is not installed." >&2
  echo "  It is a cargo subcommand, not part of the Rust toolchain." >&2
  echo "  Run 'sh scripts/install.sh' to provision the pinned version." >&2
  exit 3
fi

# Rust dependency policy: advisories, licenses, banned crates and provenance.
cargo deny check

# npm dependency license inventory. This previously called `pnpm licenses:check`,
# a script that existed in no package.json, so the gate could never pass. The
# SBOM generator from EP-009 does the real work: it reads every installed
# package's declared license and refuses to proceed when one is unstated or
# not permitted by the release policy.
cargo run -q -p vector-tools -- sbom \
  --lockfile pnpm-lock.yaml \
  --node-modules node_modules/.pnpm \
  --out .agent/evidence/EP-010/npm-licenses.spdx

echo "npm license inventory: ok"
