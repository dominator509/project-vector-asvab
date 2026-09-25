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
    content_generate_impl, content_history_impl, content_manager_impl, content_next_impl,
    content_pack_install_impl, content_pack_rollback_impl, content_packs_impl,
    content_quarantine_impl, content_reinstate_impl, content_stats_impl, migrations,
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
    let item = content_next_impl(&db, "AR", None, &[])
        .expect("next")
        .expect("an item must be servable after generation");
    assert_eq!(item.subtest, "AR");
    assert!(!item.stem.trim().is_empty());
}

#[test]
fn a_served_item_is_answerable_and_explains_its_distractors() {
    let (_dir, db) = database("shape");
    content_generate_impl(&db, "MK", 5, 7).expect("generate");

    let item = content_next_impl(&db, "MK", None, &[])
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

#[test]
fn a_generated_batch_asks_each_question_once() {
    let (_dir, db) = database("distinct-questions");
    // Large enough that the factory's random draw repeats stems: 500 draws over eight templates
    // is where the corpus readback found 126 questions asked twice with their wrong answers
    // shuffled. The store asks each question once, so the batch has to be filtered, not padded.
    let report = content_generate_impl(&db, "AR", 500, 20260923).expect("generate");
    assert!(report.activated > 100, "report: {report:?}");

    let duplicates: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM (
                 SELECT subtest, LOWER(TRIM(stem)),
                        LOWER(TRIM(COALESCE(json_extract(options_json,
                              '$[' || correct_index || ']'), ''))),
                        LOWER(TRIM(COALESCE(passage, '')))
                 FROM content_items WHERE state = 'active'
                 GROUP BY 1, 2, 3, 4 HAVING COUNT(*) > 1
             )",
            [],
            |row| row.get(0),
        )
        .expect("count duplicates");
    assert_eq!(
        duplicates, 0,
        "a question asked twice in one subtest is padding, however its options are ordered"
    );
    assert!(
        report.activated + report.already_present + report.rejected.len() == report.generated,
        "every drawn item is accounted for: {report:?}"
    );
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
    let item = content_next_impl(&db, "AR", None, &[]).expect("next");
    assert!(item.is_none(), "an empty corpus serves nothing");
}

