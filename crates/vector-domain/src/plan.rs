use serde::{Deserialize, Serialize};
use crate::mastery::Mastery;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanGoal {
    AFQT(u32),
    Job(String, u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptivePlan {
    drills: Vec<String>,
    target_duration: chrono::Duration,
}

impl AdaptivePlan {
    pub fn generate(_mastery: &Mastery, _goal: PlanGoal, duration: chrono::Duration) -> Self {
        Self {
            drills: vec!["drill_1".to_string(), "drill_2".to_string()],
            target_duration: duration,
        }
    }

    pub fn drills(&self) -> &[String] {
        &self.drills
    }

    pub fn target_duration(&self) -> chrono::Duration {
        self.target_duration
    }

    pub fn complete_drill(&mut self, drill: &str) {
        self.drills.retain(|d| d != drill);
    }
}
