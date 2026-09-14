//! EP-004 acceptance: model transport routing and provider policy.
//!
//! Requirements: REQ-014 (local llama.cpp), REQ-015 (provider-neutral
//! contract), REQ-016 (Grok native), REQ-017 (Codex native), REQ-018 (Claude
//! conditional), REQ-019 (Gemini OAuth bridge disabled).
//!
//! These tests encode the hard rules in AI_TRANSPORTS.md as enforced behavior.

use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use vector_llm::transport::{
    default_registry, scrub_api_keys, select_transport, validate_provider_text, AdapterHealth,
    BillingMode, Capabilities, IneligibilityReason, ModelTransport, PolicyRecord, PrivacyClass,
    API_KEY_ENV_VARS,
};

fn nothing_required() -> Capabilities {
    Capabilities {
        structured_output: false,
        streaming: false,
        tool_use: false,
        mcp: false,
        web_search: false,
    }
}

// ---------------------------------------------------------------------------
// REQ-019 / ADR-006: the Gemini OAuth bridge is disabled and cannot be chosen.
// ---------------------------------------------------------------------------

#[test]
fn gemini_cli_oauth_bridge_is_disabled() {
    let now = Utc::now();
    let reg = default_registry(now);

    let bridge = reg
        .iter()
        .find(|t| t.id == "gemini_cli_oauth")
        .expect("the prohibited bridge is modelled explicitly");

    assert!(
        bridge.disabled,
        "ADR-006: the OAuth bridge must be disabled"
    );
    assert!(
        !bridge.policy.permitted,
        "provider terms prohibit this bridge"
    );

    // Even with a healthy client, it must never be eligible.
    let mut healthy = bridge.clone();
    healthy.health = AdapterHealth::Healthy;
    assert_eq!(
        healthy.is_eligible(PrivacyClass::RemoteAllowed, now, 365),
        Err(IneligibilityReason::Disabled),
        "a disabled bridge is refused regardless of health"
    );

    // And it must never be selected.
    let selected = select_transport(
        &reg,
        PrivacyClass::RemoteAllowed,
        &nothing_required(),
        now,
        365,
    );
    if let Ok(t) = selected {
        assert_ne!(
            t.id, "gemini_cli_oauth",
            "the bridge must never be selected"
        );
    }
}

// ---------------------------------------------------------------------------
// Rule 6: a local-only request must never route to a network provider.
// ---------------------------------------------------------------------------

#[test]
fn local_only_requests_never_route_to_a_network_provider() {
    let now = Utc::now();
    // Make every adapter healthy so only the privacy rule can exclude them.
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Healthy;
    }

    let chosen = select_transport(&reg, PrivacyClass::LocalOnly, &nothing_required(), now, 365)
        .expect("the local adapter is eligible");

    assert_eq!(
        chosen.id, "local_llama",
        "a local-only request must select the on-machine adapter"
    );
    assert_eq!(chosen.privacy, PrivacyClass::LocalOnly);
}

#[test]
fn local_only_fails_loudly_when_no_local_adapter_is_available() {
    let now = Utc::now();
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Healthy;
    }
    // Remove the only local option.
    reg.retain(|t| t.privacy != PrivacyClass::LocalOnly);

    let result = select_transport(&reg, PrivacyClass::LocalOnly, &nothing_required(), now, 365);
    let err = result.expect_err("with no local adapter the request must fail, not fall back");
    assert_eq!(err.privacy, PrivacyClass::LocalOnly);
    // The refusal must explain itself.
    let message = err.to_string();
    assert!(
        message.contains("local-only") || message.contains("privacy"),
        "refusal must explain the privacy constraint: {message}"
    );
}

#[test]
fn remote_allowed_prefers_the_local_adapter(/* ADR-003 local-first */) {
    let now = Utc::now();
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Healthy;
    }

    let chosen = select_transport(
        &reg,
        PrivacyClass::RemoteAllowed,
        &nothing_required(),
        now,
        365,
    )
    .expect("some adapter is eligible");

    assert_eq!(
        chosen.id, "local_llama",
        "local-first: a network lane is not used merely because it exists"
    );
}

// ---------------------------------------------------------------------------
// Rule 5: a stale or revoked provider policy disables the adapter.
// ---------------------------------------------------------------------------

