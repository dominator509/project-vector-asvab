//! Content QA review state, audit history and rollback (REQ-048), and internal
//! difficulty calibration with an explicit no-equivalence-claim rule (REQ-049).
//!
//! REQ-048 requires "review state, history, rollback, audit". The control that
//! matters is that content cannot become active without a recorded review, and
//! that every transition is auditable — including a rollback, which must itself
//! be recorded rather than quietly reverting state.
//!
//! REQ-049 requires "internal calibration, no official psychometric equivalence
//! claim". The danger is a difficulty number being read as an official
//! parameter, so this module refuses to emit a label that implies equivalence.

use serde::{Deserialize, Serialize};

/// Review state of a content item or pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewState {
    /// Not yet looked at by a reviewer.
    Unreviewed,
    /// Under review.
    InReview,
    /// Approved by a named reviewer and eligible to activate.
    Approved,
    /// Rejected; must not activate.
    Rejected,
    /// Previously active, now superseded.
    Superseded,
}

impl ReviewState {
    /// Whether content in this state may be served to a learner.
    ///
    /// Only `Approved` qualifies. Everything else — including `Unreviewed`,
    /// which is the tempting default — is refused.
    pub fn is_activatable(self) -> bool {
        matches!(self, ReviewState::Approved)
    }
}

/// A reviewer decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDecision {
    pub reviewer: String,
    pub state: ReviewState,
    /// Recorded rationale. Required, so a decision can be audited later.
    pub note: String,
    pub decided_on: String,
}

/// A single audit entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub sequence: u64,
    pub item_id: String,
    pub from: ReviewState,
    pub to: ReviewState,
    pub actor: String,
    pub note: String,
    pub at: String,
}

/// Why a review transition was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewError {
    /// The item has no recorded review.
    NotReviewed(String),
    /// A decision was supplied without a rationale.
    MissingRationale,
    /// A decision was supplied without a named reviewer.
    MissingReviewer,
    /// The requested transition is not permitted from the current state.
    InvalidTransition { from: ReviewState, to: ReviewState },
    /// Unknown item.
    UnknownItem(String),
}

impl std::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewError::NotReviewed(id) => {
                write!(f, "content {id} has no recorded review and cannot activate")
            }
            ReviewError::MissingRationale => {
                write!(f, "a review decision requires a recorded rationale")
            }
            ReviewError::MissingReviewer => {
                write!(f, "a review decision requires a named reviewer")
            }
            ReviewError::InvalidTransition { from, to } => {
                write!(f, "cannot move from {from:?} to {to:?}")
            }
            ReviewError::UnknownItem(id) => write!(f, "unknown content item {id}"),
        }
    }
}

impl std::error::Error for ReviewError {}

/// A content item under review, with its full audit history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentItem {
    pub id: String,
    pub state: ReviewState,
    pub decisions: Vec<ReviewDecision>,
    pub history: Vec<AuditEntry>,
}

impl ContentItem {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: ReviewState::Unreviewed,
            decisions: Vec::new(),
            history: Vec::new(),
        }
    }
}

/// The content QA ledger.
#[derive(Debug, Clone, Default)]
pub struct ContentQa {
    items: Vec<ContentItem>,
    next_sequence: u64,
}

