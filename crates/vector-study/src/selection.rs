//! Adaptive item selection and study-plan generation (REQ-003 surface).
//!
//! This replaces the previous placeholder that returned hard-coded drill names.
//! Selection is driven by real inputs — mastery estimates, uncertainty, due
//! reviews and the learner's remaining time — so the plan reflects the learner's
//! actual state rather than a constant.
//!
//! SCORING_AND_READINESS.md is binding here: readiness is reported as a band
//! with uncertainty, never as a predicted official score.

use serde::{Deserialize, Serialize};

/// One subtest's mastery estimate as the planner sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillEstimate {
    pub subtest: String,
    /// Estimated mastery in [0, 1].
    pub mastery: f64,
    /// Uncertainty in the estimate; larger means less is known.
    pub uncertainty: f64,
    /// Number of cards currently due for review in this subtest.
    pub due_reviews: u32,
}

impl SkillEstimate {
    /// Reject estimates that cannot describe a real learner state.
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.mastery) {
            return Err(format!("mastery {} outside [0,1]", self.mastery));
        }
        if !self.uncertainty.is_finite() || self.uncertainty < 0.0 {
            return Err(format!(
                "uncertainty {} must be finite and >= 0",
                self.uncertainty
            ));
        }
        if self.subtest.trim().is_empty() {
            return Err("subtest name must not be empty".to_string());
        }
        Ok(())
    }
}

/// What the learner is working toward.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlanGoal {
    /// A target AFQT-style composite score.
    Afqt(u32),
    /// A named job/composite target with a required score.
    Job { name: String, required: u32 },
}

impl PlanGoal {
    pub fn target_score(&self) -> u32 {
        match self {
            PlanGoal::Afqt(score) => *score,
            PlanGoal::Job { required, .. } => *required,
        }
    }
}

/// A single scheduled study activity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedDrill {
    pub subtest: String,
    /// Minutes allocated to this drill.
    pub minutes: u32,
    /// Why this drill was chosen, for learner-facing transparency.
    pub reason: DrillReason,
}

/// The reason a drill was selected. Surfaced to the learner so the plan is
/// explainable rather than an opaque ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrillReason {
    /// Cards are due; reviewing them protects against forgetting.
    DueReview,
    /// Low mastery relative to the goal.
    Weakness,
    /// The estimate is imprecise; more evidence is needed.
    HighUncertainty,
}

/// A generated study plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudyPlan {
    pub drills: Vec<PlannedDrill>,
    pub total_minutes: u32,
}

impl StudyPlan {
    pub fn is_empty(&self) -> bool {
        self.drills.is_empty()
    }

    pub fn subtests(&self) -> Vec<&str> {
        self.drills.iter().map(|d| d.subtest.as_str()).collect()
    }
}

/// Uncertainty at or below this is treated as "well known enough" and does not
/// by itself justify scheduling study time.
pub const UNCERTAINTY_THRESHOLD: f64 = 0.15;

/// Weight applied to uncertainty above the threshold.
pub const UNCERTAINTY_WEIGHT: f64 = 0.5;

/// A skill this close to (or above) the goal is considered on target, so a
/// rounding-level shortfall does not schedule study it does not need.
pub const MASTERY_TOLERANCE: f64 = 0.05;

/// Weight for overdue reviews.
///
/// Reviews are the highest-value activity in spaced repetition — forgetting is
/// what erodes mastery — so a single due card must be able to outrank a moderate
/// mastery gap. With this weight, one due review contributes as much priority as
/// a 0.05 mastery shortfall.
pub const DUE_REVIEW_WEIGHT: f64 = 1.0;

/// Priority weight for a skill. Higher means scheduled earlier.
///
/// The three drivers are combined additively so no single factor silently
/// dominates. A skill that is at or above the goal, with nothing due and a
/// tight estimate, scores exactly zero so the planner can return an empty plan
/// rather than padding the schedule with busy-work.
fn priority(skill: &SkillEstimate, target_fraction: f64) -> (f64, DrillReason) {
    // Due reviews scale linearly: 1 due card is a real signal, 100 is not
    // meaningfully different from 99, so the component saturates.
    let due_component = (skill.due_reviews as f64).min(20.0) / 20.0;
    // Distance below the goal, with a tolerance band so a shortfall within
    // rounding noise is not treated as work to do.
    let weakness_component = (target_fraction - skill.mastery - MASTERY_TOLERANCE).max(0.0);
    // Only *material* uncertainty earns time.
    let uncertainty_component = if skill.uncertainty > UNCERTAINTY_THRESHOLD {
        (skill.uncertainty - UNCERTAINTY_THRESHOLD) * UNCERTAINTY_WEIGHT
    } else {
        0.0
    };

    let score = due_component * DUE_REVIEW_WEIGHT + weakness_component + uncertainty_component;

    // Report the single dominant reason for transparency.
    let reason = if due_component * DUE_REVIEW_WEIGHT >= weakness_component
        && due_component * DUE_REVIEW_WEIGHT >= uncertainty_component
    {
        DrillReason::DueReview
    } else if weakness_component >= uncertainty_component {
        DrillReason::Weakness
    } else {
        DrillReason::HighUncertainty
    };

    (score, reason)
}

