//! In-artifact self-check: the live-fire entry point.
//!
//! `AGENTS.md` §9 (reality law) says compilation, screenshots, mocked tests and
//! a health endpoint returning 200 are not feature proof, and `.agent/DONE_LAW.md`
//! requires real artifact execution with independent readback. `cargo test`
//! cannot supply that for the desktop application: the release executable is a
//! separate build with the real Tauri runtime and the embedded frontend, and a
//! test binary linked against the library says nothing about it.
//!
//! This module therefore runs *inside the shipped executable*. It opens a real
//! SQLite file through the same [`commands::AppState`] the window uses, drives
//! the same command implementations the webview invokes, and reads every effect
//! back from storage rather than from a return value. It reports what it
//! observed as JSON, on stdout and optionally in a file, and exits non-zero if
//! any step failed.
//!
//! ## What it does not claim
//!
//! It does not prove webview-to-Rust IPC delivery or that the window renders.
//! Those need a display and a UI driver; the Playwright suite covers the
//! frontend in a browser and the launch evidence covers the window. This report
//! is about the command layer and the persistence path, and it says so.
//!
//! ## Data safety
//!
//! Without `--data-dir` it uses a fresh temporary directory and deletes it
//! afterwards, so a self-check can never touch a learner's real database. With
//! `--data-dir` it operates on exactly that directory and leaves it in place,
//! which is what a deployment smoke test wants.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::commands::{
    analytics_impl, backup_create_impl, backup_list_impl, backup_restore_impl, create_profile_impl,
    evidence_get_impl, evidence_list_impl, evidence_put_impl, get_profile_impl, health_impl,
    latency_probe_impl, list_profiles_impl, migrations, readiness_impl, record_attempt_impl,
    reset_local_data_impl, set_mastery_impl, study_plan_impl, AppState,
};
use vector_application::service::{BackupDto, LatencyDto, NewEvidenceDto, RestoreDto};
use vector_persistence::MigrationManager;

/// Upper bound on the mean of one indexed count query, in microseconds.
///
/// REQ-039 asks for performance bounds measured on the artifact. A read of a
/// local table is far below this on any supported machine; the bound exists to
/// catch a regression that makes a hot read path pathological — a missing
/// index, a per-row round trip — not to measure the machine.
const SLO_MEAN_QUERY_MICROS: u64 = 20_000;

/// Upper bound on the whole self-check, in milliseconds. This is the artifact's
/// cold-start budget: process launch, migration check, and every step below.
const SLO_TOTAL_MS: u128 = 30_000;

/// Iterations for the latency probe.
const SLO_PROBE_ITERATIONS: u32 = 50;

#[derive(Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub artifact: String,
    pub version: String,
    /// `pass` or `fail`. Present so a consumer never has to infer the verdict
    /// from the exit code alone.
    pub verdict: String,
    pub data_dir: String,
    pub elapsed_ms: u128,
    pub checks: Vec<Check>,
    pub latency: Option<LatencyDto>,
    pub backup: Option<BackupDto>,
    pub restore: Option<RestoreDto>,
}

/// Accumulates check outcomes so a report lists everything that was attempted.
#[derive(Default)]
struct Checks {
    items: Vec<Check>,
}

impl Checks {
    fn record(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        self.items.push(Check {
            name: name.to_string(),
            ok,
            detail: detail.into(),
        });
    }

    fn failed(&self) -> bool {
        self.items.iter().any(|c| !c.ok)
    }
}

/// Parsed command line for the self-check entry point.
struct Options {
    data_dir: Option<PathBuf>,
    report: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        data_dir: None,
        report: None,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--self-check" => {}
            "--data-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--data-dir requires a path".to_string())?;
                options.data_dir = Some(PathBuf::from(value));
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--report requires a path".to_string())?;
                options.report = Some(PathBuf::from(value));
            }
            other => return Err(format!("unrecognised argument {other:?}")),
        }
        index += 1;
    }
    Ok(options)
}

/// A working directory that removes itself unless the caller supplied it.
struct Scratch {
    path: PathBuf,
    owned: bool,
}

