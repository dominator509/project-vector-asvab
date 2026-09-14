//! EP-004 acceptance: FSRS-compatible spaced repetition (REQ-043).

use vector_study::fsrs::{
    interval_for_retention, retrievability, CardState, Fsrs, FsrsParameters, Rating,
    MAX_INTERVAL_DAYS, MIN_INTERVAL_DAYS,
};

fn scheduler() -> Fsrs {
    Fsrs::new(FsrsParameters::default()).expect("default parameters are valid")
}

#[test]
fn first_review_sets_stability_and_difficulty_from_the_rating() {
    let fsrs = scheduler();
    let fresh = CardState::default();

    let again = fsrs.review(&fresh, Rating::Again, 0.0);
    let good = fsrs.review(&fresh, Rating::Good, 0.0);
    let easy = fsrs.review(&fresh, Rating::Easy, 0.0);

    // Higher ratings must yield higher initial stability.
    assert!(
        again.state.stability < good.state.stability,
        "Again ({}) must be below Good ({})",
        again.state.stability,
        good.state.stability
    );
    assert!(
        good.state.stability < easy.state.stability,
        "Good ({}) must be below Easy ({})",
        good.state.stability,
        easy.state.stability
    );

    // Higher ratings must yield lower difficulty (easier item).
    assert!(
        easy.state.difficulty < good.state.difficulty,
        "Easy must be less difficult than Good"
    );
    assert!(good.state.difficulty < again.state.difficulty);

    for outcome in [&again, &good, &easy] {
        assert!(outcome.state.difficulty >= 1.0 && outcome.state.difficulty <= 10.0);
        assert!(outcome.state.stability > 0.0);
        assert!(outcome.interval_days >= MIN_INTERVAL_DAYS);
    }
}

#[test]
fn successful_review_increases_stability_and_lapses_decrease_it() {
    let fsrs = scheduler();
    let start = fsrs.review(&CardState::default(), Rating::Good, 0.0);

    // On-time successful review.
    let success = fsrs.review(&start.state, Rating::Good, start.interval_days);
    assert!(
        success.state.stability > start.state.stability,
        "stability must grow on success: {} -> {}",
        start.state.stability,
        success.state.stability
    );
    assert_eq!(success.state.lapses, 0);
    assert_eq!(success.state.reps, 2);

    // A failure must reduce stability and record a lapse.
    let lapse = fsrs.review(&start.state, Rating::Again, start.interval_days);
    assert!(
        lapse.state.stability < start.state.stability,
        "stability must fall on lapse: {} -> {}",
        start.state.stability,
        lapse.state.stability
    );
    assert_eq!(lapse.state.lapses, 1, "a failure records a lapse");
}

#[test]
fn a_lapsed_card_returns_promptly_not_months_later() {
    let fsrs = scheduler();
    let mut state = CardState::default();

    // Build up a long interval, then fail the card.
    for _ in 0..6 {
        let s = fsrs.review(&state, Rating::Good, state.stability.max(1.0));
        state = s.state;
    }
    let long_interval =
        interval_for_retention(state.stability, fsrs.parameters().desired_retention);
    assert!(
        long_interval > 10.0,
        "after repeated successes the interval should be long, got {long_interval}"
    );

    let lapsed = fsrs.review(&state, Rating::Again, long_interval);
    assert_eq!(
        lapsed.interval_days, MIN_INTERVAL_DAYS,
        "a failed card must be due again immediately, not on the long interval"
    );
}

#[test]
fn more_reviews_produce_longer_intervals_for_the_same_rating() {
    // The core value of spaced repetition: intervals expand as memory
    // strengthens. If this does not hold, the scheduler is not learning.
    let fsrs = scheduler();
    let mut state = CardState::default();
    let mut previous = 0.0;

    for round in 0..8 {
        let elapsed = if round == 0 {
            0.0
        } else {
            state.stability.max(1.0)
        };
        let s = fsrs.review(&state, Rating::Good, elapsed);
        assert!(
            s.interval_days >= previous,
            "interval must not shrink on repeated success: round {round}, {previous} -> {}",
            s.interval_days
        );
        previous = s.interval_days;
        state = s.state;
    }

    assert!(
        previous > 30.0,
        "eight successful reviews should exceed a month, got {previous}"
    );
}

