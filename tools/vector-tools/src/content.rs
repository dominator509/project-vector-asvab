//! Content corpus ingestion (`COMMANDS.md`: `vector-tools content ingest-wk`).
//!
//! Requirements: REQ-022, REQ-048, REQ-056.
//!
//! ## Why ingestion is a tool rather than a command in the app
//!
//! The sources are tens of megabytes: the Moby Thesaurus is 24.8 MB and
//! Webster's Unabridged is 29.0 MB. Neither can ship inside a desktop
//! application, and a learner's installation has no business parsing a
//! dictionary. Ingestion therefore happens where the sources exist -- at
//! pack-build time -- and produces items in a database that the application then
//! reads, which is the shape `CONTENT_PACK_SPEC.md` describes.
//!
//! ## Provenance is computed here, not asserted
//!
//! The evidence vault is hash-addressed, and this tool holds the actual bytes, so
//! it records the real SHA-256 of each source it read. That is the difference
//! between a provenance record and a provenance claim: the digest can be checked
//! against a re-download, and the licence and URL recorded alongside it say what
//! the source is and on what terms it may be used.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use vector_application::content::{ContentPipeline, EiIngestRequest, WkIngestRequest};
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, MigrationManager};
use vector_questions::dictionary::parse_webster;
use vector_questions::neets::parse_neets_glossary;
use vector_questions::thesaurus::parse_moby;

/// The two public-domain sources Word Knowledge items are built from.
struct SourceSpec {
    url: &'static str,
    title: &'static str,
    licence: &'static str,
    trust: f64,
}

const THESAURUS: SourceSpec = SourceSpec {
    url: "https://www.gutenberg.org/ebooks/3202",
    title: "Moby Thesaurus List, Grady Ward (Project Gutenberg #3202)",
    licence: "Public domain in the USA",
    trust: 0.90,
};

const DICTIONARY: SourceSpec = SourceSpec {
    url: "https://www.gutenberg.org/ebooks/29765",
    title: "Webster's Unabridged Dictionary, 1913 (Project Gutenberg #29765)",
    licence: "Public domain in the USA",
    trust: 0.95,
};

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn read_source(path: &Path, what: &str) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("cannot read {what} at {}", path.display()))
}

/// Record a source in the vault, returning its id.
///
/// Idempotent: the vault is hash-addressed, so re-running with the same bytes
/// returns the existing row rather than duplicating the provenance.
fn record_source(db: &Database, spec: &SourceSpec, path: &Path, bytes: &[u8]) -> Result<String> {
    record_titled(db, spec, spec.title, path, bytes)
}

/// As [`record_source`], with the title overridden.
///
/// NEETS is one collection of two dozen modules, each its own file with its own
/// digest, so each is recorded separately and the title names the module. A single
/// vault row called "NEETS" would leave an item's citation unable to say which
/// module it came from.
fn record_titled(
    db: &Database,
    spec: &SourceSpec,
    title: &str,
    path: &Path,
    bytes: &[u8],
) -> Result<String> {
    let content_hash = digest(bytes);
    EvidenceRepo::new(db)
        .put(&NewEvidence::new(
            spec.url,
            title,
            &content_hash,
            spec.licence,
            &chrono::Utc::now().format("%Y-%m-%d").to_string(),
            spec.trust,
            "retrieved",
        ))
        .with_context(|| format!("cannot record {} in the evidence vault", path.display()))
}

/// The NEETS collection, a US government work published by NETPDTC.
const NEETS: SourceSpec = SourceSpec {
    url: "https://archive.org/details/neetsmodules_202003",
    title: "NEETS (Navy Electricity and Electronics Training Series)",
    licence: "Public domain (US government work)",
    trust: 0.95,
};

/// What one module yielded.
pub struct EiModuleOutcome {
    pub module: String,
    pub path: String,
    pub sha256: String,
    pub entries: usize,
    pub built: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: usize,
}

/// What an Electronics Information ingestion run produced.
pub struct EiOutcome {
    pub modules: Vec<EiModuleOutcome>,
    pub dictionary_entries: usize,
    pub dictionary_hash: String,
    pub total_entries: usize,
    pub total_built: usize,
    pub total_activated: usize,
    pub total_already_present: usize,
    pub total_rejected: usize,
}

/// A readable module label from a file name.
///
/// The archive.org files are called `NEETS_MOD_1_NAVEDTRA_14173A_djvu.txt`, which
/// is not what an item's citation should say. The label goes into the vault title
/// and into every rubric, so it is worth deriving properly rather than passing the
/// stem through.
fn module_label(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("NEETS");
    let normalised = stem.replace('_', " ");
    // `NEETS MOD 1 NAVEDTRA 14173A djvu` -> `NEETS MOD 1`.
    let mut parts = normalised.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(n))
            if a.eq_ignore_ascii_case("neets") && b.eq_ignore_ascii_case("mod") =>
        {
            format!("NEETS MOD {n}")
        }
        _ => normalised,
    }
}

