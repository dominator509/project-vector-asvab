//! Content pack domain type with real Ed25519 signing (REQ-023).
//!
//! `CONTENT_PACK_SPEC.md` requires packs to carry "a detached signature" that
//! installation verifies before atomic activation. The previous implementation
//! returned the string literal `"dummy_signature"` from `sign()`, ignored the
//! supplied key entirely, and force-set `reviewed = true` — three separate
//! falsehoods in one method. It has been replaced with real Ed25519 signing.
//!
//! ## What the signature covers, and what it does not
//!
//! The signature covers the pack's **content identity**: its name, version and
//! content hash. It deliberately does **not** cover lifecycle state (status,
//! freshness), because quarantining or reactivating a pack does not change the
//! content that was attested. Signing lifecycle state would mean every status
//! transition invalidated the signature, which would push callers toward
//! re-signing casually.
//!
//! ## Review is not signing
//!
//! This type no longer claims review status. Review is a separate governance
//! decision recorded in the content QA ledger (`vector_content::content_qa` at
//! the application layer); a signature proves *who attested the content*, never
//! that a human reviewed it. The previous implementation conflated the two,
//! which would have let possession of a signing key stand in for a review.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Lifecycle state of a content pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentStatus {
    /// Staged but not serving learners.
    Quarantined,
    /// Serving learners.
    Active,
    /// Retired.
    Archived,
}

/// Why a signature could not be produced or verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    /// The pack carries no signature.
    NotSigned,
    /// The pack carries no signer key.
    NoSigner,
    /// The content hash was empty, so there is nothing to attest.
    EmptyContentHash,
    /// The signature bytes were not a valid Ed25519 signature.
    MalformedSignature,
    /// The signer key bytes were not a valid Ed25519 public key.
    MalformedKey,
    /// The signature did not verify against the signer key and content.
    VerificationFailed,
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            SignatureError::NotSigned => "the content pack is not signed",
            SignatureError::NoSigner => "the content pack records no signer key",
            SignatureError::EmptyContentHash => {
                "the content pack has an empty content hash, so there is nothing to attest"
            }
            SignatureError::MalformedSignature => "the signature is not a valid Ed25519 signature",
            SignatureError::MalformedKey => "the signer key is not a valid Ed25519 public key",
            SignatureError::VerificationFailed => {
                "the signature does not match the pack content and signer key"
            }
        };
        write!(f, "{text}")
    }
}

impl std::error::Error for SignatureError {}

/// The domain tag prefixed to every signing payload.
///
/// Domain separation prevents a signature produced for one purpose being
/// replayed as a signature for another, which would be possible if two message
/// types shared a payload format.
const SIGNING_DOMAIN: &[u8] = b"VECTOR-CONTENT-PACK-v1";

/// A versioned, signed content pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPack {
    name: String,
    version: u32,
    content_hash: String,
    status: ContentStatus,
    freshness_timestamp: DateTime<Utc>,
    /// Detached Ed25519 signature over the canonical payload.
    signature: Option<Vec<u8>>,
    /// The public key that produced the signature.
    signer: Option<Vec<u8>>,
}

impl ContentPack {
    /// Create an unsigned pack for the given content.
    ///
    /// The content hash is required: a pack with no content identity cannot be
    /// meaningfully signed or verified, so it is refused at construction rather
    /// than producing a pack that fails later.
    pub fn new(name: &str, content_hash: &str) -> Result<Self, SignatureError> {
        if content_hash.trim().is_empty() {
            return Err(SignatureError::EmptyContentHash);
        }
        Ok(Self {
            name: name.to_string(),
            version: 1,
            content_hash: content_hash.to_string(),
            // A new pack is not active until it has been reviewed and activated.
            status: ContentStatus::Quarantined,
            freshness_timestamp: Utc::now(),
            signature: None,
            signer: None,
        })
    }

    /// Create a pack from a signature that arrived with it.
    ///
    /// This is the receiving half of [`ContentPack::sign`]: a pack that travelled as
    /// a file carries its signature and signer, and installation has to be able to
    /// load them before it can check them. Constructing the pack does not verify it;
    /// [`ContentPack::verify`] does, and installation must call it before acting.
    ///
    /// The signature and signer are taken as bytes rather than trusted as text so
    /// that a caller decoding them from a file has to have decoded them successfully
    /// first. A malformed length is refused here rather than at verification time, so
    /// the error names the field that is wrong.
    pub fn from_signed(
        name: &str,
        version: u32,
        content_hash: &str,
        signature: Vec<u8>,
        signer: Vec<u8>,
    ) -> Result<Self, SignatureError> {
        if content_hash.trim().is_empty() {
            return Err(SignatureError::EmptyContentHash);
        }
        if signature.len() != 64 {
            return Err(SignatureError::MalformedSignature);
        }
        if signer.len() != 32 {
            return Err(SignatureError::MalformedKey);
        }
        Ok(Self {
            name: name.to_string(),
            version,
            content_hash: content_hash.to_string(),
            // A pack that has just been received is not serving anyone yet.
            status: ContentStatus::Quarantined,
            freshness_timestamp: Utc::now(),
            signature: Some(signature),
            signer: Some(signer),
        })
    }

