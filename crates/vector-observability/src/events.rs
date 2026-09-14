//! Local structured events and component health (REQ-057, REQ-028).
//!
//! Binding source: `OBSERVABILITY.md`.
//!
//! Two rules drive this module:
//! - Structured events carry time/sequence, severity, component and event id, a
//!   correlation id, build and content versions, bounded attributes and a
//!   redaction class. **Study free text is excluded by default.**
//! - Health distinguishes healthy, disabled, unconfigured, unavailable and
//!   stale-policy states. **Process existence alone is not a health proof.**

use serde::{Deserialize, Serialize};

/// Event severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

/// How an attribute must be treated when persisted or exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactionClass {
    /// Safe to persist and to include in an approved export.
    Public,
    /// Local only; never exported.
    LocalOnly,
    /// Contains personal data; must be redacted before any export.
    Personal,
    /// A secret; never persisted in cleartext.
    Secret,
}

impl RedactionClass {
    /// Whether an attribute of this class may appear in an exported bundle.
    pub fn exportable(self) -> bool {
        matches!(self, RedactionClass::Public)
    }
}

/// Maximum length of an attribute value, in bytes.
pub const MAX_ATTRIBUTE_BYTES: usize = 512;

/// A single structured event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub timestamp: String,
    pub severity: Severity,
    /// Component that emitted the event, e.g. "persistence".
    pub component: String,
    /// Stable event identifier, e.g. "migration.applied".
    pub event_id: String,
    /// Correlates related events across components.
    pub correlation_id: String,
    pub build_version: String,
    pub content_version: String,
    pub attributes: Vec<(String, String)>,
    pub redaction_class: RedactionClass,
}

/// Why an event was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    /// A required identifier is blank.
    MissingIdentifier(&'static str),
    /// An attribute value exceeded the bound.
    AttributeTooLarge { key: String, bytes: usize },
    /// The event would record study free text, which is excluded by default.
    StudyFreeText,
    /// An attribute is classified too sensitively for the event's class.
    ClassMismatch { key: String },
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::MissingIdentifier(what) => {
                write!(f, "event is missing its {what}")
            }
            EventError::AttributeTooLarge { key, bytes } => write!(
                f,
                "attribute {key:?} is {bytes} bytes, over the {MAX_ATTRIBUTE_BYTES} bound"
            ),
            EventError::StudyFreeText => {
                write!(f, "study free text is excluded from events by default")
            }
            EventError::ClassMismatch { key } => write!(
                f,
                "attribute {key:?} may not appear in an event with this redaction class"
            ),
        }
    }
}

impl std::error::Error for EventError {}

/// Attribute keys that would carry learner free text.
///
/// `OBSERVABILITY.md`: "Study free text is excluded by default." A learner's
/// answers, notes and prompts are the most sensitive content in the app and are
/// not needed for diagnostics.
const FREE_TEXT_KEYS: &[&str] = &[
    "answer",
    "answer_text",
    "response",
    "prompt",
    "question_text",
    "stem",
    "note",
    "notes",
    "explanation",
    "passage",
    "tutor_message",
    "search_query",
];

/// Whether an attribute key would carry study free text.
pub fn is_free_text_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    FREE_TEXT_KEYS
        .iter()
        .any(|k| lower == *k || lower.ends_with(&format!("_{k}")))
}

/// The event ring buffer.
#[derive(Debug, Clone)]
pub struct EventRing {
    events: Vec<Event>,
    capacity: usize,
    next_sequence: u64,
}

impl EventRing {
    /// Create a ring with a fixed capacity.
    ///
    /// Capacity must be non-zero: a zero-length ring would silently discard
    /// every event, which looks like "no problems" rather than "no logging".
    pub fn new(capacity: usize) -> Result<Self, EventError> {
        if capacity == 0 {
            return Err(EventError::MissingIdentifier("capacity"));
        }
        Ok(Self {
            events: Vec::new(),
            capacity,
            next_sequence: 1,
        })
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Record an event, assigning the next sequence number.
    pub fn record(&mut self, mut event: Event) -> Result<u64, EventError> {
        for (field, value) in [
            ("component", &event.component),
            ("event_id", &event.event_id),
            ("correlation_id", &event.correlation_id),
        ] {
            if value.trim().is_empty() {
                return Err(EventError::MissingIdentifier(field));
            }
        }

        for (key, value) in &event.attributes {
            if value.len() > MAX_ATTRIBUTE_BYTES {
                return Err(EventError::AttributeTooLarge {
                    key: key.clone(),
                    bytes: value.len(),
                });
            }
            if is_free_text_key(key) {
                return Err(EventError::StudyFreeText);
            }
            // A secret attribute may not travel on an event classified as
            // Public, because that class is what the export gate reads.
            if event.redaction_class == RedactionClass::Public && is_secret_key(key) {
                return Err(EventError::ClassMismatch { key: key.clone() });
            }
        }

        let sequence = self.next_sequence;
        self.next_sequence += 1;
        event.sequence = sequence;

        if self.events.len() == self.capacity {
            // Oldest first: a ring keeps the most recent evidence.
            self.events.remove(0);
        }
        self.events.push(event);
        Ok(sequence)
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Events approved for export.
    ///
    /// Only Public-class events are offered, so a Personal or Secret event
    /// cannot be exported by a caller that forgets to filter.
    pub fn exportable(&self) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| e.redaction_class.exportable())
            .collect()
    }

