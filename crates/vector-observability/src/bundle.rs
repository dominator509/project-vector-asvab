//! Crash bundle assembly and consent manifest (REQ-028, REQ-029, REQ-030).
//!
//! Binding sources: `CRASH_REPAIR_PIPELINE.md` and `OBSERVABILITY.md`.
//!
//! The bundle is the artifact that leaves the machine, so three properties are
//! enforced structurally rather than advised:
//! 1. **Deterministic** — the same capture produces byte-identical output, so a
//!    bundle's hash means something across runs and machines.
//! 2. **Complete** — the components the pipeline names must all be present, or
//!    the bundle is not assemblable.
//! 3. **Consented and redacted** — the manifest records what left and under
//!    whose authority, and assembly refuses an unredacted or unconsented
//!    capture.

use serde::{Deserialize, Serialize};

use crate::crash::{
    approve_release, verify_no_secrets, CanaryRegistry, CrashCapture, ReleaseRefusal,
};

/// A file inside a crash bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleEntry {
    /// Path within the archive. Always relative and forward-slashed.
    pub path: String,
    pub contents: String,
}

/// Build identity, so a report is attributable to an exact artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildIdentity {
    pub app_version: String,
    pub git_sha: String,
    pub artifact_hash: String,
    pub schema_version: String,
    pub content_version: String,
}

/// Sanitized environment facts. No learner content, no paths, no hostname.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentFacts {
    pub os_family: String,
    pub os_version: String,
    pub arch: String,
    /// Total system memory in MiB, bucketed to the nearest 1024 to avoid a
    /// precise fingerprint.
    pub memory_mib_bucketed: u64,
}

impl EnvironmentFacts {
    /// A coarser fact set is strictly better for privacy and costs nothing
    /// diagnostically, so bucketing is applied on construction.
    pub fn new(os_family: &str, os_version: &str, arch: &str, memory_mib: u64) -> Self {
        Self {
            os_family: os_family.to_string(),
            os_version: os_version.to_string(),
            arch: arch.to_string(),
            memory_mib_bucketed: (memory_mib / 1024) * 1024,
        }
    }
}

/// A reproduction recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReproRecipe {
    pub steps: Vec<String>,
    pub expected: String,
    pub observed: String,
    /// Seed that reproduces any randomised behaviour.
    pub replay_seed: Option<u64>,
}

impl ReproRecipe {
    /// Whether the recipe is complete enough to attempt a reproduction.
    ///
    /// A recipe with no steps cannot be acted on, and one that does not state
    /// expected versus observed gives a repairer nothing to compare against.
    pub fn is_actionable(&self) -> bool {
        !self.steps.is_empty()
            && !self.expected.trim().is_empty()
            && !self.observed.trim().is_empty()
    }
}

/// What the learner agreed to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentManifest {
    /// Whether consent was granted at all.
    pub granted: bool,
    /// When consent was recorded.
    pub granted_at: String,
    /// The categories the learner saw and approved.
    pub approved_categories: Vec<String>,
    /// Destinations the learner approved.
    pub approved_destinations: Vec<String>,
    /// Redaction passes applied, recorded in the artifact itself.
    pub redaction_passes: u32,
}

/// Why a bundle could not be assembled.
#[derive(Debug, Clone, PartialEq)]
pub enum BundleError {
    /// The capture has not been redacted sufficiently, still contains a secret,
    /// or the learner has not consented.
    NotReleasable(ReleaseRefusal),
    /// The reproduction recipe is not actionable.
    ReproNotActionable,
    /// A required bundle component is missing.
    MissingComponent(String),
    /// A bundle entry has an unsafe path.
    UnsafeEntryPath(String),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleError::NotReleasable(r) => write!(f, "bundle refused: {r}"),
            BundleError::ReproNotActionable => write!(
                f,
                "the reproduction recipe has no steps or does not state expected \
                 vs observed behaviour, so it cannot be acted on"
            ),
            BundleError::MissingComponent(c) => {
                write!(f, "bundle is missing required component {c}")
            }
            BundleError::UnsafeEntryPath(p) => {
                write!(
                    f,
                    "bundle entry path {p:?} is absolute or escapes the archive"
                )
            }
        }
    }
}

impl std::error::Error for BundleError {}

/// The bundle components `CRASH_REPAIR_PIPELINE.md` names.
pub const REQUIRED_COMPONENTS: &[&str] = &[
    "manifest.json",
    "diagnostic.json",
    "events.ndjson",
    "repro.yaml",
    "environment.json",
    "consent.json",
];

/// An assembled crash bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct CrashBundle {
    pub entries: Vec<BundleEntry>,
}

