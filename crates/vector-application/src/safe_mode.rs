//! Safe mode and crash-loop recovery (REQ-051).
//!
//! REQ-051 requires "crash-loop recovery without deleting learner data". The
//! tension is real: the usual fix for a crash loop is to reset state, but
//! resetting state destroys the learner's history. This module therefore
//! degrades capability rather than data.
//!
//! The rules enforced here:
//! - escalating failures narrow what the app attempts;
//! - learner data is never destroyed by entering safe mode;
//! - safe mode is exited only after the app has proven it can start, not merely
//!   because some time passed or a launch succeeded once.

use serde::{Deserialize, Serialize};

/// How the application is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RunMode {
    /// Full functionality.
    Normal,
    /// Optional features disabled after repeated crashes.
    Restricted,
    /// Minimum viable: core study functions only, no background work.
    Safe,
    /// Only diagnostics and data export; the study UI does not start.
    Recovery,
}

/// What a mode disables, expressed declaratively so the rule is testable
/// without launching anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feature {
    /// Background workers (scheduling, sync, maintenance).
    BackgroundWorkers,
    /// Local model inference.
    LocalModel,
    /// MCP server and client.
    Mcp,
    /// Content pack updates.
    ContentUpdates,
    /// Crash reporting upload.
    CrashUpload,
    /// The study interface itself.
    StudyInterface,
}

/// Whether a feature is enabled in a given mode.
pub fn feature_enabled(mode: RunMode, feature: Feature) -> bool {
    match mode {
        RunMode::Normal => true,
        RunMode::Restricted => !matches!(
            feature,
            Feature::BackgroundWorkers | Feature::ContentUpdates | Feature::CrashUpload
        ),
        RunMode::Safe => matches!(feature, Feature::StudyInterface),
        // Recovery mode exists to get the learner's data out; it does not run
        // the app.
        RunMode::Recovery => false,
    }
}

/// Number of consecutive failed launches before escalating.
pub const RESTRICTED_THRESHOLD: u32 = 2;
/// Number of consecutive failed launches before entering safe mode.
pub const SAFE_THRESHOLD: u32 = 4;
/// Number of consecutive failed launches before entering recovery mode.
pub const RECOVERY_THRESHOLD: u32 = 6;

/// The mode implied by a run of consecutive failed launches.
pub fn mode_for_failures(consecutive_failures: u32) -> RunMode {
    if consecutive_failures >= RECOVERY_THRESHOLD {
        RunMode::Recovery
    } else if consecutive_failures >= SAFE_THRESHOLD {
        RunMode::Safe
    } else if consecutive_failures >= RESTRICTED_THRESHOLD {
        RunMode::Restricted
    } else {
        RunMode::Normal
    }
}

/// Durable launch-health state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchHealth {
    /// Consecutive launches that failed before reaching readiness.
    pub consecutive_failures: u32,
    /// Successful launches completed since the last failure.
    pub consecutive_successes: u32,
    /// The mode the app should run in.
    pub mode: RunMode,
    /// Whether the learner has been told why the app is degraded.
    pub learner_informed: bool,
}

impl Default for LaunchHealth {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            consecutive_successes: 0,
            mode: RunMode::Normal,
            learner_informed: false,
        }
    }
}

/// Successful launches required before leaving safe mode.
///
/// One success is not enough: a crash loop often allows an occasional clean
/// start, so requiring a run of successes prevents flapping between modes.
pub const SUCCESSES_TO_RECOVER: u32 = 2;

impl LaunchHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a failed launch and escalate the mode.
    pub fn record_failure(&mut self) -> RunMode {
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.mode = mode_for_failures(self.consecutive_failures);
        if self.mode != RunMode::Normal {
            self.learner_informed = false;
        }
        self.mode
    }

    /// Record a successful launch.
    ///
    /// Recovery out of an escalated mode requires a run of successes. In
    /// `Recovery` mode the app does not start normally at all, so a single
    /// success only steps back to `Safe`.
    pub fn record_success(&mut self) -> RunMode {
        self.consecutive_successes += 1;
        self.consecutive_failures = 0;

        if self.mode != RunMode::Normal && self.consecutive_successes >= SUCCESSES_TO_RECOVER {
            // Step down one level rather than jumping straight to Normal, so a
            // partially-broken build is not immediately given full capability.
            self.mode = match self.mode {
                RunMode::Recovery => RunMode::Safe,
                RunMode::Safe => RunMode::Restricted,
                RunMode::Restricted => RunMode::Normal,
                RunMode::Normal => RunMode::Normal,
            };
            self.consecutive_successes = 0;
        }
        self.mode
    }

    /// Whether the learner must be shown an explanation of the degraded mode.
    pub fn needs_learner_notice(&self) -> bool {
        self.mode != RunMode::Normal && !self.learner_informed
    }

    /// Mark the learner as having been informed.
    pub fn mark_informed(&mut self) {
        self.learner_informed = true;
    }
}

/// The learner-data guarantee.
///
/// REQ-051 forbids destroying learner data to recover from a crash loop. This is
/// modelled as an explicit, checkable action classification so no recovery path
/// can quietly include a data-destroying step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Disable a feature for this run. Never destructive.
    DisableFeature(Feature),
    /// Reduce the log level or ring size. Never destructive.
    ReduceDiagnostics,
    /// Move a corrupt database aside for later inspection, preserving it.
    QuarantineDatabase,
    /// Delete the learner's attempts and mastery. DESTRUCTIVE.
    EraseLearnerData,
    /// Reinstall the application binaries. Not destructive to learner data.
    ReinstallApplication,
}

impl RecoveryAction {
    /// Whether this action destroys learner data.
    ///
    /// Only `EraseLearnerData` does, and it is never part of an automatic
    /// recovery ladder — it requires explicit learner action.
    pub fn destroys_learner_data(self) -> bool {
        matches!(self, RecoveryAction::EraseLearnerData)
    }
}

/// The automatic recovery ladder for a given failure count.
///
/// By construction this list contains no destructive action, which is the
/// REQ-051 guarantee expressed as testable data rather than as a promise.
pub fn recovery_ladder(consecutive_failures: u32) -> Vec<RecoveryAction> {
    let mut steps = Vec::new();

    if consecutive_failures >= RESTRICTED_THRESHOLD {
        steps.push(RecoveryAction::DisableFeature(Feature::BackgroundWorkers));
        steps.push(RecoveryAction::DisableFeature(Feature::ContentUpdates));
    }
    if consecutive_failures >= SAFE_THRESHOLD {
        steps.push(RecoveryAction::DisableFeature(Feature::LocalModel));
        steps.push(RecoveryAction::DisableFeature(Feature::Mcp));
    }
    if consecutive_failures >= RECOVERY_THRESHOLD {
        steps.push(RecoveryAction::DisableFeature(Feature::StudyInterface));
        steps.push(RecoveryAction::ReduceDiagnostics);
        // Quarantine preserves the database rather than deleting it.
        steps.push(RecoveryAction::QuarantineDatabase);
    }

    steps
}

/// Whether a recovery ladder is safe to apply automatically.
///
/// Returns the offending steps if any would destroy learner data.
pub fn ladder_is_non_destructive(ladder: &[RecoveryAction]) -> Result<(), Vec<RecoveryAction>> {
    let destructive: Vec<RecoveryAction> = ladder
        .iter()
        .copied()
        .filter(|a| a.destroys_learner_data())
        .collect();
    if destructive.is_empty() {
        Ok(())
    } else {
        Err(destructive)
    }
}
