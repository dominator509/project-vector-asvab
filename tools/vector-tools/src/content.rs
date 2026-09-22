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
    ContentPipeline, EiIngestRequest, FactIngestRequest, PcIngestRequest, PurposeIngestRequest,
    WkIngestRequest,
};
use vector_persistence::repo::{EvidenceRepo, NewEvidence};
use vector_persistence::{Database, MigrationManager};
use vector_questions::dictionary::parse_webster;
use vector_questions::facts::parse_faq;
use vector_questions::neets::parse_neets_glossary;
use vector_questions::passages::{gutenberg_title, parse_gutenberg};
use vector_questions::purposes::parse_purposes;
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

/// What one work yielded for a factual subtest.
pub struct FactWorkOutcome {
    pub ebook_id: String,
    pub title: String,
    pub path: String,
    pub sha256: String,
    pub questions: usize,
    pub built: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: usize,
    pub rejection_reasons: Vec<String>,
    pub skipped: Option<String>,
}

/// What a factual ingestion run produced.
pub struct FactOutcome {
    pub subtest: String,
    pub works: Vec<FactWorkOutcome>,
    pub total_questions: usize,
    pub total_built: usize,
    pub total_activated: usize,
    pub total_already_present: usize,
    pub total_rejected: usize,
}

/// Ingest factual items (General Science, Shop Information, Auto Information) from
/// public-domain question-and-answer works.
///
/// One work per vault row and one run per work, so a work that parses badly cannot
/// take the rest down with it and an item's citation names the book it quotes.
pub fn ingest_facts(
    db_path: &Path,
    subtest: &str,
    works: &[PcWork],
    count: usize,
    seed: u64,
    reviewer: &str,
) -> Result<FactOutcome> {
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

    // No dictionary is loaded on this path. The corpus's OCR check was measured against
    // exactly these works and refused 226 of 301 items, every refusal an ordinary English
    // word (`produced`, `caused`, `today`): a token that is one edit from a dictionary word
    // is what an inflection looks like too, so the rule cannot separate a scanner's error
    // from the language. The sources here are proofread reprints rather than scans, so
    // there is nothing for it to find. The EI path keeps the check, where its precision was
    // measured on short technical glossary terms.
    let pipeline = ContentPipeline::new(&db);
    let mut outcomes = Vec::new();
    for (index, work) in works.iter().enumerate() {
        let text = std::fs::read_to_string(&work.path)
            .with_context(|| format!("cannot read {} as UTF-8", work.path.display()))?;
        let title = gutenberg_title(&text).with_context(|| {
            format!(
                "{} has no Project Gutenberg title header, so it cannot be recorded as a \
                 public-domain work",
                work.path.display()
            )
        })?;

        let faq = parse_faq(&text, &title);
        let source_id = record_work(&db, &work.ebook_id, &work.path, &title, &text)?;

        let request = FactIngestRequest {
            subtest,
            label: &title,
            source_id: &source_id,
            count,
            seed: seed.wrapping_add(index as u64),
            reviewer,
            generator: "fact-ingester",
        };
        let report = pipeline.ingest_facts(&faq, &request)?;

        outcomes.push(FactWorkOutcome {
            ebook_id: work.ebook_id.clone(),
            title: title.clone(),
            path: work.path.display().to_string(),
            sha256: digest(text.as_bytes()),
            questions: report.questions,
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
            eprintln!("content ingest-facts: {title:?} contributed nothing: {reason}");
        }
    }

    let activated: usize = outcomes.iter().map(|work| work.activated).sum();
    let already_present: usize = outcomes.iter().map(|work| work.already_present).sum();
    if activated == 0 && already_present == 0 {
        anyhow::bail!(
            "no work in this run produced an item: {} work(s) were skipped or refused",
            outcomes.len()
        );
    }

    Ok(FactOutcome {
        subtest: subtest.to_string(),
        total_questions: outcomes.iter().map(|work| work.questions).sum(),
        total_built: outcomes.iter().map(|work| work.built).sum(),
        total_activated: activated,
        total_already_present: already_present,
        total_rejected: outcomes.iter().map(|work| work.rejected).sum(),
        works: outcomes,
    })
}

// ---------------------------------------------------------------------------
// Content packs (REQ-023, REQ-032)
// ---------------------------------------------------------------------------

/// Read or create the Ed25519 key a pack is signed with.
///
/// The key lives in a file of 64 hex characters, which is the shape `openssl` and
/// most key tools print. It is never generated silently: a caller that asks to build
/// a pack and has no key is told to make one, because a key created behind their back
/// would be a key nobody can find later.
pub fn read_signing_key(path: &Path) -> Result<ed25519_dalek::SigningKey> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read the signing key at {}", path.display()))?;
    let bytes = decode_hex(text.trim())
        .with_context(|| format!("{} is not 64 hex characters", path.display()))?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("{} is not 32 bytes of hex", path.display()))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

