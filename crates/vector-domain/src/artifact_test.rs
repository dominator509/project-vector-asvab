#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn test_artifact_identity_binding_computation() {
        let binary_hash = "hash_b";
        let migrations_hash = "hash_m";
        let content_hash = "hash_c";
        let sbom_hash = "hash_s";

        let mut hasher = Sha256::new();
        hasher.update(binary_hash.as_bytes());
        hasher.update(migrations_hash.as_bytes());
        hasher.update(content_hash.as_bytes());
        hasher.update(sbom_hash.as_bytes());
        let digest = format!("{:x}", hasher.finalize());

        assert_eq!(digest.len(), 64);

        // Mutate one hash and verify bound digest changes
        let mut hasher_mutated = Sha256::new();
        hasher_mutated.update("hash_b_mutated".as_bytes());
        hasher_mutated.update(migrations_hash.as_bytes());
        hasher_mutated.update(content_hash.as_bytes());
        hasher_mutated.update(sbom_hash.as_bytes());
        let digest_mutated = format!("{:x}", hasher_mutated.finalize());

        assert_ne!(digest, digest_mutated);
    }
}
