//! Content hashing used for provenance and tamper detection.
//!
//! CONTENT_PACK_SPEC.md requires installation to verify "schema, signature,
//! hashes, compatibility, provenance completeness". Content hashes are the
//! mechanism by which an item's identity is pinned, so this module is
//! deliberately small and exact.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A SHA-256 content hash in lowercase hex.
///
/// A newtype rather than a bare `String` so a hash cannot be confused with any
/// other text field, and so an empty hash is representable but detectably
/// invalid rather than silently accepted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash(String);

impl ContentHash {
    /// Hash bytes into a content hash.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        ContentHash(format!("{:x}", hasher.finalize()))
    }

    /// Hash text, normalizing line endings first.
    ///
    /// Normalization matters because the same content checked out on Windows and
    /// Linux would otherwise produce different hashes for identical material,
    /// which would make provenance verification fail for reasons unrelated to
    /// tampering.
    pub fn of_text(text: &str) -> Self {
        Self::of(text.replace("\r\n", "\n").as_bytes())
    }

    /// Parse an existing hex digest, validating its shape.
    pub fn parse(value: &str) -> Result<Self, String> {
        let trimmed = value.trim().to_lowercase();
        if trimmed.len() != 64 {
            return Err(format!(
                "a SHA-256 digest must be 64 hex characters, got {}",
                trimmed.len()
            ));
        }
        if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("digest contains non-hexadecimal characters".to_string());
        }
        Ok(ContentHash(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this hash is empty, which callers treat as absent.
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// Whether the given bytes match this hash.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        Self::of(bytes) == *self
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Whether two hashes match, treating an empty hash as "not comparable".
pub fn hashes_match(a: &ContentHash, b: &ContentHash) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b
}
