#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never

if [ ! -f Cargo.toml ] || [ ! -f package.json ]; then
  echo "implementation repository surfaces Cargo.toml and package.json are absent; this blueprint correctly refuses to fabricate a green gate" >&2
  exit 3
fi
