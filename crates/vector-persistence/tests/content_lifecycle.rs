//! Content item repository: lifecycle, audit trail, and servability.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! `content_items.rs` tests the schema's own constraints. This file tests the
//! repository that drives them: that the documented pipeline can actually be
//! walked, that the audit trail the schema cannot generate on its own is written,
//! and that a failed transition leaves nothing behind.

use std::collections::BTreeMap;
use std::path::Path;

use vector_persistence::content::{ContentItemRepo, NewContentItem, PIPELINE};
use vector_persistence::repo::EvidenceRepo;
use vector_persistence::{Database, Migration, MigrationManager};

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
                "vector-lifecycle-{}-{}-{}-{}",
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

fn migrations() -> Vec<Migration> {
    MigrationManager::load_from_dir(Path::new("../../migrations")).expect("migrations load")
}

fn migrated(dir: &tempdir::TempDir) -> Database {
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    db
}

/// Record the source that generated items cite.
fn seed_source(db: &Database) {
    EvidenceRepo::new(db)
        .put(&vector_persistence::repo::NewEvidence::new(
            "https://www.officialasvab.com/applicants/sample-questions/",
            "ASVAB subtest constructs",
            "sha256:asvab-spec",
            "US-Government-Work",
            "2026-09-22",
            0.9,
            "retrieved",
        ))
        .expect("record source");
}

fn item_id(db: &Database) -> String {
    let conn = db.connection();
    conn.query_row("SELECT id FROM evidence_records LIMIT 1", [], |r| {
        r.get::<_, String>(0)
    })
    .expect("source id")
}

fn draft<'a>(
    id: &'a str,
    options: &'a [String],
    rationales: &'a BTreeMap<usize, String>,
) -> NewContentItem<'a> {
    NewContentItem {
        id,
        subtest: "AR",
        objective_id: "OBJ-AR-RATE-01",
        stem: "A printer produces 12 pages per minute. How many in 2.5 hours?",
        passage: None,
        options,
        correct_index: 1,
        explanation: "12 * 150 = 1800",
        distractor_rationales: rationales,
        difficulty: -0.5,
        proof_kind: "executable",
        proof_json: r#"{"Executable":{"expression":"12 * 150","answer":"1800"}}"#,
        content_hash: "sha256:item-1",
        generator_hash: Some("sha256:gen-1"),
    }
}

fn sample_options() -> Vec<String> {
    ["180", "1800", "360", "1440"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn sample_rationales() -> BTreeMap<usize, String> {
    [
        (0, "Used 15 minutes instead of 150.".to_string()),
        (2, "Treated 2.5 hours as 30 minutes.".to_string()),
        (3, "Used pages per hour rather than per minute.".to_string()),
    ]
    .into_iter()
    .collect()
}

// ---------------------------------------------------------------------------
// The lifecycle
// ---------------------------------------------------------------------------

#[test]
fn an_item_can_be_walked_from_draft_to_active_and_is_then_servable() {
    let dir = tempdir::TempDir::new("lifecycle");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);

    let options = sample_options();
    let rationales = sample_rationales();
    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert draft");
    repo.cite("q-1", &item_id(&db)).expect("cite");

    assert!(
        repo.servable("AR").expect("servable").is_empty(),
        "a draft must not be servable"
    );

    repo.walk_to_content_reviewed("q-1", "factory")
        .expect("walk pipeline");
    let reviewed = repo.get("q-1").expect("get").expect("exists");
    assert_eq!(reviewed.state, "content_reviewed");

    repo.activate("q-1", "content-reviewer", "proof checked, item approved")
        .expect("activate");

    let active = repo.get("q-1").expect("get").expect("exists");
    assert_eq!(active.state, "active");
    assert_eq!(active.reviewer, "content-reviewer");

    let servable = repo.servable("AR").expect("servable");
    assert_eq!(servable.len(), 1);
    assert_eq!(servable[0].id, "q-1");
    assert_eq!(servable[0].stem, reviewed.stem);
}

#[test]
fn a_servable_item_round_trips_its_options_and_rationales_exactly() {
    let dir = tempdir::TempDir::new("roundtrip");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);

    let options = sample_options();
    let rationales = sample_rationales();
    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    repo.cite("q-1", &item_id(&db)).expect("cite");
    repo.walk_to_content_reviewed("q-1", "factory")
        .expect("walk");
    repo.activate("q-1", "reviewer", "ok").expect("activate");

    let stored = repo.get("q-1").expect("get").expect("exists");
    assert_eq!(
        stored.options, options,
        "option order is presentation order"
    );
    assert_eq!(stored.distractor_rationales, rationales);
    assert_eq!(stored.correct_index, 1);
    assert_eq!(stored.proof_kind, "executable");
    assert!(
        stored.proof_json.contains("12 * 150"),
        "the proof must survive storage: {}",
        stored.proof_json
    );
    assert_eq!(stored.generator_hash.as_deref(), Some("sha256:gen-1"));
    assert_eq!(stored.verifier_hash, None);
}