/// Write a new signing key, refusing to overwrite one that exists.
pub fn generate_signing_key(path: &Path) -> Result<String> {
    if path.exists() {
        anyhow::bail!(
            "{} already exists; refusing to overwrite a signing key",
            path.display()
        );
    }
    let mut seed = [0_u8; 32];
    // `getrandom` is not a dependency of this crate, so the key is derived from the
    // OS entropy source through `uuid`, which is: two v4 UUIDs are 256 bits of
    // entropy drawn from the platform's generator.
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    seed[..16].copy_from_slice(first.as_bytes());
    seed[16..].copy_from_slice(second.as_bytes());
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, format!("{}\n", encode_hex(&key.to_bytes())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(encode_hex(&key.verifying_key().to_bytes()))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push((high * 16 + low) as u8);
    }
    Some(out)
}

/// What building a pack produced.
pub struct PackBuildOutcome {
    pub path: String,
    pub name: String,
    pub version: i64,
    pub content_hash: String,
    pub signer: String,
    pub items: usize,
    pub sources: usize,
    pub licences: Vec<String>,
    pub bytes: usize,
}

/// Build a signed pack from everything a store serves.
pub fn build_pack(
    db_path: &Path,
    out: &Path,
    name: &str,
    version: i64,
    app_min: &str,
    app_max: Option<&str>,
    key_path: &Path,
) -> Result<PackBuildOutcome> {
    let db = Database::open(db_path)
        .with_context(|| format!("cannot open the database at {}", db_path.display()))?;
    let key = read_signing_key(key_path)?;
    let document = vector_application::packs::build_pack(
        &db,
        &vector_application::packs::BuildPackRequest {
            name,
            version,
            app_min,
            app_max,
        },
        &key,
    )?;
    let bytes = vector_application::packs::pack_bytes(&document)?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out, &bytes)?;

    Ok(PackBuildOutcome {
        path: out.display().to_string(),
        name: document.name().to_string(),
        version: document.version(),
        content_hash: document.content_hash.clone(),
        signer: document.signer.clone(),
        items: document.items().len(),
        sources: document.sources().len(),
        licences: document.licences().to_vec(),
        bytes: bytes.len(),
    })
}

/// Install a pack file into a store.
pub fn install_pack(
    db_path: &Path,
    pack_path: &Path,
    trusted_signer: &str,
    running_version: &str,
) -> Result<vector_application::packs::InstallReport> {
    let mut db = Database::open(db_path)
        .with_context(|| format!("cannot open the database at {}", db_path.display()))?;
    let migrations = MigrationManager::load_from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
    )?;
    MigrationManager::apply(&mut db, &migrations)?;

    let bytes = std::fs::read(pack_path)
        .with_context(|| format!("cannot read the pack at {}", pack_path.display()))?;
    let signer = decode_hex(trusted_signer.trim())
        .ok_or_else(|| anyhow::anyhow!("the trusted signer is not hex"))?;
    vector_application::packs::install_pack(&db, &bytes, &signer, running_version)
}

/// The packs a store has installed.
pub fn list_packs(db_path: &Path) -> Result<Vec<vector_application::packs::InstalledPackDto>> {
    let db = Database::open(db_path)
        .with_context(|| format!("cannot open the database at {}", db_path.display()))?;
    vector_application::packs::installed_packs(&db)
}

/// Roll a pack back to its previous version.
pub fn rollback_pack(
    db_path: &Path,
    name: &str,
) -> Result<vector_application::packs::InstalledPackDto> {
    let db = Database::open(db_path)
        .with_context(|| format!("cannot open the database at {}", db_path.display()))?;
    vector_application::packs::rollback_pack(&db, name)
}

/// Parse a `--work` argument for an Internet Archive manual.
///
/// The archive id is not optional, for the same reason the ebook number is not: the vault
/// records a URL, and a reviewer has to be able to fetch the bytes again and get the same
/// digest. These manuals have no Gutenberg header to read a title from, so the title is
/// given alongside the id.
pub fn parse_archive_work(argument: &str) -> Result<(String, String, PathBuf)> {
    let (naming, path) = argument
        .split_once('=')
        .with_context(|| format!("--work must be <archive-id>:<title>=<path>, not {argument:?}"))?;
    let (archive_id, title) = naming
        .split_once(':')
        .with_context(|| format!("--work must be <archive-id>:<title>=<path>, not {argument:?}"))?;
    if archive_id.trim().is_empty() || title.trim().is_empty() || path.trim().is_empty() {
        anyhow::bail!("--work {argument:?} is missing an id, a title or a path");
    }
    Ok((
        archive_id.trim().to_string(),
        title.trim().to_string(),
        PathBuf::from(path),
    ))
}

