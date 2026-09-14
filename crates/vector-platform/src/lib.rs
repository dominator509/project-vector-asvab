//! Project VECTOR platform layer (EP-006, EP-009, EP-010).
//!
//! Owns the security-critical boundary between the Rust core and the operating
//! system (shell-free process invocation, environment isolation, approved binary
//! verification), the release identity of the shipped artifact, and the
//! V-000..V-021 release accounting that decides the ship verdict.

pub mod accounting;
pub mod process;
pub mod release;

pub fn name() -> &'static str {
    "vector-platform"
}
