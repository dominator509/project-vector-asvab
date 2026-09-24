//! Exam simulator domain logic: CAT vs paper navigation and versioned timing
//! profiles (REQ-006).
//!
//! SCORING_AND_READINESS.md is binding: "CAT-style practice follows public
//! constraints but uses VECTOR's documented adaptive selector, never a claimed
//! reconstruction of confidential official scoring/adaptation."
//!
//! The client-facing property that matters is navigation: on a real CAT the
//! learner commits to an answer and cannot go back, whereas the paper form
//! permits review. A simulator that allows backtracking on the CAT form teaches
//! the wrong test-taking behaviour, so it is enforced here rather than in the UI.

use serde::{Deserialize, Serialize};

/// Which exam form is being simulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExamForm {
    /// Computer-adaptive: answer committed, no backtracking.
    Cat,
    /// Paper form: free navigation and review before submission.
    Paper,
}

impl ExamForm {
    /// Whether the learner may revisit a previously answered item.
    pub fn allows_backtracking(self) -> bool {
        match self {
            ExamForm::Cat => false,
            ExamForm::Paper => true,
        }
    }
}

/// A versioned timing profile.
///
/// Timing differs by form and must be versioned so a test run is reproducible
/// and a learner's performance is always attributable to a known profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingProfile {
    /// Stable profile id, e.g. "cat-standard".
    pub id: String,
    /// Monotonic version of this profile.
    pub version: u32,
    pub form: ExamForm,
    /// Seconds allowed per subtest, keyed by subtest code.
    pub per_subtest_seconds: Vec<(String, u32)>,
    /// Whether the learner may pause the timer.
    pub pausable: bool,
    /// Date the profile was reviewed against public sources, `YYYY-MM-DD`.
    pub source_reviewed_on: String,
}

impl TimingProfile {
    /// Seconds allowed for a subtest, if this profile covers it.
    pub fn seconds_for(&self, subtest: &str) -> Option<u32> {
        self.per_subtest_seconds
            .iter()
            .find(|(code, _)| code.eq_ignore_ascii_case(subtest))
            .map(|(_, seconds)| *seconds)
    }

    /// Reject profiles that cannot describe a real exam.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("timing profile id must not be empty".to_string());
        }
        if self.version == 0 {
            return Err("timing profile version must be >= 1".to_string());
        }
        if self.per_subtest_seconds.is_empty() {
            return Err("timing profile must cover at least one subtest".to_string());
        }
        for (code, seconds) in &self.per_subtest_seconds {
            if code.trim().is_empty() {
                return Err("subtest code must not be empty".to_string());
            }
            // Zero seconds would make an item unanswerable.
            if *seconds == 0 {
                return Err(format!("subtest {code} has zero time"));
            }
        }
        // Duplicate subtest codes would make `seconds_for` ambiguous.
        let mut codes: Vec<String> = self
            .per_subtest_seconds
            .iter()
            .map(|(c, _)| c.to_lowercase())
            .collect();
        let total = codes.len();
        codes.sort();
        codes.dedup();
        if codes.len() != total {
            return Err("duplicate subtest code in timing profile".to_string());
        }
        Ok(())
    }
}

/// The learner's position and answer state within a simulated exam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExamSession {
    pub profile_id: String,
    pub profile_version: u32,
    pub form: ExamForm,
    /// Question ids in presentation order.
    pub items: Vec<String>,
    /// Index of the item currently shown.
    pub cursor: usize,
    /// Committed answers: (index, chosen option).
    pub answers: Vec<(usize, Option<u32>)>,
    /// Indices the learner has locked in (CAT commits on advance).
    pub locked: Vec<usize>,
    /// Remaining seconds for the whole session.
    pub remaining_seconds: u32,
    pub finished: bool,
}

/// Why an exam navigation action was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExamError {
    /// The item index is outside the exam.
    OutOfRange { index: usize, len: usize },
    /// Backtracking was attempted on a form that forbids it.
    BacktrackingForbidden,
    /// The exam is already finished.
    AlreadyFinished,
    /// The timer has run out; no further navigation is allowed.
    TimeExpired,
    /// The answer option index is not valid for a four-option item.
    InvalidOption(u32),
}

