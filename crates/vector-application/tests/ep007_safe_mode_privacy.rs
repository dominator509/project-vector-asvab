//! EP-007 acceptance: safe mode and crash-loop recovery (REQ-051), telemetry
//! posture and prohibited screening data (REQ-052), external score typing
//! (REQ-055).

use vector_application::privacy::{
    category_is_collectible, is_prohibited_field, may_blend_into_readiness, may_collect,
    screen_profile_field, DataCategory, ScoreOrigin, ScoreRecord, TelemetryMode, TelemetryRefusal,
    PROHIBITED_SCREENING_FIELDS,
};
use vector_application::safe_mode::{
    feature_enabled, ladder_is_non_destructive, mode_for_failures, recovery_ladder, Feature,
    LaunchHealth, RecoveryAction, RunMode, RECOVERY_THRESHOLD, RESTRICTED_THRESHOLD,
    SAFE_THRESHOLD, SUCCESSES_TO_RECOVER,
};

// ---------------------------------------------------------------------------
// REQ-051: crash-loop recovery without deleting learner data
// ---------------------------------------------------------------------------

#[test]
fn a_healthy_launch_series_stays_in_normal_mode() {
    let mut health = LaunchHealth::new();
    for _ in 0..10 {
        health.record_success();
    }
    assert_eq!(health.mode, RunMode::Normal);
    assert!(!health.needs_learner_notice());
}

#[test]
fn repeated_failures_escalate_through_the_modes() {
    assert_eq!(mode_for_failures(0), RunMode::Normal);
    assert_eq!(mode_for_failures(1), RunMode::Normal);
    assert_eq!(mode_for_failures(RESTRICTED_THRESHOLD), RunMode::Restricted);
    assert_eq!(mode_for_failures(SAFE_THRESHOLD), RunMode::Safe);
    assert_eq!(mode_for_failures(RECOVERY_THRESHOLD), RunMode::Recovery);
    assert_eq!(mode_for_failures(100), RunMode::Recovery);
}

#[test]
fn escalation_is_monotonic_as_failures_accumulate() {
    let mut health = LaunchHealth::new();
    let mut seen = vec![health.mode];
    for _ in 0..RECOVERY_THRESHOLD {
        seen.push(health.record_failure());
    }
    for pair in seen.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "mode must never become less restricted as failures accumulate: {:?}",
            seen
        );
    }
    assert_eq!(health.mode, RunMode::Recovery);
}

#[test]
fn safe_mode_disables_optional_features_but_keeps_study_working() {
    // The point of safe mode is that the learner can still study.
    assert!(feature_enabled(RunMode::Safe, Feature::StudyInterface));
    for disabled in [
        Feature::BackgroundWorkers,
        Feature::LocalModel,
        Feature::Mcp,
        Feature::ContentUpdates,
        Feature::CrashUpload,
    ] {
        assert!(
            !feature_enabled(RunMode::Safe, disabled),
            "{disabled:?} must be disabled in safe mode"
        );
    }
}

#[test]
fn recovery_mode_does_not_run_the_study_interface() {
    // Recovery exists to get data out, so the app does not start normally.
    assert!(!feature_enabled(RunMode::Recovery, Feature::StudyInterface));
}

#[test]
fn restricted_mode_disables_only_background_work() {
    assert!(feature_enabled(
        RunMode::Restricted,
        Feature::StudyInterface
    ));
    assert!(feature_enabled(RunMode::Restricted, Feature::LocalModel));
    assert!(!feature_enabled(
        RunMode::Restricted,
        Feature::BackgroundWorkers
    ));
    assert!(!feature_enabled(RunMode::Restricted, Feature::CrashUpload));
}

#[test]
fn no_automatic_recovery_step_destroys_learner_data() {
    // This is the REQ-051 guarantee. It is checked across the whole escalation
    // range so no threshold can introduce a destructive step.
    for failures in 0..=(RECOVERY_THRESHOLD + 5) {
        let ladder = recovery_ladder(failures);
        assert!(
            ladder_is_non_destructive(&ladder).is_ok(),
            "the ladder at {failures} failures contains a destructive step: {ladder:?}"
        );
    }
}

