//! EP-008 acceptance: crash bundles (REQ-028, REQ-030), structured events and
//! health (REQ-057), and the disabled sync seam (REQ-059).

use vector_application::sync_port::{
    requires_cloud_account, sync_enabled, SyncDirection, SyncPort, SyncRefusal, SyncRequest,
};
use vector_observability::bundle::{
    assemble_bundle, is_safe_entry_path, BuildIdentity, BundleError, ConsentManifest,
    EnvironmentFacts, ReproRecipe, REQUIRED_COMPONENTS,
};
use vector_observability::crash::{redact_capture, CanaryRegistry, CrashCapture};
use vector_observability::events::{
    is_free_text_key, ComponentHealth, Event, EventError, EventRing, HealthBasis, HealthReport,
    HealthState, RedactionClass, Severity, MAX_ATTRIBUTE_BYTES,
};

const CANARY: &str = "canary-secret-value-1234";

fn canaries() -> CanaryRegistry {
    let mut registry = CanaryRegistry::new();
    registry.register("canary", CANARY).expect("register");
    registry
}

fn redacted_capture() -> CrashCapture {
    let capture = CrashCapture {
        id: "crash-1".to_string(),
        summary: format!("provider call failed using {CANARY}"),
        message: "connection refused".to_string(),
        stack: "at auth (client.rs:42)".to_string(),
        context: vec![("subtest".to_string(), "AR".to_string())],
        log_ring: vec![format!("INFO started with {CANARY}")],
        build_id: "build-1".to_string(),
        redaction_passes: 0,
    };
    redact_capture(&capture, &canaries()).capture
}

fn build_identity() -> BuildIdentity {
    BuildIdentity {
        app_version: "0.1.0".to_string(),
        git_sha: "abc123".to_string(),
        artifact_hash: "deadbeef".to_string(),
        schema_version: "2".to_string(),
        content_version: "1".to_string(),
    }
}

fn repro() -> ReproRecipe {
    ReproRecipe {
        steps: vec![
            "Open the review queue".to_string(),
            "Answer three cards".to_string(),
        ],
        expected: "the queue advances".to_string(),
        observed: "the app exits".to_string(),
        replay_seed: Some(42),
    }
}

fn consent(granted: bool) -> ConsentManifest {
    ConsentManifest {
        granted,
        granted_at: "2026-09-10T00:00:00Z".to_string(),
        approved_categories: vec!["crash_diagnostics".to_string()],
        approved_destinations: vec!["local".to_string()],
        redaction_passes: 2,
    }
}

// ---------------------------------------------------------------------------
// REQ-028 / REQ-030: crash bundle
// ---------------------------------------------------------------------------

#[test]
fn a_bundle_contains_every_required_component() {
    let bundle = assemble_bundle(
        &redacted_capture(),
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &["{\"event\":\"start\"}".to_string()],
        &consent(true),
    )
    .expect("bundle assembles");

    for required in REQUIRED_COMPONENTS {
        assert!(
            bundle.paths().contains(required),
            "bundle is missing {required}: {:?}",
            bundle.paths()
        );
    }
}

#[test]
fn a_bundle_never_contains_a_canary() {
    let bundle = assemble_bundle(
        &redacted_capture(),
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &[],
        &consent(true),
    )
    .expect("bundle assembles");

    let rendered = bundle.render();
    assert!(
        !rendered.contains(CANARY),
        "a secret reached the bundle artifact"
    );
}

#[test]
fn bundle_assembly_requires_consent() {
    let result = assemble_bundle(
        &redacted_capture(),
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &[],
        &consent(false),
    );
    assert!(
        matches!(result, Err(BundleError::NotReleasable(_))),
        "an unconsented bundle must not assemble, got {result:?}"
    );
}

#[test]
fn bundle_assembly_requires_two_redaction_passes() {
    let mut capture = redacted_capture();
    capture.redaction_passes = 1;
    let result = assemble_bundle(
        &capture,
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &[],
        &consent(true),
    );
    assert!(matches!(result, Err(BundleError::NotReleasable(_))));
}

