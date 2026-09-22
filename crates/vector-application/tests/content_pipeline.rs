//! Content pipeline end to end: factory to active, provable, stored items.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! The strongest test here reads an item's proof back out of SQLite, re-evaluates
//! the expression text, and checks the result against the option the database
//! says is correct. That exercises the whole chain -- factory, verifier, schema,
//! repository, storage -- and would fail if any link let an unprovable item
//! become servable.

use std::path::Path;

use serde_json::Value;
use vector_application::content::{ContentPipeline, GenerateRequest};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};
use vector_questions::proof;

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
                "vector-pipeline-{}-{}-{}-{}",
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

/// A migrated database with the source every generated item will cite.
fn database(tag: &str) -> (tempdir::TempDir, Database) {
    let dir = tempdir::TempDir::new(tag);
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    EvidenceRepo::new(&db)
        .put(&NewEvidence::new(
            "https://www.officialasvab.com/applicants/sample-questions/",
            "ASVAB subtest constructs",
            "sha256:asvab-spec",
            "US-Government-Work",
            "2026-09-22",
            0.9,
            "retrieved",
        ))
        .expect("record source");
    (dir, db)
}

fn source_id(db: &Database) -> String {
    db.connection()
        .query_row("SELECT id FROM evidence_records LIMIT 1", [], |r| {
            r.get::<_, String>(0)
        })
        .expect("source id")
}

fn request<'a>(subtest: &'a str, count: usize, seed: u64, source: &'a str) -> GenerateRequest<'a> {
    GenerateRequest {
        subtest,
        count,
        seed,
        reviewer: "content-reviewer",
        source_id: source,
        generator: "factory",
    }
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

#[test]
fn a_run_generates_verifies_and_activates_a_batch() {
    let (_dir, db) = database("batch");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);

    let report = pipeline
        .generate_and_activate(&request("AR", 60, 20_260_922, &source))
        .expect("pipeline run");

    assert_eq!(report.subtest, "AR");
    assert!(report.generated >= 60, "generated {}", report.generated);
    assert_eq!(
        report.verified, report.generated,
        "every generated item must verify before storage"
    );
    assert_eq!(
        report.activated, report.generated,
        "every verified item must activate: {:?}",
        report.rejected
    );
    assert!(
        report.is_clean(),
        "a clean run has no rejections: {:?}",
        report.rejected
    );
    assert_eq!(report.item_ids.len(), report.activated);
}

#[test]
fn every_activated_item_carries_complete_provenance() {
    let (_dir, db) = database("provenance");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .generate_and_activate(&request("MK", 40, 7_777, &source))
        .expect("pipeline run");

    let repo = ContentItemRepo::new(&db);
    let items = repo.servable("MK").expect("servable");
    assert_eq!(items.len(), 40, "all 40 should be servable");

    for item in &items {
        assert_eq!(item.state, "active");
        assert!(
            !item.objective_id.trim().is_empty(),
            "REQ-056: {} has no objective",
            item.id
        );
        assert!(
            !item.reviewer.trim().is_empty(),
            "REQ-056: {} has no reviewer",
            item.id
        );
        assert!(
            !item.content_hash.trim().is_empty(),
            "REQ-056: {} has no content hash",
            item.id
        );
        assert_eq!(
            item.proof_kind, "executable",
            "a computable subtest must carry a deterministic proof"
        );
        assert_eq!(
            repo.sources(&item.id).expect("sources").len(),
            1,
            "REQ-056: {} cites no source",
            item.id
        );

        // The verifier and the generator must have produced different artefacts.
        let generator = item.generator_hash.clone().expect("generator hash");
        let verifier = item.verifier_hash.clone().expect("verifier hash");
        assert_ne!(
            generator, verifier,
            "{}: identical generator and verifier output is not verification",
            item.id
        );
    }
}

#[test]
fn the_stored_proof_re_evaluates_to_the_stored_correct_option() {
    let (_dir, db) = database("reproof");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .generate_and_activate(&request("AR", 30, 424_242, &source))
        .expect("pipeline run");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("AR").expect("servable") {
        // Parse the proof exactly as it came out of SQLite.
        let parsed: Value = serde_json::from_str(&item.proof_json).expect("proof is JSON");
        let (expression, answer) = match &parsed {
            Value::Object(map) => {
                let executable = map
                    .get("Executable")
                    .expect("an executable proof")
                    .as_object()
                    .expect("executable proof object");
                (
                    executable["expression"]
                        .as_str()
                        .expect("expression")
                        .to_string(),
                    executable["answer"].as_str().expect("answer").to_string(),
                )
            }
            other => panic!("unexpected proof shape: {other}"),
        };

        let recomputed = proof::evaluate(&expression)
            .unwrap_or_else(|e| panic!("{}: stored proof does not evaluate: {e}", item.id));
        assert_eq!(
            recomputed.to_string(),
            answer,
            "{}: stored answer disagrees with its stored expression",
            item.id
        );
        let stored_option: i128 = item.options[item.correct_index]
            .parse()
            .unwrap_or_else(|_| panic!("{}: correct option is not an integer", item.id));
        assert_eq!(
            recomputed, stored_option,
            "{}: the option marked correct is not the proven answer",
            item.id
        );
    }
}

