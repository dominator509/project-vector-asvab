//! Project VECTOR application service layer (EP-004).
//!
//! Owns cross-cutting application policy that spans the domain crates: the
//! offline-core guarantee (REQ-042) and the typed service boundary the desktop
//! UI invokes (SPEC-003).

pub mod offline;

/// Initialize the application core.
///
/// Retained for the desktop shell's startup path.
pub fn init_app() -> String {
    "VECTOR Application Core Initialized".to_string()
}
