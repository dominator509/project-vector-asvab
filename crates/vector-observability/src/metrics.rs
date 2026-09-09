use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SloMetric {
    pub name: String,
    pub duration_ms: u64,
    pub target_slo_ms: u64,
    pub met: bool,
}

pub struct SloTracker {
    name: String,
    target_slo_ms: u64,
    start_time: Instant,
}

impl SloTracker {
    pub fn start(name: impl Into<String>, target_slo_ms: u64) -> Self {
        Self {
            name: name.into(),
            target_slo_ms,
            start_time: Instant::now(),
        }
    }

    pub fn finish(self) -> SloMetric {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        SloMetric {
            name: self.name,
            duration_ms,
            target_slo_ms: self.target_slo_ms,
            met: duration_ms <= self.target_slo_ms,
        }
    }
}
