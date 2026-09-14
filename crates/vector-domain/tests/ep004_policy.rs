//! EP-004 acceptance: versioned sourced policy, job targets and retest
//! planning (REQ-012, REQ-013).
//!
//! The binding rule under test is CONTENT_GOVERNANCE.md's "Expired claims stay
//! viewable but cannot silently power a claim presented as current."

use chrono::NaiveDate;
use vector_domain::policy::{
    plan_retest, resolve_job_target, ClaimKind, Freshness, JobTargetError, PolicyError,
    PolicyPayload, PolicyRecord, PolicySource, PolicyStore, TrustTier,
};

fn now() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 10).unwrap()
}

fn source(retrieved: &str, tier: TrustTier) -> PolicySource {
    PolicySource {
        source_id: "SRC-ASVAB-004".to_string(),
        canonical_url: "https://www.officialasvab.com/applicants/military-jobs/".to_string(),
        publisher: "Official ASVAB".to_string(),
        trust_tier: tier,
        retrieved_on: retrieved.to_string(),
        effective_on: None,
        license_note: "public government information".to_string(),
    }
}

fn job_target_record(version: u32, retrieved: &str, required: u32) -> PolicyRecord {
    PolicyRecord {
        id: format!("afqt-{version}"),
        family: "afqt-composite".to_string(),
        version,
        claim_kind: ClaimKind::JobOrCutoff,
        source: source(retrieved, TrustTier::A),
        payload: PolicyPayload::JobTarget {
            job_name: "Cyber".to_string(),
            required,
            composite: "AFQT".to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// Trust tiers
// ---------------------------------------------------------------------------

#[test]
fn tier_d_can_never_justify_policy() {
    assert!(!TrustTier::D.may_justify_policy());
    for tier in [TrustTier::A, TrustTier::B, TrustTier::C] {
        assert!(tier.may_justify_policy(), "{tier:?} may justify policy");
    }
}

#[test]
fn a_tier_d_policy_record_is_refused_at_publish() {
    // A record that can never power a claim must not be storable as policy, or
    // it will eventually be cited by mistake.
    let mut store = PolicyStore::new();
    let mut record = job_target_record(1, "2026-09-01", 60);
    record.source.trust_tier = TrustTier::D;

    assert_eq!(store.publish(record), Err(PolicyError::UntrustedTier));
    assert!(store.all().is_empty(), "the record must not be stored");
}

// ---------------------------------------------------------------------------
// REQ-012: versioned, sourced targets and freshness
// ---------------------------------------------------------------------------

#[test]
fn freshness_windows_match_the_documented_defaults() {
    // CONTENT_GOVERNANCE.md freshness defaults.
    assert_eq!(ClaimKind::ProviderTerms.freshness_days(), 7);
    assert_eq!(ClaimKind::JobOrCutoff.freshness_days(), 30);
    assert_eq!(ClaimKind::AsvabPolicy.freshness_days(), 90);
    assert_eq!(ClaimKind::StableAcademic.freshness_days(), 365);
}

#[test]
fn a_recently_retrieved_job_target_is_current() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-09-01", 60))
        .expect("publish");

    let target = resolve_job_target(&store, "Cyber", now()).expect("current target");
    assert_eq!(target.required, 60);
    assert_eq!(target.composite, "AFQT");
    assert_eq!(target.source_id, "SRC-ASVAB-004");
    assert_eq!(target.trust_tier, TrustTier::A);
}

#[test]
fn an_expired_job_target_cannot_be_presented_as_current() {
    // Retrieved 60 days ago against a 30-day window: expired.
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-07-12", 60))
        .expect("publish");

    let result = resolve_job_target(&store, "Cyber", now());
    assert_eq!(
        result,
        Err(JobTargetError::NotFoundOrStale("Cyber".to_string())),
        "an expired target must not be served as current"
    );
}

#[test]
fn an_expired_claim_remains_viewable() {
    // The other half of the rule: expiry hides it from *current* use but must
    // not delete it, so the learner can still see what the policy said.
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-07-12", 60))
        .expect("publish");

    let viewable = store
        .latest_viewable("afqt-composite")
        .expect("the expired record is still viewable");
    assert_eq!(viewable.version, 1);

    match viewable.freshness(now()) {
        Freshness::Expired {
            age_days,
            window_days,
        } => {
            assert_eq!(age_days, 60);
            assert_eq!(window_days, 30);
        }
        other => panic!("expected Expired, got {other:?}"),
    }

    // But asking for it as current must fail.
    assert!(store.current("afqt-composite", now()).is_err());
}

#[test]
fn current_payload_refuses_an_expired_record() {
    let record = job_target_record(1, "2026-07-12", 60);
    let result = record.current_payload(now());
    assert!(
        result.is_err(),
        "an expired record must not yield a current payload"
    );
}

#[test]
fn version_advance_supersedes_an_older_record() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-01-01", 50))
        .expect("publish v1");
    store
        .publish(job_target_record(2, "2026-09-05", 65))
        .expect("publish v2");

    // The newest version is served.
    let target = resolve_job_target(&store, "Cyber", now()).expect("current");
    assert_eq!(target.required, 65, "the newest version must win");
    assert_eq!(store.latest_viewable("afqt-composite").unwrap().version, 2);
}

#[test]
fn policy_versions_must_increase() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(2, "2026-09-05", 65))
        .expect("publish v2");

    // Equal or lower versions are refused, so history cannot be rewritten.
    assert_eq!(
        store.publish(job_target_record(2, "2026-09-06", 70)),
        Err(PolicyError::NonMonotonicVersion {
            existing: 2,
            attempted: 2
        })
    );
    assert_eq!(
        store.publish(job_target_record(1, "2026-09-06", 70)),
        Err(PolicyError::NonMonotonicVersion {
            existing: 2,
            attempted: 1
        })
    );
}