#[test]
fn only_active_items_of_the_requested_subtest_are_served() {
    let dir = tempdir::TempDir::new("filter");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    // One item activated, one left in review.
    repo.insert_draft(&draft("q-active", &options, &rationales))
        .expect("insert");
    repo.cite("q-active", &item_id(&db)).expect("cite");
    repo.walk_to_content_reviewed("q-active", "factory")
        .expect("walk");
    repo.activate("q-active", "reviewer", "ok")
        .expect("activate");

    let mut other = draft("q-review", &options, &rationales);
    other.content_hash = "sha256:item-2";
    repo.insert_draft(&other).expect("insert");
    repo.cite("q-review", &item_id(&db)).expect("cite");
    repo.walk_to_content_reviewed("q-review", "factory")
        .expect("walk");

    let servable = repo.servable("AR").expect("servable");
    assert_eq!(servable.len(), 1);
    assert_eq!(servable[0].id, "q-active");
    assert!(repo.servable("MK").expect("servable").is_empty());
}

// ---------------------------------------------------------------------------
// REQ-048: the audit trail
// ---------------------------------------------------------------------------

#[test]
fn every_transition_is_recorded_in_order_with_its_actor() {
    let dir = tempdir::TempDir::new("history");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    repo.cite("q-1", &item_id(&db)).expect("cite");
    repo.walk_to_content_reviewed("q-1", "factory")
        .expect("walk");
    repo.activate("q-1", "reviewer", "approved after review")
        .expect("activate");

    let history = repo.history("q-1").expect("history");
    assert_eq!(history.len(), 4, "three pipeline steps plus activation");
    assert_eq!(history[0].from_state, "draft");
    assert_eq!(history[0].to_state, PIPELINE[1]);
    assert_eq!(history[3].to_state, "active");
    assert_eq!(history[3].actor, "reviewer");
    assert_eq!(history[3].rationale, "approved after review");
}

#[test]
fn a_refused_transition_leaves_no_audit_row_behind() {
    let dir = tempdir::TempDir::new("rollback");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    repo.cite("q-1", &item_id(&db)).expect("cite");

    // draft -> active skips three documented stages, so the schema aborts. The
    // audit insert runs in the same transaction, so it must not survive either.
    assert!(
        repo.advance("q-1", "active", "reviewer", "shortcut")
            .is_err(),
        "an illegal transition must be refused"
    );

    assert!(
        repo.history("q-1").expect("history").is_empty(),
        "a refused transition must not appear in the audit trail"
    );
    assert_eq!(
        repo.get("q-1").expect("get").expect("exists").state,
        "draft",
        "the item must not have moved"
    );
}

#[test]
fn advancing_an_item_that_does_not_exist_is_an_error() {
    let dir = tempdir::TempDir::new("missing");
    let db = migrated(&dir);
    let repo = ContentItemRepo::new(&db);
    assert!(repo.advance("nope", "machine_validated", "a", "r").is_err());
}

#[test]
fn activation_requires_a_named_reviewer() {
    let dir = tempdir::TempDir::new("reviewer");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    repo.cite("q-1", &item_id(&db)).expect("cite");
    repo.walk_to_content_reviewed("q-1", "factory")
        .expect("walk");

    assert!(
        repo.activate("q-1", "   ", "ok").is_err(),
        "REQ-056: activation requires a named reviewer"
    );
    assert!(
        repo.activate("q-1", "", "ok").is_err(),
        "an empty reviewer is not a name"
    );
}

#[test]
fn an_unknown_target_state_is_refused_before_touching_the_database() {
    let dir = tempdir::TempDir::new("badstate");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    assert!(repo.advance("q-1", "published", "a", "r").is_err());
    assert_eq!(
        repo.get("q-1").expect("get").expect("exists").state,
        "draft"
    );
}

// ---------------------------------------------------------------------------
// Integrity at the repository boundary
// ---------------------------------------------------------------------------

#[test]
fn inserting_an_item_whose_answer_index_is_out_of_range_is_refused() {
    let dir = tempdir::TempDir::new("badindex");
    let db = migrated(&dir);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    let mut bad = draft("q-1", &options, &rationales);
    bad.correct_index = 9;
    let error = repo.insert_draft(&bad).expect_err("must refuse");
    assert!(
        error.to_string().contains("outside"),
        "the error should name the problem: {error}"
    );
}

