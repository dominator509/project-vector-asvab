#!/usr/bin/env sh
set -eu
export CI=1 NO_COLOR=1 PAGER=cat GIT_PAGER=cat CARGO_TERM_COLOR=never
sh scripts/_require_impl.sh

# The artifact identity binds six components (REQ-053).
#
# This script previously passed placeholder paths -- `Cargo.toml` as the binary,
# `package.json` as content, `pnpm-workspace.yaml` as the SBOM -- and no
# `--licenses` or `--git-sha` at all, so `vector-tools artifact-identity`
# refused and the script had never produced an identity.
#
# Migrations and content are passed as directories: they are sets of files, and
# binding one representative file would mean adding a migration did not change
# the identity.
BINARY="${1:-target/release/vector-desktop.exe}"
MIGRATIONS="${2:-migrations}"
CONTENT="${3:-apps/desktop/dist}"
SBOM="${4:-.agent/evidence/EP-010/npm-licenses.spdx}"
LICENSES="${5:-.agent/evidence/EP-009/THIRD_PARTY_NOTICES.md}"
OUT="${6:-.agent/evidence/EP-009/artifact_identity.json}"

for required in "$BINARY" "$SBOM" "$LICENSES"; do
  if [ ! -e "$required" ]; then
    echo "artifact identity: $required is absent; run 'sh scripts/build.sh' and 'sh scripts/dependency-audit.sh' first" >&2
    exit 3
  fi
done

GIT_SHA="$(git rev-parse HEAD)"

cargo run -q -p vector-tools -- artifact-identity \
  --binary "$BINARY" \
  --migrations "$MIGRATIONS" \
  --content "$CONTENT" \
  --sbom "$SBOM" \
  --licenses "$LICENSES" \
  --git-sha "$GIT_SHA" \
  --version 0.1.0 > "$OUT"

echo "artifact identity written to $OUT"
cargo run -q -p vector-tools -- artifact-identity \
  --binary "$BINARY" \
  --migrations "$MIGRATIONS" \
  --content "$CONTENT" \
  --sbom "$SBOM" \
  --licenses "$LICENSES" \
  --git-sha "$GIT_SHA" \
  --version 0.1.0 \
  --verify "$OUT"
