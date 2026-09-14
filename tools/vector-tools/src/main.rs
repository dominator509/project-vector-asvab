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
    ArtifactIdentity {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        migrations: PathBuf,
        #[arg(long)]
        content: PathBuf,
        #[arg(long)]
        sbom: PathBuf,
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
        } => {
            use sha2::{Digest, Sha256};
            use std::fs;

            let hash_file = |p: &PathBuf| -> Result<String> {
                if !p.exists() {
                    return Ok("MISSING".to_string());
                }
                let bytes = fs::read(p)?;
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                Ok(format!("{:x}", hasher.finalize()))
            };

            let b_hash = hash_file(&binary)?;
            let m_hash = hash_file(&migrations)?;
            let c_hash = hash_file(&content)?;
            let s_hash = hash_file(&sbom)?;

            let mut combined_hasher = Sha256::new();
            combined_hasher.update(b_hash.as_bytes());
            combined_hasher.update(m_hash.as_bytes());
            combined_hasher.update(c_hash.as_bytes());
            combined_hasher.update(s_hash.as_bytes());
            let bound_digest = format!("{:x}", combined_hasher.finalize());

            let manifest = serde_json::json!({
                "binary_hash": b_hash,
                "migrations_hash": m_hash,
                "content_hash": c_hash,
                "sbom_hash": s_hash,
                "artifact_identity_digest": bound_digest,
            });

            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
    }

    Ok(())
}
