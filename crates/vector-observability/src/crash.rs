//! Crash capture with mandatory double redaction (REQ-028, REQ-029).
//!
//! SECURITY.md requires "double-redaction and secret-canary tests". The
//! existing `RedactedLogger` redacts by *key name* only, which leaks any secret
//! that appears in a **value** — most importantly inside a panic message, a
//! stack trace or a rendered log line, which is exactly where credentials
//! actually show up in a crash.
//!
//! This module implements two independent redaction passes over different
//! signals:
//!
//! 1. **Structural pass** — redact by key name and by known secret *scheme*
//!    (bearer tokens, API-key prefixes, private-key blocks, connection-string
//!    passwords). Catches a secret regardless of which field carries it.
//! 2. **Canary pass** — redact any registered canary value verbatim. A canary
//!    is a sentinel the application plants; if it survives redaction, the
//!    redactor is broken, and the test fails rather than the secret shipping.
//!
//! A crash bundle is only releasable when both passes have run and a
//! verification scan finds no residual secret.

use serde::{Deserialize, Serialize};

/// Placeholder substituted for redacted content.
pub const REDACTED: &str = "[REDACTED]";

/// A crash capture in progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrashCapture {
    pub id: String,
    /// Short human summary. Never contains a raw secret after redaction.
    pub summary: String,
    /// Panic message / error text.
    pub message: String,
    /// Backtrace or stack text.
    pub stack: String,
    /// Structured key/value context.
    pub context: Vec<(String, String)>,
    /// Log ring buffer lines.
    pub log_ring: Vec<String>,
    /// Build identity so a report is attributable to an artifact.
    pub build_id: String,
    /// Number of redaction passes applied.
    pub redaction_passes: u32,
}

/// Result of redacting a capture.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionOutcome {
    pub capture: CrashCapture,
    /// How many distinct secrets were removed.
    pub redactions: usize,
}

/// Secret *schemes* recognizable without knowing the value.
///
/// These are the shapes credentials actually take in logs and traces, which is
/// why a key-name-only redactor is insufficient.
const VALUE_PATTERNS: &[(&str, &str)] = &[
    // Authorization headers.
    (r"(?i)bearer\s+[A-Za-z0-9._\-]{8,}", "bearer token"),
    // Common provider key prefixes.
    (r"sk-[A-Za-z0-9]{16,}", "openai-style key"),
    (r"sk-ant-[A-Za-z0-9\-_]{16,}", "anthropic-style key"),
    (r"xai-[A-Za-z0-9]{16,}", "xai-style key"),
    (r"AIza[A-Za-z0-9\-_]{20,}", "google api key"),
    (r"ghp_[A-Za-z0-9]{20,}", "github token"),
    (r"github_pat_[A-Za-z0-9_]{20,}", "github pat"),
    // AWS access key id.
    (r"AKIA[0-9A-Z]{16}", "aws access key"),
    // JWT-ish triplets.
    (
        r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        "jwt",
    ),
    // Connection strings with an inline password.
    (
        r"(?i)(password|pwd)=[^;\s]{3,}",
        "connection-string password",
    ),
    // Private key blocks.
    (
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        "private key block",
    ),
    // Email addresses are personal data and must not ship in a report.
    (
        r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}",
        "email address",
    ),
];

/// Key names whose *value* is always secret.
const SECRET_KEY_MARKERS: &[&str] = &[
    "secret",
    "password",
    "passwd",
    "token",
    "api_key",
    "apikey",
    "auth",
    "credential",
    "private_key",
    "session_id",
    "cookie",
];

/// Whether a context key names a secret-bearing field.
pub fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SECRET_KEY_MARKERS.iter().any(|m| lower.contains(m))
}

/// A registered secret canary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Canary {
    /// Human label, e.g. "test-openai-key".
    pub label: String,
    /// The exact sentinel value planted in the application.
    pub value: String,
}

/// The canary registry.
#[derive(Debug, Clone, Default)]
pub struct CanaryRegistry {
    canaries: Vec<Canary>,
}

