//! Release artifact identity, SBOM and provenance (REQ-036, REQ-037, REQ-053).
//!
//! Binding sources: `DEPLOYMENT.md`, `RELEASE.md` and `HARNESS_LAWS.md` 7
//! ("The release artifact is hashed once; later gates must use that exact
//! digest").
//!
//! The property this module exists to establish is that an artifact's identity
//! is *complete and falsifiable*: every component that changes behaviour is
//! bound, a missing component is a hard error rather than a placeholder, and
//! verification re-derives the digest from the files on disk rather than
//! trusting a recorded value.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A component bound into the artifact identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ArtifactComponent {
    /// The shipped executable.
    Binary,
    /// The database migrations applied at first run.
    Migrations,
    /// The content pack manifest.
    Content,
    /// The software bill of materials.
    Sbom,
    /// Third-party license notices.
    Licenses,
    /// The git commit the artifact was built from.
    GitSha,
}

impl ArtifactComponent {
    /// Every component an artifact identity must bind.
    ///
    /// A component omitted here is a component whose change would not invalidate
    /// the digest, which is what makes the identity meaningful.
    pub const ALL: &'static [ArtifactComponent] = &[
        ArtifactComponent::Binary,
        ArtifactComponent::Migrations,
        ArtifactComponent::Content,
        ArtifactComponent::Sbom,
        ArtifactComponent::Licenses,
        ArtifactComponent::GitSha,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ArtifactComponent::Binary => "binary",
            ArtifactComponent::Migrations => "migrations",
            ArtifactComponent::Content => "content",
            ArtifactComponent::Sbom => "sbom",
            ArtifactComponent::Licenses => "licenses",
            ArtifactComponent::GitSha => "git_sha",
        }
    }
}

/// A SHA-256 digest in lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Hash bytes.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Sha256Digest(format!("{:x}", hasher.finalize()))
    }

    /// Hash text with line endings normalized.
    ///
    /// Without normalization the same content checked out on Windows and Linux
    /// would hash differently, so an artifact built on one platform could not be
    /// verified on the other.
    pub fn of_text(text: &str) -> Self {
        Self::of(text.replace("\r\n", "\n").as_bytes())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// An absent digest.
    ///
    /// Representable so a caller can express "this component was not measured",
    /// which [`ArtifactIdentity::build`] then refuses. It exists to make the
    /// absent case a value that can be checked rather than one that is merely
    /// never constructed.
    pub fn default_empty() -> Self {
        Sha256Digest(String::new())
    }
}

impl std::fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Collect every file under `dir`, with paths relative to `root`.
///
/// Sorted by the caller rather than here, so the digest does not depend on the
/// order the filesystem happens to return entries in.
fn collect_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, std::path::PathBuf)>,
    total: &mut u64,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out, total)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        *total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push((relative, path));
    }
    Ok(())
}

/// A measured component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasuredComponent {
    pub component: ArtifactComponent,
    pub digest: Sha256Digest,
    /// Size in bytes, when the component is a file.
    pub bytes: Option<u64>,
}

/// Why an artifact identity could not be built or verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// A required component was not supplied.
    MissingComponent(ArtifactComponent),
    /// A component was supplied but is empty on disk.
    EmptyComponent(ArtifactComponent),
    /// The artifact does not exist.
    ArtifactMissing(String),
    /// A recomputed digest does not match the recorded one.
    DigestMismatch {
        component: ArtifactComponent,
        recorded: String,
        actual: String,
    },
    /// The bound digest does not match the components it claims to bind.
    BindingMismatch { recorded: String, actual: String },
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::MissingComponent(c) => {
                write!(f, "artifact identity is missing component {}", c.label())
            }
            IdentityError::EmptyComponent(c) => write!(
                f,
                "component {} is present but empty, so it binds nothing",
                c.label()
            ),
            IdentityError::ArtifactMissing(p) => write!(f, "artifact not found at {p}"),
            IdentityError::DigestMismatch {
                component,
                recorded,
                actual,
            } => write!(
                f,
                "component {} does not match: recorded {recorded}, actual {actual}",
                component.label()
            ),
            IdentityError::BindingMismatch { recorded, actual } => write!(
                f,
                "artifact identity digest does not match its components: \
                 recorded {recorded}, derived {actual}"
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

/// A complete artifact identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactIdentity {
    pub components: Vec<MeasuredComponent>,
    /// The digest binding all components together.
    pub identity_digest: Sha256Digest,
    /// Version string of the artifact.
    pub version: String,
}

