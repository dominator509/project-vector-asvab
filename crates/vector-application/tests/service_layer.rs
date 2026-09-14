//! EP-004/EP-005 acceptance: the application service layer.
//!
//! These are the services the desktop UI invokes through the Tauri boundary.
//! SPEC-003 makes this the typed service boundary, so its behaviour is what the
//! UI actually depends on — testing it here is what lets the Tauri layer stay a
//! thin, mechanical adapter.

use std::path::Path;

use vector_application::service::{NewEvidenceDto, ServiceError, Services};
use vector_persistence::{Database, Migration, MigrationManager};

/// A disposable database directory.
struct TempDb {
    dir: std::path::PathBuf,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "vector-svc-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self { dir }
    }

    /// Open a migrated database at `vector.db` inside this directory.
    fn open(&self) -> Database {
        let mut db = Database::open(self.dir.join("vector.db")).expect("open db");
        let migrations: Vec<Migration> =
            MigrationManager::load_from_dir(Path::new("../../migrations"))
                .expect("load migrations");
        Services::migrate(&mut db, &migrations).expect("migrate");
        db
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

#[test]
fn a_profile_can_be_created_and_read_back() {
    let tmp = TempDb::new("profile");
    let db = tmp.open();
    let services = Services::new(&db);

    let created = services.create_profile("Ada", 72).expect("create");
    assert_eq!(created.name, "Ada");
    assert_eq!(created.target_score, 72);

    let read = services.get_profile(&created.id).expect("get");
    assert_eq!(read, created, "the stored profile must match exactly");
}

#[test]
fn a_profile_name_is_required() {
    let tmp = TempDb::new("name");
    let db = tmp.open();
    let services = Services::new(&db);

    assert!(matches!(
        services.create_profile("   ", 50),
        Err(ServiceError::Invalid(_))
    ));
}

#[test]
fn a_target_outside_the_reporting_scale_is_refused() {
    // Composite scores are reported on 1..99. A target outside that range can
    // never be met, so it is a caller error rather than a valid goal.
    let tmp = TempDb::new("target");
    let db = tmp.open();
    let services = Services::new(&db);

    for target in [0u32, 100] {
        assert!(
            matches!(
                services.create_profile("Ada", target),
                Err(ServiceError::Invalid(_))
            ),
            "target {target} must be refused"
        );
    }
}

#[test]
fn an_unknown_profile_reports_not_found() {
    let tmp = TempDb::new("missing");
    let db = tmp.open();
    let services = Services::new(&db);

    assert!(matches!(
        services.get_profile("nobody"),
        Err(ServiceError::NotFound(_))
    ));
}

#[test]
fn profiles_are_listed_by_name() {
    let tmp = TempDb::new("list");
    let db = tmp.open();
    let services = Services::new(&db);

    services.create_profile("Zoe", 50).expect("create");
    services.create_profile("Ada", 50).expect("create");

    let names: Vec<String> = services
        .list_profiles()
        .expect("list")
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, vec!["Ada", "Zoe"]);
}

// ---------------------------------------------------------------------------
// Attempts and analytics
// ---------------------------------------------------------------------------

#[test]
fn an_attempt_is_recorded_and_aggregated() {
    let tmp = TempDb::new("attempt");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    assert!(services
        .record_attempt("a1", &profile.id, "AR", "q1", true, 1200)
        .expect("record"));
    assert!(services
        .record_attempt("a2", &profile.id, "AR", "q2", false, 3000)
        .expect("record"));

    let stats = services.analytics(&profile.id, "AR").expect("analytics");
    assert_eq!(stats.total, 2);
    assert_eq!(stats.correct, 1);
    assert!((stats.accuracy - 0.5).abs() < 1e-9);
    assert_eq!(stats.mean_latency_ms, 2100);
}

