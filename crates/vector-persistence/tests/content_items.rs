//! Content item store: constraint enforcement for REQ-022, REQ-048 and REQ-056.
//!
//! The rules in `migrations/003_content_items.sql` are the database's own
//! enforcement of provenance completeness, activation preconditions and the
//! lifecycle state machine. Every test here asserts that an invalid state is
//! *unrepresentable*, not merely that a validator would reject it, so each one
//! would fail if the corresponding CHECK or TRIGGER were dropped.
//!
//! Requirements: REQ-022 (deterministic proof), REQ-048 (review audit),
//! REQ-056 (per-item provenance).

use std::path::Path;

use rusqlite::params;
use vector_persistence::{Database, Migration, MigrationManager};

/// Minimal dependency-free temp dir so the test does not pull a new crate,
/// following `ep003_acceptance.rs`. Unlike that copy this one also mixes in an
/// atomic counter: all tests here share one tag, and a same-nanosecond start on
/// two test threads would otherwise resolve to the same directory.
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
                "vector-content-{}-{}-{}-{}",
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

const NOW: &str = "2026-09-22T00:00:00Z";

fn migrations() -> Vec<Migration> {
    MigrationManager::load_from_dir(Path::new("../../migrations")).expect("migrations load")
}

fn migrated(dir: &tempdir::TempDir) -> Database {
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    db
}

/// Insert an item in the draft state. Every field a constraint could reject is
/// supplied with a valid value so that each test varies exactly one thing.
struct NewItem<'a> {
    id: &'a str,
    subtest: &'a str,
    content_hash: &'a str,
    proof_kind: &'a str,
    options_json: &'a str,
    correct_index: i64,
}

impl<'a> NewItem<'a> {
    /// An Arithmetic Reasoning item with a deterministic proof and four options.
    fn ar(id: &'a str, content_hash: &'a str) -> Self {
        Self {
            id,
            subtest: "AR",
            content_hash,
            proof_kind: "executable",
            options_json: r#"["180","1800","360","1440"]"#,
            correct_index: 1,
        }
    }
}

fn insert(db: &Database, item: &NewItem<'_>) -> rusqlite::Result<usize> {
    db.connection().execute(
        "INSERT INTO content_items (
             id, subtest, objective_id, state, stem, options_json, correct_index,
             explanation, distractors_json, difficulty, proof_kind, proof_json,
             reviewer, content_hash, created_at, updated_at
         ) VALUES (
             ?1, ?2, '', 'draft', 'A printer produces 12 pages per minute. How many
             pages in 2.5 hours?', ?3, ?4, '', '{}', 0.0, ?5, ?6, '', ?7, ?8, ?8
         )",
        params![
            item.id,
            item.subtest,
            item.options_json,
            item.correct_index,
            item.proof_kind,
            r#"{"expression":"12*150","answer":"1800"}"#,
            item.content_hash,
            NOW,
        ],
    )
}

/// Advance an item one documented step, returning the raw result so a caller can
/// assert on refusal.
fn set_state(db: &Database, id: &str, to: &str) -> rusqlite::Result<usize> {
    db.connection().execute(
        "UPDATE content_items SET state = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, to, NOW],
    )
}

/// Walk an item to `content_reviewed`, the state immediately before activation.
fn walk_to_content_reviewed(db: &Database, id: &str) {
    for state in [
        "machine_validated",
        "independent_verified",
        "content_reviewed",
    ] {
        set_state(db, id, state).unwrap_or_else(|e| panic!("advance to {state}: {e}"));
    }
    db.connection()
        .execute(
            "UPDATE content_items SET objective_id = 'OBJ-AR-001', reviewer = 'reviewer@example' 
             WHERE id = ?1",
            params![id],
        )
        .expect("stamp objective and reviewer");
}

/// Record a source in the evidence vault so it may be cited.
///
/// Migration 004 requires every citation to resolve, so a test that cites a
/// source without recording it is now refused -- which is the point.
fn put_source(db: &Database, source_id: &str) {
    db.connection()
        .execute(
            "INSERT OR IGNORE INTO evidence_records
                 (id, content_hash, url, title, license, effective_date, trust,
                  retrieval_status, created_at)
             VALUES (?1, ?2, 'https://www.officialasvab.com/', 'ASVAB subtest constructs',
                     'US-Government-Work', '2026-09-22', 0.9, 'retrieved', ?3)",
            params![source_id, format!("sha256:{source_id}"), NOW],
        )
        .expect("record source in the vault");
}