#[test]
fn only_explicit_erasure_is_destructive() {
    assert!(RecoveryAction::EraseLearnerData.destroys_learner_data());
    for action in [
        RecoveryAction::DisableFeature(Feature::Mcp),
        RecoveryAction::ReduceDiagnostics,
        RecoveryAction::QuarantineDatabase,
        RecoveryAction::ReinstallApplication,
    ] {
        assert!(
            !action.destroys_learner_data(),
            "{action:?} must not be destructive"
        );
    }
}

#[test]
fn quarantining_a_database_preserves_it() {
    // The ladder moves a corrupt database aside rather than deleting it, so a
    // learner's history can be recovered later.
    let ladder = recovery_ladder(RECOVERY_THRESHOLD);
    assert!(
        ladder.contains(&RecoveryAction::QuarantineDatabase),
        "recovery must quarantine rather than delete: {ladder:?}"
    );
    assert!(!ladder.contains(&RecoveryAction::EraseLearnerData));
}

#[test]
fn the_ladder_escalates_with_failure_count() {
    let early = recovery_ladder(RESTRICTED_THRESHOLD);
    let late = recovery_ladder(RECOVERY_THRESHOLD);
    assert!(
        late.len() > early.len(),
        "more failures must disable more: {} vs {}",
        late.len(),
        early.len()
    );
    assert!(recovery_ladder(0).is_empty(), "no failures means no action");
}

#[test]
fn a_destructive_ladder_is_detected() {
    // Guards the guard: the checker must actually flag the bad case.
    let bad = vec![
        RecoveryAction::DisableFeature(Feature::Mcp),
        RecoveryAction::EraseLearnerData,
    ];
    let found = ladder_is_non_destructive(&bad).expect_err("must flag erasure");
    assert_eq!(found, vec![RecoveryAction::EraseLearnerData]);
}

#[test]
fn leaving_safe_mode_requires_a_run_of_successes() {
    // One clean start does not mean the loop is fixed; requiring a run prevents
    // flapping between modes.
    let mut health = LaunchHealth::new();
    for _ in 0..SAFE_THRESHOLD {
        health.record_failure();
    }
    assert_eq!(health.mode, RunMode::Safe);

    health.record_success();
    assert_eq!(
        health.mode,
        RunMode::Safe,
        "a single success must not restore capability"
    );

    health.record_success();
    assert_eq!(health.mode, RunMode::Restricted);
    assert_eq!(SUCCESSES_TO_RECOVER, 2);
}

#[test]
fn recovery_mode_steps_back_one_level_at_a_time() {
    let mut health = LaunchHealth::new();
    for _ in 0..RECOVERY_THRESHOLD {
        health.record_failure();
    }
    assert_eq!(health.mode, RunMode::Recovery);

    health.record_success();
    health.record_success();
    assert_eq!(
        health.mode,
        RunMode::Safe,
        "recovery must step down to Safe, not jump to Normal"
    );
}

#[test]
fn a_failure_resets_the_success_streak() {
    let mut health = LaunchHealth::new();
    health.record_success();
    health.record_success();
    assert_eq!(health.mode, RunMode::Normal);

    health.record_failure();
    assert_eq!(health.consecutive_successes, 0);
    assert_eq!(health.consecutive_failures, 1);
}

#[test]
fn a_success_resets_the_failure_streak() {
    let mut health = LaunchHealth::new();
    health.record_failure();
    health.record_failure();
    assert_eq!(health.mode, RunMode::Restricted);

    health.record_success();
    assert_eq!(health.consecutive_failures, 0);
}

#[test]
fn the_learner_is_told_when_the_app_is_degraded() {
    // Silently degrading would leave the learner wondering why features stopped.
    let mut health = LaunchHealth::new();
    health.record_failure();
    health.record_failure();
    assert!(health.needs_learner_notice());
    health.mark_informed();
    assert!(!health.needs_learner_notice());
}

#[test]
fn launch_health_round_trips_through_serde() {
    // Launch health is durable across restarts; that is the whole mechanism.
    let mut health = LaunchHealth::new();
    health.record_failure();
    health.record_failure();

    let json = serde_json::to_string(&health).expect("serialize");
    let back: LaunchHealth = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(health, back);
}

// ---------------------------------------------------------------------------
// REQ-052: telemetry off by default, no screening data
// ---------------------------------------------------------------------------

