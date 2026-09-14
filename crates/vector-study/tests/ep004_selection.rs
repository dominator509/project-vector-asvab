//! EP-004 acceptance: adaptive selection, planning and readiness (REQ-003, REQ-011).

use vector_study::selection::{
    generate_plan, readiness_band, DrillReason, PlanGoal, SkillEstimate,
};

fn skill(subtest: &str, mastery: f64, uncertainty: f64, due: u32) -> SkillEstimate {
    SkillEstimate {
        subtest: subtest.to_string(),
        mastery,
        uncertainty,
        due_reviews: due,
    }
}

#[test]
fn plan_concentrates_time_on_the_weakest_skill() {
    let skills = vec![
        skill("AR", 0.9, 0.05, 0), // strong, nothing due
        skill("WK", 0.2, 0.2, 0),  // weak
        skill("PC", 0.5, 0.3, 5),  // some work due
    ];

    let plan = generate_plan(&skills, &PlanGoal::Afqt(70), 60).expect("plan");

    // The strongest skill with nothing due must not consume time.
    assert!(
        !plan.subtests().contains(&"AR"),
        "a strong skill with no due reviews should not be scheduled, got {:?}",
        plan.subtests()
    );

    // PC has due reviews, so it should receive the largest allocation.
    let pc = plan
        .drills
        .iter()
        .find(|d| d.subtest == "PC")
        .expect("PC scheduled");
    let wk = plan
        .drills
        .iter()
        .find(|d| d.subtest == "WK")
        .expect("WK scheduled");
    assert!(
        pc.minutes >= wk.minutes,
        "due reviews should outrank pure weakness: PC={} WK={}",
        pc.minutes,
        wk.minutes
    );
    assert_eq!(pc.reason, DrillReason::DueReview);
    assert_eq!(wk.reason, DrillReason::Weakness);
}

#[test]
fn plan_never_exceeds_the_available_time() {
    let skills = vec![
        skill("AR", 0.1, 0.5, 20),
        skill("WK", 0.1, 0.5, 20),
        skill("PC", 0.1, 0.5, 20),
        skill("MK", 0.1, 0.5, 20),
    ];

    for available in [1u32, 5, 15, 30, 60, 120, 600] {
        let plan = generate_plan(&skills, &PlanGoal::Afqt(80), available).expect("plan");
        assert!(
            plan.total_minutes <= available,
            "plan allocated {} minutes from a {available}-minute budget",
            plan.total_minutes
        );
        assert!(
            plan.drills.iter().all(|d| d.minutes > 0),
            "no drill may be allocated zero minutes"
        );
    }
}

#[test]
fn plan_is_empty_when_nothing_needs_work() {
    // A learner at goal with no due reviews must be told to stop, not given
    // busy-work. This is the anti-padding property.
    let skills = vec![skill("AR", 0.95, 0.0, 0), skill("WK", 0.99, 0.0, 0)];

    let plan = generate_plan(&skills, &PlanGoal::Afqt(50), 60).expect("plan");
    assert!(
        plan.is_empty(),
        "a learner at goal with nothing due gets an empty plan, got {:?}",
        plan.drills
    );
    assert_eq!(plan.total_minutes, 0);
}

#[test]
fn uncertain_skills_attract_attention_even_at_adequate_mastery() {
    // More evidence is needed where the estimate is imprecise.
    let skills = vec![
        skill("AR", 0.5, 0.9, 0), // at target but poorly known
        skill("WK", 0.5, 0.0, 0), // at target and well known
    ];

    let plan = generate_plan(&skills, &PlanGoal::Afqt(50), 30).expect("plan");
    assert_eq!(
        plan.subtests(),
        vec!["AR"],
        "only the uncertain skill should be scheduled"
    );
    assert_eq!(plan.drills[0].reason, DrillReason::HighUncertainty);
}

#[test]
fn plan_ordering_is_deterministic_for_equal_priorities() {
    let skills = vec![
        skill("WK", 0.3, 0.2, 3),
        skill("AR", 0.3, 0.2, 3),
        skill("PC", 0.3, 0.2, 3),
    ];

    let first = generate_plan(&skills, &PlanGoal::Afqt(60), 45).expect("plan");
    let second = generate_plan(&skills, &PlanGoal::Afqt(60), 45).expect("plan");
    assert_eq!(
        first, second,
        "equal priorities must not reorder between runs"
    );
    // Ties break by name, so the order is stable and predictable.
    assert_eq!(first.subtests(), vec!["AR", "PC", "WK"]);
}

