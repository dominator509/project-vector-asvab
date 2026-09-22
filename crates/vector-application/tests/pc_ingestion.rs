//! Paragraph Comprehension ingestion end to end: public-domain prose to servable
//! items.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! Two things make this path different from the other two, and each has its own
//! test. The passage travels *inside* the item, so `verify` alone cannot tell an
//! item that quotes its source from one whose passage was invented; the source text
//! is therefore checked separately. And the answer is a detail stated in the
//! passage, so the distractors are that statement with one word changed, which is
//! only sound if the passage does not state the changed word anywhere.

use std::path::Path;

use vector_application::content::{ContentPipeline, PcIngestRequest};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};
use vector_questions::passages::{parse_gutenberg, verify, PcItem, Text};

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
                "vector-pc-{}-{}-{}-{}",
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

/// A miniature Gutenberg file in the corpus's own shape.
///
/// Every sentence carries a quantity, because the builder picks one sentence of a
/// passage to rest on and gives up on the passage when that sentence offers nothing
/// to alter; a fixture where only some sentences qualify would make the tests
/// depend on which one the generator happened to draw. The quantities within a
/// paragraph are kept far apart so that a distractor derived from one cannot
/// accidentally be a quantity the passage states elsewhere.
const WORK: &str = "\
The Project Gutenberg eBook of A Test Work

This eBook is for the use of anyone anywhere at no cost.

*** START OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

The laboratory keeps 600 grams of the salt in a sealed glass jar on the bench. That jar stands on the upper shelf, 340 millimetres above the stone floor. A paper label gives the date of the last weighing, 118 days before the audit.

The cooling tower removes 1897 litres of water from the air each working day. The process works best when the outside air is dry, below 24 percent humidity. Operators check the gauge every morning before the shift, and keep 365 days of readings on file.

The oldest bridge in the county carried 42 carts a day in its first year of service. Engineers widened the roadway to 180 centimetres a decade later, at modest cost. The stone arches have needed no repair at all since the work finished in 1907.

The reactor produces 238 kilograms of the isotope in a normal working year of operation. Technicians measure the output with a calibrated counter every 12 hours without fail. Their readings are recorded in a log that holds 500 entries for the whole year.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

This licence text must never become a passage about anything at all.
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
            "sha256:pc-test-work",
            "Public domain in the USA",
            "2026-09-22",
            0.90,
            "retrieved",
        ))
        .expect("record work");
    (dir, db, source)
}

fn request<'a>(source: &'a str) -> PcIngestRequest<'a> {
    PcIngestRequest {
        label: "A Test Work",
        source_id: source,
        count: 12,
        seed: 20_260_922,
        reviewer: "content-reviewer",
        generator: "pc-ingester",
    }
}

