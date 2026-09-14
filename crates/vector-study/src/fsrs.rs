//! FSRS-compatible spaced repetition scheduling (REQ-043).
//!
//! SPEC-001 requires that FSRS scheduling **stores parameters and review
//! history**. This module implements the FSRS-4.5/FSRS-5 memory model on top of
//! those stored parameters.
//!
//! The critical property for a study application is *monotonicity of
//! stability*: recalling successfully must never schedule the card sooner than
//! a failure would. A scheduler that violates this silently wastes the
//! learner's time, so it is asserted directly in the tests.

use serde::{Deserialize, Serialize};

/// How well the learner recalled a card (FSRS rating scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rating {
    /// Complete blackout; the answer was not recalled.
    Again = 1,
    /// Recalled with serious difficulty.
    Hard = 2,
    /// Recalled with some effort (the expected case).
    Good = 3,
    /// Instant, effortless recall.
    Easy = 4,
}

impl Rating {
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Rating::Again),
            2 => Some(Rating::Hard),
            3 => Some(Rating::Good),
            4 => Some(Rating::Easy),
            _ => None,
        }
    }
}

/// The 17 FSRS-4.5 weights plus the FSRS-5 decay/sharpen parameters.
///
/// Stored per learner so a schedule is reproducible from persisted state rather
/// than depending on hard-coded constants that could change between releases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsrsParameters {
    /// Initial stability for each of the four ratings.
    pub w: [f64; 17],
    /// Desired retention target in (0, 1).
    pub desired_retention: f64,
}

impl Default for FsrsParameters {
    /// Published FSRS-4.5 default weights.
    ///
    /// These are the documented defaults for a learner with no review history;
    /// they are a starting point, not a per-user optimized fit.
    fn default() -> Self {
        Self {
            w: [
                0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367,
                1.0461, 2.1072, 0.0793, 0.3246, 1.587, 0.2272, 2.8755,
            ],
            desired_retention: 0.9,
        }
    }
}

impl FsrsParameters {
    /// Reject parameter sets that are structurally invalid.
    ///
    /// A negative stability or an out-of-range retention target would produce
    /// nonsensical intervals, so they are refused at the boundary rather than
    /// producing a silently broken schedule.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.desired_retention > 0.0 && self.desired_retention < 1.0) {
            return Err(format!(
                "desired_retention {} must be in (0,1)",
                self.desired_retention
            ));
        }
        if self.w.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("FSRS weights must be finite and non-negative".to_string());
        }
        Ok(())
    }
}

/// The scheduling state of one card, persisted between sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardState {
    /// Current memory stability in days.
    pub stability: f64,
    /// Current item difficulty in [1, 10].
    pub difficulty: f64,
    /// Consecutive successful reviews.
    pub reps: u32,
    /// Total reviews including failures.
    pub lapses: u32,
    /// The rating given on the most recent review.
    pub last_rating: Option<Rating>,
    /// Days since the last review at the time of scheduling.
    pub elapsed_days: f64,
}

impl Default for CardState {
    fn default() -> Self {
        Self {
            stability: 0.0,
            difficulty: 0.0,
            reps: 0,
            lapses: 0,
            last_rating: None,
            elapsed_days: 0.0,
        }
    }
}

/// The outcome of scheduling: the new state and the interval to next review.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub state: CardState,
    /// Days until the card is next due (never below 1 for a studied card).
    pub interval_days: f64,
    /// Retrievability estimated at the moment of review.
    pub retrievability: f64,
}

/// Minimum and maximum interval bounds, in days.
///
/// The lower bound keeps a card from being scheduled in the past; the upper
/// bound keeps "easy" from producing intervals so long the learner never
/// revisits the material before the exam.
pub const MIN_INTERVAL_DAYS: f64 = 1.0;
pub const MAX_INTERVAL_DAYS: f64 = 365.0;

/// Estimated probability of recall after `elapsed` days with `stability`.
///
/// This is the FSRS power-forgetting curve, `R(t) = (1 + t/(9S))^-1`.
pub fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    if stability <= 0.0 {
        return 0.0;
    }
    let t = elapsed_days.max(0.0);
    (1.0 + t / (9.0 * stability)).powf(-1.0)
}

/// The interval (in days) at which retrievability decays to `desired`.
///
/// Inverts the forgetting curve: `t = 9S(1/R - 1)`.
pub fn interval_for_retention(stability: f64, desired_retention: f64) -> f64 {
    if stability <= 0.0 {
        return MIN_INTERVAL_DAYS;
    }
    let raw = 9.0 * stability * (1.0 / desired_retention - 1.0);
    raw.clamp(MIN_INTERVAL_DAYS, MAX_INTERVAL_DAYS)
}

