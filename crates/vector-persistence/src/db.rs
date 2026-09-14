//! SQLite connection management and monotonic migration application.
//!
//! ADR-002: SQLite is the canonical local operational store. SPEC-002 requires
//! migrations to be monotonic and to be applied from N-1 without data loss.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

/// A single migration: `(version, sql_body)`.
pub type Migration = (i64, String);

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the database at `path`.
    ///
    /// Busy timeout is set so concurrent writers wait for the lock instead of
    /// failing immediately (REQ-054: serialized writers, safe background
    /// workers). WAL keeps readers non-blocking against an active writer.
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        Ok(Self { conn })
    }

    /// Open an ephemeral in-memory database (tests, dry runs).
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        Ok(Self { conn })
    }

    fn configure(conn: &Connection) -> rusqlite::Result<()> {
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;",
        )?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Run SQLite's own integrity check. Used by backup and health probes.
    pub fn integrity_check(&self) -> rusqlite::Result<bool> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        Ok(result == "ok")
    }
}

pub struct MigrationManager;

impl MigrationManager {
    /// Read `NNN_name.sql` files from `dir` in ascending version order.
    ///
    /// Filenames are the version source of truth; there is no separate index
    /// file that can drift out of sync with the schema.
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Vec<Migration>> {
        let mut out: Vec<Migration> = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sql") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            let version: i64 = match stem.split('_').next().and_then(|v| v.parse().ok()) {
                Some(v) => v,
                None => anyhow::bail!("migration {stem:?} does not start with a version number"),
            };
            out.push((version, std::fs::read_to_string(&path)?));
        }
        out.sort_by_key(|(v, _)| *v);

        // Reject duplicate versions rather than silently applying one of them.
        for pair in out.windows(2) {
            if pair[0].0 == pair[1].0 {
                anyhow::bail!("duplicate migration version {}", pair[0].0);
            }
        }
        Ok(out)
    }

    /// Apply every migration whose version is not yet recorded.
    ///
    /// Each migration runs in its own transaction: a failure rolls that
    /// migration back and leaves earlier ones applied, so the database is never
    /// left half-migrated.
    pub fn apply(db: &mut Database, migrations: &[Migration]) -> anyhow::Result<usize> {
        let conn = db.connection_mut();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                name       TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );",
            [],
        )?;

        let mut applied = 0usize;
        for (version, sql) in migrations {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
                [version],
                |row| row.get(0),
            )?;
            if exists > 0 {
                continue;
            }

            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    version,
                    format!("{version:03}"),
                    chrono::Utc::now().to_rfc3339()
                ],
            )?;
            tx.commit()?;
            applied += 1;
        }
        Ok(applied)
    }

    /// Versions recorded as applied, ascending.
    pub fn applied_versions(db: &Database) -> anyhow::Result<Vec<i64>> {
        let conn = db.connection();
        // A database that has never been migrated has no table; treat as empty.
        let table_exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name='schema_migrations'",
            [],
            |row| row.get(0),
        )?;
        if table_exists == 0 {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
