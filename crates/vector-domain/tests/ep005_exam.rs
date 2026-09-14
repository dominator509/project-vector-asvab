//! EP-005 acceptance: exam simulator navigation and timing (REQ-006).
//!
//! The behavioural property under test is that the CAT form forbids
//! backtracking while the paper form allows it. A simulator that lets a learner
//! return to a committed CAT answer teaches the wrong test-taking behaviour.

use vector_domain::exam::{
    default_profiles, ExamError, ExamForm, ExamSession, TimingProfile, OPTIONS_PER_ITEM,
};

fn items(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("q{i}")).collect()
}

fn cat_profile() -> TimingProfile {
    default_profiles()
        .into_iter()
        .find(|p| p.form == ExamForm::Cat)
        .expect("a CAT profile exists")
}

fn paper_profile() -> TimingProfile {
    default_profiles()
        .into_iter()
        .find(|p| p.form == ExamForm::Paper)
        .expect("a paper profile exists")
}

// ---------------------------------------------------------------------------
// REQ-006: no backtracking on the CAT form
// ---------------------------------------------------------------------------

#[test]
fn the_cat_form_forbids_backtracking() {
    assert!(
        !ExamForm::Cat.allows_backtracking(),
        "CAT must not allow returning to a previous item"
    );
    let mut session = ExamSession::start(&cat_profile(), items(5)).expect("start");

    session.answer(1).expect("answer item 0");
    session.advance().expect("advance to item 1");
    assert_eq!(session.cursor, 1);

    assert_eq!(
        session.previous(),
        Err(ExamError::BacktrackingForbidden),
        "previous() must be refused on a CAT form"
    );
    assert_eq!(session.cursor, 1, "a refused move must not change position");
}

#[test]
fn the_cat_form_forbids_jumping_to_an_earlier_item() {
    let mut session = ExamSession::start(&cat_profile(), items(5)).expect("start");
    session.answer(0).expect("answer");
    session.advance().expect("advance");
    session.answer(0).expect("answer");
    session.advance().expect("advance");

    assert_eq!(session.go_to(0), Err(ExamError::BacktrackingForbidden));
    assert_eq!(session.go_to(4), Err(ExamError::BacktrackingForbidden));
}

#[test]
fn a_committed_cat_answer_cannot_be_revised() {
    let mut session = ExamSession::start(&cat_profile(), items(3)).expect("start");

    session.answer(0).expect("answer item 0 with option 0");
    assert_eq!(session.answer_at(0), Some(Some(0)));

    // Even before advancing, the committed answer is locked.
    assert_eq!(
        session.answer(2),
        Err(ExamError::BacktrackingForbidden),
        "a CAT answer is committed once given"
    );
    assert_eq!(session.revise(0, 3), Err(ExamError::BacktrackingForbidden));
    assert_eq!(
        session.answer_at(0),
        Some(Some(0)),
        "the original answer must be preserved"
    );
}

#[test]
fn the_paper_form_permits_backtracking_and_revision() {
    assert!(ExamForm::Paper.allows_backtracking());
    let mut session = ExamSession::start(&paper_profile(), items(5)).expect("start");

    session.answer(1).expect("answer 0");
    session.advance().expect("advance");
    session.answer(2).expect("answer 1");
    session.advance().expect("advance");

    session.previous().expect("paper form allows going back");
    assert_eq!(session.cursor, 1);

    session.go_to(0).expect("paper form allows jumping");
    assert_eq!(session.cursor, 0);

    // An earlier answer may be changed on the paper form.
    session.revise(1, 3).expect("revision allowed");
    assert_eq!(session.answer_at(1), Some(Some(3)));
}

#[test]
fn the_paper_form_permits_answering_out_of_order() {
    let mut session = ExamSession::start(&paper_profile(), items(4)).expect("start");

    session.go_to(3).expect("jump to last");
    session.answer(2).expect("answer last first");
    session.go_to(1).expect("jump back");
    session.answer(0).expect("answer second");

    assert_eq!(session.answer_at(3), Some(Some(2)));
    assert_eq!(session.answer_at(1), Some(Some(0)));
    assert_eq!(
        session.answer_at(0),
        Some(None),
        "unanswered items stay empty"
    );
}

// ---------------------------------------------------------------------------
// Boundaries and invalid input
// ---------------------------------------------------------------------------

#[test]
fn an_invalid_option_is_refused() {
    let mut session = ExamSession::start(&cat_profile(), items(3)).expect("start");
    assert_eq!(
        session.answer(OPTIONS_PER_ITEM),
        Err(ExamError::InvalidOption(OPTIONS_PER_ITEM)),
        "an option beyond the item's choices must be refused"
    );
    let mut paper = ExamSession::start(&paper_profile(), items(3)).expect("start");
    assert_eq!(paper.answer(99), Err(ExamError::InvalidOption(99)));
    assert_eq!(paper.revise(0, 99), Err(ExamError::InvalidOption(99)));
}

#[test]
fn a_zero_length_exam_cannot_start() {
    assert!(ExamSession::start(&cat_profile(), vec![]).is_err());
}

#[test]
fn previous_at_the_first_item_is_refused() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    assert_eq!(
        session.previous(),
        Err(ExamError::OutOfRange { index: 0, len: 3 })
    );
}

#[test]
fn jumping_outside_the_exam_is_refused() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    assert_eq!(
        session.go_to(3),
        Err(ExamError::OutOfRange { index: 3, len: 3 })
    );
    assert_eq!(
        session.go_to(999),
        Err(ExamError::OutOfRange { index: 999, len: 3 })
    );
}

