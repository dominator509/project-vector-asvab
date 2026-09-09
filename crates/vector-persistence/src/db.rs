use rusqlite::{Connection, Result};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

pub struct MigrationManager;

impl MigrationManager {
    pub fn apply_migrations(
        conn: &Connection,
        sql_migrations: &[(&str, &str)],
    ) -> anyhow::Result<usize> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );",
            [],
        )?;

        let mut applied_count = 0;
        for (version_str, sql) in sql_migrations {
            let version: i32 = version_str.parse()?;
            let count: i32 = conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
                [version],
                |row| row.get(0),
            )?;

            if count == 0 {
                let tx = conn.unchecked_transaction()?;
                tx.execute_batch(sql)?;
                tx.execute(
                    "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                    rusqlite::params![version, version_str],
                )?;
                tx.commit()?;
                applied_count += 1;
            }
        }

        Ok(applied_count)
    }
}
