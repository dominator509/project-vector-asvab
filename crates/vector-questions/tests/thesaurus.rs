//! Word Knowledge ingestion: parsing a public-domain thesaurus, building items,
//! and the source-ground-truth verification that keeps them honest.
//!
//! The fixture below is a handful of lines written for these tests. It stands in
//! for the real source's *shape* — one root word, then its associated terms — and
//! deliberately contains the awkward cases: a phrase headword, a proper noun, an
//! overlapping pair whose lines share a word, and a root with too few terms to
//! fill four options.

use vector_questions::thesaurus::{parse_moby, verify, Thesaurus, WkItem, WkVerificationFailure};

/// A miniature thesaurus. `brave` and its near neighbours list each other, which
/// is what the builder's mutuality filter requires; `timid` and `lucid` have
/// partners without lines of their own, so they cannot produce items; `zigzag` is
/// a plain word with no mutual partners and `Zulu` is a proper noun.
const FIXTURE: &str = "\
brave,bold,courageous,valiant,dauntless,gallant,heroic,stout,plucky
bold,daring,audacious,brave,courageous,valiant,gallant,intrepid,stouthearted
courageous,brave,bold,valiant,dauntless,heroic,gallant,stouthearted,plucky
valiant,brave,bold,courageous,dauntless,heroic,gallant,stout,plucky
dauntless,brave,bold,courageous,valiant,heroic,gallant,intrepid,plucky
timid,shy,bashful,cowardly,fearful,mousy,retiring,skittish
cowardly,timid,craven,pusillanimous,fearful,dastardly,spineless,recreant
craven,cowardly,timid,pusillanimous,dastardly,spineless,recreant,abject
lucid,clear,pellucid,transparent,limpid,intelligible,perspicuous,coherent
clear,lucid,limpid,transparent,obvious,evident,plain,patent
zigzag,crooked,serpentine,sinuous,tortuous,meandering,devious
Zulu,African,Bantu,warrior,impis,assegai,regiment,clan
sparse,thin,meagre
opaque,cloudy,murky,dense,thick,unclear,vague,obscure
";

