//! Tauri command boundary.
//!
//! SPEC-003: "The desktop UI invokes commands through a narrow Tauri boundary."
//! This module is that boundary, and it is deliberately thin.
//!
//! ## Structure
//!
//! Every command has two halves. `<name>_impl` is an ordinary function taking
//! `&Database` and the request arguments; `<name>` is the `#[tauri::command]`
//! wrapper that unwraps the shared state, calls the impl, and flattens the error
//! into the string Tauri can serialize.
//!
//! The split exists because [`State`] cannot be constructed outside a running
//! Tauri application. Without it, the boundary would be reachable only by
//! launching the packaged window — untestable in CI, and untestable code is
//! where stubs survive. With it, `tests/command_boundary.rs` drives the same
//! functions against a real database file, so what is tested is what ships and
//! the remaining wrapper is mechanical.
//!
//! ## Migrations are embedded
//!
//! The schema is compiled into the binary rather than read from disk at
//! runtime. A packaged application has no repository around it, so a relative
//! path would work in development and fail after installation.

use std::sync::{Mutex, MutexGuard};

use tauri::State;
use vector_application::content::{
    ContentManagerDto, ContentPipeline, ContentStatsDto, GenerateRequest, GenerationReportDto,
    ItemDto, ReviewEntryDto,
};
use vector_application::service::{
    AnalyticsDto, BackupDto, BackupEntryDto, EvidenceDto, HealthDto, LatencyDto, MasteryDto,
    PlanDto, ProfileDto, ReadinessDto, ResetDto, RestoreDto, ServiceError, Services,
};
use vector_persistence::{Database, Migration, MigrationManager};

/// The schema, compiled into the binary.
///
/// Version numbers match the filenames in `migrations/`, and the list must stay
/// in ascending order because migrations are applied monotonically.
const EMBEDDED_MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../../../migrations/001_initial.sql")),
    (
        2,
        include_str!("../../../../migrations/002_ep003_persistence.sql"),
    ),
    (
        3,
        include_str!("../../../../migrations/003_content_items.sql"),
    ),
    (
        4,
        include_str!("../../../../migrations/004_content_source_integrity.sql"),
    ),
    (
        5,
        include_str!("../../../../migrations/005_content_passage.sql"),
    ),
    (
        6,
        include_str!("../../../../migrations/006_content_pack_manifest.sql"),
    ),
    (
        7,
        include_str!("../../../../migrations/007_content_pack_items.sql"),
    ),
    (
        8,
        include_str!("../../../../migrations/008_content_question_index.sql"),
    ),
];

/// The migration set the application ships with.
pub fn migrations() -> Vec<Migration> {
    EMBEDDED_MIGRATIONS
        .iter()
        .map(|(version, sql)| (*version, (*sql).to_string()))
        .collect()
}

/// Application state shared across commands.
///
/// `rusqlite::Connection` is not `Sync`, so the handle is guarded by a mutex.
/// Commands therefore serialize on database access, which is correct for a
/// single-user local application and avoids the WAL writer contention that
/// concurrent writes would otherwise cause.
pub struct AppState {
    db: Mutex<Database>,
    data_dir: std::path::PathBuf,
    db_path: std::path::PathBuf,
}

/// Where the application keeps its files, as the UI sees it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppPathsDto {
    pub data_dir: String,
    pub db_path: String,
    /// Where verified backups are written and read from.
    pub backup_dir: String,
}

impl AppState {
    /// Open (or create) the database at `path` and migrate it.
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let mut db = Database::open(path).map_err(|e| format!("cannot open database: {e}"))?;
        MigrationManager::apply(&mut db, &migrations())
            .map_err(|e| format!("cannot migrate database: {e}"))?;

        // Derived from the database path rather than passed in, so there is one
        // source of truth for where this installation's files live.
        let data_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        Ok(Self {
            db: Mutex::new(db),
            data_dir,
            db_path: path.to_path_buf(),
        })
    }

    /// Borrow the database.
    ///
    /// A poisoned mutex means another command panicked while holding the lock.
    /// That is reported rather than ignored, because continuing with a database
    /// in an unknown state would be worse than failing the one request.
    pub fn db(&self) -> Result<MutexGuard<'_, Database>, String> {
        self.db
            .lock()
            .map_err(|_| "database lock poisoned by an earlier failure".to_string())
    }

    /// The directory this installation keeps its files in.
    pub fn paths(&self) -> AppPathsDto {
        AppPathsDto {
            data_dir: self.data_dir.to_string_lossy().into_owned(),
            db_path: self.db_path.to_string_lossy().into_owned(),
            backup_dir: self.data_dir.join("backups").to_string_lossy().into_owned(),
        }
    }
}

