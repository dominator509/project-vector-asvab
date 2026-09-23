//! EP-007: performance and recovery harness.
//!
//! The execplan requires "performance and recovery harness" proof. These tests
//! assert *outcomes within a bound* rather than reporting timings, so a
//! regression that makes a hot path pathologically slow fails the suite.
//!
//! Two kinds of bound appear here, and the difference is deliberate:
//!
//! * **CPU-bound work** (redaction, planning, scheduling, screening, archive
//!   parsing) uses a generous absolute wall-clock bound. That work is dominated
//!   by this process, so a bound set orders of magnitude above the observed cost
//!   still catches an accidental O(n^2) scan or a per-iteration resource
//!   construction — the class of defect this suite found in the redaction path.
//!
//! * **Durable writes** are dominated by the disk's fsync latency, because the
//!   database runs `PRAGMA synchronous = FULL` and every insert commits. An
//!   absolute bound there measures the host rather than the code: with identical
//!   source, one such test took seconds locally and 22.96 s on a shared CI
//!   runner. It asserts *scaling* instead — that per-insert cost does not grow
//!   as the table does — which is a property of the code and holds on any disk,
//!   and which is what an O(n) insert defect actually looks like.

use std::time::{Duration, Instant};

/// Run `body` and assert it finished within `limit`.
fn within<F: FnOnce() -> R, R>(label: &str, limit: Duration, body: F) -> R {
    let start = Instant::now();
    let result = body();
    let elapsed = start.elapsed();
    assert!(
        elapsed <= limit,
        "{label} took {elapsed:?}, exceeding the {limit:?} bound"
    );
    result
}

// ---------------------------------------------------------------------------
// Redaction throughput
// ---------------------------------------------------------------------------

/// Serialise the timing-sensitive measurements in this file against each other.
///
/// The test harness runs the tests in one binary on parallel threads, and the suites beside this
/// one hammer the same disk. Measured in round 34: `many_persisted_attempts_stay_within_budget`
/// grew 1.08x on a quiet machine and tripped its 5x threshold during the full sweep, because the
/// first two hundred and fifty durable inserts competed with nine sibling suites and the last two
/// hundred and fifty did not. The threshold was not the problem and lowering it would be; the
/// measurement needed the machine to itself, so these tests take a lock for the part they time.
fn measurement_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn redacting_many_captures_stays_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_observability::crash::{redact_capture, CanaryRegistry, CrashCapture};

    let mut canaries = CanaryRegistry::new();
    canaries
        .register("k", "canary-value-1234")
        .expect("register");

    let capture = CrashCapture {
        id: "perf".to_string(),
        summary: "crash while loading content pack canary-value-1234".to_string(),
        message: "error: password=secret123 at 2026-09-10".to_string(),
        stack: "at auth (client.rs:42)\n  Bearer abcdefghijklmnop\n  AKIAIOSFODNN7EXAMPLE"
            .to_string(),
        context: vec![
            ("api_key".to_string(), "sk-abcdefghijklmnopqrst".to_string()),
            ("subtest".to_string(), "AR".to_string()),
        ],
        log_ring: (0..50)
            .map(|i| format!("line {i} with canary-value-1234"))
            .collect(),
        build_id: "build-1".to_string(),
        redaction_passes: 0,
    };

    // 500 captures each with a 50-line log ring: 25,000 lines of redaction work.
    within("redacting 500 captures", Duration::from_secs(10), || {
        for _ in 0..500 {
            let outcome = redact_capture(&capture, &canaries);
            assert!(!outcome.capture.summary.contains("canary-value-1234"));
        }
    });
}

#[test]
fn verifying_many_captures_stays_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_observability::crash::{
        redact_capture, verify_no_secrets, CanaryRegistry, CrashCapture,
    };

    let mut canaries = CanaryRegistry::new();
    canaries
        .register("k", "canary-value-1234")
        .expect("register");

    let capture = CrashCapture {
        id: "perf".to_string(),
        summary: "crash canary-value-1234".to_string(),
        message: "detail".to_string(),
        stack: "at x".to_string(),
        context: vec![],
        log_ring: (0..50).map(|i| format!("line {i}")).collect(),
        build_id: "build-1".to_string(),
        redaction_passes: 0,
    };
    let redacted = redact_capture(&capture, &canaries).capture;

    within("verifying 500 captures", Duration::from_secs(10), || {
        for _ in 0..500 {
            verify_no_secrets(&redacted, &canaries).expect("clean");
        }
    });
}

// ---------------------------------------------------------------------------
// Ingestion throughput
// ---------------------------------------------------------------------------

#[test]
fn screening_many_payloads_stays_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_questions::ingestion::{sandbox_parse, License, SourcePayload, TrustTier};

    let payload = SourcePayload {
        source_id: "SRC-PERF".to_string(),
        media_type: "text/markdown".to_string(),
        license: License::PublicDomain,
        trust_tier: TrustTier::A,
        text: "A worked example about unit conversion. ".repeat(200),
        claims_official_items: false,
    };

    within("screening 2,000 payloads", Duration::from_secs(10), || {
        for _ in 0..2_000 {
            sandbox_parse(&payload, TrustTier::D).expect("accepted");
        }
    });
}

