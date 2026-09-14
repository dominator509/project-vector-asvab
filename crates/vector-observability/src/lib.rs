//! Project VECTOR observability and safety layer (EP-006, EP-008).
//!
//! Owns local structured telemetry, component health, crash capture with
//! mandatory double redaction, crash-bundle assembly, and the release gate that
//! decides whether a diagnostic artifact may leave the machine.

pub mod bundle;
pub mod crash;
pub mod events;
pub mod metrics;
pub mod redaction;

pub use bundle::{
    assemble_bundle, BuildIdentity, BundleEntry, BundleError, ConsentManifest, CrashBundle,
    EnvironmentFacts, ReproRecipe, REQUIRED_COMPONENTS,
};
pub use crash::{
    approve_release, redact_capture, verify_no_secrets, Canary, CanaryRegistry, CrashCapture,
    ReleaseRefusal,
};
pub use events::{
    ComponentHealth, Event, EventRing, HealthBasis, HealthReport, HealthState, RedactionClass,
    Severity,
};
pub use metrics::SloTracker;
pub use redaction::RedactedLogger;

#[cfg(test)]
mod redaction_test;
#[cfg(test)]
mod slo_test;
