//! The future-sync seam (REQ-059).
//!
//! REQ-059 requires a "disabled SyncPort without cloud dependency". ADR-011
//! records the decision: a SaaS offering would arrive through a SyncPort that
//! preserves the local product architecture, and that port is disabled.
//!
//! This module exists so the seam is *explicit and testable* rather than a
//! comment. The risk it addresses is that sync behaviour gets added
//! incrementally — a background upload here, a token refresh there — until the
//! application quietly depends on a cloud service. Modelling the port as a
//! refused capability means any such addition fails a test.

use serde::{Deserialize, Serialize};

/// Whether remote sync is available.
///
/// Always false in this version. Exposed as a function rather than a constant
/// so the check reads like a capability probe at every call site.
pub const fn sync_enabled() -> bool {
    false
}

/// Why a sync operation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncRefusal {
    /// The port is disabled by ADR-011.
    Disabled,
}

impl std::fmt::Display for SyncRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncRefusal::Disabled => write!(
                f,
                "the SyncPort is disabled: VECTOR has no cloud dependency and all \
                 core study functions work offline"
            ),
        }
    }
}

impl std::error::Error for SyncRefusal {}

/// A direction of synchronisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncDirection {
    Upload,
    Download,
    Bidirectional,
}

/// A sync request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub direction: SyncDirection,
    /// Remote endpoint the request would target.
    pub endpoint: String,
}

/// The sync port.
///
/// Every method refuses. There is deliberately no constructor that produces an
/// enabled port, so no future code path can enable sync without editing this
/// module and therefore failing the tests that assert it is disabled.
pub struct SyncPort;

impl SyncPort {
    /// Attempt to synchronise.
    pub fn sync(_request: &SyncRequest) -> Result<(), SyncRefusal> {
        Err(SyncRefusal::Disabled)
    }

    /// Whether the port could serve a request.
    pub fn is_available() -> bool {
        sync_enabled()
    }

    /// Remote endpoints this build would contact.
    ///
    /// Empty, and asserted to be empty: a non-empty list would mean the
    /// application has a network dependency despite the port being disabled.
    pub fn configured_endpoints() -> Vec<String> {
        Vec::new()
    }
}

/// Whether the application requires any cloud account to function.
///
/// Always false. ADR-003 records the local-first decision; this makes the claim
/// checkable rather than aspirational.
pub const fn requires_cloud_account() -> bool {
    false
}