#[test]
fn archive_entry_checks_stay_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_questions::ingestion::check_archive_entry;

    within("5,000 entry checks", Duration::from_secs(5), || {
        for i in 0..5_000 {
            let entry = format!("content/dir{}/file{i}.md", i % 50);
            check_archive_entry(&entry, "content").expect("contained");
        }
    });
}

// ---------------------------------------------------------------------------
// Scheduling and planning throughput
// ---------------------------------------------------------------------------

#[test]
fn scheduling_many_reviews_stays_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_study::fsrs::{CardState, Fsrs, FsrsParameters, Rating};

    let fsrs = Fsrs::new(FsrsParameters::default()).expect("valid");
    let mut state = CardState::default();

    within("10,000 scheduled reviews", Duration::from_secs(5), || {
        for i in 0..10_000 {
            let rating = match i % 4 {
                0 => Rating::Again,
                1 => Rating::Hard,
                2 => Rating::Good,
                _ => Rating::Easy,
            };
            let out = fsrs.review(&state, rating, state.stability.max(1.0));
            state = out.state;
        }
    });
}

#[test]
fn planning_many_sessions_stays_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_study::selection::{generate_plan, PlanGoal, SkillEstimate};

    let skills: Vec<SkillEstimate> = (0..10)
        .map(|i| SkillEstimate {
            subtest: format!("S{i}"),
            mastery: 0.5,
            uncertainty: 0.3,
            due_reviews: (i * 3) as u32,
        })
        .collect();

    within("5,000 plans", Duration::from_secs(5), || {
        for _ in 0..5_000 {
            generate_plan(&skills, &PlanGoal::Afqt(70), 120).expect("plan");
        }
    });
}

// ---------------------------------------------------------------------------
// Persistence throughput (real SQLite round trips)
// ---------------------------------------------------------------------------

