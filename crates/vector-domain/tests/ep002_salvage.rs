//! EP-002 salvage: the ASVAB subtest model (REQ-004).
//!
//! The stranded branch `feat/EP-001-desktop-foundation` (`0a51831`) contributed
//! an `AsvabSubtest` enum carrying the official two-letter codes and the AFQT
//! composition. That domain knowledge has been salvaged into the canonical
//! [`Subtest`] enum instead of importing a second type for the same concept.
//!
//! These tests are deliberately **not** of the shape the branch shipped. Its
//! `learner_test.rs` asserted:
//!
//! ```ignore
//! let subtests = vec![Subtest::GS, Subtest::AR, /* ... */];
//! assert_eq!(subtests.len(), 10);
//! ```
//!
//! which asserts a property of a vector the test itself just built and would
//! pass even if the enum had three variants. The tests below assert properties
//! of the *enum's own data* — [`Subtest::ALL`] and [`Subtest::code`] — so they
//! fail when the mapping is wrong.

use vector_domain::mastery::Subtest;

#[test]
fn every_subtest_has_a_distinct_official_code() {
    // A bijection: wrong code strings, or two subtests sharing one, both fail.
    let mut codes: Vec<&str> = Subtest::ALL.iter().map(|s| s.code()).collect();
    let total = codes.len();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), total, "subtest codes must be distinct");
}

#[test]
fn the_codes_are_exactly_the_official_ten() {
    // Pinned, so adding or renaming a subtest cannot pass unnoticed.
    let mut actual: Vec<&str> = Subtest::ALL.iter().map(|s| s.code()).collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        vec!["AI", "AO", "AR", "EI", "GS", "MC", "MK", "PC", "SI", "WK"]
    );
}

#[test]
fn codes_round_trip_through_from_code() {
    // Exercises the parser against the enum's real data, not a literal.
    for subtest in Subtest::ALL {
        assert_eq!(
            Subtest::from_code(subtest.code()),
            Some(subtest),
            "{} must round-trip",
            subtest.code()
        );
    }
}

#[test]
fn from_code_is_case_insensitive_and_trims() {
    assert_eq!(Subtest::from_code("ar"), Some(Subtest::AR));
    assert_eq!(Subtest::from_code("  Wk  "), Some(Subtest::WK));
    assert_eq!(Subtest::from_code("nope"), None);
    assert_eq!(Subtest::from_code(""), None);
}

#[test]
fn the_afqt_is_exactly_four_subtests() {
    // The AFQT is computed from two verbal (WK, PC) and two mathematics
    // (AR, MK) subtests. This is the fact the enum previously could not express;
    // asserting the exact set means any change to it fails here.
    let mut afqt: Vec<&str> = Subtest::afqt_subtests().iter().map(|s| s.code()).collect();
    afqt.sort_unstable();
    assert_eq!(afqt, vec!["AR", "MK", "PC", "WK"]);
}

#[test]
fn the_six_line_score_subtests_are_not_afqt() {
    // The complement must hold too, or `is_afqt` could return true for
    // everything and still satisfy the test above in spirit.
    for subtest in Subtest::ALL {
        let expected = matches!(
            subtest,
            Subtest::AR | Subtest::WK | Subtest::PC | Subtest::MK
        );
        assert_eq!(
            subtest.is_afqt(),
            expected,
            "{} AFQT membership is wrong",
            subtest.code()
        );
    }
}

#[test]
fn every_subtest_has_a_human_readable_name() {
    for subtest in Subtest::ALL {
        let name = subtest.name();
        assert!(!name.trim().is_empty(), "{} needs a name", subtest.code());
        // The name must not merely echo the code.
        assert_ne!(
            name,
            subtest.code(),
            "{} name is just its code",
            subtest.code()
        );
    }
    // Spot-check real names, so a placeholder cannot pass.
    assert_eq!(Subtest::AR.name(), "Arithmetic Reasoning");
    assert_eq!(Subtest::PC.name(), "Paragraph Comprehension");
    assert_eq!(Subtest::AO.name(), "Assembling Objects");
}

#[test]
fn all_contains_each_subtest_exactly_once() {
    let mut seen = Vec::new();
    for subtest in Subtest::ALL {
        assert!(!seen.contains(&subtest), "{} listed twice", subtest.code());
        seen.push(subtest);
    }
    assert_eq!(seen.len(), 10);
}

#[test]
fn subtest_round_trips_through_serde() {
    // Mastery records persist per subtest, so the enum must survive storage.
    for subtest in Subtest::ALL {
        let json = serde_json::to_string(&subtest).expect("serialize");
        let back: Subtest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, subtest);
    }
}
