//! Project VECTOR content and evidence layer (EP-004).
//!
//! Owns source-grounded retrieval over the evidence vault (REQ-008) and the
//! content-provenance rules that keep generated material traceable.

pub mod knowledge;
pub mod notebook;
pub mod retrieval;

pub fn name() -> &'static str {
    "vector-content"
}
