//! EP-008 acceptance: repair broker workflow (REQ-031).
//!
//! CRASH_REPAIR_PIPELINE.md defines an eight-step order. The safety property is
//! the order itself: steps that could cause harm come after steps that reveal
//! it. These tests assert the ordering cannot be short-circuited.

use vector_repair::broker::{
    is_allowed_repair_transition, RepairCase, RepairError, RepairScope, RepairStage,
};

fn scope() -> RepairScope {
    RepairScope::default_scoped(
        vec![".agent/evidence/EP-008/bundle.json".to_string()],
        vec!["crates/vector-persistence/src/db.rs".to_string()],
    )
}

/// One pipeline step: an operation that advances the repair case.
type PipelineStep = fn(&mut RepairCase) -> Result<(), RepairError>;

/// Drive a case to the stage just before the one under test.
fn case_at(stage: RepairStage) -> RepairCase {
    let mut case = RepairCase::open("repair-1", scope());
    case.select_transporter("codex_native");

    let steps: &[PipelineStep] = &[
        |c| c.prepare_worktree(),
        |c| c.dispatch_agent(),
        |c| c.record_reproduction(),
        |c| c.propose_fix(),
        |c| c.record_evidence(&["sha256:abc".to_string()]),
        |c| c.preview_patch(),
        |c| c.grant_approval(),
    ];

    let order = [
        RepairStage::WorktreeReady,
        RepairStage::AgentDispatched,
        RepairStage::Reproduced,
        RepairStage::FixProposed,
        RepairStage::EvidenceRecorded,
        RepairStage::PatchPreviewed,
        RepairStage::ApprovalGranted,
    ];

    for (index, target) in order.iter().enumerate() {
        steps[index](&mut case).expect("pipeline step");
        if *target == stage {
            return case;
        }
    }
    case
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn the_pipeline_runs_in_the_documented_order() {
    let case = case_at(RepairStage::ApprovalGranted);
    assert_eq!(case.stage, RepairStage::ApprovalGranted);
    assert!(case.reproduced);
    assert!(case.previewed);
    assert!(case.approved);
    assert!(!case.evidence_hashes.is_empty());
}

#[test]
fn a_case_cannot_skip_straight_to_approval() {
    let mut case = RepairCase::open("repair-1", scope());
    case.select_transporter("codex_native");

    // The refusal names the missing precondition (the preview) rather than a
    // generic ordering error, which is more useful and still refuses.
    assert_eq!(case.grant_approval(), Err(RepairError::NotPreviewed));
    assert_ne!(case.stage, RepairStage::ApprovalGranted);
    assert!(!case.approved);
}

#[test]
fn any_stage_may_be_abandoned() {
    for stage in [
        RepairStage::AwaitingTransporterChoice,
        RepairStage::WorktreeReady,
        RepairStage::AgentDispatched,
        RepairStage::FixProposed,
        RepairStage::ApprovalGranted,
    ] {
        assert!(
            is_allowed_repair_transition(stage, RepairStage::Abandoned),
            "{stage:?} must be abandonable"
        );
    }
}

#[test]
fn an_unknown_transition_is_refused() {
    assert!(!is_allowed_repair_transition(
        RepairStage::AwaitingTransporterChoice,
        RepairStage::FixProposed
    ));
    assert!(!is_allowed_repair_transition(
        RepairStage::ApprovalGranted,
        RepairStage::Reproduced
    ));
}

// ---------------------------------------------------------------------------
// Preconditions
// ---------------------------------------------------------------------------

#[test]
fn a_worktree_requires_a_transporter_choice() {
    // Step 1 is the learner choosing what receives their data; skipping it would
    // send crash evidence somewhere unchosen.
    let mut case = RepairCase::open("repair-1", scope());
    assert_eq!(case.prepare_worktree(), Err(RepairError::NoTransporter));

    case.select_transporter("codex_native");
    assert!(case.prepare_worktree().is_ok());
}

#[test]
fn reproduction_must_precede_the_fix() {
    let mut case = RepairCase::open("repair-1", scope());
    case.select_transporter("codex_native");
    case.prepare_worktree().expect("worktree");
    case.dispatch_agent().expect("dispatch");

    assert_eq!(case.propose_fix(), Err(RepairError::NotReproduced));

    case.record_reproduction().expect("reproduce");
    assert!(case.propose_fix().is_ok());
}

#[test]
fn evidence_must_be_recorded_before_preview() {
    let mut case = RepairCase::open("repair-1", scope());
    case.select_transporter("codex_native");
    case.prepare_worktree().expect("worktree");
    case.dispatch_agent().expect("dispatch");
    case.record_reproduction().expect("reproduce");
    case.propose_fix().expect("fix");

    assert!(
        matches!(case.preview_patch(), Err(RepairError::OutOfOrder { .. })),
        "preview must not be reachable before evidence is recorded"
    );
}

#[test]
fn empty_evidence_is_refused() {
    let mut case = RepairCase::open("repair-1", scope());
    case.select_transporter("codex_native");
    case.prepare_worktree().expect("worktree");
    case.dispatch_agent().expect("dispatch");
    case.record_reproduction().expect("reproduce");
    case.propose_fix().expect("fix");

    assert_eq!(case.record_evidence(&[]), Err(RepairError::NoEvidence));
}

#[test]
fn approval_requires_a_preview() {
    // The learner must not approve a patch they have not seen.
    let mut case = RepairCase::open("repair-1", scope());
    case.select_transporter("codex_native");
    case.prepare_worktree().expect("worktree");
    case.dispatch_agent().expect("dispatch");
    case.record_reproduction().expect("reproduce");
    case.propose_fix().expect("fix");
    case.record_evidence(&["sha256:abc".to_string()])
        .expect("evidence");

    assert_eq!(case.grant_approval(), Err(RepairError::NotPreviewed));
}

// ---------------------------------------------------------------------------
// The no-auto-merge guarantee
// ---------------------------------------------------------------------------

#[test]
fn a_pull_request_requires_explicit_approval() {
    let mut case = RepairCase::open("repair-1", scope());
    assert_eq!(
        case.may_open_pull_request(),
        Err(RepairError::ApprovalRequired)
    );

    case.select_transporter("codex_native");
    case.prepare_worktree().expect("worktree");
    case.dispatch_agent().expect("dispatch");
    case.record_reproduction().expect("reproduce");
    case.propose_fix().expect("fix");
    case.record_evidence(&["sha256:abc".to_string()])
        .expect("evidence");
    case.preview_patch().expect("preview");

    assert_eq!(
        case.may_open_pull_request(),
        Err(RepairError::ApprovalRequired),
        "a previewed patch is still not approved"
    );

    case.grant_approval().expect("approve");
    assert!(case.may_open_pull_request().is_ok());
}

#[test]
fn the_broker_never_merges() {
    // CRASH_REPAIR_PIPELINE.md: "No default auto-merge or production deploy."
    // Asserted at every stage, including the fully-approved one.
    for stage in [
        RepairStage::AwaitingTransporterChoice,
        RepairStage::WorktreeReady,
        RepairStage::Reproduced,
        RepairStage::ApprovalGranted,
    ] {
        let case = case_at(stage);
        assert!(
            !case.may_merge(),
            "{stage:?} must never permit an automatic merge"
        );
    }
}

#[test]
fn an_abandoned_case_cannot_open_a_pull_request() {
    let mut case = RepairCase::open("repair-1", scope());
    case.stage = RepairStage::Abandoned;
    assert!(case.may_open_pull_request().is_err());
}

// ---------------------------------------------------------------------------
// Scope isolation
// ---------------------------------------------------------------------------

#[test]
fn the_default_scope_forbids_learner_data() {
    // THREAT_MODEL.md: the agent cannot read learner production databases.
    let scope = scope();
    assert!(
        !scope.may_read("vector.db"),
        "the learner database must never be readable by the repair agent"
    );
    assert!(!scope.may_read(".env"));
}

#[test]
fn the_default_scope_grants_rules_and_evidence() {
    let scope = scope();
    assert!(scope.may_read("AGENTS.md"));
    assert!(scope.may_read(".agent/DONE_LAW.md"));
    assert!(scope.may_read(".agent/evidence/EP-008/bundle.json"));
    assert!(scope.may_read("crates/vector-persistence/src/db.rs"));
}

#[test]
fn anything_not_granted_is_refused() {
    // Allowlist semantics: an unlisted path is not readable.
    let scope = scope();
    assert!(!scope.may_read("apps/desktop/src/App.tsx"));
    assert!(!scope.may_read("/etc/passwd"));
    assert!(!scope.may_read("../../secrets.txt"));
}

#[test]
fn an_explicitly_forbidden_path_stays_forbidden() {
    // Even if a caller adds it to a grant list, the forbidden set wins.
    let mut scope = scope();
    scope.affected_code.push("vector.db".to_string());
    assert!(
        !scope.may_read("vector.db"),
        "a forbidden path must not become readable by being granted"
    );
}

#[test]
fn wildcard_forbidden_patterns_apply() {
    let scope = scope();
    assert!(!scope.may_read(".agent/evidence/EP-003/livefire/live.db"));
    assert!(!scope.may_read(".agent/evidence/EP-003/livefire/nested/x.db"));
}

#[test]
fn windows_style_paths_are_matched() {
    let mut scope = scope();
    scope
        .affected_code
        .push("crates\\vector-persistence\\src\\db.rs".to_string());
    assert!(scope.may_read("crates/vector-persistence/src/db.rs"));
}

#[test]
fn a_scope_with_nothing_forbidden_is_refused() {
    // A scope that forbids nothing is almost certainly a mistake, and the
    // failure mode is exposing the learner database.
    let mut case = RepairCase::open(
        "repair-1",
        RepairScope {
            repository_rules: vec!["AGENTS.md".to_string()],
            crash_evidence: vec![],
            affected_code: vec![],
            forbidden_paths: vec![],
        },
    );
    case.select_transporter("codex_native");
    assert!(matches!(
        case.prepare_worktree(),
        Err(RepairError::ScopeViolation(_))
    ));
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[test]
fn the_broker_reports_its_own_health_on_a_real_basis() {
    let case = RepairCase::open("repair-1", scope());
    let report = case.health_report();
    assert!(
        report.validate().is_ok(),
        "the broker's health claim must rest on a real basis"
    );
    assert_eq!(report.components[0].component, "repair_broker");
}

#[test]
fn the_case_round_trips_through_serde() {
    let case = case_at(RepairStage::EvidenceRecorded);
    let json = serde_json::to_string(&case).expect("serialize");
    let back: RepairCase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(case, back);
}
