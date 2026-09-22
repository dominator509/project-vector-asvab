//! Shop Information ingestion end to end: a tool manual to servable items.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! What makes this path different is where the item *comes from*. The other ingested
//! paths quote something the source already wrote as a question or a statement; this one
//! forms a question from a tool description, so the prompt is written by this program
//! while the purpose clause inside it is the manual's own words. The tests therefore pin
//! down two things the other paths do not have to: that the prompt follows the grammar of
//! the clause it was built from, and that the OCR check is applied to *every* option,
//! because a misread word becomes a wrong answer that looks like a tool.

use std::path::Path;

use vector_application::content::{ContentPipeline, PurposeIngestRequest};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};
use vector_questions::dictionary::{parse_webster, Dictionary};
use vector_questions::purposes::{parse_purposes, verify, PurposeItem, Purposes};

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
                "vector-purposes-{}-{}-{}-{}",
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

/// A miniature tool manual with the scanner's habits, including one misread word that
/// must not reach an option.
const MANUAL: &str = "\
CHAPTER 1 COMMON HANDTOOLS

SCREW AND TAP EXTRACTORS
Screw extractors are used to remove broken screws without damaging the surrounding
material.

MICROMETERS
Microm-
eters are used to measure distances to the nearest one thousandth of an inch.

VISES AND CLAMPS
Vises are used for holding work when it is being planed, sawed, or drilled.

PLIERS
Long-nose pliers are used for gripping, reaching places not readily accessible to the
hand.

The inuide micrometer is used for measuring inside dimensions of a bored hole.

WRENCHES
Open-end wrenches are used to turn nuts and bolts in places where a socket will not fit.
";

/// A miniature Webster's holding the words the fixture manual's tool names are built from.
///
/// It has to be this complete because the shop path checks every word of an option: a
/// fixture dictionary missing `open` would refuse `Open-end wrenches`, and the test would
/// be measuring the fixture rather than the rule.
const DICTIONARY: &str = "\
*** START OF THE PROJECT GUTENBERG EBOOK ***

Screw

Screw, n. A cylindrical thread.

Extractor

Extractor, n. An instrument for drawing out.

Micrometer

Micrometer, n. An instrument for measuring small distances.

Vise

Vise, n. A clamping device.

Pliers

Pliers, n. A gripping tool.

Wrench

Wrench, n. A tool for turning nuts.

Inside

Inside, n. The interior.

Open

Open, a. Not closed.

End

End, n. The extremity of anything.

Long

Long, a. Extended in length.

Nose

Nose, n. The prominent part of the face.

Hand

Hand, n. The extremity of the arm.
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
            "https://archive.org/details/TM11-453",
            "TM 11-453 Shop Work (Internet Archive TM11-453)",
            "sha256:tools-test-manual",
            "Public domain (US government work)",
            "2026-09-22",
            0.90,
            "retrieved",
        ))
        .expect("record the manual");
    (dir, db, source)
}

fn dictionary() -> Dictionary {
    parse_webster(DICTIONARY)
}

fn fixture() -> Purposes {
    parse_purposes(MANUAL, "A Test Manual")
}

