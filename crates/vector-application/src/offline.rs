//! Offline-core guarantee (REQ-042).
//!
//! REQ-042 requires that "all core study functions" work with the network
//! disabled. The risk this module addresses is a function that *appears* to
//! work offline but silently depends on a network call, so the guarantee is
//! modelled as an explicit capability classification plus a runtime guard that
//! refuses network-dependent work when the app is in offline mode.

use serde::{Deserialize, Serialize};

/// Whether a capability can run with no network access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfflineSupport {
    /// Works fully with no network.
    Available,
    /// Works, but with reduced functionality, and must say so.
    Degraded { note: String },
    /// Requires the network and must be refused offline.
    Unavailable,
}

/// A named application capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub offline: OfflineSupport,
    /// True when this is a core study function (REQ-042 covers these).
    pub core: bool,
}

/// Whether the app currently believes it has network access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkState {
    Online,
    Offline,
}

/// Why a capability could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfflineError {
    /// A core study function was unavailable offline, which violates REQ-042.
    CoreUnavailableOffline(String),
    /// A non-core capability needs the network.
    RequiresNetwork(String),
}

impl std::fmt::Display for OfflineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfflineError::CoreUnavailableOffline(id) => {
                write!(f, "core study function {id} is unavailable offline")
            }
            OfflineError::RequiresNetwork(id) => {
                write!(f, "{id} requires network access")
            }
        }
    }
}

impl std::error::Error for OfflineError {}

/// The built-in capability classification.
///
/// Every core study function is `Available` offline by construction. Anything
/// that cannot be is either not core, or it is a bug this table makes visible.
pub fn builtin_capabilities() -> Vec<Capability> {
    let cap = |id: &str, label: &str, offline: OfflineSupport, core: bool| Capability {
        id: id.to_string(),
        label: label.to_string(),
        offline,
        core,
    };

    vec![
        // Core study functions: all must work offline (REQ-042).
        cap(
            "diagnostic",
            "Diagnostic assessment",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "practice",
            "Practice questions",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "review_schedule",
            "Spaced-repetition review",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "study_plan",
            "Adaptive study plan",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "exam_simulation",
            "CAT/paper exam simulation",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "mastery_tracking",
            "Mastery and progress tracking",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "readiness_band",
            "Readiness estimate",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "evidence_lookup",
            "Evidence vault lookup",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "notebook_export",
            "Notebook export (manual)",
            OfflineSupport::Available,
            true,
        ),
        cap(
            "local_ai",
            "Local model tutor",
            OfflineSupport::Available,
            true,
        ),
        // Non-core: may legitimately need the network.
        cap(
            "native_ai",
            "Native provider lanes",
            OfflineSupport::Unavailable,
            false,
        ),
        cap(
            "source_refresh",
            "Source freshness refresh",
            OfflineSupport::Degraded {
                note: "serves cached sources; freshness dates may be stale".to_string(),
            },
            false,
        ),
        cap(
            "update_check",
            "Application update check",
            OfflineSupport::Unavailable,
            false,
        ),
    ]
}

/// Guard evaluation of a capability against the current network state.
///
/// Returns the effective support level, or an error when the capability cannot
/// be used. A core function that is unavailable offline is reported as a
/// REQ-042 violation rather than being silently allowed to fail later.
pub fn require_capability(id: &str, state: NetworkState) -> Result<OfflineSupport, OfflineError> {
    let capabilities = builtin_capabilities();
    let capability = capabilities
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| OfflineError::RequiresNetwork(id.to_string()))?;

    classify_offline(id, capability.offline.clone(), capability.core, state)
}

/// The classification rule itself, separated from the registry lookup.
///
/// Exposed so the core-violation branch can be exercised directly: a guard
/// whose error path is unreachable in tests is a guard nobody has proven works.
pub fn classify_offline(
    id: &str,
    offline: OfflineSupport,
    core: bool,
    state: NetworkState,
) -> Result<OfflineSupport, OfflineError> {
    match (state, &offline) {
        (NetworkState::Online, support) => Ok(support.clone()),
        (NetworkState::Offline, OfflineSupport::Available) => Ok(OfflineSupport::Available),
        (NetworkState::Offline, OfflineSupport::Degraded { note }) => {
            Ok(OfflineSupport::Degraded { note: note.clone() })
        }
        (NetworkState::Offline, OfflineSupport::Unavailable) => {
            if core {
                Err(OfflineError::CoreUnavailableOffline(id.to_string()))
            } else {
                Err(OfflineError::RequiresNetwork(id.to_string()))
            }
        }
    }
}

/// Whether every declared core function is available offline.
///
/// This is the direct REQ-042 assertion, expressed as runnable code so it can
/// be checked in CI rather than asserted in prose.
pub fn core_functions_all_offline(capabilities: &[Capability]) -> Result<(), Vec<String>> {
    let failing: Vec<String> = capabilities
        .iter()
        .filter(|c| c.core)
        .filter(|c| !matches!(c.offline, OfflineSupport::Available))
        .map(|c| c.id.clone())
        .collect();

    if failing.is_empty() {
        Ok(())
    } else {
        Err(failing)
    }
}