#[test]
fn a_replayed_attempt_is_idempotent() {
    // The UI can retry a submission after a dropped response; that must not
    // duplicate the learner's history.
    let tmp = TempDb::new("idem");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    assert!(services
        .record_attempt("a1", &profile.id, "AR", "q1", true, 1200)
        .expect("first"));
    assert!(
        !services
            .record_attempt("a1", &profile.id, "AR", "q1", true, 1200)
            .expect("replay"),
        "a replayed attempt must report that nothing was inserted"
    );

    assert_eq!(
        services
            .analytics(&profile.id, "AR")
            .expect("analytics")
            .total,
        1
    );
}

#[test]
fn an_attempt_for_an_unknown_learner_is_refused_clearly() {
    // Fail with a typed NotFound rather than surfacing a raw foreign-key
    // violation from SQLite.
    let tmp = TempDb::new("orphan");
    let db = tmp.open();
    let services = Services::new(&db);

    assert!(matches!(
        services.record_attempt("a1", "nobody", "AR", "q1", true, 100),
        Err(ServiceError::NotFound(_))
    ));
}

#[test]
fn a_negative_latency_is_refused() {
    let tmp = TempDb::new("latency");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    assert!(matches!(
        services.record_attempt("a1", &profile.id, "AR", "q1", true, -5),
        Err(ServiceError::Invalid(_))
    ));
}

#[test]
fn an_attempt_without_an_id_is_refused() {
    let tmp = TempDb::new("noid");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    assert!(matches!(
        services.record_attempt("  ", &profile.id, "AR", "q1", true, 100),
        Err(ServiceError::Invalid(_))
    ));
}

#[test]
fn analytics_only_reports_subtests_with_attempts() {
    let tmp = TempDb::new("allstats");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    services
        .record_attempt("a1", &profile.id, "WK", "q1", true, 900)
        .expect("record");

    let all = services.analytics_all(&profile.id).expect("all");
    assert_eq!(all.len(), 1, "only attempted subtests are reported");
    assert_eq!(all[0].0, "WK");
}

// ---------------------------------------------------------------------------
// Mastery
// ---------------------------------------------------------------------------

#[test]
fn mastery_can_be_set_and_read() {
    let tmp = TempDb::new("mastery");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    services
        .set_mastery(&profile.id, "AR", 0.42, 0.31)
        .expect("set");

    let records = services.mastery(&profile.id).expect("read");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].subtest, "AR");
    assert!((records[0].score - 0.42).abs() < 1e-9);
    assert!((records[0].uncertainty - 0.31).abs() < 1e-9);
}

#[test]
fn mastery_outside_the_unit_interval_is_refused() {
    let tmp = TempDb::new("mastery-range");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    assert!(matches!(
        services.set_mastery(&profile.id, "AR", 1.5, 0.1),
        Err(ServiceError::Invalid(_))
    ));
    assert!(matches!(
        services.set_mastery(&profile.id, "AR", 0.5, -0.1),
        Err(ServiceError::Invalid(_))
    ));
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

#[test]
fn a_new_learner_gets_a_plan_covering_the_unstudied_subtests() {
    // A learner with no history has nothing known, so the planner should look
    // broadly rather than returning an empty plan.
    let tmp = TempDb::new("plan-new");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    let plan = services.study_plan(&profile.id, 70, 60).expect("plan");

    assert!(!plan.drills.is_empty(), "a new learner needs a plan");
    assert!(
        plan.total_minutes <= 60,
        "the plan must fit the available time"
    );
    assert!(plan.drills.iter().all(|d| d.minutes > 0));
}

#[test]
fn a_learner_at_goal_with_no_due_work_gets_an_empty_plan() {
    // Anti-padding: the service must be able to say "nothing to do" rather than
    // inventing busy-work.
    let tmp = TempDb::new("plan-done");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 40).expect("create");
    for subtest in vector_domain::mastery::Subtest::ALL {
        services
            .set_mastery(&profile.id, subtest.code(), 0.99, 0.0)
            .expect("set mastery");
    }

    let plan = services.study_plan(&profile.id, 40, 60).expect("plan");
    assert!(
        plan.drills.is_empty(),
        "a learner at goal with nothing due must not be given busy-work: {:?}",
        plan.drills
    );
    assert_eq!(plan.total_minutes, 0);
}

