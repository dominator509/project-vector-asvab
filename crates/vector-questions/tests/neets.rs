//! NEETS glossary ingestion: parsing, building, and source verification.
//!
//! The fixture reproduces the corpus's awkward parts, all of which were found by
//! reading the real module rather than by guessing: entries are not
//! blank-separated, definitions wrap across lines that OCR sometimes leaves blank
//! in the middle, the separator carries stray marks, and the scan confuses `c`
//! with `e` so real definitions contain `eleetron` and `eurrent`.

use vector_questions::neets::{
    parse_neets_glossary, verify, EiItem, EiVerificationFailure, Glossary,
};

const FIXTURE: &str = "\
APPENDIX A
GLOSSARY

AMMETER —An instrument for measuring the amount of electron flow in amperes.
AMPERE —The basic unit of electrical current.

ANODE —A positive electrode of an electrochemical device (such as a primary or
secondary electric cell) toward which the negative ions are drawn.

BATTERY —A device for converting chemical energy into electrical energy.

BLEEDER CURRENT —The current through a bleeder resistor.
BLEEDER RESISTOR —A resistor which is used to draw a fixed current.
BRANCH —^An individual current path in a parallel circuit.
CATHODE —The general name for any negative electrode.
CONDUCTANCE —The ability of a material to conduct or carry an electric current.
RESISTANCE —The opposition a material offers to the flow of current.
VOLTAGE —The electrical pressure that causes current to flow.
ZIGZAGGG —A definition containing a token that is not a word at all.
WRAPPED —A definition that begins here and then continues onto the next line
without any blank line in between because the scan put one there.

APPENDIX B
SOMETHING ELSE
";

