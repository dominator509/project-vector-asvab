//! Provider-neutral model transport contract (REQ-015) and the routing policy
//! that decides which adapter may serve a request.
//!
//! Binding rules come from `AI_TRANSPORTS.md`:
//! 1. Never open/copy/replay provider OAuth credential stores.
//! 2. Invoke only documented provider-owned binaries/protocols.
//! 3. Authentication happens in the provider's own UI/CLI.
//! 4. Scrub API-key environment variables from native/subscription lanes.
//! 5. A dated provider-policy record can disable an adapter when terms go stale.
//! 6. Local-only requests cannot route to network providers.
//! 7. Provider output is untrusted and schema/bounds checked.
//!
//! This module encodes rules 5 and 6 as enforced, testable behavior rather than
//! prose an implementer is trusted to remember.

use serde::{Deserialize, Serialize};

/// Privacy classification of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyClass {
    /// May only be served by a model running on this machine.
    LocalOnly,
    /// May be sent to an approved external provider.
    RemoteAllowed,
}

/// How the user pays for a transport. Surfaced so the app never silently bills
/// a user, and so a subscription lane is never confused with an API lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingMode {
    /// Runs locally; no provider billing.
    None,
    /// Covered by the user's existing provider subscription (native client).
    Subscription,
    /// Metered API usage the user pays per token.
    MeteredApi,
}

/// Declared capabilities of an adapter. AI_TRANSPORTS.md requires every adapter
/// to expose exactly these facts so the router can select on them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub structured_output: bool,
    pub streaming: bool,
    pub tool_use: bool,
    pub mcp: bool,
    pub web_search: bool,
}

/// Health of an adapter's underlying client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterHealth {
    /// Probed and working.
    Healthy,
    /// Present but not usable right now (e.g. not logged in).
    Unavailable { reason: String },
    /// Not installed / not configured on this machine.
    NotConfigured,
}

/// A dated provider-policy record. Rule 5: terms can go stale and disable an
/// adapter without a code change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRecord {
    /// Date the provider terms were last verified, RFC 3339 date.
    pub terms_verified_on: String,
    /// Whether the adapter is currently permitted by those terms.
    pub permitted: bool,
    /// Free-text note recorded with the verification.
    pub note: String,
}

impl PolicyRecord {
    /// Whether this record is still within its validity window.
    ///
    /// A stale verification is treated as *not permitted*: if we cannot show
    /// that the terms were checked recently, we do not assume they still hold.
    pub fn is_fresh(&self, now: chrono::DateTime<chrono::Utc>, max_age_days: i64) -> bool {
        match chrono::NaiveDate::parse_from_str(&self.terms_verified_on, "%Y-%m-%d") {
            Ok(date) => {
                let verified = date
                    .and_hms_opt(0, 0, 0)
                    .map(|dt| dt.and_utc())
                    .unwrap_or_else(|| now);
                let age = now.signed_duration_since(verified).num_days();
                // A record dated in the future is malformed, not fresh.
                age >= 0 && age <= max_age_days
            }
            Err(_) => false,
        }
    }
}

/// A transport adapter (REQ-015..REQ-019).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelTransport {
    pub id: String,
    /// Documented binary or protocol this adapter drives.
    pub transport: String,
    /// Who owns authentication. VECTOR never holds these credentials.
    pub auth_owner: String,
    pub billing: BillingMode,
    pub capabilities: Capabilities,
    /// Privacy class this adapter is acceptable for.
    pub privacy: PrivacyClass,
    pub policy: PolicyRecord,
    pub health: AdapterHealth,
    /// True when the adapter is prohibited outright (e.g. a banned OAuth bridge).
    pub disabled: bool,
}

impl ModelTransport {
    /// Whether this adapter may serve a request of the given privacy class.
    ///
    /// This is the single enforcement point for rules 5 and 6. Callers must not
    /// route around it.
    pub fn is_eligible(
        &self,
        privacy: PrivacyClass,
        now: chrono::DateTime<chrono::Utc>,
        max_policy_age_days: i64,
    ) -> Result<(), IneligibilityReason> {
        if self.disabled {
            return Err(IneligibilityReason::Disabled);
        }
        if !self.policy.permitted {
            return Err(IneligibilityReason::PolicyProhibits);
        }
        if !self.policy.is_fresh(now, max_policy_age_days) {
            return Err(IneligibilityReason::PolicyStale);
        }
        if !matches!(self.health, AdapterHealth::Healthy) {
            return Err(IneligibilityReason::Unhealthy);
        }
        // Rule 6: a local-only request must never leave the machine.
        if privacy == PrivacyClass::LocalOnly && self.privacy != PrivacyClass::LocalOnly {
            return Err(IneligibilityReason::PrivacyMismatch);
        }
        Ok(())
    }
}

