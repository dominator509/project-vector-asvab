//! Repair broker: isolated worktree, gated patch, approved PR (REQ-031).
//!
//! Binding source: `CRASH_REPAIR_PIPELINE.md` section "Repair broker", and
//! `THREAT_MODEL.md` ("malicious repair agent patch").
//!
//! The pipeline's ordering is the safety property. Steps that could cause harm
//! come after steps that would reveal it:
//! 1. the learner reviews what leaves the machine and picks a transporter;
//! 2. repository state is verified and an **isolated** branch/worktree created;
//! 3. the agent receives only repository rules, scoped crash evidence and the
//!    affected-code map — never the learner's database or home directory;
//! 4. reproduction is attempted before any edit;
//! 5. the fix must add regression proof and run the gates;
//! 6. diff, commands, exits and evidence hashes are recorded;
//! 7. the learner previews the patch;
//! 8. `gh` may create an issue or PR **only after explicit approval**, and there
//!    is no default auto-merge.
//!
//! This module models the workflow as a state machine whose transitions encode
//! that ordering, so a caller cannot reach step 8 without having passed 1-7.

use serde::{Deserialize, Serialize};

use vector_observability::events::{ComponentHealth, HealthBasis, HealthReport, HealthState};

/// A coding-agent transporter the learner may choose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairTransporter {
    /// Stable adapter id, matching a ModelTransport where one exists.
    pub id: String,
    /// Whether the learner selected this transporter for this repair.
    pub selected: bool,
}

/// The scoped context handed to a repair agent.
///
/// The omission of learner data is the control. A repair agent needs to
/// understand a crash; it does not need the learner's history, and granting it
/// would make any agent compromise a privacy breach rather than a nuisance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairScope {
    /// Repository rule files the agent may read.
    pub repository_rules: Vec<String>,
    /// Evidence paths the agent may read.
    pub crash_evidence: Vec<String>,
    /// Source files the crash touches.
    pub affected_code: Vec<String>,
    /// Paths the agent must never read.
    pub forbidden_paths: Vec<String>,
}

impl RepairScope {
    /// The default scope: rules and evidence in, learner data out.
    pub fn default_scoped(crash_evidence: Vec<String>, affected_code: Vec<String>) -> Self {
        Self {
            repository_rules: vec!["AGENTS.md".to_string(), ".agent/DONE_LAW.md".to_string()],
            crash_evidence,
            affected_code,
            forbidden_paths: vec![
                "vector.db".to_string(),
                ".env".to_string(),
                ".agent/evidence/**/livefire/**".to_string(),
            ],
        }
    }

    /// Whether a path is readable under this scope.
    ///
    /// Anything not explicitly granted is refused, and forbidden paths are
    /// refused even if a caller adds them to a grant list.
    pub fn may_read(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        if self
            .forbidden_paths
            .iter()
            .any(|f| path_matches(&normalized, f))
        {
            return false;
        }
        self.repository_rules
            .iter()
            .chain(self.crash_evidence.iter())
            .chain(self.affected_code.iter())
            .any(|allowed| path_matches(&normalized, allowed))
    }
}

/// Match a path against a scope pattern, supporting a trailing `**` wildcard.
fn path_matches(path: &str, pattern: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    path == pattern
}

/// Stages of the repair pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairStage {
    /// 1. Learner reviews egress and chooses a transporter.
    AwaitingTransporterChoice,
    /// 2. Repository verified, isolated worktree created.
    WorktreeReady,
    /// 3. Agent scoped and dispatched.
    AgentDispatched,
    /// 4. Reproduction attempted.
    Reproduced,
    /// 5. Fix produced with regression proof.
    FixProposed,
    /// 6. Evidence recorded.
    EvidenceRecorded,
    /// 7. Learner previewed the patch.
    PatchPreviewed,
    /// 8. Explicit approval received; a PR may be opened.
    ApprovalGranted,
    /// Terminal: the repair was abandoned.
    Abandoned,
}

/// A repair case in progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairCase {
    pub id: String,
    pub stage: RepairStage,
    pub scope: RepairScope,
    pub transporter: RepairTransporter,
    /// Whether reproduction succeeded before the fix was written.
    pub reproduced: bool,
    /// Evidence hashes recorded for the diff, commands and exits.
    pub evidence_hashes: Vec<String>,
    /// Whether the learner previewed the patch.
    pub previewed: bool,
    /// Whether the learner explicitly approved opening a PR.
    pub approved: bool,
}

/// Why a repair transition was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairError {
    /// A step was attempted out of order.
    OutOfOrder { from: RepairStage, to: RepairStage },
    /// No transporter was selected.
    NoTransporter,
    /// Reproduction was not attempted before editing.
    NotReproduced,
    /// No evidence was recorded.
    NoEvidence,
    /// The learner has not previewed the patch.
    NotPreviewed,
    /// Explicit approval is required before a PR may be opened.
    ApprovalRequired,
    /// The scope would expose learner data.
    ScopeViolation(String),
}

impl std::fmt::Display for RepairError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepairError::OutOfOrder { from, to } => {
                write!(f, "cannot move from {from:?} to {to:?}")
            }
            RepairError::NoTransporter => {
                write!(f, "the learner has not chosen a repair transporter")
            }
            RepairError::NotReproduced => {
                write!(f, "the failure must be reproduced before a fix is proposed")
            }
            RepairError::NoEvidence => {
                write!(f, "the repair must record evidence hashes before review")
            }
            RepairError::NotPreviewed => {
                write!(f, "the learner must preview the patch before approval")
            }
            RepairError::ApprovalRequired => write!(
                f,
                "opening a pull request requires explicit approval; there is no \
                 auto-merge path"
            ),
            RepairError::ScopeViolation(p) => {
                write!(f, "the repair scope would expose {p}")
            }
        }
    }
}

