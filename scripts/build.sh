#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

echo "Building workspace crates..."
cargo build --workspace --exclude vector-desktop

echo "Building frontend web app..."
pnpm --filter desktop build

if pkg-config --exists glib-2.0 2>/dev/null; then
  echo "Building Tauri desktop binary..."
  pnpm tauri build
else
  echo "NOT_RUNNABLE_ENV(headless): glib-2.0 system library missing for native Tauri bundling; web frontend and Rust workspace built cleanly."
fi
