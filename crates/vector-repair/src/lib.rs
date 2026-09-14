//! Project VECTOR repair operations (EP-008).
//!
//! Owns the repair broker: isolated worktree, scoped agent context, patch
//! preview, and the approval gate that stands between a repair and a pull
//! request. [`worktree`] is the part that makes the isolation real: it creates
//! an actual git worktree, verifies that it is separate from the main checkout,
//! and removes it again.

pub mod broker;
pub mod worktree;

pub fn name() -> &'static str {
    "vector-repair"
}