#[test]
fn advancing_past_the_last_item_submits_the_exam() {
    let mut session = ExamSession::start(&paper_profile(), items(2)).expect("start");
    session.answer(0).expect("answer");
    session.advance().expect("advance to item 1");
    assert!(!session.finished);
    session.answer(0).expect("answer");
    session.advance().expect("advance past last");
    assert!(session.finished, "advancing past the last item submits");
}

#[test]
fn a_finished_exam_refuses_further_navigation() {
    let mut session = ExamSession::start(&paper_profile(), items(1)).expect("start");
    session.answer(0).expect("answer");
    session.advance().expect("submit");

    assert_eq!(session.answer(1), Err(ExamError::AlreadyFinished));
    assert_eq!(session.previous(), Err(ExamError::AlreadyFinished));
    assert_eq!(session.go_to(0), Err(ExamError::AlreadyFinished));
    assert_eq!(session.skip(), Err(ExamError::AlreadyFinished));
}

// ---------------------------------------------------------------------------
// Timing (REQ-006: versioned timing profiles)
// ---------------------------------------------------------------------------

#[test]
fn time_expiry_ends_the_exam_and_blocks_navigation() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    session.tick(session.remaining_seconds);

    assert_eq!(session.remaining_seconds, 0);
    assert!(session.finished, "time expiry finishes the exam");
    assert_eq!(session.go_to(1), Err(ExamError::AlreadyFinished));
    assert_eq!(session.answer(0), Err(ExamError::AlreadyFinished));
}

#[test]
fn time_never_goes_negative() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    let total = session.remaining_seconds;
    session.tick(total + 10_000);
    assert_eq!(session.remaining_seconds, 0, "the clock must not underflow");
}

#[test]
fn ticking_time_reduces_the_remaining_budget() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    let total = session.remaining_seconds;
    session.tick(60);
    assert_eq!(session.remaining_seconds, total - 60);
    assert!(!session.finished);
}

#[test]
fn timing_profiles_are_versioned_and_dated() {
    for profile in default_profiles() {
        profile.validate().expect("shipped profiles must be valid");
        assert!(profile.version >= 1, "{} needs a version", profile.id);
        assert!(
            !profile.source_reviewed_on.is_empty(),
            "{} must record when it was reviewed against sources",
            profile.id
        );
    }
}

#[test]
fn the_cat_and_paper_profiles_are_distinct_and_typed() {
    let profiles = default_profiles();
    let cat = profiles
        .iter()
        .find(|p| p.form == ExamForm::Cat)
        .expect("CAT profile");
    let paper = profiles
        .iter()
        .find(|p| p.form == ExamForm::Paper)
        .expect("paper profile");

    assert_ne!(cat.id, paper.id, "the two forms need distinct profiles");
    assert_eq!(cat.version, paper.version);
}

#[test]
fn lookup_by_subtest_is_case_insensitive() {
    let profile = cat_profile();
    let upper = profile.seconds_for("AR");
    assert!(upper.is_some());
    assert_eq!(profile.seconds_for("ar"), upper);
    assert_eq!(profile.seconds_for("nope"), None);
}

#[test]
fn an_invalid_profile_is_refused() {
    let mut zero_time = cat_profile();
    zero_time.per_subtest_seconds = vec![("AR".to_string(), 0)];
    assert!(zero_time.validate().is_err(), "zero time must be refused");

    let mut no_items = cat_profile();
    no_items.per_subtest_seconds = vec![];
    assert!(no_items.validate().is_err());

    let mut dup = cat_profile();
    dup.per_subtest_seconds = vec![("AR".to_string(), 60), ("ar".to_string(), 90)];
    assert!(
        dup.validate().is_err(),
        "duplicate subtests must be refused"
    );

    let mut no_version = cat_profile();
    no_version.version = 0;
    assert!(no_version.validate().is_err());

    let mut blank_id = cat_profile();
    blank_id.id = "  ".to_string();
    assert!(blank_id.validate().is_err());
}

#[test]
fn session_start_refuses_an_invalid_profile() {
    let mut bad = cat_profile();
    bad.per_subtest_seconds = vec![("AR".to_string(), 0)];
    assert!(ExamSession::start(&bad, items(3)).is_err());
}

#[test]
fn skipping_is_explicit_and_advances() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    session.skip().expect("skip item 0");
    assert_eq!(session.cursor, 1);
    assert_eq!(
        session.answer_at(0),
        Some(None),
        "skipped items are unanswered"
    );
    assert_eq!(session.answered_count(), 0);
}

#[test]
fn completion_requires_every_item_answered() {
    let mut session = ExamSession::start(&paper_profile(), items(3)).expect("start");
    assert!(!session.is_complete());

    session.answer(0).expect("answer");
    session.advance().expect("advance");
    session.answer(0).expect("answer");
    session.advance().expect("advance");
    assert!(!session.is_complete(), "the last item is still unanswered");

    session.answer(0).expect("answer");
    assert!(session.is_complete());
    assert_eq!(session.answered_count(), 3);
}

#[test]
fn pausing_is_governed_by_the_profile() {
    let cat = cat_profile();
    let session = ExamSession::start(&cat, items(3)).expect("start");
    assert!(
        !session.pausable(&cat),
        "the standard profile is not pausable, matching exam conditions"
    );

    let mut pausable = cat.clone();
    pausable.pausable = true;
    assert!(session.pausable(&pausable));

    // A profile that does not match the session must not grant pausing.
    let mut other = pausable.clone();
    other.id = "other".to_string();
    assert!(!session.pausable(&other));
}

#[test]
fn session_round_trips_through_serde() {
    let profile = cat_profile();
    let mut session = ExamSession::start(&profile, items(3)).expect("start");
    session.answer(2).expect("answer");

    let json = serde_json::to_string(&session).expect("serialize");
    let back: ExamSession = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(session, back, "an exam session must survive persistence");
}
