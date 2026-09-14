//! Source ingestion sandbox (REQ-050) and per-item content provenance
//! (REQ-056).
//!
//! Binding sources: `QUESTION_FACTORY.md`, `CONTENT_PACK_SPEC.md`,
//! `CONTENT_GOVERNANCE.md` and `SECURITY.md`.
//!
//! REQ-050 requires "sandbox parsing, license/trust checks, prompt-injection
//! isolation". The threats are concrete:
//! - ingesting leaked or copyrighted official test items;
//! - ingesting content whose license does not permit local use;
//! - treating fetched text as instructions rather than data;
//! - archive path traversal ("zip-slip") during extraction.
//!
//! REQ-056 requires that "every active item has objective/source/proof/review/
//! hash". An item missing any of those cannot become active, and the state
//! machine from QUESTION_FACTORY.md is enforced rather than documented.

use serde::{Deserialize, Serialize};

use crate::provenance::ContentHash;

// ---------------------------------------------------------------------------
// Source trust and licensing
// ---------------------------------------------------------------------------

/// Source trust tier from CONTENT_GOVERNANCE.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustTier {
    /// Official ASVAB/DoD/service/government.
    A,
    /// Commercially compatible standards/textbooks.
    B,
    /// Reputable explanatory sources.
    C,
    /// Community/anecdotal. Never authoritative for correctness.
    D,
}

/// License terms that determine whether material may be ingested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum License {
    /// Public domain (e.g. US government works).
    PublicDomain,
    /// Permissive open license permitting reuse with attribution.
    Permissive,
    /// Commercial license already obtained.
    Licensed,
    /// Unknown or unstated terms.
    Unknown,
    /// Known to prohibit reuse.
    Prohibited,
}

impl License {
    /// Whether material under this license may be ingested into a content pack.
    ///
    /// `Unknown` is refused: unstated terms are not permission.
    pub fn permits_ingestion(self) -> bool {
        matches!(
            self,
            License::PublicDomain | License::Permissive | License::Licensed
        )
    }
}

/// Why a source was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestRefusal {
    /// The material appears to be protected or leaked official test content.
    ControlledMaterial,
    /// The license does not permit ingestion.
    LicenseForbids(License),
    /// The source is below the trust floor for the intended use.
    TrustTooLow { tier: TrustTier, floor: TrustTier },
    /// The payload is too large to parse safely.
    TooLarge { bytes: usize, limit: usize },
    /// The payload declares a type that is not plain text.
    UnsupportedMediaType(String),
    /// The payload contains active content.
    ActiveContent,
    /// An archive entry would escape the extraction root.
    PathTraversal(String),
}

impl std::fmt::Display for IngestRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IngestRefusal::ControlledMaterial => write!(
                f,
                "material appears to be protected or leaked official test content"
            ),
            IngestRefusal::LicenseForbids(l) => {
                write!(f, "license {l:?} does not permit ingestion")
            }
            IngestRefusal::TrustTooLow { tier, floor } => {
                write!(
                    f,
                    "trust tier {tier:?} is below the required floor {floor:?}"
                )
            }
            IngestRefusal::TooLarge { bytes, limit } => {
                write!(f, "payload of {bytes} bytes exceeds the {limit} byte limit")
            }
            IngestRefusal::UnsupportedMediaType(t) => {
                write!(f, "media type {t:?} is not ingestible")
            }
            IngestRefusal::ActiveContent => {
                write!(f, "payload contains active content")
            }
            IngestRefusal::PathTraversal(p) => {
                write!(f, "archive entry {p:?} would escape the extraction root")
            }
        }
    }
}

impl std::error::Error for IngestRefusal {}

/// Maximum ingestible payload size, in bytes.
pub const MAX_INGEST_BYTES: usize = 8 * 1024 * 1024;

/// Media types the sandbox will parse.
pub const ALLOWED_MEDIA_TYPES: &[&str] = &[
    "text/plain",
    "text/markdown",
    "text/csv",
    "application/json",
];

/// A proposed source payload.
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePayload {
    pub source_id: String,
    pub media_type: String,
    pub license: License,
    pub trust_tier: TrustTier,
    pub text: String,
    /// Whether the payload claims to contain official test items.
    pub claims_official_items: bool,
}

/// Markers indicating the material is protected official test content.
///
/// QUESTION_FACTORY.md: "Never seed from protected/current official question
/// text." These markers catch an ingestion attempt rather than relying on the
/// submitter to declare it honestly.
const CONTROLLED_MARKERS: &[&str] = &[
    "this is a real asvab question",
    "official asvab test item",
    "actual test question",
    "leaked exam",
    "from the real test",
    "do not distribute - official",
];

/// Whether text appears to be controlled official test material.
pub fn looks_controlled(text: &str) -> bool {
    let lowered = text.to_lowercase();
    CONTROLLED_MARKERS.iter().any(|m| lowered.contains(m))
}

