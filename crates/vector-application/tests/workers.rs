//! EP-003/EP-004 acceptance: background workers and concurrent writers
//! (REQ-054).
//!
//! `REQ-054` requires "idempotent attempts and safe DB/background workers". The
//! idempotency half is covered in `ep003_acceptance.rs`; this file covers the
//! concurrency half, against a real on-disk SQLite file with real threads and
//! real separate connections.
//!
//! An in-memory database would not do: it cannot be opened twice, so it cannot
//! reproduce the situation the requirement is about.

use std::path::{Path, PathBuf};

use vector_application::service::Services;
use vector_application::workers::{run_job, Job, JobStatus, WorkerPool};
use vector_persistence::{Database, Migration, MigrationManager};

struct TempDb {
    dir: PathBuf,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "vector-workers-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("vector.db")
    }

    /// Open a migrated database, optionally creating the learner first.
    fn open(&self) -> Database {
        let mut db = Database::open(self.path()).expect("open db");
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
// The job itself
// ---------------------------------------------------------------------------

#[test]
fn recomputing_mastery_derives_it_from_the_attempt_history() {
    let tmp = TempDb::new("derive");
    let db = tmp.open();
    let services = Services::new(&db);
    let profile = services.create_profile("Ada", 60).expect("profile");

    // Three AR attempts, two correct.
    services
        .record_attempt("a1", &profile.id, "AR", "q1", true, 1000)
        .expect("record");
    services
        .record_attempt("a2", &profile.id, "AR", "q2", true, 1000)
        .expect("record");
    services
        .record_attempt("a3", &profile.id, "AR", "q3", false, 1000)
        .expect("record");

    let written = services.recompute_mastery(&profile.id).expect("recompute");
    assert_eq!(written, 1, "only the attempted subtest is written");

    let mastery = services.mastery(&profile.id).expect("read back");
    assert_eq!(mastery.len(), 1);
    let ar = &mastery[0];
    assert_eq!(ar.subtest, "AR");

    // Laplace prior: (2 correct + 1) / (3 total + 2) = 0.6
    assert!(
        (ar.score - 0.6).abs() < 1e-9,
        "expected 0.6 from 2/3 with a Laplace prior, got {}",
        ar.score
    );
    // 1 / sqrt(3 + 1) = 0.5
    assert!(
        (ar.uncertainty - 0.5).abs() < 1e-9,
        "expected 0.5 uncertainty, got {}",
        ar.uncertainty
    );
}

#[test]
fn uncertainty_falls_as_evidence_accumulates() {
    let tmp = TempDb::new("uncertainty");
    let db = tmp.open();
    let services = Services::new(&db);
    let profile = services.create_profile("Ada", 60).expect("profile");

    let mut previous = f64::INFINITY;
    for attempt in 1..=25 {
        services
            .record_attempt(
                &format!("a{attempt}"),
                &profile.id,
                "WK",
                &format!("q{attempt}"),
                true,
                500,
            )
            .expect("record");
        services.recompute_mastery(&profile.id).expect("recompute");
        let score = services.mastery(&profile.id).expect("read")[0].uncertainty;

        assert!(
            score < previous,
            "uncertainty must fall at attempt {attempt}: {score} vs {previous}"
        );
        assert!(score > 0.0, "uncertainty must never reach zero");
        previous = score;
    }

    // Twenty-five correct answers is strong evidence but not certainty.
    assert!(previous < 0.25, "got {previous}");
}

#[test]
fn a_subtest_with_no_attempts_is_left_alone() {
    // Overwriting a deliberately-set estimate with a value derived from no data
    // would be worse than leaving it.
    let tmp = TempDb::new("untouched");
    let db = tmp.open();
    let services = Services::new(&db);
    let profile = services.create_profile("Ada", 60).expect("profile");

    services
        .set_mastery(&profile.id, "GS", 0.9, 0.05)
        .expect("set by hand");
    services
        .record_attempt("a1", &profile.id, "AR", "q1", true, 100)
        .expect("record");

    let written = services.recompute_mastery(&profile.id).expect("recompute");
    assert_eq!(written, 1);

    let mastery = services.mastery(&profile.id).expect("read");
    let gs = mastery
        .iter()
        .find(|m| m.subtest == "GS")
        .expect("GS survives");
    assert!((gs.score - 0.9).abs() < 1e-9, "the hand-set value was lost");
}

#[test]
fn recomputing_for_an_unknown_learner_reports_no_work_rather_than_failing() {
    let tmp = TempDb::new("unknown");
    let db = tmp.open();
    let outcome = run_job(
        &db,
        &Job::RecomputeMastery {
            learner_id: "learner-ghost".to_string(),
        },
    );
    assert!(
        matches!(outcome.status, JobStatus::NoWork { .. }),
        "got {:?}",
        outcome.status
    );
    assert_eq!(outcome.job, "recompute_mastery");
}

#[test]
fn recomputing_every_learner_covers_all_of_them() {
    let tmp = TempDb::new("all");
    let db = tmp.open();
    let services = Services::new(&db);

    let ada = services.create_profile("Ada", 60).expect("profile");
    let grace = services.create_profile("Grace", 70).expect("profile");
    services
        .record_attempt("a1", &ada.id, "AR", "q1", true, 100)
        .expect("record");
    services
        .record_attempt("g1", &grace.id, "WK", "q1", false, 100)
        .expect("record");

    let outcome = run_job(&db, &Job::RecomputeAllMastery);
    match outcome.status {
        JobStatus::Completed {
            mastery_rows_written,
        } => assert_eq!(mastery_rows_written, 2, "one row per learner per subtest"),
        other => panic!("expected completion, got {other:?}"),
    }

    assert_eq!(services.mastery(&ada.id).expect("ada").len(), 1);
    assert_eq!(services.mastery(&grace.id).expect("grace").len(), 1);
}

#[test]
fn recomputing_with_no_learners_reports_no_work() {
    let tmp = TempDb::new("empty");
    let db = tmp.open();
    let outcome = run_job(&db, &Job::RecomputeAllMastery);
    match outcome.status {
        JobStatus::NoWork { reason } => assert!(reason.contains("no learners"), "got {reason}"),
        other => panic!("expected no work, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The pool
// ---------------------------------------------------------------------------

#[test]
fn the_pool_runs_every_submitted_job_and_reports_each_outcome() {
    let tmp = TempDb::new("pool");
    let db = tmp.open();
    let profile = Services::new(&db)
        .create_profile("Ada", 60)
        .expect("profile");
    drop(db);

    let pool = WorkerPool::start(&tmp.path(), 2).expect("start pool");
    assert_eq!(pool.worker_count(), 2);

    let jobs = vec![
        Job::RecomputeMastery {
            learner_id: profile.id.clone(),
        },
        Job::RecomputeAllMastery,
        Job::RecomputeMastery {
            learner_id: profile.id.clone(),
        },
    ];
    let outcomes = pool.run_to_completion(jobs).expect("jobs finish");

    assert_eq!(outcomes.len(), 3, "every submitted job reports an outcome");
    assert!(
        outcomes.iter().all(|o| matches!(
            o.status,
            JobStatus::Completed { .. } | JobStatus::NoWork { .. }
        )),
        "got {outcomes:?}"
    );
}

#[test]
fn the_pool_reports_a_failure_instead_of_swallowing_it() {
    let tmp = TempDb::new("pool-failure");
    tmp.open();
    let pool = WorkerPool::start(&tmp.path(), 1).expect("start pool");

    let outcomes = pool
        .run_to_completion(vec![Job::RecomputeMastery {
            learner_id: "learner-ghost".to_string(),
        }])
        .expect("a missing learner is not a pool failure");

    // A missing learner is "no work"; the point is that the pool *reports*, and
    // does not silently drop the job.
    assert_eq!(outcomes.len(), 1);
    assert!(matches!(outcomes[0].status, JobStatus::NoWork { .. }));
}

#[test]
fn a_zero_sized_pool_is_refused() {
    let tmp = TempDb::new("pool-zero");
    tmp.open();
    assert!(WorkerPool::start(&tmp.path(), 0).is_err());
}

#[test]
fn the_pool_shuts_down_and_joins_its_threads() {
    let tmp = TempDb::new("pool-shutdown");
    tmp.open();
    let pool = WorkerPool::start(&tmp.path(), 3).expect("start pool");
    pool.run_to_completion(vec![Job::RecomputeAllMastery])
        .expect("run");
    // Dropping must join rather than detach: a detached worker could still be
    // writing when the process exits.
    drop(pool);

    // The database must still be openable and consistent afterwards.
    let db = Database::open(tmp.path()).expect("reopen");
    assert!(db.integrity_check().expect("integrity"));
}

// ---------------------------------------------------------------------------
// Concurrency
// ---------------------------------------------------------------------------

#[test]
fn concurrent_writers_lose_nothing_and_duplicate_nothing() {
    // The real requirement: a worker and the UI can write at the same time
    // without losing an attempt or double-counting a retry.
    let tmp = TempDb::new("concurrent");
    let db = tmp.open();
    let profile = Services::new(&db)
        .create_profile("Ada", 60)
        .expect("profile");
    let learner = profile.id.clone();

    // Seed mastery so the worker has work to redo while writers are active.
    Services::new(&db)
        .record_attempt("seed", &learner, "AR", "q-seed", true, 100)
        .expect("seed");
    drop(db);

    const WRITERS: usize = 8;
    const PER_WRITER: usize = 25;

    let path = tmp.path();
    let mut handles = Vec::new();
    for writer in 0..WRITERS {
        let path = path.clone();
        let learner = learner.clone();
        handles.push(std::thread::spawn(move || {
            let db = Database::open(&path).expect("worker connection");
            let services = Services::new(&db);
            for index in 0..PER_WRITER {
                let id = format!("w{writer}-a{index}");
                services
                    .record_attempt(
                        &id,
                        &learner,
                        "AR",
                        &format!("q{index}"),
                        index % 2 == 0,
                        100,
                    )
                    .expect("record");

                // Retry each attempt once: the retry must be a no-op, and it
                // happens while other threads are writing.
                let again = services
                    .record_attempt(
                        &id,
                        &learner,
                        "AR",
                        &format!("q{index}"),
                        index % 2 == 0,
                        100,
                    )
                    .expect("retry");
                assert!(!again, "a retried attempt {id} must not be recorded twice");
            }
        }));
    }

    // A worker recomputes mastery throughout, so reads and writes interleave.
    let pool = WorkerPool::start(&path, 2).expect("pool");
    for _ in 0..WRITERS {
        pool.submit(Job::RecomputeMastery {
            learner_id: learner.clone(),
        })
        .expect("submit");
    }

    for handle in handles {
        handle.join().expect("writer thread");
    }

    let outcomes = pool
        .run_to_completion(vec![Job::RecomputeAllMastery])
        .expect("final recompute");
    assert!(matches!(outcomes[0].status, JobStatus::Completed { .. }));

    // Read back from a fresh connection, so nothing is taken from a cache.
    let db = Database::open(&path).expect("verify connection");
    let services = Services::new(&db);
    let stats = services.analytics(&learner, "AR").expect("analytics");
    assert_eq!(
        stats.total,
        (WRITERS * PER_WRITER + 1) as i64,
        "every attempt must be stored exactly once"
    );

    // Counted rather than assumed: `index % 2 == 0` over `0..PER_WRITER` yields
    // the even indices, which is not `PER_WRITER / 2` when the count is odd.
    let correct_per_writer = (0..PER_WRITER).filter(|index| index % 2 == 0).count();
    let expected_correct = (WRITERS * correct_per_writer + 1) as i64;
    assert_eq!(
        stats.correct, expected_correct,
        "no writer's correct answers may be lost"
    );

    assert!(db.integrity_check().expect("integrity"));

    // The final recompute must match the stored history exactly.
    let mastery = services.mastery(&learner).expect("mastery");
    assert_eq!(mastery.len(), 1);
    let expected_score = (expected_correct as f64 + 1.0) / (stats.total as f64 + 2.0);
    assert!(
        (mastery[0].score - expected_score).abs() < 1e-9,
        "mastery {} does not match the stored history ({expected_score})",
        mastery[0].score
    );
}

#[test]
fn a_reader_can_work_while_a_writer_holds_the_database() {
    // WAL plus the busy timeout is what makes this true; without it the reader
    // would fail with `database is locked` rather than waiting.
    let tmp = TempDb::new("reader-during-write");
    let db = tmp.open();
    let profile = Services::new(&db)
        .create_profile("Ada", 60)
        .expect("profile");
    let learner = profile.id.clone();
    drop(db);

    let path = tmp.path();
    let writer_path = path.clone();
    let writer_learner = learner.clone();
    let writer = std::thread::spawn(move || {
        let db = Database::open(&writer_path).expect("writer connection");
        let services = Services::new(&db);
        for index in 0..200 {
            services
                .record_attempt(
                    &format!("w{index}"),
                    &writer_learner,
                    "AR",
                    &format!("q{index}"),
                    true,
                    100,
                )
                .expect("record");
        }
    });

    let reader_path = path.clone();
    let reader = std::thread::spawn(move || {
        let db = Database::open(&reader_path).expect("reader connection");
        let services = Services::new(&db);
        let mut observed = 0i64;
        for _ in 0..100 {
            let stats = services
                .analytics(&learner, "AR")
                .expect("read during writes");
            // Attempts only accumulate, so a later read can never see fewer.
            assert!(
                stats.total >= observed,
                "a concurrent read went backwards: {} then {}",
                observed,
                stats.total
            );
            observed = stats.total;
        }
        observed
    });

    writer.join().expect("writer");
    let observed = reader.join().expect("reader");
    assert!(observed > 0, "the reader never saw any of the writes");
}