#[test]
fn a_plan_for_an_unknown_learner_is_refused() {
    let tmp = TempDb::new("plan-unknown");
    let db = tmp.open();
    let services = Services::new(&db);

    assert!(matches!(
        services.study_plan("nobody", 70, 30),
        Err(ServiceError::NotFound(_))
    ));
}

#[test]
fn a_plan_with_no_available_time_is_refused() {
    let tmp = TempDb::new("plan-zero");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    assert!(matches!(
        services.study_plan(&profile.id, 70, 0),
        Err(ServiceError::Invalid(_))
    ));
}

// ---------------------------------------------------------------------------
// Readiness
// ---------------------------------------------------------------------------

#[test]
fn readiness_is_a_band_and_never_an_official_score_claim() {
    // ADR-010: no precise predicted official score before a calibration cohort
    // exists. The payload carries the flag so the UI cannot invent one without
    // the claim being visible in the data.
    let tmp = TempDb::new("readiness");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    let band = services.readiness(&profile.id).expect("readiness");

    assert!(!band.official_score_claim);
    assert!(band.low <= band.high);
    assert!((0.0..=1.0).contains(&band.low));
    assert!((0.0..=1.0).contains(&band.high));
    assert!(
        band.confidence < 1.0,
        "confidence must never claim certainty"
    );
}

#[test]
fn readiness_for_an_unknown_learner_is_refused() {
    let tmp = TempDb::new("readiness-unknown");
    let db = tmp.open();
    let services = Services::new(&db);

    assert!(matches!(
        services.readiness("nobody"),
        Err(ServiceError::NotFound(_))
    ));
}

// ---------------------------------------------------------------------------
// Persistence across restart
// ---------------------------------------------------------------------------

#[test]
fn service_state_survives_a_restart() {
    // The UI closes and reopens; nothing the learner did may be lost.
    let tmp = TempDb::new("restart");

    let id = {
        let db = tmp.open();
        let services = Services::new(&db);
        let profile = services.create_profile("Ada", 72).expect("create");
        services
            .record_attempt("a1", &profile.id, "AR", "q1", true, 1200)
            .expect("record");
        services
            .set_mastery(&profile.id, "AR", 0.55, 0.2)
            .expect("mastery");
        profile.id
    };

    // Reopen from scratch, as a fresh launch would.
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.get_profile(&id).expect("profile survives");
    assert_eq!(profile.name, "Ada");
    assert_eq!(profile.target_score, 72);
    assert_eq!(
        services.analytics(&id, "AR").expect("analytics").total,
        1,
        "attempts must survive a restart"
    );
    assert_eq!(services.mastery(&id).expect("mastery").len(), 1);
}

// ---------------------------------------------------------------------------
// Evidence vault (REQ-020)
// ---------------------------------------------------------------------------

fn evidence(url: &str, hash: &str, status: &str) -> NewEvidenceDto {
    NewEvidenceDto {
        url: url.to_string(),
        title: "Official source".to_string(),
        content_hash: hash.to_string(),
        license: "public-domain".to_string(),
        effective_date: "2025-01-01".to_string(),
        trust: 0.9,
        retrieval_status: status.to_string(),
    }
}

#[test]
fn evidence_is_stored_read_back_and_listed() {
    let tmp = TempDb::new("evidence");
    let db = tmp.open();
    let services = Services::new(&db);

    let id = services
        .evidence_put(&evidence(
            "https://example.test/a",
            "sha256:aaa",
            "retrieved",
        ))
        .expect("store");

    let fetched = services.evidence_get(&id).expect("read back");
    assert_eq!(fetched.content_hash, "sha256:aaa");
    assert_eq!(fetched.trust, 0.9);

    let listed = services.evidence_list().expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
}

#[test]
fn identical_content_does_not_fork_provenance() {
    let tmp = TempDb::new("evidence-dedupe");
    let db = tmp.open();
    let services = Services::new(&db);

    let first = services
        .evidence_put(&evidence(
            "https://example.test/a",
            "sha256:same",
            "retrieved",
        ))
        .expect("first");
    // The same content retrieved from a different address is the same evidence:
    // the vault is hash-addressed, so it must not create a second identity.
    let second = services
        .evidence_put(&evidence(
            "https://mirror.test/a",
            "sha256:same",
            "retrieved",
        ))
        .expect("second");

    assert_eq!(first, second);
    assert_eq!(services.evidence_list().expect("list").len(), 1);
}

