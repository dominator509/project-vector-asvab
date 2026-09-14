//! EP-006 acceptance: crash double-redaction and secret canaries
//! (REQ-028, REQ-029; SECURITY.md "double-redaction and secret-canary tests").
//!
//! The threat being defended against is a crash report leaking a credential or
//! personal data. The tests below plant secrets in every field a crash actually
//! populates — panic message, stack, structured context and the log ring — and
//! assert they do not survive.

use vector_observability::crash::{
    approve_release, is_secret_key, redact_capture, verify_no_secrets, CanaryRegistry,
    CrashCapture, ReleaseRefusal, REDACTED,
};

const CANARY_OPENAI: &str = "sk-canary0000000000000000000000";
const CANARY_DB_PASSWORD: &str = "canary-db-password-9f3a";
const CANARY_EMAIL: &str = "learner.canary@example.org";

fn canaries() -> CanaryRegistry {
    let mut registry = CanaryRegistry::new();
    registry
        .register("test-openai-key", CANARY_OPENAI)
        .expect("register openai canary");
    registry
        .register("test-db-password", CANARY_DB_PASSWORD)
        .expect("register db canary");
    registry
        .register("test-email", CANARY_EMAIL)
        .expect("register email canary");
    registry
}

/// A capture with secrets planted in every field a real crash populates.
fn dirty_capture() -> CrashCapture {
    CrashCapture {
        id: "crash-1".to_string(),
        summary: format!("Provider call failed using {CANARY_OPENAI}"),
        message: format!("connection failed: password={CANARY_DB_PASSWORD}; owner {CANARY_EMAIL}"),
        stack: "at auth (client.rs:42)\n  authorization: Bearer abcdefghijklmnop\n  \
                key=AIzaSyA1234567890abcdefghijklmnopq"
            .to_string(),
        context: vec![
            ("api_key".to_string(), CANARY_OPENAI.to_string()),
            ("user_email".to_string(), CANARY_EMAIL.to_string()),
            ("subtest".to_string(), "AR".to_string()),
            ("trace".to_string(), format!("token={CANARY_DB_PASSWORD}")),
        ],
        log_ring: vec![
            format!("INFO starting with {CANARY_OPENAI}"),
            "INFO loaded 12 questions".to_string(),
            format!("WARN retrying with {CANARY_DB_PASSWORD}"),
        ],
        build_id: "build-2026-09-10".to_string(),
        redaction_passes: 0,
    }
}

// ---------------------------------------------------------------------------
// Canary registry
// ---------------------------------------------------------------------------

#[test]
fn a_canary_requires_a_meaningful_value() {
    let mut registry = CanaryRegistry::new();
    assert!(
        registry.register("empty", "").is_err(),
        "an empty canary would match everywhere"
    );
    assert!(registry.register("blank", "   ").is_err());
    assert!(
        registry.register("tiny", "ab").is_err(),
        "a two-character sentinel is not meaningful"
    );
    assert!(registry.register("ok", "canary-value-1234").is_ok());
}

#[test]
fn secret_key_detection_covers_common_names() {
    for key in [
        "password",
        "db_password",
        "api_key",
        "apikey",
        "auth_token",
        "secret",
        "private_key",
        "cookie",
        "session_id",
        "credential",
    ] {
        assert!(
            is_secret_key(key),
            "{key} must be treated as secret-bearing"
        );
    }
    for key in ["subtest", "score", "learner_id", "created_at"] {
        assert!(!is_secret_key(key), "{key} is not a secret field");
    }
}

// ---------------------------------------------------------------------------
// Double redaction
// ---------------------------------------------------------------------------

#[test]
fn redaction_applies_two_passes() {
    let outcome = redact_capture(&dirty_capture(), &canaries());
    assert_eq!(
        outcome.capture.redaction_passes, 2,
        "SECURITY.md requires double redaction"
    );
}

