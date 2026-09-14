//! Application services: the typed boundary the desktop UI invokes.
//!
//! SPEC-003: "Internal application services expose typed Rust traits. The
//! desktop UI invokes commands through a narrow Tauri boundary."
//!
//! This module is that service layer. It owns the real behaviour — profile
//! lifecycle, attempt recording, analytics, plan generation, readiness — and the
//! Tauri commands in `apps/desktop/src-tauri` are thin wrappers over these
//! functions.
//!
//! ## Why the logic lives here rather than in the Tauri crate
//!
//! A binary crate cannot be imported by an integration test, so logic written
//! inside `src-tauri` is reachable only by launching the packaged application.
//! In a headless environment that means it is effectively untestable, and
//! untestable code is where stubs survive. Keeping the behaviour here makes it
//! directly testable while leaving the Tauri layer as a thin, mechanical
//! adapter with nothing to get wrong.
//!
//! ## Validation
//!
//! Every entry point validates its input and returns a typed error. The Tauri
//! boundary is reachable from the webview, so it is a trust boundary: a
//! malformed request must be refused here rather than corrupting state.

use serde::{Deserialize, Serialize};

use vector_persistence::backup::{BackupManager, BackupManifest};
use vector_persistence::repo::{
    AttemptRepo, AttemptStats, EvidenceRecord, EvidenceRepo, MasteryRecord, MasteryRepo,
};
use vector_persistence::{Database, MigrationManager};
use vector_study::selection::{
    generate_plan, readiness_band, PlanGoal, ReadinessBand, SkillEstimate, StudyPlan,
};

/// Errors surfaced to the caller.
///
/// A flat enum rather than a stringly-typed error, so the UI can distinguish an
/// invalid request from a storage failure instead of rendering both the same
/// way.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceError {
    /// The request failed validation.
    Invalid(String),
    /// The requested entity does not exist.
    NotFound(String),
    /// The storage layer failed.
    Storage(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Invalid(msg) => write!(f, "invalid request: {msg}"),
            ServiceError::NotFound(what) => write!(f, "not found: {what}"),
            ServiceError::Storage(msg) => write!(f, "storage error: {msg}"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<anyhow::Error> for ServiceError {
    fn from(error: anyhow::Error) -> Self {
        ServiceError::Storage(error.to_string())
    }
}

impl From<rusqlite::Error> for ServiceError {
    fn from(error: rusqlite::Error) -> Self {
        ServiceError::Storage(error.to_string())
    }
}

/// A learner profile as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileDto {
    pub id: String,
    pub name: String,
    pub target_score: u32,
}

/// A mastery estimate as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasteryDto {
    pub subtest: String,
    pub score: f64,
    pub uncertainty: f64,
}

/// A single planned drill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillDto {
    pub subtest: String,
    pub minutes: u32,
    pub reason: String,
}

/// A study plan as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanDto {
    pub drills: Vec<DrillDto>,
    pub total_minutes: u32,
}

/// A readiness band as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadinessDto {
    pub low: f64,
    pub high: f64,
    pub confidence: f64,
    /// Always false. ADR-010 forbids a precise predicted official score before a
    /// calibration cohort exists, and this field exists so the UI cannot invent
    /// one without the lie being visible in the payload.
    pub official_score_claim: bool,
}

/// Aggregate performance for one subtest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticsDto {
    pub total: i64,
    pub correct: i64,
    pub accuracy: f64,
    pub mean_latency_ms: i64,
}

/// A health report as the UI sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthDto {
    /// The component being reported on.
    pub component: String,
    /// Whether the component is healthy.
    pub healthy: bool,
    /// How the verdict was reached. Always `operation_succeeded`: a running
    /// process is not a health proof, so the check performs real work.
    pub basis: String,
    /// Number of stored profiles, which proves the schema is present and
    /// queryable rather than merely declared.
    pub profiles: i64,
}