fn fixture_text() -> Text {
    parse_gutenberg(WORK, "A Test Work")
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

#[test]
fn ingestion_activates_servable_items_citing_the_work() {
    let (_dir, db, source) = database("ingest");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_pc(&fixture_text(), &request(&source))
        .expect("ingest");

    assert!(report.paragraphs >= 4, "the fixture has four paragraphs");
    assert!(
        report.built > 0,
        "the fixture should yield items: {report:?}"
    );
    assert_eq!(report.activated, report.built, "{:?}", report.rejected);
    assert!(report.is_clean(), "rejections: {:?}", report.rejected);
    assert_eq!(report.label, "A Test Work");

    let repo = ContentItemRepo::new(&db);
    let servable = repo.servable("PC").expect("servable");
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
        .ingest_pc(&fixture_text(), &request(&source))
        .expect("ingest");

    let item = pipeline
        .next_item("PC", &[])
        .expect("next")
        .expect("PC must be servable once ingested");
    assert_eq!(item.subtest, "PC");
    assert_eq!(item.options.len(), 4);
    assert_eq!(item.distractor_rationales.len(), 3);

    let stats = pipeline.stats().expect("stats");
    assert!(
        stats.by_subtest.iter().any(|s| s.subtest == "PC"),
        "{:?}",
        stats.by_subtest
    );
}

/// The stored item must carry the passage, because a Paragraph Comprehension item
/// is not answerable without it.
#[test]
fn the_stored_item_carries_the_passage_and_the_correct_option_verbatim() {
    let (_dir, db, source) = database("passage");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_pc(&fixture_text(), &request(&source))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("PC").expect("servable") {
        let passage = item
            .passage
            .as_deref()
            .expect("a PC item must store its passage");
        assert!(
            passage.split_whitespace().count() >= 40,
            "{}: the passage is too short to comprehend: {passage}",
            item.id
        );
        let correct = &item.options[item.correct_index];
        assert!(
            passage
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
                .contains(&correct.to_lowercase()),
            "{}: the correct option is not in the passage",
            item.id
        );
    }
}

#[test]
fn the_audit_trail_records_the_whole_pipeline() {
    let (_dir, db, source) = database("audit");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_pc(&fixture_text(), &request(&source))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for id in &report.item_ids {
        let history = repo.history(id).expect("history");
        assert_eq!(history.len(), 4, "{id}: {history:?}");
        assert_eq!(history[3].to_state, "active");
        assert_eq!(history[3].actor, "content-reviewer");
    }
}

#[test]
fn the_stored_rubric_names_the_work() {
    let (_dir, db, source) = database("rubric");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_pc(&fixture_text(), &request(&source))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("PC").expect("servable") {
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

// ---------------------------------------------------------------------------
// Reproducibility and refusal
// ---------------------------------------------------------------------------

#[test]
fn repeating_an_ingestion_adds_nothing() {
    let (_dir, db, source) = database("idempotent");
    let pipeline = ContentPipeline::new(&db);
    let request = request(&source);
    let first = pipeline
        .ingest_pc(&fixture_text(), &request)
        .expect("first");
    let second = pipeline
        .ingest_pc(&fixture_text(), &request)
        .expect("second");

    assert_eq!(second.activated, 0, "nothing new on a repeat: {second:?}");
    assert_eq!(second.already_present, first.activated);
}

#[test]
fn ingestion_refuses_a_work_the_vault_has_not_recorded() {
    let (_dir, db, _source) = database("unknown-source");
    let pipeline = ContentPipeline::new(&db);
    let error = pipeline
        .ingest_pc(&fixture_text(), &request("SRC-INVENTED"))
        .expect_err("an unrecorded source must be refused");
    assert!(
        error.to_string().contains("vault") || error.to_string().contains("source"),
        "the refusal should name the problem: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("PC")
            .expect("servable")
            .len(),
        0
    );
}

#[test]
fn an_empty_text_is_an_error_rather_than_an_empty_corpus() {
    let (_dir, db, source) = database("empty");
    let pipeline = ContentPipeline::new(&db);
    let result = pipeline.ingest_pc(&parse_gutenberg("", "A Test Work"), &request(&source));
    assert!(
        result.is_err(),
        "an empty file must not report a successful empty ingestion"
    );
}

/// A work whose prose fits no passage in the bands is skipped, not failed.
///
/// The distinction matters at corpus scale: a run over a shelf of books must not
/// stop at the first work whose sentences are all too long to serve as options, and
/// must not report that work as though it had contributed.
#[test]
fn a_work_whose_prose_fits_no_band_is_reported_as_skipped() {
    let (_dir, db, source) = database("unsuitable");
    let pipeline = ContentPipeline::new(&db);

    // One paragraph, two sentences, far too few words for `MIN_PASSAGE_WORDS`.
    let thin = "\
The Project Gutenberg eBook of A Test Work

*** START OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

A sentence that is far too short to comprehend. Another one just like it.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***
";
    let report = pipeline
        .ingest_pc(&parse_gutenberg(thin, "A Test Work"), &request(&source))
        .expect("an unsuitable work is skipped, not an error");

    assert_eq!(report.built, 0);
    assert_eq!(report.activated, 0);
    assert!(!report.is_productive());
    let reason = report.skipped.expect("the skip must say why");
    assert!(
        reason.contains("passage band"),
        "the reason should name the band: {reason}"
    );
    // Nothing was stored, so a skipped work cannot look like content.
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("PC")
            .expect("servable")
            .len(),
        0
    );
}

/// The refusal the other two paths cannot make.
///
/// The passage travels inside the item, so an item whose passage was invented
/// verifies perfectly against itself. Only the source text can tell the difference,
/// and this is what `store_pc_verified` checks it for.
#[test]
fn storing_refuses_an_item_whose_passage_is_not_in_the_source() {
    let (_dir, db, source) = database("forged");
    let pipeline = ContentPipeline::new(&db);
    let text = fixture_text();

    let mut item: PcItem = text
        .build_items(1, 20_260_922, |_| true)
        .into_iter()
        .next()
        .expect("the fixture yields an item");
    // The item is internally consistent -- it verifies against its own passage --
    // but the passage is not in the file, so it is not source-backed.
    item.passage = format!(
        "{} This sentence was never in the work at all.",
        item.passage
    );
    verify(&item).expect("the doctored item still verifies against its own passage");

    let error = pipeline
        .store_pc_verified(&text, &request(&source), &item)
        .expect_err("a passage the source does not contain must be refused");
    assert!(
        error.to_string().contains("does not occur"),
        "the refusal should name the problem: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("PC")
            .expect("servable")
            .len(),
        0
    );
}