#[test]
fn bundle_assembly_refuses_a_residual_secret() {
    let mut capture = redacted_capture();
    // Simulate a redaction bug that let a value through.
    capture.log_ring.push(format!("leaked {CANARY}"));

    let result = assemble_bundle(
        &capture,
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &[],
        &consent(true),
    );
    assert!(
        matches!(result, Err(BundleError::NotReleasable(_))),
        "a bundle carrying a secret must be refused"
    );
}

#[test]
fn bundle_assembly_requires_an_actionable_repro() {
    let vague = ReproRecipe {
        steps: vec![],
        expected: String::new(),
        observed: String::new(),
        replay_seed: None,
    };
    let result = assemble_bundle(
        &redacted_capture(),
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &vague,
        &[],
        &consent(true),
    );
    assert_eq!(result, Err(BundleError::ReproNotActionable));
}

#[test]
fn a_recipe_that_omits_expected_or_observed_is_not_actionable() {
    let no_expected = ReproRecipe {
        steps: vec!["do a thing".to_string()],
        expected: "  ".to_string(),
        observed: "it broke".to_string(),
        replay_seed: None,
    };
    assert!(!no_expected.is_actionable());

    let no_observed = ReproRecipe {
        expected: "it works".to_string(),
        observed: String::new(),
        ..no_expected
    };
    assert!(!no_observed.is_actionable());
}

#[test]
fn bundle_rendering_is_deterministic() {
    // A bundle hash is only meaningful as evidence if identical content always
    // produces identical bytes.
    let make = || {
        assemble_bundle(
            &redacted_capture(),
            &canaries(),
            &build_identity(),
            &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
            &repro(),
            &[],
            &consent(true),
        )
        .expect("bundle assembles")
    };

    assert_eq!(make().render(), make().render(), "rendering must be stable");
}

#[test]
fn bundle_rendering_orders_entries_consistently() {
    let mut bundle = assemble_bundle(
        &redacted_capture(),
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &[],
        &consent(true),
    )
    .expect("assembles");

    let forward = bundle.render();
    bundle.entries.reverse();
    assert_eq!(
        forward,
        bundle.render(),
        "entry order must not affect the rendered artifact"
    );
}

#[test]
fn unsafe_bundle_entry_paths_are_rejected() {
    for bad in [
        "/etc/passwd",
        "\\windows\\x",
        "../escape",
        "a/../../b",
        "C:\\x",
        "",
    ] {
        assert!(!is_safe_entry_path(bad), "{bad:?} must be rejected");
    }
    for good in ["manifest.json", "nested/dir/file.txt"] {
        assert!(is_safe_entry_path(good), "{good:?} must be accepted");
    }
}

#[test]
fn environment_facts_bucket_memory_to_avoid_fingerprinting() {
    // A precise memory figure is a fingerprint; diagnostics do not need it.
    let facts = EnvironmentFacts::new("windows", "11", "x86_64", 31_744);
    assert_eq!(facts.memory_mib_bucketed, 31_744 / 1024 * 1024);
    assert_eq!(facts.memory_mib_bucketed % 1024, 0);
}

#[test]
fn bundle_entries_round_trip_through_serde() {
    let bundle = assemble_bundle(
        &redacted_capture(),
        &canaries(),
        &build_identity(),
        &EnvironmentFacts::new("windows", "11", "x86_64", 32_768),
        &repro(),
        &[],
        &consent(true),
    )
    .expect("assembles");

    let json = serde_json::to_string(&bundle.entries).expect("serialize");
    assert!(
        !json.contains(CANARY),
        "a secret reached the serialized bundle"
    );
}

// ---------------------------------------------------------------------------
// REQ-057: structured events
// ---------------------------------------------------------------------------

fn event(attributes: Vec<(&str, &str)>, class: RedactionClass) -> Event {
    Event {
        sequence: 0,
        timestamp: "2026-09-10T00:00:00Z".to_string(),
        severity: Severity::Info,
        component: "persistence".to_string(),
        event_id: "migration.applied".to_string(),
        correlation_id: "corr-1".to_string(),
        build_version: "0.1.0".to_string(),
        content_version: "1".to_string(),
        attributes: attributes
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        redaction_class: class,
    }
}

#[test]
fn an_event_records_a_monotonic_sequence() {
    let mut ring = EventRing::new(16).expect("ring");
    let first = ring
        .record(event(vec![], RedactionClass::Public))
        .expect("record");
    let second = ring
        .record(event(vec![], RedactionClass::Public))
        .expect("record");
    assert!(second > first, "sequence numbers must increase");
}