#[test]
fn many_persisted_attempts_stay_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_persistence::repo::AttemptRepo;
    use vector_persistence::{Database, MigrationManager};

    let dir = std::env::temp_dir().join(format!("vec-perf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("perf.db");

    let mut db = Database::open(&path).expect("open");
    let migrations =
        MigrationManager::load_from_dir(std::path::Path::new("../../migrations")).expect("load");
    MigrationManager::apply(&mut db, &migrations).expect("migrate");
    db.connection()
        .execute(
            "INSERT INTO learner_profile (id, name, target_score) VALUES ('L','Perf',50)",
            [],
        )
        .expect("seed learner");

    let repo = AttemptRepo::new(&db);

    // What this test can and cannot measure.
    //
    // A durable insert here costs one fsync: the database runs
    // `PRAGMA synchronous = FULL`, and every `record` commits. An absolute
    // wall-clock bound for that workload therefore measures the *host disk*.
    // With identical code this test passed locally in seconds and failed on a
    // shared CI runner at 22.96 s for 2,000 inserts. DOD-022 requires a
    // threshold to be set against a defined environment, and no environment is
    // declared for this suite, so an absolute budget calibrated on one machine
    // cannot be a product criterion — it distinguishes disks, not code.
    //
    // The defect the suite exists to catch is a per-insert scan that makes
    // writing O(n): an accidental full-table lookup, a missing index, a
    // resource built per iteration. That shows up as per-insert cost *growing*
    // as the table grows, which is measurable without knowing how fast the disk
    // is, by comparing two samples from the same run on the same hardware.
    const TOTAL: usize = 2_000;
    // Larger than it needs to be for the signal, because each sample is a
    // timing sum on a shared machine and a bigger sample is a steadier one.
    const SAMPLE: usize = 250;
    // A constant-time insert stays flat across the two samples. A per-insert
    // scan over 2,000 rows grows by roughly 20x between them, so five leaves
    // room for scheduler noise on a shared runner while still failing on the
    // defect class.
    //
    // Both samples and the ratio are in the assertion message, so a failure
    // carries the measurement. On a passing run libtest captures test stdout,
    // and this line appears only when the suite is run with `--nocapture`; the
    // numbers are not in the CI log of a green run, and this comment says so
    // rather than implying otherwise.
    const MAX_GROWTH: f64 = 5.0;

    let mut first_sample = Duration::ZERO;
    let mut last_sample = Duration::ZERO;
    let started = Instant::now();

    for i in 0..TOTAL {
        let insert_started = Instant::now();
        repo.record("L", "AR", &format!("q{i}"), i % 2 == 0, 1000 + i as i64)
            .expect("record");
        let insert_elapsed = insert_started.elapsed();
        if i < SAMPLE {
            first_sample += insert_elapsed;
        }
        if i >= TOTAL - SAMPLE {
            last_sample += insert_elapsed;
        }
    }

    let total = started.elapsed();
    let growth = last_sample.as_secs_f64() / first_sample.as_secs_f64().max(1e-9);
    eprintln!(
        "persistence throughput: {TOTAL} durable inserts in {total:?}; \
         first {SAMPLE} {first_sample:?}, last {SAMPLE} {last_sample:?}, growth {growth:.2}x"
    );

    // A hang guard, not a performance criterion: loose enough to hold on the
    // slowest supported disk, tight enough that a stall fails instead of
    // running until the job timeout.
    assert!(
        total <= Duration::from_secs(300),
        "writing {TOTAL} attempts took {total:?}; this bound exists only to fail a hang"
    );

    assert!(
        growth <= MAX_GROWTH,
        "per-insert cost grew {growth:.2}x between the first and last {SAMPLE} inserts \
         (first {first_sample:?}, last {last_sample:?}); writing an attempt must not get \
         slower as the table grows"
    );

    let stats = repo.analytics("L", "AR").expect("analytics");
    assert_eq!(stats.total, 2_000, "every attempt must persist");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn analytics_over_a_large_table_stays_within_budget() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_persistence::repo::AttemptRepo;
    use vector_persistence::{Database, MigrationManager};

    let dir = std::env::temp_dir().join(format!("vec-perf2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("perf.db");

    let mut db = Database::open(&path).expect("open");
    let migrations =
        MigrationManager::load_from_dir(std::path::Path::new("../../migrations")).expect("load");
    MigrationManager::apply(&mut db, &migrations).expect("migrate");
    db.connection()
        .execute(
            "INSERT INTO learner_profile (id, name, target_score) VALUES ('L','Perf',50)",
            [],
        )
        .expect("seed learner");

    {
        let conn = db.connection();
        conn.execute_batch("BEGIN").expect("begin");
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO attempts
                     (id, learner_id, subtest, question_id, correct, latency_ms, created_at)
                     VALUES (?1,'L','AR',?2,?3,?4,'2026-09-10T00:00:00Z')",
                )
                .expect("prepare");
            for i in 0..20_000 {
                stmt.execute(rusqlite::params![
                    format!("a{i}"),
                    format!("q{i}"),
                    (i % 2) as i64,
                    1000i64
                ])
                .expect("insert");
            }
        }
        conn.execute_batch("COMMIT").expect("commit");
    }

    let repo = AttemptRepo::new(&db);
    let stats = within(
        "aggregating 20,000 attempts",
        Duration::from_secs(5),
        || repo.analytics("L", "AR").expect("analytics"),
    );
    assert_eq!(stats.total, 20_000);

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Recovery: the app remains usable after forced termination
// ---------------------------------------------------------------------------

#[test]
fn a_database_survives_forced_termination_and_reopens() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_persistence::repo::AttemptRepo;
    use vector_persistence::{Database, MigrationManager};

    let dir = std::env::temp_dir().join(format!("vec-recover-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("recover.db");

    {
        let mut db = Database::open(&path).expect("open");
        let migrations = MigrationManager::load_from_dir(std::path::Path::new("../../migrations"))
            .expect("load");
        MigrationManager::apply(&mut db, &migrations).expect("migrate");
        db.connection()
            .execute(
                "INSERT INTO learner_profile (id, name, target_score) VALUES ('L','Ada',72)",
                [],
            )
            .expect("seed");
        AttemptRepo::new(&db)
            .record("L", "AR", "q1", true, 1200)
            .expect("record");
        // Dropping the handle without any clean shutdown call simulates the
        // process being terminated: WAL recovery is what must save the data.
    }

    let mut db = Database::open(&path).expect("reopen after forced termination");
    let migrations =
        MigrationManager::load_from_dir(std::path::Path::new("../../migrations")).expect("load");
    MigrationManager::apply(&mut db, &migrations).expect("migrate");

    let stats = AttemptRepo::new(&db)
        .analytics("L", "AR")
        .expect("analytics");
    assert_eq!(
        stats.total, 1,
        "committed data must survive an unclean shutdown"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_interrupted_migration_leaves_a_usable_database() {
    // Timed: see `measurement_lock` for why this is serialised.
    let _measured = measurement_lock();
    use vector_persistence::{Database, MigrationManager};

    let dir = std::env::temp_dir().join(format!("vec-migfail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("migfail.db");

    let mut db = Database::open(&path).expect("open");
    let good = (
        1i64,
        "CREATE TABLE keep_me (id INTEGER PRIMARY KEY);".to_string(),
    );
    let bad = (2i64, "THIS IS NOT VALID SQL;".to_string());

    assert!(
        MigrationManager::apply(&mut db, &[good, bad]).is_err(),
        "an invalid migration must fail"
    );

    let versions = MigrationManager::applied_versions(&db).expect("versions");
    assert_eq!(versions, vec![1], "the successful migration stays applied");

    // The database is still usable afterwards.
    let mut db = Database::open(&path).expect("reopen");
    MigrationManager::apply(
        &mut db,
        &[(
            2i64,
            "CREATE TABLE later (id INTEGER PRIMARY KEY);".to_string(),
        )],
    )
    .expect("a corrected migration applies");
    assert!(MigrationManager::applied_versions(&db)
        .expect("versions")
        .contains(&2));

    let _ = std::fs::remove_dir_all(&dir);
}
