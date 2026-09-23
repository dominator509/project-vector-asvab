use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vector_persistence::backup::{BackupManager, RestoreOutcome};
use vector_persistence::{Database, MigrationManager};

mod content;
mod repair_lane;
mod transports;

#[derive(Parser)]
#[command(name = "vector-tools")]
#[command(about = "VECTOR developer and release tools", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Db {
        #[command(subcommand)]
        action: DbCommands,
    },
    /// Model transports and the MCP server/client (REQ-016..REQ-019, REQ-025,
    /// REQ-026, REQ-058).
    ///
    /// `COMMANDS.md` has documented `provider probe` and `mcp probe-loopback`
    /// since the control plane was written; both previously failed with
    /// "unrecognized subcommand", so nothing that depended on them was ever run.
    Provider {
        #[command(subcommand)]
        action: ProviderCommands,
    },
    Mcp {
        #[command(subcommand)]
        action: McpCommands,
    },
    /// Work in an isolated git worktree (REQ-031, AGENTS.md §11).
    Repair {
        #[command(subcommand)]
        action: RepairCommands,
    },
    /// Build the study corpus from public-domain sources (REQ-022, REQ-056).
    ///
    /// Ingestion lives here rather than in the application because the sources
    /// are tens of megabytes and cannot ship inside a desktop install; see
    /// `content.rs`.
    Content {
        #[command(subcommand)]
        action: ContentCommands,
    },
    /// Build or verify the release artifact identity.
    ///
    /// Replaces the earlier form, which accepted four ad-hoc file paths and
    /// hashed the literal string "MISSING" for an absent one, so a manifest
    /// built from nothing still produced a confident-looking digest.
    ArtifactIdentity {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        migrations: PathBuf,
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        sbom: PathBuf,
        /// Third-party license notices file.
        #[arg(long)]
        licenses: Option<PathBuf>,
        /// Git commit the artifact was built from.
        #[arg(long)]
        git_sha: Option<String>,
        /// Artifact version recorded in the identity.
        #[arg(long, default_value = "0.1.0")]
        version: String,
        /// Verify an existing identity file instead of building a new one.
        #[arg(long)]
        verify: Option<PathBuf>,
    },
    /// Generate the software bill of materials.
    ///
    /// Licenses are read from installed package manifests, because
    /// `pnpm-lock.yaml` records versions but not licenses. A package whose
    /// license cannot be established is reported and the SBOM refuses, rather
    /// than the tool inventing terms.
    Sbom {
        #[arg(long, default_value = "pnpm-lock.yaml")]
        lockfile: PathBuf,
        #[arg(long, default_value = "node_modules")]
        node_modules: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "0.1.0")]
        version: String,
    },
    /// Generate the V-000..V-021 release accounting from the master registry.
    ///
    /// Every registry ID is accounted for. A not-applicable classification is
    /// only assigned when a subsystem probe proves the subsystem is absent, as
    /// HARNESS_LAWS.md law 5 requires.
    Account {
        #[arg(long, default_value = ".agent/verification/MASTER_TEST_REGISTRY.csv")]
        registry: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        summary_out: Option<PathBuf>,
    },
    /// Generate an Ed25519 key for signing content packs.
    ///
    /// Refuses to overwrite an existing key: a signing key that was quietly replaced
    /// is a key whose packs nobody can identify.
    #[command(name = "pack-keygen")]
    PackKeygen {
        /// Where to write the 64-hex-character key.
        #[arg(long)]
        key: PathBuf,
    },
    /// Build a signed pack from everything a store currently serves.
    #[command(name = "pack-build")]
    PackBuild {
        /// Database to read from.
        #[arg(long)]
        db: PathBuf,
        /// Where to write the pack file.
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "core-asvab")]
        name: String,
        #[arg(long, default_value_t = 1)]
        version: i64,
        /// The oldest application version that can install this pack.
        #[arg(long, default_value = "0.1.0")]
        app_min: String,
        /// The newest, when the pack is known not to work past a version.
        #[arg(long)]
        app_max: Option<String>,
        /// The Ed25519 signing key, as 64 hex characters. See `pack-keygen`.
        #[arg(long)]
        key: PathBuf,
        /// A JSON array of curriculum nodes. Without it the pack declares one node per
        /// objective its items teach, with no prerequisites.
        #[arg(long)]
        curriculum: Option<PathBuf>,
        /// A JSON array of calibration entries. Without it every objective is declared from
        /// the items' difficulty scale, resting on no responses.
        #[arg(long)]
        calibration: Option<PathBuf>,
    },
    /// Verify and install a pack file.
    ///
    /// The trusted signer is required: a signature only proves that *someone*
    /// attested the pack, so an installation that accepted any signer would accept a
    /// pack its own author signed.
    #[command(name = "pack-install")]
    PackInstall {
        /// Database to install into.
        #[arg(long)]
        db: PathBuf,
        /// The pack file.
        #[arg(long)]
        pack: PathBuf,
        /// The public key this installation trusts, as 64 hex characters.
        #[arg(long)]
        trusted_signer: String,
        /// The running application version, for the compatibility check.
        #[arg(long, default_value = "0.1.0")]
        app_version: String,
    },
    /// List the packs a store has installed.
    #[command(name = "pack-list")]
    PackList {
        #[arg(long)]
        db: PathBuf,
    },
    /// Roll a pack back to its previous version.
    #[command(name = "pack-rollback")]
    PackRollback {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        name: String,
    },
    /// Put a registered pack version back into service: the way back from a rollback.
    ///
    /// `pack-install` will not do this, on purpose -- a reinstall must not silently undo a
    /// rollback somebody made -- so the transition back is its own command and the caller has to
    /// mean it. Whatever is active is superseded rather than quarantined, so the transition can
    /// be made again in either direction.
    #[command(name = "pack-activate")]
    PackActivate {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: i64,
    },
}