fn fixture() -> Thesaurus {
    parse_moby(FIXTURE)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn parses_root_words_and_their_terms() {
    let t = fixture();
    assert_eq!(t.root_count(), 14, "one entry per non-empty line");
    assert!(
        t.vocabulary_size() > 40,
        "vocabulary {}",
        t.vocabulary_size()
    );

    let related = t.related_to("brave").expect("brave is a root word");
    assert_eq!(related[0], "bold");
    assert!(related.contains(&"valiant".to_string()));
}

#[test]
fn parsing_is_case_insensitive_for_lookup() {
    let t = fixture();
    assert!(t.related_to("BRAVE").is_some());
    assert!(t.related_to("  brave  ").is_some());
    assert!(t.co_occurs("Brave", "bold"));
}

#[test]
fn an_empty_or_malformed_source_parses_to_an_empty_thesaurus() {
    assert!(parse_moby("").is_empty());
    assert!(parse_moby("\n\n   \n").is_empty());
    // Root word with no terms is not an entry.
    assert!(parse_moby("lonely\n").is_empty());
    // Comments are skipped.
    assert!(
        parse_moby("# a note\nbrave,bold,valiant,heroic,stout,plucky,gallant\n").root_count() == 1
    );
}

#[test]
fn co_occurrence_is_per_line_and_symmetric() {
    let t = fixture();
    assert!(t.co_occurs("brave", "bold"));
    assert!(t.co_occurs("bold", "brave"), "the check must be symmetric");

    // Co-occurrence is per line, not transitive. `bashful` shares a line with
    // `timid`, and `timid` shares one with `craven`, but `bashful` and `craven`
    // never appear together -- so the source has not said those two belong
    // together, and an item pairing them would be asserting more than the source
    // supports.
    assert!(t.co_occurs("bashful", "timid"));
    assert!(t.co_occurs("timid", "craven"));
    assert!(!t.co_occurs("bashful", "craven"));

    assert!(!t.co_occurs("brave", "shy"));
    assert!(!t.co_occurs("brave", "not-a-word"));
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

#[test]
fn builds_items_whose_options_are_plain_words() {
    let t = fixture();
    let items = t.build_items_configured(6, 1, 1, |_, _| true);
    assert!(!items.is_empty(), "the fixture should yield items");
    for item in &items {
        assert_eq!(item.options.len(), 4);
        for option in &item.options {
            assert!(
                option.chars().all(|c| c.is_ascii_alphabetic()),
                "{option:?} is not a plain word"
            );
        }
        assert!(item.prompt.contains(&item.headword.to_uppercase()));
        assert_eq!(item.objective_id, "OBJ-WK-SYNONYM-01");
    }
}

#[test]
fn never_uses_a_phrase_or_proper_noun_as_a_headword() {
    let t = fixture();
    for _ in 0..12 {
        for item in t.build_items_configured(8, 7, 1, |_, _| true) {
            assert_ne!(item.headword.to_lowercase(), "zigzag");
            assert_ne!(item.headword.to_lowercase(), "zulu");
        }
    }
}

#[test]
fn every_built_item_passes_independent_verification() {
    let t = fixture();
    for seed in 0..40 {
        for item in t.build_items_configured(5, seed, 1, |_, _| true) {
            if let Err(failure) = verify(&item, &t) {
                panic!(
                    "seed {seed}: {} failed verification: {failure}",
                    item.headword
                );
            }
        }
    }
}

#[test]
fn the_same_seed_reproduces_the_same_items() {
    let t = fixture();
    let first = t.build_items_configured(5, 4242, 1, |_, _| true);
    let second = t.build_items_configured(5, 4242, 1, |_, _| true);
    assert_eq!(first, second, "ingestion must be reproducible");
    assert_eq!(
        first.iter().map(WkItem::content_hash).collect::<Vec<_>>(),
        second.iter().map(WkItem::content_hash).collect::<Vec<_>>()
    );
}

#[test]
fn a_batch_contains_no_duplicate_questions() {
    let t = fixture();
    let items = t.build_items_configured(6, 99, 1, |_, _| true);
    let hashes: std::collections::HashSet<String> =
        items.iter().map(WkItem::content_hash).collect();
    assert_eq!(hashes.len(), items.len(), "no repeated headword");
}

#[test]
fn a_root_word_with_too_few_terms_is_not_used() {
    let t = fixture();
    // `sparse` has three related terms, so four options cannot be filled from a
    // well-formed item; it must never appear as a headword.
    for item in t.build_items_configured(20, 3, 1, |_, _| true) {
        assert_ne!(item.headword.to_lowercase(), "sparse");
    }
}

#[test]
fn the_explanation_shows_the_source_list_rather_than_inventing_one() {
    let t = fixture();
    let items = t.build_items_configured(4, 5, 1, |_, _| true);
    let item = items.first().expect("an item");
    let explanation = item.explanation();
    assert!(explanation.contains(&item.headword));
    let related = t.related_to(&item.headword).expect("known headword");
    assert!(
        explanation.contains(&related[0]),
        "the explanation must quote the source: {explanation}"
    );
}

#[test]
fn an_attached_definition_leads_the_explanation_and_still_quotes_the_source() {
    let t = fixture();
    let mut items = t.build_items_configured(4, 5, 1, |_, _| true);
    let headword = items.first().expect("an item").headword.clone();
    let related = t.related_to(&headword).expect("known headword");

    // A lookup that supplies a definition for every headword, standing in for
    // Webster's. The definition must appear, and the source list must survive
    // beneath it -- an explanation is evidence, not a replacement for it.
    Thesaurus::attach_definitions(&mut items, |_| Some("a plain definition".to_string()));
    let item = items.first().expect("an item");
    let explanation = item.explanation();
    assert!(
        explanation.starts_with("a plain definition"),
        "the definition must lead: {explanation}"
    );
    assert!(
        explanation.contains(&related[0]),
        "the source list must still be quoted: {explanation}"
    );
    assert!(explanation.contains(&item.headword));
}

#[test]
fn the_definition_source_join_uses_a_plain_separator_not_an_em_dash() {
    let t = fixture();
    let mut items = t.build_items_configured(4, 5, 1, |_, _| true);
    // A definition that ends in a full stop, as a dictionary entry does.
    Thesaurus::attach_definitions(&mut items, |_| Some("a plain definition.".to_string()));
    for item in &items {
        let explanation = item.explanation();
        assert!(
            !explanation.contains('\u{2014}'),
            "the join must be plain, not an em-dash: {explanation}"
        );
        assert!(
            explanation.starts_with("a plain definition. The source lists"),
            "the definition's own full stop must not double with the separator: {explanation}"
        );
    }
}

#[test]
fn a_missing_definition_leaves_the_explanation_on_the_source_list_alone() {
    let t = fixture();
    let mut items = t.build_items_configured(4, 5, 1, |_, _| true);
    let naked: Vec<String> = items.iter().map(|i| i.explanation()).collect();

    // The dictionary has no entry for the headword, so nothing is invented and
    // the explanation is byte-for-byte what it was before.
    Thesaurus::attach_definitions(&mut items, |_| None);
    let after: Vec<String> = items.iter().map(|i| i.explanation()).collect();
    assert_eq!(
        naked, after,
        "an absent definition must not change the text"
    );
    assert!(items.iter().all(|i| i.definition.is_none()));
}

#[test]
fn attach_definitions_only_fills_the_headwords_the_lookup_knows() {
    let t = fixture();
    let mut items = t.build_items_configured(8, 3, 1, |_, _| true);
    let known = items.first().expect("an item").headword.clone();
    Thesaurus::attach_definitions(&mut items, |word| {
        (word == known).then(|| "known definition".to_string())
    });
    for item in &items {
        if item.headword == known {
            assert_eq!(item.definition.as_deref(), Some("known definition"));
        } else {
            assert!(item.definition.is_none());
        }
    }
}

// ---------------------------------------------------------------------------
// Verification must be able to fail
// ---------------------------------------------------------------------------

#[test]
fn verification_refuses_an_option_marked_correct_that_the_source_does_not_list() {
    let t = fixture();
    let mut item = t.build_items_configured(3, 11, 1, |_, _| true).remove(0);
    // Point correct_index at a distractor, which by construction is not listed
    // with the headword.
    item.correct_index = (item.correct_index + 1) % item.options.len();
    match verify(&item, &t) {
        Err(WkVerificationFailure::CorrectOptionNotInSource { .. }) => {}
        other => panic!("expected a source refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_an_ambiguous_distractor() {
    let t = fixture();
    let mut item = t.build_items_configured(3, 12, 1, |_, _| true).remove(0);
    // Substitute a word the source *does* list with the headword. The item then
    // has two defensible answers, which is the failure this check exists for.
    let related = t.related_to(&item.headword).expect("known headword");
    let ambiguous = related
        .iter()
        .find(|word| !item.options.iter().any(|o| o.eq_ignore_ascii_case(word)))
        .expect("a related term not already offered");
    let wrong = (item.correct_index + 1) % item.options.len();
    item.options[wrong] = ambiguous.clone();
    match verify(&item, &t) {
        Err(WkVerificationFailure::AmbiguousDistractor { .. }) => {}
        other => panic!("expected an ambiguity refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_an_unknown_headword() {
    let t = fixture();
    let mut item = t.build_items_configured(3, 13, 1, |_, _| true).remove(0);
    item.headword = "notinthesource".to_string();
    match verify(&item, &t) {
        Err(WkVerificationFailure::UnknownHeadword(_)) => {}
        other => panic!("expected an unknown-headword refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_duplicate_options() {
    let t = fixture();
    let mut item = t.build_items_configured(3, 14, 1, |_, _| true).remove(0);
    let wrongs: Vec<usize> = (0..item.options.len())
        .filter(|i| *i != item.correct_index)
        .collect();
    item.options[wrongs[1]] = item.options[wrongs[0]].clone();
    match verify(&item, &t) {
        Err(WkVerificationFailure::DuplicateOption(_)) => {}
        other => panic!("expected a duplicate refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_incomplete_rationale_coverage() {
    let t = fixture();
    let mut item = t.build_items_configured(3, 15, 1, |_, _| true).remove(0);
    let victim = *item
        .distractor_rationales
        .keys()
        .next()
        .expect("a distractor rationale");
    item.distractor_rationales.remove(&victim);
    match verify(&item, &t) {
        Err(WkVerificationFailure::RationaleCoverage { missing, .. }) => {
            assert_eq!(missing, vec![victim]);
        }
        other => panic!("expected a coverage refusal, got {other:?}"),
    }
}

#[test]
fn verification_refuses_an_out_of_range_correct_index() {
    let t = fixture();
    let mut item = t.build_items_configured(3, 16, 1, |_, _| true).remove(0);
    item.correct_index = item.options.len();
    match verify(&item, &t) {
        Err(WkVerificationFailure::CorrectIndexOutOfRange { .. }) => {}
        other => panic!("expected a range refusal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The caller's quality filter
// ---------------------------------------------------------------------------

#[test]
fn a_filter_that_accepts_nothing_yields_no_items() {
    let t = fixture();
    let items = t.build_items_configured(5, 1, 1, |_, _| false);
    assert!(
        items.is_empty(),
        "a rejecting filter must produce an empty corpus rather than falling \
         back to unfiltered pairs: {} item(s)",
        items.len()
    );
}

#[test]
fn the_filter_is_consulted_with_the_headword_and_the_candidate() {
    let t = fixture();
    let mut seen: Vec<(String, String)> = Vec::new();
    let items = t.build_items_configured(4, 2, 1, |headword, candidate| {
        seen.push((headword.to_string(), candidate.to_string()));
        true
    });
    assert!(!items.is_empty());
    assert!(!seen.is_empty(), "the filter must actually be consulted");
    for item in &items {
        let correct = item.options[item.correct_index].clone();
        assert!(
            seen.iter()
                .any(|(head, candidate)| head == &item.headword && candidate == &correct),
            "{} -> {correct} was built without the filter seeing the pair",
            item.headword
        );
    }
}

#[test]
fn a_selective_filter_changes_which_pairs_are_used() {
    let t = fixture();
    let items = t.build_items_configured(6, 3, 1, |_, candidate| {
        candidate.to_lowercase().starts_with('b')
    });
    assert!(
        !items.is_empty(),
        "the filter should still admit some pairs"
    );
    for item in &items {
        let correct = &item.options[item.correct_index];
        assert!(
            correct.to_lowercase().starts_with('b'),
            "{} -> {correct} slipped past the filter",
            item.headword
        );
    }
}

#[test]
fn filtered_items_still_verify_against_the_source() {
    let t = fixture();
    let items = t.build_items_configured(5, 4, 1, |_, candidate| {
        candidate.to_lowercase().starts_with('b')
    });
    for item in &items {
        verify(item, &t)
            .unwrap_or_else(|failure| panic!("{} failed verification: {failure}", item.headword));
    }
}

/// The verifier must not depend on the builder having produced the item.
#[test]
fn a_hand_built_item_with_a_wrong_answer_is_rejected() {
    let t = fixture();
    let item = WkItem {
        // `timid` is not listed with `brave` anywhere in the fixture.
        objective_id: "OBJ-WK-SYNONYM-01".to_string(),
        headword: "brave".to_string(),
        prompt: "Choose the word that most nearly means the same as BRAVE.".to_string(),
        options: vec![
            "timid".to_string(),
            "bold".to_string(),
            "cowardly".to_string(),
            "shy".to_string(),
        ],
        correct_index: 0,
        distractor_rationales: [
            (1, "not listed".to_string()),
            (2, "not listed".to_string()),
            (3, "not listed".to_string()),
        ]
        .into_iter()
        .collect(),
        supporting_line: "bold, courageous, valiant".to_string(),
        related_count: 8,
        difficulty: 0.0,
        seed: 0,
        definition: None,
    };
    match verify(&item, &t) {
        Err(WkVerificationFailure::CorrectOptionNotInSource { .. }) => {}
        other => panic!("an unprovable item must be rejected, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Difficulty (issue #9): derived from the source, not a flat constant
// ---------------------------------------------------------------------------

/// The builder must not ship one difficulty for every item.
///
/// The defect this guards: `difficulty: 0.2` was hardcoded, so the whole bank
/// carried a single value and the scheduler had nothing to discriminate on.
/// A build over the fixture, whose headwords have related-term lists of
/// different lengths, must produce more than one distinct difficulty.
#[test]
fn difficulty_varies_across_items_rather_than_being_a_constant() {
    let t = fixture();
    let items = t.build_items_configured(200, 4, 1, |_, _| true);
    assert!(
        items.len() >= 2,
        "need at least two items to test variation, built {}",
        items.len()
    );
    let mut distinct: Vec<f64> = items.iter().map(|item| item.difficulty).collect();
    distinct.sort_by(|a, b| a.partial_cmp(b).expect("no NaN difficulties"));
    distinct.dedup();
    assert!(
        distinct.len() > 1,
        "all {} items share difficulty {:?}; the score is not being derived",
        items.len(),
        distinct
    );
}

/// The derivation must read `related_count`, so a headword with a longer
/// related-term list is never scored easier than one with a shorter list.
///
/// This is the property the scheduler relies on, checked directly rather than
/// through the item builder: it pins the ranking, not just the presence of
/// variation.
#[test]
fn difficulty_is_monotone_in_related_count_for_equal_length_headwords() {
    // `opaque` (8 listed terms) vs `sparse` (3) — both plain six-character words,
    // so the length tilt is identical and breadth is the only difference.
    let t = fixture();
    let sparse = t.related_to("sparse").expect("sparse is a root word").len();
    let opaque = t.related_to("opaque").expect("opaque is a root word").len();
    assert!(
        opaque > sparse,
        "fixture must exercise both ends: opaque {opaque}, sparse {sparse}"
    );

    let items = t.build_items_configured(400, 1, 1, |_, _| true);
    let sparse_item = items.iter().find(|i| i.headword == "sparse");
    let opaque_item = items.iter().find(|i| i.headword == "opaque");
    if let (Some(s), Some(o)) = (sparse_item, opaque_item) {
        assert_eq!(s.related_count, sparse, "related_count is carried through");
        assert_eq!(o.related_count, opaque, "related_count is carried through");
        assert!(
            o.difficulty >= s.difficulty,
            "opaque ({opaque} terms, {}) must not score easier than sparse \
             ({sparse} terms, {})",
            o.difficulty,
            s.difficulty
        );
    }
}

/// Every derived difficulty stays inside the declared band.
#[test]
fn derived_difficulty_stays_within_the_declared_band() {
    let t = fixture();
    for item in t.build_items_configured(300, 7, 1, |_, _| true) {
        assert!(
            (-1.0..=1.5).contains(&item.difficulty),
            "difficulty {} for {:?} is outside [-1.0, 1.5]",
            item.difficulty,
            item.headword
        );
    }
}