fn cite_source(db: &Database, item_id: &str, source_id: &str) {
    put_source(db, source_id);
    db.connection()
        .execute(
            "INSERT INTO content_item_sources (item_id, source_id) VALUES (?1, ?2)",
            params![item_id, source_id],
        )
        .expect("cite source");
}

// ---------------------------------------------------------------------------
// Shape constraints
// ---------------------------------------------------------------------------

#[test]
fn a_well_formed_item_can_be_stored_as_draft() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    let item = NewItem::ar("q-ar-1", "sha256:aaa");
    assert_eq!(insert(&db, &item).expect("insert"), 1);
}

#[test]
fn the_correct_answer_must_index_an_option_that_exists() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    let mut item = NewItem::ar("q-ar-1", "sha256:aaa");
    // Only four options exist, so index 4 cannot be correct.
    item.correct_index = 4;
    assert!(
        insert(&db, &item).is_err(),
        "a correct_index outside the option list must be rejected"
    );
}

#[test]
fn a_question_needs_at_least_two_options() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    let mut item = NewItem::ar("q-ar-1", "sha256:aaa");
    item.options_json = r#"["only"]"#;
    item.correct_index = 0;
    assert!(
        insert(&db, &item).is_err(),
        "a single-option item is not a question"
    );
}

#[test]
fn an_empty_stem_is_rejected() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    let result = db.connection().execute(
        "INSERT INTO content_items (
             id, subtest, state, stem, options_json, correct_index, proof_kind,
             proof_json, content_hash, created_at, updated_at
         ) VALUES ('x', 'AR', 'draft', '   ', '[\"a\",\"b\"]', 0, 'executable',
                   '{}', 'sha256:x', ?1, ?1)",
        params![NOW],
    );
    assert!(result.is_err(), "whitespace-only stem must be rejected");
}

#[test]
fn identical_content_deduplicates_to_one_identity() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:same")).expect("first");
    let duplicate = insert(&db, &NewItem::ar("q-ar-2", "sha256:same"));
    assert!(
        duplicate.is_err(),
        "content_hash is UNIQUE so identical content cannot be stored twice"
    );
}

// ---------------------------------------------------------------------------
// Lifecycle: the state machine is enforced by the database, not only in Rust
// ---------------------------------------------------------------------------

#[test]
fn items_cannot_be_inserted_already_active() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    let result = db.connection().execute(
        "INSERT INTO content_items (
             id, subtest, objective_id, state, stem, options_json, correct_index,
             proof_kind, proof_json, reviewer, content_hash, created_at, updated_at
         ) VALUES ('x', 'AR', 'OBJ-1', 'active', 'stem', '[\"a\",\"b\"]', 0,
                   'executable', '{}', 'someone', 'sha256:x', ?1, ?1)",
        params![NOW],
    );
    assert!(
        result.is_err(),
        "items enter at draft; activation must go through the pipeline"
    );
}

#[test]
fn a_transition_that_skips_the_pipeline_is_refused() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    assert!(
        set_state(&db, "q-ar-1", "active").is_err(),
        "draft -> active skips three documented stages"
    );
    assert!(
        set_state(&db, "q-ar-1", "content_reviewed").is_err(),
        "draft -> content_reviewed is not a documented transition"
    );
}

#[test]
fn any_state_may_be_quarantined_and_remediated() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    set_state(&db, "q-ar-1", "machine_validated").expect("advance");
    set_state(&db, "q-ar-1", "quarantined").expect("quarantine from any state");
    set_state(&db, "q-ar-1", "draft").expect("a quarantined item re-enters review");
}

#[test]
fn activation_without_a_cited_source_is_refused() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    walk_to_content_reviewed(&db, "q-ar-1");
    let activated = set_state(&db, "q-ar-1", "active");
    assert!(
        activated.is_err(),
        "REQ-056: an active item must cite at least one source"
    );
}