#[test]
fn every_scheduled_skill_gets_a_share_of_time() {
    // A dominant skill must not starve the others of all time.
    let skills = vec![
        skill("AR", 0.0, 0.9, 100),
        skill("WK", 0.4, 0.1, 1),
        skill("PC", 0.4, 0.1, 1),
        skill("MK", 0.4, 0.1, 1),
    ];

    let plan = generate_plan(&skills, &PlanGoal::Afqt(90), 40).expect("plan");
    assert_eq!(plan.drills.len(), 4, "all four skills need work");
    for drill in &plan.drills {
        assert!(
            drill.minutes >= 1,
            "{} was starved at {} minutes",
            drill.subtest,
            drill.minutes
        );
    }
}

#[test]
fn plan_inputs_are_validated() {
    let bad_mastery = vec![skill("AR", 1.5, 0.1, 0)];
    assert!(generate_plan(&bad_mastery, &PlanGoal::Afqt(50), 30).is_err());

    let bad_uncertainty = vec![skill("AR", 0.5, -0.2, 0)];
    assert!(generate_plan(&bad_uncertainty, &PlanGoal::Afqt(50), 30).is_err());

    let empty_name = vec![skill("  ", 0.5, 0.1, 0)];
    assert!(generate_plan(&empty_name, &PlanGoal::Afqt(50), 30).is_err());

    let ok = vec![skill("AR", 0.5, 0.1, 0)];
    assert!(
        generate_plan(&ok, &PlanGoal::Afqt(50), 0).is_err(),
        "zero available time is not a plan"
    );
}

#[test]
fn job_goal_uses_its_required_score() {
    let skills = vec![skill("AR", 0.5, 0.1, 0), skill("WK", 0.5, 0.1, 0)];

    // A high job requirement makes the same mastery look insufficient.
    let demanding = generate_plan(
        &skills,
        &PlanGoal::Job {
            name: "Cyber".into(),
            required: 90,
        },
        30,
    )
    .expect("plan");
    assert!(!demanding.is_empty(), "a demanding goal requires study");

    // A low requirement makes the same learner already ready.
    let modest = generate_plan(
        &skills,
        &PlanGoal::Job {
            name: "General".into(),
            required: 20,
        },
        30,
    )
    .expect("plan");
    assert!(modest.is_empty(), "an easily met goal needs no study");
}

#[test]
fn readiness_is_a_band_never_a_point_prediction() {
    let skills = vec![skill("AR", 0.6, 0.2, 0), skill("WK", 0.5, 0.2, 0)];

    let band = readiness_band(&skills).expect("band");

    assert!(band.low <= band.high, "band must be ordered");
    assert!(
        !band.official_score_claim,
        "ADR-010: no precise predicted official score may be claimed"
    );
    assert!(
        band.high > band.low,
        "a band with no width would be a point prediction"
    );
    assert!((0.0..=1.0).contains(&band.low));
    assert!((0.0..=1.0).contains(&band.high));
    assert!(band.confidence < 1.0, "never claim certainty");
}

#[test]
fn readiness_band_narrows_as_uncertainty_falls() {
    let vague = vec![skill("AR", 0.6, 0.9, 0), skill("WK", 0.6, 0.9, 0)];
    let precise = vec![skill("AR", 0.6, 0.01, 0), skill("WK", 0.6, 0.01, 0)];

    let wide = readiness_band(&vague).expect("band");
    let narrow = readiness_band(&precise).expect("band");

    assert!(
        (wide.high - wide.low) > (narrow.high - narrow.low),
        "more uncertainty must widen the band"
    );
    assert!(
        narrow.confidence > wide.confidence,
        "more evidence must raise confidence"
    );
}

#[test]
fn readiness_band_widens_when_subtests_disagree() {
    // Same mean mastery and same per-skill uncertainty, but one learner is
    // consistent and the other is spiky. The spiky learner is less predictable.
    let consistent = vec![skill("AR", 0.5, 0.1, 0), skill("WK", 0.5, 0.1, 0)];
    let spiky = vec![skill("AR", 0.9, 0.1, 0), skill("WK", 0.1, 0.1, 0)];

    let a = readiness_band(&consistent).expect("band");
    let b = readiness_band(&spiky).expect("band");

    assert!(
        (b.high - b.low) > (a.high - a.low),
        "disagreement between subtests must widen the band"
    );
}

#[test]
fn readiness_requires_evidence() {
    let err = readiness_band(&[]);
    assert!(
        err.is_err(),
        "readiness cannot be estimated from no evidence"
    );
}
