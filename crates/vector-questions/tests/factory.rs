//! Original-item factory: generation, determinism, and independent verification.
//!
//! Requirements: REQ-022 (original item + deterministic proof + independent
//! verification).
//!
//! The central claim under test is that a generated item can be *checked* rather
//! than believed: the factory declares an arithmetic expression, and
//! `factory::verify` re-evaluates that expression text through a separate parser
//! instead of consuming any value the factory computed. Several tests below
//! deliberately corrupt an item and assert that verification refuses it, because
//! a verifier that only ever returns `Ok` proves nothing.

use vector_questions::factory::{self, GeneratedItem, VerificationFailure};

/// Seeds exercised per subtest. Large enough that every template is drawn many
/// times, since which template runs is itself seed-dependent.
const SEEDS: u64 = 600;

fn all_items(subtest: &str) -> Vec<GeneratedItem> {
    let mut out = Vec::new();
    for seed in 0..SEEDS {
        if let Some(item) = factory::generate_one(subtest, seed) {
            out.push(item);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

#[test]
fn every_quantitative_subtest_generates_items() {
    for subtest in ["AR", "MK", "MC"] {
        let items = all_items(subtest);
        assert!(
            items.len() >= (SEEDS as usize) / 2,
            "{subtest} generated only {} of {SEEDS} seeds",
            items.len()
        );
        assert!(
            !factory::templates_for(subtest).is_empty(),
            "{subtest} must have at least one template"
        );
    }
}

#[test]
fn every_template_is_exercised_and_named_consistently() {
    for subtest in ["AR", "MK", "MC"] {
        let items = all_items(subtest);
        let used: std::collections::HashSet<&str> = items.iter().map(|i| i.template_id).collect();
        let declared = factory::templates_for(subtest);
        assert_eq!(
            used.len(),
            declared.len(),
            "{subtest}: templates declared {declared:?} but only {used:?} were produced"
        );
        for item in &items {
            assert_eq!(item.subtest, subtest, "subtest must match its template");
            assert!(
                item.objective_id.starts_with("OBJ-"),
                "every item must serve a named objective: {}",
                item.objective_id
            );
        }
    }
}

#[test]
fn every_generated_item_passes_independent_verification() {
    for subtest in ["AR", "MK", "MC"] {
        for item in all_items(subtest) {
            if let Err(failure) = factory::verify(&item) {
                panic!(
                    "{} seed={} template={} failed verification: {failure}",
                    subtest, item.seed, item.template_id
                );
            }
        }
    }
}

#[test]
fn items_have_four_distinct_options_and_named_misconceptions() {
    for subtest in ["AR", "MK", "MC"] {
        for item in all_items(subtest) {
            assert_eq!(item.options.len(), 4, "{} options", item.options.len());
            let unique: std::collections::HashSet<&String> = item.options.iter().collect();
            assert_eq!(
                unique.len(),
                4,
                "options must be distinct: {:?}",
                item.options
            );
            assert_eq!(
                item.distractor_rationales.len(),
                3,
                "each wrong option needs a rationale: {:?}",
                item.distractor_rationales
            );
            assert!(
                !item.distractor_rationales.contains_key(&item.correct_index),
                "the correct option must not carry a misconception rationale"
            );
        }
    }
}

#[test]
fn every_option_is_a_positive_integer() {
    // Truncating integer division is the failure mode that produced distractors
    // whose value did not match the misconception they claimed to represent: the
    // first symptom is usually a zero or a negative appearing among the options.
    for subtest in ["AR", "MK", "MC"] {
        for item in all_items(subtest) {
            for (index, option) in item.options.iter().enumerate() {
                let value: i128 = option.parse().unwrap_or_else(|_| {
                    panic!(
                        "{} option {index} is not an integer: {option:?}",
                        item.template_id
                    )
                });
                assert!(
                    value > 0,
                    "{} produced a non-positive option {option:?}; a distractor \
                     computed by truncating division is the usual cause",
                    item.template_id
                );
            }
            let answer: i128 = item.answer.parse().expect("answer is an integer");
            assert!(
                answer > 0,
                "{} produced a non-positive answer",
                item.template_id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Determinism: a pack must be regenerable, so the same seed must reproduce it
// ---------------------------------------------------------------------------

#[test]
fn the_same_seed_reproduces_an_identical_item() {
    for subtest in ["AR", "MK", "MC"] {
        for seed in [1_u64, 42, 999, 123_456] {
            let first = factory::generate_one(subtest, seed).expect("first");
            let second = factory::generate_one(subtest, seed).expect("second");
            assert_eq!(first, second, "{subtest} seed={seed} must be reproducible");
            assert_eq!(first.content_hash(), second.content_hash());
        }
    }
}

#[test]
fn distinct_seeds_produce_distinct_questions() {
    let items = all_items("AR");
    let hashes: std::collections::HashSet<String> =
        items.iter().map(|i| i.content_hash()).collect();
    // Some collisions are expected because templates draw from small parameter
    // ranges, but the space must be far larger than a handful of items.
    assert!(
        hashes.len() > items.len() / 2,
        "{} items produced only {} distinct questions",
        items.len(),
        hashes.len()
    );
}

#[test]
fn option_order_varies_so_the_answer_is_not_always_in_one_position() {
    for subtest in ["AR", "MK", "MC"] {
        let items = all_items(subtest);
        let positions: std::collections::HashSet<usize> =
            items.iter().map(|i| i.correct_index).collect();
        assert_eq!(
            positions.len(),
            4,
            "{subtest}: the correct option landed in only {positions:?}"
        );
    }
}

#[test]
fn a_batch_contains_no_duplicate_questions() {
    let batch = factory::generate_many("AR", 50, 20_260_922);
    assert_eq!(
        batch.len(),
        50,
        "the batch should fill to the requested size"
    );
    let hashes: std::collections::HashSet<String> =
        batch.iter().map(|i| i.content_hash()).collect();
    assert_eq!(hashes.len(), 50, "generate_many must not repeat a question");
    for item in &batch {
        factory::verify(item).expect("batched items must verify");
    }
}

#[test]
fn content_hash_ignores_the_seed_but_tracks_the_question() {
    let base = factory::generate_one("AR", 7).expect("item");
    let mut reseeded = base.clone();
    reseeded.seed = 8;
    assert_eq!(
        base.content_hash(),
        reseeded.content_hash(),
        "the seed is not part of the question"
    );

    let mut reworded = base.clone();
    reworded.stem.push_str(" (revised)");
    assert_ne!(
        base.content_hash(),
        reworded.content_hash(),
        "changing the question must change its identity"
    );
}

#[test]
fn an_unknown_subtest_generates_nothing_rather_than_guessing() {
    assert!(factory::generate_one("WK", 1).is_none());
    assert!(factory::generate_one("", 1).is_none());
    assert!(factory::templates_for("WK").is_empty());
}

// ---------------------------------------------------------------------------
// Verification must be able to fail. Each case corrupts exactly one property.
// ---------------------------------------------------------------------------

#[test]
fn verification_refuses_a_tampered_answer() {
    let mut item = factory::generate_one("AR", 11).expect("item");
    let original = item.answer.clone();
    item.answer = (original.parse::<i128>().expect("numeric") + 1).to_string();
    match factory::verify(&item) {
        Err(VerificationFailure::ProofRejected(_)) => {}
        other => panic!("a wrong answer must be rejected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_tampered_expression() {
    let mut item = factory::generate_one("MK", 12).expect("item");
    // A plausible-looking but incorrect expression must not agree with the
    // declared answer.
    item.expression = "1 + 1".to_string();
    match factory::verify(&item) {
        Err(VerificationFailure::ProofRejected(_)) => {}
        other => panic!("a wrong expression must be rejected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_duplicate_options() {
    let mut item = factory::generate_one("AR", 13).expect("item");
    // Duplicate between two *wrong* options, deliberately not involving the
    // correct one: clobbering the correct index instead can be refused by the
    // distractor-equals-answer rule first, depending on where the shuffle put
    // it, and then this rule would go untested.
    let wrongs: Vec<usize> = (0..item.options.len())
        .filter(|i| *i != item.correct_index)
        .collect();
    item.options[wrongs[1]] = item.options[wrongs[0]].clone();
    match factory::verify(&item) {
        Err(VerificationFailure::DuplicateOption(_)) => {}
        other => panic!("duplicate options must be rejected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_distractor_equal_to_the_answer() {
    let mut item = factory::generate_one("MK", 14).expect("item");
    let wrongs: Vec<usize> = (0..item.options.len())
        .filter(|i| *i != item.correct_index)
        .collect();
    // Textually different from the correct option but numerically equal. Writing
    // the answer text verbatim would also trip the duplicate-option rule, so this
    // test would then pass without the numeric rule existing at all.
    item.options[wrongs[0]] = format!(" {}", item.answer);
    match factory::verify(&item) {
        Err(VerificationFailure::DistractorEqualsAnswer { index, .. }) => {
            assert_eq!(index, wrongs[0]);
        }
        other => panic!("a distractor equal to the answer must be rejected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_correct_option_that_contradicts_the_proof() {
    let mut item = factory::generate_one("AR", 18).expect("item");
    // The expression and the declared answer still agree, so only the rule tying
    // the correct option to the proof can catch this.
    let replacement = (item.answer.parse::<i128>().expect("numeric") + 7).to_string();
    assert!(
        !item.options.contains(&replacement),
        "the replacement must not collide with an existing option"
    );
    item.options[item.correct_index] = replacement;
    match factory::verify(&item) {
        Err(VerificationFailure::CorrectOptionDisagreesWithProof { .. }) => {}
        other => panic!("a correct option contradicting the proof must be rejected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_incomplete_rationale_coverage() {
    let mut item = factory::generate_one("AR", 15).expect("item");
    let victim = *item
        .distractor_rationales
        .keys()
        .next()
        .expect("at least one distractor");
    item.distractor_rationales.remove(&victim);
    match factory::verify(&item) {
        Err(VerificationFailure::RationaleCoverage { missing, .. }) => {
            assert_eq!(missing, vec![victim]);
        }
        other => panic!("missing rationale must be detected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_an_out_of_range_correct_index() {
    let mut item = factory::generate_one("AR", 16).expect("item");
    item.correct_index = item.options.len();
    match factory::verify(&item) {
        Err(VerificationFailure::CorrectIndexOutOfRange { .. }) => {}
        other => panic!("out-of-range index must be rejected, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_blank_stem() {
    let mut item = factory::generate_one("MK", 17).expect("item");
    item.stem = "   ".to_string();
    match factory::verify(&item) {
        Err(VerificationFailure::Blank(_)) => {}
        other => panic!("a blank stem must be rejected, got {other:?}"),
    }
}

/// The verifier must not depend on the factory having produced the item.
///
/// This hand-builds an item whose arithmetic is wrong but whose declared answer
/// matches the option, which is the failure a self-validating generator would
/// miss: it would compare the option to the value it computed rather than
/// re-deriving from the expression.
#[test]
fn a_consistent_but_arithmetically_wrong_item_is_still_rejected() {
    let item = GeneratedItem {
        subtest: "AR".to_string(),
        objective_id: "OBJ-AR-RATE-01".to_string(),
        template_id: "hand.built",
        stem: "A machine makes 5 parts per minute. How many in 1 hour?".to_string(),
        options: vec![
            "300".to_string(),
            "5".to_string(),
            "60".to_string(),
            "12".to_string(),
        ],
        correct_index: 0,
        // 5 * 6 is 30, not 300, so the proof cannot support the declared answer
        // even though the option and the answer field agree with each other.
        expression: "5 * 6".to_string(),
        answer: "300".to_string(),
        distractor_rationales: [
            (1, "Used the rate alone.".to_string()),
            (2, "Used the minutes alone.".to_string()),
            (3, "Divided instead of multiplying.".to_string()),
        ]
        .into_iter()
        .collect(),
        difficulty: 0.0,
        seed: 0,
    };
    match factory::verify(&item) {
        Err(VerificationFailure::ProofRejected(_)) => {}
        other => panic!("an unprovable item must be rejected, got {other:?}"),
    }
}
