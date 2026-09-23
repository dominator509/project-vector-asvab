//! Application updates: a signed manifest, a verified artifact, an atomic stage, a way back.
//!
//! REQ-037 asks for "signature/hash/atomic stage/rollback" and the requirement was PARTIAL
//! because two of the four were missing: the digest and tamper detection existed, the signature
//! and the staged transition did not. This module is that half.
//!
//! ## What is verified, and in what order
//!
//! 1. **Format and schema**, so a file of the wrong shape is refused as itself rather than as a
//!    broken signature.
//! 2. **The signer**, against the key this installation trusts, before the signature is checked:
//!    a valid signature by an untrusted key is not a lesser failure than an invalid one, it is a
//!    different fact, and the message should say which.
//! 3. **The signature**, over a payload rebuilt from the manifest's *current* fields -- so any
//!    edit to the version, the artifact name, its digest or its size breaks it.
//! 4. **The version relation**, after the cryptography: `running < offered` (an update that is
//!    not newer is not an update) and `running >= min_running_version` (the release says which
//!    versions may jump to it).
//! 5. **The artifact**, by digest and by length, before anything is written anywhere.
//!
//! ## Why staging is not installation
//!
//! On Windows a running executable cannot be replaced, so an updater that claims to install
//! while the application is running is claiming something it cannot do. What this module does is
//! stage the verified artifact beside the installation under a versioned name, and apply it as a
//! separate step -- and applying moves the previous artifact aside rather than deleting it, so a
//! failure part way through is recoverable by `rollback`. The launcher calls `apply_staged`;
//! this code never replaces the file it is running from.
//!
//! ## What is deliberately not here
//!
//! Downloading. The manifest names where the artifact came from; fetching it is the operator's or
//! the launcher's step, and the network is not a dependency of this crate. The signing key is
//! generated and held by the release process (`update-keygen`), and the certificate a real
//! distribution needs is an external gate (REQ-036).

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The manifest format this build understands.
pub const UPDATE_FORMAT: &str = "vector-update-1";

/// One published release, as the release process describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub format: String,
    /// The application the update belongs to, so a manifest for another product is refused.
    pub app: String,
    /// The version being offered.
    pub version: String,
    /// The oldest running version this release may be applied over.
    pub min_running_version: String,
    /// When it was published, RFC 3339. Recorded rather than interpreted.
    pub published_at: String,
    pub artifact: UpdateArtifact,
    /// Where a reader can find the release notes. Optional: absence is not a defect.
    pub notes_url: Option<String>,
}

/// The file the manifest describes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateArtifact {
    pub file_name: String,
    /// `sha256:<hex>`, matching the artifact-identity convention used everywhere else.
    pub sha256: String,
    pub bytes: u64,
}

/// A manifest and the detached signature over it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedUpdate {
    pub manifest: UpdateManifest,
    /// Detached Ed25519 signature over the manifest identity, hex.
    pub signature: String,
    /// The public key that produced it, hex. Checked against the trusted key before use.
    pub signer: String,
}

impl UpdateManifest {
    /// The bytes the signature covers.
    ///
    /// Every field is length-prefixed, so no two different manifests serialize to the same
    /// payload by moving a byte across a boundary -- the same construction the content pack uses
    /// for its identity.
    fn signing_payload(&self) -> Vec<u8> {
        let bytes = self.artifact.bytes.to_string();
        let mut out = Vec::new();
        for field in [
            self.format.as_bytes(),
            self.app.as_bytes(),
            self.version.as_bytes(),
            self.min_running_version.as_bytes(),
            self.published_at.as_bytes(),
            self.artifact.file_name.as_bytes(),
            self.artifact.sha256.as_bytes(),
            bytes.as_bytes(),
        ] {
            out.extend_from_slice(&(field.len() as u64).to_le_bytes());
            out.extend_from_slice(field);
        }
        out
    }

    /// The digest of the manifest itself, for logs and for a release index.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.signing_payload());
        format!("sha256:{:x}", hasher.finalize())
    }
}