#[test]
fn a_stale_newer_version_does_not_fall_back_to_an_older_one() {
    // The trap: v2 is expired, v1 is even older. Falling back would present an
    // even staler number as current. Returning nothing is the honest answer.
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2025-01-01", 50))
        .expect("publish v1");
    store
        .publish(job_target_record(2, "2026-07-01", 65))
        .expect("publish v2");

    assert!(
        resolve_job_target(&store, "Cyber", now()).is_err(),
        "an expired v2 must not silently serve v1"
    );
}

#[test]
fn a_malformed_retrieval_date_fails_closed() {
    let mut store = PolicyStore::new();
    let mut record = job_target_record(1, "not-a-date", 60);
    record.source.retrieved_on = "not-a-date".to_string();
    store.publish(record).expect("publish");

    assert_eq!(
        store
            .latest_viewable("afqt-composite")
            .unwrap()
            .freshness(now()),
        Freshness::Unverifiable,
        "an unparseable date must not be treated as fresh"
    );
    assert!(
        resolve_job_target(&store, "Cyber", now()).is_err(),
        "an unverifiable record must not power a current claim"
    );
}

#[test]
fn a_future_retrieval_date_fails_closed() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2027-01-01", 60))
        .expect("publish");

    assert_eq!(
        store
            .latest_viewable("afqt-composite")
            .unwrap()
            .freshness(now()),
        Freshness::Unverifiable,
        "a future retrieval date is malformed, not fresh"
    );
}

#[test]
fn an_unknown_job_is_reported_not_guessed() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-09-05", 60))
        .expect("publish");

    assert_eq!(
        resolve_job_target(&store, "Underwater Basket Weaving", now()),
        Err(JobTargetError::NotFoundOrStale(
            "Underwater Basket Weaving".to_string()
        )),
        "an unknown job must not receive an invented requirement"
    );
}

#[test]
fn job_matching_is_case_insensitive_but_exact() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-09-05", 60))
        .expect("publish");

    assert!(resolve_job_target(&store, "cyber", now()).is_ok());
    assert!(resolve_job_target(&store, "CYBER", now()).is_ok());
    // A prefix is not a match: "Cyb" must not resolve to "Cyber".
    assert!(resolve_job_target(&store, "Cyb", now()).is_err());
}