impl ContentQa {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_sequence: 1,
        }
    }

    pub fn add(&mut self, id: &str) {
        if !self.items.iter().any(|i| i.id == id) {
            self.items.push(ContentItem::new(id));
        }
    }

    pub fn item(&self, id: &str) -> Option<&ContentItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn history(&self, id: &str) -> Vec<AuditEntry> {
        self.item(id).map(|i| i.history.clone()).unwrap_or_default()
    }

    /// Record a review decision and append an audit entry.
    ///
    /// A decision without a reviewer or a rationale is refused: an unattributed
    /// or unexplained approval is not auditable.
    pub fn decide(
        &mut self,
        id: &str,
        reviewer: &str,
        state: ReviewState,
        note: &str,
        at: &str,
    ) -> Result<(), ReviewError> {
        if reviewer.trim().is_empty() {
            return Err(ReviewError::MissingReviewer);
        }
        if note.trim().is_empty() {
            return Err(ReviewError::MissingRationale);
        }

        let from = {
            let item = self
                .items
                .iter()
                .find(|i| i.id == id)
                .ok_or_else(|| ReviewError::UnknownItem(id.to_string()))?;
            if !is_allowed_transition(item.state, state) {
                return Err(ReviewError::InvalidTransition {
                    from: item.state,
                    to: state,
                });
            }
            item.state
        };

        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let item = self
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .expect("item existence checked above");

        item.state = state;
        item.decisions.push(ReviewDecision {
            reviewer: reviewer.to_string(),
            state,
            note: note.to_string(),
            decided_on: at.to_string(),
        });
        item.history.push(AuditEntry {
            sequence,
            item_id: id.to_string(),
            from,
            to: state,
            actor: reviewer.to_string(),
            note: note.to_string(),
            at: at.to_string(),
        });

        Ok(())
    }

    /// Activate an item, requiring an approved review.
    pub fn activate(&mut self, id: &str, actor: &str, at: &str) -> Result<(), ReviewError> {
        let state = self
            .item(id)
            .ok_or_else(|| ReviewError::UnknownItem(id.to_string()))?
            .state;

        if !state.is_activatable() {
            return Err(ReviewError::NotReviewed(id.to_string()));
        }

        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let item = self
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .expect("existence checked");
        item.history.push(AuditEntry {
            sequence,
            item_id: id.to_string(),
            from: state,
            to: ReviewState::Superseded,
            actor: actor.to_string(),
            note: "activated".to_string(),
            at: at.to_string(),
        });
        item.state = ReviewState::Superseded;
        Ok(())
    }

    /// Roll an item back to a rejected state.
    ///
    /// The rollback is recorded as its own audit entry, so history shows that a
    /// reversion happened rather than appearing to have never been approved.
    pub fn rollback(
        &mut self,
        id: &str,
        actor: &str,
        reason: &str,
        at: &str,
    ) -> Result<(), ReviewError> {
        if reason.trim().is_empty() {
            return Err(ReviewError::MissingRationale);
        }
        let from = self
            .item(id)
            .ok_or_else(|| ReviewError::UnknownItem(id.to_string()))?
            .state;

        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let item = self
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .expect("existence checked");
        item.state = ReviewState::Rejected;
        item.history.push(AuditEntry {
            sequence,
            item_id: id.to_string(),
            from,
            to: ReviewState::Rejected,
            actor: actor.to_string(),
            note: reason.to_string(),
            at: at.to_string(),
        });
        Ok(())
    }

    /// The full audit trail across all items, in sequence order.
    pub fn audit(&self) -> Vec<AuditEntry> {
        let mut all: Vec<AuditEntry> = self.items.iter().flat_map(|i| i.history.clone()).collect();
        all.sort_by_key(|e| e.sequence);
        all
    }
}

/// Whether a review-state transition is permitted.
fn is_allowed_transition(from: ReviewState, to: ReviewState) -> bool {
    use ReviewState::*;
    match (from, to) {
        // A reviewer may start looking at unseen content.
        (Unreviewed, InReview) => true,
        // A review may conclude either way.
        (InReview, Approved) => true,
        (InReview, Rejected) => true,
        // Rejected content can be reconsidered after rework.
        (Rejected, InReview) => true,
        // An approved item may be rejected on later discovery of a defect.
        (Approved, Rejected) => true,
        // Activation and rollback are handled by their own methods.
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// REQ-049: internal difficulty calibration
// ---------------------------------------------------------------------------

/// An internally calibrated difficulty estimate.
///
/// This is VECTOR's own estimate derived from observed learner performance. It
/// is explicitly **not** an official psychometric parameter, and the type
/// carries that constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibratedDifficulty {
    /// Difficulty on VECTOR's own internal logit-like scale.
    pub internal: f64,
    /// Number of observations behind the estimate.
    pub observations: u32,
    /// Standard error of the internal estimate.
    pub standard_error: f64,
}

/// A learner-facing difficulty label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DifficultyLabel {
    Easy,
    Moderate,
    Hard,
}