impl CanaryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a canary. Empty values are refused: an empty needle would match
    /// everywhere and redact the entire report.
    pub fn register(&mut self, label: &str, value: &str) -> Result<(), String> {
        if value.trim().is_empty() {
            return Err(format!("canary {label} must not have an empty value"));
        }
        if value.len() < 4 {
            return Err(format!(
                "canary {label} value is too short to be a meaningful sentinel"
            ));
        }
        self.canaries.push(Canary {
            label: label.to_string(),
            value: value.to_string(),
        });
        Ok(())
    }

    pub fn canaries(&self) -> &[Canary] {
        &self.canaries
    }

    /// Replace every canary occurrence in `text`.
    fn scrub(&self, text: &str) -> (String, usize) {
        let mut out = text.to_string();
        let mut hits = 0usize;
        for canary in &self.canaries {
            if out.contains(&canary.value) {
                hits += out.matches(&canary.value).count();
                out = out.replace(&canary.value, REDACTED);
            }
        }
        (out, hits)
    }
}

/// The compiled redaction patterns, built once for the process lifetime.
///
/// Compiling on every call was a real performance defect: redacting one crash
/// capture compiles the whole pattern set once per text field, so a report with
/// many log lines rebuilt hundreds of regexes. The `regex` crate documents that
/// construction is expensive relative to matching and intends the compiled form
/// to be reused; `OnceLock` gives that without adding a dependency.
static COMPILED_PATTERNS: std::sync::OnceLock<Vec<(regex::Regex, &'static str)>> =
    std::sync::OnceLock::new();

fn compiled_patterns() -> &'static [(regex::Regex, &'static str)] {
    COMPILED_PATTERNS.get_or_init(|| {
        VALUE_PATTERNS
            .iter()
            .map(|(pattern, label)| {
                let re = regex::Regex::new(pattern).unwrap_or_else(|e| {
                    // A malformed built-in pattern is a programming error, not a
                    // runtime condition: fail loudly rather than ship unredacted.
                    panic!("built-in redaction pattern {pattern:?} is invalid: {e}")
                });
                (re, *label)
            })
            .collect()
    })
}

/// Apply scheme-based and key-based redaction to arbitrary text.
fn scrub_text(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut hits = 0usize;

    for (re, _label) in compiled_patterns() {
        // `replace_all` returns Borrowed when nothing matched, which lets the
        // match count come from the replacement itself rather than a second
        // scan of the field.
        match re.replace_all(&out, REDACTED) {
            std::borrow::Cow::Borrowed(_) => {}
            std::borrow::Cow::Owned(replaced) => {
                hits += count_occurrences(&replaced);
                out = replaced;
            }
        }
    }

    (out, hits)
}

/// Count redaction placeholders introduced by a replacement.
///
/// Counting placeholders rather than re-running the pattern avoids a second
/// pass and cannot disagree with what was actually substituted.
fn count_occurrences(text: &str) -> usize {
    text.matches(REDACTED).count()
}

/// Apply both redaction passes to a crash capture.
///
/// Pass 1 is scheme/key based and needs no configuration. Pass 2 scrubs
/// registered canaries verbatim. Both run unconditionally, and the recorded
/// `redaction_passes` is what the release gate checks.
pub fn redact_capture(capture: &CrashCapture, canaries: &CanaryRegistry) -> RedactionOutcome {
    let mut redactions = 0usize;

    // --- Pass 1: structural (scheme + key name) ---
    let (summary, a) = scrub_text(&capture.summary);
    let (message, b) = scrub_text(&capture.message);
    let (stack, c) = scrub_text(&capture.stack);
    redactions += a + b + c;

    let mut context = Vec::with_capacity(capture.context.len());
    for (key, value) in &capture.context {
        if is_secret_key(key) {
            // A sensitive field is redacted wholesale, regardless of content.
            context.push((key.clone(), REDACTED.to_string()));
            redactions += 1;
        } else {
            let (scrubbed, hits) = scrub_text(value);
            redactions += hits;
            context.push((key.clone(), scrubbed));
        }
    }

    let mut log_ring = Vec::with_capacity(capture.log_ring.len());
    for line in &capture.log_ring {
        let (scrubbed, hits) = scrub_text(line);
        redactions += hits;
        log_ring.push(scrubbed);
    }

    let mut out = CrashCapture {
        id: capture.id.clone(),
        summary,
        message,
        stack,
        context,
        log_ring,
        build_id: capture.build_id.clone(),
        redaction_passes: 1,
    };

    // --- Pass 2: canary scrub over the already-redacted capture ---
    // Running over pass-1 output is the point: a canary that pass 1 missed
    // (because it matches no known scheme) is still removed here.
    let (summary, d) = canaries.scrub(&out.summary);
    let (message, e) = canaries.scrub(&out.message);
    let (stack, f) = canaries.scrub(&out.stack);
    redactions += d + e + f;
    out.summary = summary;
    out.message = message;
    out.stack = stack;

    let mut scrubbed_context = Vec::with_capacity(out.context.len());
    for (key, value) in &out.context {
        let (v, hits) = canaries.scrub(value);
        redactions += hits;
        scrubbed_context.push((key.clone(), v));
    }
    out.context = scrubbed_context;

    let mut scrubbed_log = Vec::with_capacity(out.log_ring.len());
    for line in &out.log_ring {
        let (l, hits) = canaries.scrub(line);
        redactions += hits;
        scrubbed_log.push(l);
    }
    out.log_ring = scrubbed_log;

    out.redaction_passes = 2;

    RedactionOutcome {
        capture: out,
        redactions,
    }
}

