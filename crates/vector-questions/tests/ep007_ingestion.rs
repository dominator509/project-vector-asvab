//! EP-007 acceptance: source ingestion sandbox (REQ-050) and content
//! provenance (REQ-056).

use vector_questions::ingestion::{
    check_archive_entry, has_active_content, is_allowed_transition, looks_controlled,
    sandbox_parse, AnswerProof, ContentItem, IngestRefusal, ItemProvenance, ItemState, License,
    ProvenanceError, SourcePayload, TrustTier, MAX_INGEST_BYTES,
};
use vector_questions::provenance::{hashes_match, ContentHash};

fn payload(text: &str) -> SourcePayload {
    SourcePayload {
        source_id: "SRC-TEST-001".to_string(),
        media_type: "text/markdown".to_string(),
        license: License::PublicDomain,
        trust_tier: TrustTier::A,
        text: text.to_string(),
        claims_official_items: false,
    }
}

fn complete_provenance() -> ItemProvenance {
    ItemProvenance {
        objective_id: "OBJ-AR-RATES".to_string(),
        source_ids: vec!["SRC-ASVAB-004".to_string()],
        answer_proof: AnswerProof::Executable {
            expression: "12 * 150".to_string(),
            answer: "1800".to_string(),
        },
        reviewer: "reviewer-a".to_string(),
        content_hash: ContentHash::of_text("a printer produces 12 pages per minute"),
        generator_hash: Some(ContentHash::of_text("generator-v1 output")),
        verifier_hash: Some(ContentHash::of_text("verifier-v2 output")),
    }
}

// ---------------------------------------------------------------------------
// REQ-050: controlled material is refused
// ---------------------------------------------------------------------------

#[test]
fn leaked_official_material_is_refused() {
    // QUESTION_FACTORY.md: never seed from protected/current official text.
    let attempt = payload("This is a real ASVAB question from last year's form.");
    assert_eq!(
        sandbox_parse(&attempt, TrustTier::D),
        Err(IngestRefusal::ControlledMaterial)
    );
}

#[test]
fn a_self_declared_official_payload_is_refused() {
    let mut attempt = payload("Perfectly ordinary study notes.");
    attempt.claims_official_items = true;
    assert_eq!(
        sandbox_parse(&attempt, TrustTier::D),
        Err(IngestRefusal::ControlledMaterial)
    );
}

#[test]
fn controlled_material_markers_are_detected_case_insensitively() {
    for text in [
        "OFFICIAL ASVAB TEST ITEM",
        "Leaked Exam material",
        "From The Real Test",
        "Actual Test Question 14",
    ] {
        assert!(looks_controlled(text), "should be flagged: {text}");
    }
    assert!(!looks_controlled("A worked example about rates."));
}

#[test]
fn ordinary_public_domain_material_is_accepted() {
    let text = "To convert minutes to hours, divide by 60.";
    assert_eq!(
        sandbox_parse(&payload(text), TrustTier::D),
        Ok(text.to_string())
    );
}

// ---------------------------------------------------------------------------
// REQ-050: licensing and trust
// ---------------------------------------------------------------------------

#[test]
fn a_prohibited_license_is_refused() {
    let mut attempt = payload("text");
    attempt.license = License::Prohibited;
    assert_eq!(
        sandbox_parse(&attempt, TrustTier::D),
        Err(IngestRefusal::LicenseForbids(License::Prohibited))
    );
}

#[test]
fn an_unknown_license_is_refused() {
    // Unstated terms are not permission.
    let mut attempt = payload("text");
    attempt.license = License::Unknown;
    assert_eq!(
        sandbox_parse(&attempt, TrustTier::D),
        Err(IngestRefusal::LicenseForbids(License::Unknown))
    );
}

#[test]
fn only_known_permissive_licenses_permit_ingestion() {
    assert!(License::PublicDomain.permits_ingestion());
    assert!(License::Permissive.permits_ingestion());
    assert!(License::Licensed.permits_ingestion());
    assert!(!License::Unknown.permits_ingestion());
    assert!(!License::Prohibited.permits_ingestion());
}

