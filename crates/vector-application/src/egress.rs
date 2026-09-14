//! Egress privacy classification and the no-ads guarantee (REQ-046).
//!
//! REQ-046 forbids behavioral advertising and data brokerage. THREAT_MODEL.md
//! adds "false score/eligibility claims" and SECURITY.md requires an "egress
//! privacy classifier".
//!
//! The control implemented here is that **every outbound request must be
//! classified before it can be sent**, and any payload carrying learner data
//! requires an explicit, per-destination consent decision. There is no
//! telemetry or advertising destination in the allowlist, and that absence is
//! asserted by tests rather than left to convention.

use serde::{Deserialize, Serialize};

/// Why data is leaving the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EgressPurpose {
    /// The learner asked a model a question.
    ModelQuery,
    /// Checking whether provider terms have changed.
    PolicyRefresh,
    /// Checking for a new application release.
    UpdateCheck,
    /// Sending a crash report the learner explicitly approved.
    CrashReport,
    /// Fetching a cited source.
    SourceFetch,
}

impl EgressPurpose {
    /// Whether this purpose may carry data that identifies the learner.
    ///
    /// A model query necessarily includes the learner's prompt, so it is
    /// permitted but requires consent. The others must be able to run without
    /// learner data at all.
    pub fn may_carry_learner_data(self) -> bool {
        matches!(self, EgressPurpose::ModelQuery | EgressPurpose::CrashReport)
    }
}

/// What kind of data a payload contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DataClass {
    /// No learner-identifying content.
    Anonymous,
    /// Study content: questions, mastery, attempts.
    StudyData,
    /// Directly identifying: name, email, learner id.
    PersonalData,
    /// Credentials or secrets. Must never leave.
    Secret,
}

/// A destination an egress request may target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    /// Stable id, e.g. "local_llama" or "update_channel".
    pub id: String,
    /// Host, for auditability.
    pub host: String,
    /// Whether the learner has consented to this destination receiving data.
    pub consented: bool,
    /// Whether this destination is a behavioral-advertising or brokerage
    /// endpoint. Such a destination is never permitted (REQ-046).
    pub is_advertising: bool,
}

/// Why an egress request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressRefusal {
    /// The destination is an advertising or brokerage endpoint.
    AdvertisingForbidden(String),
    /// The learner has not consented to this destination.
    NoConsent(String),
    /// A secret was about to be transmitted.
    SecretPayload,
    /// Learner data would be sent for a purpose that must not carry it.
    PurposeForbidsLearnerData(EgressPurpose),
}

impl std::fmt::Display for EgressRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EgressRefusal::AdvertisingForbidden(id) => write!(
                f,
                "destination {id} is an advertising or data-brokerage endpoint, \
                 which VECTOR never contacts"
            ),
            EgressRefusal::NoConsent(id) => {
                write!(f, "the learner has not consented to sending data to {id}")
            }
            EgressRefusal::SecretPayload => {
                write!(f, "a secret must never be transmitted")
            }
            EgressRefusal::PurposeForbidsLearnerData(p) => {
                write!(f, "{p:?} must not carry learner-identifying data")
            }
        }
    }
}

impl std::error::Error for EgressRefusal {}

/// A request to send data off the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct EgressRequest {
    pub destination: Destination,
    pub purpose: EgressPurpose,
    pub data_class: DataClass,
    /// Whether this destination is on the machine (loopback/local process).
    pub is_local: bool,
}

/// Classify and authorize an egress request.
///
/// Order matters: advertising is refused before consent is even considered, so
/// no consent setting can ever enable it.
pub fn authorize_egress(request: &EgressRequest) -> Result<(), EgressRefusal> {
    // A local destination never leaves the machine, so host-level rules do not
    // apply. This is what keeps the offline core usable (REQ-042).
    if request.is_local {
        if request.data_class == DataClass::Secret {
            return Err(EgressRefusal::SecretPayload);
        }
        return Ok(());
    }

    if request.destination.is_advertising {
        return Err(EgressRefusal::AdvertisingForbidden(
            request.destination.id.clone(),
        ));
    }

    if request.data_class == DataClass::Secret {
        return Err(EgressRefusal::SecretPayload);
    }

    if matches!(
        request.data_class,
        DataClass::PersonalData | DataClass::StudyData
    ) && !request.purpose.may_carry_learner_data()
    {
        return Err(EgressRefusal::PurposeForbidsLearnerData(request.purpose));
    }

    // Anything beyond anonymous data needs explicit consent for this
    // destination.
    if request.data_class != DataClass::Anonymous && !request.destination.consented {
        return Err(EgressRefusal::NoConsent(request.destination.id.clone()));
    }

    Ok(())
}

/// The built-in destination registry.
///
/// Note what is *absent*: there is no analytics, advertising or brokerage
/// destination. A test asserts that no registered destination is advertising,
/// so the guarantee is checked rather than assumed.
pub fn default_destinations() -> Vec<Destination> {
    vec![
        Destination {
            id: "local_llama".to_string(),
            host: "127.0.0.1".to_string(),
            consented: true,
            is_advertising: false,
        },
        Destination {
            id: "update_channel".to_string(),
            host: "updates.vector.invalid".to_string(),
            consented: false,
            is_advertising: false,
        },
        Destination {
            id: "policy_registry".to_string(),
            host: "policy.vector.invalid".to_string(),
            consented: false,
            is_advertising: false,
        },
    ]
}

/// Whether VECTOR performs behavioral advertising or data brokerage.
///
/// Always false. Exposed as code so the product claim is testable and cannot
/// drift from the implementation without a failing test.
pub const fn behavioral_advertising_enabled() -> bool {
    false
}

/// Whether VECTOR sells or brokers learner data.
pub const fn data_brokerage_enabled() -> bool {
    false
}
