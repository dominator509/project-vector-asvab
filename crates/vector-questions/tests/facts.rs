//! Question-and-answer ingestion: parsing, building, and source verification.
//!
//! The fixture reproduces the corpus's awkward parts, each found by reading real
//! output: a table of contents that repeats every question, a licence block after
//! the end marker, answers that run over several paragraphs, and answers whose first
//! sentence is too long to serve as an option.

use vector_questions::facts::{
    parse_faq, verify, FactItem, FactVerificationFailure, Faq, MAX_OPTION_WORDS, MIN_OPTION_WORDS,
};

/// A miniature question-and-answer work in the shape the corpus uses.
const FIXTURE: &str = "\
The Project Gutenberg eBook of A Test Work

This eBook is for the use of anyone anywhere at no cost.

*** START OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

CONTENTS

Why Do We Count in Tens?
How Does a Camera Take a Picture?
Why Does a Pencil Write?
What Makes the Wind Blow?

Why Do We Count in Tens?
When man found it necessary to count, the only implements at hand were his fingers
and his toes. He had ten of each, so he counted in tens and has done so ever since.

How Does a Camera Take a Picture?
A lens gathers the light from the scene and bends it to a focus on a surface that
records it. The size of the opening decides how much light arrives.

Why Does a Pencil Write?
The lead of a pencil is a soft form of carbon that rubs off on the paper. The
roughness of the paper pulls the particles away from the point.

What Makes the Wind Blow?
Air moves from where the pressure is high to where the pressure is low. The sun
heats some places more than others, which is what starts the difference.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

This licence text must never become a question about anything at all.
";

fn fixture() -> Faq {
    parse_faq(FIXTURE, "A Test Work")
}

fn accept_all(_: &FactItem) -> bool {
    true
}

