//! Acceptance oracle for the desktop command boundary (SPEC-003, EP-001).
//!
//! Requirements: REQ-010 (offline analytics), REQ-011 (readiness band),
//! REQ-020 (local evidence/vault readback), REQ-054 (idempotent attempts).
//!
//! ## What this proves and what it does not
//!
//! `#[tauri::command]` wrappers cannot be called outside a running Tauri
//! application — `State<'_, AppState>` has no test constructor — so the wrappers
//! are kept mechanical and every command's behaviour lives in a `<name>_impl`
//! function. These tests drive those impls through `AppState`, which is the same
//! state the wrappers unwrap, against a real on-disk SQLite file created by the
//! application's own embedded migration set.
//!
//! What is *not* proven here is webview-to-Rust IPC delivery. That is a property
//! of the Tauri runtime, not of this code, and it is covered separately by the
//! live-fire launch recorded in `.agent/evidence/EP-001/`. This file does not
//! claim it.
//!
//! ## Why the assertions look like this
//!
//! `AGENTS.md` §9 forbids treating compilation or a health endpoint as feature
//! proof, so each test reads back the effect of the call it made rather than
//! trusting the return value alone, and the negative cases assert the refusal
//! *and* that no state changed.

use std::path::Path;

use vector_application::service::ServiceError;
use vector_desktop_lib::commands::{
    analytics_all_impl, analytics_impl, create_profile_impl, get_profile_impl, health_impl,
    list_profiles_impl, mastery_impl, migrations, readiness_impl, record_attempt_impl,
    set_mastery_impl, study_plan_impl, AppState,
};
use vector_persistence::{Database, MigrationManager};

/// Minimal dependency-free temp dir so the test does not pull a new crate.
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "vector-ipc-{}-{}-{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            path.push(unique);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Open application state the way `lib::run` does, through the real entry point.
///
/// Returns the state and the directory so the caller can prove the file exists.
fn open_state(tag: &str) -> (tempdir::TempDir, AppState) {
    let dir = tempdir::TempDir::new(tag);
    let state = AppState::open(&dir.path().join("vector.db")).expect("open application state");
    (dir, state)
}

/// Create a profile through the command impl and return its id.
fn seeded_profile(state: &AppState, name: &str) -> String {
    let guard = state.db().expect("lock");
    create_profile_impl(&guard, name, 50)
        .expect("create profile")
        .id
}

// ---------------------------------------------------------------------------
// Startup path: the file, the schema, and the drift guard.
// ---------------------------------------------------------------------------

#[test]
fn open_creates_the_database_file_and_applies_the_embedded_schema() {
    let dir = tempdir::TempDir::new("startup");
    let db_path = dir.path().join("vector.db");

    assert!(
        !db_path.exists(),
        "precondition: no database before startup"
    );

    let state = AppState::open(&db_path).expect("startup must succeed");

    // Read back the effect rather than trusting the constructor's return.
    assert!(db_path.exists(), "startup must create the database file");
    let guard = state.db().expect("lock");
    let versions = MigrationManager::applied_versions(&guard).expect("read applied versions");
    let expected: Vec<i64> = migrations().iter().map(|(v, _)| *v).collect();
    assert_eq!(
        versions, expected,
        "every embedded migration must be recorded as applied"
    );
}

#[test]
fn embedded_migrations_do_not_drift_from_the_files_on_disk() {
    // The binary embeds the schema at compile time, so a migration edited on
    // disk without rebuilding would ship a stale schema while every other check
    // still passed. Compare the embedded text with the working tree.
    let on_disk = MigrationManager::load_from_dir(Path::new("../../../migrations"))
        .expect("load migrations from the repository");
    assert_eq!(
        migrations(),
        on_disk,
        "the embedded migration set must match migrations/ exactly"
    );
}

