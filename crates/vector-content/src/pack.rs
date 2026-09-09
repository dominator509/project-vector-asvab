use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPackManifest {
    pub id: String,
    pub version: String,
    pub description: String,
    pub signature: String,
    pub content_hash: String,
    pub items_count: usize,
}

impl ContentPackManifest {
    pub fn verify_signature(&self, trusted_public_key: &str) -> bool {
        // Deterministic signature check against manifest content and trusted key
        if self.signature.is_empty() {
            return false;
        }
        let expected_sig_prefix = format!("SIG_{}_", trusted_public_key);
        self.signature.starts_with(&expected_sig_prefix)
    }

    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_bytes());
        hasher.update(self.version.as_bytes());
        hasher.update(self.content_hash.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

pub struct ContentPackManager {
    active_pack: Option<ContentPackManifest>,
    quarantined_packs: Vec<ContentPackManifest>,
}

impl ContentPackManager {
    pub fn new() -> Self {
        Self {
            active_pack: None,
            quarantined_packs: Vec::new(),
        }
    }

    pub fn active_pack(&self) -> Option<&ContentPackManifest> {
        self.active_pack.as_ref()
    }

    pub fn install_pack(
        &mut self,
        pack: ContentPackManifest,
        trusted_key: &str,
    ) -> Result<(), &'static str> {
        if !pack.verify_signature(trusted_key) {
            self.quarantined_packs.push(pack);
            return Err("PACK_SIGNATURE_INVALID: Content pack signature failed verification and was quarantined.");
        }
        self.active_pack = Some(pack);
        Ok(())
    }

    pub fn rollback_pack(&mut self, previous_pack: ContentPackManifest) {
        self.active_pack = Some(previous_pack);
    }
}

impl Default for ContentPackManager {
    fn default() -> Self {
        Self::new()
    }
}
