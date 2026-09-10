use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerProfile {
    pub id: Uuid,
    pub name: String,
    pub target_score: u32,
}

impl LearnerProfile {
    pub fn new(name: &str, target_score: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            target_score,
        }
    }
}
