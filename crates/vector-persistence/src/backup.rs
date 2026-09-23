//! Atomic, integrity-checked backup and restore (REQ-033).
//!
//! Design constraints from REQ-033 and SPEC-002:
//! - the backup is atomic: a partial file is never left in place;
//! - it is integrity-checked before it is considered a backup;
//! - restore verifies the digest before touching live state, so a corrupted
//!   archive is rejected instead of silently applied;
//! - encryption is optional and never silently downgraded.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::db::Database;

/// Metadata describing a completed backup.
#[derive(Debug, Clone, PartialEq)]
pub struct BackupManifest {
    pub path: PathBuf,
    pub checksum: String,
    pub bytes: u64,
    pub integrity: String,
    pub encrypted: bool,
    pub attempt_rows: i64,
}

/// Result of a restore.
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreOutcome {
    Restored { rows: i64 },
}

pub struct BackupManager;

impl BackupManager {
    /// Create a backup of `db` at `dest`.
    ///
    /// Steps: checkpoint the WAL so the main file is self-contained, copy to a
    /// temporary sibling, run `integrity_check` on the copy, then atomically
    /// rename it into place. A failure anywhere removes the temporary file and
    /// leaves any pre-existing backup untouched.
    pub fn create(db: &Database, dest: &Path) -> anyhow::Result<BackupManifest> {
        let conn = db.connection();

        // Fold the WAL into the main database so a file copy is complete.
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;

        let src: String =
            conn.query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))?;
        if src.is_empty() {
            anyhow::bail!("cannot back up an in-memory database");
        }

        let tmp = dest.with_extension("tmp-backup");
        if tmp.exists() {
            std::fs::remove_file(&tmp)?;
        }
        std::fs::copy(&src, &tmp)?;

        // Verify the copy is a structurally valid database before promoting it.
        let integrity = Self::verify_integrity(&tmp)?;
        if integrity != "ok" {
            let _ = std::fs::remove_file(&tmp);
            anyhow::bail!("backup failed integrity_check: {integrity}");
        }

        let attempt_rows = Self::count_attempts(&tmp)?;
        let bytes = std::fs::metadata(&tmp)?.len();
        let checksum = Self::sha256_file(&tmp)?;
        let encrypted = Self::looks_encrypted(&tmp)?;

        // Atomic promotion: readers see either the old file or the new one.
        std::fs::rename(&tmp, dest)?;

        Ok(BackupManifest {
            path: dest.to_path_buf(),
            checksum,
            bytes,
            integrity,
            encrypted,
            attempt_rows,
        })
    }

    /// Restore `db` from the backup at `source`.
    ///
    /// The digest is verified *before* live state is replaced, and the live
    /// database is staged and integrity-checked prior to the swap, so a
    /// corrupted or truncated archive cannot damage existing data.
    ///
    /// Note that `PRAGMA integrity_check` alone is not sufficient: SQLite files
    /// contain free pages, so a corrupted byte can pass the structural check
    /// while still changing stored data. Pass the digest recorded at backup time
    /// via [`BackupManager::restore_verified`] to detect that case.
    pub fn restore(db: &mut Database, source: &Path) -> anyhow::Result<RestoreOutcome> {
        Self::restore_verified(db, source, None)
    }

    /// Restore, optionally requiring the archive to match `expected_checksum`.
    ///
    /// When a digest is supplied it is checked before anything is touched, which
    /// is what makes tampering or bit-rot detectable rather than silently
    /// restored.
    pub fn restore_verified(
        db: &mut Database,
        source: &Path,
        expected_checksum: Option<&str>,
    ) -> anyhow::Result<RestoreOutcome> {
        if !source.exists() {
            anyhow::bail!("backup not found at {}", source.display());
        }

        // 1. Content check first: it catches corruption that structural
        //    integrity_check cannot see (free-page flips, silent bit-rot).
        if let Some(expected) = expected_checksum {
            let actual = Self::sha256_file(source)?;
            if actual != expected {
                anyhow::bail!(
                    "backup at {} does not match the recorded digest (expected {expected}, got {actual})",
                    source.display()
                );
            }
        }

        // 2. Structural validity of the archive itself.
        let integrity = Self::verify_integrity(source)?;
        if integrity != "ok" {
            anyhow::bail!("backup at {} is corrupt: {integrity}", source.display());
        }

        let rows = Self::count_attempts(source)?;

        // 2. Stage the archive beside the live database, then open it as a real
        //    SQLite database to confirm it is readable before any swap.
        let live: String = db
            .connection()
            .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))?;
        if live.is_empty() {
            anyhow::bail!("cannot restore into an in-memory database");
        }
        let live_path = PathBuf::from(&live);
        let staged = live_path.with_extension("restore-stage");
        if staged.exists() {
            std::fs::remove_file(&staged)?;
        }
        std::fs::copy(source, &staged)?;

        {
            let probe = match rusqlite::Connection::open(&staged) {
                Ok(c) => c,
                Err(e) => {
                    let _ = std::fs::remove_file(&staged);
                    anyhow::bail!("backup could not be opened as a database: {e}");
                }
            };
            let check: String = probe.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if check != "ok" {
                let _ = std::fs::remove_file(&staged);
                anyhow::bail!("staged backup failed integrity_check: {check}");
            }
        }

        // 3. Close the live connection before touching the file. Windows holds a
        //    mandatory lock on open database files, so the swap below would fail
        //    with a sharing violation if this connection stayed open.
        let placeholder = Self::close_and_placeholder(db);
        if let Err(e) = placeholder {
            let _ = std::fs::remove_file(&staged);
            return Err(e);
        }

        // 4. Move the live file aside rather than deleting it, so a failure part
        //    way through can still be undone. WAL sidecars must go too, or
        //    SQLite would replay them against the restored file.
        let displaced = live_path.with_extension("pre-restore");
        if displaced.exists() {
            let _ = std::fs::remove_file(&displaced);
        }

        let cleanup = |keep_displaced: bool| {
            for suffix in ["-wal", "-shm"] {
                let sidecar = PathBuf::from(format!("{}{suffix}", live_path.display()));
                if sidecar.exists() {
                    let _ = std::fs::remove_file(&sidecar);
                }
            }
            if keep_displaced {
                let _ = std::fs::remove_file(&displaced);
            }
        };

        if let Err(e) = std::fs::rename(&live_path, &displaced) {
            cleanup(false);
            // Put a usable database back before reporting the failure.
            *db = Database::open(&live_path)?;
            let _ = std::fs::remove_file(&staged);
            return Err(anyhow::anyhow!("could not displace live database: {e}"));
        }

        if let Err(e) = std::fs::rename(&staged, &live_path) {
            // Roll the original back so the learner is never left with no data.
            let _ = std::fs::rename(&displaced, &live_path);
            cleanup(false);
            *db = Database::open(&live_path)?;
            let _ = std::fs::remove_file(&staged);
            return Err(anyhow::anyhow!("could not install restored database: {e}"));
        }

        cleanup(true);
        *db = Database::open(&live_path)?;

        Ok(RestoreOutcome::Restored { rows })
    }

    /// Restore when the live database cannot be opened at all.
    ///
    /// `restore_verified` needs a handle on the live file: it reads the path from the open
    /// connection and swaps the bytes underneath it. That is correct for a store that is
    /// merely wrong, and useless for one that is *malformed* -- SQLite refuses to open it, so
    /// the documented restore path fails before it can do anything, which is the state a
    /// truncated or bit-rotted file is in. Measured in round 30: of three hard failures, the
    /// corrupt-page and deleted cases recovered and the truncated one refused with
    /// "database disk image is malformed".
    ///
    /// So this variant starts from the path instead of the handle, runs the same checks on the
    /// archive -- checksum, integrity, readability -- and swaps the file with no connection to
    /// the damaged database at all. The damaged file is moved aside rather than deleted, so a
    /// failure part way through can still be undone.
    pub fn restore_verified_at(
        live_path: &Path,
        source: &Path,
        expected_checksum: Option<&str>,
    ) -> anyhow::Result<RestoreOutcome> {
        if !source.exists() {
            anyhow::bail!("backup not found at {}", source.display());
        }
        if let Some(expected) = expected_checksum {
            let actual = Self::sha256_file(source)?;
            if actual != expected {
                anyhow::bail!(
                    "backup at {} does not match the recorded digest (expected {expected}, got {actual})",
                    source.display()
                );
            }
        }
        let integrity = Self::verify_integrity(source)?;
        if integrity != "ok" {
            anyhow::bail!("backup at {} is corrupt: {integrity}", source.display());
        }
        let rows = Self::count_attempts(source)?;

        let staged = live_path.with_extension("restore-stage");
        if staged.exists() {
            std::fs::remove_file(&staged)?;
        }
        std::fs::copy(source, &staged)?;
        {
            let probe = match rusqlite::Connection::open(&staged) {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = std::fs::remove_file(&staged);
                    anyhow::bail!("backup could not be opened as a database: {error}");
                }
            };
            let check: String = probe.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if check != "ok" {
                let _ = std::fs::remove_file(&staged);
                anyhow::bail!("staged backup failed integrity_check: {check}");
            }
        }

        let displaced = live_path.with_extension("pre-restore");
        if displaced.exists() {
            let _ = std::fs::remove_file(&displaced);
        }
        let remove_sidecars = || {
            for suffix in ["-wal", "-shm"] {
                let sidecar = PathBuf::from(format!("{}{suffix}", live_path.display()));
                if sidecar.exists() {
                    let _ = std::fs::remove_file(&sidecar);
                }
            }
        };
        if live_path.exists() {
            if let Err(error) = std::fs::rename(live_path, &displaced) {
                let _ = std::fs::remove_file(&staged);
                return Err(anyhow::anyhow!("could not displace live database: {error}"));
            }
        }
        if let Err(error) = std::fs::rename(&staged, live_path) {
            // Put the damaged file back rather than leaving the learner with nothing.
            let _ = std::fs::rename(&displaced, live_path);
            remove_sidecars();
            let _ = std::fs::remove_file(&staged);
            return Err(anyhow::anyhow!(
                "could not install restored database: {error}"
            ));
        }
        remove_sidecars();
        let _ = std::fs::remove_file(&displaced);

        Ok(RestoreOutcome::Restored { rows })
    }

    /// Replace the caller's handle with a throwaway in-memory database so the
    /// live file lock is released. The temporary handle is never used for reads;
    /// the caller rebinds `db` to the real file afterwards.
    ///
    /// If `db` cannot be reopened later, the previous handle is already gone, so
    /// this returns an error and the caller rebinds before reporting.
    fn close_and_placeholder(db: &mut Database) -> anyhow::Result<()> {
        let placeholder = Database::open_in_memory()?;
        let old = std::mem::replace(db, placeholder);
        drop(old);
        Ok(())
    }

    /// Run SQLite's integrity check against an arbitrary database file.
    fn verify_integrity(path: &Path) -> anyhow::Result<String> {
        let conn = match rusqlite::Connection::open(path) {
            Ok(c) => c,
            Err(e) => return Ok(format!("unreadable: {e}")),
        };
        let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        Ok(result)
    }

    fn count_attempts(path: &Path) -> anyhow::Result<i64> {
        let conn = rusqlite::Connection::open(path)?;
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attempts'",
            [],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Ok(0);
        }
        Ok(conn.query_row("SELECT COUNT(*) FROM attempts", [], |row| row.get(0))?)
    }

    /// SHA-256 of a file, streamed so large databases do not load into memory.
    pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
        use std::io::Read;

        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Detect an encryption header. Plain SQLite files begin with "SQLite
    /// format 3"; anything else is treated as an encrypted/opaque container so
    /// the manifest reports `encrypted` truthfully rather than assuming.
    fn looks_encrypted(path: &Path) -> anyhow::Result<bool> {
        use std::io::Read;

        let mut file = std::fs::File::open(path)?;
        let mut header = [0u8; 16];
        let n = file.read(&mut header)?;
        Ok(n < 16 || &header != b"SQLite format 3\0")
    }
}
