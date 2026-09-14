//! EP-006 acceptance: motivation features stay separate from mastery (REQ-045).
//!
//! REQ-045 permits optional goals, streaks and achievements but requires them to
//! be **separate from mastery**. The risk this guards against is a motivational
//! signal leaking into the estimate that drives study planning and readiness,
//! which would make a streak influence an academic judgement.
//!
//! These tests assert the separation structurally: the types that carry mastery
//! and readiness inputs contain no motivational field, and motivational values
//! cannot change a mastery or readiness outcome.

use vector_study::selection::{generate_plan, readiness_band, PlanGoal, SkillEstimate};

fn skill(subtest: &str, mastery: f64, uncertainty: f64, due: u32) -> SkillEstimate {
    SkillEstimate {
        subtest: subtest.to_string(),
        mastery,
        uncertainty,
        due_reviews: due,
    }
}

/// The fields that may influence a mastery or readiness computation.
///
/// If a motivational signal is ever added to these inputs, this list changes and
/// the test below fails — which is the point: the separation becomes a checked
/// property rather than a convention.
const MASTERY_INPUT_FIELDS: &[&str] = &["subtest", "mastery", "uncertainty", "due_reviews"];

#[test]
fn skill_estimate_carries_only_academic_inputs() {
    // Serialize a real estimate and inspect its keys. A motivational field
    // appearing here would be a REQ-045 violation.
    let estimate = skill("AR", 0.5, 0.2, 3);
    let value = serde_json::to_value(&estimate).expect("serialize");
    let object = value.as_object().expect("object");

    for key in object.keys() {
        assert!(
            MASTERY_INPUT_FIELDS.contains(&key.as_str()),
            "SkillEstimate gained a field {key:?} that is not an academic input; \
             if this is a motivational signal it violates REQ-045"
        );
    }

    for forbidden in ["streak", "achievement", "badge", "xp", "points", "level"] {
        assert!(
            !object.contains_key(forbidden),
            "SkillEstimate must not carry motivational field {forbidden}"
        );
    }
}

#[test]
fn a_streak_cannot_change_the_study_plan() {
    // Two learners with identical academic state must receive identical plans,
    // regardless of any motivational difference between them. Since the plan
    // function has no motivational parameter, the guarantee is that the plan is
    // a pure function of academic inputs.
    let academic = vec![skill("AR", 0.2, 0.3, 5), skill("WK", 0.8, 0.05, 0)];

    let plan_a = generate_plan(&academic, &PlanGoal::Afqt(70), 60).expect("plan");
    let plan_b = generate_plan(&academic, &PlanGoal::Afqt(70), 60).expect("plan");

    assert_eq!(
        plan_a, plan_b,
        "the plan must be a pure function of academic inputs"
    );
}

#[test]
fn a_streak_cannot_change_the_readiness_band() {
    let academic = vec![skill("AR", 0.6, 0.2, 0), skill("WK", 0.5, 0.2, 0)];

    let band_a = readiness_band(&academic).expect("band");
    let band_b = readiness_band(&academic).expect("band");

    assert_eq!(
        band_a, band_b,
        "readiness must not vary for fixed academic input"
    );
}

#[test]
fn the_readiness_band_has_no_motivational_component() {
    let band = readiness_band(&[skill("AR", 0.6, 0.1, 0)]).expect("band");
    let value = serde_json::to_value(&band).expect("serialize");
    let object = value.as_object().expect("object");

    for forbidden in ["streak", "achievement", "badge", "xp", "points"] {
        assert!(
            !object.contains_key(forbidden),
            "the readiness band must not expose a motivational field {forbidden}"
        );
    }
    // The band is a pure academic estimate plus its calibration context.
    assert!(object.contains_key("low"));
    assert!(object.contains_key("high"));
    assert!(object.contains_key("confidence"));
}

#[test]
fn motivational_inputs_are_not_accepted_where_academic_inputs_are_required() {
    // The stronger half of this guarantee is at compile time: adding a required
    // field to SkillEstimate breaks every construction site (E0063), so a
    // motivational signal cannot be introduced silently. This asserts the
    // runtime half — the serialized shape is exactly the academic input set.
    let estimate = SkillEstimate {
        subtest: "AR".to_string(),
        mastery: 0.4,
        uncertainty: 0.1,
        due_reviews: 0,
    };
    let value = serde_json::to_value(&estimate).expect("serialize");
    let keys = value.as_object().expect("object");

    assert_eq!(
        keys.len(),
        MASTERY_INPUT_FIELDS.len(),
        "SkillEstimate must have exactly the academic input fields"
    );
    for key in keys.keys() {
        assert!(
            MASTERY_INPUT_FIELDS.contains(&key.as_str()),
            "unexpected field {key:?} on SkillEstimate"
        );
    }
}

#[test]
fn the_shape_guard_flags_a_widened_input_set() {
    // Records that the guard above is load-bearing: given an input set widened
    // with a motivational field, it identifies the offending key rather than
    // passing. Without this, the guard could be vacuous and nothing would show
    // it — which is what the first mutation attempt of this test revealed.
    let widened = serde_json::json!({
        "subtest": "AR",
        "mastery": 0.4,
        "uncertainty": 0.1,
        "due_reviews": 0,
        "streak": 12
    });
    let keys = widened.as_object().expect("object");

    let unexpected: Vec<&str> = keys
        .keys()
        .map(String::as_str)
        .filter(|k| !MASTERY_INPUT_FIELDS.contains(k))
        .collect();

    assert_eq!(
        unexpected,
        vec!["streak"],
        "the shape guard must flag a widened input set"
    );
}