#[test]
fn a_source_below_the_trust_floor_is_refused() {
    let mut attempt = payload("community forum anecdote");
    attempt.trust_tier = TrustTier::D;
    assert_eq!(
        sandbox_parse(&attempt, TrustTier::B),
        Err(IngestRefusal::TrustTooLow {
            tier: TrustTier::D,
            floor: TrustTier::B
        })
    );
}

#[test]
fn a_source_meeting_the_trust_floor_is_accepted() {
    let mut attempt = payload("peer-reviewed explanation");
    attempt.trust_tier = TrustTier::C;
    assert!(sandbox_parse(&attempt, TrustTier::C).is_ok());
}

// ---------------------------------------------------------------------------
// REQ-050: size, media type and active content
// ---------------------------------------------------------------------------

#[test]
fn an_oversized_payload_is_refused() {
    let huge = "x".repeat(MAX_INGEST_BYTES + 1);
    assert!(matches!(
        sandbox_parse(&payload(&huge), TrustTier::D),
        Err(IngestRefusal::TooLarge { .. })
    ));
}

#[test]
fn an_unsupported_media_type_is_refused() {
    let mut attempt = payload("binary-ish");
    attempt.media_type = "application/x-executable".to_string();
    assert!(matches!(
        sandbox_parse(&attempt, TrustTier::D),
        Err(IngestRefusal::UnsupportedMediaType(_))
    ));
}

#[test]
fn active_content_is_refused() {
    // CONTENT_GOVERNANCE.md: strip active content, bound size/type.
    for hostile in [
        "notes <script>alert(1)</script>",
        "text <iframe src=...>",
        "[go](javascript:alert(1))",
        "<?php echo 1; ?>",
    ] {
        assert!(
            has_active_content(hostile),
            "should be detected as active: {hostile}"
        );
        assert_eq!(
            sandbox_parse(&payload(hostile), TrustTier::D),
            Err(IngestRefusal::ActiveContent)
        );
    }
}

#[test]
fn clean_markdown_is_not_flagged_as_active() {
    assert!(!has_active_content(
        "# Heading\n\nSome **bold** text and `code`."
    ));
}

// ---------------------------------------------------------------------------
// REQ-050: archive path traversal (zip-slip)
// ---------------------------------------------------------------------------

#[test]
fn archive_entries_cannot_escape_the_extraction_root() {
    for entry in [
        "../etc/passwd",
        "../../secret",
        "a/../../b",
        "content/../../../etc/shadow",
    ] {
        assert_eq!(
            check_archive_entry(entry, "content"),
            Err(IngestRefusal::PathTraversal(entry.to_string())),
            "{entry} must not escape the root"
        );
    }
}

#[test]
fn absolute_archive_entries_are_refused() {
    assert!(check_archive_entry("/etc/passwd", "content").is_err());
    assert!(check_archive_entry("\\windows\\system32", "content").is_err());
    assert!(check_archive_entry("C:\\Windows\\system32", "content").is_err());
}

#[test]
fn an_absolute_entry_is_refused_even_when_its_root_prefix_matches() {
    // The interesting case: "/content/lesson.md" normalizes to a path that
    // *looks* contained. It is still absolute, and an absolute entry ignores the
    // extraction root entirely, so it must be refused. This is the case the
    // explicit absolute-path guard exists for; without it, only the containment
    // fall-through would catch absolute entries, and a matching prefix would
    // slip past.
    assert!(
        check_archive_entry("/content/lesson.md", "content").is_err(),
        "an absolute path must be refused even when its text starts with the root"
    );
    assert!(
        check_archive_entry("\\content\\lesson.md", "content").is_err(),
        "a backslash-absolute path must be refused too"
    );
}

#[test]
fn entries_inside_the_root_are_accepted() {
    assert!(check_archive_entry("content/lesson.md", "content").is_ok());
    assert!(check_archive_entry("content/sub/deep/item.json", "content").is_ok());
    assert!(check_archive_entry("content", "content").is_ok());
}

