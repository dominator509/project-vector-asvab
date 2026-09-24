//! Paragraph Comprehension ingestion: parsing, building, and source verification.
//!
//! The fixture reproduces the corpus's awkward parts, each of which was found by
//! reading real output rather than by guessing: Gutenberg markers whose bare words
//! occur throughout prose, figure labels that look like quantities, and sentences
//! that are too long to serve as options.

use vector_questions::dictionary::{parse_webster, Dictionary};
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

The glaciers of the southern Andes descend almost to the water in many places along this coast. These glaciers are fed by the same heavy snow that falls throughout the long winter months. A glacier advancing slowly into the fjord breaks off at last in great masses of floating ice.

Volcanoes are numerous along the whole length of this chain of mountains. Several volcanoes were seen in eruption during the survey of the neighbouring coast. The sailors reported that the volcanoes threw out fine ash for many days together.

The harbours of the strait afford excellent shelter for shipping of every size. These harbours are deep enough for the largest vessels to anchor close inshore. A harbour on the eastern side is formed by a natural breakwater of rock.

The tribes of this region build canoes of bark and of plank. A canoe is paddled by the women while the men manage the fire in the middle of the craft. The canoes are often kept afloat in weather that would swamp a larger boat.

*** END OF THE PROJECT GUTENBERG EBOOK A TEST WORK ***

This licence text must never become a passage about anything at all.
";

fn fixture() -> Text {
    parse_gutenberg(FIXTURE, "test work")
}

/// A miniature Webster's file, with entries for words the fixture passages use.
///
/// The vocabulary builder needs a dictionary; this supplies real headwords and
/// `Defn:` bodies for enough words in the fixture prose that classes of items can be
/// built. Each entry's first `Defn:` body is what `sense_gloss` reads.
const DICTIONARY_FIXTURE: &str = "\
The Project Gutenberg eBook of Webster's Unabridged Dictionary

*** START OF THE PROJECT GUTENBERG EBOOK ***

Forest

Forest (n.) Defn: A large tract of land covered with trees.

Climate

Climate (n.) Defn: The habitual weather conditions of a region.

Timber

Timber (n.) Defn: Wood prepared for building or for use in carpentry.

Glacier

Glacier (n.) Defn: A mass of ice moving slowly down a slope or valley.

Volcano

Volcano (n.) Defn: A mountain that vents molten rock and ash.

Harbour

Harbour (n.) Defn: A sheltered place where ships may lie at anchor.

Canoe

Canoe (n.) Defn: A light narrow boat moved by paddles.

Vessel

Vessel (n.) Defn: A hollow container for holding liquids; a ship.

Mountain

Mountain (n.) Defn: A large natural elevation of the earth's surface.

Shelter

Shelter (n.) Defn: A structure that protects from weather or danger.

Snow

Snow (n.) Defn: Water frozen in light white flakes and falling from the sky.

Island

Island (n.) Defn: A piece of land completely surrounded by water.

Weather

Weather (n.) Defn: The state of the atmosphere at a place and time.

*** END OF THE PROJECT GUTENBERG EBOOK ***

Licence text that must not become a definition.
";