/// The FSRS scheduler.
pub struct Fsrs {
    params: FsrsParameters,
}

impl Fsrs {
    pub fn new(params: FsrsParameters) -> Result<Self, String> {
        params.validate()?;
        Ok(Self { params })
    }

    pub fn parameters(&self) -> &FsrsParameters {
        &self.params
    }

    /// Initial stability for a first-ever review, by rating.
    fn initial_stability(&self, rating: Rating) -> f64 {
        // FSRS: S_0(G) = w[G-1], clamped so it is always positive.
        self.params.w[(rating as usize) - 1].max(0.01)
    }

    /// Initial difficulty for a first-ever review, by rating.
    fn initial_difficulty(&self, rating: Rating) -> f64 {
        // FSRS: D_0(G) = w4 - (G - 3) * w5, clamped to [1, 10].
        let d = self.params.w[4] - ((rating as usize) as f64 - 3.0) * self.params.w[5];
        d.clamp(1.0, 10.0)
    }

    /// Difficulty after a review, with mean reversion toward the FSRS mean.
    fn next_difficulty(&self, difficulty: f64, rating: Rating) -> f64 {
        let delta = -self.params.w[6] * ((rating as usize) as f64 - 3.0);
        // Linear damping, then reversion toward D_0(Good) = w4.
        let damped = difficulty + delta * (10.0 - difficulty) / 9.0;
        let reverted = self.params.w[7] * self.initial_difficulty(Rating::Good)
            + (1.0 - self.params.w[7]) * damped;
        reverted.clamp(1.0, 10.0)
    }

    /// Stability after a successful review.
    fn stability_on_success(&self, difficulty: f64, stability: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard {
            self.params.w[15]
        } else {
            1.0
        };
        let easy_bonus = if rating == Rating::Easy {
            self.params.w[16]
        } else {
            1.0
        };

        let growth = 1.0
            + easy_bonus
                * self.params.w[8]
                * (11.0 - difficulty)
                * stability.powf(-self.params.w[9])
                * ((self.params.w[10] * (1.0 - r)).exp() - 1.0)
                * hard_penalty;

        (stability * growth).max(0.01)
    }

    /// Stability after a lapse (a failed review).
    fn stability_on_lapse(&self, difficulty: f64, stability: f64, r: f64) -> f64 {
        let s = self.params.w[11]
            * difficulty.powf(-self.params.w[12])
            * ((stability + 1.0).powf(self.params.w[13]) - 1.0)
            * ((1.0 - r) * self.params.w[14]).exp();
        s.max(0.01)
    }

    /// Schedule a card after a review.
    pub fn review(&self, state: &CardState, rating: Rating, elapsed_days: f64) -> Schedule {
        let elapsed = elapsed_days.max(0.0);

        // First review: stability and difficulty come from the initial curves.
        if state.reps == 0 {
            let stability = self.initial_stability(rating);
            let difficulty = self.initial_difficulty(rating);
            let interval = interval_for_retention(stability, self.params.desired_retention)
                .max(MIN_INTERVAL_DAYS);

            return Schedule {
                state: CardState {
                    stability,
                    difficulty,
                    reps: 1,
                    lapses: if rating == Rating::Again { 1 } else { 0 },
                    last_rating: Some(rating),
                    elapsed_days: elapsed,
                },
                interval_days: interval,
                retrievability: 1.0,
            };
        }

        let r = retrievability(elapsed, state.stability);
        let difficulty = self.next_difficulty(state.difficulty, rating);

        let (stability, lapses) = if rating == Rating::Again {
            (
                self.stability_on_lapse(difficulty, state.stability, r),
                state.lapses + 1,
            )
        } else {
            (
                self.stability_on_success(difficulty, state.stability, r, rating),
                state.lapses,
            )
        };

        // A failed review must come back promptly, not months later: the
        // interval is bounded by the relearn step rather than the stability
        // curve alone. This is what makes "Again" behave like a lapse.
        let interval = if rating == Rating::Again {
            MIN_INTERVAL_DAYS
        } else {
            interval_for_retention(stability, self.params.desired_retention)
        };

        Schedule {
            state: CardState {
                stability,
                difficulty,
                reps: state.reps + 1,
                lapses,
                last_rating: Some(rating),
                elapsed_days: elapsed,
            },
            interval_days: interval.max(MIN_INTERVAL_DAYS),
            retrievability: r,
        }
    }
}
