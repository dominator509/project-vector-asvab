//! Content command boundary: generation, serving, and statistics.
//!
//! Requirements: REQ-022 (original item + deterministic proof + independent
//! verification), REQ-056 (per-item provenance).
//!
//! These drive the `<name>_impl` functions rather than the `#[tauri::command]`
//! wrappers, because `State<'_, AppState>` has no test constructor outside a
//! running Tauri application. The wrappers are mechanical, and webview-to-Rust
//! IPC delivery is a property of the Tauri runtime covered by the packaged
//! live-fire launch, not claimed here.
//!
//! The database is created by the application's own embedded migration set, so
//! these tests fail if a migration the commands depend on stops being registered.

use vector_desktop_lib::commands::{
    content_generate_impl, content_next_impl, content_stats_impl, migrations,
};
use vector_persistence::{Database, MigrationManager};

/// Minimal dependency-free temp dir so the test does not pull a new crate.
mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "vector-content-cmd-{}-{}-{}-{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
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

/// A database built by the application's own migration set.
fn database(tag: &str) -> (tempdir::TempDir, Database) {
    let dir = tempdir::TempDir::new(tag);
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("apply embedded migrations");
    (dir, db)
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

#[test]
fn generating_activates_items_that_can_then_be_served() {
    let (_dir, db) = database("generate");

    let report = content_generate_impl(&db, "AR", 25, 20_260_922).expect("generate");
    assert_eq!(report.subtest, "AR");
    assert_eq!(report.activated, 25, "report: {report:?}");
    assert_eq!(report.verified, 25);
    assert!(
        report.rejected.is_empty(),
        "a clean run rejects nothing: {:?}",
        report.rejected
    );

    // Read the effect back rather than trusting the return value.
    let item = content_next_impl(&db, "AR", &[])
        .expect("next")
        .expect("an item must be servable after generation");
    assert_eq!(item.subtest, "AR");
    assert!(!item.stem.trim().is_empty());
}

#[test]
fn a_served_item_is_answerable_and_explains_its_distractors() {
    let (_dir, db) = database("shape");
    content_generate_impl(&db, "MK", 5, 7).expect("generate");

    let item = content_next_impl(&db, "MK", &[])
        .expect("next")
        .expect("item");

    assert_eq!(item.options.len(), 4, "four options: {:?}", item.options);
    assert!(
        item.correct_index < item.options.len(),
        "the correct index must address an option"
    );
    assert!(
        !item.explanation.trim().is_empty(),
        "an item without a worked explanation teaches nothing"
    );
    // A wrong option without a rationale is a missed teaching moment, which is
    // the entire reason the factory generates misconceptions rather than noise.
    assert_eq!(
        item.distractor_rationales.len(),
        3,
        "every wrong option needs a rationale: {:?}",
        item.distractor_rationales
    );
    assert!(
        !item.distractor_rationales.contains_key(&item.correct_index),
        "the correct option must not carry a misconception rationale"
    );
    assert!(
        item.objective_id.starts_with("OBJ-"),
        "a served item must name the objective it serves: {}",
        item.objective_id
    );
}

#[test]
fn the_same_request_twice_adds_nothing() {
    let (_dir, db) = database("idempotent");
    let first = content_generate_impl(&db, "AR", 20, 4242).expect("first");
    assert_eq!(first.activated, 20);

    let second = content_generate_impl(&db, "AR", 20, 4242).expect("second");
    assert_eq!(
        second.already_present, 20,
        "the same seed regenerates the same questions: {second:?}"
    );
    assert_eq!(second.activated, 0, "nothing new should be written");

    let stats = content_stats_impl(&db).expect("stats");
    assert_eq!(stats.total, 20, "the corpus must not have grown");
}

#[test]
fn a_different_seed_extends_the_corpus() {
    let (_dir, db) = database("extend");
    content_generate_impl(&db, "AR", 20, 1).expect("first");
    let second = content_generate_impl(&db, "AR", 20, 2).expect("second");

    assert!(
        second.activated > 0,
        "a new seed should contribute items: {second:?}"
    );
    let stats = content_stats_impl(&db).expect("stats");
    assert!(stats.total > 20, "corpus total {}", stats.total);
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn a_zero_count_is_refused_and_writes_nothing() {
    let (_dir, db) = database("zero");
    let error = content_generate_impl(&db, "AR", 0, 1).expect_err("must refuse");
    assert!(
        error.to_string().contains("at least 1"),
        "the refusal should be explicit: {error}"
    );
    assert_eq!(
        content_stats_impl(&db).expect("stats").total,
        0,
        "a refused request must not create the source row or any item"
    );
}

#[test]
fn an_oversized_batch_is_refused_before_it_blocks_the_command_thread() {
    let (_dir, db) = database("oversized");
    let error = content_generate_impl(&db, "AR", 5_000, 1).expect_err("must refuse");
    assert!(
        error.to_string().contains("per-call limit"),
        "the refusal should name the limit: {error}"
    );
    assert_eq!(content_stats_impl(&db).expect("stats").total, 0);
}

#[test]
fn a_subtest_with_no_templates_is_refused_rather_than_reporting_success() {
    let (_dir, db) = database("unsupported");
    // WK has a dictionary plan but no template yet. Returning an empty report
    // would look like a successful no-op, which is the failure mode AGENTS.md
    // calls software that appears to work.
    let error = content_generate_impl(&db, "WK", 10, 1).expect_err("must refuse");
    assert!(
        error.to_string().contains("nothing"),
        "the refusal should say the factory produced nothing: {error}"
    );
}

// ---------------------------------------------------------------------------
// Serving
// ---------------------------------------------------------------------------

#[test]
fn next_item_returns_nothing_when_the_corpus_is_empty() {
    let (_dir, db) = database("empty");
    let item = content_next_impl(&db, "AR", &[]).expect("next");
    assert!(item.is_none(), "an empty corpus serves nothing");
}

#[test]
fn next_item_skips_ids_the_caller_has_already_seen() {
    let (_dir, db) = database("rotation");
    content_generate_impl(&db, "AR", 3, 99).expect("generate");

    let mut seen: Vec<String> = Vec::new();
    for _ in 0..3 {
        let item = content_next_impl(&db, "AR", &seen)
            .expect("next")
            .expect("an unseen item");
        assert!(
            !seen.contains(&item.id),
            "rotation must not repeat {} within a session",
            item.id
        );
        seen.push(item.id);
    }
    assert_eq!(seen.len(), 3, "three distinct items were served");

    // With everything seen, the corpus wraps rather than starving the learner.
    let wrapped = content_next_impl(&db, "AR", &seen).expect("next");
    assert!(
        wrapped.is_some(),
        "a fully-seen corpus should wrap rather than serve nothing"
    );
}

#[test]
fn next_item_does_not_cross_subtests() {
    let (_dir, db) = database("scoping");
    content_generate_impl(&db, "AR", 5, 3).expect("AR");
    let mk = content_next_impl(&db, "MK", &[]).expect("next");
    assert!(mk.is_none(), "MK has no items yet");
}

// ---------------------------------------------------------------------------
// Statistics and provenance
// ---------------------------------------------------------------------------

#[test]
fn stats_report_the_corpus_and_its_recorded_source() {
    let (_dir, db) = database("stats");
    content_generate_impl(&db, "AR", 12, 5).expect("generate");

    let stats = content_stats_impl(&db).expect("stats");
    assert_eq!(stats.total, 12);
    assert_eq!(stats.servable, 12, "every activated item is servable");
    assert!(
        stats.sources >= 1,
        "generation must record the construct source it cites"
    );
    assert!(
        stats
            .by_state
            .iter()
            .any(|s| s.state == "active" && s.count == 12),
        "by_state should report the active count: {:?}",
        stats.by_state
    );
    assert!(
        stats
            .by_subtest
            .iter()
            .any(|s| s.subtest == "AR" && s.count == 12),
        "by_subtest should report AR: {:?}",
        stats.by_subtest
    );
}

#[test]
fn an_untouched_installation_reports_an_empty_corpus() {
    let (_dir, db) = database("fresh");
    let stats = content_stats_impl(&db).expect("stats");
    assert_eq!(stats.total, 0);
    assert_eq!(stats.servable, 0);
    assert!(
        stats.by_state.is_empty() && stats.by_subtest.is_empty(),
        "a fresh install has no content rows: {stats:?}"
    );
}
