//! Project VECTOR repair operations (EP-008).
//!
//! Owns the repair broker: isolated worktree, scoped agent context, patch
//! preview, and the approval gate that stands between a repair and a pull
//! request.

pub mod broker;

pub fn name() -> &'static str {
    "vector-repair"
}