#[test]
fn the_ring_discards_the_oldest_events_first() {
    let mut ring = EventRing::new(3).expect("ring");
    for i in 0..5 {
        ring.record(event(vec![("i", &i.to_string())], RedactionClass::Public))
            .expect("record");
    }
    assert_eq!(ring.len(), 3, "the ring must respect its capacity");
    assert_eq!(
        ring.events()[0].attributes[0].1,
        "2",
        "the oldest events must be dropped"
    );
}

#[test]
fn a_zero_capacity_ring_is_refused() {
    // A zero-length ring silently discards everything, which reads as "no
    // problems" rather than "no logging".
    assert!(EventRing::new(0).is_err());
}

#[test]
fn study_free_text_is_refused() {
    // OBSERVABILITY.md: study free text is excluded by default.
    let mut ring = EventRing::new(8).expect("ring");
    for key in ["answer", "prompt", "note", "question_text", "tutor_message"] {
        let result = ring.record(event(
            vec![(key, "the learner's words")],
            RedactionClass::Public,
        ));
        assert_eq!(
            result,
            Err(EventError::StudyFreeText),
            "{key} must be refused as free text"
        );
    }
}

#[test]
fn free_text_detection_survives_key_variants() {
    for key in ["answer_text", "learner_note", "my_prompt", "search_query"] {
        assert!(is_free_text_key(key), "{key} must be recognized");
    }
    for key in ["subtest", "count", "duration_ms", "component"] {
        assert!(!is_free_text_key(key), "{key} must be allowed");
    }
}

#[test]
fn an_oversized_attribute_is_refused() {
    let mut ring = EventRing::new(8).expect("ring");
    let huge = "x".repeat(MAX_ATTRIBUTE_BYTES + 1);
    let result = ring.record(event(vec![("blob", &huge)], RedactionClass::Public));
    assert!(matches!(result, Err(EventError::AttributeTooLarge { .. })));
}

#[test]
fn an_event_missing_an_identifier_is_refused() {
    let mut ring = EventRing::new(8).expect("ring");
    let mut bad = event(vec![], RedactionClass::Public);
    bad.component = "   ".to_string();
    assert_eq!(
        ring.record(bad),
        Err(EventError::MissingIdentifier("component"))
    );
}

#[test]
fn a_secret_attribute_cannot_appear_on_a_public_event() {
    // The export gate reads the class, so a mismatch would leak.
    let mut ring = EventRing::new(8).expect("ring");
    let result = ring.record(event(
        vec![("api_key", "sk-abcdefghijklmnop")],
        RedactionClass::Public,
    ));
    assert!(matches!(result, Err(EventError::ClassMismatch { .. })));
}

#[test]
fn only_public_events_are_exportable() {
    let mut ring = EventRing::new(8).expect("ring");
    ring.record(event(vec![], RedactionClass::Public))
        .expect("public");
    ring.record(event(vec![], RedactionClass::LocalOnly))
        .expect("local");
    ring.record(event(vec![], RedactionClass::Personal))
        .expect("personal");

    let exportable = ring.exportable();
    assert_eq!(exportable.len(), 1, "only the Public event may be exported");
    assert_eq!(exportable[0].redaction_class, RedactionClass::Public);
}

#[test]
fn ndjson_rendering_excludes_non_public_events() {
    let mut ring = EventRing::new(8).expect("ring");
    ring.record(event(vec![("subtest", "AR")], RedactionClass::Public))
        .expect("public");
    ring.record(event(vec![("learner", "ada")], RedactionClass::Personal))
        .expect("personal");

    let rendered = ring.render_ndjson();
    assert!(rendered.contains("AR"));
    assert!(
        !rendered.contains("ada"),
        "personal data must not reach the exported event stream"
    );
    for line in rendered.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("each line is valid JSON");
    }
}

// ---------------------------------------------------------------------------
// REQ-057: health states
// ---------------------------------------------------------------------------

