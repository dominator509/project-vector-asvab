//! Release accounting: the V-000..V-021 verification harness (EP-010).
//!
//! Binding sources: `.agent/verification/HARNESS_LAWS.md` and
//! `.agent/verification/GRAPH.md`.
//!
//! The harness's central discipline is that every one of the 484 registry IDs
//! must be *accounted for*, and that a not-applicable classification requires
//! evidence that the subsystem is genuinely absent (law 5) rather than being a
//! convenient way to make the numbers work. This module implements the
//! classification rules so the accounting is derived from observable repository
//! state rather than hand-written.
//!
//! Nothing here can mark a test PASS. A PASS requires observed command evidence
//! (law 4), which this code deliberately cannot manufacture: the only statuses it
//! assigns by itself are the non-PASS ones.

use serde::{Deserialize, Serialize};

/// A status from `AGENTS.md` section 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Pending,
    Pass,
    Fail,
    BlockedPrerequisite,
    BlockedCapability,
    /// Not applicable. Requires proof the subsystem is absent (HARNESS_LAWS 5).
    SkippedNotApplicable,
    /// Gated by an external human or authority.
    AcceptedExternalGate,
}

impl TestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TestStatus::Pending => "PENDING",
            TestStatus::Pass => "PASS",
            TestStatus::Fail => "FAIL",
            TestStatus::BlockedPrerequisite => "BLOCKED_PREREQUISITE",
            TestStatus::BlockedCapability => "BLOCKED_CAPABILITY",
            TestStatus::SkippedNotApplicable => "SKIPPED_NOT_APPLICABLE",
            TestStatus::AcceptedExternalGate => "ACCEPTED_EXTERNAL_GATE",
        }
    }

    /// Whether this status is a terminal accounting for a test ID.
    pub fn is_accounted(self) -> bool {
        !matches!(self, TestStatus::Pending)
    }
}

/// A subsystem whose presence decides whether a group of tests applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsystemProbe {
    /// Human name, e.g. "blockchain".
    pub name: String,
    /// Path fragments that would indicate the subsystem exists.
    pub indicators: Vec<String>,
    /// Files actually found in the repository matching those indicators.
    pub found: Vec<String>,
}

impl SubsystemProbe {
    /// Whether the subsystem is present.
    ///
    /// A subsystem counts as absent only when no indicator matched. The probe
    /// records *what it looked for* as well as what it found, so the absence
    /// claim is falsifiable rather than a bare assertion.
    pub fn is_absent(&self) -> bool {
        self.found.is_empty()
    }

    /// The V-003 evidence string for this probe.
    pub fn evidence(&self) -> String {
        format!(
            "V-003 subsystem probe {:?}: searched for {:?} across tracked files; \
             matches found: {:?}",
            self.name, self.indicators, self.found
        )
    }
}

/// A registry row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryRow {
    pub test_id: String,
    pub source_group: String,
    pub kind: String,
    pub default_stage: String,
    pub applicability: String,
}

/// An accounting decision for one registry row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingRow {
    pub test_id: String,
    pub stage: String,
    pub status: TestStatus,
    pub evidence_path: String,
    pub blocker_or_na_reason: String,
}

/// Classify a registry row given the subsystem probes.
///
/// Returns the status the harness can assign *without* observed command
/// evidence. Any test whose group's subsystem is absent is classified
/// `SKIPPED_NOT_APPLICABLE` with the probe evidence attached. Everything else
/// stays `PENDING`, because this function has no basis to claim a PASS.
pub fn classify(row: &RegistryRow, probes: &[SubsystemProbe]) -> AccountingRow {
    // Which subsystem, if any, gates this group.
    let governing = probes.iter().find(|probe| {
        row.source_group
            .to_lowercase()
            .contains(&probe.name.to_lowercase())
    });

    match governing {
        Some(probe) if probe.is_absent() => AccountingRow {
            test_id: row.test_id.clone(),
            stage: row.default_stage.clone(),
            status: TestStatus::SkippedNotApplicable,
            evidence_path: String::new(),
            blocker_or_na_reason: probe.evidence(),
        },
        Some(probe) => AccountingRow {
            test_id: row.test_id.clone(),
            stage: row.default_stage.clone(),
            status: TestStatus::Pending,
            evidence_path: String::new(),
            blocker_or_na_reason: format!(
                "subsystem {:?} is present ({} indicator match(es)); test applies",
                probe.name,
                probe.found.len()
            ),
        },
        None => AccountingRow {
            test_id: row.test_id.clone(),
            stage: row.default_stage.clone(),
            status: TestStatus::Pending,
            evidence_path: String::new(),
            blocker_or_na_reason: String::new(),
        },
    }
}

/// The accounting summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingSummary {
    pub total: usize,
    pub by_status: Vec<(String, usize)>,
}

impl AccountingSummary {
    pub fn of(rows: &[AccountingRow]) -> Self {
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for row in rows {
            *counts.entry(row.status.as_str().to_string()).or_insert(0) += 1;
        }
        Self {
            total: rows.len(),
            by_status: counts.into_iter().collect(),
        }
    }

    pub fn count(&self, status: TestStatus) -> usize {
        self.by_status
            .iter()
            .find(|(name, _)| name == status.as_str())
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }

    /// Whether every row is accounted for.
    pub fn is_complete(&self) -> bool {
        self.count(TestStatus::Pending) == 0
    }
}

/// Render accounting rows as the CSV the harness script reads.
///
/// Column order matches `reports/COMPLETE_TEST_ACCOUNTING.csv`, because
/// `scripts/harness-accounting.sh` parses that file positionally by header.
pub fn render_csv(rows: &[AccountingRow]) -> String {
    let mut out = String::from("test_id,stage,status,evidence_path,blocker_or_na_reason\n");
    for row in rows {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            row.test_id,
            row.stage,
            row.status.as_str(),
            row.evidence_path,
            csv_escape(&row.blocker_or_na_reason)
        ));
    }
    out
}

/// Escape a field for CSV, quoting when it contains a comma, quote or newline.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Render the release verdict record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseVerdict {
    /// GO, CONDITIONAL_EXTERNAL_GATES or NO_GO.
    pub verdict: String,
    pub summary: AccountingSummary,
    /// External gates the verdict is conditional on.
    pub external_gates: Vec<String>,
    /// Reasons a GO is not available.
    pub blocking_findings: Vec<String>,
}

/// Decide the release verdict from the accounting and external gates.
///
/// The rule is deliberately conservative: any unaccounted test ID or any
/// outstanding external gate means the verdict is not an unqualified GO. A
/// candidate that has not been fully accounted for is not production-ready,
/// which is what `RELEASE.md` says.
pub fn decide_verdict(
    summary: &AccountingSummary,
    external_gates: &[String],
    blocking_findings: &[String],
) -> ReleaseVerdict {
    let pending = summary.count(TestStatus::Pending);
    let failures = summary.count(TestStatus::Fail);

    let mut blockers = blocking_findings.to_vec();
    if pending > 0 {
        blockers.push(format!(
            "{pending} registry test IDs remain PENDING and are not accounted for"
        ));
    }
    if failures > 0 {
        blockers.push(format!("{failures} registry test IDs are in FAIL state"));
    }

    let verdict = if !blockers.is_empty() {
        "NO_GO"
    } else if !external_gates.is_empty() {
        "CONDITIONAL_EXTERNAL_GATES"
    } else {
        "GO"
    };

    ReleaseVerdict {
        verdict: verdict.to_string(),
        summary: summary.clone(),
        external_gates: external_gates.to_vec(),
        blocking_findings: blockers,
    }
}