    /// Render exportable events as newline-delimited JSON.
    pub fn render_ndjson(&self) -> String {
        self.exportable()
            .iter()
            .filter_map(|e| serde_json::to_string(e).ok())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Whether an attribute key names a secret.
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    ["secret", "password", "token", "api_key", "credential"]
        .iter()
        .any(|m| lower.contains(m))
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Component health, distinguishing the states OBSERVABILITY.md requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthState {
    Healthy,
    /// Deliberately turned off by the learner or by policy.
    Disabled {
        reason: String,
    },
    /// Present but not set up.
    Unconfigured,
    /// Configured but not reachable or not working.
    Unavailable {
        reason: String,
    },
    /// Reachable, but its data or policy is out of date.
    StalePolicy {
        verified_on: String,
        max_age_days: i64,
    },
}

impl HealthState {
    /// Whether this state should be presented to the learner as a problem.
    ///
    /// `Disabled` is not a problem: the learner chose it. Treating an
    /// intentional choice as a fault trains people to ignore warnings.
    pub fn is_actionable_problem(&self) -> bool {
        matches!(
            self,
            HealthState::Unavailable { .. } | HealthState::StalePolicy { .. }
        )
    }

    /// Whether this state counts as working.
    pub fn is_working(&self) -> bool {
        matches!(self, HealthState::Healthy)
    }
}

/// A component's reported health.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub component: String,
    pub state: HealthState,
    /// How the state was determined, so it can be challenged.
    pub basis: HealthBasis,
}

/// How a health verdict was reached.
///
/// OBSERVABILITY.md is explicit that "process existence alone is not a health
/// proof", so a liveness-only basis is representable but explicitly insufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthBasis {
    /// The process or socket exists. NOT sufficient on its own.
    ProcessAlive,
    /// A real operation succeeded and its effect was read back.
    OperationSucceeded,
    /// A documented capability probe ran and reported.
    CapabilityProbe,
    /// Configuration was inspected without exercising anything.
    ConfigurationOnly,
}

impl HealthBasis {
    /// Whether this basis can support a Healthy verdict.
    pub fn can_support_healthy(self) -> bool {
        matches!(
            self,
            HealthBasis::OperationSucceeded | HealthBasis::CapabilityProbe
        )
    }
}

/// Why a health report is not credible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthError {
    /// A Healthy verdict was claimed on an insufficient basis.
    InsufficientBasis {
        component: String,
        basis: HealthBasis,
    },
}

impl std::fmt::Display for HealthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthError::InsufficientBasis { component, basis } => write!(
                f,
                "{component} claims Healthy on a {basis:?} basis; process existence \
                 alone is not a health proof"
            ),
        }
    }
}

impl std::error::Error for HealthError {}

impl ComponentHealth {
    /// Validate the report.
    ///
    /// A component may be reported Healthy only when the basis actually
    /// demonstrates function. This is the enforcement point for the "process
    /// existence is not proof" rule.
    pub fn validate(&self) -> Result<(), HealthError> {
        if self.state.is_working() && !self.basis.can_support_healthy() {
            return Err(HealthError::InsufficientBasis {
                component: self.component.clone(),
                basis: self.basis,
            });
        }
        Ok(())
    }
}

/// The overall health summary across components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    pub components: Vec<ComponentHealth>,
}

impl HealthReport {
    /// Validate every component report.
    pub fn validate(&self) -> Result<(), HealthError> {
        for component in &self.components {
            component.validate()?;
        }
        Ok(())
    }

    /// Components that need learner attention.
    pub fn actionable(&self) -> Vec<&ComponentHealth> {
        self.components
            .iter()
            .filter(|c| c.state.is_actionable_problem())
            .collect()
    }

    /// Whether every component is working.
    pub fn all_healthy(&self) -> bool {
        !self.components.is_empty() && self.components.iter().all(|c| c.state.is_working())
    }
}
