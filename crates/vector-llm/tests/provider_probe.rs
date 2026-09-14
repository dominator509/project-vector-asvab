//! EP-004 acceptance: native client lane health and policy freshness
//! (REQ-016, REQ-017, REQ-018, REQ-058).
//!
//! Each native lane reports its own authentication state through its own CLI, so
//! the verdict is only as good as the parsing. These tests pin the strings the
//! providers actually emit — captured from the installed CLIs — and the
//! ambiguous cases: a command that exits 0 while reporting that it is not
//! authenticated is the trap this file exists to cover.

use vector_llm::probe::{
    parse_claude_auth, parse_codex_login, parse_grok_models, policy_is_usable, MAX_POLICY_AGE_DAYS,
};
use vector_llm::transport::{AdapterHealth, PolicyRecord};

fn unavailable_reason(health: AdapterHealth) -> String {
    match health {
        AdapterHealth::Unavailable { reason } => reason,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// codex: `codex login status`
// ---------------------------------------------------------------------------

#[test]
fn codex_reports_a_chatgpt_session_as_healthy() {
    // Captured from `codex login status` on an authenticated machine.
    let health = parse_codex_login("Logged in using ChatGPT\n", true);
    assert_eq!(health, AdapterHealth::Healthy);
}

#[test]
fn codex_signed_out_is_unavailable_not_healthy() {
    let health = parse_codex_login("Not logged in\n", true);
    assert!(unavailable_reason(health).contains("not signed in"));
}

#[test]
fn codex_with_no_credentials_is_unavailable() {
    let health = parse_codex_login("No credentials found. Run `codex login`.\n", false);
    assert!(unavailable_reason(health).contains("not signed in"));
}

#[test]
fn codex_output_that_says_nothing_usable_is_unavailable() {
    // Silence, or output from a different command, must not read as healthy.
    for (text, ok) in [
        ("", true),
        ("usage: codex ...\n", true),
        ("Logged in\n", false),
    ] {
        let health = parse_codex_login(text, ok);
        assert!(
            !matches!(health, AdapterHealth::Healthy),
            "{text:?} (exit ok {ok}) must not be healthy"
        );
    }
}

// ---------------------------------------------------------------------------
// grok: `grok models`
// ---------------------------------------------------------------------------

#[test]
fn grok_authenticated_reports_models() {
    let health = parse_grok_models(
        "Default model: grok-4.6\n\nAvailable models:\n  * grok-4.6 (default)\n  - grok-4.5\n",
        true,
    );
    assert_eq!(health, AdapterHealth::Healthy);
}

#[test]
fn grok_signed_out_is_unavailable_even_though_the_command_succeeds() {
    // This is the whole reason the exit code is ignored: `grok models` exits 0
    // while reporting that it is not authenticated and listing cached model
    // names. Trusting the exit status would advertise a lane that cannot answer.
    let health = parse_grok_models(
        "You are not authenticated.\n\n\
         2026-09-14T17:15:17Z  WARN Failed to fetch models: Auth(\"No auth credentials for cli-chat-proxy\")\n\n\
         Default model: grok-4.6\n\nAvailable models:\n  * grok-4.6 (default)\n",
        true,
    );
    assert!(unavailable_reason(health).contains("not signed in"));
}

#[test]
fn grok_output_without_a_model_list_is_unavailable() {
    let health = parse_grok_models("grok 1.0.5\n", true);
    assert!(unavailable_reason(health).contains("did not report"));
}

// ---------------------------------------------------------------------------
// claude: `claude auth status`
// ---------------------------------------------------------------------------

#[test]
fn claude_signed_in_json_is_healthy() {
    let health = parse_claude_auth(
        r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty"}"#,
        true,
    );
    assert_eq!(health, AdapterHealth::Healthy);
}

#[test]
fn claude_signed_out_json_is_unavailable_and_names_the_method() {
    // Captured from `claude auth status` while signed out.
    let health = parse_claude_auth(
        "{\n  \"loggedIn\": false,\n  \"authMethod\": \"none\",\n  \"apiProvider\": \"firstParty\"\n}\n",
        false,
    );
    let reason = unavailable_reason(health);
    assert!(reason.contains("not signed in"), "got {reason}");
    assert!(reason.contains("none"), "the auth method must be reported");
}

#[test]
fn claude_claiming_a_session_while_failing_is_not_healthy() {
    // The two signals disagreeing is a real failure state, not a success.
    let health = parse_claude_auth(r#"{"loggedIn":true}"#, false);
    assert!(unavailable_reason(health).contains("exited with a failure"));
}

#[test]
fn claude_non_json_output_falls_back_to_the_text_signal() {
    let health = parse_claude_auth("Not logged in · Please run /login\n", false);
    assert!(unavailable_reason(health).contains("not signed in"));

    let health = parse_claude_auth("something entirely unexpected", true);
    assert!(unavailable_reason(health).contains("parseable"));
}

// ---------------------------------------------------------------------------
// Policy freshness (REQ-058)
// ---------------------------------------------------------------------------

fn record(verified: &str, permitted: bool) -> PolicyRecord {
    PolicyRecord {
        terms_verified_on: verified.to_string(),
        permitted,
        note: "reviewed".to_string(),
    }
}

fn at(date: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(&format!("{date}T00:00:00Z"))
        .expect("valid date")
        .with_timezone(&chrono::Utc)
}

#[test]
fn a_fresh_permitted_policy_is_usable() {
    assert!(policy_is_usable(&record("2026-01-01", true), at("2026-02-01")).is_ok());
}

#[test]
fn a_policy_older_than_the_bound_fails_closed() {
    let now = at("2026-06-01");
    let stale = record("2026-01-01", true);
    let error = policy_is_usable(&stale, now).expect_err("stale policy must refuse");
    assert!(error.contains("older than"), "got {error}");

    // Exactly at the bound is still usable; one day past it is not.
    let boundary = now - chrono::Duration::days(MAX_POLICY_AGE_DAYS);
    let on_bound = record(&boundary.format("%Y-%m-%d").to_string(), true);
    assert!(policy_is_usable(&on_bound, now).is_ok());

    let past = now - chrono::Duration::days(MAX_POLICY_AGE_DAYS + 1);
    let past_bound = record(&past.format("%Y-%m-%d").to_string(), true);
    assert!(policy_is_usable(&past_bound, now).is_err());
}

#[test]
fn a_prohibited_policy_refuses_regardless_of_freshness() {
    let error = policy_is_usable(&record("2026-05-31", false), at("2026-06-01"))
        .expect_err("prohibited must refuse");
    assert!(error.contains("prohibit"), "got {error}");
}

#[test]
fn a_future_dated_verification_is_malformed_not_fresh() {
    // A record dated in the future cannot have been produced by a real review,
    // so it must not be accepted as evidence that the terms were checked.
    let error = policy_is_usable(&record("2027-01-01", true), at("2026-06-01"))
        .expect_err("a future date must refuse");
    assert!(error.contains("older than"), "got {error}");
}

#[test]
fn an_unparseable_date_refuses() {
    for date in ["", "yesterday", "01/02/2026", "2026-13-45"] {
        assert!(
            policy_is_usable(&record(date, true), at("2026-06-01")).is_err(),
            "{date:?} must not be accepted"
        );
    }
}
