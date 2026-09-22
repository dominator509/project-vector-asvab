//! Durable repositories over the EP-003 schema.
//!
//! Requirements: REQ-010 (offline analytics), REQ-020 (evidence vault),
//! REQ-032 (pull requests), REQ-054 (idempotent attempts).
//!
//! Every method here issues real SQL against the live connection; there is no
//! in-memory shadow state, so what these return is what a restart reads back.

use rusqlite::{params, OptionalExtension};

use crate::db::Database;

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(crate) fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}

// ---------------------------------------------------------------------------
// Attempts (REQ-010, REQ-054)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub id: String,
    pub learner_id: String,
    pub subtest: String,
    pub question_id: String,
    pub correct: bool,
    pub latency_ms: i64,
    pub created_at: String,
}

/// Offline mastery/speed/confidence aggregate for one learner and subtest.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptStats {
    pub total: i64,
    pub correct: i64,
    pub accuracy: f64,
    pub mean_latency_ms: i64,
}

pub struct AttemptRepo<'a> {
    db: &'a Database,
}

impl<'a> AttemptRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Record an attempt with a generated identity.
    pub fn record(
        &self,
        learner_id: &str,
        subtest: &str,
        question_id: &str,
        correct: bool,
        latency_ms: i64,
    ) -> anyhow::Result<String> {
        let id = new_id("attempt");
        self.record_with_id(&id, learner_id, subtest, question_id, correct, latency_ms)?;
        Ok(id)
    }

    /// Record an attempt under a caller-supplied identity.
    ///
    /// Returns `true` when the row was inserted. A duplicate primary key means
    /// this logical attempt was already recorded (client retry, double submit,
    /// replayed worker job), so the write is a no-op and `false` is returned.
    /// This is what makes submission idempotent rather than duplicating data.
    pub fn record_idempotent(
        &self,
        attempt_id: &str,
        learner_id: &str,
        subtest: &str,
        question_id: &str,
        correct: bool,
        latency_ms: i64,
    ) -> anyhow::Result<bool> {
        let changed = self.db.connection().execute(
            "INSERT OR IGNORE INTO attempts
                (id, learner_id, subtest, question_id, correct, latency_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                attempt_id,
                learner_id,
                subtest,
                question_id,
                i64::from(correct),
                latency_ms,
                now()
            ],
        )?;
        Ok(changed == 1)
    }

    fn record_with_id(
        &self,
        id: &str,
        learner_id: &str,
        subtest: &str,
        question_id: &str,
        correct: bool,
        latency_ms: i64,
    ) -> anyhow::Result<()> {
        self.db.connection().execute(
            "INSERT INTO attempts
                (id, learner_id, subtest, question_id, correct, latency_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                learner_id,
                subtest,
                question_id,
                i64::from(correct),
                latency_ms,
                now()
            ],
        )?;
        Ok(())
    }

    /// Aggregate stored attempts for offline analytics (REQ-010).
    ///
    /// Computed in SQL over persisted rows, so the numbers reflect the real
    /// database rather than a cached counter.
    pub fn analytics(&self, learner_id: &str, subtest: &str) -> anyhow::Result<AttemptStats> {
        let (total, correct, mean_latency): (i64, i64, Option<f64>) =
            self.db.connection().query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(correct), 0),
                        AVG(latency_ms)
                 FROM attempts
                 WHERE learner_id = ?1 AND subtest = ?2",
                params![learner_id, subtest],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;

        let accuracy = if total == 0 {
            0.0
        } else {
            correct as f64 / total as f64
        };
        Ok(AttemptStats {
            total,
            correct,
            accuracy,
            mean_latency_ms: mean_latency.unwrap_or(0.0).round() as i64,
        })
    }

    pub fn list(&self, learner_id: &str, subtest: &str) -> anyhow::Result<Vec<Attempt>> {
        let conn = self.db.connection();
        let mut stmt = conn.prepare(
            "SELECT id, learner_id, subtest, question_id, correct, latency_ms, created_at
             FROM attempts
             WHERE learner_id = ?1 AND subtest = ?2
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![learner_id, subtest], |row| {
            Ok(Attempt {
                id: row.get(0)?,
                learner_id: row.get(1)?,
                subtest: row.get(2)?,
                question_id: row.get(3)?,
                correct: row.get::<_, i64>(4)? == 1,
                latency_ms: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

// ---------------------------------------------------------------------------
// Mastery (REQ-010)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct MasteryRecord {
    pub learner_id: String,
    pub subtest: String,
    pub score: f64,
    pub uncertainty: f64,
    pub updated_at: String,
}

pub struct MasteryRepo<'a> {
    db: &'a Database,
}

impl<'a> MasteryRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Insert or replace the estimate for one (learner, subtest) pair.
    pub fn upsert(
        &self,
        learner_id: &str,
        subtest: &str,
        score: f64,
        uncertainty: f64,
    ) -> anyhow::Result<()> {
        if !(0.0..=1.0).contains(&score) {
            anyhow::bail!("mastery score {score} outside [0,1]");
        }
        if uncertainty < 0.0 {
            anyhow::bail!("uncertainty {uncertainty} must be >= 0");
        }
        self.db.connection().execute(
            "INSERT INTO mastery (learner_id, subtest, score, uncertainty, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (learner_id, subtest) DO UPDATE SET
                 score = excluded.score,
                 uncertainty = excluded.uncertainty,
                 updated_at = excluded.updated_at",
            params![learner_id, subtest, score, uncertainty, now()],
        )?;
        Ok(())
    }

    pub fn get(&self, learner_id: &str, subtest: &str) -> anyhow::Result<Option<MasteryRecord>> {
        let conn = self.db.connection();
        let row = conn
            .query_row(
                "SELECT learner_id, subtest, score, uncertainty, updated_at
                 FROM mastery WHERE learner_id = ?1 AND subtest = ?2",
                params![learner_id, subtest],
                |row| {
                    Ok(MasteryRecord {
                        learner_id: row.get(0)?,
                        subtest: row.get(1)?,
                        score: row.get(2)?,
                        uncertainty: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn all_for(&self, learner_id: &str) -> anyhow::Result<Vec<MasteryRecord>> {
        let conn = self.db.connection();
        let mut stmt = conn.prepare(
            "SELECT learner_id, subtest, score, uncertainty, updated_at
             FROM mastery WHERE learner_id = ?1 ORDER BY subtest",
        )?;
        let rows = stmt.query_map(params![learner_id], |row| {
            Ok(MasteryRecord {
                learner_id: row.get(0)?,
                subtest: row.get(1)?,
                score: row.get(2)?,
                uncertainty: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

// ---------------------------------------------------------------------------
// Evidence vault (REQ-020)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRecord {
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

/// A source snapshot to store in the vault.
///
/// Grouped into a struct because the fields are positionally indistinguishable
/// (two different hashes, two dates), and a caller silently swapping them would
/// corrupt provenance without any type error.
#[derive(Debug, Clone)]
pub struct NewEvidence<'a> {
    pub url: &'a str,
    pub title: &'a str,
    pub content_hash: &'a str,
    pub license: &'a str,
    pub effective_date: &'a str,
    pub trust: f64,
    pub retrieval_status: &'a str,
}

impl<'a> NewEvidence<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: &'a str,
        title: &'a str,
        content_hash: &'a str,
        license: &'a str,
        effective_date: &'a str,
        trust: f64,
        retrieval_status: &'a str,
    ) -> Self {
        Self {
            url,
            title,
            content_hash,
            license,
            effective_date,
            trust,
            retrieval_status,
        }
    }
}

pub struct EvidenceRepo<'a> {
    db: &'a Database,
}

impl<'a> EvidenceRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Store a source snapshot, keyed by content hash.
    ///
    /// Identical content deduplicates to the existing identity: the vault is
    /// hash-addressed, so re-retrieving a source does not create a second row.
    /// Returns the row id (existing or new).
    pub fn put(&self, evidence: &NewEvidence<'_>) -> anyhow::Result<String> {
        let url = evidence.url;
        let title = evidence.title;
        let content_hash = evidence.content_hash;
        let license = evidence.license;
        let effective_date = evidence.effective_date;
        let trust = evidence.trust;
        let retrieval_status = evidence.retrieval_status;

        if !(0.0..=1.0).contains(&trust) {
            anyhow::bail!("trust {trust} outside [0,1]");
        }

        let conn = self.db.connection();
        if let Some(existing) = conn
            .query_row(
                "SELECT id FROM evidence_records WHERE content_hash = ?1",
                params![content_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing);
        }

        let id = new_id("ev");
        conn.execute(
            "INSERT INTO evidence_records
                (id, content_hash, url, title, license, effective_date,
                 trust, retrieval_status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                content_hash,
                url,
                title,
                license,
                effective_date,
                trust,
                retrieval_status,
                now()
            ],
        )?;
        Ok(id)
    }

    pub fn get(&self, id: &str) -> anyhow::Result<Option<EvidenceRecord>> {
        let conn = self.db.connection();
        let row = conn
            .query_row(
                "SELECT id, content_hash, url, title, license, effective_date,
                        trust, retrieval_status, created_at
                 FROM evidence_records WHERE id = ?1",
                params![id],
                |row| {
                    Ok(EvidenceRecord {
                        id: row.get(0)?,
                        content_hash: row.get(1)?,
                        url: row.get(2)?,
                        title: row.get(3)?,
                        license: row.get(4)?,
                        effective_date: row.get(5)?,
                        trust: row.get(6)?,
                        retrieval_status: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Every stored snapshot, newest first.
    ///
    /// The source viewer (SPEC-004) has to list what the vault holds, and a
    /// reader that could only fetch by known id would force the UI to guess
    /// which ids exist. Ordered by `created_at` then `id` so the order is total
    /// and stable rather than depending on insertion luck.
    pub fn list(&self) -> anyhow::Result<Vec<EvidenceRecord>> {
        let conn = self.db.connection();
        let mut stmt = conn.prepare(
            "SELECT id, content_hash, url, title, license, effective_date,
                    trust, retrieval_status, created_at
             FROM evidence_records
             ORDER BY created_at DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(EvidenceRecord {
                id: row.get(0)?,
                content_hash: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                license: row.get(4)?,
                effective_date: row.get(5)?,
                trust: row.get(6)?,
                retrieval_status: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Attempt to revise a stored trust score.
    ///
    /// This always fails: vault rows are immutable (enforced by a database
    /// trigger, not merely by this method), so provenance cannot be rewritten
    /// after the fact. Exposed so callers get a typed error instead of a raw
    /// SQLite failure.
    pub fn set_trust(&self, id: &str, trust: f64) -> anyhow::Result<()> {
        let changed = self.db.connection().execute(
            "UPDATE evidence_records SET trust = ?2 WHERE id = ?1",
            params![id, trust],
        )?;
        if changed == 0 {
            // The trigger fired, or no such row: both are refusals.
            anyhow::bail!("evidence vault records are immutable (id {id})");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pull requests (REQ-032)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PullRequest {
    pub id: String,
    pub repo: String,
    pub title: String,
    pub diff: String,
    pub approved: bool,
    pub approved_by: Option<String>,
    pub merged: bool,
    pub created_at: String,
}

pub struct PullRequestRepo<'a> {
    db: &'a Database,
}

impl<'a> PullRequestRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Record a proposed repair PR. New rows are always unapproved and
    /// unmerged: approval is a separate, explicit human act.
    pub fn record(&self, repo: &str, title: &str, diff: &str) -> anyhow::Result<String> {
        let id = new_id("pr");
        self.db.connection().execute(
            "INSERT INTO pull_requests (id, repo, title, diff, approved, merged, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, 0, ?5)",
            params![id, repo, title, diff, now()],
        )?;
        Ok(id)
    }

    pub fn get(&self, id: &str) -> anyhow::Result<Option<PullRequest>> {
        let conn = self.db.connection();
        let row = conn
            .query_row(
                "SELECT id, repo, title, diff, approved, approved_by, merged, created_at
                 FROM pull_requests WHERE id = ?1",
                params![id],
                |row| {
                    Ok(PullRequest {
                        id: row.get(0)?,
                        repo: row.get(1)?,
                        title: row.get(2)?,
                        diff: row.get(3)?,
                        approved: row.get::<_, i64>(4)? == 1,
                        approved_by: row.get(5)?,
                        merged: row.get::<_, i64>(6)? == 1,
                        created_at: row.get(7)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Record explicit human approval.
    pub fn approve(&self, id: &str, approver: &str) -> anyhow::Result<()> {
        let changed = self.db.connection().execute(
            "UPDATE pull_requests SET approved = 1, approved_by = ?2 WHERE id = ?1",
            params![id, approver],
        )?;
        if changed == 0 {
            anyhow::bail!("pull request {id} not found");
        }
        Ok(())
    }

    /// Merge an approved PR.
    ///
    /// Refuses when approval is missing, and the row-level CHECK constraint
    /// makes an unapproved-but-merged row unrepresentable even if this method
    /// were bypassed. There is deliberately no auto-merge path (REQ-032).
    pub fn merge(&self, id: &str, actor: &str) -> anyhow::Result<bool> {
        let pr = self
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("pull request {id} not found"))?;
        if !pr.approved || pr.approved_by.is_none() {
            anyhow::bail!("pull request {id} has no explicit approval; merge refused");
        }
        if pr.merged {
            return Ok(false);
        }

        let changed = self.db.connection().execute(
            "UPDATE pull_requests SET merged = 1 WHERE id = ?1 AND approved = 1",
            params![id],
        )?;
        if changed == 0 {
            anyhow::bail!("merge refused for {id} (actor {actor})");
        }
        Ok(true)
    }

    /// PRs still awaiting a human decision.
    pub fn pending(&self) -> anyhow::Result<Vec<PullRequest>> {
        let conn = self.db.connection();
        let mut stmt = conn.prepare(
            "SELECT id FROM pull_requests
             WHERE approved = 0 AND merged = 0 ORDER BY created_at, id",
        )?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(pr) = self.get(&id)? {
                out.push(pr);
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Content packs (SPEC-002 rollback)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ContentPackRecord {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub signature: String,
    pub status: String,
    pub created_at: String,
}

pub struct ContentPackRepo<'a> {
    db: &'a Database,
}

impl<'a> ContentPackRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Publish a version, superseding the previously active one.
    ///
    /// Runs in one transaction so there is never a window with zero or two
    /// active packs for a name.
    pub fn publish(&self, name: &str, version: i64, signature: &str) -> anyhow::Result<String> {
        let conn = self.db.connection();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> anyhow::Result<String> {
            conn.execute(
                "UPDATE content_packs SET status = 'superseded'
                 WHERE name = ?1 AND status = 'active'",
                params![name],
            )?;
            let id = new_id("pack");
            conn.execute(
                "INSERT INTO content_packs (id, name, version, signature, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
                params![id, name, version, signature, now()],
            )?;
            Ok(id)
        })();

        match result {
            Ok(id) => {
                conn.execute_batch("COMMIT")?;
                Ok(id)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn active(&self, name: &str) -> anyhow::Result<Option<ContentPackRecord>> {
        let conn = self.db.connection();
        let row = conn
            .query_row(
                "SELECT id, name, version, signature, status, created_at
                 FROM content_packs WHERE name = ?1 AND status = 'active'",
                params![name],
                |row| {
                    Ok(ContentPackRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        version: row.get(2)?,
                        signature: row.get(3)?,
                        status: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Roll back to the highest superseded version below the active one.
    ///
    /// Used when a newly published pack is found bad: the previous signed
    /// version becomes active again.
    pub fn rollback(&self, name: &str) -> anyhow::Result<ContentPackRecord> {
        let active = self
            .active(name)?
            .ok_or_else(|| anyhow::anyhow!("no active pack named {name}"))?;

        let conn = self.db.connection();
        let previous: Option<(String, i64, String, String)> = conn
            .query_row(
                "SELECT id, version, signature, created_at
                 FROM content_packs
                 WHERE name = ?1 AND version < ?2
                 ORDER BY version DESC LIMIT 1",
                params![name, active.version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        let (prev_id, prev_version, prev_signature, prev_created) = previous
            .ok_or_else(|| anyhow::anyhow!("no earlier pack to roll back to for {name}"))?;

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> anyhow::Result<()> {
            conn.execute(
                "UPDATE content_packs SET status = 'quarantined' WHERE id = ?1",
                params![active.id],
            )?;
            conn.execute(
                "UPDATE content_packs SET status = 'active' WHERE id = ?1",
                params![prev_id],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(ContentPackRecord {
                    id: prev_id,
                    name: name.to_string(),
                    version: prev_version,
                    signature: prev_signature,
                    status: "active".to_string(),
                    created_at: prev_created,
                })
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}