fn fixture() -> Glossary {
    parse_neets_glossary(FIXTURE, "NEETS MOD 1")
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn parses_entries_including_adjacent_lines_with_no_blank_between() {
    let g = fixture();
    // `AMMETER` and `AMPERE` are on consecutive lines, so a blank-line rule would
    // have merged them into one entry.
    assert!(g.defines("AMMETER"));
    assert!(g.defines("AMPERE"));
    assert_eq!(
        g.definition_of("AMPERE"),
        Some("The basic unit of electrical current.")
    );
}

#[test]
fn a_definition_wrapping_across_lines_is_joined() {
    let g = fixture();
    let anode = g.definition_of("ANODE").expect("anode");
    assert!(
        anode.contains("secondary electric cell") && anode.contains("negative ions"),
        "the wrapped tail was lost: {anode}"
    );
    assert!(
        !anode.contains('\n'),
        "the definition should be joined into one line: {anode:?}"
    );
}

#[test]
fn a_blank_line_inside_a_definition_does_not_end_the_entry() {
    let g = fixture();
    let wrapped = g.definition_of("WRAPPED").expect("wrapped");
    assert!(
        wrapped.contains("blank line in between"),
        "the scan's stray blank line truncated the definition: {wrapped}"
    );
}

#[test]
fn ocr_marks_after_the_separator_are_trimmed() {
    let g = fixture();
    assert_eq!(
        g.definition_of("BRANCH"),
        Some("An individual current path in a parallel circuit.")
    );
}

#[test]
fn the_appendix_heading_after_the_glossary_is_not_an_entry() {
    let g = fixture();
    assert!(
        !g.defines("SOMETHING ELSE"),
        "the next section heading became a glossary entry"
    );
    // Not merely "not an entry": the heading must not be swallowed as definition
    // text either, which is what happened before the section-heading rule.
    let wrapped = g.definition_of("WRAPPED").expect("wrapped");
    assert!(
        !wrapped.contains("APPENDIX") && !wrapped.contains("SOMETHING ELSE"),
        "the next section was absorbed into the last definition: {wrapped}"
    );
    assert!(g.entry_count() >= 12, "parsed {} entries", g.entry_count());
}

#[test]
fn an_empty_source_parses_to_an_empty_glossary() {
    assert!(parse_neets_glossary("", "MOD 1").is_empty());
    assert!(parse_neets_glossary("no entries here at all\n", "MOD 1").is_empty());
}

/// Modules 3, 4, 6 and 7 were scanned with a different separator from the rest.
/// Supporting only the em dash silently discarded them: module 4 parsed to one
/// entry instead of ninety-two, and module 6 to none at all.
#[test]
fn the_spaced_hyphen_separator_is_supported() {
    let g = parse_neets_glossary(
        "ACTUATOR - The part of a switch that is acted upon to change connections.\n\
         AMMETER - A meter used to measure current.\n",
        "NEETS MOD 3",
    );
    assert_eq!(g.entry_count(), 2, "the hyphen form should parse");
    assert_eq!(
        g.definition_of("AMMETER"),
        Some("A meter used to measure current.")
    );
}

#[test]
fn a_term_containing_an_internal_hyphen_still_parses() {
    let g = parse_neets_glossary(
        "P-N JUNCTION - A boundary between p-type and n-type material.\n",
        "NEETS MOD 7",
    );
    // The internal hyphen must not be mistaken for the separator, which is why
    // only a *spaced* hyphen counts.
    assert!(
        g.defines("P-N JUNCTION"),
        "the term was split at its internal hyphen"
    );
    assert_eq!(
        g.definition_of("P-N JUNCTION"),
        Some("A boundary between p-type and n-type material.")
    );
}

#[test]
fn a_hyphen_inside_a_definition_does_not_truncate_it() {
    let g = parse_neets_glossary(
        "BIAS - A voltage applied to a device - often dc - to establish a point.\n",
        "NEETS MOD 7",
    );
    let definition = g.definition_of("BIAS").expect("bias");
    assert!(
        definition.contains("establish a point"),
        "the definition was cut at an internal hyphen: {definition}"
    );
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// Accept every definition, standing in for the caller's OCR check.
fn accept_all(_: &str) -> bool {
    true
}

#[test]
fn builds_items_whose_options_are_all_glossary_terms() {
    let g = fixture();
    let items = g.build_items(4, 1, 3, accept_all);
    assert!(!items.is_empty(), "the fixture should yield items");
    for item in &items {
        assert_eq!(item.options.len(), 4);
        for option in &item.options {
            assert!(
                g.defines(option),
                "{option:?} is not a glossary term, so it is not a real distractor"
            );
        }
        assert!(item.prompt.contains(&item.supporting_definition));
        assert_eq!(item.module, "NEETS MOD 1");
        assert_eq!(item.objective_id, "OBJ-EI-TERMINOLOGY-01");
    }
}

#[test]
fn distractors_come_from_entries_near_the_answer() {
    let g = fixture();
    let items = g.build_items(30, 3, 3, accept_all);
    assert!(!items.is_empty());
    // The mechanism, not a named term: every distractor must sit within the
    // neighbour window of the answer. Naming `BLEEDER RESISTOR` and hoping the
    // seeded draw picked its neighbour is a probabilistic test, and it failed
    // exactly that way -- 40 items drew every entry except the one under test.
    for item in &items {
        let answer = g.position_of(&item.term).expect("the answer is an entry");
        for (index, option) in item.options.iter().enumerate() {
            if index == item.correct_index {
                continue;
            }
            let position = g.position_of(option).expect("options are entries");
            let distance = position.abs_diff(answer);
            assert!(
                distance <= 6,
                "{} -> {option} is {distance} entries away, outside the neighbour window",
                item.term
            );
        }
    }
}

#[test]
fn a_distractor_is_never_a_term_conspicuously_longer_than_the_others() {
    let g = fixture();
    for item in g.build_items(20, 8, 3, accept_all) {
        for option in &item.options {
            assert!(
                option.len().abs_diff(item.term.len()) <= 10,
                "{}: option {option:?} is a length outlier beside {:?}",
                item.term,
                item.options
            );
        }
    }
}

#[test]
fn every_built_item_passes_independent_verification() {
    let g = fixture();
    for seed in 0..40 {
        for item in g.build_items(5, seed, 3, accept_all) {
            if let Err(failure) = verify(&item, &g) {
                panic!("seed {seed}: {} failed verification: {failure}", item.term);
            }
        }
    }
}

#[test]
fn a_definition_the_caller_vetoes_is_never_used() {
    let g = fixture();
    // Reject anything mentioning electrons, standing in for the OCR check.
    let items = g.build_items(6, 5, 3, |definition| !definition.contains("electron"));
    assert!(!items.is_empty());
    for item in &items {
        assert!(
            !item.supporting_definition.contains("electron"),
            "a vetoed definition was used: {}",
            item.supporting_definition
        );
    }
}

#[test]
fn definitions_outside_the_readable_length_band_are_not_used() {
    let g = fixture();
    // A minimum of 12 words excludes most of the fixture's short definitions.
    let items = g.build_items(6, 7, 12, accept_all);
    for item in &items {
        assert!(
            item.supporting_definition.split_whitespace().count() >= 12,
            "a definition below the floor was used: {}",
            item.supporting_definition
        );
    }
}

#[test]
fn the_same_seed_reproduces_the_same_items() {
    let g = fixture();
    let first = g.build_items(4, 4242, 3, accept_all);
    let second = g.build_items(4, 4242, 3, accept_all);
    assert_eq!(first, second, "ingestion must be reproducible");
}

#[test]
fn a_batch_contains_no_duplicate_questions() {
    let g = fixture();
    let items = g.build_items(8, 99, 3, accept_all);
    let hashes: std::collections::HashSet<String> =
        items.iter().map(EiItem::content_hash).collect();
    assert_eq!(hashes.len(), items.len());
}

#[test]
fn an_empty_glossary_yields_no_items_rather_than_panicking() {
    let empty = parse_neets_glossary("", "MOD 1");
    assert!(empty.build_items(5, 1, 3, accept_all).is_empty());
}

#[test]
fn the_explanation_quotes_the_source_definition() {
    let g = fixture();
    let items = g.build_items(3, 11, 3, accept_all);
    let item = items.first().expect("an item");
    let explanation = item.explanation();
    assert!(explanation.contains(&item.supporting_definition));
    assert!(explanation.contains("NEETS MOD 1"));
}

// ---------------------------------------------------------------------------
// Verification must be able to fail
// ---------------------------------------------------------------------------

#[test]
fn verification_refuses_an_option_that_is_not_a_glossary_term() {
    let g = fixture();
    let mut item = g.build_items(3, 12, 3, accept_all).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    item.options[wrong] = "NOTINTERMS".to_string();
    match verify(&item, &g) {
        Err(EiVerificationFailure::OptionIsNotAGlossaryTerm(_)) => {}
        other => panic!("expected a term refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_quoted_definition_that_is_not_the_source_definition() {
    let g = fixture();
    let mut item = g.build_items(3, 13, 3, accept_all).remove(0);
    item.supporting_definition = "Something this glossary never says.".to_string();
    match verify(&item, &g) {
        Err(EiVerificationFailure::CorrectOptionIsNotTheDefinedTerm { .. }) => {}
        other => panic!("expected a definition refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_correct_option_that_is_not_the_defined_term() {
    let g = fixture();
    let mut item = g.build_items(3, 14, 3, accept_all).remove(0);
    // Point correct_index at a distractor: the prompt still quotes the original
    // term's definition, so the item no longer answers its own question.
    item.correct_index = (item.correct_index + 1) % item.options.len();
    match verify(&item, &g) {
        Err(EiVerificationFailure::CorrectOptionIsNotTheDefinedTerm { .. }) => {}
        other => panic!("expected a mismatch refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_duplicate_options() {
    let g = fixture();
    let mut item = g.build_items(3, 15, 3, accept_all).remove(0);
    let wrongs: Vec<usize> = (0..item.options.len())
        .filter(|i| *i != item.correct_index)
        .collect();
    item.options[wrongs[1]] = item.options[wrongs[0]].clone();
    match verify(&item, &g) {
        Err(EiVerificationFailure::DuplicateOption(_)) => {}
        other => panic!("expected a duplicate refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_incomplete_rationale_coverage() {
    let g = fixture();
    let mut item = g.build_items(3, 16, 3, accept_all).remove(0);
    let victim = *item
        .distractor_rationales
        .keys()
        .next()
        .expect("a rationale");
    item.distractor_rationales.remove(&victim);
    match verify(&item, &g) {
        Err(EiVerificationFailure::RationaleCoverage { missing, .. }) => {
            assert_eq!(missing, vec![victim]);
        }
        other => panic!("expected a coverage refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_term_the_glossary_does_not_define() {
    let g = fixture();
    let mut item = g.build_items(3, 17, 3, accept_all).remove(0);
    item.term = "NOTINTERMS".to_string();
    match verify(&item, &g) {
        Err(EiVerificationFailure::UnknownTerm(_)) => {}
        other => panic!("expected an unknown-term refusal, got {other:?}"),
    }
}

/// The verifier must not depend on the builder having produced the item.
#[test]
fn a_hand_built_item_quoting_the_wrong_definition_is_rejected() {
    let g = fixture();
    let item = EiItem {
        objective_id: "OBJ-EI-TERMINOLOGY-01".to_string(),
        term: "AMMETER".to_string(),
        prompt: "Which term means: \"The basic unit of electrical current.\"?".to_string(),
        options: vec![
            "AMMETER".to_string(),
            "AMPERE".to_string(),
            "ANODE".to_string(),
            "CATHODE".to_string(),
        ],
        correct_index: 0,
        distractor_rationales: [
            (1, "different".to_string()),
            (2, "different".to_string()),
            (3, "different".to_string()),
        ]
        .into_iter()
        .collect(),
        // The definition of AMPERE, attached to AMMETER.
        supporting_definition: "The basic unit of electrical current.".to_string(),
        module: "NEETS MOD 1".to_string(),
        difficulty: 0.0,
        seed: 0,
    };
    match verify(&item, &g) {
        Err(EiVerificationFailure::CorrectOptionIsNotTheDefinedTerm { .. }) => {}
        other => panic!("an item quoting the wrong definition must be rejected, got {other:?}"),
    }
}
