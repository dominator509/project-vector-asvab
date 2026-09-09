#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

cargo run -p vector-tools -- artifact-identity \
  --binary "${1:-Cargo.toml}" \
  --migrations "${2:-migrations/001_initial.sql}" \
  --content "${3:-package.json}" \
  --sbom "${4:-pnpm-workspace.yaml}"