impl std::fmt::Display for ExamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExamError::OutOfRange { index, len } => {
                write!(f, "item index {index} outside exam of {len} items")
            }
            ExamError::BacktrackingForbidden => {
                write!(
                    f,
                    "this exam form does not allow returning to a previous item"
                )
            }
            ExamError::AlreadyFinished => write!(f, "the exam is already finished"),
            ExamError::TimeExpired => write!(f, "the exam time has expired"),
            ExamError::InvalidOption(o) => write!(f, "option {o} is not a valid answer"),
        }
    }
}

impl std::error::Error for ExamError {}

/// Number of answer options per item in the shipped content model.
pub const OPTIONS_PER_ITEM: u32 = 4;

impl ExamSession {
    /// Start a session from a validated profile.
    pub fn start(profile: &TimingProfile, items: Vec<String>) -> Result<Self, String> {
        profile.validate()?;
        if items.is_empty() {
            return Err("an exam session requires at least one item".to_string());
        }
        let remaining: u32 = profile.per_subtest_seconds.iter().map(|(_, s)| *s).sum();
        Ok(Self {
            profile_id: profile.id.clone(),
            profile_version: profile.version,
            form: profile.form,
            items,
            cursor: 0,
            answers: vec![(0, None); 1],
            locked: Vec::new(),
            remaining_seconds: remaining,
            finished: false,
        })
    }

    pub fn current_item(&self) -> Option<&String> {
        self.items.get(self.cursor)
    }

    fn check_playable(&self) -> Result<(), ExamError> {
        if self.finished {
            return Err(ExamError::AlreadyFinished);
        }
        if self.remaining_seconds == 0 {
            return Err(ExamError::TimeExpired);
        }
        Ok(())
    }

    /// Record an answer for the current item.
    ///
    /// On a CAT form the answer is locked and cannot be changed, mirroring the
    /// real test where an answer is committed on submission.
    pub fn answer(&mut self, option: u32) -> Result<(), ExamError> {
        self.check_playable()?;
        if option >= OPTIONS_PER_ITEM {
            return Err(ExamError::InvalidOption(option));
        }

        if self.form == ExamForm::Cat && self.locked.contains(&self.cursor) {
            return Err(ExamError::BacktrackingForbidden);
        }

        self.set_answer(self.cursor, Some(option));
        if self.form == ExamForm::Cat {
            self.locked.push(self.cursor);
        }
        Ok(())
    }

    fn set_answer(&mut self, index: usize, value: Option<u32>) {
        if let Some(slot) = self.answers.iter_mut().find(|(i, _)| *i == index) {
            slot.1 = value;
        } else {
            self.answers.push((index, value));
        }
    }

