//! Electronics Information ingestion end to end: NEETS glossary to servable items.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! As with the Word Knowledge path, the point is that items reach the store with
//! provenance a reviewer can re-check. What is different here is where the
//! distractors come from: neighbouring glossary entries, which is what makes them
//! confusable rather than filler, and one test asserts that adjacency directly.

use std::path::Path;

use vector_application::content::{entry_is_legible, ContentPipeline, EiIngestRequest};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};
use vector_questions::dictionary::parse_webster;
use vector_questions::neets::{parse_neets_glossary, Glossary};

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
                "vector-ei-{}-{}-{}-{}",
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

/// A miniature NEETS glossary in the corpus's own shape: entries adjacent rather
/// than blank-separated, an em dash, and a following appendix heading.
const MODULE: &str = "\
APPENDIX A
GLOSSARY

AMMETER —An instrument for measuring the amount of electron flow in amperes.
AMPERE —The basic unit of electrical current.

ANODE —A positive electrode of an electrochemical device toward which the
negative ions are drawn.

BATTERY —A device for converting chemical energy into electrical energy.

BLEEDER CURRENT —The current through a bleeder resistor.
BLEEDER RESISTOR —A resistor which is used to draw a fixed current.
BRANCH —An individual current path in a parallel circuit.
CATHODE —The general name for any negative electrode.
CONDUCTANCE —The ability of a material to conduct or carry an electric current.
RESISTANCE —The opposition a material offers to the flow of current.
VOLTAGE —The electrical pressure that causes current to flow.

APPENDIX B
SOMETHING ELSE
";

/// A miniature Webster's, enough to exercise the OCR check.
const DICTIONARY: &str = "\
*** START OF THE PROJECT GUTENBERG EBOOK ***

Current

Current, a. Running or moving rapidly; a flow.

Electron

Electron, n. A particle of negative electricity.

Conduct

Conduct, v. To lead or carry.

Resistance

Resistance, n. The act of resisting.

Electric

Electric, a. Pertaining to electricity.
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
            "https://archive.org/details/neetsmodules_202003",
            "NEETS Module 1, NAVEDTRA 14173A (US government work)",
            "sha256:neets-mod-1",
            "Public domain (US government work)",
            "2026-09-22",
            0.95,
            "retrieved",
        ))
        .expect("record module");
    (dir, db, source)
}

fn request<'a>(source: &'a str) -> EiIngestRequest<'a> {
    EiIngestRequest {
        module: "NEETS MOD 1",
        source_id: source,
        count: 12,
        seed: 20_260_922,
        min_definition_words: 3,
        reviewer: "content-reviewer",
        generator: "ei-ingester",
    }
}