#[test]
fn stale_provider_policy_disables_the_adapter() {
    let now = Utc::now();
    let adapter = ModelTransport {
        id: "claude_native".into(),
        transport: "official Claude Code noninteractive mode".into(),
        auth_owner: "Anthropic native client".into(),
        billing: BillingMode::Subscription,
        capabilities: nothing_required(),
        privacy: PrivacyClass::RemoteAllowed,
        policy: PolicyRecord {
            // Verified two years ago: outside any reasonable window.
            terms_verified_on: (now - Duration::days(730)).format("%Y-%m-%d").to_string(),
            permitted: true,
            note: "old verification".into(),
        },
        health: AdapterHealth::Healthy,
        disabled: false,
    };

    assert_eq!(
        adapter.is_eligible(PrivacyClass::RemoteAllowed, now, 90),
        Err(IneligibilityReason::PolicyStale),
        "an unverified-recently policy must not be assumed still valid"
    );
}

#[test]
fn revoked_provider_policy_disables_the_adapter_immediately() {
    let now = Utc::now();
    let mut adapter = default_registry(now)
        .into_iter()
        .find(|t| t.id == "grok_native")
        .expect("grok adapter present");
    adapter.health = AdapterHealth::Healthy;
    assert!(adapter
        .is_eligible(PrivacyClass::RemoteAllowed, now, 365)
        .is_ok());

    // A signed policy record flips the adapter off without a code change.
    adapter.policy.permitted = false;
    assert_eq!(
        adapter.is_eligible(PrivacyClass::RemoteAllowed, now, 365),
        Err(IneligibilityReason::PolicyProhibits)
    );
}

#[test]
fn a_future_dated_policy_record_is_not_treated_as_fresh() {
    let now = Utc::now();
    let record = PolicyRecord {
        terms_verified_on: (now + Duration::days(30)).format("%Y-%m-%d").to_string(),
        permitted: true,
        note: "clock skew or tampering".into(),
    };
    assert!(
        !record.is_fresh(now, 365),
        "a future verification date is malformed and must not count as fresh"
    );
}

#[test]
fn a_malformed_policy_date_is_not_fresh() {
    let now = Utc::now();
    let record = PolicyRecord {
        terms_verified_on: "not-a-date".into(),
        permitted: true,
        note: "bad data".into(),
    };
    assert!(
        !record.is_fresh(now, 365),
        "an unparseable date must fail closed, not open"
    );
}

#[test]
fn unhealthy_adapters_are_not_selected() {
    let now = Utc::now();
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Unavailable {
            reason: "not logged in".into(),
        };
    }

    let err = select_transport(
        &reg,
        PrivacyClass::RemoteAllowed,
        &nothing_required(),
        now,
        365,
    )
    .expect_err("no healthy adapter exists");

    // Every refusal must be explained. Disabled/prohibited adapters report their
    // own (earlier) reason, which is correct ordering; healthy-but-unavailable
    // ones report Unhealthy.
    assert_eq!(
        err.refusals.len(),
        reg.len(),
        "every adapter must report a reason"
    );
    for (id, reason) in &err.refusals {
        let adapter = reg.iter().find(|t| t.id == *id).expect("known adapter");
        let expected = if adapter.disabled {
            IneligibilityReason::Disabled
        } else if !adapter.policy.permitted {
            IneligibilityReason::PolicyProhibits
        } else {
            IneligibilityReason::Unhealthy
        };
        assert_eq!(reason, &expected, "{id} reported the wrong refusal reason");
    }

    // At least one adapter is unhealthy-but-permitted, proving the health gate
    // is what excluded it.
    assert!(
        err.refusals
            .iter()
            .any(|(_, r)| *r == IneligibilityReason::Unhealthy),
        "the health gate must have excluded at least one adapter"
    );
}

// ---------------------------------------------------------------------------
// Capability requirements are honoured.
// ---------------------------------------------------------------------------

#[test]
fn a_request_requiring_tools_does_not_land_on_a_tool_less_adapter() {
    let now = Utc::now();
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Healthy;
    }

    // local_llama has no tool_use; a tool request must skip it.
    let requires_tools = Capabilities {
        tool_use: true,
        ..nothing_required()
    };
    let chosen = select_transport(&reg, PrivacyClass::RemoteAllowed, &requires_tools, now, 365)
        .expect("a native lane can serve tools");
    assert!(
        chosen.capabilities.tool_use,
        "selected adapter must actually have the required capability"
    );
    assert_ne!(chosen.id, "local_llama");
}

#[test]
fn a_local_only_request_requiring_tools_fails_rather_than_leaking() {
    let now = Utc::now();
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Healthy;
    }

    // The local adapter lacks tool use, and privacy forbids the remote ones.
    // The only correct outcome is a refusal.
    let requires_tools = Capabilities {
        tool_use: true,
        ..nothing_required()
    };
    let result = select_transport(&reg, PrivacyClass::LocalOnly, &requires_tools, now, 365);
    assert!(
        result.is_err(),
        "must refuse rather than route a local-only tool request to a network provider"
    );
}