#[test]
fn next_item_skips_ids_the_caller_has_already_seen() {
    let (_dir, db) = database("rotation");
    content_generate_impl(&db, "AR", 3, 99).expect("generate");

    let mut seen: Vec<String> = Vec::new();
    for _ in 0..3 {
        let item = content_next_impl(&db, "AR", None, &seen)
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
    let wrapped = content_next_impl(&db, "AR", None, &seen).expect("next");
    assert!(
        wrapped.is_some(),
        "a fully-seen corpus should wrap rather than serve nothing"
    );
}

#[test]
fn next_item_does_not_cross_subtests() {
    let (_dir, db) = database("scoping");
    content_generate_impl(&db, "AR", 5, 3).expect("AR");
    let mk = content_next_impl(&db, "MK", None, &[]).expect("next");
    assert!(mk.is_none(), "MK has no items yet");
}

#[test]
fn a_named_objective_serves_its_own_items_and_not_a_neighbour() {
    let (_dir, db) = database("objective");
    // The factory spreads one subtest across several objectives, so this corpus
    // holds more than one objective's items in the same subtest -- which is what
    // makes the assertion below meaningful rather than tautological.
    let report = content_generate_impl(&db, "AR", 40, 4_242).expect("generate");
    assert!(report.activated > 8, "report: {report:?}");

    // Read the objective spread back out of the store rather than assuming which
    // objectives the factory's templates map onto.
    let spread: Vec<(String, i64)> = {
        let conn = db.connection();
        let mut statement = conn
            .prepare(
                "SELECT objective_id, COUNT(*) FROM content_items
                 WHERE subtest = 'AR' GROUP BY objective_id ORDER BY objective_id",
            )
            .expect("prepare");
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query");
        rows.collect::<Result<Vec<(String, i64)>, _>>()
            .expect("rows")
    };
    assert!(
        spread.len() >= 2,
        "one subtest must hold two objectives for this test to mean anything: {spread:?}"
    );

    for (objective, count) in &spread {
        // Walked to exhaustion: an unnarrowed read returns the lowest id in the
        // subtest whatever objective it belongs to, so a first-item coincidence
        // cannot pass this, and serving exactly `count` distinct items proves the
        // objective's pool is neither widened nor truncated.
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..*count {
            let served = content_next_impl(&db, "AR", Some(objective), &seen).expect("next");
            let item = served
                .unwrap_or_else(|| panic!("{objective} holds {count} items but served nothing"));
            assert_eq!(
                &item.objective_id, objective,
                "practice for {objective} served an item from {}",
                item.objective_id
            );
            assert!(
                !seen.contains(&item.id),
                "{objective} repeated {} before exhausting its {count} items",
                item.id
            );
            seen.push(item.id);
        }
        assert_eq!(
            seen.len() as i64,
            *count,
            "{objective} should serve all {count} of its items"
        );
    }
}

#[test]
fn an_objective_with_no_content_falls_back_to_the_subtest() {
    let (_dir, db) = database("objective-fallback");
    content_generate_impl(&db, "AR", 12, 11).expect("generate");

    // A plan can name an objective this installation has no content for, either
    // because a pack was rolled back or because the objective is unbuilt. Serving
    // nothing would turn a content gap into a blocked learner.
    let fallback = content_next_impl(&db, "AR", Some("OBJ-AR-NOT-BUILT-99"), &[]).expect("next");
    let item = fallback.expect("an unbuilt objective must fall back to the subtest");
    assert_eq!(item.subtest, "AR");

    // The fallback is scoped to the subtest, not to the whole corpus.
    let other = content_next_impl(&db, "MK", Some("OBJ-MK-NOT-BUILT-99"), &[]).expect("next");
    assert!(
        other.is_none(),
        "the fallback must not cross into another subtest: {other:?}"
    );
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

// ---------------------------------------------------------------------------
// The content manager surface
// ---------------------------------------------------------------------------

#[test]
fn the_manager_reports_the_corpus_its_sources_and_its_items() {
    let (_dir, db) = database("manager");
    let report = content_generate_impl(&db, "AR", 15, 7).expect("generate");

    let view = content_manager_impl(&db, 50).expect("manager view");
    // The view reports what the pipeline stored, not what it was asked for: a question the batch
    // drew twice is asked once, so the two counts are tied to the pipeline's own accounting
    // rather than to the request.
    assert_eq!(
        view.stats.total, report.activated as i64,
        "report: {report:?}"
    );
    assert_eq!(view.stats.servable, report.activated as i64);
    assert!(
        !view.sources.is_empty(),
        "the manager must show what the corpus rests on"
    );
    // Every source needs a licence and a URL: a provenance record without them is
    // not something a reviewer can act on.
    for source in &view.sources {
        assert!(!source.licence.trim().is_empty(), "{source:?}");
        assert!(!source.url.trim().is_empty(), "{source:?}");
        assert!(!source.title.trim().is_empty(), "{source:?}");
    }
    assert_eq!(view.items.len(), report.activated);
    for item in &view.items {
        assert_eq!(item.state, "active");
        assert_eq!(item.subtest, "AR");
        assert!(!item.preview.trim().is_empty());
        assert!(!item.correct_answer.trim().is_empty());
        assert!(!item.content_hash.trim().is_empty());
        assert!(!item.sources.is_empty(), "{} cites nothing", item.id);
    }
}

#[test]
fn the_manager_is_empty_on_an_untouched_installation() {
    let (_dir, db) = database("manager-empty");
    let view = content_manager_impl(&db, 50).expect("manager view");
    assert_eq!(view.stats.total, 0);
    assert!(view.items.is_empty());
    assert!(view.sources.is_empty());
}

#[test]
fn quarantining_an_item_removes_it_from_service_and_records_who_did_it() {
    let (_dir, db) = database("quarantine");
    content_generate_impl(&db, "AR", 5, 3).expect("generate");
    let target = content_manager_impl(&db, 50).expect("view").items[0]
        .id
        .clone();

    content_quarantine_impl(&db, &target, "local-reviewer", "answer key looks wrong")
        .expect("quarantine");

    let view = content_manager_impl(&db, 50).expect("view");
    assert_eq!(
        view.stats.servable, 4,
        "a quarantined item must not be servable"
    );
    let item = view
        .items
        .iter()
        .find(|item| item.id == target)
        .expect("still listed");
    assert_eq!(item.state, "quarantined");

    // The audit trail is the point of a quarantine: an item that vanishes with no
    // record is indistinguishable from one that was never there.
    let history = content_history_impl(&db, &target).expect("history");
    let last = history.last().expect("a transition");
    assert_eq!(last.to_state, "quarantined");
    assert_eq!(last.actor, "local-reviewer");
    assert_eq!(last.rationale, "answer key looks wrong");
}

#[test]
fn a_quarantined_item_is_never_served() {
    let (_dir, db) = database("quarantine-serving");
    content_generate_impl(&db, "AR", 5, 3).expect("generate");
    let target = content_manager_impl(&db, 50).expect("view").items[0]
        .id
        .clone();
    content_quarantine_impl(&db, &target, "local-reviewer", "suspect").expect("quarantine");

    // Read the serving path directly rather than trusting the count.
    for _ in 0..10 {
        if let Some(item) = content_next_impl(&db, "AR", None, &[]).expect("next") {
            assert_ne!(item.id, target, "a quarantined item was served");
        }
    }
}

#[test]
fn reinstating_returns_an_item_to_service_through_the_pipeline() {
    let (_dir, db) = database("reinstate");
    content_generate_impl(&db, "AR", 5, 3).expect("generate");
    let target = content_manager_impl(&db, 50).expect("view").items[0]
        .id
        .clone();

    content_quarantine_impl(&db, &target, "local-reviewer", "suspect").expect("quarantine");
    content_reinstate_impl(&db, &target, "local-reviewer", "checked, it was correct")
        .expect("reinstate");

    let view = content_manager_impl(&db, 50).expect("view");
    assert_eq!(view.stats.servable, 5, "the item should be servable again");

    // Reinstatement walks the pipeline rather than jumping to active, so the trail
    // shows the item was re-verified instead of merely un-flagged.
    let history = content_history_impl(&db, &target).expect("history");
    let states: Vec<&str> = history
        .iter()
        .map(|entry| entry.to_state.as_str())
        .collect();
    assert_eq!(
        states,
        vec![
            "machine_validated",
            "independent_verified",
            "content_reviewed",
            "active",
            "quarantined",
            "draft",
            "machine_validated",
            "independent_verified",
            "content_reviewed",
            "active",
        ],
        "{states:?}"
    );
}

#[test]
fn quarantining_twice_is_refused_rather_than_recorded_as_two_events() {
    let (_dir, db) = database("double-quarantine");
    content_generate_impl(&db, "AR", 3, 3).expect("generate");
    let target = content_manager_impl(&db, 50).expect("view").items[0]
        .id
        .clone();

    content_quarantine_impl(&db, &target, "a", "first").expect("first");
    let second = content_quarantine_impl(&db, &target, "a", "second");
    assert!(
        second.is_err(),
        "a second quarantine is a no-op, not an event"
    );

    let history = content_history_impl(&db, &target).expect("history");
    assert_eq!(
        history
            .iter()
            .filter(|e| e.to_state == "quarantined")
            .count(),
        1
    );
}

#[test]
fn reinstating_an_item_that_is_not_quarantined_is_refused() {
    let (_dir, db) = database("reinstate-active");
    content_generate_impl(&db, "AR", 3, 3).expect("generate");
    let target = content_manager_impl(&db, 50).expect("view").items[0]
        .id
        .clone();
    let result = content_reinstate_impl(&db, &target, "a", "no reason");
    assert!(
        result.is_err(),
        "an active item has nothing to be reinstated from"
    );
}

#[test]
fn the_manager_surface_refuses_an_unknown_item() {
    let (_dir, db) = database("unknown-item");
    assert!(content_quarantine_impl(&db, "q-nope", "a", "r").is_err());
    assert!(content_reinstate_impl(&db, "q-nope", "a", "r").is_err());
    assert!(content_history_impl(&db, "q-nope")
        .expect("history")
        .is_empty());
}

#[test]
fn the_manager_limit_bounds_the_listing() {
    let (_dir, db) = database("limit");
    content_generate_impl(&db, "AR", 20, 3).expect("generate");
    let view = content_manager_impl(&db, 5).expect("view");
    assert_eq!(view.items.len(), 5, "the limit must bound the listing");
    assert_eq!(view.stats.total, 20, "but not the statistics");
}

// ---------------------------------------------------------------------------
// Ingested content through the command boundary
// ---------------------------------------------------------------------------

/// Mechanical Comprehension is computable, so the factory serves it like the other
/// two quantitative subtests, and the command boundary must not treat it as one of
/// the subtests that has no templates.
#[test]
fn mechanical_comprehension_generates_and_is_served() {
    let (_dir, db) = database("mc");
    let report = content_generate_impl(&db, "MC", 20, 20_260_922).expect("generate MC");
    assert_eq!(report.subtest, "MC");
    assert_eq!(report.activated, 20, "report: {report:?}");
    assert!(
        report.rejected.is_empty(),
        "a clean run rejects nothing: {:?}",
        report.rejected
    );

    let item = content_next_impl(&db, "MC", None, &[])
        .expect("next")
        .expect("an MC item must be servable after generation");
    assert_eq!(item.subtest, "MC");
    // Every option is a whole number: a mechanical item's answer is arithmetic.
    for option in &item.options {
        assert!(
            option.parse::<i64>().is_ok(),
            "MC options are numbers, got {option:?}"
        );
    }
    // A mechanical item is not a comprehension item and needs no passage.
    assert_eq!(item.passage, None);
}
/// A miniature Project Gutenberg work, in the corpus's own shape.
const WORK: &str = "\
The Project Gutenberg eBook of A Test Work

*** START OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

The laboratory keeps 600 grams of the salt in a sealed glass jar on the bench. That jar stands on the upper shelf, 340 millimetres above the stone floor. A paper label gives the date of the last weighing, 118 days before the audit.

The cooling tower removes 1897 litres of water from the air each working day. The process works best when the outside air is dry, below 24 percent humidity. Operators check the gauge every morning before the shift, and keep 365 days of readings on file.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***
";

/// An ingested item is served through the same command as a generated one, and
/// carries what makes it answerable.
///
/// The two paths meet here and nowhere else in the suite: `pc_ingestion.rs` proves
/// the pipeline stores provenance, and this proves the command boundary serves what
/// it stored. Without the passage travelling across, a learner is handed a question
/// about a text they were never shown.
#[test]
fn an_ingested_comprehension_item_is_served_with_its_passage() {
    use vector_application::content::{ContentPipeline, PcIngestRequest};
    use vector_persistence::repo::{EvidenceRepo, NewEvidence};
    use vector_questions::passages::parse_gutenberg;

    let (_dir, db) = database("ingested-pc");
    let source = EvidenceRepo::new(&db)
        .put(&NewEvidence::new(
            "https://www.gutenberg.org/ebooks/99999",
            "A Test Work (Project Gutenberg #99999)",
            "sha256:pc-command-test",
            "Public domain in the USA",
            "2026-09-22",
            0.90,
            "retrieved",
        ))
        .expect("record the work");

    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_pc(
            &parse_gutenberg(WORK, "A Test Work"),
            &PcIngestRequest {
                label: "A Test Work",
                source_id: &source,
                count: 6,
                seed: 20_260_922,
                dictionary: None,
                reviewer: "content-reviewer",
                generator: "pc-ingester",
            },
        )
        .expect("ingest");
    assert!(report.activated > 0, "the fixture should yield items");

    let item = content_next_impl(&db, "PC", None, &[])
        .expect("next")
        .expect("an ingested item must be servable");
    assert_eq!(item.subtest, "PC");
    let passage = item
        .passage
        .as_deref()
        .expect("a served comprehension item must carry its passage");
    assert!(
        passage.split_whitespace().count() >= 40,
        "the passage is too short to comprehend: {passage}"
    );
    assert!(
        passage
            .to_lowercase()
            .contains(&item.options[item.correct_index].to_lowercase()),
        "the correct option must be stated in the served passage"
    );
    assert!(
        !item.objective_id.trim().is_empty() && !item.explanation.trim().is_empty(),
        "the item must carry its objective and its explanation"
    );
}

// ---------------------------------------------------------------------------
// Content packs through the command boundary (REQ-023, REQ-032)
// ---------------------------------------------------------------------------

/// A pack the tests can install: two Arithmetic Reasoning items, signed by a key the
/// test holds.
fn a_signed_pack(db: &Database, signing: &ed25519_dalek::SigningKey) -> Vec<u8> {
    use vector_application::content::{ContentPipeline, GenerateRequest};
    use vector_persistence::repo::{EvidenceRepo, NewEvidence};

    let source = EvidenceRepo::new(db)
        .put(&NewEvidence::new(
            "https://www.officialasvab.com/applicants/sample-questions/",
            "ASVAB subtest constructs (facts only)",
            "sha256:pack-command-source",
            "Public domain in the USA",
            "2026-09-22",
            0.9,
            "retrieved",
        ))
        .expect("record the source");
    ContentPipeline::new(db)
        .generate_and_activate(&GenerateRequest {
            subtest: "AR",
            count: 4,
            seed: 20_260_922,
            reviewer: "content-reviewer",
            source_id: &source,
            generator: "factory",
        })
        .expect("generate");

    let document = vector_application::packs::build_pack(
        db,
        &vector_application::packs::BuildPackRequest {
            name: "core-asvab",
            version: 1,
            app_min: "0.1.0",
            app_max: None,
            curriculum: Vec::new(),
            calibration: Vec::new(),
        },
        signing,
    )
    .expect("build the pack");
    vector_application::packs::pack_bytes(&document).expect("serialize")
}

fn test_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[7_u8; 32])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn a_pack_installed_through_the_command_is_served() {
    let (_source_dir, source_db) = database("pack-build-source");
    let bytes = a_signed_pack(&source_db, &test_key());
    let pack_path = std::env::temp_dir().join(format!(
        "vector-pack-command-{}-{}.vpack",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::write(&pack_path, &bytes).expect("write the pack");

    let (_target_dir, target_db) = database("pack-install-target");
    let trusted = hex(&test_key().verifying_key().to_bytes());
    let report = content_pack_install_impl(
        &target_db,
        pack_path.to_str().expect("path"),
        Some(&trusted),
        "0.1.0",
    )
    .expect("install");
    assert_eq!(report.installed, 4);
    assert_eq!(report.name, "core-asvab");

    // The effect is read back through the serving command, not the report.
    let item = content_next_impl(&target_db, "AR", None, &[])
        .expect("next")
        .expect("an installed item must be served");
    assert_eq!(item.subtest, "AR");

    let packs = content_packs_impl(&target_db).expect("packs");
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].status, "active");
    assert!(packs[0].signature_valid);

    let _ = std::fs::remove_file(&pack_path);
}

#[test]
fn installing_with_no_trusted_key_configured_is_refused() {
    // The trusted key is configuration, not an argument: a caller that could name the
    // key it trusts could install a pack it signed itself, which is the one thing the
    // signature exists to prevent.
    let (_dir, db) = database("pack-no-key");
    let error = content_pack_install_impl(&db, "any.vpack", None, "0.1.0")
        .expect_err("an installation with no trusted key must refuse every pack");
    assert!(
        error
            .to_string()
            .contains("no pack signing key is configured"),
        "the refusal should say what is missing: {error}"
    );
    assert!(content_packs_impl(&db).expect("packs").is_empty());
}

#[test]
fn a_trusted_key_that_is_not_a_public_key_is_refused_before_any_file_is_read() {
    let (_dir, db) = database("pack-bad-key");
    let error = content_pack_install_impl(&db, "missing.vpack", Some("not-hex"), "0.1.0")
        .expect_err("a malformed key must be refused");
    assert!(
        error.to_string().contains("64 hex"),
        "the refusal should name the problem: {error}"
    );
}

#[test]
fn a_pack_signed_by_another_key_is_refused_through_the_command() {
    let (_source_dir, source_db) = database("pack-wrong-signer");
    let bytes = a_signed_pack(&source_db, &test_key());
    let pack_path = std::env::temp_dir().join(format!(
        "vector-pack-signer-{}-{}.vpack",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::write(&pack_path, &bytes).expect("write the pack");

    let other = ed25519_dalek::SigningKey::from_bytes(&[9_u8; 32]);
    let trusted = hex(&other.verifying_key().to_bytes());
    let (_target_dir, target_db) = database("pack-wrong-signer-target");
    let error = content_pack_install_impl(
        &target_db,
        pack_path.to_str().expect("path"),
        Some(&trusted),
        "0.1.0",
    )
    .expect_err("a pack signed by another key must be refused");
    assert!(
        error
            .to_string()
            .contains("not the key this installation trusts"),
        "the refusal should say why: {error}"
    );
    assert_eq!(
        content_next_impl(&target_db, "AR", None, &[]).expect("next"),
        None,
        "a refused pack leaves nothing to serve"
    );

    let _ = std::fs::remove_file(&pack_path);
}

#[test]
fn rolling_back_a_pack_with_no_earlier_version_is_refused_through_the_command() {
    let (_source_dir, source_db) = database("pack-rollback-source");
    let bytes = a_signed_pack(&source_db, &test_key());
    let pack_path = std::env::temp_dir().join(format!(
        "vector-pack-rollback-{}-{}.vpack",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::write(&pack_path, &bytes).expect("write the pack");

    let (_target_dir, target_db) = database("pack-rollback-target");
    let trusted = hex(&test_key().verifying_key().to_bytes());
    content_pack_install_impl(
        &target_db,
        pack_path.to_str().expect("path"),
        Some(&trusted),
        "0.1.0",
    )
    .expect("install");

    let error = content_pack_rollback_impl(&target_db, "core-asvab")
        .expect_err("there is no earlier version to roll back to");
    assert!(
        error.to_string().contains("no earlier pack"),
        "the refusal should say why: {error}"
    );

    let _ = std::fs::remove_file(&pack_path);
}