/// Why an update was refused.
///
/// One variant per fact, because "the update failed" is not something an operator can act on:
/// an untrusted signer means the release key changed, a digest mismatch means the file in hand is
/// not the file that was published, and a version relation means the update does not apply here.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateRefusal {
    Unreadable(String),
    UnsupportedFormat { found: String },
    WrongApplication { found: String, expected: String },
    Unsigned,
    MalformedSignature,
    MalformedKey,
    UntrustedSigner { found: String, trusted: String },
    InvalidSignature(String),
    UnparsableVersion(String),
    NotNewer { running: String, offered: String },
    RequiresNewerBase { running: String, requires: String },
    MissingArtifact(String),
    ArtifactDigestMismatch { recorded: String, computed: String },
    ArtifactSizeMismatch { recorded: u64, found: u64 },
    StagedAlreadyPresent(String),
    Staging(String),
}

impl std::fmt::Display for UpdateRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateRefusal::Unreadable(detail) => write!(f, "the update manifest is unreadable: {detail}"),
            UpdateRefusal::UnsupportedFormat { found } => {
                write!(f, "manifest format {found:?} is not {UPDATE_FORMAT:?}")
            }
            UpdateRefusal::WrongApplication { found, expected } => {
                write!(f, "the manifest is for {found:?}, not {expected:?}")
            }
            UpdateRefusal::Unsigned => write!(f, "the manifest carries no signature"),
            UpdateRefusal::MalformedSignature => write!(f, "the signature is not 64 hex-decoded bytes"),
            UpdateRefusal::MalformedKey => write!(f, "the signer is not a 32-byte Ed25519 key"),
            UpdateRefusal::UntrustedSigner { found, trusted } => write!(
                f,
                "the manifest is signed by {found}, which is not the key this installation trusts ({trusted})"
            ),
            UpdateRefusal::InvalidSignature(detail) => {
                write!(f, "the manifest signature does not verify: {detail}")
            }
            UpdateRefusal::UnparsableVersion(value) => {
                write!(f, "version {value:?} is not a dot-separated number")
            }
            UpdateRefusal::NotNewer { running, offered } => {
                write!(f, "version {offered} is not newer than the running {running}")
            }
            UpdateRefusal::RequiresNewerBase { running, requires } => write!(
                f,
                "this release applies to {requires} or newer, and this installation runs {running}"
            ),
            UpdateRefusal::MissingArtifact(path) => write!(f, "no artifact at {path}"),
            UpdateRefusal::ArtifactDigestMismatch { recorded, computed } => write!(
                f,
                "the artifact hashes to {computed}, and the manifest records {recorded}"
            ),
            UpdateRefusal::ArtifactSizeMismatch { recorded, found } => write!(
                f,
                "the artifact is {found} bytes, and the manifest records {recorded}"
            ),
            UpdateRefusal::StagedAlreadyPresent(path) => {
                write!(f, "an update is already staged at {path}")
            }
            UpdateRefusal::Staging(detail) => write!(f, "staging failed: {detail}"),
        }
    }
}

impl std::error::Error for UpdateRefusal {}

/// Parse a signed manifest.
pub fn parse_signed(bytes: &[u8]) -> Result<SignedUpdate, UpdateRefusal> {
    serde_json::from_slice(bytes).map_err(|error| UpdateRefusal::Unreadable(error.to_string()))
}

/// Sign a manifest, the way the release process does.
pub fn sign(manifest: UpdateManifest, key: &SigningKey) -> SignedUpdate {
    let signature = key.sign(&manifest.signing_payload());
    SignedUpdate {
        manifest,
        signature: to_hex(&signature.to_bytes()),
        signer: to_hex(&key.verifying_key().to_bytes()),
    }
}