/// Why an adapter was refused. Typed so the UI can explain the refusal instead
/// of failing silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IneligibilityReason {
    Disabled,
    PolicyProhibits,
    PolicyStale,
    Unhealthy,
    PrivacyMismatch,
}

impl std::fmt::Display for IneligibilityReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            IneligibilityReason::Disabled => "adapter is disabled",
            IneligibilityReason::PolicyProhibits => "provider terms prohibit this adapter",
            IneligibilityReason::PolicyStale => "provider terms verification is stale",
            IneligibilityReason::Unhealthy => "adapter is not healthy",
            IneligibilityReason::PrivacyMismatch => {
                "a local-only request cannot use a network provider"
            }
        };
        write!(f, "{text}")
    }
}

/// The built-in transport registry, reflecting the AI_TRANSPORTS.md matrix.
///
/// The `gemini_cli_oauth` entry is present but disabled: ADR-006 and REQ-019
/// disable that bridge because current Google terms prohibit it. Modelling it
/// as a disabled adapter keeps the decision explicit and testable rather than
/// relying on it simply being absent.
pub fn default_registry(now: chrono::DateTime<chrono::Utc>) -> Vec<ModelTransport> {
    let today = now.format("%Y-%m-%d").to_string();
    let policy = |permitted: bool, note: &str| PolicyRecord {
        terms_verified_on: today.clone(),
        permitted,
        note: note.to_string(),
    };

    vec![
        ModelTransport {
            id: "local_llama".into(),
            transport: "llama.cpp server (documented HTTP interface)".into(),
            auth_owner: "none (local process)".into(),
            billing: BillingMode::None,
            capabilities: Capabilities {
                structured_output: true,
                streaming: true,
                tool_use: false,
                mcp: false,
                web_search: false,
            },
            privacy: PrivacyClass::LocalOnly,
            policy: policy(true, "no external provider terms involved"),
            health: AdapterHealth::NotConfigured,
            disabled: false,
        },
        ModelTransport {
            id: "grok_native".into(),
            transport: "official Grok CLI headless/ACP".into(),
            auth_owner: "xAI native client (user login)".into(),
            billing: BillingMode::Subscription,
            capabilities: Capabilities {
                structured_output: true,
                streaming: true,
                tool_use: true,
                mcp: true,
                web_search: true,
            },
            privacy: PrivacyClass::RemoteAllowed,
            policy: policy(true, "native client lane; credentials never read by VECTOR"),
            health: AdapterHealth::NotConfigured,
            disabled: false,
        },
        ModelTransport {
            id: "codex_native".into(),
            transport: "official `codex exec`".into(),
            auth_owner: "OpenAI native client (ChatGPT login)".into(),
            billing: BillingMode::Subscription,
            capabilities: Capabilities {
                structured_output: true,
                streaming: true,
                tool_use: true,
                mcp: true,
                web_search: true,
            },
            privacy: PrivacyClass::RemoteAllowed,
            policy: policy(true, "native client lane; credentials never read by VECTOR"),
            health: AdapterHealth::NotConfigured,
            disabled: false,
        },
        ModelTransport {
            id: "claude_native".into(),
            transport: "official Claude Code noninteractive mode".into(),
            auth_owner: "Anthropic native client (user login)".into(),
            billing: BillingMode::Subscription,
            capabilities: Capabilities {
                structured_output: true,
                streaming: true,
                tool_use: true,
                mcp: true,
                web_search: false,
            },
            privacy: PrivacyClass::RemoteAllowed,
            // Conditional: only while the terms registry permits it.
            policy: policy(true, "conditional; disabled by policy when terms change"),
            health: AdapterHealth::NotConfigured,
            disabled: false,
        },
        ModelTransport {
            id: "gemini_cli_oauth".into(),
            transport: "third-party OAuth bridge".into(),
            auth_owner: "n/a".into(),
            billing: BillingMode::Subscription,
            capabilities: Capabilities {
                structured_output: false,
                streaming: false,
                tool_use: false,
                mcp: false,
                web_search: false,
            },
            privacy: PrivacyClass::RemoteAllowed,
            policy: policy(
                false,
                "ADR-006 / REQ-019: third-party OAuth bridge prohibited by provider terms",
            ),
            health: AdapterHealth::NotConfigured,
            disabled: true,
        },
    ]
}

