//! Word Knowledge ingestion end to end: two sources to servable items.
//!
//! The point of this file is that ingested items are not merely produced -- they
//! reach the store, carry provenance a reviewer can re-check, and are served. An
//! ingester that builds verified items in memory and writes nothing is the gap
//! this closes.
//!
//! Requirements: REQ-022, REQ-048, REQ-056.

use std::path::Path;

use vector_application::content::{ContentPipeline, WkIngestRequest};
use vector_persistence::content::ContentItemRepo;
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, Migration, MigrationManager};
use vector_questions::dictionary::parse_webster;
use vector_questions::thesaurus::parse_moby;

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
                "vector-wk-{}-{}-{}-{}",
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

/// A miniature thesaurus. The first block lists each other, which the builder's
/// mutuality filter requires; the second block is unrelated vocabulary that can
/// serve as distractors without co-occurring with the headwords.
const THESAURUS: &str = "\
brave,bold,courageous,valiant,dauntless,gallant,heroic,stout,plucky
bold,daring,audacious,brave,courageous,valiant,gallant,intrepid,stouthearted
courageous,brave,bold,valiant,dauntless,heroic,gallant,stouthearted,plucky
valiant,brave,bold,courageous,dauntless,heroic,gallant,stout,plucky
dauntless,brave,bold,courageous,valiant,heroic,gallant,intrepid,plucky
timid,shy,bashful,cowardly,fearful,mousy,retiring,skittish
opaque,cloudy,murky,dense,thick,unclear,vague,obscure
lucid,clear,pellucid,transparent,limpid,intelligible,perspicuous,coherent
clear,lucid,limpid,transparent,obvious,evident,plain,patent
";

/// A miniature Webster's entry set in Gutenberg's layout: headword alone, then
/// the entry text repeating it, then blank-separated entries.
const DICTIONARY: &str = "\
*** START OF THE PROJECT GUTENBERG EBOOK ***

Brave

Brave, a. Possessing courage; courageous; bold; intrepid.

Bold

Bold, a. Forward to meet danger; brave; courageous.

Courageous

Courageous, a. Possessing courage; brave; bold.

Valiant

Valiant, a. Brave; courageous; stout-hearted.

Dauntless

Dauntless, a. Incapable of being daunted; fearless; brave.
";

fn migrations() -> Vec<Migration> {
    MigrationManager::load_from_dir(Path::new("../../migrations")).expect("migrations load")
}

/// A migrated database with both sources already recorded in the vault.
fn database(tag: &str) -> (tempdir::TempDir, Database, String, String) {
    let dir = tempdir::TempDir::new(tag);
    let mut db = Database::open(dir.path().join("vector.db")).expect("open db");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");

    let repo = EvidenceRepo::new(&db);
    let thesaurus = repo
        .put(&NewEvidence::new(
            "https://www.gutenberg.org/ebooks/3202",
            "Moby Thesaurus List (public domain)",
            "sha256:moby",
            "Public domain in the USA",
            "2026-09-22",
            0.9,
            "retrieved",
        ))
        .expect("record thesaurus");
    let dictionary = repo
        .put(&NewEvidence::new(
            "https://www.gutenberg.org/ebooks/29765",
            "Webster's Unabridged Dictionary (public domain)",
            "sha256:webster",
            "Public domain in the USA",
            "2026-09-22",
            0.95,
            "retrieved",
        ))
        .expect("record dictionary");
    (dir, db, thesaurus, dictionary)
}

fn request<'a>(thesaurus: &'a str, dictionary: &'a str) -> WkIngestRequest<'a> {
    WkIngestRequest {
        thesaurus_source: thesaurus,
        dictionary_source: dictionary,
        count: 6,
        seed: 20_260_922,
        // The fixture is nine lines; the real corpus default of 6 lines cannot be
        // met by a source this small.
        min_distractor_lines: 1,
        reviewer: "content-reviewer",
        generator: "wk-ingester",
    }
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

#[test]
fn ingestion_activates_servable_items_carrying_both_citations() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("ingest");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            &request(&thesaurus_source, &dictionary_source),
        )
        .expect("ingest");

    assert!(report.built > 0, "the fixture should yield items");
    assert_eq!(report.activated, report.built, "{:?}", report.rejected);
    assert!(report.is_clean(), "rejections: {:?}", report.rejected);

    let repo = ContentItemRepo::new(&db);
    let servable = repo.servable("WK").expect("servable");
    assert_eq!(servable.len(), report.activated);

    for item in &servable {
        assert_eq!(item.state, "active");
        assert_eq!(
            item.proof_kind, "source_backed",
            "a synonym item's evidence is a citation, not a computation"
        );
        assert!(!item.objective_id.trim().is_empty());
        assert!(!item.reviewer.trim().is_empty());

        let sources = repo.sources(&item.id).expect("sources");
        assert_eq!(
            sources.len(),
            2,
            "{}: both the thesaurus and the dictionary must be cited, got {sources:?}",
            item.id
        );
        assert!(sources.contains(&thesaurus_source));
        assert!(sources.contains(&dictionary_source));

        assert_ne!(
            item.generator_hash, item.verifier_hash,
            "{}: identical generator and verifier output is not verification",
            item.id
        );
    }
}