impl CrashBundle {
    /// Render the archive to a single deterministic string.
    ///
    /// Entries are sorted by path and serialized with a fixed key order, so the
    /// same logical bundle always produces identical bytes. Without that, a
    /// bundle hash would not be comparable between runs and would be useless as
    /// evidence (HARNESS_LAWS.md 7).
    pub fn render(&self) -> String {
        let mut sorted = self.entries.clone();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));

        let mut out = String::new();
        for entry in sorted {
            out.push_str("=== ");
            out.push_str(&entry.path);
            out.push_str(" ===\n");
            out.push_str(&entry.contents);
            if !entry.contents.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }

    /// Component paths present in this bundle.
    pub fn paths(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.path.as_str()).collect()
    }
}

/// Whether a bundle entry path is safe to write into an archive.
///
/// Same containment rule as source ingestion: relative only, no `..`, no drive
/// qualifier. A bundle is an archive, so it has the same zip-slip exposure.
pub fn is_safe_entry_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return false;
    }
    !path
        .replace('\\', "/")
        .split('/')
        .any(|segment| segment == "..")
}

/// Assemble a crash bundle.
///
/// Refuses unless the capture passes the release gate, every named component is
/// present, and the reproduction recipe is actionable. Each of those is a real
/// precondition for the bundle being useful; producing a bundle without them
/// would be producing evidence that cannot support a repair.
#[allow(clippy::too_many_arguments)]
pub fn assemble_bundle(
    capture: &CrashCapture,
    canaries: &CanaryRegistry,
    build: &BuildIdentity,
    environment: &EnvironmentFacts,
    repro: &ReproRecipe,
    events: &[String],
    consent: &ConsentManifest,
) -> Result<CrashBundle, BundleError> {
    // The release gate is the same one the crash center uses, so a bundle
    // cannot take a laxer path to disk than an upload would.
    approve_release(capture, canaries, consent.granted).map_err(BundleError::NotReleasable)?;

    // Belt and braces: re-verify rather than trusting that approve_release and
    // the capture's own pass count agree.
    if let Err(residual) = verify_no_secrets(capture, canaries) {
        return Err(BundleError::NotReleasable(ReleaseRefusal::ResidualSecrets(
            residual,
        )));
    }

    if !repro.is_actionable() {
        return Err(BundleError::ReproNotActionable);
    }

    let entries = vec![
        BundleEntry {
            path: "manifest.json".to_string(),
            contents: serde_json::to_string_pretty(&serde_json::json!({
                "capture_id": capture.id,
                "build": build,
                "redaction_passes": capture.redaction_passes,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        },
        BundleEntry {
            path: "diagnostic.json".to_string(),
            contents: serde_json::to_string_pretty(&serde_json::json!({
                "summary": capture.summary,
                "message": capture.message,
                "stack": capture.stack,
                "context": capture.context,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        },
        BundleEntry {
            path: "events.ndjson".to_string(),
            contents: events.join("\n"),
        },
        BundleEntry {
            path: "repro.yaml".to_string(),
            contents: render_repro(repro),
        },
        BundleEntry {
            path: "environment.json".to_string(),
            contents: serde_json::to_string_pretty(environment)
                .unwrap_or_else(|_| "{}".to_string()),
        },
        BundleEntry {
            path: "consent.json".to_string(),
            contents: serde_json::to_string_pretty(consent).unwrap_or_else(|_| "{}".to_string()),
        },
    ];

    for entry in &entries {
        if !is_safe_entry_path(&entry.path) {
            return Err(BundleError::UnsafeEntryPath(entry.path.clone()));
        }
    }

    let present = bundle_paths(&entries);
    for required in REQUIRED_COMPONENTS {
        if !present.iter().any(|p| p == required) {
            return Err(BundleError::MissingComponent((*required).to_string()));
        }
    }

    Ok(CrashBundle { entries })
}

fn bundle_paths(entries: &[BundleEntry]) -> Vec<String> {
    entries.iter().map(|e| e.path.clone()).collect()
}

/// Render the reproduction recipe as YAML.
///
/// Written by hand rather than through a serializer so no dependency is added
/// for one small document, and so the escaping rules are explicit.
fn render_repro(repro: &ReproRecipe) -> String {
    let mut out = String::from("steps:\n");
    for step in &repro.steps {
        out.push_str(&format!("  - {}\n", yaml_quote(step)));
    }
    out.push_str(&format!("expected: {}\n", yaml_quote(&repro.expected)));
    out.push_str(&format!("observed: {}\n", yaml_quote(&repro.observed)));
    match repro.replay_seed {
        Some(seed) => out.push_str(&format!("replay_seed: {seed}\n")),
        None => out.push_str("replay_seed: null\n"),
    }
    out
}

/// Quote a scalar for YAML so a step containing punctuation stays one value.
fn yaml_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