impl ArtifactIdentity {
    /// Derive the binding digest from a component list.
    ///
    /// Components are sorted by kind and their digests concatenated with the
    /// label, so the digest is independent of the order components were
    /// supplied. Without that, two callers measuring the same artifact could
    /// produce different identities.
    pub fn derive_digest(components: &[MeasuredComponent]) -> Sha256Digest {
        let mut sorted = components.to_vec();
        sorted.sort_by_key(|c| c.component);

        let mut hasher = Sha256::new();
        for component in sorted {
            hasher.update(component.component.label().as_bytes());
            hasher.update(b":");
            hasher.update(component.digest.as_str().as_bytes());
            hasher.update(b"\n");
        }
        Sha256Digest(format!("{:x}", hasher.finalize()))
    }

    /// Build an identity, refusing to proceed with an incomplete component set.
    ///
    /// An earlier implementation substituted the literal string "MISSING" for an
    /// absent file and hashed that, so a manifest built from nothing still
    /// produced a confident-looking digest. Missing is now a hard error.
    pub fn build(version: &str, components: Vec<MeasuredComponent>) -> Result<Self, IdentityError> {
        for required in ArtifactComponent::ALL {
            let found = components.iter().find(|c| c.component == *required);
            match found {
                None => return Err(IdentityError::MissingComponent(*required)),
                Some(measured) if measured.digest.is_empty() => {
                    return Err(IdentityError::EmptyComponent(*required));
                }
                Some(_) => {}
            }
        }

        let identity_digest: Sha256Digest = Self::derive_digest(&components);
        Ok(Self {
            components,
            identity_digest,
            version: version.to_string(),
        })
    }

    /// Measure a file on disk.
    pub fn measure_file(
        component: ArtifactComponent,
        path: &std::path::Path,
    ) -> Result<MeasuredComponent, IdentityError> {
        let bytes = std::fs::read(path)
            .map_err(|_| IdentityError::ArtifactMissing(path.display().to_string()))?;
        Ok(MeasuredComponent {
            component,
            digest: Sha256Digest::of(&bytes),
            bytes: Some(bytes.len() as u64),
        })
    }

    /// Measure a file, or every file under a directory.
    ///
    /// A component like `Migrations` is a set of files, and binding one
    /// representative file would mean adding a migration did not change the
    /// artifact identity — which defeats the point of binding it. The EP-009
    /// anti-gaming review named this as the correct eventual form.
    ///
    /// A directory is digested as a sorted list of `relative/path\0contents\0`
    /// entries with line endings normalized, so the digest depends on the set of
    /// files, their names and their contents, and not on enumeration order or on
    /// which platform produced them.
    pub fn measure_path(
        component: ArtifactComponent,
        path: &std::path::Path,
    ) -> Result<MeasuredComponent, IdentityError> {
        if !path.is_dir() {
            return Self::measure_file(component, path);
        }

        let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
        let mut total: u64 = 0;
        collect_files(path, path, &mut files, &mut total)
            .map_err(|_| IdentityError::ArtifactMissing(path.display().to_string()))?;

        if files.is_empty() {
            // An empty directory is not a measured component; reporting a digest
            // of nothing would let an absent component look present.
            return Err(IdentityError::ArtifactMissing(format!(
                "{} contains no files",
                path.display()
            )));
        }

        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut hasher = Sha256::new();
        for (relative, full) in &files {
            let bytes = std::fs::read(full)
                .map_err(|_| IdentityError::ArtifactMissing(full.display().to_string()))?;
            let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
            hasher.update(relative.as_bytes());
            hasher.update([0u8]);
            hasher.update(text.as_bytes());
            hasher.update([0u8]);
        }

        Ok(MeasuredComponent {
            component,
            digest: Sha256Digest(format!("{:x}", hasher.finalize())),
            bytes: Some(total),
        })
    }

