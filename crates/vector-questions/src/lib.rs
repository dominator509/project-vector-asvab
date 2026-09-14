//! Project VECTOR question factory and content integrity layer (EP-007).
//!
//! Owns source-ingestion sandboxing (REQ-050), content provenance and the item
//! lifecycle (REQ-056), and the hashing primitives those depend on.

pub mod ingestion;
pub mod provenance;

pub fn name() -> &'static str {
    "vector-questions"
}