impl std::error::Error for RepairError {}

/// Whether a transition follows the documented pipeline order.
pub fn is_allowed_repair_transition(from: RepairStage, to: RepairStage) -> bool {
    use RepairStage::*;
    matches!(
        (from, to),
        (AwaitingTransporterChoice, WorktreeReady)
            | (WorktreeReady, AgentDispatched)
            | (AgentDispatched, Reproduced)
            | (Reproduced, FixProposed)
            | (FixProposed, EvidenceRecorded)
            | (EvidenceRecorded, PatchPreviewed)
            | (PatchPreviewed, ApprovalGranted)
    ) || to == Abandoned
}

impl RepairCase {
    /// Open a new repair case. It begins before any transporter is chosen.
    pub fn open(id: &str, scope: RepairScope) -> Self {
        Self {
            id: id.to_string(),
            stage: RepairStage::AwaitingTransporterChoice,
            scope,
            transporter: RepairTransporter {
                id: String::new(),
                selected: false,
            },
            reproduced: false,
            evidence_hashes: Vec::new(),
            previewed: false,
            approved: false,
        }
    }

    /// Step 1: the learner selects a transporter.
    pub fn select_transporter(&mut self, id: &str) {
        self.transporter = RepairTransporter {
            id: id.to_string(),
            selected: true,
        };
    }

    /// Step 2: verify repository state and create the isolated worktree.
    pub fn prepare_worktree(&mut self) -> Result<(), RepairError> {
        if !self.transporter.selected {
            return Err(RepairError::NoTransporter);
        }
        if self.scope.forbidden_paths.is_empty() {
            // A scope with nothing forbidden is almost certainly a mistake; the
            // default always forbids the learner database.
            return Err(RepairError::ScopeViolation("vector.db".to_string()));
        }
        self.advance(RepairStage::WorktreeReady)
    }

    /// Step 3: dispatch the scoped agent.
    pub fn dispatch_agent(&mut self) -> Result<(), RepairError> {
        self.advance(RepairStage::AgentDispatched)
    }

    /// Step 4: record that the failure was reproduced.
    pub fn record_reproduction(&mut self) -> Result<(), RepairError> {
        self.advance(RepairStage::Reproduced)?;
        self.reproduced = true;
        Ok(())
    }

    /// Step 5: propose a fix. Refused unless reproduction happened first.
    pub fn propose_fix(&mut self) -> Result<(), RepairError> {
        if !self.reproduced {
            return Err(RepairError::NotReproduced);
        }
        self.advance(RepairStage::FixProposed)
    }

    /// Step 6: record evidence hashes.
    pub fn record_evidence(&mut self, hashes: &[String]) -> Result<(), RepairError> {
        if hashes.is_empty() {
            return Err(RepairError::NoEvidence);
        }
        self.advance(RepairStage::EvidenceRecorded)?;
        self.evidence_hashes = hashes.to_vec();
        Ok(())
    }

    /// Step 7: the learner previews the patch.
    pub fn preview_patch(&mut self) -> Result<(), RepairError> {
        self.advance(RepairStage::PatchPreviewed)?;
        self.previewed = true;
        Ok(())
    }

    /// Step 8: grant approval to open a PR.
    ///
    /// Requires the preview. The pipeline puts preview before approval so a
    /// learner cannot approve a patch they have not seen.
    pub fn grant_approval(&mut self) -> Result<(), RepairError> {
        if !self.previewed {
            return Err(RepairError::NotPreviewed);
        }
        self.advance(RepairStage::ApprovalGranted)?;
        self.approved = true;
        Ok(())
    }

    fn advance(&mut self, to: RepairStage) -> Result<(), RepairError> {
        if !is_allowed_repair_transition(self.stage, to) {
            return Err(RepairError::OutOfOrder {
                from: self.stage,
                to,
            });
        }
        self.stage = to;
        Ok(())
    }

    /// Whether the broker may open an issue or pull request.
    ///
    /// This is the single gate the GitHub integration must consult. There is no
    /// parameter, flag or environment variable that can bypass it, which is what
    /// makes "no default auto-merge" a property rather than a setting.
    pub fn may_open_pull_request(&self) -> Result<(), RepairError> {
        if self.stage != RepairStage::ApprovalGranted || !self.approved {
            return Err(RepairError::ApprovalRequired);
        }
        if !self.previewed {
            return Err(RepairError::NotPreviewed);
        }
        if self.evidence_hashes.is_empty() {
            return Err(RepairError::NoEvidence);
        }
        Ok(())
    }

    /// Whether the broker may merge a pull request.
    ///
    /// Always false. CRASH_REPAIR_PIPELINE.md: "No default auto-merge or
    /// production deploy." Merging is a human act performed in the forge.
    pub fn may_merge(&self) -> bool {
        false
    }

    /// Health of the repair subsystem itself.
    pub fn health(&self) -> ComponentHealth {
        ComponentHealth {
            component: "repair_broker".to_string(),
            state: if self.approved {
                HealthState::Healthy
            } else {
                HealthState::Disabled {
                    reason: "no approved repair in progress".to_string(),
                }
            },
            // The stage value is inspected directly rather than probing a
            // process, which is a real state read.
            basis: HealthBasis::OperationSucceeded,
        }
    }

    /// A health report covering this case.
    pub fn health_report(&self) -> HealthReport {
        HealthReport {
            components: vec![self.health()],
        }
    }
}
