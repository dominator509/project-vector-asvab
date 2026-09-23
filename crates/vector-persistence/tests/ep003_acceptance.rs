//! EP-003 acceptance oracle: data and persistence.
//!
//! Requirements: REQ-010 (offline analytics), REQ-020 (evidence vault),
//! REQ-032 (PR records, no auto-merge), REQ-033 (atomic integrity-checked backup),
//! REQ-054 (idempotent attempts, safe concurrency).
//!
//! SPEC-002 requires: migration from N-1, backup/restore, crash interruption,
//! concurrent readers with serialized writers, foreign-key integrity,
//! content-pack rollback, and exact readback after restart.
//!
//! These tests are the acceptance oracle. They exercise the real public
//! boundary against an on-disk SQLite file so that restart persistence,
//! WAL recovery, and constraint enforcement are genuinely verified.

use std::path::Path;

use vector_persistence::backup::{BackupManager, RestoreOutcome};
use vector_persistence::repo::{
    AttemptRepo, EvidenceRepo, MasteryRepo, NewEvidence, PullRequestRepo,
};
use vector_persistence::{Database, Migration, MigrationManager};

fn migrations() -> Vec<Migration> {
    MigrationManager::load_from_dir(Path::new("../../migrations")).expect("migrations load")
}

fn open_db(dir: &tempdir::TempDir) -> Database {
    let path = dir.path().join("vector.db");
    Database::open(&path).expect("open db")
}

