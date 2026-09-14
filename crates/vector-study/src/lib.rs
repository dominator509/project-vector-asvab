//! Project VECTOR study engine (EP-004).
//!
//! Owns the learning mechanics that operate on persisted learner evidence:
//! spaced-repetition scheduling (REQ-043) and the adaptive plan/selection
//! logic built on top of mastery state (REQ-003 surface used by EP-004).

pub mod fsrs;
pub mod selection;

pub fn name() -> &'static str {
    "vector-study"
}
