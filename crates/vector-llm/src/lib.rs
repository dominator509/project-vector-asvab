//! Project VECTOR model transport layer (EP-004).
//!
//! Implements the provider-neutral transport contract from REQ-015 and the
//! per-provider policy gates from REQ-014 and REQ-016..REQ-019.
//!
//! [`probe`] turns each native CLI's own status output into an adapter health
//! verdict, so an installed-but-signed-out lane is reported as unavailable
//! instead of being silently selected and failing when a learner asks for help.

pub mod probe;
pub mod transport;

pub fn name() -> &'static str {
    "vector-llm"
}
