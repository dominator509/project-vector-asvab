//! Project VECTOR platform layer (EP-006, EP-009).
//!
//! Owns the security-critical boundary between the Rust core and the operating
//! system (shell-free process invocation, environment isolation, approved binary
//! verification), and the release identity of the shipped artifact.

pub mod process;
pub mod release;

pub fn name() -> &'static str {
    "vector-platform"
}
