use crate::mastery::MasteryState;
use serde::{Deserialize, Serialize};
use vector_domain::subtests::AsvabSubtest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsrsCard {
    pub stability: f64,
    pub difficulty: f64,
    pub elapsed_days: u32,
    pub scheduled_days: u32,
    pub reps: u32,
    pub state: FsrsState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsrsState {
    New,
    Learning,
    Review,
    Relearning,
}

impl Default for FsrsCard {
    fn default() -> Self {
        Self {
            stability: 1.0,
            difficulty: 5.0,
            elapsed_days: 0,
            scheduled_days: 1,
            reps: 0,
            state: FsrsState::New,
        }
    }
}

impl FsrsCard {
    pub fn review(&mut self, rating: u8) {
        // rating: 1=Again, 2=Hard, 3=Good, 4=Easy
        self.reps += 1;
        match rating {
            1 => {
                self.state = FsrsState::Relearning;
                self.stability = (self.stability * 0.5).max(0.1);
                self.difficulty = (self.difficulty + 1.0).min(10.0);
                self.scheduled_days = 1;
            }
            2 => {
                self.state = FsrsState::Review;
                self.stability *= 1.2;
                self.scheduled_days = (self.scheduled_days as f64 * 1.2).ceil() as u32;
            }
            3 => {
                self.state = FsrsState::Review;
                self.stability *= 2.0;
                self.scheduled_days = (self.scheduled_days as f64 * 2.0).ceil() as u32;
            }
            4 => {
                self.state = FsrsState::Review;
                self.stability *= 3.0;
                self.difficulty = (self.difficulty - 0.5).max(1.0);
                self.scheduled_days = (self.scheduled_days as f64 * 3.0).ceil() as u32;
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPlanItem {
    pub subtest: AsvabSubtest,
    pub recommended_minutes: u32,
    pub priority_score: f64,
}

pub struct AdaptivePlanner;

impl AdaptivePlanner {
    pub fn generate_daily_plan(
        masteries: &[MasteryState],
        available_minutes: u32,
    ) -> Vec<DailyPlanItem> {
        let mut items: Vec<DailyPlanItem> = masteries
            .iter()
            .map(|m| {
                // Priority is higher for lower mastery and higher uncertainty
                let priority = (1.0 - m.mastery_level) * 0.7 + m.uncertainty * 0.3;
                DailyPlanItem {
                    subtest: m.subtest,
                    recommended_minutes: 0,
                    priority_score: priority,
                }
            })
            .collect();

        items.sort_by(|a, b| b.priority_score.partial_cmp(&a.priority_score).unwrap());

        let total_priority: f64 = items.iter().map(|i| i.priority_score).sum();
        if total_priority > 0.0 {
            for item in &mut items {
                item.recommended_minutes = ((item.priority_score / total_priority)
                    * available_minutes as f64)
                    .round() as u32;
            }
        }

        items
    }
}
