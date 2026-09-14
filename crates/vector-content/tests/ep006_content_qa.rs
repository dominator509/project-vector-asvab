//! EP-006 acceptance: content QA audit and internal difficulty calibration
//! (REQ-048, REQ-049).

use vector_content::content_qa::{
    calibrate, describe_difficulty, difficulty_label, official_equivalence_claim_permitted,
    AuditEntry, CalibrationError, ContentQa, DifficultyLabel, ReviewError, ReviewState,
    MIN_CALIBRATION_OBSERVATIONS,
};

const AT: &str = "2026-09-10T00:00:00Z";

fn reviewed_qa() -> ContentQa {
    let mut qa = ContentQa::new();
    qa.add("pack-1");
    qa.decide(
        "pack-1",
        "reviewer-a",
        ReviewState::InReview,
        "starting review",
        AT,
    )
    .expect("in review");
    qa.decide(
        "pack-1",
        "reviewer-a",
        ReviewState::Approved,
        "sources verified",
        AT,
    )
    .expect("approved");
    qa
}

// ---------------------------------------------------------------------------
// REQ-048: review state, history, rollback, audit
// ---------------------------------------------------------------------------

#[test]
fn only_approved_content_is_activatable() {
    for state in [
        ReviewState::Unreviewed,
        ReviewState::InReview,
        ReviewState::Rejected,
        ReviewState::Superseded,
    ] {
        assert!(
            !state.is_activatable(),
            "{state:?} must not be servable to a learner"
        );
    }
    assert!(ReviewState::Approved.is_activatable());
}

#[test]
fn unreviewed_content_cannot_be_activated() {
    // The tempting default is to serve whatever is present. Refusing is the
    // whole point of a review gate.
    let mut qa = ContentQa::new();
    qa.add("pack-1");

    assert_eq!(
        qa.activate("pack-1", "system", AT),
        Err(ReviewError::NotReviewed("pack-1".to_string()))
    );
}

#[test]
fn content_in_review_cannot_be_activated() {
    let mut qa = ContentQa::new();
    qa.add("pack-1");
    qa.decide("pack-1", "reviewer-a", ReviewState::InReview, "looking", AT)
        .expect("in review");

    assert!(matches!(
        qa.activate("pack-1", "system", AT),
        Err(ReviewError::NotReviewed(_))
    ));
}

#[test]
fn rejected_content_cannot_be_activated() {
    let mut qa = ContentQa::new();
    qa.add("pack-1");
    qa.decide("pack-1", "r", ReviewState::InReview, "looking", AT)
        .expect("in review");
    qa.decide("pack-1", "r", ReviewState::Rejected, "unsourced claims", AT)
        .expect("rejected");

    assert!(qa.activate("pack-1", "system", AT).is_err());
}

#[test]
fn approved_content_can_be_activated() {
    let mut qa = reviewed_qa();
    assert!(qa.activate("pack-1", "system", AT).is_ok());
    assert_eq!(qa.item("pack-1").unwrap().state, ReviewState::Superseded);
}

#[test]
fn a_decision_requires_a_named_reviewer() {
    let mut qa = ContentQa::new();
    qa.add("pack-1");
    assert_eq!(
        qa.decide("pack-1", "  ", ReviewState::InReview, "note", AT),
        Err(ReviewError::MissingReviewer)
    );
}

#[test]
fn a_decision_requires_a_rationale() {
    // An unexplained approval is not auditable.
    let mut qa = ContentQa::new();
    qa.add("pack-1");
    assert_eq!(
        qa.decide("pack-1", "reviewer-a", ReviewState::InReview, "   ", AT),
        Err(ReviewError::MissingRationale)
    );
}

#[test]
fn an_unknown_item_is_refused() {
    let mut qa = ContentQa::new();
    assert!(matches!(
        qa.decide("nope", "r", ReviewState::InReview, "note", AT),
        Err(ReviewError::UnknownItem(_))
    ));
}

#[test]
fn invalid_transitions_are_refused() {
    // Skipping review straight to approval would defeat the gate.
    let mut qa = ContentQa::new();
    qa.add("pack-1");
    assert_eq!(
        qa.decide("pack-1", "r", ReviewState::Approved, "looks fine", AT),
        Err(ReviewError::InvalidTransition {
            from: ReviewState::Unreviewed,
            to: ReviewState::Approved
        })
    );
}

#[test]
fn every_decision_appends_an_audit_entry() {
    let qa = reviewed_qa();
    let history = qa.history("pack-1");

    assert_eq!(history.len(), 2, "each decision must be recorded");
    assert_eq!(history[0].from, ReviewState::Unreviewed);
    assert_eq!(history[0].to, ReviewState::InReview);
    assert_eq!(history[1].from, ReviewState::InReview);
    assert_eq!(history[1].to, ReviewState::Approved);
    assert_eq!(history[0].actor, "reviewer-a");
    assert!(!history[0].note.is_empty());
}

#[test]
fn audit_entries_have_unique_increasing_sequences() {
    let mut qa = reviewed_qa();
    qa.activate("pack-1", "system", AT).expect("activate");

    let audit = qa.audit();
    assert_eq!(audit.len(), 3);
    for pair in audit.windows(2) {
        assert!(
            pair[1].sequence > pair[0].sequence,
            "audit sequence must increase"
        );
    }
}