#[test]
fn higher_desired_retention_produces_shorter_intervals() {
    let low = FsrsParameters {
        desired_retention: 0.75,
        ..FsrsParameters::default()
    };
    let high = FsrsParameters {
        desired_retention: 0.97,
        ..FsrsParameters::default()
    };

    let low_fsrs = Fsrs::new(low).expect("valid");
    let high_fsrs = Fsrs::new(high).expect("valid");

    let mut low_state = CardState::default();
    let mut high_state = CardState::default();
    for _ in 0..4 {
        low_state = low_fsrs
            .review(&low_state, Rating::Good, low_state.stability.max(1.0))
            .state;
        high_state = high_fsrs
            .review(&high_state, Rating::Good, high_state.stability.max(1.0))
            .state;
    }

    let low_interval =
        interval_for_retention(low_state.stability, low_fsrs.parameters().desired_retention);
    let high_interval = interval_for_retention(
        high_state.stability,
        high_fsrs.parameters().desired_retention,
    );

    assert!(
        high_interval < low_interval,
        "targeting higher retention must shorten intervals: {high_interval} !< {low_interval}"
    );
}

#[test]
fn retrievability_decays_monotonically_and_matches_the_curve() {
    // R(t) = (1 + t/(9S))^-1
    let s = 10.0;
    assert!((retrievability(0.0, s) - 1.0).abs() < 1e-12, "R(0) = 1");
    assert!(
        (retrievability(9.0 * s, s) - 0.5).abs() < 1e-12,
        "R(9S) = 0.5 by construction"
    );

    let mut last = 1.1;
    for day in 0..500 {
        let r = retrievability(day as f64, s);
        assert!(r <= last, "retrievability must not increase over time");
        assert!((0.0..=1.0).contains(&r), "retrievability stays in [0,1]");
        last = r;
    }

    // Zero/negative stability has no memory: recall is impossible.
    assert_eq!(retrievability(5.0, 0.0), 0.0);
    assert_eq!(retrievability(5.0, -1.0), 0.0);
}

#[test]
fn interval_for_retention_inverts_the_forgetting_curve() {
    let s = 12.5;
    for desired in [0.5, 0.75, 0.8, 0.9, 0.95] {
        let t = interval_for_retention(s, desired);
        let r = retrievability(t, s);
        assert!(
            (r - desired).abs() < 1e-9,
            "interval for retention {desired} gives R={r}"
        );
    }
}

#[test]
fn intervals_are_clamped_to_sane_bounds() {
    assert_eq!(interval_for_retention(0.0, 0.9), MIN_INTERVAL_DAYS);
    assert_eq!(interval_for_retention(-5.0, 0.9), MIN_INTERVAL_DAYS);
    // Absurd stability must not schedule beyond the cap.
    assert_eq!(interval_for_retention(1e9, 0.99), MAX_INTERVAL_DAYS);
    assert!(interval_for_retention(1e-9, 0.9) >= MIN_INTERVAL_DAYS);
}

#[test]
fn invalid_parameters_are_rejected_at_the_boundary() {
    let bad_retention = FsrsParameters {
        desired_retention: 1.0,
        ..FsrsParameters::default()
    };
    assert!(
        Fsrs::new(bad_retention).is_err(),
        "retention 1.0 must be refused"
    );

    let zero = FsrsParameters {
        desired_retention: 0.0,
        ..FsrsParameters::default()
    };
    assert!(Fsrs::new(zero).is_err(), "retention 0.0 must be refused");

    let mut negative_weights = FsrsParameters::default().w;
    negative_weights[3] = -1.0;
    let negative = FsrsParameters {
        w: negative_weights,
        ..FsrsParameters::default()
    };
    assert!(
        Fsrs::new(negative).is_err(),
        "negative weight must be refused"
    );

    let mut nan_weights = FsrsParameters::default().w;
    nan_weights[0] = f64::NAN;
    let nan = FsrsParameters {
        w: nan_weights,
        ..FsrsParameters::default()
    };
    assert!(Fsrs::new(nan).is_err(), "NaN weight must be refused");
}

