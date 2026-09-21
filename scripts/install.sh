#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

corepack pnpm install --frozen-lockfile
cargo fetch --locked

# Security tooling that the gates run.
#
# `cargo audit` and `cargo deny` are cargo SUBCOMMANDS, not part of the Rust
# toolchain, and until now nothing in this repository declared them: the gates
# simply assumed both existed. They did on the machine the gates were written
# on. The first CI run to reach the security gate failed with
#
#     error: no such command: `audit`
#
# which is the same defect as assuming a globally installed pnpm. Pinned here so
# that a fresh checkout is provisioned by the documented install command, and so
# a version bump is a reviewable change rather than whatever the machine had.
#
# Installed only when the pinned version is absent, so re-running this script
# costs nothing and a warm cargo-bin cache lets CI skip the compile.
install_cargo_tool() {
  tool=$1
  version=$2
  if command -v "$tool" >/dev/null 2>&1 && "$tool" --version 2>/dev/null | grep -q "$version"; then
    echo "install: $tool $version is already present"
    return 0
  fi
  echo "install: building $tool $version (compiles from source; cached afterwards)"
  cargo install "$tool" --locked --version "$version"
}

install_cargo_tool cargo-audit 0.22.2
install_cargo_tool cargo-deny 0.19.9
