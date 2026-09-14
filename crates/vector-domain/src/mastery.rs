use serde::{Deserialize, Serialize};

/// The ten ASVAB subtests, modeled separately (REQ-004).
///
/// The variant names are the official two-letter subtest codes. The domain
/// knowledge that matters — which subtests compose the AFQT — lives in
/// [`Subtest::is_afqt`].
///
/// Salvaged from the stranded `feat/EP-001-desktop-foundation` branch
/// (`0a51831`), whose `subtests.rs` carried the official code mapping and the
/// AFQT composition that this enum previously lacked. The branch's separate
/// `AsvabSubtest` enum was not imported: two enums for one concept is the same
/// duplicate-implementation problem as leaving a stub beside its replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Subtest {
    /// General Science
    GS,
    /// Arithmetic Reasoning
    AR,
    /// Word Knowledge
    WK,
    /// Paragraph Comprehension
    PC,
    /// Mathematics Knowledge
    MK,
    /// Electronics Information
    EI,
    /// Auto Information
    AI,
    /// Shop Information
    SI,
    /// Mechanical Comprehension
    MC,
    /// Assembling Objects
    AO,
}

impl Subtest {
    /// Every subtest, in official presentation order.
    ///
    /// A canonical list lets tests and callers iterate rather than re-listing
    /// variants, which is how a subtest silently goes missing from coverage.
    pub const ALL: [Subtest; 10] = [
        Subtest::GS,
        Subtest::AR,
        Subtest::WK,
        Subtest::PC,
        Subtest::MK,
        Subtest::EI,
        Subtest::AI,
        Subtest::SI,
        Subtest::MC,
        Subtest::AO,
    ];

    /// The official two-letter code for this subtest.
    pub fn code(self) -> &'static str {
        match self {
            Subtest::GS => "GS",
            Subtest::AR => "AR",
            Subtest::WK => "WK",
            Subtest::PC => "PC",
            Subtest::MK => "MK",
            Subtest::EI => "EI",
            Subtest::AI => "AI",
            Subtest::SI => "SI",
            Subtest::MC => "MC",
            Subtest::AO => "AO",
        }
    }

    /// The full subtest name, for learner-facing text.
    pub fn name(self) -> &'static str {
        match self {
            Subtest::GS => "General Science",
            Subtest::AR => "Arithmetic Reasoning",
            Subtest::WK => "Word Knowledge",
            Subtest::PC => "Paragraph Comprehension",
            Subtest::MK => "Mathematics Knowledge",
            Subtest::EI => "Electronics Information",
            Subtest::AI => "Auto Information",
            Subtest::SI => "Shop Information",
            Subtest::MC => "Mechanical Comprehension",
            Subtest::AO => "Assembling Objects",
        }
    }

    /// Parse an official subtest code, case-insensitively.
    pub fn from_code(code: &str) -> Option<Subtest> {
        let upper = code.trim().to_ascii_uppercase();
        Subtest::ALL.into_iter().find(|s| s.code() == upper)
    }

    /// Whether this subtest contributes to the AFQT composite.
    ///
    /// The AFQT is computed from two verbal subtests (Word Knowledge and
    /// Paragraph Comprehension) and two mathematics subtests (Arithmetic
    /// Reasoning and Mathematics Knowledge). The remaining six subtests
    /// contribute to individual line scores, never to the AFQT.
    ///
    /// This is the domain fact the enum previously could not express, and it
    /// matters: scoring or readiness logic that treated all ten subtests as
    /// AFQT inputs would be wrong.
    pub fn is_afqt(self) -> bool {
        matches!(self, Subtest::AR | Subtest::WK | Subtest::PC | Subtest::MK)
    }

    /// The four AFQT-contributing subtests.
    pub fn afqt_subtests() -> Vec<Subtest> {
        Subtest::ALL.into_iter().filter(|s| s.is_afqt()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mastery {
    pub diagnostic_score: f64,
    pub uncertainty: f64,
}

impl Default for Mastery {
    fn default() -> Self {
        Self::new()
    }
}

impl Mastery {
    pub fn new() -> Self {
        Self {
            diagnostic_score: 0.0,
            uncertainty: 1.0,
        }
    }

    pub fn diagnostic_score(&self) -> f64 {
        self.diagnostic_score
    }

    pub fn uncertainty(&self) -> f64 {
        self.uncertainty
    }
}