fn build(count: usize, seed: u64) -> Vec<FactItem> {
    fixture().build_items(
        "GS",
        count,
        seed,
        MIN_OPTION_WORDS,
        MAX_OPTION_WORDS,
        accept_all,
    )
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn the_licence_after_the_end_marker_is_not_a_question() {
    let faq = fixture();
    assert!(
        faq.entry_count() > 0,
        "the body should parse into questions"
    );
    for entry in faq.entries() {
        assert!(
            !entry.question.contains("licence"),
            "the trailing licence became a question: {:?}",
            entry.question
        );
        assert!(
            !entry.full_answer.contains("must never become"),
            "the licence became an answer"
        );
    }
}

#[test]
fn a_question_repeated_in_the_contents_is_asked_once() {
    let faq = fixture();
    // The fixture lists every question in a contents block before asking it. The
    // contents copy has no answer of its own, so it must not become an entry.
    for entry in faq.entries() {
        assert!(
            !entry.answer.is_empty(),
            "every entry must carry the answer that follows it: {entry:?}"
        );
    }
    assert_eq!(faq.entry_count(), 4, "{:?}", faq.entries());
}

#[test]
fn an_answer_is_the_first_sentence_of_what_follows_the_question() {
    let faq = fixture();
    let entry = faq
        .entries()
        .iter()
        .find(|entry| entry.question.starts_with("Why Do We Count"))
        .expect("the counting question is present");
    assert_eq!(
        entry.answer,
        "When man found it necessary to count, the only implements at hand were his \
         fingers and his toes."
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    );
    // The whole answer is kept, so a reviewer can read the context an option was
    // taken from.
    assert!(entry.full_answer.len() > entry.answer.len());
}

#[test]
fn a_line_that_merely_ends_in_a_question_mark_is_not_a_question() {
    let faq = parse_faq(
        "*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\n\
         Was it not splendid?\n\n\
         Why Is the Sky Blue?\n\
         Because the air scatters the shorter wavelengths of sunlight far more than \
         the longer ones, and our eyes read that scattered light as blue.\n\n\
         *** END OF THE PROJECT GUTENBERG EBOOK X ***\n",
        "x",
    );
    assert_eq!(faq.entry_count(), 1, "{:?}", faq.entries());
    assert!(faq.entries()[0].question.starts_with("Why Is"));
}

#[test]
fn an_empty_source_parses_to_an_empty_faq() {
    assert!(parse_faq("", "x").is_empty());
    assert!(parse_faq("prose with no questions at all\n", "x").is_empty());
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

#[test]
fn every_option_is_an_answer_the_source_gives() {
    let items = build(6, 20_260_922);
    assert!(!items.is_empty(), "the fixture should yield items");
    let faq = fixture();
    for item in &items {
        assert_eq!(item.options.len(), 4);
        for option in &item.options {
            assert!(
                faq.question_for(option).is_some(),
                "option {option:?} is not an answer in the source"
            );
        }
    }
}

#[test]
fn a_distractor_is_an_answer_to_a_different_question() {
    let items = build(6, 7);
    let faq = fixture();
    for item in &items {
        for (index, option) in item.options.iter().enumerate() {
            if index == item.correct_index {
                continue;
            }
            let entry = faq.question_for(option).expect("an answer in the source");
            assert_ne!(
                entry.question, item.prompt,
                "a distractor must not answer the question that was asked"
            );
            let rationale = item
                .distractor_rationales
                .get(&index)
                .expect("every distractor carries a rationale");
            assert!(
                rationale.contains(&entry.question),
                "the rationale should name the question this answers: {rationale}"
            );
        }
    }
}

#[test]
fn every_option_fits_the_comparable_band() {
    for item in build(6, 3) {
        for option in &item.options {
            let words = option.split_whitespace().count();
            assert!(
                (MIN_OPTION_WORDS..=MAX_OPTION_WORDS).contains(&words),
                "option has {words} words: {option:?}"
            );
        }
    }
}

#[test]
fn the_same_seed_reproduces_the_same_items() {
    let first = build(6, 42);
    let second = build(6, 42);
    assert_eq!(first, second);
}

#[test]
fn the_caller_can_veto_a_built_item() {
    // The application layer vetoes items whose answer contains an OCR misreading,
    // so the closure has to be able to remove one.
    let faq = fixture();
    let all = faq.build_items("GS", 4, 1, MIN_OPTION_WORDS, MAX_OPTION_WORDS, accept_all);
    assert!(!all.is_empty());
    let none = faq.build_items("GS", 4, 1, MIN_OPTION_WORDS, MAX_OPTION_WORDS, |_| false);
    assert!(none.is_empty(), "a vetoing caller must receive nothing");
}

#[test]
fn a_source_with_too_few_answers_yields_nothing_rather_than_a_broken_item() {
    let thin = parse_faq(
        "*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\n\
         Why Is the Sky Blue?\n\
         Because the air scatters the shorter wavelengths of sunlight far more than \
         the longer ones, and our eyes read that scattered light as blue.\n\n\
         *** END OF THE PROJECT GUTENBERG EBOOK X ***\n",
        "x",
    );
    assert!(
        thin.build_items("GS", 4, 1, MIN_OPTION_WORDS, MAX_OPTION_WORDS, accept_all)
            .is_empty(),
        "four options are impossible from one answer"
    );
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

#[test]
fn every_built_item_passes_independent_verification() {
    for item in build(8, 11) {
        verify(&item, &fixture()).unwrap_or_else(|failure| {
            panic!("item {:?} failed verification: {failure}", item.prompt)
        });
    }
}

#[test]
fn verification_refuses_a_correct_option_the_source_does_not_give() {
    let mut item = build(1, 5).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    item.options[item.correct_index] = item.options[wrong].clone();
    match verify(&item, &fixture()) {
        Err(FactVerificationFailure::AnswerNotFromSource(_))
        | Err(FactVerificationFailure::DuplicateOption(_)) => {}
        other => panic!("expected an answer-from-source refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_distractor_the_source_does_not_contain() {
    let mut item = build(1, 13).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    item.options[wrong] = "A statement this book never makes about anything.".to_string();
    match verify(&item, &fixture()) {
        Err(FactVerificationFailure::DistractorNotFromSource(_)) => {}
        other => panic!("expected a distractor-from-source refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_prompt_that_is_not_a_question() {
    let mut item = build(1, 17).remove(0);
    item.prompt = "A statement rather than a question.".to_string();
    match verify(&item, &fixture()) {
        Err(FactVerificationFailure::NotAQuestion(_)) => {}
        other => panic!("expected a not-a-question refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_a_distractor_carrying_no_rationale() {
    let mut item = build(1, 19).remove(0);
    let wrong = (item.correct_index + 1) % item.options.len();
    item.distractor_rationales.remove(&wrong);
    match verify(&item, &fixture()) {
        Err(FactVerificationFailure::MissingRationale(index)) if index == wrong => {}
        other => panic!("expected a missing-rationale refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_an_index_that_addresses_no_option() {
    let mut item = build(1, 23).remove(0);
    item.correct_index = item.options.len();
    match verify(&item, &fixture()) {
        Err(FactVerificationFailure::CorrectIndexOutOfRange { .. }) => {}
        other => panic!("expected an index refusal, got {other:?}"),
    }
}