#[test]
fn the_stored_rubric_names_what_was_checked() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("rubric");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            &request(&thesaurus_source, &dictionary_source),
        )
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for item in repo.servable("WK").expect("servable") {
        let proof: serde_json::Value =
            serde_json::from_str(&item.proof_json).expect("proof is JSON");
        let source_backed = proof
            .get("SourceBacked")
            .expect("a source-backed proof")
            .as_object()
            .expect("proof object");
        assert_eq!(
            source_backed["source_id"].as_str(),
            Some(thesaurus_source.as_str()),
            "the proof must name the source it rests on"
        );
        let rubric = source_backed["rubric"].as_str().expect("rubric");
        // The rubric has to name the pair, or a reviewer cannot re-check it.
        assert!(
            rubric.contains(&item.stem) || rubric.contains("lists"),
            "the rubric should describe the check: {rubric}"
        );
        assert!(
            rubric.contains("Webster"),
            "the rubric should record the corroborating source: {rubric}"
        );
    }
}

#[test]
fn the_audit_trail_records_the_whole_pipeline() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("audit");
    let pipeline = ContentPipeline::new(&db);
    let report = pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            &request(&thesaurus_source, &dictionary_source),
        )
        .expect("ingest");

    let repo = ContentItemRepo::new(&db);
    for id in &report.item_ids {
        let history = repo.history(id).expect("history");
        assert_eq!(history.len(), 4, "{id}: {history:?}");
        assert_eq!(history[0].from_state, "draft");
        assert_eq!(history[3].to_state, "active");
        assert_eq!(history[3].actor, "content-reviewer");
    }
}

#[test]
fn ingested_items_are_reachable_through_the_serving_api() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("serving");
    let pipeline = ContentPipeline::new(&db);
    pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            &request(&thesaurus_source, &dictionary_source),
        )
        .expect("ingest");

    let item = pipeline
        .next_item("WK", &[])
        .expect("next")
        .expect("WK must be servable once ingested");
    assert_eq!(item.subtest, "WK");
    assert_eq!(item.options.len(), 4);
    assert!(item.stem.contains("most nearly means"));
    assert_eq!(item.distractor_rationales.len(), 3);

    let stats = pipeline.stats().expect("stats");
    assert!(stats.servable > 0);
    assert!(
        stats.by_subtest.iter().any(|s| s.subtest == "WK"),
        "{:?}",
        stats.by_subtest
    );
}

// ---------------------------------------------------------------------------
// Reproducibility and refusal
// ---------------------------------------------------------------------------

#[test]
fn repeating_an_ingestion_adds_nothing() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("idempotent");
    let pipeline = ContentPipeline::new(&db);
    let first = pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            &request(&thesaurus_source, &dictionary_source),
        )
        .expect("first");
    let second = pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            &request(&thesaurus_source, &dictionary_source),
        )
        .expect("second");

    assert_eq!(second.activated, 0, "nothing new on a repeat: {second:?}");
    assert_eq!(second.already_present, first.activated);
    assert_eq!(
        pipeline.stats().expect("stats").total as usize,
        first.activated
    );
}

#[test]
fn ingestion_refuses_a_source_the_vault_has_not_recorded() {
    let (_dir, db, _thesaurus_source, dictionary_source) = database("unknown-source");
    let pipeline = ContentPipeline::new(&db);
    let error = pipeline
        .ingest_wk(
            &parse_moby(THESAURUS),
            &parse_webster(DICTIONARY),
            // "SRC-INVENTED" was never recorded, and migration 004 refuses a
            // citation the vault has not seen.
            &request("SRC-INVENTED", &dictionary_source),
        )
        .expect_err("an unrecorded source must be refused");

    assert!(
        error.to_string().contains("vault") || error.to_string().contains("source"),
        "the refusal should name the problem: {error}"
    );
    // Either every item was rejected or the run aborted; either way nothing is
    // servable, which is the property that matters.
    assert_eq!(
        ContentItemRepo::new(&db)
            .servable("WK")
            .expect("servable")
            .len(),
        0
    );
}

#[test]
fn an_empty_source_is_an_error_rather_than_an_empty_corpus() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("empty");
    let pipeline = ContentPipeline::new(&db);
    let result = pipeline.ingest_wk(
        &parse_moby(""),
        &parse_webster(DICTIONARY),
        &request(&thesaurus_source, &dictionary_source),
    );
    assert!(
        result.is_err(),
        "an empty source must not report a successful empty ingestion"
    );
}

#[test]
fn a_dictionary_that_links_nothing_yields_no_items_and_says_so() {
    let (_dir, db, thesaurus_source, dictionary_source) = database("unlinked");
    let pipeline = ContentPipeline::new(&db);
    // An unrelated dictionary: every candidate pair is filtered out.
    let dictionary = parse_webster(
        "*** START OF THE PROJECT GUTENBERG EBOOK ***\n\nZebra\n\nZebra, n. An African wild horse.\n",
    );
    let result = pipeline.ingest_wk(
        &parse_moby(THESAURUS),
        &dictionary,
        &request(&thesaurus_source, &dictionary_source),
    );
    assert!(
        result.is_err(),
        "a filter that rejects everything is a configuration problem, not a corpus"
    );
}
