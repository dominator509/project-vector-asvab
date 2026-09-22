//! Factual ingestion end to end: a question-and-answer work to servable items.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! What is different here from the other three paths is where the *distractors* come
//! from. A Word Knowledge item's wrong answers are synonyms, an Electronics
//! Information item's are neighbouring glossary terms, and a Paragraph Comprehension
//! item's are the right sentence with one word changed. A factual item's wrong
//! answers are the same work's answers to other questions -- each one a true
//! statement about something else, which is what makes the item turn on knowledge
//! rather than on recognising the shape of an answer.

use std::path::Path;

use vector_application::content::{ContentPipeline, FactIngestRequest};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};
use vector_questions::facts::{parse_faq, verify, FactItem, Faq};

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
                "vector-facts-{}-{}-{}-{}",
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

/// A miniature question-and-answer work in the corpus's shape: a contents list that
/// repeats every question, answers running over several paragraphs, and a licence
/// block after the end marker.
const WORK: &str = "\
The Project Gutenberg eBook of A Test Work

*** START OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

CONTENTS

Why Do We Count in Tens?
What Makes the Wind Blow?
Why Does a Pencil Write?
Why Is the Sky Blue?

Why Do We Count in Tens?
When man found it necessary to count, the only implements at hand were his fingers
and his toes. He had ten of each, so he counted in tens and has done so ever since.

What Makes the Wind Blow?
Air moves from where the pressure is high to where the pressure is low. The sun
heats some places more than others, which is what starts the difference.

Why Does a Pencil Write?
The lead of a pencil is a soft form of carbon that rubs off on the paper. The
roughness of the paper pulls the particles away from the point.

Why Is the Sky Blue?
The air scatters the shorter wavelengths of sunlight far more than the longer ones.
Our eyes read that scattered light as blue.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***
";

fn migrations() -> Vec<Migration> {
    MigrationManager::load_from_dir(Path::new("../../migrations")).expect("migrations load")
}

fn database(tag: &str) -> (tempdir::TempDir, Database, String) {
    let dir = tempdir::TempDir::new(tag);
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let source = EvidenceRepo::new(&db)
        .put(&NewEvidence::new(
            "https://www.gutenberg.org/ebooks/99999",
            "A Test Work (Project Gutenberg #99999)",
            "sha256:facts-test-work",
            "Public domain in the USA",
            "2026-09-22",
            0.90,
            "retrieved",
        ))
        .expect("record the work");
    (dir, db, source)
}

fn request<'a>(source: &'a str) -> FactIngestRequest<'a> {
    FactIngestRequest {
        subtest: "GS",
        label: "A Test Work",
        source_id: source,
        count: 8,
        seed: 20_260_922,
        reviewer: "content-reviewer",
        generator: "fact-ingester",
    }
}

