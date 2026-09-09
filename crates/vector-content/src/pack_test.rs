#[cfg(test)]
mod tests {
    use super::super::pack::{ContentPackManager, ContentPackManifest};

    #[test]
    fn test_signed_versioned_content_pack_installation_and_rollback() {
        let mut manager = ContentPackManager::new();
        let pub_key = "KEY_2026";

        let v1_pack = ContentPackManifest {
            id: "asvab-prep-pack".to_string(),
            version: "1.0.0".to_string(),
            description: "Initial content pack".to_string(),
            signature: "SIG_KEY_2026_abc123".to_string(),
            content_hash: "hash_v1".to_string(),
            items_count: 500,
        };

        // Installing valid signed pack succeeds
        assert!(manager.install_pack(v1_pack.clone(), pub_key).is_ok());
        assert_eq!(manager.active_pack().unwrap().version, "1.0.0");

        // Installing invalid signed pack fails and quarantines
        let tampered_pack = ContentPackManifest {
            id: "asvab-prep-pack".to_string(),
            version: "2.0.0".to_string(),
            description: "Tampered pack".to_string(),
            signature: "INVALID_SIG".to_string(),
            content_hash: "hash_v2_tampered".to_string(),
            items_count: 500,
        };

        assert!(manager.install_pack(tampered_pack, pub_key).is_err());
        // Active pack remains v1.0.0 (rollbacked / untouched)
        assert_eq!(manager.active_pack().unwrap().version, "1.0.0");
    }
}