#[test]
fn rollback_is_recorded_rather_than_hidden() {
    // A quiet revert would make history look as though the approval never
    // happened, which is exactly what an audit trail must prevent.
    let mut qa = reviewed_qa();
    qa.rollback("pack-1", "reviewer-b", "found an unsourced claim", AT)
        .expect("rollback");

    let history = qa.history("pack-1");
    assert_eq!(
        history.len(),
        3,
        "the approval and the rollback both remain"
    );
    let last = history.last().expect("a last entry");

    assert_eq!(last.from, ReviewState::Approved);
    assert_eq!(last.to, ReviewState::Rejected);
    assert!(last.note.contains("unsourced"));
    assert_eq!(qa.item("pack-1").unwrap().state, ReviewState::Rejected);
}

#[test]
fn rollback_requires_a_reason() {
    let mut qa = reviewed_qa();
    assert_eq!(
        qa.rollback("pack-1", "reviewer-b", "  ", AT),
        Err(ReviewError::MissingRationale)
    );
}

#[test]
fn rejected_content_may_be_reconsidered() {
    let mut qa = reviewed_qa();
    qa.rollback("pack-1", "r", "defect", AT).expect("rollback");
    qa.decide("pack-1", "r2", ReviewState::InReview, "rework done", AT)
        .expect("re-review allowed");
    qa.decide("pack-1", "r2", ReviewState::Approved, "fixed", AT)
        .expect("approval allowed");
    assert!(qa.item("pack-1").unwrap().state.is_activatable());
}

#[test]
fn the_audit_trail_is_serializable() {
    // Audit history is evidence; it must survive persistence.
    let qa = reviewed_qa();
    let json = serde_json::to_string(&qa.history("pack-1")).expect("serialize");
    let back: Vec<AuditEntry> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(qa.history("pack-1"), back);
}

// ---------------------------------------------------------------------------
// REQ-049: internal calibration, no equivalence claim
// ---------------------------------------------------------------------------

#[test]
fn an_official_equivalence_claim_is_never_permitted() {
    assert!(
        !official_equivalence_claim_permitted(),
        "REQ-049 forbids claiming official psychometric equivalence"
    );
}

#[test]
fn calibration_requires_enough_observations() {
    // An estimate from a handful of attempts is noise presented as measurement.
    let result = calibrate(MIN_CALIBRATION_OBSERVATIONS - 1, 5);
    assert!(matches!(
        result,
        Err(CalibrationError::InsufficientObservations { .. })
    ));
}

#[test]
fn calibration_succeeds_at_the_observation_floor() {
    let result = calibrate(MIN_CALIBRATION_OBSERVATIONS, 15);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().observations, MIN_CALIBRATION_OBSERVATIONS);
}

#[test]
fn more_correct_answers_means_an_easier_item() {
    let easy = calibrate(100, 90).expect("calibrate");
    let hard = calibrate(100, 10).expect("calibrate");
    assert!(
        easy.internal < hard.internal,
        "a higher success rate must produce a lower difficulty: {} !< {}",
        easy.internal,
        hard.internal
    );
    assert_eq!(difficulty_label(&easy), DifficultyLabel::Easy);
    assert_eq!(difficulty_label(&hard), DifficultyLabel::Hard);
}

#[test]
fn a_mid_difficulty_item_is_labelled_moderate() {
    let mid = calibrate(100, 50).expect("calibrate");
    assert_eq!(difficulty_label(&mid), DifficultyLabel::Moderate);
}

#[test]
fn calibration_never_produces_a_non_finite_value() {
    // A perfect or zero success rate must not produce an infinite logit.
    for correct in [0u32, 1, 49, 50, 99, 100] {
        let result = calibrate(100, correct).expect("calibrate");
        assert!(
            result.internal.is_finite(),
            "internal estimate must be finite for {correct}/100"
        );
        assert!(
            result.standard_error.is_finite(),
            "standard error must be finite for {correct}/100"
        );
        assert!(result.standard_error >= 0.0);
    }
}

#[test]
fn more_observations_reduce_the_standard_error() {
    let few = calibrate(40, 20).expect("calibrate");
    let many = calibrate(4000, 2000).expect("calibrate");
    assert!(
        many.standard_error < few.standard_error,
        "more evidence must narrow the estimate: {} !< {}",
        many.standard_error,
        few.standard_error
    );
}

#[test]
fn more_correct_than_attempts_is_invalid() {
    assert_eq!(calibrate(50, 51), Err(CalibrationError::InvalidInput));
}

#[test]
fn the_description_never_implies_official_equivalence() {
    let difficulty = calibrate(100, 50).expect("calibrate");
    let text = describe_difficulty(&difficulty);

    assert!(text.contains("internal estimate"));
    assert!(text.contains("not an official psychometric value"));
    // It must not read as an official parameter claim.
    for forbidden in [
        "official difficulty",
        "equivalent to",
        "calibrated to the ASVAB",
    ] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "description implies an official claim: {text}"
        );
    }
}

#[test]
fn the_description_states_how_much_evidence_it_rests_on() {
    let difficulty = calibrate(120, 60).expect("calibrate");
    let text = describe_difficulty(&difficulty);
    assert!(
        text.contains("120"),
        "the observation count must be visible"
    );
}

#[test]
fn calibration_round_trips_through_serde() {
    let difficulty = calibrate(100, 40).expect("calibrate");
    let json = serde_json::to_string(&difficulty).expect("serialize");
    let back = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(difficulty, back);
}

#[test]
fn the_equivalence_refusal_explains_itself() {
    let message = CalibrationError::EquivalenceClaimRefused.to_string();
    assert!(message.contains("internal estimate"));
    assert!(message.contains("must not be presented"));
}
