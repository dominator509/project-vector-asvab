//! Project VECTOR core domain (EP-002).
//!
//! Domain types and invariants that the rest of the workspace builds on.
//!
//! # Why `plan` is absent
//!
//! This crate previously shipped `plan::AdaptivePlan::generate`, which returned
//! a hardcoded `["drill_1", "drill_2"]` while ignoring the mastery state, the
//! goal and the available time it was handed. It was a stub wearing a
//! completion costume: it satisfied its test (which only asserted the vector was
//! non-empty) while implementing nothing.
//!
//! The real planner lives in `vector_study::selection::generate_plan`, which
//! derives priority from due reviews, mastery distance from the goal and
//! estimate uncertainty, returns an empty plan when nothing needs work, and is
//! covered by property tests over generated input. It cannot live here because
//! `vector-study` depends on `vector-domain`, so delegating would be circular.
//!
//! The stub was therefore deleted rather than implemented a second time. Two
//! implementations of one concept — one real, one fake — is worse than one,
//! because a reader cannot tell which they have found.

pub mod artifact_test;
pub mod config;
pub mod content;
pub mod errors;
pub mod exam;
pub mod ingestion;
pub mod mastery;
pub mod policy;
pub mod profile;
pub mod simulator;
pub use config::AppConfig;
pub mod tests;
