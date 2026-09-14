#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh
mkdir -p .agent/evidence/EP-004
# Starts a real MCP server over a real pipe and drives it with the real client:
# handshake, tool listing, an authorized read, capability/path/size denials, a
# malformed message, and the defanging of injected content.
cargo run -p vector-tools -- mcp probe-loopback \
  --out .agent/evidence/EP-004/mcp-loopback.json
