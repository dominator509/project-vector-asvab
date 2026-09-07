#!/usr/bin/env sh
# GraphLock v3: materialize the losslessly embedded canonical atomic-security prompt library.
# No network access. Writes only under .agent/verification/source-library/security/.
set -eu

PAYLOAD=".agent/verification/atomic-security-sources.tar.gz.b64"
DEST_ROOT=".agent/verification/source-library"
DEST="$DEST_ROOT/security"
ARCHIVE_SHA256="b9b07b2bd16bf5a284a645735a3fedeff5f8d80da06e428c39daa5e143608826"
GENERAL_SHA256="559cadaf736319a4f422ade27287407c42d36910d04fb6a0b7031e9c5d40bfde"
HIPAA_SHA256="4b6aa580fd715d6f140d606d491c3e2f0e777fac78a72068b58e3bb269c5043d"
BLOCKCHAIN_SHA256="3666d59dd36d3457b802f04ce340d50306925bc4a9d46e2a33fb4ed825df2bc2"

fail() { echo "atomic source library: FAIL - $1" >&2; exit 1; }
[ -f "$PAYLOAD" ] || fail "missing payload: $PAYLOAD"
command -v tar >/dev/null 2>&1 || fail "tar is required"
command -v mktemp >/dev/null 2>&1 || fail "mktemp is required"

hash_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$file" <<'PY'
import hashlib, pathlib, sys
print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
  else
    fail "need sha256sum, shasum, or python3 for integrity verification"
  fi
}

decode_payload() {
  input="$1"
  output="$2"
  if command -v base64 >/dev/null 2>&1; then
    if base64 -d "$input" > "$output" 2>/dev/null; then
      return 0
    fi
    if base64 -D "$input" > "$output" 2>/dev/null; then
      return 0
    fi
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$input" "$output" <<'PY'
import base64, pathlib, sys
src = pathlib.Path(sys.argv[1]).read_bytes()
pathlib.Path(sys.argv[2]).write_bytes(base64.b64decode(src, validate=False))
PY
    return 0
  fi
  return 1
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
ARCHIVE="$TMP/atomic-security-sources.tar.gz"
EXTRACT="$TMP/extracted"
mkdir -p "$EXTRACT"

decode_payload "$PAYLOAD" "$ARCHIVE" || fail "no compatible base64 decoder"
[ "$(hash_file "$ARCHIVE")" = "$ARCHIVE_SHA256" ] || fail "archive SHA-256 mismatch"

EXPECTED_LIST='security/
security/blockchain-security-testing-prompts.md
security/general-dev-security-testing-prompts.md
security/hipaa-software-dev-security-testing-prompts.md'
ACTUAL_LIST=$(tar -tzf "$ARCHIVE") || fail "archive listing failed"
[ "$ACTUAL_LIST" = "$EXPECTED_LIST" ] || fail "archive contains unexpected paths"

tar -xzf "$ARCHIVE" -C "$EXTRACT" || fail "archive extraction failed"
GENERAL="$EXTRACT/security/general-dev-security-testing-prompts.md"
HIPAA="$EXTRACT/security/hipaa-software-dev-security-testing-prompts.md"
BLOCKCHAIN="$EXTRACT/security/blockchain-security-testing-prompts.md"
[ -f "$GENERAL" ] && [ -f "$HIPAA" ] && [ -f "$BLOCKCHAIN" ] || fail "one or more canonical source files are missing"
[ "$(hash_file "$GENERAL")" = "$GENERAL_SHA256" ] || fail "General source SHA-256 mismatch"
[ "$(hash_file "$HIPAA")" = "$HIPAA_SHA256" ] || fail "HIPAA source SHA-256 mismatch"
[ "$(hash_file "$BLOCKCHAIN")" = "$BLOCKCHAIN_SHA256" ] || fail "Blockchain source SHA-256 mismatch"

mkdir -p "$DEST_ROOT"
NEXT="$DEST_ROOT/.security.next.$$"
rm -rf "$NEXT"
mv "$EXTRACT/security" "$NEXT"
rm -rf "$DEST"
mv "$NEXT" "$DEST"

echo "atomic source library: ok"