/// A stored source snapshot as the UI sees it (REQ-020).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceDto {
    pub id: String,
    pub content_hash: String,
    pub url: String,
    pub title: String,
    pub license: String,
    pub effective_date: String,
    pub trust: f64,
    pub retrieval_status: String,
    pub created_at: String,
}

impl From<EvidenceRecord> for EvidenceDto {
    fn from(r: EvidenceRecord) -> Self {
        Self {
            id: r.id,
            content_hash: r.content_hash,
            url: r.url,
            title: r.title,
            license: r.license,
            effective_date: r.effective_date,
            trust: r.trust,
            retrieval_status: r.retrieval_status,
            created_at: r.created_at,
        }
    }
}

/// A source snapshot to store in the vault.
///
/// A named-field struct rather than seven positional strings, two of which are
/// hashes and two of which are dates: positionally swapped arguments would
/// corrupt provenance with no type error to catch it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewEvidenceDto {
    pub url: String,
    pub title: String,
    pub content_hash: String,
    pub license: String,
    pub effective_date: String,
    pub trust: f64,
    pub retrieval_status: String,
}

/// A completed backup as the UI sees it (REQ-033, REQ-034).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupDto {
    pub path: String,
    /// SHA-256 of the archive. The UI shows it so a restore can be checked
    /// against the digest recorded when the backup was taken.
    pub checksum: String,
    pub bytes: u64,
    pub integrity: String,
    pub encrypted: bool,
    pub attempt_rows: i64,
}

impl From<BackupManifest> for BackupDto {
    fn from(m: BackupManifest) -> Self {
        Self {
            path: m.path.to_string_lossy().into_owned(),
            checksum: m.checksum,
            bytes: m.bytes,
            integrity: m.integrity,
            encrypted: m.encrypted,
            attempt_rows: m.attempt_rows,
        }
    }
}

/// The result of a restore (REQ-034).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoreDto {
    /// Attempt rows present in the database after the restore, read back from
    /// live state rather than reported from the archive.
    pub rows: i64,
}

/// An archive found on disk, offered for restore (REQ-034).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupEntryDto {
    pub path: String,
    pub checksum: String,
    pub bytes: u64,
    pub modified: String,
}

/// The outcome of erasing all local learner data (REQ-035, REQ-030).
///
/// Reports both what was removed and what remains. The remaining counts are the
/// ones that matter: they are read back from live state, so a deletion that
/// silently failed cannot be reported as a success.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetDto {
    pub profiles_removed: i64,
    pub attempts_removed: i64,
    pub evidence_removed: i64,
    pub profiles_remaining: i64,
    pub attempts_remaining: i64,
    pub evidence_remaining: i64,
}

/// The literal a caller must supply to erase local data.
///
/// Checked in the service layer, not only in the view. The typed confirmation
/// in the UI is a courtesy to the learner; this constant is the guard, so a
/// malformed request from anywhere on the boundary cannot destroy data.
pub const RESET_CONFIRMATION_PHRASE: &str = "DELETE";

/// A measured latency sample used for the artifact SLO probe (REQ-039).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatencyDto {
    pub operation: String,
    pub iterations: u32,
    pub mean_micros: u64,
    pub max_micros: u64,
}

