use rusqlite::{params, Connection, OptionalExtension};
use crate::db::DatabaseError;

pub struct MasteryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MasteryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn save_mastery(&self, user_id: &str, score: f64) -> Result<(), DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS mastery (user_id TEXT PRIMARY KEY, score REAL)",
            [],
        )?;
        self.conn.execute(
            "INSERT INTO mastery (user_id, score) VALUES (?1, ?2)
             ON CONFLICT(user_id) DO UPDATE SET score=excluded.score",
            params![user_id, score],
        )?;
        Ok(())
    }

    pub fn get_mastery(&self, user_id: &str) -> Result<Option<f64>, DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS mastery (user_id TEXT PRIMARY KEY, score REAL)",
            [],
        )?;
        let score: Option<f64> = self
            .conn
            .query_row(
                "SELECT score FROM mastery WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(score)
    }
}

pub struct EvidenceVault<'a> {
    conn: &'a Connection,
}

impl<'a> EvidenceVault<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn save_snapshot(&self, hash: &str, content: &str) -> Result<(), DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS evidence_vault (hash TEXT PRIMARY KEY, content TEXT)",
            [],
        )?;
        self.conn.execute(
            "INSERT INTO evidence_vault (hash, content) VALUES (?1, ?2)",
            params![hash, content],
        )?;
        Ok(())
    }

    pub fn get_snapshot(&self, hash: &str) -> Result<Option<String>, DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS evidence_vault (hash TEXT PRIMARY KEY, content TEXT)",
            [],
        )?;
        let content: Option<String> = self
            .conn
            .query_row(
                "SELECT content FROM evidence_vault WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .optional()?;
        Ok(content)
    }
}

pub struct PrStateRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PrStateRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn set_pr_approval(&self, pr_id: &str, approved: bool) -> Result<(), DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS pr_state (pr_id TEXT PRIMARY KEY, approved INTEGER)",
            [],
        )?;
        self.conn.execute(
            "INSERT INTO pr_state (pr_id, approved) VALUES (?1, ?2)
             ON CONFLICT(pr_id) DO UPDATE SET approved=excluded.approved",
            params![pr_id, if approved { 1 } else { 0 }],
        )?;
        Ok(())
    }

    pub fn is_approved(&self, pr_id: &str) -> Result<bool, DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS pr_state (pr_id TEXT PRIMARY KEY, approved INTEGER)",
            [],
        )?;
        let approved: Option<i32> = self
            .conn
            .query_row(
                "SELECT approved FROM pr_state WHERE pr_id = ?1",
                params![pr_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(approved == Some(1))
    }
}

pub struct AttemptRepository<'a> {
    conn: &'a Connection,
}

impl<'a> AttemptRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn record_attempt(&self, attempt_id: &str, result: &str) -> Result<bool, DatabaseError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS attempts (attempt_id TEXT PRIMARY KEY, result TEXT)",
            [],
        )?;
        // Use INSERT OR IGNORE for idempotency
        let rows = self.conn.execute(
            "INSERT OR IGNORE INTO attempts (attempt_id, result) VALUES (?1, ?2)",
            params![attempt_id, result],
        )?;
        Ok(rows > 0)
    }
}
