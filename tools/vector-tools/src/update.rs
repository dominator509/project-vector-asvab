//! The release process's side of an application update: build a manifest, sign it, verify it,
//! stage it, apply it, roll it back.
//!
//! REQ-037 asks for "signature/hash/atomic stage/rollback" and the verification lives in
//! `vector_platform::update`. This is the operator-facing half: the commands a release engineer
//! runs to publish a release, and the commands an installation runs to take one -- so the path
//! can be exercised end to end against real files rather than only in unit tests.
//!
//! Nothing here downloads anything. The manifest names the artifact's digest and size, and
//! fetching the bytes is the operator's step; the verification is what this side owns.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use vector_platform::update::{
    self, seed_from_entropy, SignedUpdate, UpdateArtifact, UpdateManifest, UPDATE_FORMAT,
};

/// A manifest for one artifact, with its digest and size filled in from the file itself.
pub fn manifest_for_artifact(
    artifact: &Path,
    app: &str,
    version: &str,
    min_running_version: &str,
    notes_url: Option<String>,
) -> Result<UpdateManifest> {
    let bytes = std::fs::read(artifact)
        .with_context(|| format!("cannot read the artifact at {}", artifact.display()))?;
    let file_name = artifact
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", artifact.display()))?;
    // Reuse the update module's own digest, so the manifest and the verification cannot disagree
    // about what they are hashing.
    let placeholder = UpdateManifest {
        format: UPDATE_FORMAT.to_string(),
        app: app.to_string(),
        version: version.to_string(),
        min_running_version: min_running_version.to_string(),
        published_at: chrono::Utc::now().to_rfc3339(),
        artifact: UpdateArtifact {
            file_name,
            sha256: String::new(),
            bytes: bytes.len() as u64,
        },
        notes_url,
    };
    let digest = manifest_digest_of(&bytes);
    Ok(UpdateManifest {
        artifact: UpdateArtifact {
            sha256: digest,
            ..placeholder.artifact
        },
        ..placeholder
    })
}

fn manifest_digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// Write a new update signing key, refusing to overwrite one that exists.
pub fn generate_update_key(path: &Path) -> Result<String> {
    if path.exists() {
        anyhow::bail!(
            "{} already exists; refusing to overwrite a signing key",
            path.display()
        );
    }
    let key = update::key_from_seed(&seed_from_entropy());
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, encode_hex(&key.to_bytes()))?;
    Ok(encode_hex(&key.verifying_key().to_bytes()))
}

/// Sign a manifest with the update key, writing the signed document.
pub fn sign_manifest(manifest_path: &Path, key_path: &Path, out: &Path) -> Result<SignedUpdate> {
    let manifest: UpdateManifest = serde_json::from_slice(
        &std::fs::read(manifest_path)
            .with_context(|| format!("cannot read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("cannot parse a manifest from {}", manifest_path.display()))?;
    let key = read_update_key(key_path)?;
    let signed = update::sign(manifest, &key);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, serde_json::to_vec_pretty(&signed)?)?;
    Ok(signed)
}

/// Verify a signed manifest and the artifact it describes.
pub fn verify_update(
    signed_path: &Path,
    artifact: &Path,
    trusted_signer: &str,
    app: &str,
    running_version: &str,
) -> Result<UpdateManifest> {
    let signed = read_signed(signed_path)?;
    update::verify(&signed, trusted_signer, app, running_version)
        .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    update::verify_artifact(&signed.manifest, artifact)
        .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    Ok(signed.manifest)
}

/// Verify, stage, and apply an update over the installed artifact.
///
/// `target` is what the installation runs, which is not the published file name: a release ships
/// `vector-desktop-0.2.0.exe` and an installation runs `vector-desktop.exe`. The staged copy keeps
/// the version in its name; the installed file keeps the name the launcher uses.
///
/// Returns (staged path, displaced path or None on a first install). Applying moves the current
/// artifact aside rather than deleting it, so `rollback_update` can put it back.
pub fn stage_update(
    signed_path: &Path,
    artifact: &Path,
    trusted_signer: &str,
    app: &str,
    running_version: &str,
    install_dir: &Path,
    target: &Path,
) -> Result<(PathBuf, Option<PathBuf>)> {
    let manifest = verify_update(signed_path, artifact, trusted_signer, app, running_version)?;
    let staged = update::stage(&manifest, artifact, install_dir)
        .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    let displaced =
        update::apply_staged(&staged, target).map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    Ok((staged, displaced))
}

/// Put the previous artifact back.
pub fn rollback_update(target: &Path) -> Result<PathBuf> {
    update::rollback(target).map_err(|refusal| anyhow::anyhow!("{refusal}"))
}

fn read_signed(path: &Path) -> Result<SignedUpdate> {
    update::parse_signed(
        &std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
    )
    .map_err(|refusal| anyhow::anyhow!("{refusal}"))
}

fn read_update_key(path: &Path) -> Result<ed25519_dalek::SigningKey> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read the key at {}", path.display()))?;
    let bytes =
        decode_hex(text.trim()).ok_or_else(|| anyhow::anyhow!("{} is not hex", path.display()))?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("{} is not 32 bytes of hex", path.display()))?;
    Ok(update::key_from_seed(&seed))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).ok())
        .collect()
}