/// Migrate and seed the learner profile that attempts/mastery reference.
///
/// `attempts` and `mastery` carry a real foreign key to `learner_profile`, so
/// tests must create the learner the same way production does.
fn open_db_with_learner(dir: &tempdir::TempDir, learner_id: &str) -> Database {
    let mut db = open_db(dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    seed_learner(&db, learner_id);
    db
}

fn seed_learner(db: &Database, learner_id: &str) {
    db.connection()
        .execute(
            "INSERT OR IGNORE INTO learner_profile (id, name, target_score)
             VALUES (?1, 'Test Learner', 50)",
            [learner_id],
        )
        .expect("seed learner profile");
}

/// Minimal dependency-free temp dir so the test does not pull a new crate.
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "vector-ep003-{}-{}-{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            );
            path.push(unique);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

// ---------------------------------------------------------------------------
// REQ-033 / SPEC-002: migration applies, is monotonic, and is idempotent.
// ---------------------------------------------------------------------------

#[test]
fn migration_applies_and_is_idempotent() {
    let dir = tempdir::TempDir::new("migrate");
    let mut db = open_db(&dir);

    let first = MigrationManager::apply(&mut db, &migrations()).expect("first apply");
    assert!(first >= 1, "at least one migration must apply, got {first}");

    // Re-running must not re-apply anything: migrations are monotonic.
    let second = MigrationManager::apply(&mut db, &migrations()).expect("second apply");
    assert_eq!(second, 0, "migrations must not re-apply");

    let recorded = MigrationManager::applied_versions(&db).expect("applied versions");
    assert!(
        recorded.contains(&1),
        "version 1 must be recorded, got {recorded:?}"
    );
}

#[test]
fn migration_from_n_minus_one_preserves_existing_rows() {
    let dir = tempdir::TempDir::new("nminus1");
    let mut db = open_db(&dir);

    let all = migrations();
    assert!(
        all.len() >= 2,
        "expected at least two migrations to prove N-1"
    );

    // Apply only the first migration, insert a row, then apply the rest.
    MigrationManager::apply(&mut db, &all[..1]).expect("apply v1");
    db.connection()
        .execute(
            "INSERT INTO learner_profile (id, name, target_score) VALUES ('p1','Ada',72)",
            [],
        )
        .expect("insert profile under v1");

    let applied = MigrationManager::apply(&mut db, &all).expect("apply remainder");
    assert!(applied >= 1, "later migrations must apply on top of v1");

    let name: String = db
        .connection()
        .query_row("SELECT name FROM learner_profile WHERE id='p1'", [], |r| {
            r.get(0)
        })
        .expect("profile survives migration");
    assert_eq!(name, "Ada", "N-1 data must survive migration");
}

// ---------------------------------------------------------------------------
// SPEC-002: exact readback after restart (real file, new connection).
// ---------------------------------------------------------------------------

#[test]
fn mastery_survives_restart_with_exact_readback() {
    let dir = tempdir::TempDir::new("restart");
    let path = dir.path().join("vector.db");

    {
        let mut db = Database::open(&path).expect("open");
        MigrationManager::apply(&mut db, &migrations()).expect("migrate");
        seed_learner(&db, "learner-1");
        let repo = MasteryRepo::new(&db);
        repo.upsert("learner-1", "AR", 0.42, 0.31).expect("upsert");
    } // connection dropped: simulates process exit

    let mut db = Database::open(&path).expect("reopen");
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let repo = MasteryRepo::new(&db);
    let m = repo
        .get("learner-1", "AR")
        .expect("query")
        .expect("row must persist across restart");

    assert_eq!(m.learner_id, "learner-1");
    assert_eq!(m.subtest, "AR");
    assert!((m.score - 0.42).abs() < 1e-9, "score readback exact");
    assert!(
        (m.uncertainty - 0.31).abs() < 1e-9,
        "uncertainty readback exact"
    );
}

// ---------------------------------------------------------------------------
// REQ-010: offline mastery analytics aggregate real stored attempts.
// ---------------------------------------------------------------------------

#[test]
fn analytics_aggregate_persisted_attempts() {
    let dir = tempdir::TempDir::new("analytics");
    let db = open_db_with_learner(&dir, "learner-1");
    let attempts = AttemptRepo::new(&db);

    attempts
        .record("learner-1", "AR", "q1", true, 1200)
        .expect("record");
    attempts
        .record("learner-1", "AR", "q2", false, 3000)
        .expect("record");
    attempts
        .record("learner-1", "AR", "q3", true, 1800)
        .expect("record");

    let stats = attempts.analytics("learner-1", "AR").expect("analytics");
    assert_eq!(stats.total, 3);
    assert_eq!(stats.correct, 2);
    assert!((stats.accuracy - (2.0 / 3.0)).abs() < 1e-9, "accuracy");
    assert_eq!(stats.mean_latency_ms, 2000, "mean speed");

    // A different subtest must not leak into this aggregate.
    let other = attempts.analytics("learner-1", "WK").expect("analytics");
    assert_eq!(other.total, 0, "subtests are isolated");
}

// ---------------------------------------------------------------------------
// REQ-054: attempts are idempotent under retry (exactly-once effect).
// ---------------------------------------------------------------------------

#[test]
fn attempt_recording_is_idempotent_by_id() {
    let dir = tempdir::TempDir::new("idempotent");
    let db = open_db_with_learner(&dir, "learner-1");
    let attempts = AttemptRepo::new(&db);

    let first = attempts
        .record_idempotent("attempt-abc", "learner-1", "AR", "q1", true, 1200)
        .expect("first");
    assert!(first, "first write inserts");

    // Same logical attempt replayed (retry / double-submit) must be a no-op.
    let second = attempts
        .record_idempotent("attempt-abc", "learner-1", "AR", "q1", true, 1200)
        .expect("retry");
    assert!(!second, "replayed attempt must not insert again");

    let stats = attempts.analytics("learner-1", "AR").expect("analytics");
    assert_eq!(stats.total, 1, "exactly-once semantics");
}

// ---------------------------------------------------------------------------
// REQ-020: evidence vault records are hash-addressed and immutable.
// ---------------------------------------------------------------------------

#[test]
fn evidence_vault_stores_snapshots_and_rejects_mutation() {
    let dir = tempdir::TempDir::new("vault");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let vault = EvidenceRepo::new(&db);

    let id = vault
        .put(&NewEvidence::new(
            "https://example.org/source",
            "Example Source",
            "sha256:abc123",
            "CC-BY-4.0",
            "2026-09-01",
            0.8,
            "retrieved",
        ))
        .expect("put");

    let row = vault.get(&id).expect("get").expect("present");
    assert_eq!(row.url, "https://example.org/source");
    assert_eq!(row.license, "CC-BY-4.0");
    assert!((row.trust - 0.8).abs() < 1e-9);
    assert_eq!(row.retrieval_status, "retrieved");

    // Identical content is deduplicated by content hash, not duplicated.
    let again = vault
        .put(&NewEvidence::new(
            "https://mirror.example.org/source",
            "Mirror",
            "sha256:abc123",
            "CC-BY-4.0",
            "2026-09-01",
            0.8,
            "retrieved",
        ))
        .expect("put dup");
    assert_eq!(again, id, "content hash is the identity");

    // Evidence snapshots are immutable: an update must be refused.
    let mutated = vault.set_trust(&id, 0.1);
    assert!(mutated.is_err(), "vault rows must be immutable");

    let after = vault.get(&id).expect("get").expect("present");
    assert!(
        (after.trust - 0.8).abs() < 1e-9,
        "trust unchanged after refusal"
    );
}

// ---------------------------------------------------------------------------
// REQ-032: PR records carry approval state and can never auto-merge.
// ---------------------------------------------------------------------------

#[test]
fn pr_records_require_explicit_approval_and_never_auto_merge() {
    let dir = tempdir::TempDir::new("pr");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let prs = PullRequestRepo::new(&db);

    let id = prs
        .record("vector-repair", "fix crash on resume", "diff --git a b")
        .expect("record");

    let pr = prs.get(&id).expect("get").expect("present");
    assert!(!pr.approved, "new PR starts unapproved");
    assert!(!pr.merged, "new PR starts unmerged");

    // Merging before explicit human approval is forbidden. The refusal must come
    // from the explicit approval guard, not merely from a downstream SQL
    // predicate, otherwise removing the guard would go undetected by this test.
    let premature = prs
        .merge(&id, "automation")
        .expect_err("merge without approval must fail");
    let message = premature.to_string();
    assert!(
        message.contains("no explicit approval"),
        "refusal must name the missing approval, got: {message}"
    );

    // The row must be untouched by the refused merge attempt.
    let pr = prs.get(&id).expect("get").expect("present");
    assert!(!pr.merged, "a refused merge must not mark the PR merged");

    prs.approve(&id, "human-reviewer").expect("approve");
    let merged = prs.merge(&id, "human-reviewer").expect("merge");
    assert!(merged, "merge after approval succeeds");

    let pr = prs.get(&id).expect("get").expect("present");
    assert!(pr.merged);
    assert_eq!(pr.approved_by.as_deref(), Some("human-reviewer"));
}

#[test]
fn approval_flag_without_an_approver_cannot_be_merged() {
    // The database CHECK constraint is the last line of defence, so it must be
    // proven independently of the Rust guard. Setting approved=1 directly (as a
    // buggy migration or a future code path might) must still be unmergeable.
    let dir = tempdir::TempDir::new("pr-null-approver");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let prs = PullRequestRepo::new(&db);

    let id = prs
        .record("vector-repair", "sneaky merge", "diff --git a b")
        .expect("record");

    // Attempt to record approval without naming a human approver.
    let dangling = db.connection().execute(
        "UPDATE pull_requests SET approved = 1, approved_by = NULL WHERE id = ?1",
        [&id],
    );
    assert!(
        dangling.is_err(),
        "a row claiming approval with no approver must be rejected by the schema"
    );

    // And the repository must refuse to merge it regardless.
    let merged = prs.merge(&id, "automation");
    assert!(merged.is_err(), "unapproved PR must never merge");
}

#[test]
fn merge_is_not_repeatable() {
    let dir = tempdir::TempDir::new("pr-idempotent");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let prs = PullRequestRepo::new(&db);

    let id = prs.record("vector-repair", "t", "diff").expect("record");
    prs.approve(&id, "human").expect("approve");

    assert!(
        prs.merge(&id, "human").expect("first merge"),
        "first merge applies"
    );
    assert!(
        !prs.merge(&id, "human").expect("second merge"),
        "re-merging an already merged PR is a no-op, not a second merge"
    );
}

// ---------------------------------------------------------------------------
// REQ-033: atomic, integrity-checked backup and restore.
// ---------------------------------------------------------------------------

#[test]
fn backup_is_integrity_checked_and_restores_exact_state() {
    let dir = tempdir::TempDir::new("backup");
    let mut db = open_db_with_learner(&dir, "learner-1");
    AttemptRepo::new(&db)
        .record("learner-1", "AR", "q1", true, 1200)
        .expect("record");
    let last = AttemptRepo::new(&db)
        .record("learner-1", "AR", "q2", false, 2500)
        .expect("record");

    let backup_path = dir.path().join("backup.sqlite");
    let manifest = BackupManager::create(&db, &backup_path).expect("create backup");

    assert!(backup_path.exists(), "backup file must exist");
    assert_eq!(manifest.integrity, "ok", "backup must pass integrity_check");
    assert_eq!(manifest.checksum.len(), 64, "sha256 hex digest");
    assert!(manifest.attempt_rows >= 2, "manifest counts rows");

    // Mutate the live DB after the backup; restore must roll it back.
    AttemptRepo::new(&db)
        .record("learner-1", "AR", "q3", true, 900)
        .expect("record");
    assert_eq!(
        AttemptRepo::new(&db)
            .analytics("learner-1", "AR")
            .expect("analytics")
            .total,
        3
    );

    let outcome = BackupManager::restore(&mut db, &backup_path).expect("restore");
    assert_eq!(outcome, RestoreOutcome::Restored { rows: 2 });

    let stats = AttemptRepo::new(&db)
        .analytics("learner-1", "AR")
        .expect("analytics");
    assert_eq!(stats.total, 2, "restore rolls back to snapshot state");
    let _ = last;
}

#[test]
fn restore_rejects_a_corrupted_backup() {
    let dir = tempdir::TempDir::new("corrupt");
    let mut db = open_db_with_learner(&dir, "learner-1");
    AttemptRepo::new(&db)
        .record("learner-1", "AR", "q1", true, 1200)
        .expect("record");

    let backup_path = dir.path().join("backup.sqlite");
    let manifest = BackupManager::create(&db, &backup_path).expect("create backup");

    // Flip bytes in the middle of the file to simulate corruption.
    let mut bytes = std::fs::read(&backup_path).expect("read backup");
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    bytes[mid + 1] ^= 0xFF;
    std::fs::write(&backup_path, &bytes).expect("write corrupted");

    // A SQLite file contains free pages, so a flipped byte can pass
    // integrity_check while still altering content. The recorded digest is what
    // makes tampering detectable, so it must be enforced.
    let unverified = BackupManager::restore(&mut db, &backup_path);
    let verified = BackupManager::restore_verified(&mut db, &backup_path, Some(&manifest.checksum));

    assert!(
        unverified.is_err() || verified.is_err(),
        "corrupted backup must be rejected, not silently applied \
         (integrity_check={unverified:?}, digest={verified:?})"
    );
    assert!(
        verified.is_err(),
        "a digest mismatch must always be rejected, got {verified:?}"
    );

    // Live data must be untouched by the refused restores.
    let stats = AttemptRepo::new(&db)
        .analytics("learner-1", "AR")
        .expect("analytics");
    assert_eq!(stats.total, 1, "refused restore must not alter live data");
}

#[test]
fn restore_rejects_a_truncated_backup() {
    let dir = tempdir::TempDir::new("truncated");
    let mut db = open_db_with_learner(&dir, "learner-1");
    AttemptRepo::new(&db)
        .record("learner-1", "AR", "q1", true, 1200)
        .expect("record");

    let backup_path = dir.path().join("backup.sqlite");
    BackupManager::create(&db, &backup_path).expect("create backup");

    let bytes = std::fs::read(&backup_path).expect("read backup");
    std::fs::write(&backup_path, &bytes[..bytes.len() / 3]).expect("truncate");

    let result = BackupManager::restore(&mut db, &backup_path);
    assert!(result.is_err(), "truncated backup must be rejected");

    let stats = AttemptRepo::new(&db)
        .analytics("learner-1", "AR")
        .expect("analytics");
    assert_eq!(stats.total, 1, "live data survives a refused restore");
}

#[test]
fn backup_digest_matches_the_archive_on_disk() {
    let dir = tempdir::TempDir::new("digest");
    let db = open_db_with_learner(&dir, "learner-1");
    AttemptRepo::new(&db)
        .record("learner-1", "AR", "q1", true, 1200)
        .expect("record");

    let backup_path = dir.path().join("backup.sqlite");
    let manifest = BackupManager::create(&db, &backup_path).expect("create backup");

    // The manifest digest must actually describe the file that was written,
    // otherwise integrity reporting is decorative.
    let recomputed = BackupManager::sha256_file(&backup_path).expect("hash");
    assert_eq!(
        manifest.checksum, recomputed,
        "manifest digest matches file"
    );
    assert_eq!(
        manifest.bytes,
        std::fs::metadata(&backup_path).unwrap().len()
    );

    // A clean archive restores under digest verification.
    let mut db = db;
    let outcome = BackupManager::restore_verified(&mut db, &backup_path, Some(&manifest.checksum))
        .expect("verified restore of a clean backup");
    assert_eq!(outcome, RestoreOutcome::Restored { rows: 1 });
}

/// A store too damaged to open is exactly the case a restore exists for.
///
/// Found by the DOD-036 recovery measurement in round 30: of three hard failures on a real
/// store, a corrupted page and a deleted file restored, and a *truncated* file did not --
/// `Database::open` failed with "database disk image is malformed" before any restore logic
/// ran, so the documented path could not recover the learner's data at all.
#[test]
fn a_truncated_store_is_restored_from_its_archive() {
    let dir = tempdir::TempDir::new("restore-truncated");
    let path = dir.path().join("vector.db");
    let backup_path = dir.path().join("backup.sqlite");

    let (checksum, before_rows) = {
        let mut db = open_db(&dir);
        MigrationManager::apply(&mut db, &migrations()).expect("migrate");
        db.connection()
            .execute(
                "INSERT INTO learner_profile (id, name, target_score, created_at)
                 VALUES ('learner-1','Ada',60,'2026-09-10T00:00:00Z')",
                [],
            )
            .expect("learner");
        AttemptRepo::new(&db)
            .record("learner-1", "AR", "q1", true, 1200)
            .expect("record");
        AttemptRepo::new(&db)
            .record("learner-1", "AR", "q2", false, 1500)
            .expect("record");
        let manifest = BackupManager::create(&db, &backup_path).expect("create backup");
        let rows: i64 = db
            .connection()
            .query_row("SELECT COUNT(*) FROM attempts", [], |row| row.get(0))
            .expect("count");
        (manifest.checksum, rows)
    };

    // Truncate the live file in half, the way a power loss or a bad copy leaves it.
    let length = std::fs::metadata(&path).expect("metadata").len();
    {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open live file");
        file.set_len(length / 2).expect("truncate");
    }
    assert!(
        Database::open(&path).is_err(),
        "the fixture has to be a store that cannot be opened, or this test proves nothing"
    );

    // Without a handle to the damaged store, the archive still restores and reconciles.
    let outcome =
        BackupManager::restore_verified_at(&path, &backup_path, Some(&checksum)).expect("restore");
    assert_eq!(outcome, RestoreOutcome::Restored { rows: before_rows });

    let mut restored = Database::open(&path).expect("the restored store opens");
    MigrationManager::apply(&mut restored, &migrations()).expect("migrations still apply");
    let rows: i64 = restored
        .connection()
        .query_row("SELECT COUNT(*) FROM attempts", [], |row| row.get(0))
        .expect("count");
    assert_eq!(rows, before_rows, "the restored store holds what it held");
    let integrity: String = restored
        .connection()
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity");
    assert_eq!(integrity, "ok");
}

// ---------------------------------------------------------------------------
// SPEC-002: foreign-key integrity is enforced, not merely declared.
// ---------------------------------------------------------------------------

#[test]
fn foreign_key_integrity_is_enforced() {
    let dir = tempdir::TempDir::new("fk");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");

    // Attempts reference a learner profile; an orphan must be rejected.
    let orphan = db.connection().execute(
        "INSERT INTO attempts (id, learner_id, subtest, question_id, correct, latency_ms, created_at)
         VALUES ('a-orphan','no-such-learner','AR','q1',1,100,'2026-09-10T00:00:00Z')",
        [],
    );
    assert!(
        orphan.is_err(),
        "foreign key violation must be rejected (got {orphan:?})"
    );
}

// ---------------------------------------------------------------------------
// SPEC-002: concurrent readers with serialized writers.
// ---------------------------------------------------------------------------

#[test]
fn concurrent_readers_with_serialized_writers() {
    use std::sync::Arc;

    let dir = tempdir::TempDir::new("concurrent");
    let path = dir.path().join("vector.db");

    {
        let mut db = Database::open(&path).expect("open");
        MigrationManager::apply(&mut db, &migrations()).expect("migrate");
        db.connection()
            .execute(
                "INSERT INTO learner_profile (id, name, target_score) VALUES ('learner-1','Ada',72)",
                [],
            )
            .expect("seed profile");
    }

    let writers = 4;
    let per_writer = 10;
    let mut handles = Vec::new();

    // Writers open independent connections; SQLite must serialize them safely.
    for w in 0..writers {
        let path = Arc::new(path.clone());
        handles.push(std::thread::spawn(move || {
            let db = Database::open(path.as_path()).expect("open writer");
            let repo = AttemptRepo::new(&db);
            for i in 0..per_writer {
                repo.record(
                    "learner-1",
                    "AR",
                    &format!("q-{w}-{i}"),
                    i % 2 == 0,
                    1000 + i as i64,
                )
                .expect("concurrent record");
            }
        }));
    }

    // Concurrent readers must not be blocked into failure by writers.
    for _ in 0..2 {
        let path = Arc::new(path.clone());
        handles.push(std::thread::spawn(move || {
            let db = Database::open(path.as_path()).expect("open reader");
            let repo = AttemptRepo::new(&db);
            for _ in 0..10 {
                let _ = repo.analytics("learner-1", "AR").expect("read");
            }
        }));
    }

    for h in handles {
        h.join().expect("thread must not panic");
    }

    // Every writer's work must be durable: no lost updates.
    let db = Database::open(&path).expect("reopen");
    let stats = AttemptRepo::new(&db)
        .analytics("learner-1", "AR")
        .expect("analytics");
    assert_eq!(
        stats.total,
        (writers * per_writer) as i64,
        "all concurrent writes must persist"
    );
}

// ---------------------------------------------------------------------------
// SPEC-002: content-pack rollback restores the prior active version.
// ---------------------------------------------------------------------------

#[test]
fn content_pack_rollback_restores_previous_active_pack() {
    let dir = tempdir::TempDir::new("packrollback");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let packs = vector_persistence::repo::ContentPackRepo::new(&db);

    // A registry row now has to record the identity installation verifies: which key
    // signed it, what content the signature covers, and what the pack holds. A row
    // carrying only a signature string could not answer any of those questions, so
    // the schema refuses it as `active`.
    fn publish<'a>(
        name: &'a str,
        version: i64,
        signature: &'a str,
        content_hash: &'a str,
    ) -> vector_persistence::repo::NewPackRecord<'a> {
        vector_persistence::repo::NewPackRecord {
            name,
            version,
            signature,
            signer: "dGVzdC1rZXk=",
            content_hash,
            schema_version: 1,
            manifest_json: "{}",
            item_count: 1,
        }
    }

    packs
        .publish(&publish(
            "core-asvab",
            1,
            "sha256:v1-signature",
            "sha256:v1",
        ))
        .expect("publish v1");
    packs
        .publish(&publish(
            "core-asvab",
            2,
            "sha256:v2-signature",
            "sha256:v2",
        ))
        .expect("publish v2");

    let active = packs.active("core-asvab").expect("active").expect("some");
    assert_eq!(active.version, 2, "newest published pack is active");
    assert_eq!(active.content_hash, "sha256:v2");

    let rolled = packs.rollback("core-asvab").expect("rollback");
    assert_eq!(rolled.version, 1, "rollback restores prior version");

    let active = packs.active("core-asvab").expect("active").expect("some");
    assert_eq!(active.version, 1, "prior version is active after rollback");
    assert_eq!(active.signature, "sha256:v1-signature");
    assert_eq!(active.content_hash, "sha256:v1");
}

