use crate::mastery::Subtest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SimulatorState {
    NotStarted,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSimulator {
    subtest: Subtest,
    duration: chrono::Duration,
    state: SimulatorState,
}

impl PaperSimulator {
    pub fn new(subtest: Subtest, duration: chrono::Duration) -> Self {
        Self {
            subtest,
            duration,
            state: SimulatorState::NotStarted,
        }
    }

    pub fn state(&self) -> SimulatorState {
        self.state.clone()
    }

    pub fn start(&mut self) {
        self.state = SimulatorState::InProgress;
    }

    pub fn can_navigate_back(&self) -> bool {
        true // Paper simulator allows navigating backward
    }
}
