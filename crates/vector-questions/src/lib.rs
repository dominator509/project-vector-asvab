//! Project VECTOR question factory and content integrity layer (EP-007).
//!
//! Owns source-ingestion sandboxing (REQ-050), content provenance and the item
//! lifecycle (REQ-056), the original-item factory and its deterministic answer
//! proofs (REQ-022), and the hashing primitives those depend on.

pub mod dictionary;
pub mod factory;
pub mod facts;
pub mod ingestion;
pub mod neets;
pub mod passages;
pub mod proof;
pub mod provenance;
pub mod thesaurus;

pub fn name() -> &'static str {
    "vector-questions"
}