#[test]
fn counts_report_each_lifecycle_state() {
    let dir = tempdir::TempDir::new("counts");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    let mut second = draft("q-2", &options, &rationales);
    second.content_hash = "sha256:item-2";
    repo.insert_draft(&second).expect("insert");
    repo.cite("q-2", &item_id(&db)).expect("cite");
    repo.walk_to_content_reviewed("q-2", "factory")
        .expect("walk");

    let counts = repo.counts_by_state().expect("counts");
    assert!(counts.contains(&("draft".to_string(), 1)), "{counts:?}");
    assert!(
        counts.contains(&("content_reviewed".to_string(), 1)),
        "{counts:?}"
    );
}

#[test]
fn cited_sources_are_reported_for_an_item() {
    let dir = tempdir::TempDir::new("sources");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-1", &options, &rationales))
        .expect("insert");
    repo.cite("q-1", &item_id(&db)).expect("cite");

    let sources = repo.sources("q-1").expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0], item_id(&db));
}

// ---------------------------------------------------------------------------
// Migration 005: a Paragraph Comprehension item carries its passage
// ---------------------------------------------------------------------------

/// A drafting helper for PC items, whose rule is about a column the others ignore.
fn pc_draft<'a>(
    id: &'a str,
    passage: Option<&'a str>,
    options: &'a [String],
    rationales: &'a BTreeMap<usize, String>,
) -> NewContentItem<'a> {
    let mut item = draft(id, options, rationales);
    item.subtest = "PC";
    item.proof_kind = "source_backed";
    item.passage = passage;
    item
}

/// A passage of the length the ingester writes, so it clears the store's floor.
fn a_passage() -> String {
    "The tower stood on the ridge for a hundred years before the surveyors arrived to \
     measure it. They recorded the result of that survey in a log, and the log is kept \
     in the county office where anyone may read it."
        .to_string()
}

#[test]
fn a_paragraph_comprehension_item_without_a_passage_cannot_be_stored() {
    let dir = tempdir::TempDir::new("pc-no-passage");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    let error = repo
        .insert_draft(&pc_draft("q-pc", None, &options, &rationales))
        .expect_err("a PC item with no passage must be refused");
    assert!(
        error.to_string().contains("passage"),
        "the refusal should name the problem: {error}"
    );
}

#[test]
fn a_paragraph_comprehension_item_with_a_token_passage_is_refused() {
    let dir = tempdir::TempDir::new("pc-short-passage");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    // `MIN_PASSAGE_WORDS` is 40 and the ingester enforces it, but the store is the
    // last line: "See above." is a passage a builder bug could write, and it asks
    // the learner to find a detail in a text that has none.
    let error = repo
        .insert_draft(&pc_draft("q-pc", Some("See above."), &options, &rationales))
        .expect_err("a passage too short to comprehend must be refused");
    assert!(
        error.to_string().contains("long enough"),
        "the refusal should name the problem: {error}"
    );
}

#[test]
fn a_passage_cannot_be_stripped_from_a_stored_paragraph_comprehension_item() {
    let dir = tempdir::TempDir::new("pc-strip");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    let passage = a_passage();
    repo.insert_draft(&pc_draft("q-pc", Some(&passage), &options, &rationales))
        .expect("insert");

    let error = db
        .connection()
        .execute(
            "UPDATE content_items SET passage = NULL WHERE id = 'q-pc'",
            [],
        )
        .expect_err("nulling the passage of a PC item must be refused");
    assert!(
        error.to_string().contains("passage"),
        "the refusal should name the problem: {error}"
    );

    // The row is unchanged, so the guard cannot have half-applied.
    let stored: Option<String> = db
        .connection()
        .query_row(
            "SELECT passage FROM content_items WHERE id = 'q-pc'",
            [],
            |row| row.get(0),
        )
        .expect("row");
    assert_eq!(stored.as_deref(), Some(passage.as_str()));
}

/// The rule is scoped to PC: no other subtest is required to carry a passage.
#[test]
fn subtests_without_a_passage_are_unaffected() {
    let dir = tempdir::TempDir::new("pc-scope");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    repo.insert_draft(&draft("q-ar", &options, &rationales))
        .expect("an AR item needs no passage");
    let stored = repo.get("q-ar").expect("get").expect("present");
    assert_eq!(stored.passage, None);
}

/// The guard is a trigger, and this proves it: with the trigger removed the same
/// insert succeeds, so what refused it was the rule and not an accident of the
/// repository's own validation.
#[test]
fn the_passage_rule_is_load_bearing() {
    let dir = tempdir::TempDir::new("pc-trigger-proof");
    let db = migrated(&dir);
    seed_source(&db);
    let repo = ContentItemRepo::new(&db);
    let options = sample_options();
    let rationales = sample_rationales();

    db.connection()
        .execute_batch("DROP TRIGGER content_items_pc_requires_passage_insert;")
        .expect("drop the guard");

    repo.insert_draft(&pc_draft("q-pc", None, &options, &rationales))
        .expect("with the trigger gone, the insert the guard refused now succeeds");
}