#[test]
fn scheduling_never_produces_a_non_finite_or_negative_interval() {
    // Property test: across a wide grid of states and ratings, the scheduler
    // must always emit usable numbers. A NaN interval would silently corrupt
    // every downstream due-date calculation.
    let fsrs = scheduler();
    let ratings = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy];

    for reps in [0u32, 1, 3, 10] {
        for stability in [0.5, 1.0, 5.0, 50.0, 500.0] {
            for difficulty in [1.0, 5.0, 10.0] {
                for elapsed in [0.0, 1.0, 30.0, 365.0] {
                    let state = CardState {
                        stability,
                        difficulty,
                        reps,
                        lapses: reps / 3,
                        last_rating: Some(Rating::Good),
                        elapsed_days: elapsed,
                    };
                    for rating in ratings {
                        let out = fsrs.review(&state, rating, elapsed);
                        assert!(
                            out.interval_days.is_finite() && out.interval_days >= MIN_INTERVAL_DAYS,
                            "bad interval {:?} for reps={reps} s={stability} d={difficulty} \
                             elapsed={elapsed} rating={rating:?}",
                            out.interval_days
                        );
                        assert!(
                            out.interval_days <= MAX_INTERVAL_DAYS,
                            "interval exceeded cap: {}",
                            out.interval_days
                        );
                        assert!(
                            out.state.stability.is_finite() && out.state.stability > 0.0,
                            "bad stability {}",
                            out.state.stability
                        );
                        assert!(
                            out.state.difficulty.is_finite()
                                && (1.0..=10.0).contains(&out.state.difficulty),
                            "difficulty out of range: {}",
                            out.state.difficulty
                        );
                        assert!(
                            out.retrievability.is_finite()
                                && (0.0..=1.0).contains(&out.retrievability),
                            "bad retrievability {}",
                            out.retrievability
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn scheduling_is_deterministic_for_identical_inputs() {
    // Reproducibility matters: the same persisted state and rating must always
    // schedule the same way, or evidence cannot be re-derived.
    let fsrs = scheduler();
    let state = CardState {
        stability: 7.3,
        difficulty: 4.8,
        reps: 5,
        lapses: 1,
        last_rating: Some(Rating::Hard),
        elapsed_days: 6.0,
    };

    let a = fsrs.review(&state, Rating::Good, 6.0);
    let b = fsrs.review(&state, Rating::Good, 6.0);
    assert_eq!(a, b, "identical inputs must produce identical schedules");
}

#[test]
fn parameter_round_trip_through_serde_preserves_scheduling() {
    // Persisted parameters must reload to an identical schedule.
    let original = FsrsParameters::default();
    let json = serde_json::to_string(&original).expect("serialize");
    let reloaded: FsrsParameters = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(original, reloaded);

    let a = Fsrs::new(original).expect("valid").review(
        &CardState {
            stability: 3.0,
            difficulty: 6.0,
            reps: 2,
            lapses: 0,
            last_rating: Some(Rating::Good),
            elapsed_days: 3.0,
        },
        Rating::Good,
        3.0,
    );
    let b = Fsrs::new(reloaded).expect("valid").review(
        &CardState {
            stability: 3.0,
            difficulty: 6.0,
            reps: 2,
            lapses: 0,
            last_rating: Some(Rating::Good),
            elapsed_days: 3.0,
        },
        Rating::Good,
        3.0,
    );
    assert_eq!(a, b, "reloaded parameters schedule identically");
}

#[test]
fn rating_codes_map_only_to_defined_ratings() {
    assert_eq!(Rating::from_i64(1), Some(Rating::Again));
    assert_eq!(Rating::from_i64(4), Some(Rating::Easy));
    assert_eq!(Rating::from_i64(0), None, "0 is not a valid rating");
    assert_eq!(Rating::from_i64(5), None, "5 is not a valid rating");
}
