//! EP-006 acceptance: egress privacy classification and the no-ads guarantee
//! (REQ-046).

use vector_application::egress::{
    authorize_egress, behavioral_advertising_enabled, data_brokerage_enabled, default_destinations,
    DataClass, Destination, EgressPurpose, EgressRefusal, EgressRequest,
};

fn dest(id: &str, consented: bool, advertising: bool) -> Destination {
    Destination {
        id: id.to_string(),
        host: format!("{id}.example.invalid"),
        consented,
        is_advertising: advertising,
    }
}

fn request(
    destination: Destination,
    purpose: EgressPurpose,
    data_class: DataClass,
) -> EgressRequest {
    EgressRequest {
        destination,
        purpose,
        data_class,
        is_local: false,
    }
}

// ---------------------------------------------------------------------------
// REQ-046: no advertising, no brokerage
// ---------------------------------------------------------------------------

#[test]
fn behavioral_advertising_is_disabled() {
    assert!(
        !behavioral_advertising_enabled(),
        "REQ-046 forbids behavioral advertising"
    );
}

#[test]
fn data_brokerage_is_disabled() {
    assert!(
        !data_brokerage_enabled(),
        "REQ-046 forbids selling or brokering learner data"
    );
}

#[test]
fn no_registered_destination_is_an_advertising_endpoint() {
    // The guarantee is an absence, so it must be asserted rather than assumed.
    for destination in default_destinations() {
        assert!(
            !destination.is_advertising,
            "{} is registered as an advertising endpoint",
            destination.id
        );
    }
}

#[test]
fn an_advertising_destination_is_refused_even_with_consent() {
    // Consent must never be able to enable something the product forbids.
    let result = authorize_egress(&request(
        dest("ad-network", true, true),
        EgressPurpose::ModelQuery,
        DataClass::StudyData,
    ));
    assert_eq!(
        result,
        Err(EgressRefusal::AdvertisingForbidden(
            "ad-network".to_string()
        ))
    );
}

#[test]
fn an_advertising_destination_is_refused_even_for_anonymous_data() {
    let result = authorize_egress(&request(
        dest("analytics", true, true),
        EgressPurpose::UpdateCheck,
        DataClass::Anonymous,
    ));
    assert!(matches!(
        result,
        Err(EgressRefusal::AdvertisingForbidden(_))
    ));
}

// ---------------------------------------------------------------------------
// Consent
// ---------------------------------------------------------------------------

#[test]
fn learner_data_requires_consent() {
    let result = authorize_egress(&request(
        dest("provider", false, false),
        EgressPurpose::ModelQuery,
        DataClass::StudyData,
    ));
    assert_eq!(
        result,
        Err(EgressRefusal::NoConsent("provider".to_string()))
    );
}

#[test]
fn learner_data_is_permitted_with_consent_for_a_model_query() {
    assert!(authorize_egress(&request(
        dest("provider", true, false),
        EgressPurpose::ModelQuery,
        DataClass::StudyData,
    ))
    .is_ok());
}

#[test]
fn anonymous_data_does_not_require_consent() {
    // An update check with no learner content is not a privacy event.
    assert!(authorize_egress(&request(
        dest("update_channel", false, false),
        EgressPurpose::UpdateCheck,
        DataClass::Anonymous,
    ))
    .is_ok());
}

#[test]
fn personal_data_requires_consent_like_study_data() {
    let result = authorize_egress(&request(
        dest("provider", false, false),
        EgressPurpose::ModelQuery,
        DataClass::PersonalData,
    ));
    assert!(matches!(result, Err(EgressRefusal::NoConsent(_))));
}

// ---------------------------------------------------------------------------
// Secrets never leave
// ---------------------------------------------------------------------------

#[test]
fn a_secret_payload_is_refused_for_every_purpose() {
    for purpose in [
        EgressPurpose::ModelQuery,
        EgressPurpose::PolicyRefresh,
        EgressPurpose::UpdateCheck,
        EgressPurpose::CrashReport,
        EgressPurpose::SourceFetch,
    ] {
        let result = authorize_egress(&request(
            dest("provider", true, false),
            purpose,
            DataClass::Secret,
        ));
        assert_eq!(
            result,
            Err(EgressRefusal::SecretPayload),
            "a secret must never be transmitted ({purpose:?})"
        );
    }
}

