#[cfg(test)]
mod tests {
    use crate::db::{Database, MigrationManager};

    #[test]
    fn test_db_setup_and_migration_from_zero() {
        let db = Database::open_in_memory().expect("Failed to open memory db");
        let migration_1 = "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT);";
        let count = MigrationManager::apply_migrations(db.connection(), &[("1", migration_1)])
            .expect("Migration failed");
        assert_eq!(count, 1);

        // Applying the same migration again should be idempotent (0 applied)
        let count_again =
            MigrationManager::apply_migrations(db.connection(), &[("1", migration_1)])
                .expect("Second migration failed");
        assert_eq!(count_again, 0);

        // Verify table exists
        let mut stmt = db
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='test_table'")
            .unwrap();
        let exists = stmt.exists([]).unwrap();
        assert!(exists);
    }
}