fn fixture() -> Faq {
    parse_faq(WORK, "A Test Work")
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

#[test]
fn ingestion_activates_servable_items_citing_the_work() {
    let (_dir, db, source) = database("ingest");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_facts(&fixture(), &request(&source))
        .expect("ingest");

    assert_eq!(report.questions, 4, "the fixture asks four questions");
    assert!(
        report.built > 0,
        "the fixture should yield items: {report:?}"
    );
    assert_eq!(report.activated, report.built, "{:?}", report.rejected);
    assert!(report.is_clean(), "rejections: {:?}", report.rejected);

    let repo = ContentItemRepo::new(&db);
    let servable = repo.servable("GS").expect("servable");
    assert_eq!(servable.len(), report.activated);

    for item in &servable {
        assert_eq!(item.state, "active");
        assert_eq!(item.proof_kind, "source_backed");
        assert!(!item.objective_id.trim().is_empty());
        assert!(!item.reviewer.trim().is_empty());
        assert_eq!(
            repo.sources(&item.id).expect("sources"),
            vec![source.clone()],
            "{}: the work must be cited",
            item.id
        );
        assert_ne!(
            item.generator_hash, item.verifier_hash,
            "{}: identical generator and verifier output is not verification",
            item.id
        );
    }
}

#[test]
fn ingested_items_are_reachable_through_the_serving_api() {
    let (_dir, db, source) = database("serving");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_facts(&fixture(), &request(&source))
        .expect("ingest");

    let item = pipeline
        .next_item("GS", &[])
        .expect("next")
        .expect("GS must be servable once ingested");
    assert_eq!(item.subtest, "GS");
    assert_eq!(item.options.len(), 4);
    assert_eq!(item.distractor_rationales.len(), 3);
    assert!(
        item.stem.ends_with('?'),
        "a factual item's stem is a question: {:?}",
        item.stem
    );
    // A factual item is not a comprehension item, so it must not claim a passage.
    assert_eq!(item.passage, None);

    let stats = pipeline.stats().expect("stats");
    assert!(
        stats.by_subtest.iter().any(|s| s.subtest == "GS"),
        "{:?}",
        stats.by_subtest
    );
}

#[test]
fn the_stored_rubric_names_the_work() {
    let (_dir, db, source) = database("rubric");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_facts(&fixture(), &request(&source))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("GS").expect("servable") {
        let proof: serde_json::Value =
            serde_json::from_str(&item.proof_json).expect("proof is JSON");
        let source_backed = proof
            .get("SourceBacked")
            .expect("a source-backed proof")
            .as_object()
            .expect("proof object");
        assert_eq!(source_backed["source_id"].as_str(), Some(source.as_str()));
        let rubric = source_backed["rubric"].as_str().expect("rubric");
        assert!(
            rubric.contains("A Test Work"),
            "the rubric should name the work: {rubric}"
        );
    }
}

#[test]
fn the_audit_trail_records_the_whole_pipeline() {
    let (_dir, db, source) = database("audit");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_facts(&fixture(), &request(&source))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for id in &report.item_ids {
        let history = repo.history(id).expect("history");
        assert_eq!(history.len(), 4, "{id}: {history:?}");
        assert_eq!(history[3].to_state, "active");
        assert_eq!(history[3].actor, "content-reviewer");
    }
}

// ---------------------------------------------------------------------------
// Reproducibility and refusal
// ---------------------------------------------------------------------------

#[test]
fn repeating_an_ingestion_adds_nothing() {
    let (_dir, db, source) = database("idempotent");
    let pipeline = ContentPipeline::new(&db);
    let first = pipeline
        .ingest_facts(&fixture(), &request(&source))
        .expect("first");
    let second = pipeline
        .ingest_facts(&fixture(), &request(&source))
        .expect("second");

    assert_eq!(second.activated, 0, "nothing new on a repeat: {second:?}");
    assert_eq!(second.already_present, first.activated);
}

#[test]
fn ingestion_refuses_a_work_the_vault_has_not_recorded() {
    let (_dir, db, _source) = database("unknown-source");
    let pipeline = ContentPipeline::new(&db);
    let error = pipeline
        .ingest_facts(&fixture(), &request("SRC-INVENTED"))
        .expect_err("an unrecorded source must be refused");
    assert!(
        error.to_string().contains("vault") || error.to_string().contains("source"),
        "the refusal should name the problem: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("GS")
            .expect("servable")
            .len(),
        0
    );
}

#[test]
fn an_empty_text_is_an_error_rather_than_an_empty_corpus() {
    let (_dir, db, source) = database("empty");
    let pipeline = ContentPipeline::new(&db);
    let result = pipeline.ingest_facts(&parse_faq("", "A Test Work"), &request(&source));
    assert!(
        result.is_err(),
        "an empty file must not report a successful empty ingestion"
    );
}

/// The refusal this path's distractor rule exists for.
///
/// A distractor has to be an answer the work gives to a *different* question. If it
/// answered the question that was asked, two options would be true and the item would
/// have no unique answer.
#[test]
fn storing_refuses_a_distractor_that_answers_the_question_asked() {
    let (_dir, db, source) = database("same-question");
    let pipeline = ContentPipeline::new(&db);
    let faq = fixture();

    let mut item: FactItem = faq
        .build_items("GS", 1, 20_260_922, 6, 30, |_| true)
        .into_iter()
        .next()
        .expect("the fixture yields an item");
    let wrong = (item.correct_index + 1) % item.options.len();
    // Point a distractor at the answer to the question being asked.
    item.options[wrong] = item.supporting_answer.clone();
    if wrong == item.correct_index {
        item.correct_index = (wrong + 1) % item.options.len();
    }
    verify(&item, &faq).expect_err("two options cannot both answer the question");

    let error = pipeline
        .store_fact_verified(&faq, &request(&source), &item)
        .expect_err("an ambiguous item must be refused");
    assert!(
        error.to_string().contains("verification"),
        "the refusal should name verification: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("GS")
            .expect("servable")
            .len(),
        0
    );
}