#[test]
fn reopening_the_same_file_preserves_state() {
    let dir = tempdir::TempDir::new("restart");
    let db_path = dir.path().join("vector.db");

    let learner_id = {
        let state = AppState::open(&db_path).expect("first start");
        let id = seeded_profile(&state, "Restart Learner");
        let guard = state.db().expect("lock");
        record_attempt_impl(&guard, "a-1", &id, "AR", "q-1", true, 1200).expect("record attempt");
        id
    }; // state dropped: the process has "exited"

    let state = AppState::open(&db_path).expect("second start");
    let guard = state.db().expect("lock");

    let profile = get_profile_impl(&guard, &learner_id).expect("profile survives restart");
    assert_eq!(profile.name, "Restart Learner");

    let stats = analytics_impl(&guard, &learner_id, "AR").expect("analytics survive restart");
    assert_eq!(stats.total, 1);
    assert_eq!(stats.correct, 1);
}

#[test]
fn health_reports_operation_basis_and_refuses_an_unmigrated_schema() {
    let (_dir, state) = open_state("health");
    {
        let guard = state.db().expect("lock");
        let report = health_impl(&guard).expect("health on a migrated database");
        assert!(report.healthy, "a freshly migrated database is healthy");
        assert_eq!(report.component, "database");
        // Liveness is not a health proof; the report must say what it did.
        assert_eq!(report.basis, "operation_succeeded");
        assert_eq!(report.profiles, 0, "no learner has been created yet");
    }

    // Negative case: a database with no schema must fail, not report healthy.
    let raw_dir = tempdir::TempDir::new("health-raw");
    let raw = Database::open(raw_dir.path().join("empty.db")).expect("open empty database");
    let error = health_impl(&raw).expect_err("an unmigrated database is not healthy");
    match error {
        ServiceError::Storage(message) => {
            assert!(
                message.contains("schema is not usable"),
                "the failure must name the real cause, got {message:?}"
            );
        }
        other => panic!("expected a storage error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Profiles.
// ---------------------------------------------------------------------------

#[test]
fn create_profile_rejects_blank_names_and_out_of_range_targets() {
    let (_dir, state) = open_state("profile-validation");
    let guard = state.db().expect("lock");

    for (name, target) in [("", 50), ("   ", 50), ("\t\n", 50)] {
        let error =
            create_profile_impl(&guard, name, target).expect_err("a blank name must be refused");
        assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");
    }

    for target in [0u32, 100, 1000] {
        let error = create_profile_impl(&guard, "Learner", target)
            .expect_err("a target outside 1..99 must be refused");
        assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");
    }

    // The refusals must not have written anything.
    assert!(
        list_profiles_impl(&guard).expect("list").is_empty(),
        "a refused create must not persist a row"
    );

    // Boundaries are accepted.
    create_profile_impl(&guard, "Min", 1).expect("target 1 is on the scale");
    create_profile_impl(&guard, "Max", 99).expect("target 99 is on the scale");
    assert_eq!(list_profiles_impl(&guard).expect("list").len(), 2);
}

#[test]
fn names_are_trimmed_and_listed_in_a_stable_order() {
    let (_dir, state) = open_state("profile-order");
    let guard = state.db().expect("lock");

    create_profile_impl(&guard, "  Zoe  ", 50).expect("create");
    create_profile_impl(&guard, "Adam", 50).expect("create");
    create_profile_impl(&guard, "Mia", 50).expect("create");

    let names: Vec<String> = list_profiles_impl(&guard)
        .expect("list")
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, vec!["Adam", "Mia", "Zoe"], "sorted and trimmed");
}

#[test]
fn get_profile_reports_not_found_for_an_unknown_id() {
    let (_dir, state) = open_state("profile-missing");
    let guard = state.db().expect("lock");

    let error = get_profile_impl(&guard, "learner-does-not-exist").expect_err("must not resolve");
    match error {
        ServiceError::NotFound(what) => assert!(
            what.contains("learner-does-not-exist"),
            "the error must name the missing id, got {what:?}"
        ),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Attempts, idempotency and analytics (REQ-010, REQ-054).
// ---------------------------------------------------------------------------

#[test]
fn record_attempt_is_idempotent_by_attempt_id() {
    let (_dir, state) = open_state("idempotent");
    let learner = seeded_profile(&state, "Idempotent Learner");
    let guard = state.db().expect("lock");

    let first = record_attempt_impl(&guard, "a-1", &learner, "AR", "q-1", true, 900)
        .expect("first submission");
    assert!(first, "the first submission inserts");

    // A retried submission — the same attempt id — must not double-count.
    let second = record_attempt_impl(&guard, "a-1", &learner, "AR", "q-1", true, 900)
        .expect("retried submission");
    assert!(!second, "a retry must report that nothing new was written");

    let stats = analytics_impl(&guard, &learner, "AR").expect("analytics");
    assert_eq!(stats.total, 1, "a retry must not create a second row");
}

#[test]
fn record_attempt_refuses_a_negative_latency_and_an_unknown_learner() {
    let (_dir, state) = open_state("attempt-validation");
    let learner = seeded_profile(&state, "Validation Learner");
    let guard = state.db().expect("lock");

    let error = record_attempt_impl(&guard, "a-neg", &learner, "AR", "q-1", true, -1)
        .expect_err("negative latency must be refused");
    assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");

    let error = record_attempt_impl(&guard, "a-orphan", "learner-ghost", "AR", "q-1", true, 10)
        .expect_err("an unknown learner must be refused");
    assert!(matches!(error, ServiceError::NotFound(_)), "got {error:?}");

    let error = record_attempt_impl(&guard, "  ", &learner, "AR", "q-1", true, 10)
        .expect_err("a blank attempt id must be refused");
    assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");

    assert_eq!(
        analytics_impl(&guard, &learner, "AR")
            .expect("analytics")
            .total,
        0,
        "no refused attempt may have been persisted"
    );
}

#[test]
fn analytics_aggregate_only_the_requested_subtest() {
    let (_dir, state) = open_state("analytics");
    let learner = seeded_profile(&state, "Analytics Learner");
    let guard = state.db().expect("lock");

    // Three AR attempts (two correct, latency 1000/2000/3000) and one WK miss.
    record_attempt_impl(&guard, "ar-1", &learner, "AR", "q-1", true, 1000).expect("record");
    record_attempt_impl(&guard, "ar-2", &learner, "AR", "q-2", true, 2000).expect("record");
    record_attempt_impl(&guard, "ar-3", &learner, "AR", "q-3", false, 3000).expect("record");
    record_attempt_impl(&guard, "wk-1", &learner, "WK", "q-4", false, 500).expect("record");

    let ar = analytics_impl(&guard, &learner, "AR").expect("AR analytics");
    assert_eq!(ar.total, 3);
    assert_eq!(ar.correct, 2);
    assert!(
        (ar.accuracy - 2.0 / 3.0).abs() < 1e-9,
        "got {}",
        ar.accuracy
    );
    assert_eq!(ar.mean_latency_ms, 2000);

    let wk = analytics_impl(&guard, &learner, "WK").expect("WK analytics");
    assert_eq!(wk.total, 1);
    assert_eq!(wk.correct, 0);
    assert_eq!(wk.accuracy, 0.0);
    assert_eq!(wk.mean_latency_ms, 500);

    // A subtest with no history reports zeroes rather than failing.
    let gs = analytics_impl(&guard, &learner, "GS").expect("GS analytics");
    assert_eq!((gs.total, gs.correct, gs.mean_latency_ms), (0, 0, 0));
}

#[test]
fn analytics_all_omits_subtests_the_learner_has_not_attempted() {
    let (_dir, state) = open_state("analytics-all");
    let learner = seeded_profile(&state, "Partial Learner");
    let guard = state.db().expect("lock");

    record_attempt_impl(&guard, "ar-1", &learner, "AR", "q-1", true, 100).expect("record");
    record_attempt_impl(&guard, "mk-1", &learner, "MK", "q-2", false, 100).expect("record");

    let all = analytics_all_impl(&guard, &learner).expect("analytics for every subtest");
    let codes: Vec<&str> = all.iter().map(|(code, _)| code.as_str()).collect();
    assert_eq!(
        codes,
        vec!["AR", "MK"],
        "only attempted subtests may be reported"
    );
    assert!(
        all.iter().all(|(_, dto)| dto.total > 0),
        "a reported subtest must have evidence behind it"
    );
}

// ---------------------------------------------------------------------------
// Mastery.
// ---------------------------------------------------------------------------

#[test]
fn set_mastery_validates_and_round_trips() {
    let (_dir, state) = open_state("mastery");
    let learner = seeded_profile(&state, "Mastery Learner");
    let guard = state.db().expect("lock");

    for bad in [-0.01, 1.01, 2.0, f64::NAN] {
        let error = set_mastery_impl(&guard, &learner, "AR", bad, 0.2)
            .expect_err("a mastery outside [0,1] must be refused");
        assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");
    }
    let error = set_mastery_impl(&guard, &learner, "AR", 0.5, -0.1)
        .expect_err("negative uncertainty must be refused");
    assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");

    assert!(
        mastery_impl(&guard, &learner).expect("read").is_empty(),
        "no refused write may persist"
    );

    set_mastery_impl(&guard, &learner, "AR", 0.4, 0.3).expect("write AR");
    set_mastery_impl(&guard, &learner, "WK", 0.9, 0.05).expect("write WK");

    let mut records = mastery_impl(&guard, &learner).expect("read");
    records.sort_by(|a, b| a.subtest.cmp(&b.subtest));
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].subtest, "AR");
    assert!((records[0].score - 0.4).abs() < 1e-9);
    assert!((records[0].uncertainty - 0.3).abs() < 1e-9);
    assert_eq!(records[1].subtest, "WK");
    assert!((records[1].score - 0.9).abs() < 1e-9);

    // Re-writing the same subtest updates in place rather than duplicating.
    set_mastery_impl(&guard, &learner, "AR", 0.6, 0.2).expect("update AR");
    let records = mastery_impl(&guard, &learner).expect("read");
    assert_eq!(records.len(), 2, "mastery is one row per subtest");
    let ar = records
        .iter()
        .find(|m| m.subtest == "AR")
        .expect("AR present");
    assert!((ar.score - 0.6).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Planning and readiness (REQ-003, REQ-011, ADR-010).
// ---------------------------------------------------------------------------

#[test]
fn study_plan_allocates_the_whole_budget_across_subtests() {
    let (_dir, state) = open_state("plan");
    let learner = seeded_profile(&state, "Plan Learner");
    let guard = state.db().expect("lock");

    let plan = study_plan_impl(&guard, &learner, 50, 100).expect("generate a plan");

    assert!(
        !plan.drills.is_empty(),
        "an unstudied learner has work to do"
    );
    assert_eq!(
        plan.total_minutes, 100,
        "the plan must use exactly the time offered"
    );
    assert_eq!(
        plan.drills.iter().map(|d| d.minutes).sum::<u32>(),
        plan.total_minutes,
        "reported total must equal the sum of the drills"
    );
    assert!(
        plan.drills.iter().all(|d| d.minutes >= 1),
        "a scheduled drill must have time to happen"
    );
    assert!(
        plan.drills.iter().all(|d| !d.reason.is_empty()),
        "every drill must explain why it was chosen"
    );
    // Deterministic: the same state must produce the same plan.
    let again = study_plan_impl(&guard, &learner, 50, 100).expect("regenerate");
    assert_eq!(plan, again, "plan generation must be deterministic");
}

#[test]
fn study_plan_rejects_a_zero_budget_and_an_off_scale_target() {
    let (_dir, state) = open_state("plan-validation");
    let learner = seeded_profile(&state, "Plan Validation Learner");
    let guard = state.db().expect("lock");

    assert!(matches!(
        study_plan_impl(&guard, &learner, 50, 0).expect_err("zero minutes"),
        ServiceError::Invalid(_)
    ));
    assert!(matches!(
        study_plan_impl(&guard, &learner, 0, 60).expect_err("target 0"),
        ServiceError::Invalid(_)
    ));
    assert!(matches!(
        study_plan_impl(&guard, &learner, 100, 60).expect_err("target 100"),
        ServiceError::Invalid(_)
    ));
    assert!(matches!(
        study_plan_impl(&guard, "learner-ghost", 50, 60).expect_err("unknown learner"),
        ServiceError::NotFound(_)
    ));
}

#[test]
fn a_fully_mastered_learner_with_nothing_due_gets_no_busy_work() {
    let (_dir, state) = open_state("plan-saturated");
    let learner = seeded_profile(&state, "Saturated Learner");
    let guard = state.db().expect("lock");

    // Mastery at or above a target of 1, with tight estimates and no attempts.
    for subtest in vector_domain::mastery::Subtest::ALL {
        set_mastery_impl(&guard, &learner, subtest.code(), 1.0, 0.0).expect("set mastery");
    }

    let plan = study_plan_impl(&guard, &learner, 1, 60).expect("generate a plan");
    assert!(
        plan.drills.is_empty(),
        "a learner with nothing to work on must get an empty plan, got {:?}",
        plan.drills
    );
    assert_eq!(plan.total_minutes, 0);
}

#[test]
fn readiness_is_reported_as_a_band_and_never_claims_an_official_score() {
    let (_dir, state) = open_state("readiness");
    let learner = seeded_profile(&state, "Readiness Learner");
    let guard = state.db().expect("lock");

    // Nothing is known yet: the band must be wide, not falsely precise.
    let unknown = readiness_impl(&guard, &learner).expect("readiness with no evidence");
    assert!(!unknown.official_score_claim, "ADR-010 forbids this claim");
    assert!(unknown.low <= unknown.high, "the band must be ordered");
    assert!(
        (unknown.high - unknown.low) > 0.5,
        "an unestimated learner must produce a wide band, got {unknown:?}"
    );

    // Accumulate evidence: high mastery, low uncertainty.
    for subtest in vector_domain::mastery::Subtest::ALL {
        set_mastery_impl(&guard, &learner, subtest.code(), 0.8, 0.02).expect("set mastery");
    }
    let known = readiness_impl(&guard, &learner).expect("readiness with evidence");

    assert!(!known.official_score_claim);
    assert!(
        (known.high - known.low) < (unknown.high - unknown.low),
        "more evidence must narrow the band: {known:?} vs {unknown:?}"
    );
    assert!(
        known.confidence > unknown.confidence,
        "more evidence must raise confidence: {known:?} vs {unknown:?}"
    );
    assert!(
        known.confidence <= 0.95,
        "confidence must never claim certainty, got {}",
        known.confidence
    );
    assert!(
        known.low <= 0.8 && 0.8 <= known.high,
        "0.8 lies in {known:?}"
    );

    assert!(matches!(
        readiness_impl(&guard, "learner-ghost").expect_err("unknown learner"),
        ServiceError::NotFound(_)
    ));
}

// ---------------------------------------------------------------------------
// The shared handle.
// ---------------------------------------------------------------------------

#[test]
fn the_shared_handle_serializes_access_and_stays_usable() {
    let (_dir, state) = open_state("handle");
    let learner = seeded_profile(&state, "Handle Learner");

    // Sequential acquisitions must all succeed, and the guard must be released
    // when dropped or the second acquisition would deadlock.
    for _ in 0..8 {
        let guard = state.db().expect("acquire");
        get_profile_impl(&guard, &learner).expect("read through the handle");
        drop(guard);
    }

    assert_eq!(
        list_profiles_impl(&state.db().expect("acquire"))
            .expect("list")
            .len(),
        1
    );
}
