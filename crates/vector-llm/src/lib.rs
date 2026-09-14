//! Project VECTOR model transport layer (EP-004).
//!
//! Implements the provider-neutral transport contract from REQ-015 and the
//! per-provider policy gates from REQ-014 and REQ-016..REQ-019.

pub mod transport;

pub fn name() -> &'static str {
    "vector-llm"
}