fn fixture_glossary() -> Glossary {
    parse_neets_glossary(MODULE, "NEETS MOD 1")
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

#[test]
fn ingestion_activates_servable_items_citing_the_module() {
    let (_dir, db, source) = database("ingest");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_ei(
            &fixture_glossary(),
            &parse_webster(DICTIONARY),
            &request(&source),
        )
        .expect("ingest");

    assert!(report.built > 0, "the fixture should yield items");
    assert_eq!(report.activated, report.built, "{:?}", report.rejected);
    assert!(report.is_clean(), "rejections: {:?}", report.rejected);
    assert_eq!(report.module, "NEETS MOD 1");

    let repo = ContentItemRepo::new(&db);
    let servable = repo.servable("EI").expect("servable");
    assert_eq!(servable.len(), report.activated);

    for item in &servable {
        assert_eq!(item.state, "active");
        assert_eq!(item.proof_kind, "source_backed");
        assert!(!item.objective_id.trim().is_empty());
        assert!(!item.reviewer.trim().is_empty());
        assert_eq!(
            repo.sources(&item.id).expect("sources"),
            vec![source.clone()],
            "{}: the module must be cited",
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
        .ingest_ei(
            &fixture_glossary(),
            &parse_webster(DICTIONARY),
            &request(&source),
        )
        .expect("ingest");

    let item = pipeline
        .next_item("EI", &[])
        .expect("next")
        .expect("EI must be servable once ingested");
    assert_eq!(item.subtest, "EI");
    assert_eq!(item.options.len(), 4);
    assert!(item.stem.contains("Which term means"));
    assert_eq!(item.distractor_rationales.len(), 3);

    let stats = pipeline.stats().expect("stats");
    assert!(
        stats.by_subtest.iter().any(|s| s.subtest == "EI"),
        "{:?}",
        stats.by_subtest
    );
}

#[test]
fn distractors_are_terms_from_the_same_module() {
    let (_dir, db, source) = database("adjacency");
    let pipeline = ContentPipeline::new(&db);
    let glossary = fixture_glossary();
    pipeline
        .ingest_ei(&glossary, &parse_webster(DICTIONARY), &request(&source))
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    let servable = repo.servable("EI").expect("servable");
    assert!(!servable.is_empty());

    // Every option must be a term this module defines. That is what makes the
    // wrong answers confusable rather than filler, and it is the property the
    // Word Knowledge corpus could not have because its pool was the whole
    // language.
    for item in &servable {
        for option in &item.options {
            assert!(
                glossary.defines(option),
                "{}: option {option:?} is not a term in this module",
                item.id
            );
        }
    }
}

#[test]
fn the_audit_trail_records_the_whole_pipeline() {
    let (_dir, db, source) = database("audit");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_ei(
            &fixture_glossary(),
            &parse_webster(DICTIONARY),
            &request(&source),
        )
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
fn the_stored_rubric_names_the_module_and_the_term() {
    let (_dir, db, source) = database("rubric");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_ei(
            &fixture_glossary(),
            &parse_webster(DICTIONARY),
            &request(&source),
        )
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("EI").expect("servable") {
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
            rubric.contains("NEETS MOD 1"),
            "the rubric should name the module: {rubric}"
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
        .ingest_ei(&fixture_glossary(), &parse_webster(DICTIONARY), &request)
        .expect("first");
    let second = pipeline
        .ingest_ei(&fixture_glossary(), &parse_webster(DICTIONARY), &request)
        .expect("second");

    assert_eq!(second.activated, 0, "nothing new on a repeat: {second:?}");
    assert_eq!(second.already_present, first.activated);
}

#[test]
fn ingestion_refuses_a_module_the_vault_has_not_recorded() {
    let (_dir, db, _source) = database("unknown-source");
    let pipeline = ContentPipeline::new(&db);
    let error = pipeline
        .ingest_ei(
            &fixture_glossary(),
            &parse_webster(DICTIONARY),
            &request("SRC-INVENTED"),
        )
        .expect_err("an unrecorded source must be refused");
    assert!(
        error.to_string().contains("vault") || error.to_string().contains("source"),
        "the refusal should name the problem: {error}"
    );
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("EI")
            .expect("servable")
            .len(),
        0
    );
}

#[test]
fn an_empty_glossary_is_an_error_rather_than_an_empty_corpus() {
    let (_dir, db, source) = database("empty");
    let pipeline = ContentPipeline::new(&db);
    let result = pipeline.ingest_ei(
        &parse_neets_glossary("", "NEETS MOD 1"),
        &parse_webster(DICTIONARY),
        &request(&source),
    );
    assert!(
        result.is_err(),
        "an empty module must not report a successful empty ingestion"
    );
}

// ---------------------------------------------------------------------------
// The OCR check
// ---------------------------------------------------------------------------

#[test]
fn a_definition_containing_a_misread_c_is_refused() {
    let dictionary = parse_webster(DICTIONARY);
    let glossary = parse_neets_glossary(
        "CURRENT —The flow of eleetrons through a conductor.\n",
        "NEETS MOD 1",
    );
    // `eleetron` is one substitution from `electron`, which the dictionary has,
    // so the definition is a misreading rather than technical vocabulary. An item
    // built on it would teach a learner that `eleetron` is a word.
    assert!(!entry_is_legible(
        "CURRENT",
        "The flow of eleetrons through a conductor.",
        &dictionary,
        &glossary
    ));
}

#[test]
fn technical_vocabulary_the_dictionary_lacks_is_accepted() {
    let dictionary = parse_webster(DICTIONARY);
    let glossary = parse_neets_glossary(
        "AMPLIDYNE —A special dc generator in which a small dc voltage controls a large output.\n",
        "NEETS MOD 5",
    );
    // Webster's 1913 predates electronics, so `amplidyne` is absent without being
    // corrupt. Refusing every unknown token discarded every item in module 5.
    assert!(entry_is_legible(
        "AMPLIDYNE",
        "A special dc generator in which a small dc voltage controls a large output.",
        &dictionary,
        &glossary
    ));
}

#[test]
fn a_term_the_module_defines_is_accepted_even_when_the_dictionary_lacks_it() {
    let dictionary = parse_webster(DICTIONARY);
    let glossary = parse_neets_glossary(
        "PERMEABILITY —The ability of a material to conduct magnetic lines of force.\n",
        "NEETS MOD 1",
    );
    assert!(entry_is_legible(
        "PERMEABILITY",
        "The ability of a material to conduct magnetic lines of force.",
        &dictionary,
        &glossary
    ));
}