#[derive(Subcommand)]
enum RepairCommands {
    /// Create an isolated worktree, run one gate inside it, and remove it.
    ///
    /// The gate must be one of the lane gates named in COMMANDS.md; the worktree
    /// is verified to contain no learner state before the gate runs.
    Lane {
        /// Gate to run, e.g. `format-check`.
        #[arg(long)]
        gate: String,
        /// Commit-ish to check out.
        #[arg(long, default_value = "HEAD")]
        base: String,
        /// Repository to branch from. Defaults to this checkout.
        #[arg(long)]
        repository: Option<PathBuf>,
        /// Write the JSON report here as well as to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// Probe every configured model transport and report its real state.
    Probe {
        /// Probe every adapter in the registry, not only the ones that look
        /// configured. A lane that is installed but signed out is a fact worth
        /// reporting.
        #[arg(long)]
        all_configured: bool,
        /// Send one minimal prompt through each healthy lane and validate the
        /// response. This contacts the provider under the user's own login, so
        /// it is off by default.
        #[arg(long)]
        live: bool,
        /// Write the JSON report here as well as to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum McpCommands {
    /// Serve MCP on stdin/stdout.
    Serve {
        /// Study database to expose. Without it the server answers "no database
        /// is attached" rather than inventing content.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Root the connected peer may write drafts into, when it holds the
        /// capability.
        #[arg(long)]
        writable_root: Option<PathBuf>,
    },
    /// Start a real MCP server and drive it with the real client.
    ProbeLoopback {
        /// Study database the server should expose.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Write the JSON report here as well as to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    Setup {
        #[arg(long, default_value = "vector.db")]
        db_path: String,
    },
    Migrate {
        #[arg(long, default_value = "vector.db")]
        db_path: String,
    },
    Backup {
        #[arg(long, default_value = "vector.db")]
        db_path: String,
        #[arg(long)]
        dest: PathBuf,
    },
    Restore {
        #[arg(long, default_value = "vector.db")]
        db_path: String,
        #[arg(long)]
        source: PathBuf,
        /// Optional SHA-256 the archive must match before it is applied.
        #[arg(long)]
        checksum: Option<String>,
    },
}

/// Directory holding the numbered SQL migrations.
fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

#[derive(Subcommand)]
enum ContentCommands {
    /// Ingest Word Knowledge items from public-domain sources.
    ///
    /// Both sources are required. The thesaurus supplies the synonym
    /// relationship and Webster's corroborates it, so a pair is kept only when
    /// the dictionary links the two words. Running with the thesaurus alone
    /// produced a corpus the dictionary endorses 8% of the time; see the commit
    /// that added this filter.
    #[command(name = "ingest-wk")]
    Wk {
        /// Database to write into. The application's own database is at
        /// `%APPDATA%\com.vector.app\vector.db`.
        #[arg(long)]
        db: PathBuf,
        /// Moby Thesaurus `words.txt` (public domain, Gutenberg #3202).
        #[arg(long)]
        thesaurus: PathBuf,
        /// Webster's Unabridged 1913 text (public domain, Gutenberg #29765).
        #[arg(long)]
        dictionary: PathBuf,
        #[arg(long, default_value_t = 2000)]
        count: usize,
        /// Fixed so a pack can be regenerated and its hashes re-derived.
        #[arg(long, default_value_t = 20_260_922)]
        seed: u64,
        #[arg(long, default_value = "content-reviewer")]
        reviewer: String,
        /// Write a JSON report here as well as printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Ingest Electronics Information items from NEETS module glossaries.
    ///
    /// One vault row and one ingestion run per module, so an item's citation names
    /// the module it came from and a module that parses badly cannot take the rest
    /// of the collection down with it.
    #[command(name = "ingest-ei")]
    Ei {
        /// Database to write into.
        #[arg(long)]
        db: PathBuf,
        /// Webster's Unabridged 1913 text, used only for the OCR check.
        #[arg(long)]
        dictionary: PathBuf,
        /// A NEETS module text file. Repeat for each module.
        #[arg(long = "module", required = true)]
        modules: Vec<PathBuf>,
        /// Items to attempt per module.
        #[arg(long, default_value_t = 400)]
        count: usize,
        #[arg(long, default_value_t = 20_260_922)]
        seed: u64,
        /// Definitions shorter than this cannot identify a concept.
        #[arg(long, default_value_t = 5)]
        min_definition_words: usize,
        #[arg(long, default_value = "content-reviewer")]
        reviewer: String,
        /// Write a JSON report here as well as printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Ingest Paragraph Comprehension items from public-domain prose.
    ///
    /// One vault row and one run per work, so an item's citation names the work its
    /// passage came from. The work's title and the ebook number are both required:
    /// the title is read from the file's own header, and the number is what makes
    /// the recorded URL resolvable, so provenance can be checked by re-downloading.
    #[command(name = "ingest-pc")]
    Pc {
        /// Database to write into.
        #[arg(long)]
        db: PathBuf,
        /// A work to ingest, as `<project-gutenberg-ebook-number>=<path>`. Repeat
        /// for each work, e.g. `49819=sources/gutenberg/doe-nuclear-1.txt`.
        #[arg(long = "work", required = true)]
        works: Vec<String>,
        /// Items to attempt per work.
        #[arg(long, default_value_t = 400)]
        count: usize,
        #[arg(long, default_value_t = 20_260_922)]
        seed: u64,
        #[arg(long, default_value = "content-reviewer")]
        reviewer: String,
        /// Write a JSON report here as well as printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Ingest Shop Information items from public-domain tool manuals.
    ///
    /// These manuals are Internet Archive scans rather than Project Gutenberg works, so a
    /// manual is named `<archive-id>:<title>=<path>`: the id makes the recorded URL
    /// resolvable and the title is what a citation should say.
    #[command(name = "ingest-tools")]
    Tools {
        /// Database to write into.
        #[arg(long)]
        db: PathBuf,
        /// The subtest the items serve: `SI` for the shop manuals, `AI` for the automotive ones.
        #[arg(long, default_value = "SI")]
        subtest: String,
        /// Which half of each description to ask about: `tools` or `functions`.
        #[arg(long, default_value = "tools")]
        ask: String,
        /// Webster's Unabridged 1913 text, used only for the OCR check.
        #[arg(long)]
        dictionary: PathBuf,
        /// A manual, as `<archive-id>:<title>=<path>`. Repeat for each manual.
        #[arg(long = "work", required = true)]
        works: Vec<String>,
        /// Items to attempt per manual.
        #[arg(long, default_value_t = 200)]
        count: usize,
        #[arg(long, default_value_t = 20_260_922)]
        seed: u64,
        #[arg(long, default_value = "content-reviewer")]
        reviewer: String,
        /// Write a JSON report here as well as printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Ingest factual items from public-domain question-and-answer works.
    ///
    /// The subtest is required because one book can serve more than one: a science
    /// work answers General Science questions, and a tool or automotive manual
    /// answers Shop and Auto Information questions.
    #[command(name = "ingest-facts")]
    Facts {
        /// Database to write into.
        #[arg(long)]
        db: PathBuf,
        /// The subtest the items serve, e.g. `GS`, `AI`.
        #[arg(long)]
        subtest: String,
        /// A work to ingest, as `<project-gutenberg-ebook-number>=<path>`. Repeat
        /// for each work.
        #[arg(long = "work", required = true)]
        works: Vec<String>,
        /// Items to attempt per work.
        #[arg(long, default_value_t = 500)]
        count: usize,
        #[arg(long, default_value_t = 20_260_922)]
        seed: u64,
        #[arg(long, default_value = "content-reviewer")]
        reviewer: String,
        /// Write a JSON report here as well as printing it.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn migrate(db_path: &str) -> Result<usize> {
    let mut db = Database::open(db_path)?;
    let migrations = MigrationManager::load_from_dir(&migrations_dir())?;
    MigrationManager::apply(&mut db, &migrations)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Content { action } => match action {
            ContentCommands::Wk {
                db,
                thesaurus,
                dictionary,
                count,
                seed,
                reviewer,
                out,
            } => {
                let started = std::time::Instant::now();
                let outcome =
                    content::ingest_wk(&db, &thesaurus, &dictionary, count, seed, &reviewer)?;
                let elapsed = started.elapsed();

                let report = serde_json::json!({
                    "database": db.display().to_string(),
                    "thesaurus": {
                        "path": thesaurus.display().to_string(),
                        "sha256": outcome.thesaurus_hash,
                        "root_words": outcome.thesaurus_roots,
                    },
                    "dictionary": {
                        "path": dictionary.display().to_string(),
                        "sha256": outcome.dictionary_hash,
                        "entries": outcome.dictionary_entries,
                    },
                    "requested": count,
                    "seed": seed,
                    "built": outcome.built,
                    "activated": outcome.activated,
                    "already_present": outcome.already_present,
                    "rejected": outcome.rejected,
                    "distinct_items": outcome.item_ids.len(),
                    "elapsed_ms": elapsed.as_millis(),
                });
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, format!("{text}\n"))?;
                }
                if outcome.activated == 0 {
                    // The pipeline already refuses a run that stored nothing, so
                    // reaching here means everything was already present. Saying
                    // so plainly beats reporting a zero that looks like failure.
                    eprintln!(
                        "content ingest-wk: nothing new; {} item(s) already present",
                        outcome.already_present
                    );
                }
            }
            ContentCommands::Ei {
                db,
                dictionary,
                modules,
                count,
                seed,
                min_definition_words,
                reviewer,
                out,
            } => {
                let started = std::time::Instant::now();
                let outcome = content::ingest_ei(
                    &db,
                    &dictionary,
                    &modules,
                    count,
                    seed,
                    &reviewer,
                    min_definition_words,
                )?;

                let report = serde_json::json!({
                    "database": db.display().to_string(),
                    "dictionary": {
                        "path": dictionary.display().to_string(),
                        "sha256": outcome.dictionary_hash,
                        "entries": outcome.dictionary_entries,
                    },
                    "requested_per_module": count,
                    "seed": seed,
                    "min_definition_words": min_definition_words,
                    "modules": outcome.modules.iter().map(|m| serde_json::json!({
                        "module": m.module,
                        "path": m.path,
                        "sha256": m.sha256,
                        "glossary_entries": m.entries,
                        "built": m.built,
                        "activated": m.activated,
                        "already_present": m.already_present,
                        "rejected": m.rejected,
                        "skipped": m.skipped,
                    })).collect::<Vec<_>>(),
                    "totals": {
                        "glossary_entries": outcome.total_entries,
                        "built": outcome.total_built,
                        "activated": outcome.total_activated,
                        "already_present": outcome.total_already_present,
                        "rejected": outcome.total_rejected,
                        "skipped_modules": outcome
                            .modules
                            .iter()
                            .filter(|m| m.skipped.is_some())
                            .count(),
                    },
                    "elapsed_ms": started.elapsed().as_millis(),
                });
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, format!("{text}\n"))?;
                }
                if outcome.total_activated == 0 {
                    eprintln!(
                        "content ingest-ei: nothing new; {} item(s) already present",
                        outcome.total_already_present
                    );
                }
            }
            ContentCommands::Pc {
                db,
                works,
                count,
                seed,
                reviewer,
                out,
            } => {
                let started = std::time::Instant::now();
                let parsed = works
                    .iter()
                    .map(|work| content::parse_work(work))
                    .collect::<Result<Vec<_>>>()?;
                let outcome = content::ingest_pc(&db, &parsed, count, seed, &reviewer)?;

                let report = serde_json::json!({
                    "database": db.display().to_string(),
                    "requested_per_work": count,
                    "seed": seed,
                    "works": outcome.works.iter().map(|w| serde_json::json!({
                        "ebook": w.ebook_id,
                        "title": w.title,
                        "path": w.path,
                        "sha256": w.sha256,
                        "paragraphs": w.paragraphs,
                        "built": w.built,
                        "activated": w.activated,
                        "already_present": w.already_present,
                        "rejected": w.rejected,
                        "rejection_reasons": w.rejection_reasons,
                        "skipped": w.skipped,
                    })).collect::<Vec<_>>(),
                    "totals": {
                        "paragraphs": outcome.total_paragraphs,
                        "built": outcome.total_built,
                        "activated": outcome.total_activated,
                        "already_present": outcome.total_already_present,
                        "rejected": outcome.total_rejected,
                    },
                    "elapsed_ms": started.elapsed().as_millis(),
                });
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, format!("{text}\n"))?;
                }
                if outcome.total_activated == 0 {
                    eprintln!(
                        "content ingest-pc: nothing new; {} item(s) already present",
                        outcome.total_already_present
                    );
                }
            }
            ContentCommands::Facts {
                db,
                subtest,
                works,
                count,
                seed,
                reviewer,
                out,
            } => {
                let started = std::time::Instant::now();
                let parsed = works
                    .iter()
                    .map(|work| content::parse_work(work))
                    .collect::<Result<Vec<_>>>()?;
                let outcome =
                    content::ingest_facts(&db, &subtest, &parsed, count, seed, &reviewer)?;

                let report = serde_json::json!({
                    "database": db.display().to_string(),
                    "subtest": outcome.subtest,
                    "requested_per_work": count,
                    "seed": seed,
                    "works": outcome.works.iter().map(|w| serde_json::json!({
                        "ebook": w.ebook_id,
                        "title": w.title,
                        "path": w.path,
                        "sha256": w.sha256,
                        "questions": w.questions,
                        "built": w.built,
                        "activated": w.activated,
                        "already_present": w.already_present,
                        "rejected": w.rejected,
                        "rejection_reasons": w.rejection_reasons,
                        "skipped": w.skipped,
                    })).collect::<Vec<_>>(),
                    "totals": {
                        "questions": outcome.total_questions,
                        "built": outcome.total_built,
                        "activated": outcome.total_activated,
                        "already_present": outcome.total_already_present,
                        "rejected": outcome.total_rejected,
                    },
                    "elapsed_ms": started.elapsed().as_millis(),
                });
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, format!("{text}\n"))?;
                }
                if outcome.total_activated == 0 {
                    eprintln!(
                        "content ingest-facts: nothing new; {} item(s) already present",
                        outcome.total_already_present
                    );
                }
            }
            ContentCommands::Tools {
                db,
                subtest,
                ask,
                dictionary,
                works,
                count,
                seed,
                reviewer,
                out,
            } => {
                let started = std::time::Instant::now();
                let parsed = works
                    .iter()
                    .map(|work| content::parse_tool_source(work))
                    .collect::<Result<Vec<_>>>()?;
                let kind = match ask.as_str() {
                    "functions" => vector_questions::purposes::ItemKind::Function,
                    "tools" => vector_questions::purposes::ItemKind::Tool,
                    other => anyhow::bail!("--ask must be tools or functions, not {other:?}"),
                };
                let outcome = content::ingest_tools(
                    &db,
                    &parsed,
                    &content::ToolIngestOptions {
                        subtest: &subtest,
                        kind,
                        count,
                        seed,
                        dictionary_path: &dictionary,
                        reviewer: &reviewer,
                    },
                )?;

                let report = serde_json::json!({
                    "database": db.display().to_string(),
                    "subtest": outcome.subtest,
                    "dictionary": {
                        "path": dictionary.display().to_string(),
                        "sha256": outcome.dictionary_hash,
                        "entries": outcome.dictionary_entries,
                    },
                    "requested_per_manual": count,
                    "seed": seed,
                    "manuals": outcome.works.iter().map(|w| serde_json::json!({
                        "source_id": w.source_id,
                        "url": w.url,
                        "title": w.title,
                        "path": w.path,
                        "sha256": w.sha256,
                        "statements": w.statements,
                        "built": w.built,
                        "activated": w.activated,
                        "already_present": w.already_present,
                        "rejected": w.rejected,
                        "rejection_reasons": w.rejection_reasons,
                        "skipped": w.skipped,
                    })).collect::<Vec<_>>(),
                    "totals": {
                        "statements": outcome.total_statements,
                        "built": outcome.total_built,
                        "activated": outcome.total_activated,
                        "already_present": outcome.total_already_present,
                        "rejected": outcome.total_rejected,
                    },
                    "elapsed_ms": started.elapsed().as_millis(),
                });
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, format!("{text}\n"))?;
                }
                if outcome.total_activated == 0 {
                    eprintln!(
                        "content ingest-tools: nothing new; {} item(s) already present",
                        outcome.total_already_present
                    );
                }
            }
        },
        Commands::PackKeygen { key } => {
            let public = content::generate_signing_key(&key)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "key": key.display().to_string(),
                    "public_key": public,
                    "note": "the public key is what an installation passes as --trusted-signer",
                }))?
            );
        }
        Commands::PackBuild {
            db,
            out,
            name,
            version,
            app_min,
            app_max,
            key,
            curriculum,
            calibration,
        } => {
            let outcome = content::build_pack(
                &db,
                &out,
                &content::PackBuildOptions {
                    name: &name,
                    version,
                    app_min: &app_min,
                    app_max: app_max.as_deref(),
                    key_path: &key,
                    curriculum_path: curriculum.as_deref(),
                    calibration_path: calibration.as_deref(),
                },
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "pack": outcome.path,
                    "name": outcome.name,
                    "version": outcome.version,
                    "content_hash": outcome.content_hash,
                    "signer": outcome.signer,
                    "items": outcome.items,
                    "sources": outcome.sources,
                    "licences": outcome.licences,
                    "bytes": outcome.bytes,
                }))?
            );
        }
        Commands::PackInstall {
            db,
            pack,
            trusted_signer,
            app_version,
        } => {
            let report = content::install_pack(&db, &pack, &trusted_signer, &app_version)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "database": db.display().to_string(),
                    "name": report.name,
                    "version": report.version,
                    "content_hash": report.content_hash,
                    "items": report.items,
                    "installed": report.installed,
                    "already_present": report.already_present,
                    "sources_added": report.sources_added,
                    "sources_reused": report.sources_reused,
                }))?
            );
        }
        Commands::PackList { db } => {
            let packs = content::list_packs(&db)?;
            println!("{}", serde_json::to_string_pretty(&packs)?);
        }
        Commands::PackRollback { db, name } => {
            let rolled = content::rollback_pack(&db, &name)?;
            println!("{}", serde_json::to_string_pretty(&rolled)?);
        }
        Commands::PackActivate { db, name, version } => {
            let activated = content::activate_pack(&db, &name, version)?;
            println!("{}", serde_json::to_string_pretty(&activated)?);
        }
        Commands::Provider { action } => match action {
            ProviderCommands::Probe {
                all_configured,
                live,
                out,
            } => {
                let now = chrono::Utc::now();
                let reports = transports::provider_probe(live);
                let report = serde_json::json!({
                    "probe": transports::provider_report_json(&reports, now),
                    "routing": transports::routing_report_json(now, &reports),
                });
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    std::fs::write(&path, format!("{text}\n"))?;
                    eprintln!("wrote {}", path.display());
                }

                // An explicit request to probe *every* configured lane should
                // not fail merely because a provider is unconfigured; the state
                // is the result. It fails only when the registry itself is
                // inconsistent, which cannot happen with a derived registry.
                let healthy = reports
                    .iter()
                    .filter(|r| matches!(r.health, vector_llm::transport::AdapterHealth::Healthy))
                    .count();
                eprintln!(
                    "provider probe: {} lane(s) checked, {healthy} healthy{}",
                    reports.len(),
                    if all_configured {
                        " (all configured)"
                    } else {
                        ""
                    }
                );
            }
        },
        Commands::Mcp { action } => match action {
            McpCommands::Serve { db, writable_root } => {
                let root = writable_root
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned());
                transports::serve_stdio(db.as_deref(), root.as_deref())?;
            }
            McpCommands::ProbeLoopback { db, out } => {
                let checks = transports::mcp_probe_loopback(db.as_deref())?;
                let report = transports::probe_report_json(&checks);
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    std::fs::write(&path, format!("{text}\n"))?;
                    eprintln!("wrote {}", path.display());
                }

                let failed: Vec<&transports::ProbeCheck> =
                    checks.iter().filter(|c| !c.ok).collect();
                for check in &checks {
                    eprintln!(
                        "  {} {}: {}",
                        if check.ok { "PASS" } else { "FAIL" },
                        check.name,
                        check.detail
                    );
                }
                if !failed.is_empty() {
                    anyhow::bail!("{} loopback check(s) failed", failed.len());
                }
                eprintln!("mcp probe-loopback: {} checks passed", checks.len());
            }
        },
        Commands::Repair { action } => match action {
            RepairCommands::Lane {
                gate,
                base,
                repository,
                out,
            } => {
                let root = repository.unwrap_or_else(repair_lane::repository_root);
                let report = repair_lane::run_lane(&root, &base, &gate)?;
                let text = serde_json::to_string_pretty(&report)?;
                println!("{text}");
                if let Some(path) = out {
                    std::fs::write(&path, format!("{text}\n"))?;
                    eprintln!("wrote {}", path.display());
                }

                let code = report
                    .get("exitCode")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(-1);
                eprintln!(
                    "repair lane: {} in {} at {} -> exit {code}",
                    gate,
                    report
                        .get("worktree")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?"),
                    report
                        .get("head")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?")
                );
                if code != 0 {
                    anyhow::bail!("the {gate} gate failed inside the worktree (exit {code})");
                }
            }
        },
        Commands::Db { action } => match action {
            DbCommands::Setup { db_path } => {
                println!("Setting up database at {}", db_path);
                let count = migrate(&db_path)?;
                println!("Database setup complete. Applied {} migrations.", count);
            }
            DbCommands::Migrate { db_path } => {
                println!("Migrating database at {}", db_path);
                let count = migrate(&db_path)?;
                println!("Migration complete. Applied {} migrations.", count);
            }
            DbCommands::Backup { db_path, dest } => {
                let db = Database::open(&db_path)?;
                let manifest = BackupManager::create(&db, &dest)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": manifest.path.display().to_string(),
                        "checksum": manifest.checksum,
                        "bytes": manifest.bytes,
                        "integrity": manifest.integrity,
                        "encrypted": manifest.encrypted,
                        "attempt_rows": manifest.attempt_rows,
                    }))?
                );
            }
            DbCommands::Restore {
                db_path,
                source,
                checksum,
            } => {
                let mut db = Database::open(&db_path)?;
                let outcome =
                    BackupManager::restore_verified(&mut db, &source, checksum.as_deref())?;
                match outcome {
                    RestoreOutcome::Restored { rows } => {
                        println!("Restored {} attempt rows from {}", rows, source.display());
                    }
                }
            }
        },
        Commands::ArtifactIdentity {
            binary,
            migrations,
            content,
            sbom,
            licenses,
            git_sha,
            version,
            verify,
        } => {
            use vector_platform::release::{ArtifactComponent, ArtifactIdentity, Sha256Digest};

            // Every component REQ-053 binds. A missing one is an error rather
            // than a placeholder, so an identity can never overstate what it
            // covers. `measure_path` accepts a file or a directory, because the
            // migration and content components are sets of files: binding one
            // representative file would mean an added migration did not change
            // the identity.
            let mut components = vec![
                ArtifactIdentity::measure_path(ArtifactComponent::Binary, &binary)?,
                ArtifactIdentity::measure_path(ArtifactComponent::Migrations, &migrations)?,
                ArtifactIdentity::measure_path(ArtifactComponent::Content, &content)?,
                ArtifactIdentity::measure_path(ArtifactComponent::Sbom, &sbom)?,
            ];

            match &licenses {
                Some(path) => components.push(ArtifactIdentity::measure_path(
                    ArtifactComponent::Licenses,
                    path,
                )?),
                None => {
                    anyhow::bail!(
                        "the identity must bind third-party license notices; \
                         pass --licenses <path>"
                    );
                }
            }

            match &git_sha {
                Some(sha) => components.push(vector_platform::release::MeasuredComponent {
                    component: ArtifactComponent::GitSha,
                    digest: Sha256Digest::of_text(sha),
                    bytes: None,
                }),
                None => {
                    anyhow::bail!("the identity must bind the build commit; pass --git-sha <sha>");
                }
            }

            let identity = ArtifactIdentity::build(&version, components)?;

            if let Some(recorded_path) = verify {
                // Re-derive from the files on disk rather than trusting the
                // recorded digests.
                let recorded: ArtifactIdentity =
                    serde_json::from_str(&std::fs::read_to_string(&recorded_path)?)?;
                let mut paths = vec![
                    (ArtifactComponent::Binary, binary.clone()),
                    (ArtifactComponent::Migrations, migrations.clone()),
                    (ArtifactComponent::Content, content.clone()),
                    (ArtifactComponent::Sbom, sbom.clone()),
                ];
                if let Some(path) = &licenses {
                    paths.push((ArtifactComponent::Licenses, path.clone()));
                }
                recorded.verify_against(&paths)?;
                println!("identity verified against {}", recorded_path.display());
            }

            println!("{}", serde_json::to_string_pretty(&identity)?);
        }
        Commands::Sbom {
            lockfile,
            node_modules,
            out,
            version,
        } => {
            let text = std::fs::read_to_string(&lockfile)?;
            let locked = vector_platform::release::parse_pnpm_lockfile(&text);
            let installed = vector_platform::release::read_installed_licenses(&node_modules);

            // Prefer installed manifests, which declare licenses; fall back to
            // the lockfile so a dependency present in the lockfile but not
            // installed is still listed and reported as unknown.
            let entries = if installed.is_empty() {
                eprintln!(
                    "WARN: no installed packages found under {}; \
                     licenses cannot be established from the lockfile alone",
                    node_modules.display()
                );
                locked
            } else {
                installed
            };

            let sbom = vector_platform::release::Sbom { entries };

            // Report rather than hide: the SBOM is written even when validation
            // fails, so a human can see exactly which dependency is unresolved.
            if let Err(error) = sbom.validate() {
                let rendered = sbom.render_spdx("project-vector", &version);
                std::fs::write(&out, &rendered)?;
                eprintln!(
                    "WROTE {} with an unresolved license question: {error}",
                    out.display()
                );
                eprintln!(
                    "{} dependencies listed, {} with a declared license",
                    sbom.entries.len(),
                    sbom.entries.iter().filter(|e| e.license.is_some()).count()
                );
                std::process::exit(2);
            }

            let rendered = sbom.render_spdx("project-vector", &version);
            std::fs::write(&out, &rendered)?;
            println!(
                "wrote {} ({} dependencies, {} distinct licenses)",
                out.display(),
                sbom.entries.len(),
                sbom.licenses().len()
            );
        }
        Commands::Account {
            registry,
            out,
            summary_out,
        } => {
            use vector_platform::accounting::{classify, render_csv, AccountingSummary};

            let text = std::fs::read_to_string(&registry)?;
            let rows = parse_registry(&text)?;

            // Subsystem probes. Each records what it searched for and what it
            // found, so an absence claim is falsifiable rather than asserted.
            //
            // Tracked files only (`git ls-files`), because an untracked scratch
            // file is not part of the product.
            let tracked = tracked_files()?;

            let probes = vec![
                probe_subsystem(
                    "blockchain",
                    &[
                        ".sol",
                        "solidity",
                        "web3",
                        "ethers",
                        "smart_contract",
                        "smart-contract",
                    ],
                    &tracked,
                ),
                probe_subsystem(
                    "hipaa",
                    &["hipaa", "phi_", "patient_", "healthcare", "medical_record"],
                    &tracked,
                ),
            ];

            for probe in &probes {
                println!("probe {}: absent={}", probe.name, probe.is_absent());
                for found in &probe.found {
                    println!("  found: {found}");
                }
            }

            let accounted: Vec<_> = rows.iter().map(|row| classify(row, &probes)).collect();
            let summary = AccountingSummary::of(&accounted);

            std::fs::write(&out, render_csv(&accounted))?;
            println!(
                "wrote {} ({} rows, {} PENDING)",
                out.display(),
                summary.total,
                summary.count(vector_platform::accounting::TestStatus::Pending)
            );
            for (status, count) in &summary.by_status {
                println!("  {status}: {count}");
            }

            if let Some(path) = summary_out {
                std::fs::write(&path, serde_json::to_string_pretty(&summary)?)?;
                println!("wrote summary to {}", path.display());
            }
        }
    }

    Ok(())
}

