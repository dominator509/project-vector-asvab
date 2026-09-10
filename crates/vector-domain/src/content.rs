use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentStatus {
    Quarantined,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPack {
    name: String,
    version: u32,
    status: ContentStatus,
    freshness_timestamp: chrono::DateTime<chrono::Utc>,
    signature: Option<String>,
    reviewed: bool,
}

impl ContentPack {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version: 1,
            status: ContentStatus::Active,
            freshness_timestamp: Utc::now(),
            signature: None,
            reviewed: false,
        }
    }

    pub fn stage_update(&mut self) {
        self.status = ContentStatus::Quarantined;
        self.freshness_timestamp = Utc::now();
    }

    pub fn rollback(&mut self) {
        self.status = ContentStatus::Active;
    }

    pub fn status(&self) -> ContentStatus {
        self.status.clone()
    }

    pub fn freshness(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.freshness_timestamp)
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn signature(&self) -> Option<String> {
        self.signature.clone()
    }

    pub fn sign(&self, _key: &str) -> Self {
        let mut signed_pack = self.clone();
        signed_pack.signature = Some("dummy_signature".to_string());
        signed_pack.reviewed = true;
        signed_pack
    }

    pub fn is_reviewed(&self) -> bool {
        self.reviewed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnswerProof {
    Deterministic(String),
    Heuristic(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionItem {
    text: String,
    proof: AnswerProof,
    derived: bool,
}

impl QuestionItem {
    pub fn new(text: &str, proof: AnswerProof) -> Self {
        Self {
            text: text.to_string(),
            proof,
            derived: false,
        }
    }

    pub fn verify(&self) -> bool {
        matches!(self.proof, AnswerProof::Deterministic(_))
    }

    pub fn is_derived(&self) -> bool {
        self.derived
    }
}
