//! Liveness probes for the native client lanes (REQ-016, REQ-017, REQ-018).
//!
//! Each native lane is driven by a provider-owned binary that authenticates in
//! its own login flow (`AI_TRANSPORTS.md` rules 2 and 3). VECTOR therefore
//! cannot ask "is the user logged in?" through an API — it has to read what the
//! provider's own CLI reports.
//!
//! The parsing lives here, as pure functions over `(stdout, exit_ok)`, because
//! the *decision* is the part that must be tested: a probe that treated a
//! non-zero exit as healthy, or that looked for the wrong string, would report
//! a lane as available and then fail at the moment a learner asked a question.
//! Spawning the processes is left to the caller, which is also what keeps this
//! testable without any of the three CLIs installed.
//!
//! ## Flags used, and where they are documented
//!
//! * `codex login status` — `codex --help` lists `login`, and this subcommand is
//!   the documented way to report the active authentication method.
//! * `grok models` — `grok --help` lists `models: List available models and
//!   exit`. It needs credentials, so its output distinguishes the two states.
//! * `claude auth status` — `claude auth --help` lists `status: Show
//!   authentication status`, which emits JSON and exits non-zero when signed
//!   out.
//!
//! None of these three sends a prompt or bills anything.

use crate::transport::{AdapterHealth, PolicyRecord};

/// How long a caller should allow each probe before giving up.
pub const PROBE_TIMEOUT_SECONDS: u64 = 30;

/// Interpret `codex login status`.
pub fn parse_codex_login(stdout: &str, exit_ok: bool) -> AdapterHealth {
    let lowered = stdout.to_lowercase();
    if lowered.contains("not logged in") || lowered.contains("no credentials") {
        return AdapterHealth::Unavailable {
            reason: "codex is installed but not signed in".to_string(),
        };
    }
    if exit_ok && lowered.contains("logged in") {
        return AdapterHealth::Healthy;
    }
    AdapterHealth::Unavailable {
        reason: format!("codex did not report an authenticated session (exit ok: {exit_ok})"),
    }
}

/// Interpret `grok models`.
///
/// This command exits 0 whether or not the user is signed in, so the exit status
/// carries no information and the output is the only signal. Treating the exit
/// code as the verdict is exactly the mistake this function exists to prevent.
pub fn parse_grok_models(stdout: &str, _exit_ok: bool) -> AdapterHealth {
    let lowered = stdout.to_lowercase();
    if lowered.contains("not authenticated")
        || lowered.contains("no auth credentials")
        || lowered.contains("failed to fetch models")
    {
        return AdapterHealth::Unavailable {
            reason: "grok is installed but not signed in".to_string(),
        };
    }
    if lowered.contains("available models") {
        return AdapterHealth::Healthy;
    }
    AdapterHealth::Unavailable {
        reason: "grok did not report an authenticated model list".to_string(),
    }
}

/// Interpret `claude auth status`.
///
/// The command emits JSON with a `loggedIn` flag and exits non-zero when signed
/// out, so both signals are checked and they must agree.
pub fn parse_claude_auth(stdout: &str, exit_ok: bool) -> AdapterHealth {
    let trimmed = stdout.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        match value.get("loggedIn").and_then(serde_json::Value::as_bool) {
            Some(true) if exit_ok => return AdapterHealth::Healthy,
            Some(true) => {
                return AdapterHealth::Unavailable {
                    reason: "claude reports a session but exited with a failure".to_string(),
                }
            }
            Some(false) => {
                let method = value
                    .get("authMethod")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("none");
                return AdapterHealth::Unavailable {
                    reason: format!(
                        "claude is installed but not signed in (auth method: {method})"
                    ),
                };
            }
            None => {}
        }
    }

    let lowered = trimmed.to_lowercase();
    if lowered.contains("not logged in") {
        return AdapterHealth::Unavailable {
            reason: "claude is installed but not signed in".to_string(),
        };
    }
    AdapterHealth::Unavailable {
        reason: "claude did not report a parseable authentication status".to_string(),
    }
}

/// Age in days after which a provider-policy record is treated as stale.
///
/// REQ-058 and `AI_TRANSPORTS.md` rule 5 require an adapter to fail closed when
/// its terms have not been re-verified. Ninety days is the bound this
/// implementation applies; it is a policy constant, not a guess about any
/// provider's own notice period.
pub const MAX_POLICY_AGE_DAYS: i64 = 90;

/// Whether a policy record may still permit an adapter right now.
pub fn policy_is_usable(
    policy: &PolicyRecord,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    if !policy.permitted {
        return Err(format!(
            "provider terms prohibit this adapter: {}",
            policy.note
        ));
    }
    if !policy.is_fresh(now, MAX_POLICY_AGE_DAYS) {
        return Err(format!(
            "provider terms were last verified on {} which is older than {MAX_POLICY_AGE_DAYS} days",
            policy.terms_verified_on
        ));
    }
    Ok(())
}
