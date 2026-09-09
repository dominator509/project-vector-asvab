use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerProfile {
    pub id: Uuid,
    pub name: String,
    pub target_score: u32,
}
