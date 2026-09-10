use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerProfile {
    pub id: Uuid,
    pub name: String,
    pub target_score: u32,
    pub onboarding_completed: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("Target score cannot be zero or excessively high")]
    InvalidTargetScore,
    #[error("Name cannot be empty")]
    InvalidName,
}

impl LearnerProfile {
    pub fn new(name: &str, target_score: u32) -> Result<Self, ProfileError> {
        if name.trim().is_empty() {
            return Err(ProfileError::InvalidName);
        }
        if target_score == 0 || target_score > 99 {
            return Err(ProfileError::InvalidTargetScore);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            target_score,
            onboarding_completed: false,
            created_at: chrono::Utc::now(),
        })
    }

    pub fn complete_onboarding(&mut self) {
        self.onboarding_completed = true;
    }
}