#[test]
fn telemetry_defaults_to_off() {
    // Collection that requires an action to disable is not consent.
    assert_eq!(TelemetryMode::default(), TelemetryMode::Off);
    assert!(!TelemetryMode::Off.transmits_remotely());
    assert!(!TelemetryMode::LocalOnly.transmits_remotely());
    assert!(TelemetryMode::RemoteOptIn.transmits_remotely());
}

#[test]
fn remote_transmission_requires_opt_in() {
    // "LocalOnly" means exactly that: local collection is permitted, remote is
    // not. The distinction is the mode, not the category.
    assert!(
        may_collect(TelemetryMode::Off, DataCategory::UsageCounts).is_err(),
        "with telemetry off, nothing is recorded"
    );
    assert!(
        may_collect(TelemetryMode::LocalOnly, DataCategory::UsageCounts).is_ok(),
        "local-only mode may keep usage counts on this machine"
    );
    assert!(may_collect(TelemetryMode::RemoteOptIn, DataCategory::UsageCounts).is_ok());
}

#[test]
fn remote_disabled_is_the_reason_reported_when_telemetry_is_off() {
    // The refusal must name the real cause, so a caller is not misled into
    // thinking the category itself is forbidden.
    assert_eq!(
        may_collect(TelemetryMode::Off, DataCategory::CrashDiagnostics),
        Err(TelemetryRefusal::RemoteDisabled)
    );
    assert_eq!(
        may_collect(TelemetryMode::Off, DataCategory::ScreeningData),
        Err(TelemetryRefusal::CategoryForbidden(
            DataCategory::ScreeningData
        )),
        "a forbidden category reports its own reason regardless of mode"
    );
}

#[test]
fn crash_diagnostics_may_be_kept_locally() {
    assert!(may_collect(TelemetryMode::LocalOnly, DataCategory::CrashDiagnostics).is_ok());
}

#[test]
fn answer_content_is_never_collected_as_telemetry() {
    // Answers are study data, not diagnostics, in every mode.
    for mode in [
        TelemetryMode::Off,
        TelemetryMode::LocalOnly,
        TelemetryMode::RemoteOptIn,
    ] {
        let result = may_collect(mode, DataCategory::AnswerContent);
        assert!(
            matches!(result, Err(TelemetryRefusal::CategoryForbidden(_))),
            "answer content must be refused in {mode:?}, got {result:?}"
        );
    }
}

#[test]
fn screening_data_and_secrets_are_never_collectible() {
    assert!(!category_is_collectible(DataCategory::ScreeningData));
    assert!(!category_is_collectible(DataCategory::Secrets));
    for mode in [
        TelemetryMode::Off,
        TelemetryMode::LocalOnly,
        TelemetryMode::RemoteOptIn,
    ] {
        assert!(may_collect(mode, DataCategory::ScreeningData).is_err());
        assert!(may_collect(mode, DataCategory::Secrets).is_err());
    }
}

#[test]
fn consent_cannot_enable_screening_data_collection() {
    // Consent cannot make the app an appropriate processor of these attributes.
    assert!(may_collect(TelemetryMode::RemoteOptIn, DataCategory::ScreeningData).is_err());
}

#[test]
fn enlistment_screening_fields_are_recognized_and_refused() {
    for field in PROHIBITED_SCREENING_FIELDS {
        assert!(is_prohibited_field(field), "{field} must be prohibited");
        assert!(
            screen_profile_field(field).is_err(),
            "{field} must be refused at the profile boundary"
        );
    }
}

#[test]
fn prohibitions_survive_field_name_variants() {
    // A caller renaming the field must not defeat the screen.
    for variant in [
        "medical-history",
        "medical history",
        "MEDICAL_HISTORY",
        "has_criminal_record",
        "my_credit_score",
    ] {
        assert!(
            is_prohibited_field(variant),
            "{variant} must be recognized as prohibited"
        );
    }
}

#[test]
fn ordinary_study_fields_are_permitted() {
    for field in [
        "subtest",
        "mastery",
        "target_score",
        "study_minutes",
        "learner_id",
        "created_at",
    ] {
        assert!(!is_prohibited_field(field), "{field} must be permitted");
        assert!(screen_profile_field(field).is_ok());
    }
}

// ---------------------------------------------------------------------------
// REQ-055: external scores are typed as external
// ---------------------------------------------------------------------------

