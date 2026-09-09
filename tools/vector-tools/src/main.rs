use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Db { action } => match action {
            DbCommands::Setup { db_path } => {
                println!("Setting up database at {}", db_path);
                let db = Database::open(&db_path)?;
                let initial_sql = include_str!("../../../migrations/001_initial.sql");
                let count =
                    MigrationManager::apply_migrations(db.connection(), &[("1", initial_sql)])?;
                println!("Database setup complete. Applied {} migrations.", count);
            }
            DbCommands::Migrate { db_path } => {
                println!("Migrating database at {}", db_path);
                let db = Database::open(&db_path)?;
                let initial_sql = include_str!("../../../migrations/001_initial.sql");
                let count =
                    MigrationManager::apply_migrations(db.connection(), &[("1", initial_sql)])?;
                println!("Migration complete. Applied {} migrations.", count);
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
