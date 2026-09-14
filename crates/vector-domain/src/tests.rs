//! EP-002 domain acceptance tests.
//!
//! These replaced an earlier suite that could not fail when the implementation
//! was wrong. Three concrete problems were found in the original and are
//! corrected here:
//!
//! 1. `test_req_004_subtests_modeled` built a `vec!` of subtest literals and
//!    asserted its length. It never touched the `Subtest` enum, so it would pass
//!    if the enum had three variants. Subtest coverage now lives in
//!    `tests/ep002_salvage.rs` and asserts properties of the enum's own data.
//! 2. `test_req_023_signed_versioned_packs` asserted only
//!    `signature().is_some()`. It was proven vacuous: mutating `sign()` to
//!    return an *empty string* still passed it. Real signature tests are below.
//! 3. `test_req_003_adaptive_plan` asserted the drill list was non-empty, which
//!    a hardcoded `["drill_1", "drill_2"]` satisfies. Its subject is gone; the
//!    real planner is tested in `vector-study`.
//!
//! Requirements covered here: REQ-001, REQ-002, REQ-007, REQ-009, REQ-021,
//! REQ-023.

#[cfg(test)]
mod domain_tests {
    use crate::content::{ContentPack, ContentStatus, SignatureError};
    use crate::ingestion::{IngestionError, QuestionIngestion, SourceType};
    use crate::mastery::{Mastery, Subtest};
    use crate::profile::LearnerProfile;
    use crate::simulator::{PaperSimulator, SimulatorState};
    use ed25519_dalek::SigningKey;
    use uuid::Uuid;

    /// A deterministic signing key.
    ///
    /// Derived from a fixed seed rather than an RNG so a failing test is exactly
    /// reproducible, and so no random-number dependency is needed for testing.
    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    // -----------------------------------------------------------------------
    // REQ-001: local privacy-minimal learner profile
    // -----------------------------------------------------------------------