/// Active-content markers that must never be ingested.
const ACTIVE_MARKERS: &[&str] = &[
    "<script",
    "<iframe",
    "javascript:",
    "<object",
    "<embed",
    "<?php",
];

/// Whether text carries active content.
pub fn has_active_content(text: &str) -> bool {
    let lowered = text.to_lowercase();
    ACTIVE_MARKERS.iter().any(|m| lowered.contains(m))
}

/// Sandbox-parse a source payload.
///
/// Checks run in order of severity: controlled material first (a legal and
/// integrity matter), then licensing, trust, size, media type and active
/// content. Text is returned unchanged — the sandbox governs whether it may
/// enter, not what it says, because the authority-isolation rule means it is
/// data regardless.
pub fn sandbox_parse(
    payload: &SourcePayload,
    trust_floor: TrustTier,
) -> Result<String, IngestRefusal> {
    // A payload that self-identifies as official item text is refused before
    // any other consideration.
    if payload.claims_official_items || looks_controlled(&payload.text) {
        return Err(IngestRefusal::ControlledMaterial);
    }

    if !payload.license.permits_ingestion() {
        return Err(IngestRefusal::LicenseForbids(payload.license));
    }

    if payload.trust_tier > trust_floor {
        return Err(IngestRefusal::TrustTooLow {
            tier: payload.trust_tier,
            floor: trust_floor,
        });
    }

    if payload.text.len() > MAX_INGEST_BYTES {
        return Err(IngestRefusal::TooLarge {
            bytes: payload.text.len(),
            limit: MAX_INGEST_BYTES,
        });
    }

    if !ALLOWED_MEDIA_TYPES.contains(&payload.media_type.as_str()) {
        return Err(IngestRefusal::UnsupportedMediaType(
            payload.media_type.clone(),
        ));
    }

    if has_active_content(&payload.text) {
        return Err(IngestRefusal::ActiveContent);
    }

    Ok(payload.text.clone())
}

/// Validate an archive entry path against an extraction root.
///
/// THREAT_MODEL.md names "zip-slip" explicitly: an archive entry like
/// `../../etc/passwd` must never be written outside the root. Containment is
/// decided on normalized components, and any residual `..` is refused outright
/// rather than resolved, because lexical resolution cannot account for symlinks.
pub fn check_archive_entry(entry: &str, root: &str) -> Result<(), IngestRefusal> {
    let normalized_entry = normalize(entry);
    let normalized_root = normalize(root);

    // Absolute entries and drive-qualified paths are refused: they ignore root.
    if entry.starts_with('/') || entry.starts_with('\\') {
        return Err(IngestRefusal::PathTraversal(entry.to_string()));
    }
    if entry.len() >= 2 && entry.as_bytes()[1] == b':' {
        return Err(IngestRefusal::PathTraversal(entry.to_string()));
    }

    if normalized_entry.split('/').any(|s| s == "..") {
        return Err(IngestRefusal::PathTraversal(entry.to_string()));
    }

    if normalized_root.is_empty() {
        return Ok(());
    }

    if normalized_entry == normalized_root
        || normalized_entry
            .strip_prefix(&format!("{normalized_root}/"))
            .is_some()
    {
        return Ok(());
    }

    Err(IngestRefusal::PathTraversal(entry.to_string()))
}

/// Normalize a path for component-wise comparison, preserving `..` so it can be
/// detected by the caller.
fn normalize(path: &str) -> String {
    let unified = path.replace('\\', "/");
    let mut out = String::new();
    for segment in unified.split('/') {
        match segment {
            "" | "." => continue,
            other => {
                if !out.is_empty() {
                    out.push('/');
                }
                out.push_str(other);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// REQ-056: content provenance and the item state machine
// ---------------------------------------------------------------------------

/// Item lifecycle from QUESTION_FACTORY.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemState {
    Draft,
    MachineValidated,
    IndependentVerified,
    ContentReviewed,
    Active,
    Quarantined,
}

impl ItemState {
    /// Whether an item in this state may be served to a learner.
    pub fn is_servable(self) -> bool {
        matches!(self, ItemState::Active)
    }
}

/// Deterministic answer proof.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnswerProof {
    /// An executable or symbolic check (preferred for quantitative items).
    Executable {
        /// The check expression, recorded verbatim.
        expression: String,
        /// The verified answer.
        answer: String,
    },
    /// A source-backed rubric (used for verbal/science items).
    SourceBacked { source_id: String, rubric: String },
}

impl AnswerProof {
    /// Whether the proof is deterministic and independently checkable.
    pub fn is_deterministic(&self) -> bool {
        match self {
            AnswerProof::Executable { .. } => true,
            // A rubric is source-backed but requires judgement, so it is not
            // deterministic in the executable sense.
            AnswerProof::SourceBacked { .. } => false,
        }
    }
}

/// The provenance required before an item may become active (REQ-056).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemProvenance {
    /// The versioned learning objective this item serves.
    pub objective_id: String,
    /// Every source this item draws on.
    pub source_ids: Vec<String>,
    pub answer_proof: AnswerProof,
    /// Named reviewer who approved the item.
    pub reviewer: String,
    /// Hash of the item content.
    pub content_hash: ContentHash,
    /// Hash of the generator, when machine-drafted.
    pub generator_hash: Option<ContentHash>,
    /// Hash of the independent verifier's work.
    pub verifier_hash: Option<ContentHash>,
}

/// Why an item may not become active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    MissingObjective,
    MissingSources,
    MissingProof,
    MissingReviewer,
    MissingContentHash,
    /// The independent verifier echoed the generator rather than verifying.
    VerifierNotIndependent,
    /// A required state transition was skipped.
    InvalidTransition {
        from: ItemState,
        to: ItemState,
    },
}

