use crate::db::{Database, MigrationManager};

#[test]
fn db_setup_and_migration_from_zero() {
    let mut db = Database::open_in_memory().expect("open memory db");
    let migration_1 = (
        1i64,
        "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT);".to_string(),
    );

    let count = MigrationManager::apply(&mut db, std::slice::from_ref(&migration_1))
        .expect("migration failed");
    assert_eq!(count, 1, "first application applies the migration");

    // Re-applying the same version must be a no-op (monotonic migrations).
    let count_again = MigrationManager::apply(&mut db, std::slice::from_ref(&migration_1))
        .expect("second migration failed");
    assert_eq!(count_again, 0, "migrations must not re-apply");

    let mut stmt = db
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='test_table'")
        .expect("prepare");
    assert!(stmt.exists([]).expect("exists"), "table must exist");
}

#[test]
fn migration_failure_rolls_back_and_leaves_earlier_versions_applied() {
    let mut db = Database::open_in_memory().expect("open memory db");
    let good = (
        1i64,
        "CREATE TABLE keep_me (id INTEGER PRIMARY KEY);".to_string(),
    );
    let bad = (2i64, "THIS IS NOT VALID SQL;".to_string());

    let err = MigrationManager::apply(&mut db, &[good.clone(), bad]);
    assert!(err.is_err(), "invalid migration must fail loudly");

    // Version 1 stayed applied; version 2 is not recorded.
    let versions = MigrationManager::applied_versions(&db).expect("versions");
    assert_eq!(
        versions,
        vec![1],
        "only the successful migration is recorded"
    );
}

#[test]
fn duplicate_migration_versions_are_rejected() {
    let dir = std::env::temp_dir().join(format!("vector-dup-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("001_a.sql"), "SELECT 1;").expect("write a");
    std::fs::write(dir.join("001_b.sql"), "SELECT 2;").expect("write b");

    let result = MigrationManager::load_from_dir(&dir);
    assert!(result.is_err(), "duplicate versions must be rejected");

    let _ = std::fs::remove_dir_all(&dir);
}