#[test]
fn a_secret_is_refused_even_to_a_local_destination() {
    // Redaction is the crash path's job; an unredacted secret reaching the
    // egress layer is a bug, and this layer fails closed.
    let local = EgressRequest {
        destination: dest("local_llama", true, false),
        purpose: EgressPurpose::ModelQuery,
        data_class: DataClass::Secret,
        is_local: true,
    };
    assert_eq!(authorize_egress(&local), Err(EgressRefusal::SecretPayload));
}

// ---------------------------------------------------------------------------
// Purpose limits
// ---------------------------------------------------------------------------

#[test]
fn a_purpose_that_must_not_carry_learner_data_is_refused() {
    // An update check has no legitimate reason to include study history.
    for purpose in [
        EgressPurpose::UpdateCheck,
        EgressPurpose::PolicyRefresh,
        EgressPurpose::SourceFetch,
    ] {
        let result = authorize_egress(&request(
            dest("provider", true, false),
            purpose,
            DataClass::StudyData,
        ));
        assert_eq!(
            result,
            Err(EgressRefusal::PurposeForbidsLearnerData(purpose)),
            "{purpose:?} must not carry learner data"
        );
    }
}

#[test]
fn only_model_queries_and_crash_reports_may_carry_learner_data() {
    assert!(EgressPurpose::ModelQuery.may_carry_learner_data());
    assert!(EgressPurpose::CrashReport.may_carry_learner_data());
    assert!(!EgressPurpose::UpdateCheck.may_carry_learner_data());
    assert!(!EgressPurpose::PolicyRefresh.may_carry_learner_data());
    assert!(!EgressPurpose::SourceFetch.may_carry_learner_data());
}

// ---------------------------------------------------------------------------
// Local traffic is not egress
// ---------------------------------------------------------------------------

#[test]
fn a_local_request_is_permitted_without_host_rules() {
    // This is what keeps the offline core usable (REQ-042).
    let local = EgressRequest {
        destination: dest("local_llama", false, false),
        purpose: EgressPurpose::ModelQuery,
        data_class: DataClass::PersonalData,
        is_local: true,
    };
    assert!(authorize_egress(&local).is_ok());
}

#[test]
fn local_requests_still_never_carry_secrets() {
    let local = EgressRequest {
        destination: dest("local_llama", true, false),
        purpose: EgressPurpose::ModelQuery,
        data_class: DataClass::Secret,
        is_local: true,
    };
    assert!(authorize_egress(&local).is_err());
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn refusal_messages_explain_the_rule() {
    assert!(EgressRefusal::AdvertisingForbidden("x".to_string())
        .to_string()
        .contains("never contacts"));
    assert!(EgressRefusal::NoConsent("x".to_string())
        .to_string()
        .contains("consented"));
    assert!(EgressRefusal::SecretPayload
        .to_string()
        .contains("never be transmitted"));
}

#[test]
fn the_default_registry_covers_the_expected_destinations() {
    let ids: Vec<String> = default_destinations().into_iter().map(|d| d.id).collect();
    assert!(ids.contains(&"local_llama".to_string()));
    assert!(ids.contains(&"update_channel".to_string()));
    assert!(ids.contains(&"policy_registry".to_string()));
    // No advertising or analytics destination exists to be enabled.
    assert!(!ids.iter().any(|i| i.contains("ad") && i.contains("net")));
}

#[test]
fn the_local_destination_is_consented_by_default() {
    // The local model runs on this machine; requiring consent for it would make
    // the offline tutor unusable for no privacy benefit.
    let local = default_destinations()
        .into_iter()
        .find(|d| d.id == "local_llama")
        .expect("local destination present");
    assert!(local.consented);
    assert_eq!(local.host, "127.0.0.1", "the local model is loopback only");
}