/// Convert a target score into a rough fraction-of-maximum goal.
///
/// AFQT and composite scores are reported on a 1..99 scale. This maps the goal
/// onto the same [0,1] space as mastery so the two are comparable. It is a
/// planning heuristic, explicitly not a score prediction.
fn target_fraction(target_score: u32) -> f64 {
    (target_score.clamp(1, 99) as f64) / 99.0
}

/// Generate a study plan for the available time.
///
/// Drills are ordered by priority and then allocated minutes until the time
/// budget is exhausted. A skill with no due reviews, adequate mastery relative
/// to the goal, and low uncertainty receives no time — the plan must be able to
/// say "nothing to do", otherwise it is padding.
pub fn generate_plan(
    skills: &[SkillEstimate],
    goal: &PlanGoal,
    available_minutes: u32,
) -> Result<StudyPlan, String> {
    for skill in skills {
        skill.validate()?;
    }

    if available_minutes == 0 {
        return Err("available time must be greater than zero".to_string());
    }

    let target = target_fraction(goal.target_score());

    let mut ranked: Vec<(f64, DrillReason, &SkillEstimate)> = skills
        .iter()
        .map(|s| {
            let (score, reason) = priority(s, target);
            (score, reason, s)
        })
        // A skill is worth scheduling only if it actually needs work.
        .filter(|(score, _, _)| *score > 0.0)
        .collect();

    // Deterministic ordering: priority descending, then subtest name ascending
    // so equal-priority skills do not reorder between runs.
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.2.subtest.cmp(&b.2.subtest))
    });

    let mut drills = Vec::new();
    let mut remaining = available_minutes;
    let n = ranked.len();

    for (index, (score, reason, skill)) in ranked.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        // Split the time across the skills that need work, weighted by priority,
        // but always leave at least one minute for each remaining skill so a
        // dominant skill cannot starve the others entirely.
        let share = if index + 1 == n {
            remaining
        } else {
            let weight = score / ranked.iter().map(|(s, _, _)| *s).sum::<f64>();
            let minutes = (available_minutes as f64 * weight).floor() as u32;
            minutes.clamp(1, remaining)
        };

        drills.push(PlannedDrill {
            subtest: skill.subtest.clone(),
            minutes: share,
            reason: *reason,
        });
        remaining -= share;
    }

    let total_minutes = drills.iter().map(|d| d.minutes).sum();
    Ok(StudyPlan {
        drills,
        total_minutes,
    })
}

/// A readiness band. Never a predicted official score (ADR-010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadinessBand {
    pub low: f64,
    pub high: f64,
    pub confidence: f64,
    /// Always false in this version: no calibration cohort exists yet.
    pub official_score_claim: bool,
}

/// Estimate a readiness band from mastery estimates (REQ-011).
///
/// The band widens with uncertainty and narrows as evidence accumulates. It
/// deliberately does not emit a point estimate, because ADR-010 and
/// SCORING_AND_READINESS.md forbid presenting a precise predicted score before
/// a validation cohort exists.
pub fn readiness_band(skills: &[SkillEstimate]) -> Result<ReadinessBand, String> {
    for skill in skills {
        skill.validate()?;
    }
    if skills.is_empty() {
        return Err("cannot estimate readiness without any skill estimates".to_string());
    }

    let n = skills.len() as f64;
    let mean_mastery = skills.iter().map(|s| s.mastery).sum::<f64>() / n;
    // Combined uncertainty: the average estimate uncertainty, widened by
    // disagreement between subtests (a learner strong in one area and weak in
    // another is less predictable overall).
    let mean_uncertainty = skills.iter().map(|s| s.uncertainty).sum::<f64>() / n;
    let variance = skills
        .iter()
        .map(|s| (s.mastery - mean_mastery).powi(2))
        .sum::<f64>()
        / n;
    let spread = variance.sqrt();

    let half_width = (mean_uncertainty + spread).clamp(0.0, 1.0);

    Ok(ReadinessBand {
        low: (mean_mastery - half_width).clamp(0.0, 1.0),
        high: (mean_mastery + half_width).clamp(0.0, 1.0),
        // More evidence (lower uncertainty) and tighter agreement raise
        // confidence; the value is bounded so it never claims certainty.
        confidence: (1.0 - half_width).clamp(0.0, 0.95),
        official_score_claim: false,
    })
}
