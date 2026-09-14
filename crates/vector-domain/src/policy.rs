//! Versioned, sourced policy data for job/composite targets and the retest
//! planner (REQ-012, REQ-013).
//!
//! Binding rules from `CONTENT_GOVERNANCE.md`:
//! - Every policy claim carries a canonical source, retrieval/effective dates
//!   and a trust tier.
//! - Trust tiers: A official/government, B commercially compatible standards,
//!   C reputable explanatory, D community/anecdotal — **never** authoritative
//!   for policy or correctness.
//! - Freshness defaults: provider terms 7 days, current job/cutoff claims 30
//!   days, ASVAB policy pages 90 days, stable academic sources annual.
//! - "Expired claims stay viewable but cannot silently power a claim presented
//!   as current."
//!
//! That last rule is the security property this module enforces: a stale policy
//! record is returned as *viewable but expired*, and any attempt to present it
//! as current is refused rather than silently allowed.

use serde::{Deserialize, Serialize};

/// Trust tier of a source (CONTENT_GOVERNANCE.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustTier {
    /// Official ASVAB/DoD/service/government. Authoritative for policy.
    A,
    /// Commercially compatible standards/textbooks.
    B,
    /// Reputable explanatory sources.
    C,
    /// Community/anecdotal. Never authoritative for policy or correctness.
    D,
}

impl TrustTier {
    /// Whether this tier may be used to justify a policy or correctness claim.
    ///
    /// Tier D exists to be visible, not to be cited.
    pub fn may_justify_policy(self) -> bool {
        !matches!(self, TrustTier::D)
    }

    /// The freshness window in days for claims of this tier.
    pub fn freshness_days(self) -> i64 {
        match self {
            TrustTier::A => 90,
            TrustTier::B => 90,
            TrustTier::C => 180,
            TrustTier::D => 30,
        }
    }
}

/// What kind of claim a policy record makes. Determines the default freshness
/// window independently of trust tier, because a provider-terms claim goes
/// stale far faster than a published policy page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimKind {
    /// Provider terms / automation policy: 7 days.
    ProviderTerms,
    /// Current job or cutoff claim: 30 days.
    JobOrCutoff,
    /// ASVAB policy page: 90 days.
    AsvabPolicy,
    /// Stable academic source: annual.
    StableAcademic,
}

impl ClaimKind {
    pub fn freshness_days(self) -> i64 {
        match self {
            ClaimKind::ProviderTerms => 7,
            ClaimKind::JobOrCutoff => 30,
            ClaimKind::AsvabPolicy => 90,
            ClaimKind::StableAcademic => 365,
        }
    }
}

/// A source citation attached to a policy record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicySource {
    pub source_id: String,
    pub canonical_url: String,
    pub publisher: String,
    pub trust_tier: TrustTier,
    /// Retrieval date, `YYYY-MM-DD`.
    pub retrieved_on: String,
    /// Effective/publication date if known, `YYYY-MM-DD`.
    pub effective_on: Option<String>,
    pub license_note: String,
}

/// A versioned policy record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRecord {
    pub id: String,
    /// The policy family, e.g. "afqt-composite" or "retest-wait".
    pub family: String,
    /// Monotonic version within the family.
    pub version: u32,
    pub claim_kind: ClaimKind,
    pub source: PolicySource,
    /// The policy payload (formula id, targets, notes).
    pub payload: PolicyPayload,
}

/// The content of a policy record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyPayload {
    /// A composite/job target requirement.
    JobTarget {
        job_name: String,
        /// Required composite score.
        required: u32,
        /// Named composite, e.g. "AFQT" or "GT".
        composite: String,
    },
    /// A composite formula definition.
    CompositeFormula {
        composite: String,
        /// The subtests that contribute.
        subtests: Vec<String>,
        /// Human-readable expression, recorded verbatim from the source.
        expression: String,
    },
    /// A retest policy: how long a learner must wait before retesting.
    RetestPolicy {
        wait_days: u32,
        /// Whether a waiver is described by the source.
        waiver_available: bool,
    },
}

/// Whether a policy record is current, and why not if it is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    Current,
    /// Viewable, but may not power a claim presented as current.
    Expired {
        age_days: i64,
        window_days: i64,
    },
    /// The dates could not be parsed, so freshness cannot be established.
    /// Treated as not current: an unverifiable record fails closed.
    Unverifiable,
}

impl Freshness {
    pub fn is_current(&self) -> bool {
        matches!(self, Freshness::Current)
    }
}

impl PolicyRecord {
    /// Evaluate freshness against a reference date.
    ///
    /// The window comes from the claim kind. A D-tier source is additionally
    /// never current for policy purposes, since it cannot justify policy.
    pub fn freshness(&self, now: chrono::NaiveDate) -> Freshness {
        if !self.source.trust_tier.may_justify_policy() {
            return Freshness::Unverifiable;
        }

        let retrieved =
            match chrono::NaiveDate::parse_from_str(&self.source.retrieved_on, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => return Freshness::Unverifiable,
            };

        // A retrieval date in the future is malformed, not fresh.
        let age = (now - retrieved).num_days();
        if age < 0 {
            return Freshness::Unverifiable;
        }

        let window = self.claim_kind.freshness_days();
        if age <= window {
            Freshness::Current
        } else {
            Freshness::Expired {
                age_days: age,
                window_days: window,
            }
        }
    }

    /// Return the payload only if the record may power a current claim.
    ///
    /// This is the enforcement point for "expired claims stay viewable but
    /// cannot silently power a claim presented as current."
    pub fn current_payload(&self, now: chrono::NaiveDate) -> Result<&PolicyPayload, Freshness> {
        let freshness = self.freshness(now);
        if freshness.is_current() {
            Ok(&self.payload)
        } else {
            Err(freshness)
        }
    }
}

