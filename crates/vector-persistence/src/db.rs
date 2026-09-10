use rusqlite::Connection;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open_in_memory() -> Result<Self, DatabaseError> {
        let conn = Connection::open_in_memory()?;
        conn.execute("CREATE TABLE IF NOT EXISTS migrations (id TEXT PRIMARY KEY, applied_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
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
        migrations: &[(&str, &str)],
    ) -> Result<usize, DatabaseError> {
        let mut count = 0;
        for (id, sql) in migrations {
            let mut stmt = conn.prepare("SELECT 1 FROM migrations WHERE id = ?1")?;
            if !stmt.exists([id])? {
                conn.execute_batch(sql)?;
                conn.execute("INSERT INTO migrations (id) VALUES (?1)", [id])?;
                count += 1;
            }
        }
        Ok(count)
    }
}

impl Database {
    pub fn open(path: &str) -> Result<Self, DatabaseError> {
        let conn = Connection::open(path)?;
        conn.execute("CREATE TABLE IF NOT EXISTS migrations (id TEXT PRIMARY KEY, applied_at DATETIME DEFAULT CURRENT_TIMESTAMP)", [])?;
        Ok(Self { conn })
    }
}
