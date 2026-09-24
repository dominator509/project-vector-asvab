//! EP-005 acceptance: `cat-standard` timing profile matches the published spec (REQ-006).
//!
//! `reference/asvab-test-specification.md` records the official published
//! CAT-ASVAB per-subtest time limits. The `cat-standard` profile shipped in
//! `crates/vector-domain/src/exam.rs` previously disagreed with that document
//! on four of its five subtests (AR 39 vs 55, WK 11 vs 9, PC 22 vs 27,
//! MK 20 vs 31, GS 11 vs 12), so a learner practising against the simulator
//! was training to the wrong clock.
//!
//! The property under test is that the shipped profile and the reference
//! document cannot silently diverge again: the profile is asserted against
//! the spec values, and the simulator is asserted to model exactly five
//! subtests (it is not the full CAT-ASVAB battery).

use vector_domain::exam::{
    default_profiles, ExamForm, CAT_STANDARD_SPEC_MINUTES, SIMULATED_SUBTESTS,
};

fn cat_profile() -> vector_domain::exam::TimingProfile {
    default_profiles()
        .into_iter()
        .find(|p| p.form == ExamForm::Cat)
        .expect("a CAT profile exists")
}

// ---------------------------------------------------------------------------
// REQ-006: the CAT profile matches the published reference spec, per subtest
// ---------------------------------------------------------------------------

#[test]
fn cat_standard_matches_reference_spec() {
    let profile = cat_profile();
    assert_eq!(
        profile.id, "cat-standard",
        "the CAT profile is cat-standard"
    );

    for (code, minutes) in CAT_STANDARD_SPEC_MINUTES {
        let actual = profile
            .per_subtest_seconds
            .iter()
            .find(|(c, _)| c == code)
            .map(|(_, s)| *s)
            .unwrap_or_else(|| panic!("cat-standard is missing subtest {code}"));
        assert_eq!(
            actual,
            minutes * 60,
            "cat-standard {code} must be {minutes} minutes per \
             reference/asvab-test-specification.md, found {} seconds",
            actual
        );
    }
}

// ---------------------------------------------------------------------------
// REQ-006: the profile is not carrying stale limits, and models exactly 5 subtests
// ---------------------------------------------------------------------------

#[test]
fn cat_standard_has_no_stale_subtest_limits() {
    let profile = cat_profile();

    // The four values that were wrong before this fix must not reappear.
    let stale = [
        ("AR", 39 * 60),
        ("WK", 11 * 60),
        ("PC", 22 * 60),
        ("MK", 20 * 60),
    ];
    for (code, seconds) in stale {
        let actual = profile
            .per_subtest_seconds
            .iter()
            .find(|(c, _)| c == code)
            .map(|(_, s)| *s);
        assert_ne!(
            actual,
            Some(seconds),
            "cat-standard {code} still carries the superseded limit of {} seconds",
            seconds
        );
    }
}

#[test]
fn cat_standard_models_exactly_the_five_simulated_subtests() {
    let profile = cat_profile();
    assert_eq!(
        profile.per_subtest_seconds.len(),
        SIMULATED_SUBTESTS.len(),
        "the simulator covers {} subtests, not the full CAT-ASVAB battery",
        SIMULATED_SUBTESTS.len()
    );
    for code in SIMULATED_SUBTESTS {
        assert!(
            profile.per_subtest_seconds.iter().any(|(c, _)| c == code),
            "cat-standard must define a limit for {code}"
        );
    }
}

#[test]
fn every_simulated_subtest_has_a_positive_limit() {
    let profile = cat_profile();
    for (code, seconds) in &profile.per_subtest_seconds {
        assert!(
            *seconds > 0,
            "cat-standard {code} must have a positive limit, found {seconds}"
        );
    }
}
