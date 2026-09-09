use serde::{Deserialize, Serialize};
use vector_domain::subtests::AsvabSubtest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillObservation {
    pub subtest: AsvabSubtest,
    pub is_correct: bool,
    pub response_time_ms: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteryState {
    pub subtest: AsvabSubtest,
    pub mastery_level: f64, // 0.0 to 1.0
    pub uncertainty: f64,   // 0.0 to 1.0
    pub observation_count: usize,
}

impl MasteryState {
    pub fn new(subtest: AsvabSubtest) -> Self {
        Self {
            subtest,
            mastery_level: 0.5,
            uncertainty: 0.5,
            observation_count: 0,
        }
    }

    pub fn update(&mut self, obs: &SkillObservation) {
        self.observation_count += 1;
        let weight = 1.0 / (self.observation_count as f64).sqrt();
        let target = if obs.is_correct { 1.0 } else { 0.0 };
        self.mastery_level += weight * (target - self.mastery_level);
        self.uncertainty = (self.uncertainty * (1.0 - weight)).max(0.05);
    }
}