#[test]
fn activation_succeeds_once_provenance_is_complete() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    walk_to_content_reviewed(&db, "q-ar-1");
    cite_source(&db, "q-ar-1", "SRC-ASVAB-SPEC-001");
    assert_eq!(set_state(&db, "q-ar-1", "active").expect("activate"), 1);

    let state: String = db
        .connection()
        .query_row(
            "SELECT state FROM content_items WHERE id = 'q-ar-1'",
            [],
            |r| r.get(0),
        )
        .expect("read back");
    assert_eq!(state, "active");
}

#[test]
fn an_item_without_an_objective_cannot_activate() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    for state in [
        "machine_validated",
        "independent_verified",
        "content_reviewed",
    ] {
        set_state(&db, "q-ar-1", state).expect("advance");
    }
    cite_source(&db, "q-ar-1", "SRC-1");
    db.connection()
        .execute(
            "UPDATE content_items SET reviewer = 'r@example' WHERE id = 'q-ar-1'",
            [],
        )
        .expect("set reviewer, leave objective empty");
    assert!(
        set_state(&db, "q-ar-1", "active").is_err(),
        "REQ-056: activation requires a learning objective"
    );
}

#[test]
fn an_item_without_a_reviewer_cannot_activate() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    for state in [
        "machine_validated",
        "independent_verified",
        "content_reviewed",
    ] {
        set_state(&db, "q-ar-1", state).expect("advance");
    }
    cite_source(&db, "q-ar-1", "SRC-1");
    db.connection()
        .execute(
            "UPDATE content_items SET objective_id = 'OBJ-1' WHERE id = 'q-ar-1'",
            [],
        )
        .expect("set objective, leave reviewer empty");
    assert!(
        set_state(&db, "q-ar-1", "active").is_err(),
        "REQ-056: activation requires a named reviewer"
    );
}

// ---------------------------------------------------------------------------
// REQ-022: the deterministic-proof requirement that was previously unenforced
// ---------------------------------------------------------------------------

#[test]
fn a_quantitative_item_cannot_activate_on_a_non_deterministic_proof() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    // AR is computable, so a source-backed rubric is not sufficient evidence.
    let mut item = NewItem::ar("q-ar-1", "sha256:aaa");
    item.proof_kind = "source_backed";
    insert(&db, &item).expect("insert");
    walk_to_content_reviewed(&db, "q-ar-1");
    cite_source(&db, "q-ar-1", "SRC-1");
    assert!(
        set_state(&db, "q-ar-1", "active").is_err(),
        "AR must carry an executable proof; `is_deterministic` had no callers"
    );
}

#[test]
fn a_verbal_item_may_activate_on_a_source_backed_rubric() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    let mut item = NewItem::ar("q-wk-1", "sha256:bbb");
    item.subtest = "WK";
    item.proof_kind = "source_backed";
    insert(&db, &item).expect("insert");
    walk_to_content_reviewed(&db, "q-wk-1");
    cite_source(&db, "q-wk-1", "SRC-WEBSTER-1913");
    assert_eq!(
        set_state(&db, "q-wk-1", "active").expect("activate"),
        1,
        "a synonym item's evidence is its cited dictionary entry"
    );
}

#[test]
fn a_verifier_that_echoed_the_generator_is_not_independent_verification() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    walk_to_content_reviewed(&db, "q-ar-1");
    cite_source(&db, "q-ar-1", "SRC-1");
    db.connection()
        .execute(
            "UPDATE content_items SET generator_hash = 'sha256:g', verifier_hash = 'sha256:g'
             WHERE id = 'q-ar-1'",
            [],
        )
        .expect("set identical hashes");
    assert!(
        set_state(&db, "q-ar-1", "active").is_err(),
        "identical generator and verifier output means verification did not occur"
    );
}

#[test]
fn an_independent_verifier_with_distinct_output_may_activate() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    walk_to_content_reviewed(&db, "q-ar-1");
    cite_source(&db, "q-ar-1", "SRC-1");
    db.connection()
        .execute(
            "UPDATE content_items SET generator_hash = 'sha256:g', verifier_hash = 'sha256:v'
             WHERE id = 'q-ar-1'",
            [],
        )
        .expect("set distinct hashes");
    assert_eq!(set_state(&db, "q-ar-1", "active").expect("activate"), 1);
}

