//! Project VECTOR MCP integration (EP-004).
//!
//! Implements the least-privilege capability model, per-peer scoping and
//! untrusted-content authority isolation required by REQ-025, REQ-026 and
//! REQ-027, and the transport that carries them: [`protocol`] is the JSON-RPC
//! wire format, [`server`] enforces the capability model on the request path,
//! and [`client`] connects VECTOR to an allowlisted external peer.

pub mod capability;
pub mod client;
pub mod protocol;
pub mod server;

pub fn name() -> &'static str {
    "vector-mcp"
}