#[test]
fn redaction_removes_secrets_from_every_field() {
    let outcome = redact_capture(&dirty_capture(), &canaries());
    let capture = &outcome.capture;

    assert!(!capture.summary.contains(CANARY_OPENAI));
    assert!(!capture.message.contains(CANARY_DB_PASSWORD));
    assert!(!capture.message.contains(CANARY_EMAIL));
    assert!(!capture.stack.contains("abcdefghijklmnop"));
    assert!(!capture.stack.contains("AIzaSyA1234567890abcdefghijklmnopq"));

    for (key, value) in &capture.context {
        assert!(
            !value.contains(CANARY_OPENAI) && !value.contains(CANARY_DB_PASSWORD),
            "context field {key} still contains a secret: {value}"
        );
    }
    for line in &capture.log_ring {
        assert!(
            !line.contains(CANARY_OPENAI) && !line.contains(CANARY_DB_PASSWORD),
            "log line still contains a secret: {line}"
        );
    }
}

#[test]
fn a_secret_keyed_field_is_redacted_wholesale() {
    // Even a value with no recognizable scheme is removed when its key says it
    // is sensitive, because the key is the stronger signal.
    let capture = CrashCapture {
        id: "c".to_string(),
        summary: "x".to_string(),
        message: "y".to_string(),
        stack: "z".to_string(),
        context: vec![("api_key".to_string(), "totally-unknown-format".to_string())],
        log_ring: vec![],
        build_id: "b".to_string(),
        redaction_passes: 0,
    };
    let outcome = redact_capture(&capture, &CanaryRegistry::new());
    assert_eq!(outcome.capture.context[0].1, REDACTED);
}

#[test]
fn non_secret_content_survives_redaction() {
    // Over-redaction would make crash reports useless for repair.
    let outcome = redact_capture(&dirty_capture(), &canaries());
    let capture = &outcome.capture;

    assert!(capture
        .log_ring
        .iter()
        .any(|l| l.contains("loaded 12 questions")));
    let subtest = capture
        .context
        .iter()
        .find(|(k, _)| k == "subtest")
        .expect("subtest context preserved");
    assert_eq!(subtest.1, "AR");
    assert_eq!(capture.build_id, "build-2026-09-10");
    assert_eq!(capture.id, "crash-1");
}

#[test]
fn a_canary_with_no_recognizable_scheme_is_still_removed() {
    // The canary pass exists precisely for secrets no pattern would catch.
    let opaque = "zzz-opaque-sentinel-4711";
    let mut registry = CanaryRegistry::new();
    registry.register("opaque", opaque).expect("register");

    let capture = CrashCapture {
        id: "c".to_string(),
        summary: format!("failed with {opaque}"),
        message: format!("detail {opaque}"),
        stack: format!("at {opaque}"),
        context: vec![("note".to_string(), opaque.to_string())],
        log_ring: vec![format!("log {opaque}")],
        build_id: "b".to_string(),
        redaction_passes: 0,
    };

    let outcome = redact_capture(&capture, &registry);
    assert!(verify_no_secrets(&outcome.capture, &registry).is_ok());
    assert!(!outcome.capture.summary.contains(opaque));
    assert!(!outcome.capture.message.contains(opaque));
    assert!(!outcome.capture.stack.contains(opaque));
}

#[test]
fn redaction_records_how_many_secrets_were_removed() {
    let outcome = redact_capture(&dirty_capture(), &canaries());
    assert!(
        outcome.redactions >= 5,
        "expected several redactions, counted {}",
        outcome.redactions
    );
}

#[test]
fn redaction_is_idempotent() {
    // Re-redacting an already-redacted capture must not corrupt it further.
    let once = redact_capture(&dirty_capture(), &canaries());
    let twice = redact_capture(&once.capture, &canaries());

    assert_eq!(
        once.capture.message, twice.capture.message,
        "redaction must be stable under repetition"
    );
    assert!(verify_no_secrets(&twice.capture, &canaries()).is_ok());
}

// ---------------------------------------------------------------------------
// Verification: redaction that ran is not redaction that worked
// ---------------------------------------------------------------------------

#[test]
fn verification_catches_an_unredacted_capture() {
    let result = verify_no_secrets(&dirty_capture(), &canaries());
    let residual = result.expect_err("a raw capture must not pass verification");
    assert!(!residual.is_empty());
    // It should name where the leak is, so the failure is actionable.
    assert!(residual.iter().any(|r| r.location.contains("summary")));
}

#[test]
fn verification_passes_after_redaction() {
    let outcome = redact_capture(&dirty_capture(), &canaries());
    assert!(
        verify_no_secrets(&outcome.capture, &canaries()).is_ok(),
        "a redacted capture must verify clean"
    );
}