// ---------------------------------------------------------------------------
// REQ-048: the review audit trail
// ---------------------------------------------------------------------------

#[test]
fn review_history_is_append_only() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    db.connection()
        .execute(
            "INSERT INTO content_item_reviews
                 (id, item_id, from_state, to_state, actor, rationale, created_at)
             VALUES ('rev-1', 'q-ar-1', 'draft', 'machine_validated', 'generator',
                     'proof check passed', ?1)",
            params![NOW],
        )
        .expect("append review");

    let rewritten = db.connection().execute(
        "UPDATE content_item_reviews SET rationale = 'rewritten' WHERE id = 'rev-1'",
        [],
    );
    assert!(
        rewritten.is_err(),
        "an audit trail that can be edited is not an audit trail"
    );
}

#[test]
fn sources_are_recorded_per_item_and_cascade_with_it() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    cite_source(&db, "q-ar-1", "SRC-A");
    cite_source(&db, "q-ar-1", "SRC-B");

    let count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM content_item_sources WHERE item_id = 'q-ar-1'",
            [],
            |r| r.get(0),
        )
        .expect("count sources");
    assert_eq!(count, 2, "an item may cite more than one source");

    db.connection()
        .execute("DELETE FROM content_items WHERE id = 'q-ar-1'", [])
        .expect("delete item");
    let remaining: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM content_item_sources WHERE item_id = 'q-ar-1'",
            [],
            |r| r.get(0),
        )
        .expect("count after delete");
    assert_eq!(remaining, 0, "citations cascade with the item");
}

// ---------------------------------------------------------------------------
// Migration 004: a citation must resolve, in both directions
// ---------------------------------------------------------------------------

#[test]
fn a_citation_must_name_a_source_the_vault_has_recorded() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    let unrecorded = db.connection().execute(
        "INSERT INTO content_item_sources (item_id, source_id)
         VALUES ('q-ar-1', 'SRC-NEVER-RECORDED')",
        [],
    );
    assert!(
        unrecorded.is_err(),
        "REQ-056: naming a source the vault has never seen is not provenance"
    );
}

#[test]
fn a_citation_cannot_be_rewritten_to_an_unrecorded_source() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    cite_source(&db, "q-ar-1", "SRC-A");

    // Inserting a valid pair and then rewriting the key would side-step an
    // insert-only rule, so the update path is guarded too.
    let rewritten = db.connection().execute(
        "UPDATE content_item_sources SET source_id = 'SRC-NEVER-RECORDED'
         WHERE item_id = 'q-ar-1'",
        [],
    );
    assert!(rewritten.is_err(), "the citation key must stay resolvable");
}

#[test]
fn a_recorded_source_cannot_be_deleted_while_an_item_cites_it() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    insert(&db, &NewItem::ar("q-ar-1", "sha256:aaa")).expect("insert");
    cite_source(&db, "q-ar-1", "SRC-A");

    let deleted = db
        .connection()
        .execute("DELETE FROM evidence_records WHERE id = 'SRC-A'", []);
    assert!(
        deleted.is_err(),
        "deleting a cited source would leave dangling provenance"
    );

    // Once nothing cites it, the vault row is free to go.
    db.connection()
        .execute(
            "DELETE FROM content_item_sources WHERE item_id = 'q-ar-1'",
            [],
        )
        .expect("remove citation");
    assert_eq!(
        db.connection()
            .execute("DELETE FROM evidence_records WHERE id = 'SRC-A'", [])
            .expect("delete uncited source"),
        1
    );
}

#[test]
fn recording_the_same_source_twice_is_a_no_op() {
    let dir = tempdir::TempDir::new("vec-items");
    let db = migrated(&dir);
    put_source(&db, "SRC-UNUSED");
    put_source(&db, "SRC-UNUSED");
    let count: i64 = db
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM evidence_records WHERE id = 'SRC-UNUSED'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(count, 1, "the vault is hash-addressed and deduplicates");
}
