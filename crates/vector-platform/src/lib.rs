//! Project VECTOR platform layer (EP-006).
//!
//! Owns the security-critical boundary between the Rust core and the operating
//! system: shell-free process invocation, environment isolation, and approved
//! binary verification.

pub mod process;

pub fn name() -> &'static str {
    "vector-platform"
}