    /// Move an active pack back to quarantine for review.
    ///
    /// Does not invalidate the signature: quarantine is a lifecycle decision,
    /// and the attested content has not changed.
    pub fn stage_update(&mut self) {
        self.status = ContentStatus::Quarantined;
        self.freshness_timestamp = Utc::now();
    }

    /// Return a quarantined pack to active service.
    pub fn rollback(&mut self) {
        self.status = ContentStatus::Active;
    }

    /// Activate the pack.
    pub fn activate(&mut self) {
        self.status = ContentStatus::Active;
    }

    pub fn status(&self) -> ContentStatus {
        self.status
    }

    pub fn freshness(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.freshness_timestamp)
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    pub fn signature(&self) -> Option<&[u8]> {
        self.signature.as_deref()
    }

    /// The public key that produced the signature, when the pack carries one.
    ///
    /// Symmetric with [`ContentPack::signature`]: a caller storing a signed pack has
    /// to record which key attested it, or the signature cannot be checked later.
    pub fn signer(&self) -> Option<&[u8]> {
        self.signer.as_deref()
    }

    pub fn is_signed(&self) -> bool {
        self.signature.is_some() && self.signer.is_some()
    }

    /// Change the pack version, invalidating any existing signature.
    ///
    /// A new version is a new content identity, so the previous attestation no
    /// longer applies. Clearing the signature makes that explicit rather than
    /// leaving one in place that verifies against stale content.
    pub fn set_version(&mut self, version: u32) {
        self.version = version;
        self.signature = None;
        self.signer = None;
    }

    /// The canonical bytes covered by the signature.
    ///
    /// Every field is length-prefixed so concatenation cannot be ambiguous:
    /// without it, name `"ab"` at version 1 would produce the same payload as
    /// name `"a"` plus a version whose leading byte is `b`, letting a signature
    /// be transplanted between distinct packs.
    fn signing_payload(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(SIGNING_DOMAIN.len() as u64).to_le_bytes());
        out.extend_from_slice(SIGNING_DOMAIN);

        let version_bytes = self.version.to_le_bytes();
        for field in [
            self.name.as_bytes(),
            version_bytes.as_slice(),
            self.content_hash.as_bytes(),
        ] {
            out.extend_from_slice(&(field.len() as u64).to_le_bytes());
            out.extend_from_slice(field);
        }
        out
    }

    /// Sign the pack's content identity with an Ed25519 private key.
    ///
    /// Returns a new signed pack; the receiver is unchanged, so signing cannot
    /// silently mutate a pack another holder is using.
    pub fn sign(&self, key: &SigningKey) -> Result<Self, SignatureError> {
        if self.content_hash.trim().is_empty() {
            return Err(SignatureError::EmptyContentHash);
        }
        let signature = key.sign(&self.signing_payload());

        let mut signed = self.clone();
        signed.signature = Some(signature.to_bytes().to_vec());
        signed.signer = Some(key.verifying_key().to_bytes().to_vec());
        Ok(signed)
    }

    /// Verify the pack's signature against the signer key it carries.
    ///
    /// This is the check installation performs before atomic activation. The
    /// payload is rebuilt from the pack's *current* fields, so any change to
    /// name, version or content hash causes verification to fail.
    pub fn verify(&self) -> Result<(), SignatureError> {
        let signature_bytes = self.signature.as_ref().ok_or(SignatureError::NotSigned)?;
        let signer_bytes = self.signer.as_ref().ok_or(SignatureError::NoSigner)?;

        let signature = Signature::from_slice(signature_bytes)
            .map_err(|_| SignatureError::MalformedSignature)?;

        let key_bytes: [u8; 32] = signer_bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::MalformedKey)?;
        let verifying_key =
            VerifyingKey::from_bytes(&key_bytes).map_err(|_| SignatureError::MalformedKey)?;

        verifying_key
            .verify(&self.signing_payload(), &signature)
            .map_err(|_| SignatureError::VerificationFailed)
    }
}

/// Test-only field access.
///
/// Tamper detection can only be proven by producing a pack whose stored fields
/// disagree with its signature — that is, by simulating corruption or an
/// adversary editing the record at rest. Doing that through the public API
/// would require adding setters whose only purpose is to create invalid state,
/// which would invite real callers to misuse them. These hooks are compiled
/// only under `cfg(test)`, so production code cannot reach them.
#[cfg(test)]
impl ContentPack {
    pub(crate) fn set_signature_for_test(&mut self, signature: Option<Vec<u8>>) {
        self.signature = signature;
    }

    pub(crate) fn set_signer_for_test(&mut self, signer: Option<Vec<u8>>) {
        self.signer = signer;
    }

    pub(crate) fn set_content_hash_for_test(&mut self, content_hash: &str) {
        self.content_hash = content_hash.to_string();
    }

    pub(crate) fn set_name_for_test(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// The recorded signer key, so a transplant test can carry it across.
    pub(crate) fn signer_for_test(&self) -> Option<Vec<u8>> {
        self.signer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_requires_a_content_hash() {
        assert_eq!(
            ContentPack::new("pack", "  ").unwrap_err(),
            SignatureError::EmptyContentHash
        );
    }
}