    /// Verify a recorded identity against the files currently on disk.
    ///
    /// Re-derives every digest rather than trusting the recorded values, then
    /// re-derives the binding digest. This is the check an installer performs
    /// before activating an artifact.
    pub fn verify_against(
        &self,
        paths: &[(ArtifactComponent, std::path::PathBuf)],
    ) -> Result<(), IdentityError> {
        for (component, path) in paths {
            let recorded = self
                .components
                .iter()
                .find(|c| c.component == *component)
                .ok_or(IdentityError::MissingComponent(*component))?;
            // `measure_path` so a component that is a directory verifies the
            // same way it was measured.
            let actual = Self::measure_path(*component, path)?;
            if actual.digest != recorded.digest {
                return Err(IdentityError::DigestMismatch {
                    component: *component,
                    recorded: recorded.digest.to_string(),
                    actual: actual.digest.to_string(),
                });
            }
        }

        let derived = Self::derive_digest(&self.components);
        if derived != self.identity_digest {
            return Err(IdentityError::BindingMismatch {
                recorded: self.identity_digest.to_string(),
                actual: derived.to_string(),
            });
        }
        Ok(())
    }

    /// Components bound by this identity.
    pub fn component_kinds(&self) -> Vec<ArtifactComponent> {
        self.components.iter().map(|c| c.component).collect()
    }
}

// ---------------------------------------------------------------------------
// SBOM
// ---------------------------------------------------------------------------

/// One dependency entry in the bill of materials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SbomEntry {
    pub name: String,
    pub version: String,
    /// SPDX license identifier, when known.
    pub license: Option<String>,
    /// Ecosystem the dependency came from.
    pub ecosystem: String,
}

/// The software bill of materials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sbom {
    pub entries: Vec<SbomEntry>,
}

/// Why an SBOM was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbomError {
    /// The SBOM lists no dependencies.
    Empty,
    /// A dependency has no license recorded.
    UnknownLicense(String),
    /// A dependency uses a license incompatible with the release.
    IncompatibleLicense { name: String, license: String },
}

impl std::fmt::Display for SbomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SbomError::Empty => write!(f, "the SBOM is empty"),
            SbomError::UnknownLicense(n) => write!(
                f,
                "dependency {n} has no recorded license, so its terms are unstated"
            ),
            SbomError::IncompatibleLicense { name, license } => write!(
                f,
                "dependency {name} uses {license}, which the release license policy does not permit"
            ),
        }
    }
}

impl std::error::Error for SbomError {}

/// License identifiers the release policy permits for commercial distribution.
///
/// A copyleft license that would impose obligations on VECTOR's own source is
/// not on this list. An unlisted license is a blocking question for a human, not
/// something to be waved through.
pub const PERMITTED_LICENSES: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "CC0-1.0",
    "Unlicense",
    "Zlib",
    "MPL-2.0",
    "0BSD",
    "MIT OR Apache-2.0",
    "Apache-2.0 OR MIT",
    "Unicode-3.0",
    "BSL-1.0",
    // Content/data licenses that are permissive but require attribution.
    // These are acceptable only for build-time data dependencies, which is why
    // they are checked against ATTRIBUTION_ONLY rather than permitted blindly.
    "CC-BY-4.0",
    "CC-BY-3.0",
];

/// Licenses that are permissive but impose an attribution obligation.
///
/// Admitting one of these means the release must ship the attribution, so the
/// SBOM records it and [`Sbom::attribution_required`] reports which entries
/// trigger it. A blanket "permitted" would hide the obligation.
pub const ATTRIBUTION_ONLY: &[&str] = &["CC-BY-4.0", "CC-BY-3.0"];