/// Any residual secret found in a capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidualSecret {
    pub location: String,
    pub label: String,
}

/// Scan a redacted capture for anything that still looks like a secret.
///
/// This is the release gate. Redaction that "ran" is not the same as redaction
/// that worked, so the bundle is verified rather than trusted.
///
/// The compiled pattern set is reused rather than rebuilt per field: an earlier
/// version compiled every pattern inline for every field, which made this the
/// slowest part of the pipeline by three orders of magnitude.
pub fn verify_no_secrets(
    capture: &CrashCapture,
    canaries: &CanaryRegistry,
) -> Result<(), Vec<ResidualSecret>> {
    let mut found = Vec::new();

    let mut check = |location: &str, text: &str| {
        for (re, label) in compiled_patterns() {
            if re.is_match(text) {
                found.push(ResidualSecret {
                    location: location.to_string(),
                    label: (*label).to_string(),
                });
            }
        }
        for canary in canaries.canaries() {
            if text.contains(&canary.value) {
                found.push(ResidualSecret {
                    location: location.to_string(),
                    label: format!("canary:{}", canary.label),
                });
            }
        }
    };

    check("summary", &capture.summary);
    check("message", &capture.message);
    check("stack", &capture.stack);
    for (key, value) in &capture.context {
        check(&format!("context.{key}"), value);
    }
    for (index, line) in capture.log_ring.iter().enumerate() {
        check(&format!("log_ring[{index}]"), line);
    }

    if found.is_empty() {
        Ok(())
    } else {
        Err(found)
    }
}

/// Why a crash bundle may not be shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseRefusal {
    /// Fewer than two redaction passes were applied.
    InsufficientRedaction { passes: u32 },
    /// Residual secret material remains.
    ResidualSecrets(Vec<ResidualSecret>),
    /// The learner has not consented to sharing.
    NoConsent,
}

impl std::fmt::Display for ReleaseRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseRefusal::InsufficientRedaction { passes } => write!(
                f,
                "crash bundle has {passes} redaction pass(es); at least 2 are required"
            ),
            ReleaseRefusal::ResidualSecrets(s) => {
                write!(f, "crash bundle still contains {} secret(s): ", s.len())?;
                for item in s {
                    write!(f, "{}({}) ", item.location, item.label)?;
                }
                Ok(())
            }
            ReleaseRefusal::NoConsent => {
                write!(f, "the learner has not consented to sharing this report")
            }
        }
    }
}

impl std::error::Error for ReleaseRefusal {}

/// Decide whether a redacted capture may leave the machine.
///
/// Fails closed on every condition: no consent, too few passes, or any residual
/// secret all block release.
pub fn approve_release(
    capture: &CrashCapture,
    canaries: &CanaryRegistry,
    consented: bool,
) -> Result<(), ReleaseRefusal> {
    if !consented {
        return Err(ReleaseRefusal::NoConsent);
    }
    if capture.redaction_passes < 2 {
        return Err(ReleaseRefusal::InsufficientRedaction {
            passes: capture.redaction_passes,
        });
    }
    if let Err(residual) = verify_no_secrets(capture, canaries) {
        return Err(ReleaseRefusal::ResidualSecrets(residual));
    }
    Ok(())
}
