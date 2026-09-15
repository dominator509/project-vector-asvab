use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vector_persistence::backup::{BackupManager, RestoreOutcome};
use vector_persistence::{Database, MigrationManager};

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

fn migrate(db_path: &str) -> Result<usize> {
    let mut db = Database::open(db_path)?;
    let migrations = MigrationManager::load_from_dir(&migrations_dir())?;
    MigrationManager::apply(&mut db, &migrations)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