#[test]
fn verification_reports_the_leak_location() {
    let capture = CrashCapture {
        id: "c".to_string(),
        summary: "clean".to_string(),
        message: format!("leak: {CANARY_DB_PASSWORD}"),
        stack: "clean".to_string(),
        context: vec![],
        log_ring: vec![],
        build_id: "b".to_string(),
        redaction_passes: 2,
    };
    let residual = verify_no_secrets(&capture, &canaries()).expect_err("must detect");
    assert!(residual.iter().any(|r| r.location == "message"));
}

#[test]
fn recognized_credential_schemes_are_detected_without_a_canary() {
    // No canary registered: detection must rely on the value's shape alone.
    let empty = CanaryRegistry::new();
    for secret in [
        "Bearer abcdefghijklmnopqrst",
        "sk-abcdefghijklmnopqrstuvwx",
        "ghp_abcdefghijklmnopqrstuvwxyz01",
        "AKIAIOSFODNN7EXAMPLE",
    ] {
        let capture = CrashCapture {
            id: "c".to_string(),
            summary: secret.to_string(),
            message: String::new(),
            stack: String::new(),
            context: vec![],
            log_ring: vec![],
            build_id: "b".to_string(),
            redaction_passes: 2,
        };
        assert!(
            verify_no_secrets(&capture, &empty).is_err(),
            "{secret} should have been recognized as a credential"
        );
    }
}

#[test]
fn private_key_blocks_are_redacted() {
    let key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
    let capture = CrashCapture {
        id: "c".to_string(),
        summary: "crash".to_string(),
        message: key.to_string(),
        stack: String::new(),
        context: vec![],
        log_ring: vec![],
        build_id: "b".to_string(),
        redaction_passes: 0,
    };
    let outcome = redact_capture(&capture, &CanaryRegistry::new());
    assert!(!outcome.capture.message.contains("MIIEowIBAAKCAQEA"));
    assert!(verify_no_secrets(&outcome.capture, &CanaryRegistry::new()).is_ok());
}

// ---------------------------------------------------------------------------
// Release gate
// ---------------------------------------------------------------------------

#[test]
fn release_requires_consent() {
    let outcome = redact_capture(&dirty_capture(), &canaries());
    assert_eq!(
        approve_release(&outcome.capture, &canaries(), false),
        Err(ReleaseRefusal::NoConsent),
        "a redacted report still must not leave without consent"
    );
    assert!(approve_release(&outcome.capture, &canaries(), true).is_ok());
}

#[test]
fn release_refuses_a_single_pass_capture() {
    // Defence in depth: a capture that bypassed pass 2 must not be shippable.
    let mut capture = redact_capture(&dirty_capture(), &canaries()).capture;
    capture.redaction_passes = 1;
    assert_eq!(
        approve_release(&capture, &canaries(), true),
        Err(ReleaseRefusal::InsufficientRedaction { passes: 1 })
    );
}

#[test]
fn release_refuses_residual_secrets_even_with_consent() {
    let mut capture = redact_capture(&dirty_capture(), &canaries()).capture;
    // Simulate a redaction bug that let a secret through.
    capture
        .log_ring
        .push(format!("leaked {CANARY_DB_PASSWORD}"));

    match approve_release(&capture, &canaries(), true) {
        Err(ReleaseRefusal::ResidualSecrets(found)) => {
            assert!(!found.is_empty());
        }
        other => panic!("expected a residual-secret refusal, got {other:?}"),
    }
}

#[test]
fn release_refusal_messages_name_the_problem() {
    let refusal = ReleaseRefusal::InsufficientRedaction { passes: 0 };
    assert!(refusal.to_string().contains("2 are required"));

    let refusal = ReleaseRefusal::NoConsent;
    assert!(refusal.to_string().contains("consent"));
}

#[test]
fn capture_round_trips_through_serde() {
    // A crash bundle is serialized to disk; redaction must survive that.
    let outcome = redact_capture(&dirty_capture(), &canaries());
    let json = serde_json::to_string(&outcome.capture).expect("serialize");
    assert!(
        !json.contains(CANARY_OPENAI) && !json.contains(CANARY_DB_PASSWORD),
        "the serialized bundle must not contain a secret"
    );
    let back: CrashCapture = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(outcome.capture, back);
}
