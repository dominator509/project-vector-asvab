//! Project VECTOR observability and safety layer (EP-006, EP-008).
//!
//! Owns local telemetry, crash capture with mandatory double redaction, and the
//! release gate that decides whether a diagnostic bundle may leave the machine.

pub mod crash;
pub mod metrics;
pub mod redaction;

pub use crash::{
    approve_release, redact_capture, verify_no_secrets, Canary, CanaryRegistry, CrashCapture,
    ReleaseRefusal,
};
pub use metrics::SloTracker;
pub use redaction::RedactedLogger;

#[cfg(test)]
mod redaction_test;
#[cfg(test)]
mod slo_test;
