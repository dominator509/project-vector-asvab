//! Telemetry posture (REQ-052) and externally-sourced score handling (REQ-055).
//!
//! REQ-052 requires "telemetry off, no sensitive enlistment-screening data".
//! Two distinct obligations:
//! 1. Remote telemetry is off unless the learner opts in, and some data classes
//!    may never be collected even with consent.
//! 2. Enlistment-screening attributes (medical, criminal, financial, dependency
//!    history) must not be collected or stored, because they are not needed to
//!    teach arithmetic and their presence would make the app a sensitive-data
//!    processor.
//!
//! REQ-055 requires optional user-entered official scores to be "clearly typed
//! as external". The danger is an imported official score being mistaken for a
//! VECTOR estimate and therefore appearing to validate it.

use serde::{Deserialize, Serialize};

/// The telemetry posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TelemetryMode {
    /// No remote telemetry. The default: collection that requires an action to
    /// disable is not consent.
    #[default]
    Off,
    /// Local-only diagnostics kept on this machine.
    LocalOnly,
    /// Remote telemetry, explicitly opted in by the learner.
    RemoteOptIn,
}

impl TelemetryMode {
    /// Whether data leaves the machine under this mode.
    pub fn transmits_remotely(self) -> bool {
        matches!(self, TelemetryMode::RemoteOptIn)
    }
}

/// A category of data that might be collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataCategory {
    /// Crash stack traces, already redacted.
    CrashDiagnostics,
    /// Aggregate feature usage counts.
    UsageCounts,
    /// The learner's actual answers.
    AnswerContent,
    /// Anything from the prohibited screening set.
    ScreeningData,
    /// Credentials or secrets.
    Secrets,
}

/// Why a telemetry collection was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryRefusal {
    /// Remote telemetry is not enabled.
    RemoteDisabled,
    /// This category may never be collected, regardless of consent.
    CategoryForbidden(DataCategory),
}

impl std::fmt::Display for TelemetryRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TelemetryRefusal::RemoteDisabled => {
                write!(f, "remote telemetry is off")
            }
            TelemetryRefusal::CategoryForbidden(c) => {
                write!(f, "data category {c:?} must never be collected")
            }
        }
    }
}

impl std::error::Error for TelemetryRefusal {}

/// Whether a category may be collected at all, with any consent.
///
/// Screening data and secrets are forbidden outright — consent cannot make the
/// app an appropriate processor of enlistment-screening attributes. Answer
/// content is also excluded here: the learner's actual answers are study data,
/// not telemetry, and recording them under a "diagnostics" label would be a
/// mislabelling rather than a collection choice.
pub fn category_is_collectible(category: DataCategory) -> bool {
    matches!(
        category,
        DataCategory::CrashDiagnostics | DataCategory::UsageCounts
    )
}

/// Decide whether a telemetry event may be recorded.
pub fn may_collect(mode: TelemetryMode, category: DataCategory) -> Result<(), TelemetryRefusal> {
    // A category that is never collectible is refused first, so the reason
    // reported is the real one regardless of mode.
    if !category_is_collectible(category) {
        return Err(TelemetryRefusal::CategoryForbidden(category));
    }
    if mode.transmits_remotely() {
        return Ok(());
    }
    // Local diagnostics are still collected in LocalOnly mode; anything going
    // off-machine requires opt-in.
    match mode {
        TelemetryMode::LocalOnly => Ok(()),
        _ => Err(TelemetryRefusal::RemoteDisabled),
    }
}

// ---------------------------------------------------------------------------
// REQ-052: prohibited screening data
// ---------------------------------------------------------------------------

/// Attributes that must never be collected.
///
/// These are the sensitive enlistment-screening categories. VECTOR teaches
/// arithmetic and vocabulary; none of this is needed, and collecting it would
/// create obligations and risk for no product benefit.
pub const PROHIBITED_SCREENING_FIELDS: &[&str] = &[
    "medical_history",
    "mental_health",
    "prescription",
    "disability",
    "criminal_record",
    "arrest",
    "drug_use",
    "alcohol_use",
    "financial_debt",
    "credit_score",
    "bankruptcy",
    "dependency_status",
    "citizenship_status",
    "immigration_status",
    "sexual_orientation",
    "religion",
    "race_ethnicity",
    "polygraph",
];

/// Whether a field name belongs to the prohibited screening set.
pub fn is_prohibited_field(field: &str) -> bool {
    let lower = field.to_lowercase().replace(['-', ' '], "_");
    PROHIBITED_SCREENING_FIELDS
        .iter()
        .any(|p| lower.contains(p) || p.contains(lower.as_str()))
}

/// A profile write that must be screened before it is stored.
pub fn screen_profile_field(field: &str) -> Result<(), TelemetryRefusal> {
    if is_prohibited_field(field) {
        Err(TelemetryRefusal::CategoryForbidden(
            DataCategory::ScreeningData,
        ))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// REQ-055: externally-sourced scores
// ---------------------------------------------------------------------------

/// Where a score came from.
///
/// The distinction is the whole point of REQ-055: an external score must never
/// be presented as if VECTOR computed it, because that would imply VECTOR's
/// model has been validated against the official test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreOrigin {
    /// VECTOR's own practice estimate.
    VectorEstimate,
    /// Entered by the learner from an official score report.
    UserEnteredOfficial,
    /// Imported from an official score report file.
    ImportedOfficial,
}

impl ScoreOrigin {
    /// Whether this score originates outside VECTOR.
    pub fn is_external(self) -> bool {
        !matches!(self, ScoreOrigin::VectorEstimate)
    }

    /// The label that must accompany the score wherever it is shown.
    pub fn required_label(self) -> &'static str {
        match self {
            ScoreOrigin::VectorEstimate => "VECTOR practice estimate — not an official score",
            ScoreOrigin::UserEnteredOfficial => {
                "Official score entered by you — from an external test administration"
            }
            ScoreOrigin::ImportedOfficial => "Official score imported from an external report",
        }
    }
}

/// A score record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreRecord {
    pub origin: ScoreOrigin,
    /// The score value, on the official 1..99 scale when external.
    pub value: u32,
    /// Date the learner reported or imported it.
    pub recorded_on: String,
    /// Free-text provenance the learner supplied.
    pub note: String,
}

impl ScoreRecord {
    /// Validate a record.
    ///
    /// An external score must carry provenance: an unattributed official score
    /// is indistinguishable from a VECTOR guess, which is exactly the
    /// confusion REQ-055 exists to prevent.
    pub fn validate(&self) -> Result<(), String> {
        if self.value == 0 || self.value > 99 {
            return Err(format!(
                "score {} is outside the 1..99 reporting scale",
                self.value
            ));
        }
        if self.recorded_on.trim().is_empty() {
            return Err("a score record must record when it was entered".to_string());
        }
        if self.origin.is_external() && self.note.trim().is_empty() {
            return Err(
                "an external score requires provenance describing where it came from".to_string(),
            );
        }
        Ok(())
    }

    /// The label this record must be displayed with.
    pub fn display_label(&self) -> &'static str {
        self.origin.required_label()
    }
}

/// Whether an external score may be blended into VECTOR's readiness estimate.
///
/// It may not. Blending a real official score into a practice estimate would
/// silently imply the estimate is calibrated against the official test, which
/// ADR-010 forbids and no validation study supports.
pub fn may_blend_into_readiness(origin: ScoreOrigin) -> bool {
    !origin.is_external()
}