#[test]
fn a_prefix_sibling_directory_is_not_inside_the_root() {
    // "content-evil" shares a textual prefix with "content" but is a different
    // directory.
    assert!(
        check_archive_entry("content-evil/steal.md", "content").is_err(),
        "string-prefix matching would wrongly allow this"
    );
}

#[test]
fn windows_style_separators_are_handled() {
    assert!(check_archive_entry("content\\lesson.md", "content").is_ok());
    assert!(check_archive_entry("content\\..\\..\\escape", "content").is_err());
}

// ---------------------------------------------------------------------------
// REQ-056: provenance completeness
// ---------------------------------------------------------------------------

#[test]
fn complete_provenance_passes() {
    assert!(complete_provenance().is_complete().is_ok());
}

#[test]
fn provenance_missing_any_required_field_is_refused() {
    // REQ-056 names objective, source, proof, review and hash. Each is required.
    let mut no_objective = complete_provenance();
    no_objective.objective_id = "  ".to_string();
    assert_eq!(
        no_objective.is_complete(),
        Err(ProvenanceError::MissingObjective)
    );

    let mut no_sources = complete_provenance();
    no_sources.source_ids = vec![];
    assert_eq!(
        no_sources.is_complete(),
        Err(ProvenanceError::MissingSources)
    );

    let mut blank_sources = complete_provenance();
    blank_sources.source_ids = vec!["   ".to_string()];
    assert_eq!(
        blank_sources.is_complete(),
        Err(ProvenanceError::MissingSources),
        "a blank source id is not a source"
    );

    let mut no_reviewer = complete_provenance();
    no_reviewer.reviewer = "".to_string();
    assert_eq!(
        no_reviewer.is_complete(),
        Err(ProvenanceError::MissingReviewer)
    );

    let mut no_hash = complete_provenance();
    no_hash.content_hash = blank_hash();
    assert_eq!(
        no_hash.is_complete(),
        Err(ProvenanceError::MissingContentHash)
    );
}

/// A ContentHash whose value is empty, to exercise the absent-hash path.
///
/// Built through serde because [`ContentHash::parse`] validates length and would
/// refuse to construct one — which is the correct behaviour, and is exactly why
/// the absent case has to be reached another way.
fn blank_hash() -> ContentHash {
    serde_json::from_value(serde_json::json!("")).expect("empty hash deserializes")
}

#[test]
fn a_non_independent_verifier_is_refused() {
    // QUESTION_FACTORY.md step 7: the verifier "may not simply echo generator
    // output". Identical hashes mean verification did not occur.
    let mut echoed = complete_provenance();
    let same = ContentHash::of_text("identical output");
    echoed.generator_hash = Some(same.clone());
    echoed.verifier_hash = Some(same);

    assert_eq!(
        echoed.is_complete(),
        Err(ProvenanceError::VerifierNotIndependent)
    );
}

#[test]
fn missing_verifier_hashes_are_tolerated_but_distinct_ones_are_required() {
    // A hand-authored item may have no generator; provenance still needs the
    // other four fields.
    let mut manual = complete_provenance();
    manual.generator_hash = None;
    manual.verifier_hash = None;
    assert!(manual.is_complete().is_ok());
}

// ---------------------------------------------------------------------------
// REQ-056: item lifecycle
// ---------------------------------------------------------------------------

#[test]
fn only_active_items_are_servable() {
    for state in [
        ItemState::Draft,
        ItemState::MachineValidated,
        ItemState::IndependentVerified,
        ItemState::ContentReviewed,
        ItemState::Quarantined,
    ] {
        assert!(!state.is_servable(), "{state:?} must not be servable");
    }
    assert!(ItemState::Active.is_servable());
}

