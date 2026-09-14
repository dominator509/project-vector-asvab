//! Project VECTOR content and evidence layer (EP-004, EP-006).
//!
//! Owns source-grounded retrieval over the evidence vault (REQ-008), the
//! content-provenance rules that keep generated material traceable, content QA
//! review/audit/rollback (REQ-048) and internal difficulty calibration with an
//! explicit no-equivalence-claim rule (REQ-049).

pub mod content_qa;
pub mod knowledge;
pub mod notebook;
pub mod retrieval;

pub fn name() -> &'static str {
    "vector-content"
}