fn request<'a>(source: &'a str, dictionary: &'a Dictionary) -> PurposeIngestRequest<'a> {
    PurposeIngestRequest {
        subtest: "SI",
        label: "A Test Manual",
        source_id: source,
        count: 8,
        seed: 20_260_922,
        dictionary,
        reviewer: "content-reviewer",
        generator: "tool-ingester",
    }
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

#[test]
fn ingestion_activates_servable_items_citing_the_manual() {
    let (_dir, db, source) = database("ingest");
    let pipeline = ContentPipeline::new(&db);
    let dictionary = dictionary();
    let report = pipeline
        .ingest_purposes(&fixture(), &request(&source, &dictionary))
        .expect("ingest");

    assert!(report.statements >= 4, "the fixture describes tools");
    assert!(
        report.built > 0,
        "the fixture should yield items: {report:?}"
    );
    assert_eq!(report.activated, report.built, "{:?}", report.rejected);
    assert!(report.is_clean(), "rejections: {:?}", report.rejected);

    let repo = ContentItemRepo::new(&db);
    let servable = repo.servable("SI").expect("servable");
    assert_eq!(servable.len(), report.activated);
    for item in &servable {
        assert_eq!(item.state, "active");
        assert_eq!(item.proof_kind, "source_backed");
        assert!(!item.objective_id.trim().is_empty());
        assert!(!item.reviewer.trim().is_empty());
        assert_eq!(
            repo.sources(&item.id).expect("sources"),
            vec![source.clone()],
            "{}: the manual must be cited",
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
    let dictionary = dictionary();
    pipeline
        .ingest_purposes(&fixture(), &request(&source, &dictionary))
        .expect("ingest");

    let item = pipeline
        .next_item("SI", &[])
        .expect("next")
        .expect("SI must be servable once ingested");
    assert_eq!(item.subtest, "SI");
    assert_eq!(item.options.len(), 4);
    assert_eq!(item.distractor_rationales.len(), 3);
    assert!(
        item.stem.starts_with("Which tool is used"),
        "an SI item asks what a tool is for: {:?}",
        item.stem
    );
    // A shop item is not a comprehension item and needs no passage.
    assert_eq!(item.passage, None);

    let stats = pipeline.stats().expect("stats");
    assert!(
        stats.by_subtest.iter().any(|s| s.subtest == "SI"),
        "{:?}",
        stats.by_subtest
    );
}

/// The check this path exists to make: a misread word must not become a wrong answer.
#[test]
fn an_option_containing_a_misreading_is_refused() {
    let (_dir, db, source) = database("misreading");
    let pipeline = ContentPipeline::new(&db);
    let dictionary = dictionary();

    // `inuide` is one substitution from `inside`, which the dictionary has. The fixture
    // contains a sentence naming it, so the source offers it as a tool; keeping it out of
    // an option is the pipeline's job, not the builder's.
    let manual = fixture();
    assert!(
        manual
            .entries()
            .iter()
            .any(|entry| entry.tool.contains("inuide")),
        "the fixture should describe the misread tool: {:?}",
        manual.entries()
    );

    pipeline
        .ingest_purposes(&manual, &request(&source, &dictionary))
        .expect("ingest");
    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("SI").expect("servable") {
        for option in &item.options {
            assert!(
                !option.contains("inuide"),
                "{}: a misreading reached an option: {option}",
                item.id
            );
        }
    }
}

#[test]
fn the_stored_rubric_names_the_manual() {
    let (_dir, db, source) = database("rubric");
    let pipeline = ContentPipeline::new(&db);
    let dictionary = dictionary();
    pipeline
        .ingest_purposes(&fixture(), &request(&source, &dictionary))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("SI").expect("servable") {
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
            rubric.contains("A Test Manual"),
            "the rubric should name the manual: {rubric}"
        );
    }
}

#[test]
fn the_audit_trail_records_the_whole_pipeline() {
    let (_dir, db, source) = database("audit");
    let pipeline = ContentPipeline::new(&db);
    let dictionary = dictionary();
    let report = pipeline
        .ingest_purposes(&fixture(), &request(&source, &dictionary))
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
    let dictionary = dictionary();
    let first = pipeline
        .ingest_purposes(&fixture(), &request(&source, &dictionary))
        .expect("first");
    let second = pipeline
        .ingest_purposes(&fixture(), &request(&source, &dictionary))
        .expect("second");

    assert_eq!(second.activated, 0, "nothing new on a repeat: {second:?}");
    assert_eq!(second.already_present, first.activated);
}

#[test]
fn ingestion_refuses_a_manual_the_vault_has_not_recorded() {
    let (_dir, db, _source) = database("unknown-source");
    let pipeline = ContentPipeline::new(&db);
    let dictionary = dictionary();
    let error = pipeline
        .ingest_purposes(&fixture(), &request("SRC-INVENTED", &dictionary))
        .expect_err("an unrecorded source must be refused");
    assert!(
        error.to_string().contains("vault") || error.to_string().contains("source"),
        "the refusal should name the problem: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("SI")
            .expect("servable")
            .len(),
        0
    );
}

#[test]
fn an_empty_manual_is_an_error_rather_than_an_empty_corpus() {
    let (_dir, db, source) = database("empty");
    let pipeline = ContentPipeline::new(&db);
    let dictionary = dictionary();
    let result = pipeline.ingest_purposes(
        &parse_purposes("", "A Test Manual"),
        &request(&source, &dictionary),
    );
    assert!(
        result.is_err(),
        "an empty manual must not report a successful empty ingestion"
    );
}

/// The refusal this path's distractor rule exists for.
#[test]
fn storing_refuses_a_distractor_that_serves_the_same_purpose() {
    let (_dir, db, source) = database("same-purpose");
    let pipeline = ContentPipeline::new(&db);
    let faq = fixture();
    let dictionary = dictionary();

    let mut item: PurposeItem = faq
        .build_items("SI", 1, 20_260_922, |_| true)
        .into_iter()
        .next()
        .expect("the fixture yields an item");
    let wrong = (item.correct_index + 1) % item.options.len();
    let correct = item.options[item.correct_index].clone();
    // Point a distractor at the tool the source names for this very purpose.
    item.options[wrong] = correct;
    if wrong == item.correct_index {
        item.correct_index = (wrong + 1) % item.options.len();
    }

    let error = pipeline
        .store_purpose_verified(&faq, &request(&source, &dictionary), &item)
        .expect_err("two options serving the same purpose must be refused");
    assert!(
        error.to_string().contains("verification"),
        "the refusal should name verification: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("SI")
            .expect("servable")
            .len(),
        0
    );
}

/// The verifier re-derives the item from the source, so a doctored prompt is caught.
#[test]
fn storing_refuses_a_purpose_the_manual_does_not_state() {
    let (_dir, db, source) = database("doctored");
    let pipeline = ContentPipeline::new(&db);
    let faq = fixture();
    let dictionary = dictionary();

    let mut item: PurposeItem = faq
        .build_items("SI", 1, 20_260_922, |_| true)
        .into_iter()
        .next()
        .expect("the fixture yields an item");
    item.prompt = item.prompt.replace('?', " underwater?");
    verify(&item, &faq).expect_err("the source does not state that purpose");

    let error = pipeline
        .store_purpose_verified(&faq, &request(&source, &dictionary), &item)
        .expect_err("a purpose the manual does not state must be refused");
    assert!(
        error.to_string().contains("verification"),
        "the refusal should name verification: {error}"
    );
}