/// The refusal path must be reachable, or deleting the verification would break
/// no test. The factory's own output always verifies, so the item is hand-built.
#[test]
fn an_unprovable_item_is_refused_and_not_stored() {
    let (_dir, db) = database("refusal");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);

    // Arithmetically wrong but internally consistent: the option and the answer
    // field agree with each other, so only re-deriving from the expression can
    // catch it.
    let broken = vector_questions::factory::GeneratedItem {
        subtest: "AR".to_string(),
        objective_id: "OBJ-AR-RATE-01".to_string(),
        template_id: "hand.built",
        stem: "A machine makes 5 parts per minute. How many in 1 hour?".to_string(),
        options: vec![
            "300".to_string(),
            "5".to_string(),
            "60".to_string(),
            "12".to_string(),
        ],
        correct_index: 0,
        expression: "5 * 6".to_string(),
        answer: "300".to_string(),
        distractor_rationales: [
            (1, "Used the rate alone.".to_string()),
            (2, "Used the minutes alone.".to_string()),
            (3, "Divided instead of multiplying.".to_string()),
        ]
        .into_iter()
        .collect(),
        difficulty: 0.0,
        seed: 0,
    };

    let error = pipeline
        .store_verified(&request("AR", 1, 0, &source), &broken)
        .expect_err("an unprovable item must be refused");
    assert!(
        error.to_string().contains("independent verification"),
        "the refusal should say why: {error}"
    );

    let repo = ContentItemRepo::new(&db);
    assert!(
        repo.all().expect("all").is_empty(),
        "a refused item must leave nothing behind, not even a draft"
    );
    assert!(
        repo.servable("AR").expect("servable").is_empty(),
        "a refused item must never be servable"
    );
}

#[test]
fn the_audit_trail_records_the_whole_pipeline_for_each_item() {
    let (_dir, db) = database("audit");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .generate_and_activate(&request("AR", 10, 31_337, &source))
        .expect("pipeline run");

    let repo = ContentItemRepo::new(&db);
    for id in &report.item_ids {
        let history = repo.history(id).expect("history");
        assert_eq!(
            history.len(),
            4,
            "{id}: three pipeline steps plus activation, got {history:?}"
        );
        assert_eq!(history[0].from_state, "draft");
        assert_eq!(history[0].to_state, "machine_validated");
        assert_eq!(history[3].to_state, "active");
        assert_eq!(history[3].actor, "content-reviewer");
    }
}

// ---------------------------------------------------------------------------
// Reproducibility and idempotency
// ---------------------------------------------------------------------------

#[test]
fn the_same_seed_reproduces_the_batch_rather_than_duplicating_it() {
    let (_dir, db) = database("idempotent");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);

    let first = pipeline
        .generate_and_activate(&request("AR", 25, 555, &source))
        .expect("first run");
    assert_eq!(first.activated, 25);

    // A second run with the same seed regenerates the same questions, so every
    // one is already stored and nothing is duplicated.
    let second = pipeline
        .generate_and_activate(&request("AR", 25, 555, &source))
        .expect("second run");
    assert_eq!(
        second.already_present, 25,
        "a repeat seed must recognise identical content: {second:?}"
    );
    assert_eq!(second.activated, 0, "nothing new should be stored");

    let total = ContentItemRepo::new(&db)
        .counts_by_state()
        .expect("counts")
        .into_iter()
        .map(|(_, n)| n)
        .sum::<i64>();
    assert_eq!(total, 25, "the corpus must not grow on a repeat run");
}

#[test]
fn a_different_seed_adds_new_questions() {
    let (_dir, db) = database("distinct");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);

    pipeline
        .generate_and_activate(&request("AR", 20, 1, &source))
        .expect("first run");
    let second = pipeline
        .generate_and_activate(&request("AR", 20, 2, &source))
        .expect("second run");

    assert!(
        second.activated > 0,
        "a different seed should contribute new items: {second:?}"
    );
    let servable = ContentItemRepo::new(&db).servable("AR").expect("servable");
    assert!(
        servable.len() > 20,
        "the corpus should have grown past the first run, got {}",
        servable.len()
    );
}

#[test]
fn an_unknown_subtest_is_an_error_rather_than_an_empty_corpus() {
    let (_dir, db) = database("unknown");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);
    // WK has a dictionary-based plan but no template yet; silently producing
    // nothing would look like success.
    let result = pipeline.generate_and_activate(&request("WK", 10, 1, &source));
    assert!(
        result.is_err(),
        "an unsupported subtest must not report success"
    );
}

#[test]
fn items_are_scoped_to_their_subtest() {
    let (_dir, db) = database("scoping");
    let source = source_id(&db);
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .generate_and_activate(&request("AR", 10, 99, &source))
        .expect("AR run");
    pipeline
        .generate_and_activate(&request("MK", 10, 99, &source))
        .expect("MK run");

    let repo = ContentItemRepo::new(&db);
    assert_eq!(repo.servable("AR").expect("AR").len(), 10);
    assert_eq!(repo.servable("MK").expect("MK").len(), 10);
}