/// Report where this installation keeps its files.
#[tauri::command]
pub fn app_paths(state: State<'_, AppState>) -> AppPathsDto {
    state.paths()
}

/// Record that the webview reached the Rust command layer.
///
/// This exists so the webview-to-Rust hop can be *verified* rather than
/// assumed. Rendering the interface and answering a command are different
/// facts, and `AGENTS.md` §9 does not accept the first as proof of the second:
/// a bundle can render while every `invoke` fails, which is exactly what a
/// misconfigured Content-Security-Policy or a missing handler registration
/// would produce.
///
/// The frontend calls this once when it mounts, and the row it writes is the
/// evidence. It carries the frontend build stamp, so the record identifies
/// which bundle made the call.
pub fn ui_ready_impl(db: &Database, bundle: &str) -> Result<String, ServiceError> {
    let stamp = chrono::Utc::now().to_rfc3339();
    let id = format!("ui-ready-{}", uuid::Uuid::new_v4());
    let details = serde_json::json!({
        "bundle": bundle.trim(),
        "recorded_at": stamp,
    })
    .to_string();

    db.connection()
        .execute(
            "INSERT INTO health_diagnostics (id, component, status, details_json)
             VALUES (?1, 'webview', 'ready', ?2)",
            rusqlite::params![id, details],
        )
        .map_err(|e| ServiceError::Storage(e.to_string()))?;

    // Read the row back before reporting success: an insert that was accepted
    // but not persisted must not be reported as a working boundary.
    let readback: String = db
        .connection()
        .query_row(
            "SELECT status FROM health_diagnostics WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(|e| ServiceError::Storage(format!("cannot read back the marker: {e}")))?;

    if readback != "ready" {
        return Err(ServiceError::Storage(
            "the readiness marker did not persist correctly".into(),
        ));
    }

    Ok(id)
}

#[tauri::command]
pub fn ui_ready(state: State<'_, AppState>, bundle: String) -> Result<String, String> {
    with_db(&state, |db| ui_ready_impl(db, &bundle))
}

// ---------------------------------------------------------------------------
// Background work (REQ-054)
// ---------------------------------------------------------------------------

pub fn recompute_mastery_impl(db: &Database, learner_id: &str) -> Result<usize, ServiceError> {
    Services::new(db).recompute_mastery(learner_id)
}

/// Recompute a learner's mastery estimates from their stored attempts.
///
/// Runs on the command thread rather than in a background pool. The recompute is
/// a bounded read of one indexed table per subtest, and a learner waiting for it
/// is waiting for the number it produces; pushing it to a worker would add a
/// failure mode (a job that never reports) in exchange for latency nobody can
/// observe. `vector_application::workers` holds the pool for work that is *not*
/// on a user-visible path.
#[tauri::command]
pub fn recompute_mastery(state: State<'_, AppState>, learner_id: String) -> Result<usize, String> {
    with_db(&state, |db| recompute_mastery_impl(db, &learner_id))
}

/// Run `body` against the shared database, converting errors to strings.
fn with_db<T>(
    state: &State<'_, AppState>,
    body: impl FnOnce(&Database) -> Result<T, ServiceError>,
) -> Result<T, String> {
    let guard = state.db()?;
    body(&guard).map_err(|e| e.to_string())
}

/// Run `body` with exclusive access to the shared database.
///
/// Restore replaces the connection, so it needs `&mut Database` rather than the
/// shared reference `with_db` provides.
fn with_db_mut<T>(
    state: &State<'_, AppState>,
    body: impl FnOnce(&mut Database) -> Result<T, ServiceError>,
) -> Result<T, String> {
    let mut guard = state.db()?;
    body(&mut guard).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Report the application's health. See [`Services::health`].
pub fn health_impl(db: &Database) -> Result<HealthDto, ServiceError> {
    Services::new(db).health()
}

#[tauri::command]
pub fn health(state: State<'_, AppState>) -> Result<HealthDto, String> {
    with_db(&state, health_impl)
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

pub fn create_profile_impl(
    db: &Database,
    name: &str,
    target_score: u32,
) -> Result<ProfileDto, ServiceError> {
    Services::new(db).create_profile(name, target_score)
}

#[tauri::command]
pub fn create_profile(
    state: State<'_, AppState>,
    name: String,
    target_score: u32,
) -> Result<ProfileDto, String> {
    with_db(&state, |db| create_profile_impl(db, &name, target_score))
}

pub fn get_profile_impl(db: &Database, id: &str) -> Result<ProfileDto, ServiceError> {
    Services::new(db).get_profile(id)
}

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>, id: String) -> Result<ProfileDto, String> {
    with_db(&state, |db| get_profile_impl(db, &id))
}

pub fn list_profiles_impl(db: &Database) -> Result<Vec<ProfileDto>, ServiceError> {
    Services::new(db).list_profiles()
}

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ProfileDto>, String> {
    with_db(&state, list_profiles_impl)
}

// ---------------------------------------------------------------------------
// Attempts and analytics
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn record_attempt_impl(
    db: &Database,
    attempt_id: &str,
    learner_id: &str,
    subtest: &str,
    question_id: &str,
    correct: bool,
    latency_ms: i64,
) -> Result<bool, ServiceError> {
    Services::new(db).record_attempt(
        attempt_id,
        learner_id,
        subtest,
        question_id,
        correct,
        latency_ms,
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn record_attempt(
    state: State<'_, AppState>,
    attempt_id: String,
    learner_id: String,
    subtest: String,
    question_id: String,
    correct: bool,
    latency_ms: i64,
) -> Result<bool, String> {
    with_db(&state, |db| {
        record_attempt_impl(
            db,
            &attempt_id,
            &learner_id,
            &subtest,
            &question_id,
            correct,
            latency_ms,
        )
    })
}

pub fn analytics_impl(
    db: &Database,
    learner_id: &str,
    subtest: &str,
) -> Result<AnalyticsDto, ServiceError> {
    Services::new(db).analytics(learner_id, subtest)
}

#[tauri::command]
pub fn analytics(
    state: State<'_, AppState>,
    learner_id: String,
    subtest: String,
) -> Result<AnalyticsDto, String> {
    with_db(&state, |db| analytics_impl(db, &learner_id, &subtest))
}

pub fn analytics_all_impl(
    db: &Database,
    learner_id: &str,
) -> Result<Vec<(String, AnalyticsDto)>, ServiceError> {
    Services::new(db).analytics_all(learner_id)
}

#[tauri::command]
pub fn analytics_all(
    state: State<'_, AppState>,
    learner_id: String,
) -> Result<Vec<(String, AnalyticsDto)>, String> {
    with_db(&state, |db| analytics_all_impl(db, &learner_id))
}

// ---------------------------------------------------------------------------
// Mastery
// ---------------------------------------------------------------------------

pub fn set_mastery_impl(
    db: &Database,
    learner_id: &str,
    subtest: &str,
    score: f64,
    uncertainty: f64,
) -> Result<(), ServiceError> {
    Services::new(db).set_mastery(learner_id, subtest, score, uncertainty)
}

#[tauri::command]
pub fn set_mastery(
    state: State<'_, AppState>,
    learner_id: String,
    subtest: String,
    score: f64,
    uncertainty: f64,
) -> Result<(), String> {
    with_db(&state, |db| {
        set_mastery_impl(db, &learner_id, &subtest, score, uncertainty)
    })
}

pub fn mastery_impl(db: &Database, learner_id: &str) -> Result<Vec<MasteryDto>, ServiceError> {
    Services::new(db).mastery(learner_id)
}

#[tauri::command]
pub fn mastery(state: State<'_, AppState>, learner_id: String) -> Result<Vec<MasteryDto>, String> {
    with_db(&state, |db| mastery_impl(db, &learner_id))
}

// ---------------------------------------------------------------------------
// Planning and readiness
// ---------------------------------------------------------------------------

pub fn study_plan_impl(
    db: &Database,
    learner_id: &str,
    target_score: u32,
    available_minutes: u32,
) -> Result<PlanDto, ServiceError> {
    Services::new(db).study_plan(learner_id, target_score, available_minutes)
}

#[tauri::command]
pub fn study_plan(
    state: State<'_, AppState>,
    learner_id: String,
    target_score: u32,
    available_minutes: u32,
) -> Result<PlanDto, String> {
    with_db(&state, |db| {
        study_plan_impl(db, &learner_id, target_score, available_minutes)
    })
}

pub fn readiness_impl(db: &Database, learner_id: &str) -> Result<ReadinessDto, ServiceError> {
    Services::new(db).readiness(learner_id)
}

#[tauri::command]
pub fn readiness(state: State<'_, AppState>, learner_id: String) -> Result<ReadinessDto, String> {
    with_db(&state, |db| readiness_impl(db, &learner_id))
}

// ---------------------------------------------------------------------------
// Evidence vault (REQ-020)
// ---------------------------------------------------------------------------

pub fn evidence_list_impl(db: &Database) -> Result<Vec<EvidenceDto>, ServiceError> {
    Services::new(db).evidence_list()
}

#[tauri::command]
pub fn evidence_list(state: State<'_, AppState>) -> Result<Vec<EvidenceDto>, String> {
    with_db(&state, evidence_list_impl)
}

pub fn evidence_get_impl(db: &Database, id: &str) -> Result<EvidenceDto, ServiceError> {
    Services::new(db).evidence_get(id)
}

#[tauri::command]
pub fn evidence_get(state: State<'_, AppState>, id: String) -> Result<EvidenceDto, String> {
    with_db(&state, |db| evidence_get_impl(db, &id))
}

pub fn evidence_put_impl(
    db: &Database,
    input: &vector_application::service::NewEvidenceDto,
) -> Result<String, ServiceError> {
    Services::new(db).evidence_put(input)
}

#[tauri::command]
pub fn evidence_put(
    state: State<'_, AppState>,
    input: vector_application::service::NewEvidenceDto,
) -> Result<String, String> {
    with_db(&state, |db| evidence_put_impl(db, &input))
}

// ---------------------------------------------------------------------------
// Backup and restore (REQ-033, REQ-034)
// ---------------------------------------------------------------------------

pub fn backup_create_impl(db: &Database, dest_dir: &str) -> Result<BackupDto, ServiceError> {
    Services::new(db).backup_create(std::path::Path::new(dest_dir))
}

#[tauri::command]
pub fn backup_create(state: State<'_, AppState>, dest_dir: String) -> Result<BackupDto, String> {
    with_db(&state, |db| backup_create_impl(db, &dest_dir))
}

/// Restore live state from a backup.
///
/// `expected_checksum` is required rather than optional. A restore that does
/// not verify the archive against the digest recorded when it was taken can
/// load silently corrupted data, and making the argument mandatory means the UI
/// cannot accidentally offer an unverified restore path.
pub fn backup_restore_impl(
    db: &mut Database,
    source: &str,
    expected_checksum: &str,
) -> Result<RestoreDto, ServiceError> {
    Services::backup_restore(db, std::path::Path::new(source), Some(expected_checksum))
}

#[tauri::command]
pub fn backup_restore(
    state: State<'_, AppState>,
    source: String,
    expected_checksum: String,
) -> Result<RestoreDto, String> {
    with_db_mut(&state, |db| {
        backup_restore_impl(db, &source, &expected_checksum)
    })
}

// ---------------------------------------------------------------------------
// SLO probe (REQ-039)
// ---------------------------------------------------------------------------

pub fn latency_probe_impl(db: &Database, iterations: u32) -> Result<LatencyDto, ServiceError> {
    Services::new(db).latency_probe(iterations)
}

#[tauri::command]
pub fn latency_probe(state: State<'_, AppState>, iterations: u32) -> Result<LatencyDto, String> {
    with_db(&state, |db| latency_probe_impl(db, iterations))
}

// ---------------------------------------------------------------------------
// Existing backups and local erasure (REQ-030, REQ-034, REQ-035)
// ---------------------------------------------------------------------------

pub fn backup_list_impl(db: &Database, dir: &str) -> Result<Vec<BackupEntryDto>, ServiceError> {
    Services::new(db).backup_list(std::path::Path::new(dir))
}

#[tauri::command]
pub fn backup_list(state: State<'_, AppState>, dir: String) -> Result<Vec<BackupEntryDto>, String> {
    with_db(&state, |db| backup_list_impl(db, &dir))
}

pub fn reset_local_data_impl(db: &Database, confirmation: &str) -> Result<ResetDto, ServiceError> {
    Services::new(db).reset_local_data(confirmation)
}

/// Erase every learner record on this device.
///
/// The confirmation phrase is checked in the service layer as well as in the
/// view. A destructive operation must not depend on the caller having rendered
/// a particular input field.
#[tauri::command]
pub fn reset_local_data(
    state: State<'_, AppState>,
    confirmation: String,
) -> Result<ResetDto, String> {
    with_db(&state, |db| reset_local_data_impl(db, &confirmation))
}

// ---------------------------------------------------------------------------
// Content: generation, serving, and corpus statistics (REQ-022, REQ-056)
// ---------------------------------------------------------------------------
//
// Until these existed the practice view rendered three literals from
// `apps/desktop/src/data/sample.ts`, because the frontend had no way to ask the
// backend for a question. The database could hold provable items and nothing
// could reach them.

/// Convert a pipeline failure into a service error.
///
/// Generic over the error rather than taking `anyhow::Error` by name, so the
/// desktop crate does not gain a dependency purely to spell a type it only calls
/// `to_string` on. Every failure the pipeline can produce is a storage or
/// provenance failure rather than a malformed request, so `Storage` is the right
/// side of the distinction the service boundary already draws.
fn content_error<E: std::fmt::Display>(error: E) -> ServiceError {
    ServiceError::Storage(error.to_string())
}

/// Generate original items for a subtest and activate the ones that prove out.
///
/// Idempotent in the useful direction: the same `subtest`, `count` and seed
/// regenerate the same questions, so a second call reports them as
/// `already_present` and adds nothing. `count` is capped because generation
/// writes to the database on the caller's thread and an unbounded request would
/// block the command for an unbounded time.
pub fn content_generate_impl(
    db: &Database,
    subtest: &str,
    count: u32,
    seed: u64,
) -> Result<GenerationReportDto, ServiceError> {
    const MAX_PER_CALL: u32 = 500;
    if count == 0 {
        return Err(ServiceError::Invalid(
            "count must be at least 1".to_string(),
        ));
    }
    if count > MAX_PER_CALL {
        return Err(ServiceError::Invalid(format!(
            "count {count} exceeds the per-call limit of {MAX_PER_CALL}"
        )));
    }

    let pipeline = ContentPipeline::new(db);
    let source_id = pipeline.ensure_construct_source().map_err(content_error)?;
    let request = GenerateRequest {
        subtest,
        count: count as usize,
        seed,
        // The reviewer recorded on activation. A generated item's proof is
        // machine-checked, but REQ-056 still requires a named reviewer, and
        // naming the automated one is more honest than inventing a person.
        reviewer: "machine-verifier",
        source_id: &source_id,
        generator: "factory",
    };
    pipeline
        .generate_and_activate(&request)
        .map(GenerationReportDto::from)
        .map_err(content_error)
}

#[tauri::command]
pub fn content_generate(
    state: State<'_, AppState>,
    subtest: String,
    count: u32,
    seed: u64,
) -> Result<GenerationReportDto, String> {
    with_db(&state, |db| {
        content_generate_impl(db, &subtest, count, seed)
    })
}

/// The next item to practise, skipping ids the caller has already seen.
pub fn content_next_impl(
    db: &Database,
    subtest: &str,
    seen: &[String],
) -> Result<Option<ItemDto>, ServiceError> {
    ContentPipeline::new(db)
        .next_item(subtest, seen)
        .map_err(content_error)
}

#[tauri::command]
pub fn content_next(
    state: State<'_, AppState>,
    subtest: String,
    seen: Vec<String>,
) -> Result<Option<ItemDto>, String> {
    with_db(&state, |db| content_next_impl(db, &subtest, &seen))
}

/// How much content this installation holds, by state and subtest.
pub fn content_stats_impl(db: &Database) -> Result<ContentStatsDto, ServiceError> {
    ContentPipeline::new(db).stats().map_err(content_error)
}

#[tauri::command]
pub fn content_stats(state: State<'_, AppState>) -> Result<ContentStatsDto, String> {
    with_db(&state, content_stats_impl)
}

/// Everything the content manager shows, in one call.
///
/// One command rather than three because the view draws all of it at once, and
/// three round trips could render statistics and a listing that disagree.
pub fn content_manager_impl(db: &Database, limit: u32) -> Result<ContentManagerDto, ServiceError> {
    ContentPipeline::new(db)
        .manager_view(limit as usize)
        .map_err(content_error)
}

#[tauri::command]
pub fn content_manager(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<ContentManagerDto, String> {
    with_db(&state, |db| content_manager_impl(db, limit))
}

/// Withdraw an item from service, recording who did it and why.
pub fn content_quarantine_impl(
    db: &Database,
    item_id: &str,
    actor: &str,
    reason: &str,
) -> Result<(), ServiceError> {
    ContentPipeline::new(db)
        .quarantine_item(item_id, actor, reason)
        .map_err(content_error)
}

#[tauri::command]
pub fn content_quarantine(
    state: State<'_, AppState>,
    item_id: String,
    actor: String,
    reason: String,
) -> Result<(), String> {
    with_db(&state, |db| {
        content_quarantine_impl(db, &item_id, &actor, &reason)
    })
}

/// Return a quarantined item to service along the documented pipeline.
pub fn content_reinstate_impl(
    db: &Database,
    item_id: &str,
    actor: &str,
    reason: &str,
) -> Result<(), ServiceError> {
    ContentPipeline::new(db)
        .reinstate_item(item_id, actor, reason)
        .map_err(content_error)
}

#[tauri::command]
pub fn content_reinstate(
    state: State<'_, AppState>,
    item_id: String,
    actor: String,
    reason: String,
) -> Result<(), String> {
    with_db(&state, |db| {
        content_reinstate_impl(db, &item_id, &actor, &reason)
    })
}

/// One item's audit trail.
pub fn content_history_impl(
    db: &Database,
    item_id: &str,
) -> Result<Vec<ReviewEntryDto>, ServiceError> {
    ContentPipeline::new(db)
        .item_history(item_id)
        .map_err(content_error)
}

#[tauri::command]
pub fn content_history(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<Vec<ReviewEntryDto>, String> {
    with_db(&state, |db| content_history_impl(db, &item_id))
}

// ---------------------------------------------------------------------------
// Content packs (REQ-023, REQ-032)
// ---------------------------------------------------------------------------

/// Every pack the registry holds.
pub fn content_packs_impl(
    db: &Database,
) -> Result<Vec<vector_application::packs::InstalledPackDto>, ServiceError> {
    vector_application::packs::installed_packs(db).map_err(content_error)
}

#[tauri::command]
pub fn content_packs(
    state: State<'_, AppState>,
) -> Result<Vec<vector_application::packs::InstalledPackDto>, String> {
    with_db(&state, content_packs_impl)
}

/// Install a pack file, after every check the installer performs.
///
/// The trusted signer is application configuration rather than a command argument: a
/// caller that could name the key it trusts could install a pack it signed itself,
/// which is the one thing the signature is there to prevent. It is read from the
/// environment at startup and defaults to absent, in which case installation refuses
/// every pack and says so.
pub fn content_pack_install_impl(
    db: &Database,
    pack_path: &str,
    trusted_signer: Option<&str>,
    app_version: &str,
) -> Result<vector_application::packs::InstallReport, ServiceError> {
    let trusted = trusted_signer.ok_or_else(|| {
        ServiceError::Invalid(
            "no pack signing key is configured, so no pack can be verified; set \
             VECTOR_PACK_TRUSTED_SIGNER to the public key this installation trusts"
                .to_string(),
        )
    })?;
    let signer = decode_hex_key(trusted).ok_or_else(|| {
        ServiceError::Invalid(
            "the configured pack signing key is not 64 hex characters".to_string(),
        )
    })?;
    let path = std::path::Path::new(pack_path);
    let bytes = std::fs::read(path).map_err(|error| {
        ServiceError::Invalid(format!("cannot read the pack at {pack_path}: {error}"))
    })?;
    vector_application::packs::install_pack(db, &bytes, &signer, app_version).map_err(content_error)
}

#[tauri::command]
pub fn content_pack_install(
    state: State<'_, AppState>,
    pack_path: String,
) -> Result<vector_application::packs::InstallReport, String> {
    let trusted = std::env::var(PACK_TRUSTED_SIGNER_ENV).ok();
    with_db(&state, |db| {
        content_pack_install_impl(db, &pack_path, trusted.as_deref(), APP_VERSION)
    })
}

/// Withdraw the active version of a pack in favour of the previous one.
pub fn content_pack_rollback_impl(
    db: &Database,
    name: &str,
) -> Result<vector_application::packs::InstalledPackDto, ServiceError> {
    vector_application::packs::rollback_pack(db, name).map_err(content_error)
}

#[tauri::command]
pub fn content_pack_rollback(
    state: State<'_, AppState>,
    name: String,
) -> Result<vector_application::packs::InstalledPackDto, String> {
    with_db(&state, |db| content_pack_rollback_impl(db, &name))
}

/// The environment variable naming the public key an installation trusts for packs.
pub const PACK_TRUSTED_SIGNER_ENV: &str = "VECTOR_PACK_TRUSTED_SIGNER";

/// The version this build reports for the pack compatibility check.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Decode 64 hex characters into the 32 bytes of an Ed25519 public key.
fn decode_hex_key(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
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
    if out.len() == 32 {
        Some(out)
    } else {
        None
    }
}