#[test]
fn the_pipeline_must_be_followed_in_order() {
    let mut item = ContentItem::new("item-1", complete_provenance());

    // Skipping straight to Active is refused.
    assert_eq!(
        item.advance_to(ItemState::Active),
        Err(ProvenanceError::InvalidTransition {
            from: ItemState::Draft,
            to: ItemState::Active
        })
    );

    item.advance_to(ItemState::MachineValidated)
        .expect("step 1");
    item.advance_to(ItemState::IndependentVerified)
        .expect("step 2");
    item.advance_to(ItemState::ContentReviewed).expect("step 3");
    item.advance_to(ItemState::Active).expect("activate");
    assert!(item.is_servable());
}

#[test]
fn any_state_may_be_quarantined() {
    for start in [
        ItemState::Draft,
        ItemState::MachineValidated,
        ItemState::IndependentVerified,
        ItemState::ContentReviewed,
        ItemState::Active,
    ] {
        assert!(
            is_allowed_transition(start, ItemState::Quarantined),
            "{start:?} must be quarantinable"
        );
    }
}

#[test]
fn activation_requires_complete_provenance() {
    // A state machine can be advanced without evidence; activation checks both.
    let mut incomplete = complete_provenance();
    incomplete.source_ids = vec![];

    let mut item = ContentItem::new("item-1", incomplete);
    item.advance_to(ItemState::MachineValidated)
        .expect("step 1");
    item.advance_to(ItemState::IndependentVerified)
        .expect("step 2");
    item.advance_to(ItemState::ContentReviewed).expect("step 3");

    assert_eq!(
        item.advance_to(ItemState::Active),
        Err(ProvenanceError::MissingSources),
        "activation must refuse incomplete provenance"
    );
    assert!(!item.is_servable());
}

#[test]
fn a_servable_item_needs_both_state_and_provenance() {
    let mut item = ContentItem::new("item-1", complete_provenance());
    assert!(!item.is_servable(), "a draft is not servable");

    item.advance_to(ItemState::MachineValidated).expect("1");
    item.advance_to(ItemState::IndependentVerified).expect("2");
    item.advance_to(ItemState::ContentReviewed).expect("3");
    assert!(!item.is_servable(), "reviewed but not active");

    item.advance_to(ItemState::Active).expect("activate");
    assert!(item.is_servable());
}

#[test]
fn a_quarantined_item_may_be_reworked() {
    let mut item = ContentItem::new("item-1", complete_provenance());
    item.advance_to(ItemState::Quarantined).expect("quarantine");
    assert!(!item.is_servable());
    item.advance_to(ItemState::Draft).expect("rework");
    assert!(item.advance_to(ItemState::MachineValidated).is_ok());
}

#[test]
fn an_unknown_transition_is_refused() {
    assert!(!is_allowed_transition(
        ItemState::Draft,
        ItemState::IndependentVerified
    ));
    assert!(!is_allowed_transition(
        ItemState::Active,
        ItemState::MachineValidated
    ));
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

#[test]
fn a_text_hash_ignores_line_ending_style() {
    // Identical content under different line endings must hash the same, or
    // provenance verification fails for reasons unrelated to tampering.
    let unix = ContentHash::of_text("line one\nline two");
    let windows = ContentHash::of_text("line one\r\nline two");
    assert_eq!(unix, windows);
}

#[test]
fn a_hash_parse_rejects_malformed_digests() {
    assert!(ContentHash::parse("abc").is_err());
    assert!(ContentHash::parse(&"g".repeat(64)).is_err());
    assert!(ContentHash::parse(&"a".repeat(64)).is_ok());
    // Uppercase is normalized rather than rejected.
    assert_eq!(
        ContentHash::parse(&"A".repeat(64)).unwrap().as_str(),
        "a".repeat(64)
    );
}

#[test]
fn a_hash_detects_tampering() {
    let original = ContentHash::of_text("original content");
    assert!(original.matches(b"original content"));
    assert!(!original.matches(b"tampered content"));
}

#[test]
fn empty_hashes_never_match() {
    let empty = blank_hash();
    let real = ContentHash::of_text("x");
    assert!(
        !hashes_match(&empty, &real),
        "an absent hash must not be treated as matching"
    );
}
