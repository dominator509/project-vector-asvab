//! EP-004 acceptance: offline-core guarantee (REQ-042).

use vector_application::offline::{
    builtin_capabilities, core_functions_all_offline, require_capability, Capability, NetworkState,
    OfflineError, OfflineSupport,
};

#[test]
fn every_core_study_function_works_offline() {
    // This is REQ-042 stated as an executable assertion: all core study
    // functions must be usable with the network disabled.
    let capabilities = builtin_capabilities();

    core_functions_all_offline(&capabilities)
        .expect("REQ-042: every core study function must be available offline");

    let core: Vec<&Capability> = capabilities.iter().filter(|c| c.core).collect();
    assert!(
        core.len() >= 10,
        "expected the documented core study surface, found {}",
        core.len()
    );
    for capability in core {
        assert_eq!(
            capability.offline,
            OfflineSupport::Available,
            "core function {} must be fully available offline",
            capability.id
        );
    }
}

#[test]
fn the_documented_core_functions_are_all_present() {
    // Named so that removing a core function from the app is caught here.
    let ids: Vec<String> = builtin_capabilities().into_iter().map(|c| c.id).collect();
    for required in [
        "diagnostic",
        "practice",
        "review_schedule",
        "study_plan",
        "exam_simulation",
        "mastery_tracking",
        "readiness_band",
        "evidence_lookup",
        "notebook_export",
        "local_ai",
    ] {
        assert!(
            ids.contains(&required.to_string()),
            "core study function {required} is missing"
        );
    }
}

#[test]
fn core_functions_are_usable_while_offline() {
    for id in [
        "diagnostic",
        "practice",
        "review_schedule",
        "study_plan",
        "exam_simulation",
        "readiness_band",
        "local_ai",
    ] {
        let result = require_capability(id, NetworkState::Offline);
        assert_eq!(
            result,
            Ok(OfflineSupport::Available),
            "{id} must be usable offline, got {result:?}"
        );
    }
}

#[test]
fn a_network_capability_is_refused_while_offline() {
    let result = require_capability("native_ai", NetworkState::Offline);
    assert_eq!(
        result,
        Err(OfflineError::RequiresNetwork("native_ai".to_string())),
        "a non-core network capability must be refused offline"
    );
}

#[test]
fn an_unavailable_core_function_is_reported_as_a_guarantee_violation() {
    // The runtime guard must distinguish "this non-core feature needs network"
    // from "a core study function is broken offline". Collapsing the two would
    // hide a REQ-042 violation behind an ordinary refusal, so the error kind
    // itself is asserted here rather than only the fact of failure.
    let violation = require_capability_probe("practice", OfflineSupport::Unavailable, true);
    assert_eq!(
        violation,
        Err(OfflineError::CoreUnavailableOffline("practice".to_string())),
        "an offline-unavailable CORE function must report a guarantee violation"
    );

    // The same support level for a non-core capability is an ordinary refusal.
    let ordinary = require_capability_probe("update_check", OfflineSupport::Unavailable, false);
    assert_eq!(
        ordinary,
        Err(OfflineError::RequiresNetwork("update_check".to_string())),
        "a non-core network capability is a plain refusal, not a violation"
    );
}

/// Exercise the guard's classification logic directly, so the core-violation
/// branch is reachable in a test without mutating the builtin table.
fn require_capability_probe(
    id: &str,
    offline: OfflineSupport,
    core: bool,
) -> Result<OfflineSupport, OfflineError> {
    vector_application::offline::classify_offline(id, offline, core, NetworkState::Offline)
}

#[test]
fn a_degraded_capability_declares_its_limitation() {
    // Serving stale sources is acceptable only if the app says so.
    let result = require_capability("source_refresh", NetworkState::Offline);
    match result {
        Ok(OfflineSupport::Degraded { note }) => {
            assert!(
                note.to_lowercase().contains("stale") || note.to_lowercase().contains("cache"),
                "a degraded mode must explain what is reduced, got {note:?}"
            );
        }
        other => panic!("expected a declared degraded mode, got {other:?}"),
    }
}

#[test]
fn a_network_capability_is_available_while_online() {
    assert_eq!(
        require_capability("native_ai", NetworkState::Online),
        Ok(OfflineSupport::Unavailable),
        "online, the capability exists but is classified as network-only"
    );
    assert_eq!(
        require_capability("update_check", NetworkState::Online),
        Ok(OfflineSupport::Unavailable)
    );
}

#[test]
fn an_unknown_capability_is_refused() {
    let result = require_capability("sudo_wipe_disk", NetworkState::Online);
    assert!(
        result.is_err(),
        "an unregistered capability must never be treated as available"
    );
}

#[test]
fn a_core_function_marked_unavailable_offline_is_reported() {
    // Guards the guard: if someone ever marks a core function Unavailable, the
    // check must fail loudly rather than silently downgrade the guarantee.
    let broken = vec![Capability {
        id: "practice".to_string(),
        label: "Practice questions".to_string(),
        offline: OfflineSupport::Unavailable,
        core: true,
    }];

    let result = core_functions_all_offline(&broken);
    assert_eq!(
        result,
        Err(vec!["practice".to_string()]),
        "an offline-unavailable core function must be reported"
    );
}

#[test]
fn a_degraded_core_function_also_violates_the_guarantee() {
    // "Degraded" is not "available": a core function that loses functionality
    // offline does not satisfy REQ-042.
    let degraded = vec![Capability {
        id: "exam_simulation".to_string(),
        label: "Exam simulation".to_string(),
        offline: OfflineSupport::Degraded {
            note: "no timer".to_string(),
        },
        core: true,
    }];

    assert!(
        core_functions_all_offline(&degraded).is_err(),
        "a degraded core function does not satisfy the offline guarantee"
    );
}

#[test]
fn non_core_capabilities_are_explicitly_not_core() {
    let capabilities = builtin_capabilities();
    for id in ["native_ai", "source_refresh", "update_check"] {
        let capability = capabilities
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} must be registered"));
        assert!(
            !capability.core,
            "{id} must not be classified as a core study function"
        );
    }
}

#[test]
fn capability_ids_are_unique() {
    let mut ids: Vec<String> = builtin_capabilities().into_iter().map(|c| c.id).collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "capability ids must be unique");
}

#[test]
fn every_capability_has_a_human_readable_label() {
    for capability in builtin_capabilities() {
        assert!(
            !capability.label.trim().is_empty(),
            "{} needs a learner-facing label",
            capability.id
        );
    }
}
