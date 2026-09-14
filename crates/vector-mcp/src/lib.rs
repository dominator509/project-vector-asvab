//! Project VECTOR MCP integration (EP-004).
//!
//! Implements the least-privilege capability model, per-peer scoping and
//! untrusted-content authority isolation required by REQ-025, REQ-026 and
//! REQ-027.

pub mod capability;

pub fn name() -> &'static str {
    "vector-mcp"
}