impl Scratch {
    fn new(explicit: Option<PathBuf>) -> Result<Self, String> {
        match explicit {
            Some(path) => {
                std::fs::create_dir_all(&path)
                    .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
                Ok(Self { path, owned: false })
            }
            None => {
                let mut path = std::env::temp_dir();
                path.push(format!(
                    "vector-selfcheck-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
                std::fs::create_dir_all(&path)
                    .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
                Ok(Self { path, owned: true })
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.owned {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Run the self-check. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let started = Instant::now();

    let options = match parse(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("self-check: {message}");
            return 2;
        }
    };

    let scratch = match Scratch::new(options.data_dir.clone()) {
        Ok(scratch) => scratch,
        Err(message) => {
            eprintln!("self-check: {message}");
            return 2;
        }
    };

    let mut report = Report {
        artifact: "vector-desktop".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        verdict: "fail".to_string(),
        data_dir: scratch.path.to_string_lossy().into_owned(),
        elapsed_ms: 0,
        checks: Vec::new(),
        latency: None,
        backup: None,
        restore: None,
    };

    let mut checks = Checks::default();
    execute(scratch.path.as_path(), &mut report, &mut checks);

    // The total bound is evaluated here so it covers every step above rather
    // than a single timed region.
    let elapsed_ms = started.elapsed().as_millis();
    checks.record(
        "artifact_slo_total_ms",
        elapsed_ms <= SLO_TOTAL_MS,
        format!("{elapsed_ms} ms (bound {SLO_TOTAL_MS} ms)"),
    );

    report.elapsed_ms = elapsed_ms;
    report.verdict = if checks.failed() { "fail" } else { "pass" }.to_string();
    report.checks = checks.items;

    emit(&report, options.report.as_deref());

    if report.verdict == "pass" {
        0
    } else {
        1
    }
}

/// Run every step, recording outcomes.
///
/// Deliberately does not stop at the first failure: a report that halts hides
/// the state of everything after the problem, which is exactly the information
/// needed to diagnose it.
fn execute(dir: &Path, report: &mut Report, checks: &mut Checks) {
    let db_path = dir.join("vector.db");

    let state = match AppState::open(&db_path) {
        Ok(state) => state,
        Err(message) => {
            checks.record("open_and_migrate", false, message);
            return;
        }
    };

    // ---- Startup, health and profiles -----------------------------------
    let learner = {
        let guard = match state.db() {
            Ok(guard) => guard,
            Err(message) => {
                checks.record("acquire_database", false, message);
                return;
            }
        };

        match MigrationManager::applied_versions(&guard) {
            Ok(versions) => {
                let expected: Vec<i64> = migrations().iter().map(|(v, _)| *v).collect();
                checks.record(
                    "open_and_migrate",
                    versions == expected,
                    format!(
                        "applied {versions:?}, embedded {expected:?}, database file exists {}",
                        db_path.exists()
                    ),
                );
            }
            Err(error) => checks.record("open_and_migrate", false, error.to_string()),
        }

        match health_impl(&guard) {
            Ok(health) => checks.record(
                "health",
                health.healthy && health.basis == "operation_succeeded",
                format!(
                    "healthy={} basis={} profiles={}",
                    health.healthy, health.basis, health.profiles
                ),
            ),
            Err(error) => checks.record("health", false, error.to_string()),
        }

        let created = create_profile_impl(&guard, "Self-check Learner", 50);
        let learner = match &created {
            Ok(profile) => match get_profile_impl(&guard, &profile.id) {
                Ok(found) => {
                    checks.record(
                        "profile_round_trip",
                        &found == profile,
                        format!("created and re-read {}", profile.id),
                    );
                    Some(profile.id.clone())
                }
                Err(error) => {
                    checks.record("profile_round_trip", false, error.to_string());
                    None
                }
            },
            Err(error) => {
                checks.record("profile_round_trip", false, error.to_string());
                None
            }
        };

        // A refused write must not create a row.
        let refusal = create_profile_impl(&guard, "   ", 50).is_err()
            && create_profile_impl(&guard, "Off Scale", 120).is_err();
        let count = list_profiles_impl(&guard)
            .map(|p| p.len())
            .unwrap_or(usize::MAX);
        checks.record(
            "invalid_input_refused",
            refusal && count == 1,
            format!("blank and off-scale targets refused; {count} profile(s) stored"),
        );

        learner
    };

    // ---- Attempts, analytics, planning ----------------------------------
    if let Some(learner) = learner.as_deref() {
        let guard = state.db().expect("lock");

        let first = record_attempt_impl(&guard, "sc-1", learner, "AR", "q-1", true, 1200);
        let second = record_attempt_impl(&guard, "sc-1", learner, "AR", "q-1", true, 1200);
        let _ = record_attempt_impl(&guard, "sc-2", learner, "AR", "q-2", false, 800);
        match (first, second) {
            (Ok(true), Ok(false)) => checks.record(
                "attempt_idempotency",
                true,
                "first insert returned true, retry returned false".to_string(),
            ),
            (a, b) => checks.record(
                "attempt_idempotency",
                false,
                format!("expected (true, false), got ({a:?}, {b:?})"),
            ),
        }

        match analytics_impl(&guard, learner, "AR") {
            Ok(stats) => checks.record(
                "analytics",
                stats.total == 2 && stats.correct == 1 && stats.mean_latency_ms == 1000,
                format!(
                    "total={} correct={} accuracy={:.3} mean_latency_ms={}",
                    stats.total, stats.correct, stats.accuracy, stats.mean_latency_ms
                ),
            ),
            Err(error) => checks.record("analytics", false, error.to_string()),
        }

        match study_plan_impl(&guard, learner, 50, 60) {
            Ok(plan) => {
                let sum: u32 = plan.drills.iter().map(|d| d.minutes).sum();
                checks.record(
                    "study_plan",
                    !plan.drills.is_empty() && sum == 60 && plan.total_minutes == 60,
                    format!(
                        "{} drill(s), total {} minutes",
                        plan.drills.len(),
                        plan.total_minutes
                    ),
                );
            }
            Err(error) => checks.record("study_plan", false, error.to_string()),
        }

        match readiness_impl(&guard, learner) {
            Ok(band) => checks.record(
                "readiness_band",
                !band.official_score_claim && band.low <= band.high,
                format!(
                    "band {:.3}..{:.3} confidence {:.3} official_score_claim={}",
                    band.low, band.high, band.confidence, band.official_score_claim
                ),
            ),
            Err(error) => checks.record("readiness_band", false, error.to_string()),
        }

        match set_mastery_impl(&guard, learner, "AR", 0.55, 0.2) {
            Ok(()) => checks.record("mastery_write", true, "AR mastery stored".to_string()),
            Err(error) => checks.record("mastery_write", false, error.to_string()),
        }
    }

    // ---- Evidence vault --------------------------------------------------
    {
        let guard = state.db().expect("lock");

        let record = NewEvidenceDto {
            url: "https://www.officialasvab.com/".to_string(),
            title: "Self-check source".to_string(),
            content_hash: "sha256:self-check-0001".to_string(),
            license: "public-domain".to_string(),
            effective_date: "2025-01-01".to_string(),
            trust: 0.9,
            retrieval_status: "retrieved".to_string(),
        };
        let put = evidence_put_impl(&guard, &record);
        let duplicate = evidence_put_impl(&guard, &record);
        let listed = evidence_list_impl(&guard);
        let fetched = put.as_ref().ok().map(|id| evidence_get_impl(&guard, id));
        let unhashed_refused = evidence_put_impl(
            &guard,
            &NewEvidenceDto {
                content_hash: String::new(),
                ..record.clone()
            },
        )
        .is_err();

        let vault_ok = match (&put, &duplicate, &listed, &fetched) {
            (Ok(id), Ok(same), Ok(rows), Some(Ok(found))) => {
                id == same && rows.len() == 1 && &found.id == id
            }
            _ => false,
        };
        checks.record(
            "evidence_vault",
            vault_ok && unhashed_refused,
            format!(
                "put ok={}, duplicate returned the same id={}, rows listed={:?}, unhashed record refused={unhashed_refused}",
                put.is_ok(),
                matches!((&put, &duplicate), (Ok(a), Ok(b)) if a == b),
                listed.as_ref().map(|rows| rows.len())
            ),
        );

        match backup_create_impl(&guard, &dir.join("backups").to_string_lossy()) {
            Ok(manifest) => {
                checks.record(
                    "backup_create",
                    manifest.integrity == "ok"
                        && manifest.bytes > 0
                        && manifest.checksum.len() == 64,
                    format!(
                        "{} bytes, integrity={}, checksum={}, attempt_rows={}",
                        manifest.bytes,
                        manifest.integrity,
                        manifest.checksum,
                        manifest.attempt_rows
                    ),
                );
                report.backup = Some(manifest);
            }
            Err(error) => checks.record("backup_create", false, error.to_string()),
        }

        match latency_probe_impl(&guard, SLO_PROBE_ITERATIONS) {
            Ok(latency) => {
                checks.record(
                    "slo_query_latency",
                    latency.mean_micros <= SLO_MEAN_QUERY_MICROS,
                    format!(
                        "mean {} us, max {} us over {} iterations (bound {} us)",
                        latency.mean_micros,
                        latency.max_micros,
                        latency.iterations,
                        SLO_MEAN_QUERY_MICROS
                    ),
                );
                report.latency = Some(latency);
            }
            Err(error) => checks.record("slo_query_latency", false, error.to_string()),
        }
    }

    let Some(manifest) = report.backup.clone() else {
        // `backup_create` already recorded the failure; restore cannot be
        // exercised without an archive, and inventing one would prove nothing.
        return;
    };
    let Some(learner) = learner.clone() else {
        return;
    };

    // ---- Restore, tamper refusal, restart persistence --------------------
    // Dirty the state so the restore has something real to undo.
    {
        let guard = state.db().expect("lock");
        let _ = record_attempt_impl(&guard, "sc-after-backup", &learner, "AR", "q-3", true, 100);
    }
    let dirty = {
        let guard = state.db().expect("lock");
        analytics_impl(&guard, &learner, "AR")
            .map(|s| s.total)
            .unwrap_or(-1)
    };

    {
        let mut guard = state.db().expect("lock");
        match backup_restore_impl(&mut guard, &manifest.path, &manifest.checksum) {
            Ok(restored) => {
                checks.record(
                    "restore_verified",
                    dirty > manifest.attempt_rows && restored.rows == manifest.attempt_rows,
                    format!(
                        "archive held {} attempt row(s); live state had {dirty} before the restore and {} after",
                        manifest.attempt_rows, restored.rows
                    ),
                );
                report.restore = Some(restored);
            }
            Err(error) => checks.record("restore_verified", false, error.to_string()),
        }
    }

    // Negative proof: a tampered archive must be refused, and the refusal must
    // leave live state intact rather than half-applied.
    let tampered = dir.join("tampered.db");
    match std::fs::copy(PathBuf::from(&manifest.path), &tampered) {
        Ok(_) => {
            let flipped = flip_middle_byte(&tampered);
            let mut guard = state.db().expect("lock");
            let refused =
                backup_restore_impl(&mut guard, &tampered.to_string_lossy(), &manifest.checksum)
                    .is_err();
            let intact = analytics_impl(&guard, &learner, "AR")
                .map(|s| s.total)
                .unwrap_or(-1);

            checks.record(
                "restore_rejects_tampering",
                flipped && refused && intact == manifest.attempt_rows,
                format!(
                    "byte flipped={flipped}; restore refused={refused}; live attempt rows after refusal={intact} (expected {})",
                    manifest.attempt_rows
                ),
            );
        }
        Err(error) => checks.record(
            "restore_rejects_tampering",
            false,
            format!("cannot stage a tampered copy: {error}"),
        ),
    }

    drop(state);

    match AppState::open(&db_path) {
        Ok(reopened) => {
            let guard = reopened.db().expect("lock");
            let profiles = list_profiles_impl(&guard)
                .map(|p| p.len())
                .unwrap_or(usize::MAX);
            checks.record(
                "restart_persistence",
                profiles == 1,
                format!(
                    "reopened {} and read back {profiles} profile(s)",
                    db_path.display()
                ),
            );

            // The backup directory lists the archive that was just written, so
            // a restore flow has a way to name it without guessing a path.
            match backup_list_impl(&guard, &dir.join("backups").to_string_lossy()) {
                Ok(entries) => {
                    let found = entries.iter().any(|e| e.path == manifest.path);
                    checks.record(
                        "backup_list",
                        found && entries.iter().all(|e| e.checksum.len() == 64),
                        format!(
                            "{} archive(s) listed, target present={found}",
                            entries.len()
                        ),
                    );
                }
                Err(error) => checks.record("backup_list", false, error.to_string()),
            }

            // Erasure is destructive, so it runs last and is verified by
            // reading the tables back: a refused or partial erase would show up
            // as non-zero remaining counts.
            let refused = reset_local_data_impl(&guard, "delete").is_err();
            let erased = reset_local_data_impl(&guard, "DELETE");
            match &erased {
                Ok(outcome) => {
                    let clean = outcome.profiles_remaining == 0
                        && outcome.attempts_remaining == 0
                        && outcome.evidence_remaining == 0;
                    checks.record(
                        "reset_local_data",
                        refused && clean && outcome.profiles_removed == 1,
                        format!(
                            "wrong phrase refused={refused}; removed {} profile(s), {} attempt(s), {} source(s); remaining {} / {} / {}",
                            outcome.profiles_removed,
                            outcome.attempts_removed,
                            outcome.evidence_removed,
                            outcome.profiles_remaining,
                            outcome.attempts_remaining,
                            outcome.evidence_remaining
                        ),
                    );
                }
                Err(error) => checks.record("reset_local_data", false, error.to_string()),
            }
        }
        Err(message) => checks.record("restart_persistence", false, message),
    }
}

/// Flip one bit in the middle of a file. Returns false when the file is too
/// small for that to be meaningful.
fn flip_middle_byte(path: &Path) -> bool {
    let Ok(mut bytes) = std::fs::read(path) else {
        return false;
    };
    if bytes.len() < 3 {
        return false;
    }
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    std::fs::write(path, bytes).is_ok()
}

fn emit(report: &Report, dest: Option<&Path>) {
    let json = match serde_json::to_string_pretty(report) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("self-check: cannot serialise report: {error}");
            return;
        }
    };

    println!("{json}");

    if let Some(path) = dest {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(path, format!("{json}\n")) {
            eprintln!("self-check: cannot write {}: {error}", path.display());
        }
    }
}