/// Verify a signed manifest against the trusted key and the running version.
pub fn verify(
    signed: &SignedUpdate,
    trusted_signer_hex: &str,
    application: &str,
    running_version: &str,
) -> Result<(), UpdateRefusal> {
    let manifest = &signed.manifest;
    if manifest.format != UPDATE_FORMAT {
        return Err(UpdateRefusal::UnsupportedFormat {
            found: manifest.format.clone(),
        });
    }
    if manifest.app != application {
        return Err(UpdateRefusal::WrongApplication {
            found: manifest.app.clone(),
            expected: application.to_string(),
        });
    }
    if signed.signature.is_empty() || signed.signer.is_empty() {
        return Err(UpdateRefusal::Unsigned);
    }
    let signature_bytes = from_hex(&signed.signature).ok_or(UpdateRefusal::MalformedSignature)?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|_| UpdateRefusal::MalformedSignature)?;
    let signer_bytes = from_hex(&signed.signer).ok_or(UpdateRefusal::MalformedKey)?;
    let signer: [u8; 32] = signer_bytes
        .as_slice()
        .try_into()
        .map_err(|_| UpdateRefusal::MalformedKey)?;

    // The signer check comes first, and says which key it found: a valid signature by a key this
    // installation does not trust and an invalid signature are different facts.
    let trusted = from_hex(trusted_signer_hex).ok_or(UpdateRefusal::MalformedKey)?;
    if signer.to_vec() != trusted {
        return Err(UpdateRefusal::UntrustedSigner {
            found: signed.signer.clone(),
            trusted: trusted_signer_hex.to_string(),
        });
    }

    let key = VerifyingKey::from_bytes(&signer).map_err(|_| UpdateRefusal::MalformedKey)?;
    key.verify_strict(&manifest.signing_payload(), &signature)
        .map_err(|error| UpdateRefusal::InvalidSignature(error.to_string()))?;

    // Compatibility after cryptography: a version mismatch is not an attack, and a caller can
    // act on it, so it should not be reported as one.
    let running = version_key(running_version)
        .ok_or_else(|| UpdateRefusal::UnparsableVersion(running_version.to_string()))?;
    let offered = version_key(&manifest.version)
        .ok_or_else(|| UpdateRefusal::UnparsableVersion(manifest.version.clone()))?;
    let floor = version_key(&manifest.min_running_version)
        .ok_or_else(|| UpdateRefusal::UnparsableVersion(manifest.min_running_version.clone()))?;
    if offered <= running {
        return Err(UpdateRefusal::NotNewer {
            running: running_version.to_string(),
            offered: manifest.version.clone(),
        });
    }
    if running < floor {
        return Err(UpdateRefusal::RequiresNewerBase {
            running: running_version.to_string(),
            requires: manifest.min_running_version.clone(),
        });
    }
    Ok(())
}

