//! Paragraph Comprehension ingestion: parsing, building, and source verification.
//!
//! The fixture reproduces the corpus's awkward parts, each of which was found by
//! reading real output rather than by guessing: Gutenberg markers whose bare words
//! occur throughout prose, figure labels that look like quantities, and sentences
//! that are too long to serve as options.

use vector_questions::passages::{
    parse_gutenberg, split_sentences, verify, PcItem, PcVerificationFailure, Text,
    MAX_CLAUSE_WORDS, MIN_CLAUSE_WORDS, MIN_PASSAGE_WORDS,
};

/// A miniature Gutenberg file with the header, body and licence a real one has.
const FIXTURE: &str = "\
The Project Gutenberg eBook of A Test Work

This eBook is for the use of anyone anywhere at no cost.

*** START OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

The forests for 600 miles northward of Cape Horn have a very similar aspect. They differ little from the woods that clothe the western coast. A traveller passing southward notices no abrupt change in the timber at all.

The equable humid and windy climate of Tierra del Fuego extends with only a small increase of heat. It reaches for many degrees along the west coast of the continent. The same conditions govern the islands that lie beyond the strait.

Mr. Darwin observed that the peach seldom produces fruit in Chiloe. Strawberries and apples thrive there to perfection instead. The climate corresponds in latitude with the northern parts of Spain.

A sentence that is deliberately made extremely long so that it cannot serve as an option because a learner comparing four such sentences would spend the whole sitting reading them and never once consult the passage itself. It continues at length. And it goes on further still.

As a proof of the equable climate even for 300 or 400 miles still further northward I may mention Chiloe. That island lies in latitude with the northern parts of Spain. Its gardens are watered by frequent rain throughout the year.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

This licence text must never become a passage about anything at all.
";

fn fixture() -> Text {
    parse_gutenberg(FIXTURE, "test work")
}

