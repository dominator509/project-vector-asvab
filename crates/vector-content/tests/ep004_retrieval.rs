//! EP-004 acceptance: source-grounded retrieval and anti-fabrication (REQ-008).

use vector_content::retrieval::{
    explain_from, retrieve, score, verify_grounding, GroundedExplanation, GroundingError,
    SourceDocument,
};

fn doc(id: &str, title: &str, trust: f64, text: &str) -> SourceDocument {
    SourceDocument {
        id: id.to_string(),
        title: title.to_string(),
        url: format!("https://example.org/{id}"),
        content_hash: format!("sha256:{id}"),
        license: "CC-BY-4.0".to_string(),
        trust,
        text: text.to_string(),
    }
}

fn vault() -> Vec<SourceDocument> {
    vec![
        doc(
            "src-ar",
            "Arithmetic Reasoning primer",
            0.9,
            "Arithmetic reasoning problems involve solving word problems. \
             Rate and work problems are common.",
        ),
        doc(
            "src-wk",
            "Word Knowledge guide",
            0.8,
            "Word knowledge tests synonyms and antonyms. Vocabulary breadth matters.",
        ),
        doc(
            "src-pc",
            "Paragraph Comprehension",
            0.7,
            "Paragraph comprehension requires identifying the main idea of a passage.",
        ),
    ]
}

#[test]
fn retrieval_finds_the_topically_matching_source() {
    let results = retrieve("rate and work word problems", &vault(), 3, 0.1);

    assert!(
        !results.is_empty(),
        "a matching query must retrieve something"
    );
    assert_eq!(
        results[0].source_id,
        "src-ar",
        "the arithmetic source should rank first, got {:?}",
        results.iter().map(|r| &r.source_id).collect::<Vec<_>>()
    );
}

#[test]
fn unrelated_queries_retrieve_nothing_rather_than_filler() {
    // A relevance floor is what stops a citation to an unsupporting source.
    let results = retrieve("quantum chromodynamics lattice gauge", &vault(), 3, 0.1);
    assert!(
        results.is_empty(),
        "an unrelated query must not return sources, got {results:?}"
    );
}

#[test]
fn stop_words_do_not_create_false_relevance() {
    // "the of and" are pure stop words: they must score zero against anything.
    let d = doc("x", "X", 1.0, "the of and a to in is");
    assert_eq!(
        score("the of and a to", &d),
        0.0,
        "function words must not produce a relevance score"
    );
    assert!(retrieve("the of and a to", &vault(), 5, 0.01).is_empty());
}

#[test]
fn higher_trust_wins_at_equal_coverage() {
    let low = doc(
        "low",
        "Low trust",
        0.2,
        "photosynthesis converts light to energy",
    );
    let high = doc(
        "high",
        "High trust",
        0.95,
        "photosynthesis converts light to energy",
    );

    let low_score = score("photosynthesis light energy", &low);
    let high_score = score("photosynthesis light energy", &high);
    assert!(
        high_score > low_score,
        "at identical text, the more trusted source must rank higher: {high_score} !> {low_score}"
    );

    let results = retrieve("photosynthesis light energy", &[low, high], 2, 0.01);
    assert_eq!(results[0].source_id, "high");
}

#[test]
fn retrieval_ordering_is_deterministic_and_bounded() {
    let results_a = retrieve("word knowledge synonyms", &vault(), 2, 0.01);
    let results_b = retrieve("word knowledge synonyms", &vault(), 2, 0.01);
    assert_eq!(results_a, results_b, "retrieval must be reproducible");
    assert!(results_a.len() <= 2, "limit must be honoured");
}

#[test]
fn retrieval_preserves_provenance_for_every_passage() {
    let results = retrieve("word knowledge", &vault(), 3, 0.01);
    for passage in &results {
        assert!(!passage.source_id.is_empty());
        assert!(!passage.title.is_empty());
        assert!(passage.url.starts_with("https://"));
        assert!(
            passage.content_hash.starts_with("sha256:"),
            "every passage must carry the hash of the source it came from"
        );
    }
}

#[test]
fn a_fabricated_citation_is_rejected() {
    // The anti-hallucination control: a fluent citation to a source that does
    // not exist must be refused, not emitted.
    let explanation = GroundedExplanation {
        answer: "According to [src-invented], the answer is 42.".to_string(),
        cited_source_ids: vec!["src-invented".to_string()],
    };

    let result = verify_grounding(&explanation, &vault());
    assert_eq!(
        result,
        Err(GroundingError::UnknownSource("src-invented".to_string())),
        "a citation to a non-existent source must be refused"
    );
}

#[test]
fn an_explanation_with_no_citations_is_rejected() {
    let explanation = GroundedExplanation {
        answer: "Trust me, the answer is 42.".to_string(),
        cited_source_ids: vec![],
    };
    assert_eq!(
        verify_grounding(&explanation, &vault()),
        Err(GroundingError::NoCitations),
        "ungrounded prose must not pass as a grounded explanation"
    );
}

#[test]
fn a_real_citation_verifies() {
    let explanation = GroundedExplanation {
        answer: "Rate problems are common.".to_string(),
        cited_source_ids: vec!["src-ar".to_string()],
    };
    assert!(verify_grounding(&explanation, &vault()).is_ok());
}

#[test]
fn a_partially_fabricated_citation_is_rejected() {
    // One real citation must not launder a fabricated one.
    let explanation = GroundedExplanation {
        answer: "Mixed.".to_string(),
        cited_source_ids: vec!["src-ar".to_string(), "src-fake".to_string()],
    };
    assert_eq!(
        verify_grounding(&explanation, &vault()),
        Err(GroundingError::UnknownSource("src-fake".to_string()))
    );
}

#[test]
fn explanations_are_built_only_from_retrieved_passages() {
    let passages = retrieve("arithmetic reasoning rate work", &vault(), 2, 0.01);
    let explanation = explain_from("Rate problems", &passages).expect("grounded");

    // Every citation must correspond to something that was actually retrieved.
    for id in &explanation.cited_source_ids {
        assert!(
            passages.iter().any(|p| &p.source_id == id),
            "explanation cited {id} which was not retrieved"
        );
    }
    assert!(verify_grounding(&explanation, &vault()).is_ok());
}

#[test]
fn an_explanation_cannot_be_built_without_sources() {
    let result = explain_from("Anything", &[]);
    assert_eq!(
        result,
        Err(GroundingError::NoRelevantSource),
        "no retrieved source means no grounded explanation is possible"
    );
}

#[test]
fn citations_are_deduplicated() {
    let passages = retrieve("word knowledge", &vault(), 5, 0.01);
    let explanation = explain_from("Topic", &passages).expect("grounded");

    let mut sorted = explanation.cited_source_ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        explanation.cited_source_ids.len(),
        "each source must be cited at most once"
    );
}