impl std::fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvenanceError::MissingObjective => {
                write!(f, "item has no learning objective and cannot be active")
            }
            ProvenanceError::MissingSources => {
                write!(f, "item cites no source and cannot be active")
            }
            ProvenanceError::MissingProof => {
                write!(f, "item has no answer proof and cannot be active")
            }
            ProvenanceError::MissingReviewer => {
                write!(f, "item has no named reviewer and cannot be active")
            }
            ProvenanceError::MissingContentHash => {
                write!(f, "item has no content hash and cannot be active")
            }
            ProvenanceError::VerifierNotIndependent => write!(
                f,
                "the independent verifier produced identical output to the \
                 generator, so verification did not actually occur"
            ),
            ProvenanceError::InvalidTransition { from, to } => {
                write!(f, "cannot move item from {from:?} to {to:?}")
            }
        }
    }
}

impl std::error::Error for ProvenanceError {}

impl ItemProvenance {
    /// Whether provenance is complete enough for the item to be active.
    ///
    /// Every field REQ-056 names is required. An empty source list is treated as
    /// absent rather than as "no sources needed".
    pub fn is_complete(&self) -> Result<(), ProvenanceError> {
        if self.objective_id.trim().is_empty() {
            return Err(ProvenanceError::MissingObjective);
        }
        if self.source_ids.is_empty() || self.source_ids.iter().all(|s| s.trim().is_empty()) {
            return Err(ProvenanceError::MissingSources);
        }
        if self.content_hash.as_str().trim().is_empty() {
            return Err(ProvenanceError::MissingContentHash);
        }
        if self.reviewer.trim().is_empty() {
            return Err(ProvenanceError::MissingReviewer);
        }
        // An independent verification that produced byte-identical output is
        // not independent verification (QUESTION_FACTORY.md step 7).
        if let (Some(generator), Some(verifier)) = (&self.generator_hash, &self.verifier_hash) {
            if generator == verifier {
                return Err(ProvenanceError::VerifierNotIndependent);
            }
        }
        Ok(())
    }
}

/// An item with its lifecycle state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentItem {
    pub id: String,
    pub state: ItemState,
    pub provenance: ItemProvenance,
}

/// Whether a state transition follows the documented pipeline.
pub fn is_allowed_transition(from: ItemState, to: ItemState) -> bool {
    use ItemState::*;
    match (from, to) {
        // The documented linear pipeline.
        (Draft, MachineValidated) => true,
        (MachineValidated, IndependentVerified) => true,
        (IndependentVerified, ContentReviewed) => true,
        (ContentReviewed, Active) => true,
        // Any state may be quarantined (QUESTION_FACTORY.md).
        (_, Quarantined) => true,
        // A quarantined item may re-enter review after remediation.
        (Quarantined, Draft) => true,
        _ => false,
    }
}

impl ContentItem {
    pub fn new(id: &str, provenance: ItemProvenance) -> Self {
        Self {
            id: id.to_string(),
            state: ItemState::Draft,
            provenance,
        }
    }

    /// Advance the item through the pipeline.
    pub fn advance_to(&mut self, to: ItemState) -> Result<(), ProvenanceError> {
        if !is_allowed_transition(self.state, to) {
            return Err(ProvenanceError::InvalidTransition {
                from: self.state,
                to,
            });
        }
        // Activation is the point at which provenance must be complete.
        if to == ItemState::Active {
            self.provenance.is_complete()?;
        }
        self.state = to;
        Ok(())
    }

    /// Whether this item may be shown to a learner.
    ///
    /// Both conditions are required: the lifecycle state AND complete
    /// provenance. A state machine alone can be advanced without evidence.
    pub fn is_servable(&self) -> bool {
        self.state.is_servable() && self.provenance.is_complete().is_ok()
    }
}