// ---------------------------------------------------------------------------
// REQ-013: retest planner on versioned policy data
// ---------------------------------------------------------------------------

#[test]
fn retest_plan_is_derived_from_current_policy() {
    let mut store = PolicyStore::new();
    store
        .publish(PolicyRecord {
            id: "retest-1".to_string(),
            family: "retest-wait".to_string(),
            version: 1,
            claim_kind: ClaimKind::AsvabPolicy,
            source: source("2026-09-01", TrustTier::A),
            payload: PolicyPayload::RetestPolicy {
                wait_days: 30,
                waiver_available: true,
            },
        })
        .expect("publish");

    let last_test = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    let plan = plan_retest(&store, last_test, now()).expect("plan");

    assert_eq!(plan.wait_days, 30);
    assert_eq!(
        plan.earliest_retest,
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
    );
    assert!(plan.waiver_available);
    assert_eq!(plan.source_id, "SRC-ASVAB-004");
}

#[test]
fn a_stale_retest_policy_produces_no_plan() {
    // Inventing a wait period would be a fabricated policy claim.
    let mut store = PolicyStore::new();
    store
        .publish(PolicyRecord {
            id: "retest-1".to_string(),
            family: "retest-wait".to_string(),
            version: 1,
            claim_kind: ClaimKind::AsvabPolicy,
            // 200 days old against a 90-day window.
            source: source("2026-02-01", TrustTier::A),
            payload: PolicyPayload::RetestPolicy {
                wait_days: 30,
                waiver_available: false,
            },
        })
        .expect("publish");

    let last_test = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    assert!(
        plan_retest(&store, last_test, now()).is_err(),
        "a stale retest policy must not produce a plan"
    );
}

#[test]
fn an_empty_store_yields_no_plan_and_no_target() {
    let store = PolicyStore::new();
    assert!(resolve_job_target(&store, "Cyber", now()).is_err());
    assert!(plan_retest(&store, NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), now()).is_err());
}

#[test]
fn a_provider_terms_claim_expires_much_faster_than_an_academic_one() {
    let academic = PolicyRecord {
        id: "acad".to_string(),
        family: "academic".to_string(),
        version: 1,
        claim_kind: ClaimKind::StableAcademic,
        source: source("2026-06-01", TrustTier::B),
        payload: PolicyPayload::CompositeFormula {
            composite: "AFQT".to_string(),
            subtests: vec!["AR".to_string(), "WK".to_string()],
            expression: "see source".to_string(),
        },
    };
    let mut terms = academic.clone();
    terms.id = "terms".to_string();
    terms.family = "terms".to_string();
    terms.claim_kind = ClaimKind::ProviderTerms;

    // 100 days old: fine for academic (365d), expired for provider terms (7d).
    assert!(academic.freshness(now()).is_current());
    assert!(!terms.freshness(now()).is_current());
}

#[test]
fn store_reports_its_policy_families() {
    let mut store = PolicyStore::new();
    store
        .publish(job_target_record(1, "2026-09-05", 60))
        .expect("publish");
    store
        .publish(PolicyRecord {
            id: "retest-1".to_string(),
            family: "retest-wait".to_string(),
            version: 1,
            claim_kind: ClaimKind::AsvabPolicy,
            source: source("2026-09-05", TrustTier::A),
            payload: PolicyPayload::RetestPolicy {
                wait_days: 30,
                waiver_available: false,
            },
        })
        .expect("publish");

    assert_eq!(
        store.families(),
        vec!["afqt-composite".to_string(), "retest-wait".to_string()]
    );
}

#[test]
fn policy_records_round_trip_through_serde() {
    let record = job_target_record(3, "2026-09-05", 65);
    let json = serde_json::to_string(&record).expect("serialize");
    let back: PolicyRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(record, back, "policy records must persist exactly");
}