/// A versioned store of policy records, one active version per family.
#[derive(Debug, Clone, Default)]
pub struct PolicyStore {
    records: Vec<PolicyRecord>,
}

/// Why a policy record was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// A D-tier source cannot justify policy.
    UntrustedTier,
    /// Version numbers must increase.
    NonMonotonicVersion { existing: u32, attempted: u32 },
    /// The record's family does not match the store key.
    FamilyMismatch,
    /// No record exists for the requested family.
    UnknownFamily(String),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::UntrustedTier => {
                write!(f, "a tier-D source cannot justify a policy claim")
            }
            PolicyError::NonMonotonicVersion {
                existing,
                attempted,
            } => write!(
                f,
                "policy version {attempted} does not advance beyond {existing}"
            ),
            PolicyError::FamilyMismatch => write!(f, "policy family does not match"),
            PolicyError::UnknownFamily(id) => write!(f, "no policy for family {id}"),
        }
    }
}

impl std::error::Error for PolicyError {}

impl PolicyStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a policy version.
    ///
    /// Rejects D-tier sources outright: a policy record that can never legally
    /// power a claim should not be storable as policy at all, or it will
    /// eventually be cited by mistake.
    pub fn publish(&mut self, record: PolicyRecord) -> Result<(), PolicyError> {
        if !record.source.trust_tier.may_justify_policy() {
            return Err(PolicyError::UntrustedTier);
        }

        let latest = self
            .records
            .iter()
            .filter(|r| r.family == record.family)
            .map(|r| r.version)
            .max();

        if let Some(existing) = latest {
            if record.version <= existing {
                return Err(PolicyError::NonMonotonicVersion {
                    existing,
                    attempted: record.version,
                });
            }
        }

        self.records.push(record);
        Ok(())
    }

    /// The highest-version record for a family, regardless of freshness.
    ///
    /// Returned so an expired policy stays *viewable*.
    pub fn latest_viewable(&self, family: &str) -> Option<&PolicyRecord> {
        self.records
            .iter()
            .filter(|r| r.family == family)
            .max_by_key(|r| r.version)
    }

    /// The highest-version record that is still current.
    ///
    /// If the newest version is expired, this intentionally does **not** fall
    /// back to an older version: an older record is even staler. Returning
    /// nothing is the honest answer.
    pub fn current(
        &self,
        family: &str,
        now: chrono::NaiveDate,
    ) -> Result<&PolicyRecord, Freshness> {
        match self.latest_viewable(family) {
            Some(record) => {
                if record.freshness(now).is_current() {
                    Ok(record)
                } else {
                    Err(record.freshness(now))
                }
            }
            None => Err(Freshness::Unverifiable),
        }
    }

    pub fn all(&self) -> &[PolicyRecord] {
        &self.records
    }

    pub fn families(&self) -> Vec<String> {
        let mut families: Vec<String> = self.records.iter().map(|r| r.family.clone()).collect();
        families.sort();
        families.dedup();
        families
    }
}

/// A job target resolved against the policy store.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedJobTarget {
    pub job_name: String,
    pub composite: String,
    pub required: u32,
    pub source_id: String,
    pub trust_tier: TrustTier,
}

/// Resolve a job target, refusing to serve a stale one.
pub fn resolve_job_target(
    store: &PolicyStore,
    job_name: &str,
    now: chrono::NaiveDate,
) -> Result<ResolvedJobTarget, JobTargetError> {
    let families = store.families();
    for family in &families {
        let record = match store.current(family, now) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if let PolicyPayload::JobTarget {
            job_name: name,
            required,
            composite,
        } = &record.payload
        {
            if name.eq_ignore_ascii_case(job_name) {
                return Ok(ResolvedJobTarget {
                    job_name: name.clone(),
                    composite: composite.clone(),
                    required: *required,
                    source_id: record.source.source_id.clone(),
                    trust_tier: record.source.trust_tier,
                });
            }
        }
    }
    Err(JobTargetError::NotFoundOrStale(job_name.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobTargetError {
    /// No current policy record describes this job.
    NotFoundOrStale(String),
}

impl std::fmt::Display for JobTargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobTargetError::NotFoundOrStale(job) => write!(
                f,
                "no current sourced policy for job {job}; the requirement cannot be \
                 presented as current"
            ),
        }
    }
}

impl std::error::Error for JobTargetError {}

/// A retest plan derived from current policy (REQ-013).
#[derive(Debug, Clone, PartialEq)]
pub struct RetestPlan {
    pub earliest_retest: chrono::NaiveDate,
    pub wait_days: u32,
    pub waiver_available: bool,
    pub source_id: String,
}

/// Compute the earliest retest date from current retest policy.
///
/// Refuses when the policy is stale rather than guessing a wait period: an
/// invented retest window would be a fabricated policy claim.
pub fn plan_retest(
    store: &PolicyStore,
    last_test: chrono::NaiveDate,
    now: chrono::NaiveDate,
) -> Result<RetestPlan, JobTargetError> {
    for family in store.families() {
        let record = match store.current(&family, now) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if let PolicyPayload::RetestPolicy {
            wait_days,
            waiver_available,
        } = &record.payload
        {
            return Ok(RetestPlan {
                earliest_retest: last_test + chrono::Duration::days(*wait_days as i64),
                wait_days: *wait_days,
                waiver_available: *waiver_available,
                source_id: record.source.source_id.clone(),
            });
        }
    }
    Err(JobTargetError::NotFoundOrStale("retest-policy".to_string()))
}