#[test]
fn an_unhashable_source_is_refused() {
    let tmp = TempDb::new("evidence-invalid");
    let db = tmp.open();
    let services = Services::new(&db);

    // Without a hash the snapshot cannot be re-verified, so it is not evidence.
    let error = services
        .evidence_put(&evidence("https://example.test/a", "   ", "retrieved"))
        .expect_err("a blank content hash must be refused");
    assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");

    let error = services
        .evidence_put(&NewEvidenceDto {
            trust: 1.5,
            ..evidence("https://example.test/a", "sha256:x", "retrieved")
        })
        .expect_err("trust outside [0,1] must be refused");
    assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");

    assert!(services.evidence_list().expect("list").is_empty());
}

#[test]
fn an_unknown_evidence_id_reports_not_found() {
    let tmp = TempDb::new("evidence-missing");
    let db = tmp.open();
    let services = Services::new(&db);

    let error = services
        .evidence_get("ev-nope")
        .expect_err("must not resolve");
    assert!(matches!(error, ServiceError::NotFound(_)), "got {error:?}");
}

// ---------------------------------------------------------------------------
// Backup, restore, listing and erasure (REQ-030, REQ-033, REQ-034, REQ-035)
// ---------------------------------------------------------------------------

#[test]
fn a_backup_is_written_listed_and_restored_over_dirty_state() {
    let tmp = TempDb::new("backup");
    let backups = tmp.dir.join("backups");

    let mut db = tmp.open();
    let profile_id = {
        let services = Services::new(&db);
        let profile = services.create_profile("Ada", 70).expect("create");
        services
            .record_attempt("a1", &profile.id, "AR", "q1", true, 1000)
            .expect("record");
        profile.id.clone()
    };

    let manifest = Services::new(&db)
        .backup_create(&backups)
        .expect("create backup");
    assert_eq!(manifest.integrity, "ok");
    assert_eq!(manifest.attempt_rows, 1);
    assert_eq!(manifest.checksum.len(), 64);

    let listed = Services::new(&db).backup_list(&backups).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, manifest.path);
    assert_eq!(listed[0].checksum, manifest.checksum);

    // Dirty the live state so the restore has something real to undo.
    {
        let services = Services::new(&db);
        services
            .record_attempt("a2", &profile_id, "AR", "q2", false, 500)
            .expect("record");
        assert_eq!(
            services
                .analytics(&profile_id, "AR")
                .expect("analytics")
                .total,
            2
        );
    }

    let restored =
        Services::backup_restore(&mut db, Path::new(&manifest.path), Some(&manifest.checksum))
            .expect("restore");
    assert_eq!(restored.rows, 1, "the restore must undo the later attempt");
    assert_eq!(
        Services::new(&db)
            .analytics(&profile_id, "AR")
            .expect("analytics")
            .total,
        1
    );
}

#[test]
fn a_backup_whose_digest_does_not_match_is_refused_and_changes_nothing() {
    let tmp = TempDb::new("backup-tamper");
    let backups = tmp.dir.join("backups");

    let mut db = tmp.open();
    let profile_id = {
        let services = Services::new(&db);
        let profile = services.create_profile("Ada", 70).expect("create");
        services
            .record_attempt("a1", &profile.id, "AR", "q1", true, 1000)
            .expect("record");
        profile.id.clone()
    };

    let manifest = Services::new(&db)
        .backup_create(&backups)
        .expect("create backup");

    // Flip one byte in the middle of the archive. SQLite's structural
    // integrity_check can miss this, which is why the digest is required.
    let tampered = tmp.dir.join("tampered.db");
    let mut bytes = std::fs::read(&manifest.path).expect("read archive");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    std::fs::write(&tampered, bytes).expect("write tampered archive");

    let error = Services::backup_restore(&mut db, &tampered, Some(&manifest.checksum))
        .expect_err("a tampered archive must be refused");
    assert!(
        error
            .to_string()
            .contains("does not match the recorded digest"),
        "the refusal must name the real cause, got {error}"
    );

    // Live state must be untouched by the refusal.
    let services = Services::new(&db);
    assert_eq!(
        services
            .analytics(&profile_id, "AR")
            .expect("analytics")
            .total,
        1
    );
    assert_eq!(
        services.get_profile(&profile_id).expect("profile").name,
        "Ada"
    );
}

