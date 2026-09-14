//! Project VECTOR application service layer (EP-004, EP-006, EP-007, EP-008).
//!
//! Owns cross-cutting application policy that spans the domain crates: the
//! offline-core guarantee (REQ-042), the egress privacy classifier and no-ads
//! guarantee (REQ-046), crash-loop safe mode (REQ-051), the telemetry and
//! prohibited-screening posture (REQ-052), external score typing (REQ-055), the
//! disabled future-sync seam (REQ-059), the typed service boundary the desktop
//! UI invokes (SPEC-003), and the background workers that keep derived state in
//! step with stored evidence (REQ-054).

pub mod egress;
pub mod offline;
pub mod privacy;
pub mod safe_mode;
pub mod service;
pub mod sync_port;
pub mod workers;

/// Initialize the application core.
///
/// Retained for the desktop shell's startup path.
pub fn init_app() -> String {
    "VECTOR Application Core Initialized".to_string()
}