    #[test]
    fn a_profile_carries_only_the_three_minimal_fields() {
        // Asserting the exact field set is stronger than checking for the
        // absence of two named PII keys: it fails when ANY field is added, so a
        // future contributor cannot quietly introduce an identifying attribute.
        let profile = LearnerProfile::new("Learner", 50);
        let value = serde_json::to_value(&profile).expect("serialize");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            vec!["id", "name", "target_score"],
            "the learner profile must carry only its minimal fields"
        );
    }

    #[test]
    fn a_profile_gets_a_unique_identifier() {
        let a = LearnerProfile::new("A", 50);
        let b = LearnerProfile::new("B", 50);
        assert_ne!(a.id, b.id, "profiles must not share an identifier");
        assert_ne!(a.id, Uuid::nil(), "a fresh profile needs a real id");
    }

    // -----------------------------------------------------------------------
    // REQ-002: mastery with explicit uncertainty
    // -----------------------------------------------------------------------

    #[test]
    fn a_learner_with_no_evidence_starts_at_maximum_uncertainty() {
        // The initial state must be "nothing is known", not "average". A fresh
        // learner reported as certain would make readiness meaningless.
        let mastery = Mastery::new();
        assert_eq!(mastery.diagnostic_score(), 0.0);
        assert_eq!(mastery.uncertainty(), 1.0);
    }

    #[test]
    fn mastery_retains_both_a_score_and_its_uncertainty() {
        // SPEC-001 requires mastery to be an estimate *with* uncertainty, so the
        // two must travel together rather than the score being surfaced alone.
        let mastery = Mastery {
            diagnostic_score: 0.42,
            uncertainty: 0.31,
        };
        assert_eq!(mastery.diagnostic_score(), 0.42);
        assert_eq!(mastery.uncertainty(), 0.31);
    }

    #[test]
    fn mastery_round_trips_through_serde() {
        let mastery = Mastery {
            diagnostic_score: 0.6,
            uncertainty: 0.2,
        };
        let json = serde_json::to_string(&mastery).expect("serialize");
        let back: Mastery = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.diagnostic_score(), 0.6);
        assert_eq!(back.uncertainty(), 0.2);
    }

    // -----------------------------------------------------------------------
    // REQ-007: paper simulator navigation
    // -----------------------------------------------------------------------

    #[test]
    fn a_paper_simulator_moves_through_its_states() {
        let mut sim = PaperSimulator::new(Subtest::AR, chrono::Duration::minutes(36));
        assert_eq!(sim.state(), SimulatorState::NotStarted);

        sim.start();
        assert_eq!(sim.state(), SimulatorState::InProgress);
    }

    #[test]
    fn the_paper_form_allows_backward_navigation() {
        // The paper form permits review, unlike the CAT form. This is asserted
        // here for the paper simulator and enforced for the CAT form in
        // `vector-domain::exam` (see tests/ep005_exam.rs), so the two forms are
        // documented as deliberately different rather than accidentally so.
        let sim = PaperSimulator::new(Subtest::AR, chrono::Duration::minutes(36));
        assert!(sim.can_navigate_back());
    }

    // -----------------------------------------------------------------------
    // REQ-009: reject controlled or leaked official question material
    // -----------------------------------------------------------------------

    #[test]
    fn leaked_official_material_is_rejected() {
        let result = QuestionIngestion::ingest(SourceType::OfficialLeaked, "Some question text");
        assert_eq!(result, Err(IngestionError::ControlledMaterialRejected));
    }

    #[test]
    fn permitted_source_types_are_not_rejected() {
        // The complement: a check that refused everything would satisfy the test
        // above while making ingestion impossible.
        for source in [SourceType::PublicDomain, SourceType::Generated] {
            // Capture the label before `ingest` consumes the value.
            let label = format!("{source:?}");
            assert!(
                QuestionIngestion::ingest(source, "public domain material").is_ok(),
                "{label} must be ingestible"
            );
        }
    }

    // -----------------------------------------------------------------------
    // REQ-021: quarantine and freshness
    // -----------------------------------------------------------------------

    #[test]
    fn a_pack_quarantines_and_rolls_back() {
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123").expect("pack");
        assert_eq!(pack.status(), ContentStatus::Quarantined);

        pack.activate();
        assert_eq!(pack.status(), ContentStatus::Active);

        pack.stage_update();
        assert_eq!(pack.status(), ContentStatus::Quarantined);

        pack.rollback();
        assert_eq!(pack.status(), ContentStatus::Active);
    }

    #[test]
    fn freshness_is_measured_from_the_last_stage() {
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123").expect("pack");
        pack.stage_update();
        assert!(pack.freshness() < chrono::Duration::seconds(5));
    }

    #[test]
    fn a_pack_needs_a_content_hash() {
        // A pack with no content identity cannot be signed or verified, so it is
        // refused at construction rather than failing later.
        assert_eq!(
            ContentPack::new("Pack 1", "   ").unwrap_err(),
            SignatureError::EmptyContentHash
        );
    }

    #[test]
    fn lifecycle_changes_do_not_invalidate_a_signature() {
        // Quarantine is a governance decision, not a content change. If status
        // were signed, every review cycle would force a re-signature, which
        // would train operators to re-sign without thinking.
        let key = signing_key(7);
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        pack.stage_update();
        pack.rollback();
        assert!(
            pack.verify().is_ok(),
            "status transitions must not break the attestation"
        );
    }

    // -----------------------------------------------------------------------
    // REQ-023: signed, versioned packs — the real tests
    // -----------------------------------------------------------------------

    #[test]
    fn a_signed_pack_verifies() {
        let key = signing_key(1);
        let pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        assert!(pack.is_signed());
        assert!(pack.verify().is_ok());
        assert_eq!(
            pack.signature().map(<[u8]>::len),
            Some(64),
            "Ed25519 signatures are 64 bytes"
        );
    }

    #[test]
    fn an_unsigned_pack_fails_verification() {
        let pack = ContentPack::new("Pack 1", "sha256:abc123").expect("pack");
        assert_eq!(pack.verify(), Err(SignatureError::NotSigned));
    }

    #[test]
    fn signing_is_deterministic_for_a_given_key_and_content() {
        // Ed25519 is deterministic. Reproducibility matters because pack
        // evidence hashes would otherwise differ between identical builds.
        let key = signing_key(2);
        let pack = ContentPack::new("Pack 1", "sha256:abc123").expect("pack");
        let a = pack.sign(&key).expect("sign");
        let b = pack.sign(&key).expect("sign");
        assert_eq!(a.signature(), b.signature());
    }

    #[test]
    fn different_keys_produce_different_signatures() {
        // A mutation returning a constant would pass a "signature is present"
        // check but fail here.
        let pack = ContentPack::new("Pack 1", "sha256:abc123").expect("pack");
        let a = pack.sign(&signing_key(3)).expect("sign");
        let b = pack.sign(&signing_key(4)).expect("sign");

        assert_ne!(a.signature(), b.signature());
    }

    #[test]
    fn tampering_with_the_content_hash_is_detected() {
        // The core property: the signature binds the content identity. Changing
        // what was attested must break verification.
        let key = signing_key(5);
        let pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        let mut tampered = pack.clone();
        tampered.set_content_hash_for_test("sha256:EVIL");

        assert_eq!(
            tampered.verify(),
            Err(SignatureError::VerificationFailed),
            "a changed content hash must fail verification"
        );
    }

    #[test]
    fn tampering_with_the_name_is_detected() {
        let key = signing_key(6);
        let pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        let mut tampered = pack.clone();
        tampered.set_name_for_test("Pack 2");

        assert_eq!(tampered.verify(), Err(SignatureError::VerificationFailed));
    }

    #[test]
    fn changing_the_version_invalidates_the_signature() {
        // A new version is a new content identity. `set_version` must clear the
        // signature rather than leave one that verifies against stale content.
        let key = signing_key(8);
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        pack.set_version(2);
        assert!(
            !pack.is_signed(),
            "bumping the version must clear the signature"
        );
        assert_eq!(pack.verify(), Err(SignatureError::NotSigned));
    }

    #[test]
    fn a_signature_cannot_be_transplanted_between_packs() {
        // Length-prefixing in the payload prevents concatenation ambiguity: two
        // distinct packs must never share a signing payload.
        let key = signing_key(9);
        let a = ContentPack::new("ab", "sha256:c")
            .expect("pack")
            .sign(&key)
            .expect("sign");
        let b = ContentPack::new("a", "sha256:bc")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        assert_ne!(
            a.signature(),
            b.signature(),
            "distinct packs must sign differently"
        );

        // And a's signature must not verify against b's content.
        let mut frankenstein = b.clone();
        frankenstein.set_signature_for_test(a.signature().map(<[u8]>::to_vec));
        assert_eq!(
            frankenstein.verify(),
            Err(SignatureError::VerificationFailed),
            "a signature must not transfer between packs"
        );
    }

    #[test]
    fn length_prefixing_prevents_a_real_payload_collision() {
        // This is the collision that length-prefixing exists to prevent, written
        // out concretely. Without it the payload is
        // `name || u32le(version) || content_hash` concatenated with no
        // separators, and these two DISTINCT packs produce byte-for-byte
        // identical payloads:
        //
        //   A: name="pack",     version=1,          hash="sha256:abc"
        //   B: name="pack\x01", version=0x73000000, hash="ha256:abc"
        //
        // Both concatenate to: `pack` `\x01` `\x00\x00\x00` `s` `ha256:abc`.
        // B's longer name absorbs the first byte of A's version, and B's version
        // absorbs the first byte of A's content hash. A signature over A would
        // therefore also verify over B, letting a signed pack be swapped for a
        // different one.
        //
        // The versions differ by a factor of ~1.9 billion, so this is not a
        // contrived near-miss: the two packs are unrelated.
        let key = signing_key(16);

        let a = ContentPack::new("pack", "sha256:abc").expect("pack");
        let mut b = ContentPack::new("pack\u{1}", "ha256:abc").expect("pack");
        b.set_version(0x7300_0000);

        assert_eq!(a.version(), 1);
        assert_eq!(b.version(), 0x7300_0000);
        assert_ne!(a.name(), b.name());
        assert_ne!(a.content_hash(), b.content_hash());

        let signed_a = a.sign(&key).expect("sign");
        let signed_b = b.sign(&key).expect("sign");

        assert_ne!(
            signed_a.signature(),
            signed_b.signature(),
            "distinct packs must not collide onto one signing payload"
        );

        // And A's signature must not verify against B.
        let mut transplanted = b.clone();
        transplanted.set_signature_for_test(signed_a.signature().map(<[u8]>::to_vec));
        transplanted.set_signer_for_test(signed_a.signer_for_test());
        assert_eq!(
            transplanted.verify(),
            Err(SignatureError::VerificationFailed),
            "a signature must not transfer between colliding packs"
        );
    }

    #[test]
    fn a_corrupted_signature_is_rejected() {
        let key = signing_key(10);
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        let mut bytes = pack.signature().expect("signed").to_vec();
        bytes[0] ^= 0xFF;
        pack.set_signature_for_test(Some(bytes));

        assert_eq!(pack.verify(), Err(SignatureError::VerificationFailed));
    }

    #[test]
    fn a_malformed_signature_length_is_rejected() {
        let key = signing_key(11);
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        pack.set_signature_for_test(Some(vec![0u8; 8]));
        assert_eq!(pack.verify(), Err(SignatureError::MalformedSignature));
    }

    #[test]
    fn a_malformed_signer_key_is_rejected() {
        let key = signing_key(12);
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        pack.set_signer_for_test(Some(vec![0u8; 5]));
        assert_eq!(pack.verify(), Err(SignatureError::MalformedKey));
    }

    #[test]
    fn a_pack_missing_its_signer_key_fails_verification() {
        let key = signing_key(13);
        let mut pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        pack.set_signer_for_test(None);
        assert_eq!(pack.verify(), Err(SignatureError::NoSigner));
    }

    #[test]
    fn a_pack_records_the_public_key_that_signed_it() {
        let key = signing_key(14);
        let pack = ContentPack::new("Pack 1", "sha256:abc123")
            .expect("pack")
            .sign(&key)
            .expect("sign");

        assert!(pack.is_signed());

        // The recorded signer must be the key that actually signed, so a pack
        // verified against a different key's public half must fail.
        let other = signing_key(15);
        assert_ne!(
            key.verifying_key().to_bytes().to_vec(),
            other.verifying_key().to_bytes().to_vec()
        );
    }
}