#[test]
fn a_missing_archive_reports_not_found() {
    let tmp = TempDb::new("backup-missing");
    let mut db = tmp.open();
    let error = Services::backup_restore(&mut db, Path::new("/nonexistent/nope.db"), None)
        .expect_err("must not resolve");
    assert!(matches!(error, ServiceError::NotFound(_)), "got {error:?}");
}

#[test]
fn listing_a_directory_that_does_not_exist_is_empty_not_an_error() {
    let tmp = TempDb::new("backup-nodir");
    let db = tmp.open();
    let listed = Services::new(&db)
        .backup_list(&tmp.dir.join("never-created"))
        .expect("an absent backup directory is a normal state");
    assert!(listed.is_empty());
}

#[test]
fn erasing_local_data_requires_the_exact_phrase_and_empties_every_table() {
    let tmp = TempDb::new("reset");
    let db = tmp.open();
    let services = Services::new(&db);

    let profile = services.create_profile("Ada", 70).expect("create");
    services
        .record_attempt("a1", &profile.id, "AR", "q1", true, 1000)
        .expect("record");
    services
        .set_mastery(&profile.id, "AR", 0.5, 0.2)
        .expect("mastery");
    services
        .evidence_put(&evidence(
            "https://example.test/a",
            "sha256:aaa",
            "retrieved",
        ))
        .expect("evidence");

    // A near miss must not unlock a destructive operation.
    for phrase in ["delete", "Delete", " DELETE", "DELETE ", "DELETE!"] {
        let error = services
            .reset_local_data(phrase)
            .expect_err("a near-miss phrase must be refused");
        assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");
        assert_eq!(
            services.list_profiles().expect("list").len(),
            1,
            "a refused erase must not have removed anything"
        );
    }

    let outcome = services.reset_local_data("DELETE").expect("erase");
    assert_eq!(outcome.profiles_removed, 1);
    assert_eq!(outcome.attempts_removed, 1);
    assert_eq!(outcome.evidence_removed, 1);

    // Read back from live state: the claim is that the rows are gone.
    assert_eq!(outcome.profiles_remaining, 0);
    assert_eq!(outcome.attempts_remaining, 0);
    assert_eq!(outcome.evidence_remaining, 0);
    assert!(services.list_profiles().expect("list").is_empty());
    assert!(services.evidence_list().expect("list").is_empty());

    // The schema must still be usable afterwards, not merely empty.
    let recreated = services.create_profile("Grace", 60).expect("recreate");
    assert_eq!(recreated.target_score, 60);
}

// ---------------------------------------------------------------------------
// SLO probe (REQ-039)
// ---------------------------------------------------------------------------

#[test]
fn the_latency_probe_measures_the_real_database() {
    let tmp = TempDb::new("slo");
    let db = tmp.open();
    let services = Services::new(&db);
    services.create_profile("Ada", 70).expect("create");

    let sample = services.latency_probe(25).expect("probe");
    assert_eq!(sample.iterations, 25);
    assert_eq!(sample.operation, "select_count_learner_profile");
    assert!(sample.max_micros >= sample.mean_micros);
    // A local indexed count is far below this; the bound catches a regression
    // that makes a hot read path pathological, not machine speed.
    assert!(
        sample.mean_micros < 20_000,
        "mean {} us is outside the bound",
        sample.mean_micros
    );

    let error = services
        .latency_probe(0)
        .expect_err("zero iterations is not a probe");
    assert!(matches!(error, ServiceError::Invalid(_)), "got {error:?}");
}
