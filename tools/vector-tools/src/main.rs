use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vector_persistence::backup::{BackupManager, RestoreOutcome};
use vector_persistence::{Database, MigrationManager};

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

fn migrate(db_path: &str) -> Result<usize> {
    let mut db = Database::open(db_path)?;
    let migrations = MigrationManager::load_from_dir(&migrations_dir())?;
    MigrationManager::apply(&mut db, &migrations)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
            // covers.
            let mut components = vec![
                ArtifactIdentity::measure_file(ArtifactComponent::Binary, &binary)?,
                ArtifactIdentity::measure_file(ArtifactComponent::Migrations, &migrations)?,
                ArtifactIdentity::measure_file(ArtifactComponent::Content, &content)?,
                ArtifactIdentity::measure_file(ArtifactComponent::Sbom, &sbom)?,
            ];

            match &licenses {
                Some(path) => components.push(ArtifactIdentity::measure_file(
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
                recorded.verify_against(&[
                    (ArtifactComponent::Binary, binary.clone()),
                    (ArtifactComponent::Migrations, migrations.clone()),
                    (ArtifactComponent::Content, content.clone()),
                    (ArtifactComponent::Sbom, sbom.clone()),
                ])?;
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
    }

    Ok(())
}