// ---------------------------------------------------------------------------
// Rule 4: API-key environment variables are scrubbed from native lanes.
// ---------------------------------------------------------------------------

#[test]
fn subscription_lanes_do_not_inherit_api_keys() {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), "/usr/bin".to_string());
    for key in API_KEY_ENV_VARS {
        env.insert((*key).to_string(), "sk-secret".to_string());
    }

    let scrubbed = scrub_api_keys(&env, BillingMode::Subscription);

    for key in API_KEY_ENV_VARS {
        assert!(
            !scrubbed.contains_key(*key),
            "{key} must be scrubbed from a subscription lane, or the user is \
             silently billed on a metered key they did not choose"
        );
    }
    assert_eq!(
        scrubbed.get("PATH").map(String::as_str),
        Some("/usr/bin"),
        "unrelated environment must be preserved"
    );
}

#[test]
fn an_explicit_api_lane_keeps_its_key() {
    let mut env = BTreeMap::new();
    env.insert("OPENAI_API_KEY".to_string(), "sk-explicit".to_string());

    let kept = scrub_api_keys(&env, BillingMode::MeteredApi);
    assert_eq!(
        kept.get("OPENAI_API_KEY").map(String::as_str),
        Some("sk-explicit"),
        "a deliberately chosen API lane keeps its credential"
    );
}

// ---------------------------------------------------------------------------
// Rule 7: provider output is untrusted and bounded.
// ---------------------------------------------------------------------------

#[test]
fn oversized_provider_output_is_rejected() {
    let big = "x".repeat(1000);
    assert!(validate_provider_text(&big, 100).is_err());
    assert!(validate_provider_text("short", 100).is_ok());
    // Boundary: exactly at the limit is acceptable.
    assert!(validate_provider_text(&"x".repeat(100), 100).is_ok());
    assert!(validate_provider_text(&"x".repeat(101), 100).is_err());
}

#[test]
fn provider_output_with_a_nul_byte_is_rejected() {
    assert!(
        validate_provider_text("bad\0text", 100).is_err(),
        "NUL bytes are never valid in provider text"
    );
}

// ---------------------------------------------------------------------------
// REQ-015: every adapter declares the full contract.
// ---------------------------------------------------------------------------

#[test]
fn every_registered_adapter_declares_the_required_contract_fields() {
    let now = Utc::now();
    for adapter in default_registry(now) {
        assert!(!adapter.id.is_empty(), "adapter must have an id");
        assert!(
            !adapter.transport.is_empty(),
            "{} must name its documented transport",
            adapter.id
        );
        assert!(
            !adapter.auth_owner.is_empty(),
            "{} must declare who owns authentication (VECTOR never holds it)",
            adapter.id
        );
        assert!(
            !adapter.policy.terms_verified_on.is_empty(),
            "{} must carry a dated policy record",
            adapter.id
        );
        assert!(
            !adapter.policy.note.is_empty(),
            "{} must record why its policy state holds",
            adapter.id
        );
    }
}

#[test]
fn native_client_lanes_never_claim_to_own_credentials() {
    let now = Utc::now();
    for adapter in default_registry(now) {
        // The prohibited bridge is excluded: it is disabled precisely because it
        // would require credential access VECTOR must not perform.
        if adapter.disabled {
            continue;
        }
        if matches!(adapter.billing, BillingMode::Subscription) {
            assert!(
                adapter.auth_owner.contains("native") || adapter.auth_owner.contains("client"),
                "{} is a subscription lane, so authentication must belong to the \
                 provider's own client, got {:?}",
                adapter.id,
                adapter.auth_owner
            );
            // Hard rule 1: VECTOR never stores or replays provider credentials.
            let lowered = adapter.auth_owner.to_lowercase();
            for forbidden in ["vector", "oauth token", "cookie", "refresh token"] {
                assert!(
                    !lowered.contains(forbidden),
                    "{} must not claim to own credentials ({forbidden})",
                    adapter.id
                );
            }
        }
    }
}

#[test]
fn selection_is_deterministic() {
    let now = Utc::now();
    let mut reg = default_registry(now);
    for t in reg.iter_mut() {
        t.health = AdapterHealth::Healthy;
    }

    let a = select_transport(
        &reg,
        PrivacyClass::RemoteAllowed,
        &nothing_required(),
        now,
        365,
    )
    .expect("eligible")
    .id
    .clone();
    let b = select_transport(
        &reg,
        PrivacyClass::RemoteAllowed,
        &nothing_required(),
        now,
        365,
    )
    .expect("eligible")
    .id
    .clone();
    assert_eq!(a, b, "selection must be reproducible");
}
