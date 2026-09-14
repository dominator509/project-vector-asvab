//! EP-007: property and fuzz-style hardening tests.
//!
//! The execplan requires "fuzz/property" proof. Rather than adding a fuzz
//! dependency, these tests drive the real parsers and validators over large
//! generated input spaces, including adversarial shapes, and assert invariants
//! that must hold for *every* input rather than for chosen examples.
//!
//! A deterministic PRNG keeps runs reproducible: a failure can be replayed
//! exactly, which a randomised fuzzer cannot guarantee without a recorded seed.

/// A small deterministic xorshift PRNG.
///
/// Chosen over a crate dependency so the corpus is byte-reproducible across
/// machines and toolchains, which matters when a failure has to be replayed
/// from evidence.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // A zero state would make xorshift produce only zeroes.
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_u32(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as u32
        }
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Build a string from a hostile alphabet.
    fn hostile_string(&mut self, max_len: usize) -> String {
        const ALPHABET: &[char] = &[
            'a', 'Z', '0', '9', ' ', '\n', '\r', '\t', '/', '\\', '.', ';', '|', '&', '$', '`',
            '<', '>', '"', '\'', '{', '}', '[', ']', '(', ')', '*', '?', '!', ':', '-', '_', '=',
            '%', '@', '#', '~', '^', '+', ',', 'é', '中', '\u{0}',
        ];
        let len = self.next_u32(max_len as u32 + 1) as usize;
        (0..len)
            .map(|_| ALPHABET[self.next_u32(ALPHABET.len() as u32) as usize])
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Archive path containment (security-critical, adversarial input)
// ---------------------------------------------------------------------------

#[test]
fn no_generated_archive_entry_escapes_the_root() {
    use vector_questions::ingestion::check_archive_entry;

    let mut rng = Rng::new(0xA11CE);
    let root = "content";

    for _ in 0..20_000 {
        let entry = rng.hostile_string(48);
        if check_archive_entry(&entry, root).is_ok() {
            // Anything accepted must be genuinely contained. Re-derive the
            // containment property independently of the function under test.
            let normalized = entry.replace('\\', "/");
            assert!(
                !entry.starts_with('/') && !entry.starts_with('\\'),
                "accepted an absolute entry: {entry:?}"
            );
            let segments: Vec<&str> = normalized
                .split('/')
                .filter(|s| !s.is_empty() && *s != ".")
                .collect();
            assert!(
                !segments.contains(&".."),
                "accepted a traversing entry: {entry:?}"
            );
            let joined = segments.join("/");
            assert!(
                joined == root || joined.starts_with(&format!("{root}/")),
                "accepted an entry outside the root: {entry:?} -> {joined:?}"
            );
        }
    }
}

#[test]
fn known_traversal_vectors_are_always_refused() {
    use vector_questions::ingestion::check_archive_entry;

    // Seed the generator's corpus with canonical attacks so the fuzz run is not
    // its only coverage.
    let attacks = [
        "../x",
        "..\\x",
        "a/../../x",
        "/etc/passwd",
        "\\windows\\x",
        "C:\\x",
        "content/../../x",
        "./../x",
        "content/./../../x",
        "....//x",
        "..;/x",
        "%2e%2e/x",
    ];
    for attack in attacks {
        assert!(
            check_archive_entry(attack, "content").is_err(),
            "{attack} must be refused"
        );
    }
}

// ---------------------------------------------------------------------------
// Source payload screening
// ---------------------------------------------------------------------------

#[test]
fn sandbox_parse_never_accepts_active_content() {
    use vector_questions::ingestion::{
        has_active_content, sandbox_parse, License, SourcePayload, TrustTier,
    };

    let mut rng = Rng::new(0xBEEF);
    for _ in 0..5_000 {
        let text = rng.hostile_string(64);
        let payload = SourcePayload {
            source_id: "SRC-FUZZ".to_string(),
            media_type: "text/plain".to_string(),
            license: License::PublicDomain,
            trust_tier: TrustTier::A,
            text: text.clone(),
            claims_official_items: false,
        };

        let accepted = sandbox_parse(&payload, TrustTier::D).is_ok();
        if accepted {
            assert!(
                !has_active_content(&text),
                "accepted payload containing active content: {text:?}"
            );
        }
    }
}

#[test]
fn sandbox_parse_is_deterministic_for_identical_payloads() {
    use vector_questions::ingestion::{sandbox_parse, License, SourcePayload, TrustTier};

    let mut rng = Rng::new(0xF00D);
    for _ in 0..2_000 {
        let text = rng.hostile_string(40);
        let payload = SourcePayload {
            source_id: "SRC-FUZZ".to_string(),
            media_type: "text/plain".to_string(),
            license: License::Permissive,
            trust_tier: TrustTier::B,
            text,
            claims_official_items: false,
        };
        let a = sandbox_parse(&payload, TrustTier::D);
        let b = sandbox_parse(&payload, TrustTier::D);
        assert_eq!(a, b, "screening must be deterministic");
    }
}

// ---------------------------------------------------------------------------
// Credential redaction (security-critical, adversarial input)
// ---------------------------------------------------------------------------

#[test]
fn redaction_never_leaves_a_registered_canary() {
    use vector_observability::crash::{
        redact_capture, verify_no_secrets, CanaryRegistry, CrashCapture,
    };

    let mut canaries = CanaryRegistry::new();
    canaries
        .register("canary-a", "canary-value-aaaa")
        .expect("register");
    canaries
        .register("canary-b", "canary-value-bbbb")
        .expect("register");

    let mut rng = Rng::new(0xCAFE);
    for _ in 0..2_000 {
        let noisy = rng.hostile_string(48);
        // Plant a canary at a random position in hostile noise.
        let text = format!("{noisy}canary-value-aaaa{noisy}");

        let capture = CrashCapture {
            id: "fuzz".to_string(),
            summary: text.clone(),
            message: text.clone(),
            stack: text.clone(),
            context: vec![("field".to_string(), text.clone())],
            log_ring: vec![text.clone()],
            build_id: "b".to_string(),
            redaction_passes: 0,
        };

        let outcome = redact_capture(&capture, &canaries);
        assert!(
            verify_no_secrets(&outcome.capture, &canaries).is_ok(),
            "a canary survived redaction for input {text:?}"
        );
        assert!(
            !outcome.capture.summary.contains("canary-value-aaaa"),
            "canary survived in summary for input {text:?}"
        );
    }
}

#[test]
fn redaction_is_idempotent_over_generated_input() {
    use vector_observability::crash::{redact_capture, CanaryRegistry, CrashCapture};

    let canaries = CanaryRegistry::new();
    let mut rng = Rng::new(0x1234);

    for _ in 0..2_000 {
        let text = rng.hostile_string(48);
        let capture = CrashCapture {
            id: "fuzz".to_string(),
            summary: text.clone(),
            message: text.clone(),
            stack: text,
            context: vec![],
            log_ring: vec![],
            build_id: "b".to_string(),
            redaction_passes: 0,
        };
        let once = redact_capture(&capture, &canaries);
        let twice = redact_capture(&once.capture, &canaries);
        assert_eq!(
            once.capture.summary, twice.capture.summary,
            "redaction must be stable under repetition"
        );
    }
}

// ---------------------------------------------------------------------------
// Process specification (argument content is inert)
// ---------------------------------------------------------------------------

#[test]
fn hostile_argument_content_is_preserved_byte_for_byte() {
    use std::collections::BTreeMap;
    use vector_platform::process::{build_spec, ProgramRef};

    let empty = BTreeMap::new();
    let mut rng = Rng::new(0x5678);

    for _ in 0..2_000 {
        let arg = rng.hostile_string(64);
        let spec = build_spec(
            &ProgramRef::BareName("codex".to_string()),
            std::slice::from_ref(&arg),
            &empty,
            &empty,
            None,
        )
        .expect("spec builds for any argument content");

        // The security property: no shell interprets this, so the bytes are
        // unchanged. Any mutation would indicate escaping or interpolation.
        assert_eq!(spec.args[0], arg, "argument content must be preserved");
        assert_eq!(spec.program, "codex", "program must be unchanged");
    }
}

#[test]
fn credentials_never_survive_environment_construction() {
    use std::collections::BTreeMap;
    use vector_platform::process::{build_env, ENV_DENYLIST};

    let mut rng = Rng::new(0x9ABC);
    for _ in 0..2_000 {
        let mut parent: BTreeMap<String, String> = BTreeMap::new();
        parent.insert("PATH".to_string(), "/usr/bin".to_string());
        // Sprinkle a randomly chosen denied variable into the parent.
        let denied = ENV_DENYLIST[rng.next_u32(ENV_DENYLIST.len() as u32) as usize];
        parent.insert(denied.to_string(), rng.hostile_string(24));

        let child = build_env(&parent, &BTreeMap::new()).expect("env builds");
        assert!(
            !child.contains_key(denied),
            "{denied} leaked into the child environment"
        );
    }
}

// ---------------------------------------------------------------------------
// Mastery and planning invariants
// ---------------------------------------------------------------------------

#[test]
fn planner_never_exceeds_its_budget_for_any_generated_input() {
    use vector_study::selection::{generate_plan, PlanGoal, SkillEstimate};

    let mut rng = Rng::new(0xD00D);
    for _ in 0..5_000 {
        let skill_count = 1 + rng.next_u32(6) as usize;
        let skills: Vec<SkillEstimate> = (0..skill_count)
            .map(|i| SkillEstimate {
                subtest: format!("S{i}"),
                mastery: rng.next_f64(),
                uncertainty: rng.next_f64(),
                due_reviews: rng.next_u32(50),
            })
            .collect();

        let budget = 1 + rng.next_u32(240);
        let goal = PlanGoal::Afqt(1 + rng.next_u32(99));

        let plan = generate_plan(&skills, &goal, budget).expect("plan generates");
        assert!(
            plan.total_minutes <= budget,
            "plan allocated {} from a {budget} minute budget",
            plan.total_minutes
        );
        assert!(
            plan.drills.iter().all(|d| d.minutes > 0),
            "no drill may receive zero minutes: {:?}",
            plan.drills
        );
    }
}

#[test]
fn a_plan_can_never_be_negative_or_empty_budgeted() {
    use vector_study::selection::{generate_plan, PlanGoal, SkillEstimate};

    let skills = vec![SkillEstimate {
        subtest: "AR".to_string(),
        mastery: 0.2,
        uncertainty: 0.3,
        due_reviews: 5,
    }];
    assert!(generate_plan(&skills, &PlanGoal::Afqt(70), 0).is_err());
}

#[test]
fn fsrs_scheduling_stays_finite_over_generated_states() {
    use vector_study::fsrs::{
        CardState, Fsrs, FsrsParameters, Rating, MAX_INTERVAL_DAYS, MIN_INTERVAL_DAYS,
    };

    let fsrs = Fsrs::new(FsrsParameters::default()).expect("valid");
    let ratings = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy];
    let mut rng = Rng::new(0xFEED);

    for _ in 0..5_000 {
        let state = CardState {
            stability: rng.next_f64() * 1000.0,
            difficulty: 1.0 + rng.next_f64() * 9.0,
            reps: rng.next_u32(50),
            lapses: rng.next_u32(20),
            last_rating: Some(ratings[rng.next_u32(4) as usize]),
            elapsed_days: rng.next_f64() * 365.0,
        };
        let rating = ratings[rng.next_u32(4) as usize];
        let elapsed = rng.next_f64() * 365.0;

        let out = fsrs.review(&state, rating, elapsed);
        assert!(
            out.interval_days.is_finite()
                && out.interval_days >= MIN_INTERVAL_DAYS
                && out.interval_days <= MAX_INTERVAL_DAYS,
            "bad interval {} for {state:?} {rating:?}",
            out.interval_days
        );
        assert!(
            out.state.stability.is_finite() && out.state.stability > 0.0,
            "bad stability {} for {state:?}",
            out.state.stability
        );
        assert!(
            out.state.difficulty.is_finite() && (1.0..=10.0).contains(&out.state.difficulty),
            "bad difficulty {} for {state:?}",
            out.state.difficulty
        );
        assert!(
            out.retrievability.is_finite() && (0.0..=1.0).contains(&out.retrievability),
            "bad retrievability {} for {state:?}",
            out.retrievability
        );
    }
}