/// Parse the master test registry CSV.
///
/// Written by hand rather than pulling a CSV crate, because the registry is a
/// simple five-column file and a dependency for this would need its own license
/// review.
fn parse_registry(text: &str) -> Result<Vec<vector_platform::accounting::RegistryRow>> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or_default();
    let columns: Vec<&str> = header.split(',').map(|c| c.trim()).collect();

    let index_of = |name: &str| -> Result<usize> {
        columns
            .iter()
            .position(|c| *c == name)
            .ok_or_else(|| anyhow::anyhow!("registry is missing the {name} column"))
    };

    let id_at = index_of("test_id")?;
    let group_at = index_of("source_group")?;
    let kind_at = index_of("kind")?;
    let stage_at = index_of("default_stage")?;
    let applicability_at = index_of("applicability")?;

    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // The registry contains no quoted fields, so a plain split is exact.
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() <= applicability_at {
            continue;
        }
        rows.push(vector_platform::accounting::RegistryRow {
            test_id: fields[id_at].trim().to_string(),
            source_group: fields[group_at].trim().to_string(),
            kind: fields[kind_at].trim().to_string(),
            default_stage: fields[stage_at].trim().to_string(),
            applicability: fields[applicability_at].trim().to_string(),
        });
    }
    Ok(rows)
}