fn dictionary() -> Dictionary {
    parse_webster(DICTIONARY_FIXTURE)
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
    let t = fixture().with_dictionary(dictionary());
    let items = t.build_items(20, 3, accept_all);
    assert!(!items.is_empty(), "the fixture should yield items");
    for item in &items {
        assert_eq!(item.options.len(), 4);
        // The clause band and the balanced-bracket rule are DETAIL rules. Each kind
        // is held to its own shape: DETAIL options are complete short sentences,
        // MAIN_IDEA options are complete sentences, VOCAB options are meaning phrases.
        match item.objective_id.as_str() {
            "OBJ-PC-DETAIL-01" => {
                for option in &item.options {
                    let count = option.split_whitespace().count();
                    assert!(
                        (MIN_CLAUSE_WORDS..=MAX_CLAUSE_WORDS).contains(&count),
                        "{count} words is outside the option band: {option:?}"
                    );
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
            }
            "OBJ-PC-MAINIDEA-01" => {
                // A main-idea option is a full sentence, never a bare topic word.
                for option in &item.options {
                    assert!(
                        option.split_whitespace().count() >= MIN_CLAUSE_WORDS,
                        "a main-idea option is a fragment, not a sentence: {option:?}"
                    );
                    assert!(
                        option.ends_with(['.', '!', '?']),
                        "a main-idea option is not a sentence: {option:?}"
                    );
                }
            }
            "OBJ-PC-VOCAB-01" => {
                // A vocabulary option is a meaning phrase, and the prompt quotes the
                // context so the learner judges the word as used.
                assert!(
                    item.prompt.contains('"'),
                    "a vocabulary prompt must quote the context: {}",
                    item.prompt
                );
                for option in &item.options {
                    assert!(!option.trim().is_empty(), "a meaning cannot be blank");
                }
            }
            other => panic!("unexpected objective: {other}"),
        }
        assert!(item.passage.split_whitespace().count() >= MIN_PASSAGE_WORDS);
        assert_eq!(item.source_label, "test work");
        assert!(
            Text::KINDS.contains(&item.objective_id.as_str()),
            "unexpected objective: {}",
            item.objective_id
        );
    }
}

#[test]
fn all_three_kinds_are_produced_and_each_verifies() {
    // The builder used to stamp one stem on every PC item. This pins that the three
    // supported kinds actually appear, and that one seed's worth of items covers
    // more than a single objective -- a regression to the old single-kind build
    // would otherwise pass every other test here.
    let t = fixture().with_dictionary(dictionary());
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for seed in 0..40 {
        for item in t.build_items(12, seed, accept_all) {
            seen.insert(item.objective_id.clone());
            verify(&item).unwrap_or_else(|e| panic!("seed {seed} kind {}: {e}", item.objective_id));
        }
    }
    for kind in Text::KINDS {
        assert!(
            seen.contains(kind),
            "kind {kind} was never produced; saw {seen:?}"
        );
    }
}

#[test]
fn vocabulary_items_are_built_only_when_a_dictionary_is_supplied() {
    // Without a dictionary the vocabulary builder returns None rather than falling
    // back to word-spotting, so a corpus built without one has no VOCAB items at
    // all. That is an honest gap, and this pins it.
    let t = fixture();
    let mut saw_vocab = false;
    for seed in 0..40 {
        for item in t.build_items(12, seed, accept_all) {
            if item.objective_id == "OBJ-PC-VOCAB-01" {
                saw_vocab = true;
            }
        }
    }
    assert!(
        !saw_vocab,
        "vocabulary items were built without a dictionary"
    );
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
    let t = fixture().with_dictionary(dictionary());
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

/// One DETAIL item from whichever seed yields one first.
///
/// The builder emits three kinds now; the negative tests below are all about the
/// detail rules (clause band, altered token, passage length), so they need a detail
/// item specifically rather than whatever `remove(0)` happens to draw.
fn detail_item(t: &Text) -> PcItem {
    for seed in 0..60 {
        if let Some(item) = t
            .build_items(12, seed, accept_all)
            .into_iter()
            .find(|item| item.objective_id == "OBJ-PC-DETAIL-01")
        {
            return item;
        }
    }
    panic!("the fixture yields no detail item");
}

#[test]
fn verification_refuses_a_correct_option_the_passage_does_not_state() {
    let t = fixture();
    let mut item = detail_item(&t);
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
    let mut item = detail_item(&t);
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
    let mut item = detail_item(&t);
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
    let mut item = detail_item(&t);
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
    let mut item = detail_item(&t);
    item.passage = "Far too short a passage.".to_string();
    match verify(&item) {
        Err(PcVerificationFailure::PassageLength { .. }) => {}
        other => panic!("expected a passage-length refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_incomplete_rationale_coverage() {
    let t = fixture();
    let mut item = detail_item(&t);
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
    // Take the passage from the source directly rather than from `build_items`: the
    // builder now emits three kinds, and a main-idea or vocabulary item's passage
    // may be shorter than the detail band this hand-built detail item must satisfy.
    let passage = t
        .paragraphs()
        .iter()
        .find(|p| (MIN_PASSAGE_WORDS..=220).contains(&p.split_whitespace().count()))
        .expect("the fixture has a passage in band")
        .clone();
    let item = PcItem {
        objective_id: "OBJ-PC-DETAIL-01".to_string(),
        passage,
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

/// A main-idea item whose "topic" the passage does not actually repeat must be
/// refused: an item cannot claim a passage is about something the text mentions
/// once.
#[test]
fn verification_refuses_a_main_idea_topic_the_passage_does_not_repeat() {
    let t = fixture();
    let mut item = None;
    for seed in 0..40 {
        if let Some(found) = t
            .build_items(12, seed, accept_all)
            .into_iter()
            .find(|i| i.objective_id == "OBJ-PC-MAINIDEA-01")
        {
            item = Some(found);
            break;
        }
    }
    let mut item = item.expect("the fixture yields a main-idea item");
    // Swap the answer for a word the passage contains exactly once, if any: the
    // repetition rule is what must catch it, not mere presence.
    let once = {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for word in item.passage.split_whitespace() {
            let cleaned: String = word
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_lowercase();
            if cleaned.len() >= 5 {
                *counts.entry(cleaned).or_insert(0) += 1;
            }
        }
        counts
            .into_iter()
            .find(|(_, count)| *count == 1)
            .map(|(word, _)| word)
    };
    let Some(once) = once else {
        return; // fixture has no singleton content word; nothing to exercise here
    };
    item.options[item.correct_index] = once.clone();
    item.supporting_clause = once.clone();
    match verify(&item) {
        Err(PcVerificationFailure::TopicNotRepeated { .. }) => {}
        other => panic!("expected a repetition refusal, got {other:?}"),
    }
}

/// One MAIN_IDEA item from whichever seed yields one first.
fn main_idea_item(t: &Text) -> PcItem {
    for seed in 0..60 {
        if let Some(item) = t
            .build_items(12, seed, accept_all)
            .into_iter()
            .find(|item| item.objective_id == "OBJ-PC-MAINIDEA-01")
        {
            return item;
        }
    }
    panic!("the fixture yields no main-idea item");
}

/// One VOCAB_IN_CONTEXT item from whichever seed yields one first.
fn vocab_item(t: &Text) -> PcItem {
    for seed in 0..60 {
        if let Some(item) = t
            .build_items(12, seed, accept_all)
            .into_iter()
            .find(|item| item.objective_id == "OBJ-PC-VOCAB-01")
        {
            return item;
        }
    }
    panic!("the fixture yields no vocabulary item");
}

/// Review point 2: a main-idea answer that is just the passage's own sentence is a
/// detail restatement, and verification must refuse it.
#[test]
fn verification_refuses_a_main_idea_answer_that_is_passage_text() {
    let t = fixture().with_dictionary(dictionary());
    let mut item = main_idea_item(&t);
    // Point the answer at a sentence the passage contains verbatim.
    let sentence = t
        .build_items(12, 0, accept_all)
        .into_iter()
        .find(|i| i.objective_id == "OBJ-PC-DETAIL-01")
        .map(|i| i.supporting_clause)
        .expect("the fixture yields a detail clause");
    item.options[item.correct_index] = sentence;
    match verify(&item) {
        Err(PcVerificationFailure::MainIdeaOptionIsPassageText(_))
        | Err(PcVerificationFailure::CorrectOptionNotInPassage(_)) => {}
        other => panic!("expected a passage-text refusal, got {other:?}"),
    }
}

/// Review point 2: a main-idea option that is not a full sentence must be refused.
#[test]
fn verification_refuses_a_main_idea_option_that_is_not_a_sentence() {
    let t = fixture().with_dictionary(dictionary());
    let mut item = main_idea_item(&t);
    let wrong = (item.correct_index + 1) % item.options.len();
    item.options[wrong] = "glaciers".to_string();
    match verify(&item) {
        Err(PcVerificationFailure::MainIdeaOptionNotASentence(_)) => {}
        other => panic!("expected a sentence refusal, got {other:?}"),
    }
}

/// Review point 1: a vocabulary distractor that is a bare word from the passage is
/// word-spotting, not a meaning, and must be refused.
#[test]
fn verification_refuses_a_vocabulary_distractor_that_is_a_passage_word() {
    let t = fixture().with_dictionary(dictionary());
    let mut item = vocab_item(&t);
    let wrong = (item.correct_index + 1) % item.options.len();
    // A single word lifted from the passage is exactly the old word-spotting option.
    let word = item
        .passage
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase())
        .find(|w| w.len() >= 5)
        .expect("the passage has a content word");
    item.options[wrong] = word;
    match verify(&item) {
        Err(PcVerificationFailure::VocabOptionAbsentFromPassage(_)) => {}
        other => panic!("expected a word-spotting refusal, got {other:?}"),
    }
}

/// Review point 1: the target word must really occur in the quoted context, or the
/// item asks about a word the sentence does not use.
#[test]
fn verification_refuses_a_vocabulary_target_absent_from_its_context() {
    let t = fixture().with_dictionary(dictionary());
    let mut item = vocab_item(&t);
    // Replace the quoted target with a word the context sentence does not contain.
    let forged = "In the sentence \"The harbours are deep enough for the largest vessels.\" \
                  the word \"azimuth\" most nearly means:"
        .to_string();
    item.prompt = forged;
    match verify(&item) {
        Err(PcVerificationFailure::VocabOptionAbsentFromPassage(_)) => {}
        other => panic!("expected an absent-target refusal, got {other:?}"),
    }
}

/// Review point 3: difficulty is derived from the item, so a value outside the
/// probability range must be refused rather than stored.
#[test]
fn verification_refuses_a_difficulty_outside_the_probability_range() {
    let t = fixture().with_dictionary(dictionary());
    let mut item = detail_item(&t);
    item.difficulty = 1.5;
    match verify(&item) {
        Err(PcVerificationFailure::DifficultyOutOfRange { .. }) => {}
        other => panic!("expected a difficulty refusal, got {other:?}"),
    }
}

/// Review point 3: difficulty is derived from the item, not stamped by kind. Two
/// passages of very different length must not produce the same stored value, which a
/// hard-coded constant would.
#[test]
fn difficulty_is_derived_from_the_item_not_stamped() {
    // A short passage and a long one, both in band, each with dictionary words so
    // all kinds can be built. If difficulty were a constant per kind, the two would
    // agree; if it is derived, the longer passage scores higher.
    let short = build_text_with(&[
        "The island climate governs the weather of the coast. The island snow falls in \
         winter. These island forests shelter the island vessels.",
    ]);
    let long = build_text_with(&[
        "The island climate governs the weather of the coast in every season of the \
         year. The island snow falls in winter and covers the island forests. The \
         island forests shelter the island vessels from the weather. The island \
         harbour holds the island vessels through the storm. Snow and weather together \
         shape the island climate that the island forests depend upon entirely.",
    ]);
    let sd = mean_difficulty(&short);
    let ld = mean_difficulty(&long);
    assert!(
        ld > sd,
        "the longer passage did not score harder: short={sd}, long={ld}"
    );
}

/// Difficulty of every item a text builds, averaged, so the assertion above is a
/// statement about the derivation rather than about one draw.
fn mean_difficulty(text: &Text) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for seed in 0..20 {
        for item in text.build_items(6, seed, accept_all) {
            sum += item.difficulty;
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

/// A `Text` built from inline paragraphs, with the dictionary attached.
fn build_text_with(paragraphs: &[&str]) -> Text {
    let body = paragraphs.join("\n\n");
    let file = format!(
        "*** START OF THE PROJECT GUTENBERG EBOOK X ***\n\n{body}\n\n\
         *** END OF THE PROJECT GUTENBERG EBOOK X ***\n"
    );
    parse_gutenberg(&file, "synthetic").with_dictionary(dictionary())
}