// ---------------------------------------------------------------------------
// Readiness band invariants
// ---------------------------------------------------------------------------

#[test]
fn a_readiness_band_is_always_ordered_and_never_certain() {
    use vector_study::selection::{readiness_band, SkillEstimate};

    let mut rng = Rng::new(0xABCD);
    for _ in 0..5_000 {
        let count = 1 + rng.next_u32(8) as usize;
        let skills: Vec<SkillEstimate> = (0..count)
            .map(|i| SkillEstimate {
                subtest: format!("S{i}"),
                mastery: rng.next_f64(),
                uncertainty: rng.next_f64() * 2.0,
                due_reviews: 0,
            })
            .collect();

        let band = readiness_band(&skills).expect("band");
        assert!(band.low <= band.high, "band must be ordered: {band:?}");
        assert!(
            (0.0..=1.0).contains(&band.low) && (0.0..=1.0).contains(&band.high),
            "band must stay in [0,1]: {band:?}"
        );
        assert!(
            band.confidence < 1.0,
            "confidence must never reach certainty"
        );
        assert!(
            !band.official_score_claim,
            "no generated input may produce an official score claim"
        );
    }
}

// ---------------------------------------------------------------------------
// External score typing
// ---------------------------------------------------------------------------

#[test]
fn a_generated_score_record_never_loses_its_external_typing() {
    use vector_application::privacy::{may_blend_into_readiness, ScoreOrigin, ScoreRecord};

    let mut rng = Rng::new(0x1357);
    let origins = [
        ScoreOrigin::VectorEstimate,
        ScoreOrigin::UserEnteredOfficial,
        ScoreOrigin::ImportedOfficial,
    ];

    for _ in 0..5_000 {
        let origin = origins[rng.next_u32(3) as usize];
        let record = ScoreRecord {
            origin,
            value: rng.next_u32(120),
            recorded_on: "2026-09-10".to_string(),
            note: "provenance".to_string(),
        };
        let serialized = serde_json::to_value(&record).expect("serialize");
        let back: ScoreRecord = serde_json::from_value(serialized).expect("deserialize");

        assert_eq!(back.origin, origin, "origin must survive persistence");
        assert_eq!(
            may_blend_into_readiness(back.origin),
            !origin.is_external(),
            "blend rule must follow the origin"
        );
    }
}