/// The application service layer, bound to one database.
pub struct Services<'a> {
    db: &'a Database,
}
impl<'a> Services<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Migrate a database to the current schema.
    pub fn migrate(
        db: &mut Database,
        migrations: &[vector_persistence::Migration],
    ) -> Result<usize, ServiceError> {
        Ok(MigrationManager::apply(db, migrations)?)
    }

    // -----------------------------------------------------------------------
    // Health
    // -----------------------------------------------------------------------

    /// Report database health by doing real work.
    ///
    /// `OBSERVABILITY.md` states that process liveness is not a health proof, so
    /// this runs SQLite's own integrity check and a real query against the
    /// migrated schema. A database that opens but whose schema is missing fails
    /// here, which is the failure mode that matters after a botched upgrade.
    pub fn health(&self) -> Result<HealthDto, ServiceError> {
        let integrity = self
            .db
            .integrity_check()
            .map_err(|e| ServiceError::Storage(e.to_string()))?;
        let profiles: i64 = self
            .db
            .connection()
            .query_row("SELECT COUNT(*) FROM learner_profile", [], |row| row.get(0))
            .map_err(|e| ServiceError::Storage(format!("schema is not usable: {e}")))?;

        Ok(HealthDto {
            component: "database".to_string(),
            healthy: integrity,
            basis: "operation_succeeded".to_string(),
            profiles,
        })
    }

    // -----------------------------------------------------------------------
    // Profiles
    // -----------------------------------------------------------------------

    /// Create a learner profile.
    pub fn create_profile(
        &self,
        name: &str,
        target_score: u32,
    ) -> Result<ProfileDto, ServiceError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ServiceError::Invalid("a learner name is required".into()));
        }
        // The ASVAB reports composite scores on the 1..99 scale, so a target
        // outside it cannot be met and is almost certainly a caller error.
        if !(1..=99).contains(&target_score) {
            return Err(ServiceError::Invalid(format!(
                "target score {target_score} is outside the 1..99 reporting scale"
            )));
        }

        let id = format!("learner-{}", uuid::Uuid::new_v4());
        self.db
            .connection()
            .execute(
                "INSERT INTO learner_profile (id, name, target_score) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, trimmed, target_score],
            )
            .map_err(|e| ServiceError::Storage(e.to_string()))?;

        Ok(ProfileDto {
            id,
            name: trimmed.to_string(),
            target_score,
        })
    }

    /// Look up a profile.
    pub fn get_profile(&self, id: &str) -> Result<ProfileDto, ServiceError> {
        self.db
            .connection()
            .query_row(
                "SELECT id, name, target_score FROM learner_profile WHERE id = ?1",
                [id],
                |row| {
                    Ok(ProfileDto {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        target_score: row.get(2)?,
                    })
                },
            )
            .map_err(|_| ServiceError::NotFound(format!("profile {id}")))
    }

    /// List every profile.
    pub fn list_profiles(&self) -> Result<Vec<ProfileDto>, ServiceError> {
        let conn = self.db.connection();
        let mut stmt = conn
            .prepare("SELECT id, name, target_score FROM learner_profile ORDER BY name")
            .map_err(|e| ServiceError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ProfileDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    target_score: row.get(2)?,
                })
            })
            .map_err(|e| ServiceError::Storage(e.to_string()))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // -----------------------------------------------------------------------
    // Attempts and analytics
    // -----------------------------------------------------------------------

    /// Record a practice attempt.
    ///
    /// Returns `true` when the row was inserted and `false` when the same
    /// attempt id had already been recorded, which is what makes a retried
    /// submission idempotent rather than producing duplicate history.
    pub fn record_attempt(
        &self,
        attempt_id: &str,
        learner_id: &str,
        subtest: &str,
        question_id: &str,
        correct: bool,
        latency_ms: i64,
    ) -> Result<bool, ServiceError> {
        if attempt_id.trim().is_empty() {
            return Err(ServiceError::Invalid("an attempt id is required".into()));
        }
        if latency_ms < 0 {
            return Err(ServiceError::Invalid("latency cannot be negative".into()));
        }
        // Fail early with a clear message rather than surfacing a foreign-key
        // violation from SQLite.
        self.get_profile(learner_id)?;

        Ok(AttemptRepo::new(self.db).record_idempotent(
            attempt_id,
            learner_id,
            subtest,
            question_id,
            correct,
            latency_ms,
        )?)
    }

    /// Aggregate analytics for a learner and subtest.
    pub fn analytics(&self, learner_id: &str, subtest: &str) -> Result<AnalyticsDto, ServiceError> {
        let stats: AttemptStats = AttemptRepo::new(self.db).analytics(learner_id, subtest)?;
        Ok(AnalyticsDto {
            total: stats.total,
            correct: stats.correct,
            accuracy: stats.accuracy,
            mean_latency_ms: stats.mean_latency_ms,
        })
    }

    /// Every subtest the learner has attempted, with its aggregate.
    pub fn analytics_all(
        &self,
        learner_id: &str,
    ) -> Result<Vec<(String, AnalyticsDto)>, ServiceError> {
        let subtests = vector_domain::mastery::Subtest::ALL;
        let mut out = Vec::new();
        for subtest in subtests {
            let dto = self.analytics(learner_id, subtest.code())?;
            if dto.total > 0 {
                out.push((subtest.code().to_string(), dto));
            }
        }
        Ok(out)
    }

    // -----------------------------------------------------------------------
    // Mastery
    // -----------------------------------------------------------------------

    /// Record a mastery estimate for a subtest.
    pub fn set_mastery(
        &self,
        learner_id: &str,
        subtest: &str,
        score: f64,
        uncertainty: f64,
    ) -> Result<(), ServiceError> {
        if !(0.0..=1.0).contains(&score) {
            return Err(ServiceError::Invalid(format!(
                "mastery {score} is outside [0,1]"
            )));
        }
        if uncertainty < 0.0 {
            return Err(ServiceError::Invalid(
                "uncertainty cannot be negative".into(),
            ));
        }
        MasteryRepo::new(self.db).upsert(learner_id, subtest, score, uncertainty)?;
        Ok(())
    }

    /// Read every mastery estimate for a learner.
    pub fn mastery(&self, learner_id: &str) -> Result<Vec<MasteryDto>, ServiceError> {
        let records: Vec<MasteryRecord> = MasteryRepo::new(self.db).all_for(learner_id)?;
        Ok(records
            .into_iter()
            .map(|r| MasteryDto {
                subtest: r.subtest,
                score: r.score,
                uncertainty: r.uncertainty,
            })
            .collect())
    }

    /// Recompute a learner's mastery estimates from their recorded attempts.
    ///
    /// ## Why this exists
    ///
    /// Mastery is what the planner reads, so a mastery value that only ever
    /// changes when something writes it by hand drifts away from the evidence
    /// behind it. This derives mastery from the attempt history, which makes it
    /// a statement about what the learner has actually done.
    ///
    /// ## The estimate
    ///
    /// `score = (correct + 1) / (total + 2)` — a Laplace prior. With no attempts
    /// it yields 0.5 rather than 0 or 1: "no evidence" is the honest position,
    /// and 0.5 with maximum uncertainty says that, whereas 0 would read as
    /// demonstrated failure.
    ///
    /// `uncertainty = 1 / sqrt(total + 1)`, clamped to the `[0,1]` range the
    /// storage layer enforces. It falls as evidence accumulates and never
    /// reaches zero, because an estimate from a finite sample is never certain.
    ///
    /// A subtest with no attempts is left untouched, so an estimate someone set
    /// deliberately is not overwritten by an absence of data.
    ///
    /// This is a study aid, not a psychometric model: it makes no equivalence
    /// claim about any official score (REQ-049, ADR-010).
    ///
    /// Returns the number of mastery rows written.
    pub fn recompute_mastery(&self, learner_id: &str) -> Result<usize, ServiceError> {
        self.get_profile(learner_id)?;

        let repo = AttemptRepo::new(self.db);
        let mastery = MasteryRepo::new(self.db);
        let mut written = 0usize;

        for subtest in vector_domain::mastery::Subtest::ALL {
            let code = subtest.code();
            let stats = repo.analytics(learner_id, code)?;
            if stats.total == 0 {
                continue;
            }

            let total = stats.total as f64;
            let correct = stats.correct as f64;
            let score = (correct + 1.0) / (total + 2.0);
            let uncertainty = (1.0 / (total + 1.0).sqrt()).clamp(0.0, 1.0);

            mastery.upsert(learner_id, code, score, uncertainty)?;
            written += 1;
        }

        Ok(written)
    }

    // -----------------------------------------------------------------------
    // Planning and readiness
    // -----------------------------------------------------------------------

    /// Estimates the planner needs, derived from stored state.
    ///
    /// A subtest the learner has never studied is reported as mastery 0 with
    /// maximum uncertainty, which is the honest starting position: nothing is
    /// known, so the planner should look there.
    fn skill_estimates(&self, learner_id: &str) -> Result<Vec<SkillEstimate>, ServiceError> {
        let recorded: Vec<MasteryDto> = self.mastery(learner_id)?;
        let mut estimates = Vec::new();

        for subtest in vector_domain::mastery::Subtest::ALL {
            let code = subtest.code();
            let existing = recorded.iter().find(|m| m.subtest == code);

            // Due reviews come from attempts recorded in this subtest, which is
            // the only scheduling signal the persistence layer currently holds.
            let attempts = self.analytics(learner_id, code)?;
            let due = if attempts.total == 0 { 0 } else { 1 };

            estimates.push(SkillEstimate {
                subtest: code.to_string(),
                mastery: existing.map(|m| m.score).unwrap_or(0.0),
                uncertainty: existing.map(|m| m.uncertainty).unwrap_or(1.0),
                due_reviews: due,
            });
        }

        Ok(estimates)
    }

    /// Generate a study plan for the available time.
    pub fn study_plan(
        &self,
        learner_id: &str,
        target_score: u32,
        available_minutes: u32,
    ) -> Result<PlanDto, ServiceError> {
        if !(1..=99).contains(&target_score) {
            return Err(ServiceError::Invalid(format!(
                "target score {target_score} is outside the 1..99 reporting scale"
            )));
        }
        if available_minutes == 0 {
            return Err(ServiceError::Invalid(
                "available time must be greater than zero".into(),
            ));
        }
        self.get_profile(learner_id)?;

        let estimates = self.skill_estimates(learner_id)?;
        let plan: StudyPlan =
            generate_plan(&estimates, &PlanGoal::Afqt(target_score), available_minutes)
                .map_err(ServiceError::Invalid)?;

        Ok(PlanDto {
            drills: plan
                .drills
                .iter()
                .map(|d| DrillDto {
                    subtest: d.subtest.clone(),
                    minutes: d.minutes,
                    reason: format!("{:?}", d.reason).to_lowercase(),
                })
                .collect(),
            total_minutes: plan.total_minutes,
        })
    }

    /// Estimate a readiness band (REQ-011).
    ///
    /// Returns a band, never a point score, and never an official score claim.
    pub fn readiness(&self, learner_id: &str) -> Result<ReadinessDto, ServiceError> {
        self.get_profile(learner_id)?;
        let estimates = self.skill_estimates(learner_id)?;
        let band: ReadinessBand = readiness_band(&estimates).map_err(ServiceError::Invalid)?;

        Ok(ReadinessDto {
            low: band.low,
            high: band.high,
            confidence: band.confidence,
            official_score_claim: band.official_score_claim,
        })
    }

    // -----------------------------------------------------------------------
    // Evidence vault (REQ-020)
    // -----------------------------------------------------------------------

    /// Every stored source snapshot, newest first.
    pub fn evidence_list(&self) -> Result<Vec<EvidenceDto>, ServiceError> {
        let records: Vec<EvidenceRecord> = EvidenceRepo::new(self.db).list()?;
        Ok(records.into_iter().map(EvidenceDto::from).collect())
    }

    /// Fetch one stored snapshot by id.
    pub fn evidence_get(&self, id: &str) -> Result<EvidenceDto, ServiceError> {
        EvidenceRepo::new(self.db)
            .get(id)?
            .map(EvidenceDto::from)
            .ok_or_else(|| ServiceError::NotFound(format!("evidence {id}")))
    }

    /// Store a source snapshot, returning its vault id.
    ///
    /// The vault is hash-addressed, so re-storing identical content returns the
    /// existing id rather than creating a second record — provenance must not
    /// fork just because a source was retrieved twice.
    pub fn evidence_put(&self, input: &NewEvidenceDto) -> Result<String, ServiceError> {
        // A record with no URL or no content hash cannot be re-verified later,
        // which is the only thing that makes it evidence rather than a note.
        if input.url.trim().is_empty() {
            return Err(ServiceError::Invalid("a source url is required".into()));
        }
        if input.title.trim().is_empty() {
            return Err(ServiceError::Invalid("a source title is required".into()));
        }
        if input.content_hash.trim().is_empty() {
            return Err(ServiceError::Invalid(
                "a content hash is required; an unhashed snapshot cannot be verified".into(),
            ));
        }
        if !(0.0..=1.0).contains(&input.trust) {
            return Err(ServiceError::Invalid(format!(
                "trust {} is outside [0,1]",
                input.trust
            )));
        }

        let record = vector_persistence::repo::NewEvidence::new(
            input.url.trim(),
            input.title.trim(),
            input.content_hash.trim(),
            input.license.trim(),
            input.effective_date.trim(),
            input.trust,
            input.retrieval_status.trim(),
        );
        Ok(EvidenceRepo::new(self.db).put(&record)?)
    }

    // -----------------------------------------------------------------------
    // Backup and restore (REQ-033, REQ-034)
    // -----------------------------------------------------------------------

    /// Write an integrity-checked backup into `dest_dir`.
    ///
    /// The directory is created if it does not exist; the filename is derived
    /// from the wall clock so successive backups do not overwrite one another.
    pub fn backup_create(&self, dest_dir: &std::path::Path) -> Result<BackupDto, ServiceError> {
        std::fs::create_dir_all(dest_dir).map_err(|e| {
            ServiceError::Storage(format!("cannot create {}: {e}", dest_dir.display()))
        })?;

        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let dest = dest_dir.join(format!("vector-{stamp}.db"));

        let manifest = BackupManager::create(self.db, &dest)?;
        Ok(BackupDto::from(manifest))
    }

    /// Restore live state from a backup (REQ-034).
    ///
    /// An associated function rather than a method because the connection is
    /// replaced, which needs exclusive access to the `Database`.
    ///
    /// `expected_checksum` should be the digest recorded when the backup was
    /// taken; supplying it is what makes tampering detectable, because SQLite's
    /// structural `integrity_check` cannot see a flipped free-page byte.
    pub fn backup_restore(
        db: &mut Database,
        source: &std::path::Path,
        expected_checksum: Option<&str>,
    ) -> Result<RestoreDto, ServiceError> {
        if !source.exists() {
            return Err(ServiceError::NotFound(format!(
                "backup {}",
                source.display()
            )));
        }
        BackupManager::restore_verified(db, source, expected_checksum)?;

        // Read the effect back from live state. Reporting the archive's own row
        // count would prove nothing about what was actually loaded.
        let rows: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM attempts", [], |row| row.get(0))
            .map_err(|e| ServiceError::Storage(format!("restored database is unusable: {e}")))?;

        Ok(RestoreDto { rows })
    }

    /// Archive files present in `dir`, newest first.
    ///
    /// A restore needs to name an archive, and a learner cannot be expected to
    /// type a filesystem path from memory. Each entry carries its digest so the
    /// restore can verify the archive against the digest taken when it was
    /// written, rather than trusting the file it happens to find.
    pub fn backup_list(&self, dir: &std::path::Path) -> Result<Vec<BackupEntryDto>, ServiceError> {
        if !dir.exists() {
            // No directory yet is an empty list, not an error: nothing has been
            // backed up, which is a normal state on a new installation.
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(dir)
            .map_err(|e| ServiceError::Storage(format!("cannot read {}: {e}", dir.display())))?;

        let mut out: Vec<BackupEntryDto> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| ServiceError::Storage(e.to_string()))?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("db") {
                continue;
            }
            let metadata =
                std::fs::metadata(&path).map_err(|e| ServiceError::Storage(e.to_string()))?;
            let modified = metadata
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            out.push(BackupEntryDto {
                path: path.to_string_lossy().into_owned(),
                checksum: BackupManager::sha256_file(&path)?,
                bytes: metadata.len(),
                modified,
            });
        }

        // A total order, so the list does not reshuffle between reads.
        out.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| a.path.cmp(&b.path))
        });
        Ok(out)
    }

    /// Erase every learner record on this device (REQ-035, REQ-030).
    ///
    /// Irreversible by design, so it is guarded three ways: the caller must
    /// supply the exact phrase, the whole deletion runs in one transaction so a
    /// partial erase cannot be left behind, and the result reports what remains
    /// read back from live state.
    ///
    /// The content-pack and evidence tables are included: "all local data" has
    /// to mean all of it, and a vault row that survived a deletion the learner
    /// asked for would be a privacy failure, not a convenience.
    pub fn reset_local_data(&self, confirmation: &str) -> Result<ResetDto, ServiceError> {
        if confirmation != RESET_CONFIRMATION_PHRASE {
            return Err(ServiceError::Invalid(format!(
                "erasing local data requires the exact confirmation phrase {:?}",
                RESET_CONFIRMATION_PHRASE
            )));
        }

        let conn = self.db.connection();
        let count = |table: &str| -> Result<i64, ServiceError> {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|e| ServiceError::Storage(e.to_string()))
        };

        let profiles_removed = count("learner_profile")?;
        let attempts_removed = count("attempts")?;
        let evidence_removed = count("evidence_records")?;

        let tx = conn
            .unchecked_transaction()
            .map_err(|e| ServiceError::Storage(e.to_string()))?;
        // Children before parents: the foreign keys cascade, but deleting in
        // dependency order means the intent is legible and does not rely on
        // `PRAGMA foreign_keys` being on for correctness.
        for table in [
            "attempts",
            "mastery",
            "learner_profile",
            "evidence_records",
            "pull_requests",
            "content_packs",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(|e| ServiceError::Storage(format!("cannot clear {table}: {e}")))?;
        }
        tx.commit()
            .map_err(|e| ServiceError::Storage(e.to_string()))?;

        // Reclaim the space so the erased content is not left readable in free
        // pages. VACUUM cannot run inside a transaction, hence after the commit.
        conn.execute_batch("VACUUM")
            .map_err(|e| ServiceError::Storage(format!("cannot compact the database: {e}")))?;

        Ok(ResetDto {
            profiles_removed,
            attempts_removed,
            evidence_removed,
            profiles_remaining: count("learner_profile")?,
            attempts_remaining: count("attempts")?,
            evidence_remaining: count("evidence_records")?,
        })
    }

    // -----------------------------------------------------------------------
    // SLO probe (REQ-039)
    // -----------------------------------------------------------------------    /// Measure real query latency against the live database.
    ///
    /// REQ-039 tracks service-level objectives, and an objective measured
    /// against a mock or a synthetic benchmark says nothing about the shipped
    /// artifact. This runs the same read the dashboard runs, against whatever
    /// database it is given, and reports the observed distribution.
    pub fn latency_probe(&self, iterations: u32) -> Result<LatencyDto, ServiceError> {
        if iterations == 0 {
            return Err(ServiceError::Invalid(
                "the probe needs at least one iteration".into(),
            ));
        }

        let mut samples: Vec<u64> = Vec::with_capacity(iterations as usize);
        for _ in 0..iterations {
            let started = std::time::Instant::now();
            self.db
                .connection()
                .query_row("SELECT COUNT(*) FROM learner_profile", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|e| ServiceError::Storage(e.to_string()))?;
            samples.push(started.elapsed().as_micros() as u64);
        }

        let total: u64 = samples.iter().sum();
        Ok(LatencyDto {
            operation: "select_count_learner_profile".to_string(),
            iterations,
            mean_micros: total / u64::from(iterations),
            max_micros: samples.iter().copied().max().unwrap_or(0),
        })
    }
}