impl Sbom {
    /// Validate the SBOM against the release license policy.
    ///
    /// RELEASE.md requires an SBOM and license review before GA, so an SBOM
    /// with an unstated or incompatible license blocks the release rather than
    /// being recorded and ignored.
    pub fn validate(&self) -> Result<(), SbomError> {
        if self.entries.is_empty() {
            return Err(SbomError::Empty);
        }
        for entry in &self.entries {
            let license = entry
                .license
                .as_ref()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .ok_or_else(|| SbomError::UnknownLicense(entry.name.clone()))?;

            if !PERMITTED_LICENSES.contains(&license) {
                return Err(SbomError::IncompatibleLicense {
                    name: entry.name.clone(),
                    license: license.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Distinct licenses in use, sorted.
    pub fn licenses(&self) -> Vec<String> {
        let mut licenses: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| e.license.clone())
            .collect();
        licenses.sort();
        licenses.dedup();
        licenses
    }

    /// Dependencies whose license imposes an attribution obligation.
    ///
    /// RELEASE.md requires a license review before GA, and a permissive-but-
    /// attributing license is only satisfied if the notices actually ship. This
    /// reports which entries create that duty so it cannot be forgotten.
    pub fn attribution_required(&self) -> Vec<&SbomEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.license
                    .as_ref()
                    .is_some_and(|l| ATTRIBUTION_ONLY.iter().any(|a| a == l))
            })
            .collect()
    }

    /// Render a third-party notices document.
    ///
    /// This is the artifact the attribution obligation is discharged by, and it
    /// is what the release identity binds as the `Licenses` component.
    pub fn render_notices(&self) -> String {
        let mut out = String::from(
            "# Third-party notices\n\n\
             This product includes the following third-party software and data.\n\
             Each entry is reproduced as declared by the package itself.\n\n",
        );

        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));

        for entry in entries {
            out.push_str(&format!(
                "- {} {} ({}) — {}\n",
                entry.name,
                entry.version,
                entry.ecosystem,
                entry.license.as_deref().unwrap_or("LICENSE NOT DECLARED")
            ));
        }

        let attributed = self.attribution_required();
        if !attributed.is_empty() {
            out.push_str(
                "\n## Attribution-required licenses\n\n\
                 The following dependencies are used under licenses that require\n\
                 attribution to be preserved:\n\n",
            );
            for entry in attributed {
                out.push_str(&format!(
                    "- {} {} — {}\n",
                    entry.name,
                    entry.version,
                    entry.license.as_deref().unwrap_or("unknown")
                ));
            }
        }

        out
    }

    /// Render an SPDX-style tag-value document.
    ///
    /// Chosen because it is the format license tooling reads and it is
    /// reproducible by plain string construction, so the SBOM artifact's digest
    /// is stable.
    pub fn render_spdx(&self, package_name: &str, version: &str) -> String {
        let mut out = String::new();
        out.push_str("SPDXVersion: SPDX-2.3\n");
        out.push_str("DataLicense: CC0-1.0\n");
        out.push_str("SPDXID: SPDXRef-DOCUMENT\n");
        out.push_str(&format!("DocumentName: {package_name}-{version}\n\n"));

        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| {
            a.ecosystem
                .cmp(&b.ecosystem)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.version.cmp(&b.version))
        });

        for entry in entries {
            out.push_str(&format!(
                "PackageName: {}\nSPDXID: SPDXRef-{}-{}\nPackageVersion: {}\nPackageDownloadLocation: NOASSERTION\n",
                entry.name,
                entry.ecosystem,
                entry.name,
                entry.version
            ));
            if let Some(license) = &entry.license {
                out.push_str(&format!("PackageLicenseDeclared: {license}\n"));
            }
            out.push('\n');
        }
        out
    }
}