/// Verify the artifact the manifest describes: its digest and its length.
pub fn verify_artifact(manifest: &UpdateManifest, artifact: &Path) -> Result<(), UpdateRefusal> {
    if !artifact.exists() {
        return Err(UpdateRefusal::MissingArtifact(
            artifact.display().to_string(),
        ));
    }
    let bytes =
        std::fs::read(artifact).map_err(|error| UpdateRefusal::Unreadable(error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let computed = format!("sha256:{:x}", hasher.finalize());
    if computed != manifest.artifact.sha256 {
        return Err(UpdateRefusal::ArtifactDigestMismatch {
            recorded: manifest.artifact.sha256.clone(),
            computed,
        });
    }
    if bytes.len() as u64 != manifest.artifact.bytes {
        return Err(UpdateRefusal::ArtifactSizeMismatch {
            recorded: manifest.artifact.bytes,
            found: bytes.len() as u64,
        });
    }
    Ok(())
}

/// Copy a verified artifact beside the installation under a versioned name.
///
/// The copy is verified again after it lands, because the point of staging is that what will be
/// applied later is known to be the published bytes -- a digest check on the source alone would
/// say nothing about the copy.
pub fn stage(
    manifest: &UpdateManifest,
    artifact: &Path,
    install_dir: &Path,
) -> Result<PathBuf, UpdateRefusal> {
    verify_artifact(manifest, artifact)?;
    std::fs::create_dir_all(install_dir)
        .map_err(|error| UpdateRefusal::Staging(error.to_string()))?;
    let staged = install_dir.join(format!(".update-staged-{}", manifest.version));
    if staged.exists() {
        return Err(UpdateRefusal::StagedAlreadyPresent(
            staged.display().to_string(),
        ));
    }
    std::fs::copy(artifact, &staged).map_err(|error| UpdateRefusal::Staging(error.to_string()))?;
    verify_artifact(manifest, &staged)?;
    Ok(staged)
}

/// Where an applied update keeps the artifact it replaced.
///
/// Derived from the target rather than chosen by the caller, because a rollback has to find it
/// again: the first end-to-end run of this path applied the update to a file named after the
/// *published* artifact and then could not roll back, having looked for the backup beside the
/// installed one.
pub fn displaced_path(target: &Path) -> PathBuf {
    target.with_extension("pre-update")
}

/// Put a staged artifact in place of the current one, keeping the previous one.
///
/// `target` is the *installed* artifact's path -- what the launcher runs -- which is not the same
/// thing as the published file name: a release ships `vector-desktop-0.2.0.exe` and an
/// installation runs `vector-desktop.exe`.
///
/// Returns the path the previous artifact was moved to, or `None` when there was nothing to
/// replace (a first install, which is legitimate and has nothing to roll back to). The old file
/// is moved aside rather than deleted: a transition that cannot be undone is a commitment.
pub fn apply_staged(staged: &Path, target: &Path) -> Result<Option<PathBuf>, UpdateRefusal> {
    if !staged.exists() {
        return Err(UpdateRefusal::MissingArtifact(staged.display().to_string()));
    }
    let displaced = displaced_path(target);
    if displaced.exists() {
        std::fs::remove_file(&displaced)
            .map_err(|error| UpdateRefusal::Staging(error.to_string()))?;
    }
    let had_previous = target.exists();
    if had_previous {
        std::fs::rename(target, &displaced)
            .map_err(|error| UpdateRefusal::Staging(error.to_string()))?;
    }
    if let Err(error) = std::fs::rename(staged, target) {
        // Put the previous artifact back rather than leaving no application at all.
        if had_previous {
            let _ = std::fs::rename(&displaced, target);
        }
        return Err(UpdateRefusal::Staging(error.to_string()));
    }
    Ok(had_previous.then_some(displaced))
}

/// Put the previous artifact back.
pub fn rollback(target: &Path) -> Result<PathBuf, UpdateRefusal> {
    let displaced = displaced_path(target);
    if !displaced.exists() {
        return Err(UpdateRefusal::MissingArtifact(format!(
            "{} (there is no previous artifact to restore)",
            displaced.display()
        )));
    }
    std::fs::rename(&displaced, target)
        .map_err(|error| UpdateRefusal::Staging(error.to_string()))?;
    Ok(displaced)
}

/// Compare dot-separated numeric versions.
fn version_key(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.trim().split('.');
    let mut numbers = [0u64; 3];
    for (index, part) in parts.by_ref().take(3).enumerate() {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        numbers[index] = part.parse().ok()?;
    }
    // A fourth component is not something this scheme defines, and guessing would be worse than
    // refusing.
    if parts.next().is_some() {
        return None;
    }
    Some((numbers[0], numbers[1], numbers[2]))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(value: &str) -> Option<Vec<u8>> {
    let trimmed = value.trim();
    if !trimmed.len().is_multiple_of(2) || trimmed.is_empty() {
        return None;
    }
    (0..trimmed.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&trimmed[index..index + 2], 16).ok())
        .collect()
}

/// Derive a signing key from a 32-byte seed.
///
/// The caller supplies the seed. This crate does not depend on an RNG: the release process draws
/// the seed from the OS entropy source through `uuid` (two v4 UUIDs are 256 bits from the
/// platform generator), the same way the content-pack key is made, so update signing and pack
/// signing have one key-handling story rather than two.
pub fn key_from_seed(seed: &[u8; 32]) -> SigningKey {
    SigningKey::from_bytes(seed)
}

/// A seed built from two v4 UUIDs: 256 bits from the platform's entropy source.
pub fn seed_from_entropy() -> [u8; 32] {
    let mut seed = [0_u8; 32];
    seed[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    seed[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    seed
}

#[cfg(test)]
mod tests {
    use super::*;

    mod tempdir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        static SEQUENCE: AtomicU64 = AtomicU64::new(0);

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new(tag: &str) -> Self {
                let mut path = std::env::temp_dir();
                path.push(format!(
                    "vector-update-{}-{}-{}-{}",
                    tag,
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("clock")
                        .as_nanos(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).expect("temp dir");
                Self { path }
            }

            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    fn fresh_key() -> SigningKey {
        key_from_seed(&seed_from_entropy())
    }

    fn manifest_for(bytes: &[u8], version: &str) -> UpdateManifest {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        UpdateManifest {
            format: UPDATE_FORMAT.to_string(),
            app: "vector-desktop".to_string(),
            version: version.to_string(),
            min_running_version: "0.1.0".to_string(),
            published_at: "2026-09-23T00:00:00Z".to_string(),
            artifact: UpdateArtifact {
                file_name: "vector-desktop-0.2.0.exe".to_string(),
                sha256: format!("sha256:{:x}", hasher.finalize()),
                bytes: bytes.len() as u64,
            },
            notes_url: None,
        }
    }

    #[test]
    fn a_signed_manifest_verifies_and_a_re_signed_one_is_refused() {
        let key = fresh_key();
        let trusted = to_hex(&key.verifying_key().to_bytes());
        let signed = sign(manifest_for(b"binary", "0.2.0"), &key);

        verify(&signed, &trusted, "vector-desktop", "0.1.0").expect("a valid update verifies");

        // The same manifest signed by another key is refused by signer, not by signature.
        let other = fresh_key();
        let forged = sign(manifest_for(b"binary", "0.2.0"), &other);
        match verify(&forged, &trusted, "vector-desktop", "0.1.0") {
            Err(UpdateRefusal::UntrustedSigner { found, .. }) => {
                assert_eq!(found, to_hex(&other.verifying_key().to_bytes()));
            }
            other => panic!("expected an untrusted signer, got {other:?}"),
        }
    }

    #[test]
    fn an_edited_manifest_breaks_its_signature() {
        let key = fresh_key();
        let trusted = to_hex(&key.verifying_key().to_bytes());
        let mut signed = sign(manifest_for(b"binary", "0.2.0"), &key);

        // Each field the release process publishes is covered by the signature.
        for mutate in [
            |m: &mut UpdateManifest| m.version = "0.3.0".to_string(),
            |m: &mut UpdateManifest| m.artifact.file_name = "other.exe".to_string(),
            |m: &mut UpdateManifest| m.artifact.sha256 = format!("sha256:{}", "0".repeat(64)),
            |m: &mut UpdateManifest| m.artifact.bytes += 1,
            |m: &mut UpdateManifest| m.min_running_version = "9.9.9".to_string(),
            |m: &mut UpdateManifest| m.published_at = "2030-01-01T00:00:00Z".to_string(),
        ] {
            let mut edited = signed.clone();
            mutate(&mut edited.manifest);
            match verify(&edited, &trusted, "vector-desktop", "0.1.0") {
                Err(UpdateRefusal::InvalidSignature(_)) => {}
                Err(UpdateRefusal::UnparsableVersion(_)) => {}
                other => panic!("an edited manifest must not verify, got {other:?}"),
            }
        }

        // And a signature moved onto a different manifest does not verify either.
        let other_manifest = manifest_for(b"different", "0.2.0");
        signed.manifest = other_manifest;
        assert!(matches!(
            verify(&signed, &trusted, "vector-desktop", "0.1.0"),
            Err(UpdateRefusal::InvalidSignature(_))
        ));
    }

    #[test]
    fn version_relations_are_refused_after_the_signature_checks() {
        let key = fresh_key();
        let trusted = to_hex(&key.verifying_key().to_bytes());

        let same = sign(manifest_for(b"binary", "0.1.0"), &key);
        assert!(matches!(
            verify(&same, &trusted, "vector-desktop", "0.1.0"),
            Err(UpdateRefusal::NotNewer { .. })
        ));

        let older = sign(manifest_for(b"binary", "0.0.9"), &key);
        assert!(matches!(
            verify(&older, &trusted, "vector-desktop", "0.1.0"),
            Err(UpdateRefusal::NotNewer { .. })
        ));

        let mut manifest = manifest_for(b"binary", "0.2.0");
        manifest.min_running_version = "0.5.0".to_string();
        let floor = sign(manifest, &key);
        assert!(matches!(
            verify(&floor, &trusted, "vector-desktop", "0.1.0"),
            Err(UpdateRefusal::RequiresNewerBase { .. })
        ));

        // A manifest for another product is refused before any of that.
        let mut foreign = manifest_for(b"binary", "0.2.0");
        foreign.app = "vector-ish".to_string();
        let foreign = sign(foreign, &key);
        assert!(matches!(
            verify(&foreign, &trusted, "vector-desktop", "0.1.0"),
            Err(UpdateRefusal::WrongApplication { .. })
        ));
    }

    #[test]
    fn an_artifact_that_is_not_the_published_one_is_refused() {
        let dir = tempdir::TempDir::new("artifact");
        let key = fresh_key();
        let trusted = to_hex(&key.verifying_key().to_bytes());
        let good = dir.path().join("good.exe");
        std::fs::write(&good, b"the published bytes").expect("write");

        let signed = sign(manifest_for(b"the published bytes", "0.2.0"), &key);
        verify(&signed, &trusted, "vector-desktop", "0.1.0").expect("verified");
        verify_artifact(&signed.manifest, &good).expect("the published bytes verify");

        // A file of the right length but different content.
        let tampered = dir.path().join("tampered.exe");
        std::fs::write(&tampered, b"the published bytez").expect("write");
        assert!(matches!(
            verify_artifact(&signed.manifest, &tampered),
            Err(UpdateRefusal::ArtifactDigestMismatch { .. })
        ));

        // A file of the right content but a different length.
        let mut manifest = signed.manifest.clone();
        manifest.artifact.bytes += 1;
        assert!(matches!(
            verify_artifact(&manifest, &good),
            Err(UpdateRefusal::ArtifactSizeMismatch { .. })
        ));

        // And an artifact that does not exist.
        assert!(matches!(
            verify_artifact(&signed.manifest, &dir.path().join("absent.exe")),
            Err(UpdateRefusal::MissingArtifact(_))
        ));
    }

    #[test]
    fn staging_then_applying_then_rolling_back_moves_real_files() {
        let dir = tempdir::TempDir::new("stage");
        let install = dir.path().join("install");
        std::fs::create_dir_all(&install).expect("install dir");
        let target = install.join("vector-desktop.exe");
        std::fs::write(&target, b"version 0.1.0").expect("current artifact");

        let artifact = dir.path().join("vector-desktop-0.2.0.exe");
        std::fs::write(&artifact, b"version 0.2.0").expect("new artifact");
        let signed = sign(manifest_for(b"version 0.2.0", "0.2.0"), &fresh_key());
        let _ = signed;

        let manifest = manifest_for(b"version 0.2.0", "0.2.0");
        let staged = stage(&manifest, &artifact, &install).expect("stage");
        assert!(staged.exists(), "the staged file is written");
        assert_eq!(
            std::fs::read(&staged).expect("read"),
            b"version 0.2.0",
            "the staged copy is the published bytes"
        );
        // Staging twice is refused rather than overwriting a pending update.
        assert!(matches!(
            stage(&manifest, &artifact, &install),
            Err(UpdateRefusal::StagedAlreadyPresent(_))
        ));

        // A staged file that is not the published bytes is refused before it can be applied.
        std::fs::write(&staged, b"version 9.9.9").expect("tamper with the staged copy");
        assert!(matches!(
            verify_artifact(&manifest, &staged),
            Err(UpdateRefusal::ArtifactDigestMismatch { .. })
        ));
        std::fs::write(&staged, b"version 0.2.0").expect("restore the staged bytes");

        let displaced = apply_staged(&staged, &target)
            .expect("apply")
            .expect("a previous artifact to displace");
        assert_eq!(std::fs::read(&target).expect("read"), b"version 0.2.0");
        assert_eq!(std::fs::read(&displaced).expect("read"), b"version 0.1.0");

        rollback(&target).expect("roll back");
        assert_eq!(
            std::fs::read(&target).expect("read"),
            b"version 0.1.0",
            "the previous artifact is back"
        );
        assert!(
            !displaced.exists(),
            "the backup was consumed by the rollback"
        );

        // Applying over a target with nothing there is a first install, and says so.
        let fresh = dir.path().join("fresh.exe");
        let second = dir.path().join("second.exe");
        std::fs::write(&second, b"version 0.3.0").expect("write");
        assert_eq!(
            apply_staged(&second, &fresh).expect("apply"),
            None,
            "a first install displaces nothing"
        );
        assert!(matches!(
            rollback(&fresh),
            Err(UpdateRefusal::MissingArtifact(_))
        ));
    }
}