fn accept_all(_: &str) -> bool {
    true
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn the_licence_after_the_end_marker_is_not_prose() {
    let t = fixture();
    assert!(
        t.paragraph_count() > 0,
        "the body should parse into paragraphs"
    );
    for item in t.build_items(20, 1, accept_all) {
        assert!(
            !item.passage.contains("must never become a passage"),
            "the trailing licence became a passage"
        );
    }
}

#[test]
fn the_bare_words_start_and_end_do_not_truncate_the_text() {
    // Faraday's lectures say "at the end of the tube" on nearly every page, and
    // matching the bare phrase cut a 247 KB book to 46 paragraphs.
    let text = "*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\n\
                Heat escapes from the end of the tube and falls to the floor of the room below it.\n\n\
                *** END OF THE PROJECT GUTENBERG EBOOK X ***\n";
    let parsed = parse_gutenberg(text, "x");
    assert_eq!(
        parsed.paragraph_count(),
        1,
        "a sentence containing the word 'end of' was treated as the end marker"
    );
}

#[test]
fn an_empty_source_parses_to_an_empty_text() {
    assert!(parse_gutenberg("", "x").is_empty());
    assert!(parse_gutenberg("no markers and no prose\n", "x").is_empty());
}

#[test]
fn sentences_are_not_cut_at_abbreviations() {
    let sentences =
        split_sentences("Mr. Darwin observed the peach. It seldom produces fruit in Chiloe.");
    assert_eq!(sentences.len(), 2, "{sentences:?}");
    assert!(sentences[0].starts_with("Mr. Darwin"));
}

#[test]
fn splitting_never_loses_a_character_of_the_paragraph() {
    // The strongest statement of what the splitter owes its caller: every
    // non-whitespace character survives, in order. Sentences are rejoined by cutting
    // the paragraph rather than by pasting strings, so a lost character would be a
    // passage that differs from the work.
    let paragraph = "Penna. R.R. tunnel shields are shown (illus.), 212. \
                     \u{201c}What makes it night?\u{201d} \u{201c}Where does the wind begin?\u{201d} \
                     A. Lincoln wrote it in 1863.";
    let sentences = split_sentences(paragraph);
    let stripped = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    assert_eq!(
        stripped(&sentences.join("")),
        stripped(paragraph),
        "sentences: {sentences:?}"
    );
}

#[test]
fn a_closing_quote_stays_with_the_sentence_it_closes() {
    // The earlier splitter ended a sentence at the `?` and started the next one with
    // the closing quote, so rejoining inserted a space the source never had:
    // `night? \u{201d} \u{201c}Where` instead of `night?\u{201d} \u{201c}Where`.
    let sentences = split_sentences(
        "\u{201c}What makes it night?\u{201d} \u{201c}Where does the wind begin?\u{201d}",
    );
    assert_eq!(sentences.len(), 2, "{sentences:?}");
    assert!(sentences[0].ends_with("\u{201d}"), "{sentences:?}");
    assert!(sentences[1].starts_with('\u{201c}'), "{sentences:?}");
}

#[test]
fn a_run_of_initials_is_not_a_run_of_sentences() {
    // `R.R.` holds no sentence boundary: the terminator is not followed by a space,
    // so splitting there would cut a token in half.
    let sentences = split_sentences("Penna. R.R. tunnel shields were used.");
    assert!(
        !sentences.iter().any(|sentence| sentence == "R."),
        "an initial was treated as a sentence: {sentences:?}"
    );
    // A single-letter token before the stop is an initial as well, so `A. Lincoln`
    // does not end a sentence.
    assert_eq!(
        split_sentences("The letter was signed by A. Lincoln in person.").len(),
        1
    );
    // `Penna.` is an abbreviation this splitter has no list entry for, so it does
    // split there. That is harmless precisely because a passage is cut from the
    // paragraph rather than pasted together from sentences: whatever the splitter
    // decides, the passage remains the source's own text. What must not happen is a
    // split inside a token, which the `R.R.` assertion above checks.
    assert_eq!(
        split_sentences("Penna. R.R. tunnel shields were used.").len(),
        2
    );
}

/// A passage must be the work's own text, not a reconstruction of it.
#[test]
fn every_built_passage_occurs_in_the_source() {
    let t = fixture();
    let items = t.build_items(20, 7, accept_all);
    assert!(!items.is_empty());
    for item in &items {
        assert!(
            t.contains_passage(&item.passage),
            "the passage is not in the work: {:?}",
            item.passage
        );
    }
}

#[test]
fn a_caption_list_is_not_prose() {
    // Index entries reached a built item as a passage about air locks, with other
    // index entries as its options. A paragraph that refers to illustrations over
    // and over is a list of them.
    let text = "*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\n\
                =Shield driving=, air lock bulkhead (illus.), 210 caulking the joints \
                (illus.), 214 description of airlocks, 213 erector at work (illus.), 214 \
                erector (illus.), 210 at end of journey (illus.), 216 rear end in tunnel \
                building (illus.), 210 tunnels, front view (illus.), 209\n\n\
                *** END OF THE PROJECT GUTENBERG EBOOK X ***\n";
    assert_eq!(parse_gutenberg(text, "x").paragraph_count(), 0);
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

#[test]
fn every_option_is_a_complete_sentence_within_the_band() {
    let t = fixture();
    let items = t.build_items(20, 3, accept_all);
    assert!(!items.is_empty(), "the fixture should yield items");
    for item in &items {
        assert_eq!(item.options.len(), 4);
        for option in &item.options {
            let count = option.split_whitespace().count();
            assert!(
                (MIN_CLAUSE_WORDS..=MAX_CLAUSE_WORDS).contains(&count),
                "{count} words is outside the option band: {option:?}"
            );
            // A complete statement ends with a terminator and has balanced brackets.
            assert!(
                option.ends_with(['.', '!', '?']),
                "option is a fragment, not a statement: {option:?}"
            );
            assert_eq!(
                option.matches('(').count(),
                option.matches(')').count(),
                "unbalanced brackets in {option:?}"
            );
        }
        assert!(item.passage.split_whitespace().count() >= MIN_PASSAGE_WORDS);
        assert_eq!(item.source_label, "test work");
        assert_eq!(item.objective_id, "OBJ-PC-DETAIL-01");
    }
}

#[test]
fn a_sentence_too_long_to_compare_is_never_an_option() {
    let t = fixture();
    for item in t.build_items(20, 5, accept_all) {
        for option in &item.options {
            assert!(
                !option.starts_with("A sentence that is deliberately made extremely long"),
                "the deliberately long sentence became an option"
            );
        }
    }
}

#[test]
fn a_figure_label_is_not_treated_as_a_quantity() {
    // A number that begins with a letter labels a diagram, and altering it asks the
    // learner to compare captions rather than to read the passage.
    let text = "*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\n\
                This is represented in the diagram by the letter F14 and again by the letter G7 in the margin notes here.\n\n\
                *** END OF THE PROJECT GUTENBERG EBOOK X ***\n";
    let parsed = parse_gutenberg(text, "x");
    for item in parsed.build_items(5, 1, accept_all) {
        for option in &item.options {
            assert!(
                !option.contains("F1") && !option.contains("G7"),
                "a figure label was altered: {option:?}"
            );
        }
    }
}

#[test]
fn every_built_item_passes_independent_verification() {
    let t = fixture();
    for seed in 0..30 {
        for item in t.build_items(5, seed, accept_all) {
            if let Err(failure) = verify(&item) {
                panic!("seed {seed}: failed verification: {failure}");
            }
        }
    }
}

#[test]
fn the_same_seed_reproduces_the_same_items() {
    let t = fixture();
    assert_eq!(
        t.build_items(5, 4242, accept_all),
        t.build_items(5, 4242, accept_all)
    );
}

#[test]
fn a_definition_the_caller_vetoes_is_never_used() {
    let t = fixture();
    let items = t.build_items(10, 7, |passage| !passage.contains("Tierra del Fuego"));
    for item in &items {
        assert!(!item.passage.contains("Tierra del Fuego"));
    }
}

#[test]
fn an_empty_text_yields_no_items_rather_than_panicking() {
    let empty = parse_gutenberg("", "x");
    assert!(empty.build_items(5, 1, accept_all).is_empty());
}

#[test]
fn the_explanation_quotes_the_passage() {
    let t = fixture();
    let items = t.build_items(3, 11, accept_all);
    let item = items.first().expect("an item");
    assert!(item.explanation().contains(&item.supporting_clause));
}

// ---------------------------------------------------------------------------
// Verification must be able to fail
// ---------------------------------------------------------------------------

#[test]
fn verification_refuses_a_correct_option_the_passage_does_not_state() {
    let t = fixture();
    let mut item = t.build_items(3, 12, accept_all).remove(0);
    item.options[item.correct_index] =
        "The passage never says anything at all like this sentence.".to_string();
    match verify(&item) {
        Err(PcVerificationFailure::CorrectOptionNotInPassage(_)) => {}
        other => panic!("expected a passage refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_distractor_the_passage_also_states() {
    let t = fixture();
    let mut item = t.build_items(3, 13, accept_all).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    // Point a distractor at the correct sentence: then two options are true.
    item.options[wrong] = item.supporting_clause.clone();
    match verify(&item) {
        Err(PcVerificationFailure::DuplicateOption(_))
        | Err(PcVerificationFailure::DistractorAlsoInPassage(_)) => {}
        other => panic!("expected a duplicate or passage refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_distractor_whose_altered_token_is_in_the_passage() {
    let t = fixture();
    let mut item = t.build_items(3, 14, accept_all).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    // Replace the altered quantity with one the passage states elsewhere, so the
    // passage does not rule the option out.
    let option = item.options[wrong].clone();
    let altered = option
        .split_whitespace()
        .find(|word| word.chars().any(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .to_string();
    if altered.is_empty() {
        return;
    }
    // Substitute a quantity the passage itself states, so the passage no longer
    // rules the option out and only the token rule can refuse it. The substitute
    // must not happen to reproduce another option, which would be a different
    // failure and would let this test pass without exercising the token rule.
    let elsewhere = item
        .passage
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_digit()))
        .find(|word| {
            !word.is_empty()
                && *word != altered.as_str()
                && !item.options.contains(&option.replace(&altered, word))
        })
        .expect("the fixture states a quantity other than the altered one")
        .to_string();
    item.options[wrong] = option.replace(&altered, &elsewhere);
    match verify(&item) {
        Err(PcVerificationFailure::DistractorTokenPresent { .. }) => {}
        other => panic!("expected a token refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_an_option_outside_the_length_band() {
    let t = fixture();
    let mut item = t.build_items(3, 15, accept_all).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    item.options[wrong] = "Too short.".to_string();
    match verify(&item) {
        Err(PcVerificationFailure::ClauseLength { .. }) => {}
        other => panic!("expected a length refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_passage_outside_the_length_band() {
    let t = fixture();
    let mut item = t.build_items(3, 16, accept_all).remove(0);
    item.passage = "Far too short a passage.".to_string();
    match verify(&item) {
        Err(PcVerificationFailure::PassageLength { .. }) => {}
        other => panic!("expected a passage-length refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_incomplete_rationale_coverage() {
    let t = fixture();
    let mut item = t.build_items(3, 17, accept_all).remove(0);
    let victim = *item
        .distractor_rationales
        .keys()
        .next()
        .expect("a rationale");
    item.distractor_rationales.remove(&victim);
    match verify(&item) {
        Err(PcVerificationFailure::RationaleCoverage { missing, .. }) => {
            assert_eq!(missing, vec![victim]);
        }
        other => panic!("expected a coverage refusal, got {other:?}"),
    }
}

/// The verifier must not depend on the builder having produced the item.
#[test]
fn a_hand_built_item_with_an_unsupported_answer_is_rejected() {
    let t = fixture();
    let item = PcItem {
        objective_id: "OBJ-PC-DETAIL-01".to_string(),
        passage: t
            .build_items(1, 18, accept_all)
            .first()
            .map(|built| built.passage.clone())
            .unwrap_or_default(),
        prompt: "According to the passage, which of the following is stated?".to_string(),
        options: vec![
            "The passage does not contain this statement anywhere at all.".to_string(),
            "Nor does it contain this alternative statement about anything.".to_string(),
            "This third statement is likewise absent from the text entirely.".to_string(),
            "And this fourth one is absent from the passage as well.".to_string(),
        ],
        correct_index: 0,
        distractor_rationales: [
            (1, "not stated".to_string()),
            (2, "not stated".to_string()),
            (3, "not stated".to_string()),
        ]
        .into_iter()
        .collect(),
        supporting_clause: "The passage does not contain this statement anywhere at all."
            .to_string(),
        source_label: "test work".to_string(),
        difficulty: 0.0,
        seed: 0,
    };
    match verify(&item) {
        Err(PcVerificationFailure::CorrectOptionNotInPassage(_)) => {}
        other => panic!("an unsupported answer must be rejected, got {other:?}"),
    }
}