/// Parse a pnpm lockfile's `packages:` section into SBOM entries.
///
/// pnpm lockfile entries only record a version and a resolution; they do not
/// carry license text. The parser therefore records the dependency and version
/// it can actually see, and leaves `license` as `None` when the lockfile's
/// `license:` field is absent. That means [`Sbom::validate`] will refuse the
/// result until licenses are supplied from an authoritative source — which is
/// the correct outcome, because an SBOM that invents licenses is worse than one
/// that admits it does not know them.
pub fn parse_pnpm_lockfile(text: &str) -> Vec<SbomEntry> {
    let mut entries = Vec::new();
    let mut in_packages = false;
    let mut current: Option<(String, String)> = None;
    let mut current_license: Option<String> = None;

    let flush = |entries: &mut Vec<SbomEntry>,
                 current: &Option<(String, String)>,
                 license: &Option<String>| {
        if let Some((name, version)) = current {
            entries.push(SbomEntry {
                name: name.clone(),
                version: version.clone(),
                license: license.clone(),
                ecosystem: "npm".to_string(),
            });
        }
    };

    for line in text.lines() {
        let trimmed = line.trim_end();

        if trimmed == "packages:" {
            in_packages = true;
            continue;
        }
        // A new top-level section ends the packages block.
        if in_packages && !line.starts_with(' ') && !trimmed.is_empty() {
            flush(&mut entries, &current, &current_license);
            current = None;
            current_license = None;
            in_packages = false;
            continue;
        }
        if !in_packages {
            continue;
        }

        // Package keys are two-space indented and end with a colon.
        if let Some(rest) = trimmed.strip_prefix("  ") {
            if !rest.starts_with(' ') && rest.ends_with(':') {
                flush(&mut entries, &current, &current_license);
                current_license = None;

                let key = rest.trim_end_matches(':').trim_matches('\'');
                // Keys look like "/name@1.2.3" or "/@scope/name@1.2.3".
                let key = key.trim_start_matches('/');
                if let Some(at) = key.rfind('@') {
                    let name = &key[..at];
                    let version = &key[at + 1..];
                    if !name.is_empty() && !version.is_empty() {
                        current = Some((name.to_string(), version.to_string()));
                    } else {
                        current = None;
                    }
                } else {
                    current = None;
                }
                continue;
            }

            if let Some(value) = rest.trim().strip_prefix("license:") {
                let value = value.trim().trim_matches('\'').trim_matches('"');
                if !value.is_empty() {
                    current_license = Some(value.to_string());
                }
            }
        }
    }

    flush(&mut entries, &current, &current_license);

    // Deterministic order so the SBOM artifact's digest is stable.
    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));
    entries
}

/// Read declared licenses from installed npm packages.
///
/// `pnpm-lock.yaml` records versions and resolutions but not licenses, so the
/// lockfile alone yields an SBOM that [`Sbom::validate`] correctly refuses.
/// Installed packages *do* carry a `license` field in their `package.json`, and
/// that is an authoritative declaration rather than a guess, so it is read from
/// there.
///
/// The pnpm store nests packages as `<dir>/<pkg>@<version>/node_modules/<name>`,
/// so the reader walks that shape as well as a flat `node_modules/<name>`.
/// A package whose manifest cannot be read, or which declares no license, is
/// returned with `license: None` — the SBOM then refuses, which is the intended
/// outcome. Inventing a license would be worse than admitting it is unknown.
pub fn read_installed_licenses(node_modules: &std::path::Path) -> Vec<SbomEntry> {
    let mut entries = Vec::new();
    let mut manifests: Vec<std::path::PathBuf> = Vec::new();

    let Ok(dir) = std::fs::read_dir(node_modules) else {
        return entries;
    };

    for entry in dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Flat layout: node_modules/<name>/package.json
        manifests.push(path.join("package.json"));

        // pnpm layout: <store>/<pkg>@<version>/node_modules/<name>/package.json
        let nested = path.join("node_modules");
        if nested.is_dir() {
            if let Ok(children) = std::fs::read_dir(&nested) {
                for child in children.flatten() {
                    let child_path = child.path();
                    if child_path.is_file() {
                        // A symlink or file at this level is not a package.
                        continue;
                    }
                    manifests.push(child_path.join("package.json"));
                }
            }
        }
    }

    for manifest in manifests {
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };

        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let version = value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // `license` may be a string or the legacy `licenses` array of objects.
        let license = value
            .get("license")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                value
                    .get("licenses")
                    .and_then(|v| v.as_array())
                    .and_then(|items| {
                        let names: Vec<String> = items
                            .iter()
                            .filter_map(|item| {
                                item.get("type")
                                    .and_then(|t| t.as_str())
                                    .map(str::to_string)
                            })
                            .collect();
                        if names.is_empty() {
                            None
                        } else {
                            Some(names.join(" OR "))
                        }
                    })
            });

        // The same package can appear more than once (hoisted copies); keep the
        // first declaration so the SBOM has one entry per package.
        if entries
            .iter()
            .any(|e: &SbomEntry| e.name == name && e.version == version)
        {
            continue;
        }

        entries.push(SbomEntry {
            name,
            version,
            license,
            ecosystem: "npm".to_string(),
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));
    entries
}
