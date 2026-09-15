#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

echo "Building workspace crates..."
cargo build --workspace --exclude vector-desktop

echo "Building frontend web app..."
pnpm --filter desktop build

# The packaged desktop artifact.
#
# This gate previously tested `pkg-config --exists glib-2.0` on every platform,
# so on Windows it always took the "headless" branch: `pnpm tauri build` never
# ran, `target/release/vector-desktop.exe` was never produced by the legal build
# command, and the live-fire check was left testing whatever binary happened to
# be lying around. A gate that quietly checks nothing is worse than one that
# fails.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*|Windows*)
    echo "Building Tauri desktop artifact and installers (Windows)..."
    # The Tauri CLI reads `CI` as a boolean flag and rejects the `1` the rest of
    # these scripts standardise on, so it is given `true` for this one command.
    CI=true pnpm tauri build
    ;;
  *)
    if pkg-config --exists glib-2.0 2>/dev/null; then
      echo "Building Tauri desktop artifact and installers..."
      CI=true pnpm tauri build
    else
      echo "NOT_RUNNABLE_ENV(headless): glib-2.0 system library missing for native Tauri bundling; web frontend and Rust workspace built cleanly."
    fi
    ;;
esac