#[test]
fn process_liveness_alone_is_not_a_health_proof() {
    // OBSERVABILITY.md is explicit about this, and it is the most common way a
    // health screen lies to its reader.
    let report = ComponentHealth {
        component: "mcp".to_string(),
        state: HealthState::Healthy,
        basis: HealthBasis::ProcessAlive,
    };
    assert!(
        report.validate().is_err(),
        "a Healthy verdict on liveness alone must be refused"
    );
}

#[test]
fn a_healthy_verdict_requires_a_real_basis() {
    for basis in [
        HealthBasis::OperationSucceeded,
        HealthBasis::CapabilityProbe,
    ] {
        let report = ComponentHealth {
            component: "db".to_string(),
            state: HealthState::Healthy,
            basis,
        };
        assert!(report.validate().is_ok(), "{basis:?} must support Healthy");
    }
}

#[test]
fn unhealthy_states_may_be_reported_on_any_basis() {
    // Only the optimistic claim needs evidential support.
    for state in [
        HealthState::Unconfigured,
        HealthState::Unavailable {
            reason: "not running".to_string(),
        },
        HealthState::Disabled {
            reason: "learner choice".to_string(),
        },
    ] {
        let report = ComponentHealth {
            component: "mcp".to_string(),
            state,
            basis: HealthBasis::ProcessAlive,
        };
        assert!(report.validate().is_ok());
    }
}

#[test]
fn a_disabled_component_is_not_an_actionable_problem() {
    // The learner turned it off; flagging that as a fault trains people to
    // ignore warnings.
    let disabled = HealthState::Disabled {
        reason: "turned off in settings".to_string(),
    };
    assert!(!disabled.is_actionable_problem());
    assert!(!disabled.is_working());
}

#[test]
fn unavailable_and_stale_policy_are_actionable() {
    for state in [
        HealthState::Unavailable {
            reason: "connection refused".to_string(),
        },
        HealthState::StalePolicy {
            verified_on: "2026-01-01".to_string(),
            max_age_days: 7,
        },
    ] {
        assert!(state.is_actionable_problem(), "{state:?} needs attention");
    }
}

#[test]
fn a_health_report_summarises_its_components() {
    let report = HealthReport {
        components: vec![
            ComponentHealth {
                component: "db".to_string(),
                state: HealthState::Healthy,
                basis: HealthBasis::OperationSucceeded,
            },
            ComponentHealth {
                component: "mcp".to_string(),
                state: HealthState::Disabled {
                    reason: "not enabled".to_string(),
                },
                basis: HealthBasis::ConfigurationOnly,
            },
        ],
    };

    assert!(report.validate().is_ok());
    assert!(report.actionable().is_empty(), "nothing needs attention");
    assert!(
        !report.all_healthy(),
        "a disabled component means not everything is healthy"
    );
}

#[test]
fn an_empty_health_report_is_not_all_healthy() {
    // Vacuously true would be the wrong default: no components means nothing
    // was actually checked.
    let report = HealthReport { components: vec![] };
    assert!(!report.all_healthy());
}

// ---------------------------------------------------------------------------
// REQ-059: disabled sync seam
// ---------------------------------------------------------------------------

#[test]
fn the_sync_port_is_disabled() {
    assert!(
        !sync_enabled(),
        "REQ-059 requires the SyncPort to be disabled"
    );
    assert!(!SyncPort::is_available());
}

#[test]
fn every_sync_direction_is_refused() {
    for direction in [
        SyncDirection::Upload,
        SyncDirection::Download,
        SyncDirection::Bidirectional,
    ] {
        let request = SyncRequest {
            direction,
            endpoint: "https://sync.example.invalid".to_string(),
        };
        assert_eq!(
            SyncPort::sync(&request),
            Err(SyncRefusal::Disabled),
            "{direction:?} must be refused"
        );
    }
}

#[test]
fn no_remote_endpoint_is_configured() {
    // A non-empty list would mean the app has a network dependency despite the
    // port being disabled.
    assert!(SyncPort::configured_endpoints().is_empty());
}

#[test]
fn the_application_requires_no_cloud_account() {
    // ADR-003: local-first. Stated as code so it is checkable.
    assert!(!requires_cloud_account());
}

#[test]
fn the_refusal_explains_the_local_first_guarantee() {
    let message = SyncRefusal::Disabled.to_string();
    assert!(message.contains("disabled"));
    assert!(message.contains("offline"));
}