/// A registry row that is serving learners must record what was verified about it.
#[test]
fn an_active_pack_without_an_identity_is_refused() {
    let dir = tempdir::TempDir::new("packidentity");
    let mut db = open_db(&dir);
    MigrationManager::apply(&mut db, &migrations()).expect("migrate");
    let packs = vector_persistence::repo::ContentPackRepo::new(&db);

    let anonymous = vector_persistence::repo::NewPackRecord {
        name: "core-asvab",
        version: 1,
        signature: "sha256:v1-signature",
        signer: "",
        content_hash: "",
        schema_version: 1,
        manifest_json: "{}",
        item_count: 0,
    };
    let error = packs
        .publish(&anonymous)
        .expect_err("an active pack with no signer, hash or item count must be refused");
    assert!(
        error.to_string().contains("signer"),
        "the refusal should name what is missing: {error}"
    );
    assert!(
        packs.active("core-asvab").expect("active").is_none(),
        "nothing may be left active by a refused publish"
    );
}

// ---------------------------------------------------------------------------
// Negative: unopened/absent database paths fail loudly, never silently.
// ---------------------------------------------------------------------------

#[test]
fn opening_a_directory_as_database_fails_loudly() {
    let dir = tempdir::TempDir::new("baddb");
    let err = Database::open(dir.path());
    assert!(err.is_err(), "opening a directory as a DB must fail");
}
