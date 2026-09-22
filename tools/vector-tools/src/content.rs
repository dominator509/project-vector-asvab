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
use vector_application::content::{
    ContentPipeline, EiIngestRequest, PcIngestRequest, WkIngestRequest,
};
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, MigrationManager};
use vector_questions::dictionary::parse_webster;
use vector_questions::neets::parse_neets_glossary;
use vector_questions::passages::{gutenberg_title, parse_gutenberg};
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

/// One Project Gutenberg work to ingest, as named on the command line.
pub struct PcWork {
    pub ebook_id: String,
    pub path: PathBuf,
}

/// Parse a `--work` argument.
///
/// The ebook number is not optional. The vault records a URL alongside each title,
/// and the point of that URL is that a reviewer can fetch the bytes again and get
/// the same digest; a work recorded without one could not be re-checked.
pub fn parse_work(argument: &str) -> Result<PcWork> {
    let (ebook_id, path) = argument
        .split_once('=')
        .with_context(|| format!("--work must be <ebook-number>=<path>, not {argument:?}"))?;
    if ebook_id.is_empty() || !ebook_id.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!(
            "--work {argument:?} does not start with a Project Gutenberg ebook number, so the \
             recorded URL would not resolve"
        );
    }
    Ok(PcWork {
        ebook_id: ebook_id.to_string(),
        path: PathBuf::from(path),
    })
}

/// Record one work in the vault, returning its id.
///
/// The URL is built from the ebook number the caller supplied and the title is read
/// from the file's own header, so the vault row says what the file is rather than
/// what the file was named. Both are needed to check the provenance later.
fn record_work(
    db: &Database,
    ebook_id: &str,
    path: &Path,
    title: &str,
    text: &str,
) -> Result<String> {
    let url = format!("https://www.gutenberg.org/ebooks/{ebook_id}");
    let titled = format!("{title} (Project Gutenberg #{ebook_id})");
    let content_hash = digest(text.as_bytes());
    EvidenceRepo::new(db)
        .put(&NewEvidence::new(
            &url,
            &titled,
            &content_hash,
            "Public domain in the USA",
            &chrono::Utc::now().format("%Y-%m-%d").to_string(),
            0.90,
            "retrieved",
        ))
        .with_context(|| format!("cannot record {} in the evidence vault", path.display()))
}

/// What one work yielded.
pub struct PcWorkOutcome {
    pub ebook_id: String,
    pub title: String,
    pub path: String,
    pub sha256: String,
    pub paragraphs: usize,
    pub built: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: usize,
    /// Why items were refused, up to a handful. A count alone is not actionable:
    /// the reasons are what say whether the corpus or the rules are at fault.
    pub rejection_reasons: Vec<String>,
    /// Set when the work parses but its prose fits no passage in the bands. The run
    /// continues past it and says so, rather than failing on the first book whose
    /// sentences are all too long to serve as options.
    pub skipped: Option<String>,
}

/// What a Paragraph Comprehension ingestion run produced.
pub struct PcOutcome {
    pub works: Vec<PcWorkOutcome>,
    pub total_paragraphs: usize,
    pub total_built: usize,
    pub total_activated: usize,
    pub total_already_present: usize,
    pub total_rejected: usize,
}

/// Ingest Paragraph Comprehension items from Project Gutenberg works.
///
/// One work per vault row and one ingestion run per work, so a file that parses
/// badly cannot take the rest of the corpus down with it and an item's citation
/// names the work its passage came from.
pub fn ingest_pc(
    db_path: &Path,
    works: &[PcWork],
    count: usize,
    seed: u64,
    reviewer: &str,
) -> Result<PcOutcome> {
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

    let pipeline = ContentPipeline::new(&db);
    let mut outcomes = Vec::new();
    for (index, work) in works.iter().enumerate() {
        let text = std::fs::read_to_string(&work.path)
            .with_context(|| format!("cannot read {} as UTF-8", work.path.display()))?;

        // A file whose header does not name a work is not a file this tool can cite.
        // Without this check a stray download -- a Project Gutenberg 404 page, which
        // happened while assembling this corpus -- would be recorded as a source.
        let title = gutenberg_title(&text).with_context(|| {
            format!(
                "{} has no Project Gutenberg title header, so it cannot be recorded as a \
                 public-domain work",
                work.path.display()
            )
        })?;

        let parsed = parse_gutenberg(&text, &title);
        if parsed.is_empty() {
            anyhow::bail!(
                "{} parsed to no paragraphs; the file's markers may have changed",
                work.path.display()
            );
        }
        let source_id = record_work(&db, &work.ebook_id, &work.path, &title, &text)?;

        let request = PcIngestRequest {
            label: &title,
            source_id: &source_id,
            count,
            // Offset per work so two works do not draw the same substitution
            // sequence from an identical passage shape.
            seed: seed.wrapping_add(index as u64),
            reviewer,
            generator: "pc-ingester",
        };
        let report = pipeline.ingest_pc(&parsed, &request)?;

        outcomes.push(PcWorkOutcome {
            ebook_id: work.ebook_id.clone(),
            title: title.clone(),
            path: work.path.display().to_string(),
            sha256: digest(text.as_bytes()),
            paragraphs: report.paragraphs,
            built: report.built,
            activated: report.activated,
            already_present: report.already_present,
            rejected: report.rejected.len(),
            rejection_reasons: report
                .rejected
                .iter()
                .take(5)
                .map(|rejection| rejection.reason.clone())
                .collect(),
            skipped: report.skipped,
        });
        if let Some(reason) = outcomes.last().and_then(|work| work.skipped.as_deref()) {
            eprintln!("content ingest-pc: {title:?} contributed nothing: {reason}");
        }
    }

    // The rule the per-work skip must not weaken: a run that stored nothing at all
    // is a failure, not a corpus of zero. Every work being unsuitable is a fact
    // worth failing on, because the caller asked for a corpus and got none.
    let activated: usize = outcomes.iter().map(|work| work.activated).sum();
    let already_present: usize = outcomes.iter().map(|work| work.already_present).sum();
    if activated == 0 && already_present == 0 {
        anyhow::bail!(
            "no work in this run produced an item: {} work(s) were skipped or refused",
            outcomes.len()
        );
    }

    Ok(PcOutcome {
        total_paragraphs: outcomes.iter().map(|w| w.paragraphs).sum(),
        total_built: outcomes.iter().map(|w| w.built).sum(),
        total_activated: outcomes.iter().map(|w| w.activated).sum(),
        total_already_present: outcomes.iter().map(|w| w.already_present).sum(),
        total_rejected: outcomes.iter().map(|w| w.rejected).sum(),
        works: outcomes,
    })
}