/// Select the best eligible adapter for a request.
///
/// `required` narrows the candidate set to adapters that actually have the
/// capability the caller needs, so a request requiring tools never lands on a
/// transport that cannot call them.
pub fn select_transport<'a>(
    registry: &'a [ModelTransport],
    privacy: PrivacyClass,
    required: &Capabilities,
    now: chrono::DateTime<chrono::Utc>,
    max_policy_age_days: i64,
) -> Result<&'a ModelTransport, NoEligibleTransport> {
    let mut candidates: Vec<&ModelTransport> = registry
        .iter()
        .filter(|t| t.is_eligible(privacy, now, max_policy_age_days).is_ok())
        .filter(|t| {
            (!required.structured_output || t.capabilities.structured_output)
                && (!required.streaming || t.capabilities.streaming)
                && (!required.tool_use || t.capabilities.tool_use)
                && (!required.mcp || t.capabilities.mcp)
                && (!required.web_search || t.capabilities.web_search)
        })
        .collect();

    if candidates.is_empty() {
        return Err(NoEligibleTransport {
            privacy,
            // Report why each adapter was refused so the failure is diagnosable.
            refusals: registry
                .iter()
                .map(|t| {
                    (
                        t.id.clone(),
                        t.is_eligible(privacy, now, max_policy_age_days)
                            .err()
                            .unwrap_or(IneligibilityReason::Unhealthy),
                    )
                })
                .collect(),
        });
    }

    // Local-first preference (ADR-003): prefer the local adapter when several
    // are eligible, so a network lane is not used merely because it is listed.
    candidates.sort_by_key(|t| match t.privacy {
        PrivacyClass::LocalOnly => 0,
        PrivacyClass::RemoteAllowed => 1,
    });

    Ok(candidates[0])
}

/// No adapter could serve the request.
#[derive(Debug, Clone, PartialEq)]
pub struct NoEligibleTransport {
    pub privacy: PrivacyClass,
    pub refusals: Vec<(String, IneligibilityReason)>,
}

impl std::fmt::Display for NoEligibleTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no eligible transport for {:?}: ", self.privacy)?;
        for (id, reason) in &self.refusals {
            write!(f, "{id}={reason}; ")?;
        }
        Ok(())
    }
}

impl std::error::Error for NoEligibleTransport {}

/// Environment variables that must be scrubbed before invoking a native client
/// lane (rule 4). Leaving an API key set would silently switch a subscription
/// lane to metered billing.
pub const API_KEY_ENV_VARS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "XAI_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "GROK_API_KEY",
];

/// A sanitized environment for launching a native/subscription client.
///
/// Rule 4: API-key variables are removed so the provider's own client uses its
/// logged-in subscription rather than a metered key the user did not choose.
pub fn scrub_api_keys(
    env: &std::collections::BTreeMap<String, String>,
    billing: BillingMode,
) -> std::collections::BTreeMap<String, String> {
    let mut out = env.clone();
    // Only subscription lanes are scrubbed; an explicit API lane keeps its key.
    if billing == BillingMode::Subscription {
        for key in API_KEY_ENV_VARS {
            out.remove(*key);
        }
    }
    out
}

/// Validate provider output before it is trusted (rule 7).
///
/// Provider output is untrusted input: it must be bounded and shaped, or a
/// misbehaving provider can inject unbounded text into the application.
pub fn validate_provider_text(text: &str, max_bytes: usize) -> Result<(), String> {
    if text.len() > max_bytes {
        return Err(format!(
            "provider output {} bytes exceeds the {max_bytes} byte bound",
            text.len()
        ));
    }
    if text.contains('\0') {
        return Err("provider output contains a NUL byte".to_string());
    }
    Ok(())
}