#[test]
fn an_external_score_is_typed_as_external() {
    assert!(ScoreOrigin::UserEnteredOfficial.is_external());
    assert!(ScoreOrigin::ImportedOfficial.is_external());
    assert!(!ScoreOrigin::VectorEstimate.is_external());
}

#[test]
fn every_origin_carries_a_distinguishing_label() {
    let labels = [
        ScoreOrigin::VectorEstimate.required_label(),
        ScoreOrigin::UserEnteredOfficial.required_label(),
        ScoreOrigin::ImportedOfficial.required_label(),
    ];
    // The labels must actually differ, or "clearly typed as external" is unmet.
    let unique: std::collections::BTreeSet<&str> = labels.iter().copied().collect();
    assert_eq!(unique.len(), labels.len(), "labels must be distinguishable");

    for (origin, label) in [
        (ScoreOrigin::VectorEstimate, labels[0]),
        (ScoreOrigin::UserEnteredOfficial, labels[1]),
        (ScoreOrigin::ImportedOfficial, labels[2]),
    ] {
        assert!(
            !label.trim().is_empty(),
            "{origin:?} needs a non-empty label"
        );
    }
    // External labels must say so.
    assert!(ScoreOrigin::UserEnteredOfficial
        .required_label()
        .to_lowercase()
        .contains("official"));
    assert!(ScoreOrigin::VectorEstimate
        .required_label()
        .to_lowercase()
        .contains("not an official"));
}

#[test]
fn an_external_score_requires_provenance() {
    // An unattributed official score is indistinguishable from a VECTOR guess.
    let vague = ScoreRecord {
        origin: ScoreOrigin::UserEnteredOfficial,
        value: 72,
        recorded_on: "2026-09-10".to_string(),
        note: "   ".to_string(),
    };
    assert!(vague.validate().is_err());

    let documented = ScoreRecord {
        note: "AFQT from MEPS, reported 2026-09-01".to_string(),
        ..vague
    };
    assert!(documented.validate().is_ok());
}

#[test]
fn a_vector_estimate_needs_no_external_provenance() {
    let record = ScoreRecord {
        origin: ScoreOrigin::VectorEstimate,
        value: 65,
        recorded_on: "2026-09-10".to_string(),
        note: String::new(),
    };
    assert!(record.validate().is_ok());
}

#[test]
fn a_score_outside_the_reporting_scale_is_refused() {
    for value in [0u32, 100, 200] {
        let record = ScoreRecord {
            origin: ScoreOrigin::UserEnteredOfficial,
            value,
            recorded_on: "2026-09-10".to_string(),
            note: "report".to_string(),
        };
        assert!(record.validate().is_err(), "{value} is outside 1..99");
    }
}

#[test]
fn an_external_score_is_never_blended_into_readiness() {
    // Blending a real score into a practice estimate would imply the estimate is
    // calibrated against the official test. ADR-010 forbids that.
    assert!(!may_blend_into_readiness(ScoreOrigin::UserEnteredOfficial));
    assert!(!may_blend_into_readiness(ScoreOrigin::ImportedOfficial));
    assert!(may_blend_into_readiness(ScoreOrigin::VectorEstimate));
}

#[test]
fn a_score_record_must_state_when_it_was_entered() {
    let record = ScoreRecord {
        origin: ScoreOrigin::UserEnteredOfficial,
        value: 70,
        recorded_on: "  ".to_string(),
        note: "report".to_string(),
    };
    assert!(record.validate().is_err());
}

#[test]
fn the_display_label_comes_from_the_origin() {
    let record = ScoreRecord {
        origin: ScoreOrigin::ImportedOfficial,
        value: 80,
        recorded_on: "2026-09-10".to_string(),
        note: "official report".to_string(),
    };
    assert_eq!(
        record.display_label(),
        ScoreOrigin::ImportedOfficial.required_label()
    );
}

#[test]
fn a_score_record_round_trips_through_serde() {
    let record = ScoreRecord {
        origin: ScoreOrigin::UserEnteredOfficial,
        value: 72,
        recorded_on: "2026-09-10".to_string(),
        note: "AFQT from MEPS".to_string(),
    };
    let json = serde_json::to_string(&record).expect("serialize");
    let back: ScoreRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(record, back);
    assert!(
        back.origin.is_external(),
        "the external type must survive persistence"
    );
}