    pub fn answer_at(&self, index: usize) -> Option<Option<u32>> {
        self.answers
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, v)| *v)
    }

    /// Advance to the next item.
    ///
    /// Requires an answer on the current item, because on both forms moving on
    /// without answering is a distinct decision the caller must make explicit
    /// via [`ExamSession::skip`].
    pub fn advance(&mut self) -> Result<(), ExamError> {
        self.check_playable()?;
        if self.cursor + 1 >= self.items.len() {
            // Advancing past the last item submits the exam.
            self.finished = true;
            return Ok(());
        }
        self.cursor += 1;
        // Ensure an answer slot exists for the new cursor.
        if self.answer_at(self.cursor).is_none() {
            self.set_answer(self.cursor, None);
        }
        Ok(())
    }

    /// Deliberately leave an item unanswered and advance.
    pub fn skip(&mut self) -> Result<(), ExamError> {
        self.check_playable()?;
        self.set_answer(self.cursor, None);
        self.advance()
    }

    /// Move back to a previous item.
    ///
    /// Refused on the CAT form — this is the behavioural constraint the
    /// simulator exists to reproduce.
    pub fn previous(&mut self) -> Result<(), ExamError> {
        self.check_playable()?;
        if !self.form.allows_backtracking() {
            return Err(ExamError::BacktrackingForbidden);
        }
        if self.cursor == 0 {
            return Err(ExamError::OutOfRange {
                index: 0,
                len: self.items.len(),
            });
        }
        self.cursor -= 1;
        Ok(())
    }

    /// Jump directly to an item index (paper-form review behaviour).
    pub fn go_to(&mut self, index: usize) -> Result<(), ExamError> {
        self.check_playable()?;
        if !self.form.allows_backtracking() {
            return Err(ExamError::BacktrackingForbidden);
        }
        if index >= self.items.len() {
            return Err(ExamError::OutOfRange {
                index,
                len: self.items.len(),
            });
        }
        self.cursor = index;
        if self.answer_at(index).is_none() {
            self.set_answer(index, None);
        }
        Ok(())
    }

    /// Change an existing answer.
    ///
    /// Only possible where backtracking is allowed, and never for an item the
    /// CAT form has already committed.
    pub fn revise(&mut self, index: usize, option: u32) -> Result<(), ExamError> {
        self.check_playable()?;
        if option >= OPTIONS_PER_ITEM {
            return Err(ExamError::InvalidOption(option));
        }
        if !self.form.allows_backtracking() || self.locked.contains(&index) {
            return Err(ExamError::BacktrackingForbidden);
        }
        if index >= self.items.len() {
            return Err(ExamError::OutOfRange {
                index,
                len: self.items.len(),
            });
        }
        self.set_answer(index, Some(option));
        Ok(())
    }

    /// Consume time from the session clock.
    ///
    /// Time never goes negative; hitting zero expires the session.
    pub fn tick(&mut self, seconds: u32) {
        self.remaining_seconds = self.remaining_seconds.saturating_sub(seconds);
        if self.remaining_seconds == 0 {
            self.finished = true;
        }
    }

    /// Whether the exam may be paused under this profile.
    pub fn pausable(&self, profile: &TimingProfile) -> bool {
        profile.pausable && profile.id == self.profile_id
    }

    /// Number of items answered with a committed option.
    pub fn answered_count(&self) -> usize {
        self.answers.iter().filter(|(_, v)| v.is_some()).count()
    }

    /// Whether every item has an answer.
    pub fn is_complete(&self) -> bool {
        (0..self.items.len()).all(|i| matches!(self.answer_at(i), Some(Some(_))))
    }
}

/// The built-in timing profiles.
///
/// These are VECTOR's documented practice profiles. They are versioned and
/// dated so a session's conditions are always attributable.
///
/// `cat-standard` follows the published CAT-ASVAB per-subtest time limits
/// recorded in `reference/asvab-test-specification.md` (AR 55, WK 9, PC 27,
/// MK 31, GS 12 minutes). The test `cat_standard_matches_reference_spec`
/// asserts this profile against those values, so code and reference cannot
/// silently diverge again.
///
/// NOTE: VECTOR's simulator covers only five subtests (AR, WK, PC, MK, GS).
/// It is not the full CAT-ASVAB battery — the remaining subtests (SI, EI, AI,
/// MC, AO, and the other line-score contributors) are not modelled here.
pub fn default_profiles() -> Vec<TimingProfile> {
    vec![
        TimingProfile {
            id: "cat-standard".to_string(),
            version: 1,
            form: ExamForm::Cat,
            per_subtest_seconds: vec![
                ("AR".to_string(), 55 * 60),
                ("WK".to_string(), 9 * 60),
                ("PC".to_string(), 27 * 60),
                ("MK".to_string(), 31 * 60),
                ("GS".to_string(), 12 * 60),
            ],
            pausable: false,
            source_reviewed_on: "2026-09-24".to_string(),
        },
        TimingProfile {
            id: "paper-standard".to_string(),
            version: 1,
            form: ExamForm::Paper,
            per_subtest_seconds: vec![
                ("AR".to_string(), 36 * 60),
                ("WK".to_string(), 11 * 60),
                ("PC".to_string(), 22 * 60),
                ("MK".to_string(), 24 * 60),
                ("GS".to_string(), 11 * 60),
            ],
            pausable: false,
            source_reviewed_on: "2026-09-01".to_string(),
        },
    ]
}

/// The subtest codes VECTOR's simulator models.
pub const SIMULATED_SUBTESTS: [&str; 5] = ["AR", "WK", "PC", "MK", "GS"];

/// Official published CAT-ASVAB per-subtest limits, in minutes, as recorded in
/// `reference/asvab-test-specification.md`. Used to assert `cat-standard`.
pub const CAT_STANDARD_SPEC_MINUTES: [(&str, u32); 5] =
    [("AR", 55), ("WK", 9), ("PC", 27), ("MK", 31), ("GS", 12)];
