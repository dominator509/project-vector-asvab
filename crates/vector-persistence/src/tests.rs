#[cfg(test)]
mod persistence_tests {
    use rusqlite::Connection;
    use crate::repo::{MasteryRepository, EvidenceVault, PrStateRepository, AttemptRepository};
    use crate::backup::BackupManager;

    // REQ-010: offline mastery/speed/confidence/trends
    #[test]
    fn test_req_010_offline_mastery_persistence() {
        let db = Connection::open_in_memory().unwrap();
        let repo = MasteryRepository::new(&db);

        // Negative test: uninitialized user
        let empty = repo.get_mastery("user_1").unwrap();
        assert_eq!(empty, None);

        // Happy path: save and read
        repo.save_mastery("user_1", 95.5).unwrap();
        let score = repo.get_mastery("user_1").unwrap();
        assert_eq!(score, Some(95.5));

        // Persistence overwrite
        repo.save_mastery("user_1", 99.0).unwrap();
        let updated = repo.get_mastery("user_1").unwrap();
        assert_eq!(updated, Some(99.0));
    }

    // REQ-020: hash/url/license/retrieval/effective/trust/source snapshots
    #[test]
    fn test_req_020_evidence_vault() {
        let db = Connection::open_in_memory().unwrap();
        let vault = EvidenceVault::new(&db);

        // Boundary case: missing evidence
        let missing = vault.get_snapshot("missing_hash").unwrap();
        assert_eq!(missing, None);

        vault.save_snapshot("hash_abc", "evidence_content").unwrap();
        let content = vault.get_snapshot("hash_abc").unwrap();
        assert_eq!(content, Some("evidence_content".to_string()));
    }

    // REQ-032: official gh; explicit approval; no auto-merge
    #[test]
    fn test_req_032_pr_state_persistence() {
        let db = Connection::open_in_memory().unwrap();
        let repo = PrStateRepository::new(&db);

        // Start unapproved
        assert!(!repo.is_approved("pr_100").unwrap());

        // Transition to unapproved (explicit)
        repo.set_pr_approval("pr_100", false).unwrap();
        assert!(!repo.is_approved("pr_100").unwrap());

        // Transition to approved
        repo.set_pr_approval("pr_100", true).unwrap();
        assert!(repo.is_approved("pr_100").unwrap());
    }

    // REQ-033: atomic integrity-checked optional encrypted backup
    #[test]
    fn test_req_033_atomic_backup() {
        let db = Connection::open_in_memory().unwrap();
        let result = BackupManager::create_atomic_backup(&db, "backup.sqlite");
        assert!(result.is_ok());
    }

    // REQ-054: idempotent attempts and safe DB/background workers
    #[test]
    fn test_req_054_idempotent_attempts() {
        let db = Connection::open_in_memory().unwrap();
        let repo = AttemptRepository::new(&db);

        // First insertion is true
        let first = repo.record_attempt("attempt_x", "pass").unwrap();
        assert!(first);

        // Second insertion is idempotent false
        let second = repo.record_attempt("attempt_x", "pass").unwrap();
        assert!(!second);
    }
}