/// Why a difficulty estimate could not be produced or presented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    /// Too few observations to estimate anything.
    InsufficientObservations { have: u32, need: u32 },
    /// The estimate must not be presented as an official parameter.
    EquivalenceClaimRefused,
    /// Non-finite input.
    InvalidInput,
}

impl std::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalibrationError::InsufficientObservations { have, need } => write!(
                f,
                "only {have} observations; at least {need} are required to calibrate"
            ),
            CalibrationError::EquivalenceClaimRefused => write!(
                f,
                "VECTOR difficulty is an internal estimate and must not be presented \
                 as an official psychometric parameter"
            ),
            CalibrationError::InvalidInput => write!(f, "difficulty inputs must be finite"),
        }
    }
}

impl std::error::Error for CalibrationError {}

/// Minimum observations before a difficulty estimate is reported.
pub const MIN_CALIBRATION_OBSERVATIONS: u32 = 30;

/// Estimate difficulty from the proportion of learners answering correctly.
///
/// A simple, explainable internal calibration: fewer correct answers means a
/// harder item. Refuses to report anything below the observation floor, because
/// an estimate from a handful of attempts is noise presented as measurement.
pub fn calibrate(attempts: u32, correct: u32) -> Result<CalibratedDifficulty, CalibrationError> {
    if attempts < MIN_CALIBRATION_OBSERVATIONS {
        return Err(CalibrationError::InsufficientObservations {
            have: attempts,
            need: MIN_CALIBRATION_OBSERVATIONS,
        });
    }
    if correct > attempts {
        return Err(CalibrationError::InvalidInput);
    }

    let p = correct as f64 / attempts as f64;
    // Clamp so a perfect or zero proportion does not produce an infinite logit.
    let clamped = p.clamp(0.01, 0.99);
    let internal = -(clamped / (1.0 - clamped)).ln();

    // Standard error of a proportion, mapped to the same scale.
    let se_p = ((p * (1.0 - p)) / attempts as f64).sqrt();
    let standard_error = se_p / (clamped * (1.0 - clamped));

    if !internal.is_finite() || !standard_error.is_finite() {
        return Err(CalibrationError::InvalidInput);
    }

    Ok(CalibratedDifficulty {
        internal,
        observations: attempts,
        standard_error,
    })
}

/// Map an internal estimate to a learner-facing label.
pub fn difficulty_label(difficulty: &CalibratedDifficulty) -> DifficultyLabel {
    if difficulty.internal < -0.5 {
        DifficultyLabel::Easy
    } else if difficulty.internal > 0.5 {
        DifficultyLabel::Hard
    } else {
        DifficultyLabel::Moderate
    }
}

/// Produce a learner-facing description of a difficulty estimate.
///
/// The wording is fixed and states that the value is VECTOR's own estimate.
/// There is deliberately no function that renders this as an official
/// parameter: the equivalence claim is not merely discouraged, it has no code
/// path.
pub fn describe_difficulty(difficulty: &CalibratedDifficulty) -> String {
    let label = difficulty_label(difficulty);
    format!(
        "{label:?} — VECTOR's internal estimate from {} attempt(s). \
         This is a practice heuristic, not an official psychometric value.",
        difficulty.observations
    )
}

/// Whether an official psychometric equivalence claim is permitted.
///
/// Always false. Exposed so the product rule is testable.
pub const fn official_equivalence_claim_permitted() -> bool {
    false
}