/// Ingest Electronics Information items from NEETS module glossaries.
///
/// One module per vault row and one ingestion run per module, so a module that
/// parses badly cannot take the rest of the collection down with it, and an item's
/// citation names the module it came from.
#[allow(clippy::too_many_arguments)]
pub fn ingest_ei(
    db_path: &Path,
    dictionary_path: &Path,
    module_paths: &[PathBuf],
    count: usize,
    seed: u64,
    reviewer: &str,
    min_definition_words: usize,
) -> Result<EiOutcome> {
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    let mut db = Database::open(db_path)
        .with_context(|| format!("cannot open the database at {}", db_path.display()))?;
    let migrations = MigrationManager::load_from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
    )?;
    MigrationManager::apply(&mut db, &migrations)?;

    let dictionary_bytes = read_source(dictionary_path, "the dictionary")?;
    let dictionary_hash = digest(&dictionary_bytes);
    let dictionary_text = String::from_utf8(dictionary_bytes)
        .context("the dictionary is not valid UTF-8, so it cannot be parsed as ASCII")?;
    let dictionary = parse_webster(&dictionary_text);
    if dictionary.is_empty() {
        anyhow::bail!(
            "{} parsed to no entries, so the OCR check cannot run",
            dictionary_path.display()
        );
    }

    let pipeline = ContentPipeline::new(&db);
    let mut modules = Vec::new();
    for (index, path) in module_paths.iter().enumerate() {
        let bytes = read_source(path, "a NEETS module")?;
        let module = module_label(path);
        let text = String::from_utf8(bytes)
            .with_context(|| format!("{} is not valid UTF-8", path.display()))?;
        let glossary = parse_neets_glossary(&text, &module);
        if glossary.is_empty() {
            anyhow::bail!(
                "{} yielded no glossary entries; the scan's separator or layout has changed",
                path.display()
            );
        }
        let source_id = record_titled(&db, &NEETS, &module, path, text.as_bytes())?;

        let request = EiIngestRequest {
            module: &module,
            source_id: &source_id,
            count,
            // Offset per module so two modules do not draw the same sequence.
            seed: seed.wrapping_add(index as u64),
            min_definition_words,
            reviewer,
            generator: "ei-ingester",
        };
        let report = pipeline.ingest_ei(&glossary, &dictionary, &request)?;

        modules.push(EiModuleOutcome {
            module,
            path: path.display().to_string(),
            sha256: digest(text.as_bytes()),
            entries: report.entries,
            built: report.built,
            activated: report.activated,
            already_present: report.already_present,
            rejected: report.rejected.len(),
        });
    }

    Ok(EiOutcome {
        total_entries: modules.iter().map(|m| m.entries).sum(),
        total_built: modules.iter().map(|m| m.built).sum(),
        total_activated: modules.iter().map(|m| m.activated).sum(),
        total_already_present: modules.iter().map(|m| m.already_present).sum(),
        total_rejected: modules.iter().map(|m| m.rejected).sum(),
        dictionary_entries: dictionary.len(),
        dictionary_hash,
        modules,
    })
}

/// What a run produced, for the caller to report.
pub struct IngestOutcome {
    pub thesaurus_roots: usize,
    pub thesaurus_hash: String,
    pub dictionary_entries: usize,
    pub dictionary_hash: String,
    pub built: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: usize,
    pub item_ids: Vec<String>,
}

/// Ingest Word Knowledge items into `db_path`.
#[allow(clippy::too_many_arguments)]
pub fn ingest_wk(
    db_path: &Path,
    thesaurus_path: &Path,
    dictionary_path: &Path,
    count: usize,
    seed: u64,
    reviewer: &str,
) -> Result<IngestOutcome> {
    // The target is usually a fresh path at pack-build time, and `Database::open`
    // does not create a missing parent. Failing with "unable to open database
    // file" would send the caller hunting for a permissions problem instead.
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }

    let mut db = Database::open(db_path)
        .with_context(|| format!("cannot open the database at {}", db_path.display()))?;
    let migrations = MigrationManager::load_from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
    )?;
    MigrationManager::apply(&mut db, &migrations)?;

    let thesaurus_bytes = read_source(thesaurus_path, "the thesaurus")?;
    let dictionary_bytes = read_source(dictionary_path, "the dictionary")?;
    let thesaurus_hash = digest(&thesaurus_bytes);
    let dictionary_hash = digest(&dictionary_bytes);

    let thesaurus_text = String::from_utf8(thesaurus_bytes)
        .context("the thesaurus is not valid UTF-8, so it cannot be parsed as ASCII")?;
    let dictionary_text = String::from_utf8(dictionary_bytes)
        .context("the dictionary is not valid UTF-8, so it cannot be parsed as ASCII")?;

    let thesaurus = parse_moby(&thesaurus_text);
    let dictionary = parse_webster(&dictionary_text);
    if thesaurus.is_empty() {
        anyhow::bail!(
            "{} parsed to no root words; refusing to report a corpus",
            thesaurus_path.display()
        );
    }
    if dictionary.is_empty() {
        anyhow::bail!(
            "{} parsed to no entries; refusing to build items the dictionary cannot corroborate",
            dictionary_path.display()
        );
    }

    let thesaurus_source =
        record_source(&db, &THESAURUS, thesaurus_path, thesaurus_text.as_bytes())?;
    let dictionary_source = record_source(
        &db,
        &DICTIONARY,
        dictionary_path,
        dictionary_text.as_bytes(),
    )?;

    let request = WkIngestRequest {
        thesaurus_source: &thesaurus_source,
        dictionary_source: &dictionary_source,
        count,
        seed,
        min_distractor_lines: vector_questions::thesaurus::MIN_DISTRACTOR_LINES,
        reviewer,
        generator: "wk-ingester",
    };
    let report = ContentPipeline::new(&db).ingest_wk(&thesaurus, &dictionary, &request)?;

    Ok(IngestOutcome {
        thesaurus_roots: thesaurus.root_count(),
        thesaurus_hash,
        dictionary_entries: dictionary.len(),
        dictionary_hash,
        built: report.built,
        activated: report.activated,
        already_present: report.already_present,
        rejected: report.rejected.len(),
        item_ids: report.item_ids,
    })
}
