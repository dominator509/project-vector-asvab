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

/// What a learner has done on one objective, read from the attempts themselves.
///
/// Per *objective* rather than per subtest, which is the grain the curriculum graph is written
/// at: a subtest average cannot say that a learner has mastered rate problems and is failing
/// interest ones, and an objective's prerequisites are what decides whether the harder
/// objective should be offered at all.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveStats {
    pub objective_id: String,
    pub subtest: String,
    pub attempts: i64,
    pub correct: i64,
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

    /// Attempts per objective for one learner, joined through the items they answered.
    ///
    /// The join is the point: an attempt records the question it was on, and the question
    /// records the objective it teaches, so this is the evidence the curriculum's own grain is
    /// read from rather than a second thing a caller has to keep in step.
    pub fn objective_stats(&self, learner_id: &str) -> anyhow::Result<Vec<ObjectiveStats>> {
        let conn = self.db.connection();
        let mut statement = conn.prepare(
            "SELECT i.objective_id, i.subtest, COUNT(a.id), SUM(a.correct)
             FROM attempts a
             JOIN content_items i ON i.id = a.question_id
             WHERE a.learner_id = ?1
             GROUP BY i.objective_id, i.subtest
             ORDER BY i.objective_id",
        )?;
        let rows = statement.query_map(params![learner_id], |row| {
            Ok(ObjectiveStats {
                objective_id: row.get(0)?,
                subtest: row.get(1)?,
                attempts: row.get(2)?,
                correct: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
    /// The public key that signed this pack, base64.
    pub signer: String,
    /// The digest the signature covers.
    pub content_hash: String,
    pub schema_version: i64,
    /// The manifest as it was received, stored verbatim.
    pub manifest_json: String,
    pub item_count: i64,
    pub status: String,
    pub created_at: String,
}

impl ContentPackRecord {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            version: row.get(2)?,
            signature: row.get(3)?,
            signer: row.get(4)?,
            content_hash: row.get(5)?,
            schema_version: row.get(6)?,
            manifest_json: row.get(7)?,
            item_count: row.get(8)?,
            status: row.get(9)?,
            created_at: row.get(10)?,
        })
    }
}

/// A pack registry row to write, together with the items it delivers.
pub struct NewPackRecord<'a> {
    pub name: &'a str,
    pub version: i64,
    /// Detached signature, base64.
    pub signature: &'a str,
    /// The signing key, base64.
    pub signer: &'a str,
    pub content_hash: &'a str,
    pub schema_version: i64,
    pub manifest_json: &'a str,
    pub item_count: i64,
}

/// One item a pack delivers, as the store needs it.
///
/// The fields are the ones `content_items` requires; the caller has already verified
/// them against the pack's signature and its own rules.
pub struct NewPackItem<'a> {
    pub id: &'a str,
    pub subtest: &'a str,
    pub objective_id: &'a str,
    pub stem: &'a str,
    pub passage: Option<&'a str>,
    pub options: &'a [String],
    pub correct_index: usize,
    pub explanation: &'a str,
    pub distractor_rationales: &'a std::collections::BTreeMap<usize, String>,
    pub difficulty: f64,
    pub proof_kind: &'a str,
    pub proof_json: &'a str,
    pub content_hash: &'a str,
    pub generator_hash: Option<&'a str>,
    pub verifier_hash: Option<&'a str>,
    pub reviewer: &'a str,
    /// Source ids as the pack records them. The install maps them to local
    /// evidence ids before citing.
    pub source_ids: Vec<String>,
}

/// What an install did, so the caller can report it without re-reading.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstallOutcome {
    pub pack_id: String,
    pub installed: usize,
    pub already_present: usize,
    /// Evidence records the pack needed that the vault already held, keyed by the
    /// pack's own source id.
    pub reused_sources: usize,
    pub added_sources: usize,
}

/// A source a pack's items cite, as the pack records it.
pub struct PackSourceRecord<'a> {
    /// The pack's own name for the source; items cite this.
    pub id: &'a str,
    pub url: &'a str,
    pub title: &'a str,
    pub content_hash: &'a str,
    pub licence: &'a str,
    pub effective_date: &'a str,
    pub trust: f64,
    pub retrieval_status: &'a str,
}

/// The state an item was in before `state` in the documented pipeline.
fn previous_state(state: &str) -> &'static str {
    match state {
        "machine_validated" => "draft",
        "independent_verified" => "machine_validated",
        "content_reviewed" => "independent_verified",
        _ => "content_reviewed",
    }
}

pub struct ContentPackRepo<'a> {
    db: &'a Database,
}