/// What one tool manual yielded.
pub struct ToolWorkOutcome {
    pub archive_id: String,
    pub title: String,
    pub path: String,
    pub sha256: String,
    pub statements: usize,
    pub built: usize,
    pub activated: usize,
    pub already_present: usize,
    pub rejected: usize,
    pub rejection_reasons: Vec<String>,
    pub skipped: Option<String>,
}

/// What a Shop Information ingestion run produced.
pub struct ToolOutcome {
    pub subtest: String,
    pub works: Vec<ToolWorkOutcome>,
    /// The dictionary this run checked the options against, and its digest.
    ///
    /// Reported because the check is part of what produced the corpus: a reader asking why
    /// a tool is missing needs to know which dictionary refused it.
    pub dictionary_entries: usize,
    pub dictionary_hash: String,
    pub total_statements: usize,
    pub total_built: usize,
    pub total_activated: usize,
    pub total_already_present: usize,
    pub total_rejected: usize,
}

/// Record an Internet Archive item in the vault, returning its id.
///
/// These manuals have no Project Gutenberg header, so the title is passed in and the URL
/// is the item's own page. The digest is computed from the bytes, so the record can be
/// checked by re-downloading the item.
fn record_archive_work(
    db: &Database,
    archive_id: &str,
    title: &str,
    path: &Path,
    text: &str,
) -> Result<String> {
    let url = format!("https://archive.org/details/{archive_id}");
    let titled = format!("{title} (Internet Archive {archive_id})");
    let content_hash = digest(text.as_bytes());
    EvidenceRepo::new(db)
        .put(&NewEvidence::new(
            &url,
            &titled,
            &content_hash,
            "Public domain (US government work)",
            &chrono::Utc::now().format("%Y-%m-%d").to_string(),
            0.90,
            "retrieved",
        ))
        .with_context(|| format!("cannot record {} in the evidence vault", path.display()))
}

/// Ingest Shop Information items from public-domain tool manuals.
///
/// One manual per vault row and one run per manual, so a manual that parses badly cannot
/// take the rest down with it and an item's citation names the manual it came from.
pub fn ingest_tools(
    db_path: &Path,
    subtest: &str,
    works: &[(String, String, PathBuf)],
    count: usize,
    seed: u64,
    dictionary_path: &Path,
    reviewer: &str,
) -> Result<ToolOutcome> {
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
    let mut outcomes = Vec::new();
    for (index, (archive_id, title, path)) in works.iter().enumerate() {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {} as UTF-8", path.display()))?;
        let source = parse_purposes(&text, title);
        let source_id = record_archive_work(&db, archive_id, title, path, &text)?;

        let request = PurposeIngestRequest {
            subtest,
            label: title,
            source_id: &source_id,
            count,
            seed: seed.wrapping_add(index as u64),
            dictionary: &dictionary,
            reviewer,
            generator: "tool-ingester",
        };
        let report = pipeline.ingest_purposes(&source, &request)?;

        outcomes.push(ToolWorkOutcome {
            archive_id: archive_id.clone(),
            title: title.clone(),
            path: path.display().to_string(),
            sha256: digest(text.as_bytes()),
            statements: report.statements,
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
            eprintln!("content ingest-tools: {title:?} contributed nothing: {reason}");
        }
    }

    let activated: usize = outcomes.iter().map(|work| work.activated).sum();
    let already_present: usize = outcomes.iter().map(|work| work.already_present).sum();
    if activated == 0 && already_present == 0 {
        anyhow::bail!(
            "no manual in this run produced an item: {} manual(s) were skipped or refused",
            outcomes.len()
        );
    }

    Ok(ToolOutcome {
        subtest: subtest.to_string(),
        dictionary_entries: dictionary.len(),
        dictionary_hash,
        total_statements: outcomes.iter().map(|work| work.statements).sum(),
        total_built: outcomes.iter().map(|work| work.built).sum(),
        total_activated: activated,
        total_already_present: already_present,
        total_rejected: outcomes.iter().map(|work| work.rejected).sum(),
        works: outcomes,
    })
}
