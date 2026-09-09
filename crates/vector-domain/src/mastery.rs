use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Subtest {
    GS,
    AR,
    WK,
    PC,
    MK,
    EI,
    AI,
    SI,
    MC,
    AO,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mastery {
    pub diagnostic_score: f64,
    pub uncertainty: f64,
}

impl Default for Mastery {
    fn default() -> Self {
        Self::new()
    }
}

impl Mastery {
    pub fn new() -> Self {
        Self {
            diagnostic_score: 0.0,
            uncertainty: 1.0,
        }
    }

    pub fn diagnostic_score(&self) -> f64 {
        self.diagnostic_score
    }

    pub fn uncertainty(&self) -> f64 {
        self.uncertainty
    }
}