impl<'a> ContentPackRepo<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Register a pack version without delivering any items.
    ///
    /// Used by the registry's own tests and by a caller that is recording a pack it
    /// did not install. Installation goes through [`ContentPackRepo::install`], which
    /// writes the row and the items in one transaction.
    pub fn publish(&self, pack: &NewPackRecord<'_>) -> anyhow::Result<String> {
        let conn = self.db.connection();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> anyhow::Result<String> {
            conn.execute(
                "UPDATE content_packs SET status = 'superseded'
                 WHERE name = ?1 AND status = 'active'",
                params![pack.name],
            )?;
            let id = new_id("pack");
            conn.execute(
                "INSERT INTO content_packs (
                     id, name, version, signature, signer, content_hash, schema_version,
                     manifest_json, item_count, status, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', ?10)",
                params![
                    id,
                    pack.name,
                    pack.version,
                    pack.signature,
                    pack.signer,
                    pack.content_hash,
                    pack.schema_version,
                    pack.manifest_json,
                    pack.item_count,
                    now()
                ],
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

    /// Install a pack: register it and deliver its items, in one transaction.
    ///
    /// ## Why everything is in one transaction
    ///
    /// A pack that becomes active while its items are still being written is a pack
    /// that serves learners a partial corpus, and a crash between the two writes
    /// leaves a registry claiming content the store does not hold. Both writes go in
    /// together or neither does.
    ///
    /// ## Why items are walked to `active` rather than inserted as active
    ///
    /// The schema refuses an item that does not start as `draft`, and it refuses an
    /// item that becomes active without citing a source. Both rules exist so that no
    /// path can put unreviewed content in front of a learner, and an installer is not
    /// exempt from them. The transitions and their audit rows are written here so
    /// that an installed item has the same history as an ingested one.
    ///
    /// ## Sources are keyed by content hash, not by the pack's own id
    ///
    /// The evidence vault is hash-addressed and its `content_hash` is unique, so a
    /// source the vault already holds is reused rather than duplicated. The pack's
    /// source id is therefore only a name for the bytes; the citation records the
    /// local id.
    pub fn install(
        &self,
        pack: &NewPackRecord<'_>,
        sources: &[PackSourceRecord<'_>],
        items: &[NewPackItem<'_>],
    ) -> anyhow::Result<InstallOutcome> {
        let conn = self.db.connection();
        conn.execute_batch("BEGIN IMMEDIATE")?;

        let result = (|| -> anyhow::Result<InstallOutcome> {
            // A version number identifies content. Reinstalling the pack already
            // registered at this version is a no-op; registering *different* content
            // under a version that exists is refused, because two packs would then
            // share one identity and a rollback or a citation could not say which one
            // it meant.
            let existing_version: Option<(String, String)> = conn
                .query_row(
                    "SELECT id, content_hash FROM content_packs WHERE name = ?1 AND version = ?2",
                    params![pack.name, pack.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let already_registered = match existing_version {
                Some((id, recorded)) if recorded == pack.content_hash => Some(id),
                Some((_, recorded)) => anyhow::bail!(
                    "pack {} version {} is already registered with content hash {recorded}, so \
                     installing {} under the same version would give two packs one identity",
                    pack.name,
                    pack.version,
                    pack.content_hash
                ),
                None => None,
            };

            // A pack that is already registered is left exactly as it is, status
            // included: reinstalling must not quietly undo a rollback somebody made.
            let pack_id = match already_registered {
                Some(id) => id,
                None => {
                    conn.execute(
                        "UPDATE content_packs SET status = 'superseded'
                         WHERE name = ?1 AND status = 'active'",
                        params![pack.name],
                    )?;

                    let id = new_id("pack");
                    conn.execute(
                        "INSERT INTO content_packs (
                             id, name, version, signature, signer, content_hash, schema_version,
                             manifest_json, item_count, status, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', ?10)",
                        params![
                            id,
                            pack.name,
                            pack.version,
                            pack.signature,
                            pack.signer,
                            pack.content_hash,
                            pack.schema_version,
                            pack.manifest_json,
                            pack.item_count,
                            now()
                        ],
                    )?;
                    id
                }
            };

            // An item the store already holds by content is not reinstalled; an id
            // collision with different content is a conflict the caller must resolve,
            // because silently overwriting it would change what an existing attempt
            // refers to.
            //
            // `local_ids` maps the pack's name for an item to the store's. They are
            // the same for an item written now and different for one the store already
            // held under another id -- two packs built independently from the same
            // source produce identical content with different identifiers. Membership
            // and citations have to name the local row, or a pack that overlaps the
            // corpus fails on a foreign key the moment it touches shared content.
            // Found by installing a 40-item pack into a store that already held the
            // same 40 items among 7,016.
            let mut to_install: Vec<&NewPackItem<'_>> = Vec::new();
            let mut local_ids: std::collections::HashMap<&str, String> =
                std::collections::HashMap::new();
            let mut already_present = 0usize;
            for item in items {
                let existing: Option<String> = conn
                    .query_row(
                        "SELECT id FROM content_items WHERE content_hash = ?1",
                        params![item.content_hash],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(existing_id) = existing {
                    already_present += 1;
                    local_ids.insert(item.id, existing_id);
                    continue;
                }
                let id_taken: Option<String> = conn
                    .query_row(
                        "SELECT id FROM content_items WHERE id = ?1",
                        params![item.id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if id_taken.is_some() {
                    anyhow::bail!(
                        "item {} is already present with different content, so installing \
                         this pack would replace what an existing record refers to",
                        item.id
                    );
                }
                local_ids.insert(item.id, item.id.to_string());
                to_install.push(item);
            }

            // Record the sources the pack needs, reusing any the vault already holds.
            let mut source_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut added_sources = 0usize;
            let mut reused_sources = 0usize;
            for source in sources {
                let existing: Option<String> = conn
                    .query_row(
                        "SELECT id FROM evidence_records WHERE content_hash = ?1",
                        params![source.content_hash],
                        |row| row.get(0),
                    )
                    .optional()?;
                let local_id = match existing {
                    Some(id) => {
                        reused_sources += 1;
                        id
                    }
                    None => {
                        let id = new_id("ev");
                        conn.execute(
                            "INSERT INTO evidence_records (
                                 id, url, title, content_hash, license, effective_date,
                                 trust, retrieval_status, created_at
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                            params![
                                id,
                                source.url,
                                source.title,
                                source.content_hash,
                                source.licence,
                                source.effective_date,
                                source.trust,
                                source.retrieval_status,
                                now()
                            ],
                        )?;
                        added_sources += 1;
                        id
                    }
                };
                source_map.insert(source.id.to_string(), local_id);
            }

            let timestamp = now();
            for item in &to_install {
                let options_json = serde_json::to_string(item.options)?;
                let distractors_json = serde_json::to_string(item.distractor_rationales)?;
                conn.execute(
                    "INSERT INTO content_items (
                         id, pack_id, subtest, objective_id, state, stem, passage, options_json,
                         correct_index, explanation, distractors_json, difficulty, proof_kind,
                         proof_json, reviewer, content_hash, generator_hash, verifier_hash,
                         created_at, updated_at
                     ) VALUES (
                         ?1, ?2, ?3, ?4, 'draft', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                         ?13, '', ?14, ?15, ?16, ?17, ?17
                     )",
                    params![
                        item.id,
                        pack_id,
                        item.subtest,
                        item.objective_id,
                        item.stem,
                        item.passage,
                        options_json,
                        item.correct_index as i64,
                        item.explanation,
                        distractors_json,
                        item.difficulty,
                        item.proof_kind,
                        item.proof_json,
                        item.content_hash,
                        item.generator_hash,
                        item.verifier_hash,
                        timestamp,
                    ],
                )?;

                for source_id in &item.source_ids {
                    let local = source_map.get(source_id).ok_or_else(|| {
                        anyhow::anyhow!(
                            "item {} cites source {source_id}, which the pack's ledger does \
                             not carry",
                            item.id
                        )
                    })?;
                    conn.execute(
                        "INSERT INTO content_item_sources (item_id, source_id) VALUES (?1, ?2)",
                        params![item.id, local],
                    )?;
                }

                // Walk the documented pipeline, writing the audit row each transition
                // owes. The transition guard is the schema's, not this method's.
                conn.execute(
                    "UPDATE content_items SET reviewer = ?2 WHERE id = ?1",
                    params![item.id, item.reviewer],
                )?;
                for state in [
                    "machine_validated",
                    "independent_verified",
                    "content_reviewed",
                    "active",
                ] {
                    conn.execute(
                        "UPDATE content_items SET state = ?2, updated_at = ?3 WHERE id = ?1",
                        params![item.id, state, now()],
                    )?;
                    conn.execute(
                        "INSERT INTO content_item_reviews
                             (id, item_id, from_state, to_state, actor, rationale, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            new_id("rev"),
                            item.id,
                            previous_state(state),
                            state,
                            "pack-install",
                            "delivered by a signed content pack",
                            now()
                        ],
                    )?;
                }
            }

            // Membership is recorded for *every* item the pack carries, not only the
            // rows written just now. Serving follows membership, so a version that
            // repackages content an earlier pack delivered has to list that content
            // itself, or publishing it would withdraw the corpus the moment the
            // earlier version was superseded. `content_items.pack_id` stays as the
            // pack that first delivered the row, which is a provenance fact rather
            // than a service rule.
            //
            // Written after the item inserts: the membership table references
            // `content_items`, so a row for an item that does not exist yet is a
            // foreign-key violation.
            for item in items {
                let local = local_ids
                    .get(&item.id)
                    .ok_or_else(|| anyhow::anyhow!("item {} has no local identity", item.id))?;
                conn.execute(
                    "INSERT OR IGNORE INTO content_pack_items (pack_id, item_id)
                     VALUES (?1, ?2)",
                    params![pack_id, local],
                )?;
            }

            Ok(InstallOutcome {
                pack_id,
                installed: to_install.len(),
                already_present,
                reused_sources,
                added_sources,
            })
        })();

        match result {
            Ok(outcome) => {
                conn.execute_batch("COMMIT")?;
                Ok(outcome)
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
                "SELECT id, name, version, signature, signer, content_hash, schema_version,
                        manifest_json, item_count, status, created_at
                 FROM content_packs WHERE name = ?1 AND status = 'active'",
                params![name],
                ContentPackRecord::from_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Every pack the registry holds, newest first within a name.
    pub fn list(&self) -> anyhow::Result<Vec<ContentPackRecord>> {
        let conn = self.db.connection();
        let mut statement = conn.prepare(
            "SELECT id, name, version, signature, signer, content_hash, schema_version,
                    manifest_json, item_count, status, created_at
             FROM content_packs ORDER BY name, version DESC",
        )?;
        let rows = statement.query_map([], ContentPackRecord::from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Roll back to the highest superseded version below the active one.
    ///
    /// Used when a newly published pack is found bad: the previous signed version
    /// becomes active again, and because serving is derived from pack status, its
    /// items become servable again in the same transaction. Nothing is deleted, so a
    /// rollback can be rolled back.
    ///
    /// Only `superseded` versions are eligible. A `quarantined` version was withdrawn
    /// deliberately -- by a rollback, or by a reviewer -- and reinstating it
    /// automatically would undo that decision. Found by rolling back a store that had
    /// three versions: the rollback landed on the quarantined one.
    pub fn rollback(&self, name: &str) -> anyhow::Result<ContentPackRecord> {
        let active = self
            .active(name)?
            .ok_or_else(|| anyhow::anyhow!("no active pack named {name}"))?;

        let conn = self.db.connection();
        let previous: Option<ContentPackRecord> = conn
            .query_row(
                "SELECT id, name, version, signature, signer, content_hash, schema_version,
                        manifest_json, item_count, status, created_at
                 FROM content_packs
                 WHERE name = ?1 AND version < ?2 AND status = 'superseded'
                 ORDER BY version DESC LIMIT 1",
                params![name, active.version],
                ContentPackRecord::from_row,
            )
            .optional()?;

        let previous = previous
            .ok_or_else(|| anyhow::anyhow!("no earlier pack to roll back to for {name}"))?;

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> anyhow::Result<()> {
            conn.execute(
                "UPDATE content_packs SET status = 'quarantined' WHERE id = ?1",
                params![active.id],
            )?;
            conn.execute(
                "UPDATE content_packs SET status = 'active' WHERE id = ?1",
                params![previous.id],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(ContentPackRecord {
                    status: "active".to_string(),
                    ..previous
                })
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Put a registered pack version back into service, on purpose.
    ///
    /// `install` deliberately leaves a registered pack's status alone so that a stray reinstall
    /// cannot undo a rollback somebody made. That safety property removes the forward path:
    /// after a rollback the installation is on the older content, and the signed bytes it
    /// already holds cannot be brought back by installing them again -- observed in round 29,
    /// where reinstalling the same v6 pack reported success and changed nothing. A rollback that
    /// cannot be reversed is not a rollback; it is a downgrade with no way home.
    ///
    /// So this is the way home, and it is a separate, deliberate act with its own command rather
    /// than a side effect of `install`. Whatever was active for the name is *superseded*, not
    /// quarantined: it is an intact signed pack that a later rollback or activation can return
    /// to. Activating the version that is already active is a no-op rather than an error, so a
    /// scripted transition can say what it wants without first asking what the state is.
    pub fn activate(&self, name: &str, version: i64) -> anyhow::Result<ContentPackRecord> {
        let conn = self.db.connection();
        let target: Option<ContentPackRecord> = conn
            .query_row(
                "SELECT id, name, version, signature, signer, content_hash, schema_version,
                        manifest_json, item_count, status, created_at
                 FROM content_packs WHERE name = ?1 AND version = ?2",
                params![name, version],
                ContentPackRecord::from_row,
            )
            .optional()?;
        let target = target.ok_or_else(|| {
            anyhow::anyhow!("no pack named {name} is registered at version {version}")
        })?;
        if target.status == "active" {
            return Ok(target);
        }

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> anyhow::Result<()> {
            conn.execute(
                "UPDATE content_packs SET status = 'superseded'
                 WHERE name = ?1 AND status = 'active'",
                params![name],
            )?;
            conn.execute(
                "UPDATE content_packs SET status = 'active' WHERE id = ?1",
                params![target.id],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(ContentPackRecord {
                    status: "active".to_string(),
                    ..target
                })
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}