/// List files tracked by git.
fn tracked_files() -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["ls-files"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("git ls-files failed; cannot probe the repository");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(|l| l.trim().to_string()).collect())
}

/// Probe for a subsystem by searching tracked file paths.
///
/// The test library itself is excluded: the registry's own prompt files mention
/// blockchain and HIPAA extensively and would otherwise make every subsystem
/// look present.
fn probe_subsystem(
    name: &str,
    indicators: &[&str],
    tracked: &[String],
) -> vector_platform::accounting::SubsystemProbe {
    const LIBRARY_PREFIXES: &[&str] = &[
        ".agent/verification/source-library/",
        ".agent/verification/casebooks/",
        ".agent/verification/APPLICABILITY_MATRIX.csv",
        ".agent/verification/MASTER_TEST_REGISTRY.csv",
        ".agent/verification/BLOCKCHAIN_008_022_RECONSTRUCTED.md",
    ];

    let found: Vec<String> = tracked
        .iter()
        .filter(|path| !LIBRARY_PREFIXES.iter().any(|p| path.starts_with(p)))
        .filter(|path| {
            let lower = path.to_lowercase();
            indicators.iter().any(|i| lower.contains(&i.to_lowercase()))
        })
        .cloned()
        .collect();

    vector_platform::accounting::SubsystemProbe {
        name: name.to_string(),
        indicators: indicators.iter().map(|i| (*i).to_string()).collect(),
        found,
    }
}
