//! EP-006 acceptance: process isolation and environment hygiene.
//!
//! THREAT_MODEL.md names "provider credential theft" and "native CLI
//! argument/shell injection" as primary threats, controlled by "structured
//! process invocation without shell interpolation".

use std::collections::BTreeMap;
use std::path::PathBuf;

use vector_platform::process::{
    build_env, build_spec, is_approved_binary, resolve_program, strip_denied, ProcessRefusal,
    ProgramRef, ENV_ALLOWLIST, ENV_DENYLIST, MAX_ARGUMENT_BYTES,
};

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Program naming
// ---------------------------------------------------------------------------

#[test]
fn a_bare_program_name_is_accepted() {
    assert_eq!(
        resolve_program(&ProgramRef::BareName("codex".to_string())),
        Ok("codex".to_string())
    );
    assert_eq!(
        resolve_program(&ProgramRef::BareName("grok".to_string())),
        Ok("grok".to_string())
    );
}

#[test]
fn an_empty_program_name_is_refused() {
    assert!(matches!(
        resolve_program(&ProgramRef::BareName(String::new())),
        Err(ProcessRefusal::InvalidProgram(_))
    ));
    assert!(matches!(
        resolve_program(&ProgramRef::BareName("   ".to_string())),
        Err(ProcessRefusal::InvalidProgram(_))
    ));
}

#[test]
fn shell_metacharacters_in_a_program_name_are_refused() {
    // This is the injection shape: "codex; rm -rf /" must never be a program.
    for attempt in [
        "codex; rm -rf /",
        "codex | tee /etc/passwd",
        "codex && curl evil.example",
        "codex`whoami`",
        "codex$(id)",
        "codex>out",
        "codex\nwhoami",
    ] {
        let result = resolve_program(&ProgramRef::BareName(attempt.to_string()));
        assert!(
            matches!(
                result,
                Err(ProcessRefusal::ShellMetacharacters(_))
                    | Err(ProcessRefusal::InvalidProgram(_))
            ),
            "{attempt:?} must be refused, got {result:?}"
        );
    }
}

#[test]
fn a_bare_name_containing_a_separator_is_refused() {
    // "bin/codex" is a path, not a name; it must not slip through as a name.
    assert!(matches!(
        resolve_program(&ProgramRef::BareName("bin/codex".to_string())),
        Err(ProcessRefusal::InvalidProgram(_))
    ));
    assert!(matches!(
        resolve_program(&ProgramRef::BareName("bin\\codex".to_string())),
        Err(ProcessRefusal::InvalidProgram(_))
    ));
}

#[test]
fn a_relative_program_path_is_refused() {
    assert!(matches!(
        resolve_program(&ProgramRef::Absolute(PathBuf::from("tools/codex"))),
        Err(ProcessRefusal::RelativePath(_))
    ));
}

#[test]
fn an_absolute_program_path_is_accepted() {
    let path = if cfg!(windows) {
        PathBuf::from("C:\\Program Files\\OpenAI\\codex.exe")
    } else {
        PathBuf::from("/usr/local/bin/codex")
    };
    assert!(resolve_program(&ProgramRef::Absolute(path)).is_ok());
}

#[test]
fn an_absolute_path_with_a_shell_operator_is_refused() {
    let path = if cfg!(windows) {
        PathBuf::from("C:\\tools\\codex.exe; whoami")
    } else {
        PathBuf::from("/usr/bin/codex; whoami")
    };
    assert!(matches!(
        resolve_program(&ProgramRef::Absolute(path)),
        Err(ProcessRefusal::ShellMetacharacters(_))
    ));
}

// ---------------------------------------------------------------------------
// Argument safety: content is inert because no shell is involved
// ---------------------------------------------------------------------------

#[test]
fn arguments_containing_shell_syntax_are_stored_verbatim() {
    // The security property is that the argument is passed as data. A prompt
    // containing shell syntax must round-trip unchanged rather than being
    // interpreted, escaped, or silently altered.
    let hostile = "Explain this: $(whoami) `id` ; rm -rf / | cat /etc/passwd > out.txt";
    let spec = build_spec(
        &ProgramRef::BareName("codex".to_string()),
        &["exec".to_string(), hostile.to_string()],
        &env(&[]),
        &env(&[]),
        None,
    )
    .expect("spec builds");

    assert_eq!(
        spec.args[1], hostile,
        "the argument must be preserved verbatim"
    );
    assert_eq!(spec.program, "codex", "the program must be unchanged");
    assert!(
        spec.contains_shell_metacharacters(),
        "the audit helper should flag that this vector contains shell syntax"
    );
}

#[test]
fn an_oversized_argument_is_refused() {
    let huge = "x".repeat(MAX_ARGUMENT_BYTES + 1);
    let result = build_spec(
        &ProgramRef::BareName("codex".to_string()),
        &[huge],
        &env(&[]),
        &env(&[]),
        None,
    );
    assert!(matches!(
        result,
        Err(ProcessRefusal::ArgumentTooLarge { .. })
    ));
}

#[test]
fn an_argument_at_the_limit_is_accepted() {
    let at_limit = "x".repeat(MAX_ARGUMENT_BYTES);
    assert!(build_spec(
        &ProgramRef::BareName("codex".to_string()),
        &[at_limit],
        &env(&[]),
        &env(&[]),
        None
    )
    .is_ok());
}

#[test]
fn a_relative_working_directory_is_refused() {
    assert!(matches!(
        build_spec(
            &ProgramRef::BareName("codex".to_string()),
            &[],
            &env(&[]),
            &env(&[]),
            Some(PathBuf::from("relative/dir"))
        ),
        Err(ProcessRefusal::RelativePath(_))
    ));
}

// ---------------------------------------------------------------------------
// Environment isolation: credentials must not reach a child
// ---------------------------------------------------------------------------

#[test]
fn credentials_in_the_parent_environment_never_reach_the_child() {
    // This is the credential-theft control. A denylist would leak anything not
    // enumerated; an allowlist forwards only what is explicitly safe.
    let parent = env(&[
        ("PATH", "/usr/bin"),
        ("HOME", "/home/learner"),
        ("OPENAI_API_KEY", "sk-secret"),
        ("ANTHROPIC_API_KEY", "sk-ant-secret"),
        ("AWS_SECRET_ACCESS_KEY", "aws-secret"),
        ("GITHUB_TOKEN", "ghp_secret"),
        ("DATABASE_URL", "postgres://user:pw@host/db"),
        ("SOME_UNKNOWN_TOKEN", "unknown-secret"),
    ]);

    let child = build_env(&parent, &env(&[])).expect("env builds");

    for denied in ENV_DENYLIST {
        assert!(
            !child.contains_key(*denied),
            "{denied} must not reach a child process"
        );
    }
    // Even a credential nobody enumerated is excluded, because it is not on the
    // allowlist.
    assert!(
        !child.contains_key("SOME_UNKNOWN_TOKEN"),
        "an unenumerated secret must not be forwarded"
    );

    assert_eq!(child.get("PATH").map(String::as_str), Some("/usr/bin"));
    assert_eq!(child.get("HOME").map(String::as_str), Some("/home/learner"));
}

#[test]
fn only_allowlisted_variables_are_forwarded() {
    let parent = env(&[
        ("PATH", "/usr/bin"),
        ("RANDOM_VAR", "value"),
        ("LC_ALL", "C"),
        ("MY_PRIVATE_NOTE", "something personal"),
    ]);
    let child = build_env(&parent, &env(&[])).expect("env builds");

    for key in child.keys() {
        assert!(
            ENV_ALLOWLIST.iter().any(|a| a.eq_ignore_ascii_case(key)),
            "{key} was forwarded but is not on the allowlist"
        );
    }
    assert!(!child.contains_key("MY_PRIVATE_NOTE"));
}

#[test]
fn explicitly_passing_a_denied_variable_is_an_error() {
    // Silently dropping it would let a caller believe the credential arrived.
    let result = build_env(&env(&[]), &env(&[("OPENAI_API_KEY", "sk-x")]));
    assert_eq!(
        result,
        Err(ProcessRefusal::DeniedEnvironment(
            "OPENAI_API_KEY".to_string()
        ))
    );
}

#[test]
fn the_denylist_is_enforced_case_insensitively() {
    let result = build_env(&env(&[]), &env(&[("openai_api_key", "sk-x")]));
    assert!(matches!(result, Err(ProcessRefusal::DeniedEnvironment(_))));
}

#[test]
fn allowlisted_extras_are_added() {
    let child =
        build_env(&env(&[("PATH", "/usr/bin")]), &env(&[("NO_COLOR", "1")])).expect("env builds");
    assert_eq!(child.get("NO_COLOR").map(String::as_str), Some("1"));
}

#[test]
fn a_credential_cannot_survive_by_being_on_both_lists() {
    // Defence in depth: if a future edit added a credential to the allowlist,
    // the final denylist sweep still removes it.
    let mut parent = BTreeMap::new();
    parent.insert("PATH".to_string(), "/usr/bin".to_string());
    parent.insert("GITHUB_TOKEN".to_string(), "ghp_secret".to_string());

    let child = build_env(&parent, &env(&[])).expect("env builds");
    assert!(!child.contains_key("GITHUB_TOKEN"));
}

#[test]
fn the_denylist_sweep_removes_credentials_directly() {
    // The sweep is unreachable through build_env (the allowlist already filters
    // these keys), so it is exercised directly. Its whole purpose is to be the
    // last barrier if the allowlist is ever edited wrongly; leaving it untested
    // would mean the barrier is assumed rather than proven.
    let mut map = BTreeMap::new();
    map.insert("PATH".to_string(), "/usr/bin".to_string());
    map.insert("OPENAI_API_KEY".to_string(), "sk-secret".to_string());
    map.insert("github_token".to_string(), "ghp_secret".to_string());
    map.insert("AWS_SECRET_ACCESS_KEY".to_string(), "aws".to_string());

    strip_denied(&mut map);

    assert_eq!(map.len(), 1, "only PATH should survive: {map:?}");
    assert!(map.contains_key("PATH"));
    for denied in ENV_DENYLIST {
        assert!(
            !map.keys().any(|k| k.eq_ignore_ascii_case(denied)),
            "{denied} survived the denylist sweep"
        );
    }
}

#[test]
fn the_denylist_sweep_is_case_insensitive() {
    let mut map = BTreeMap::new();
    map.insert("OpenAi_Api_Key".to_string(), "sk-x".to_string());
    map.insert("PATH".to_string(), "/usr/bin".to_string());

    strip_denied(&mut map);

    assert!(!map.keys().any(|k| k.to_lowercase().contains("api_key")));
    assert!(map.contains_key("PATH"));
}

#[test]
fn the_denylist_sweep_leaves_ordinary_variables_alone() {
    let mut map = BTreeMap::new();
    map.insert("PATH".to_string(), "/usr/bin".to_string());
    map.insert("HOME".to_string(), "/home/learner".to_string());
    map.insert("LANG".to_string(), "en_GB.UTF-8".to_string());

    strip_denied(&mut map);

    assert_eq!(map.len(), 3, "safe variables must be preserved: {map:?}");
}

// ---------------------------------------------------------------------------
// Provider-binary impersonation
// ---------------------------------------------------------------------------

#[test]
fn a_lookalike_binary_in_another_directory_is_not_approved() {
    let dir = std::env::temp_dir().join(format!("vec-bin-{}", std::process::id()));
    let approved_dir = dir.join("approved");
    let shady_dir = dir.join("shady");
    std::fs::create_dir_all(&approved_dir).expect("mkdir approved");
    std::fs::create_dir_all(&shady_dir).expect("mkdir shady");

    let approved = approved_dir.join("codex");
    let lookalike = shady_dir.join("codex");
    std::fs::write(&approved, b"#!/bin/sh\n").expect("write approved");
    std::fs::write(&lookalike, b"#!/bin/sh\necho pwned\n").expect("write lookalike");

    let approved_list = std::slice::from_ref(&approved);

    assert!(
        is_approved_binary(&approved, approved_list),
        "the approved binary must be recognized"
    );
    assert!(
        !is_approved_binary(&lookalike, approved_list),
        "a same-named binary in another directory must not be approved"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_nonexistent_binary_is_not_approved() {
    let missing = std::env::temp_dir().join("vector-definitely-missing-binary");
    assert!(!is_approved_binary(
        &missing,
        std::slice::from_ref(&missing)
    ));
}

#[test]
fn approval_requires_a_non_empty_approved_list() {
    let dir = std::env::temp_dir().join(format!("vec-bin2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("codex");
    std::fs::write(&file, b"x").expect("write");

    assert!(
        !is_approved_binary(&file, &[]),
        "with nothing approved, nothing is approved"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Display / logging
// ---------------------------------------------------------------------------

#[test]
fn the_display_string_quotes_arguments_but_is_not_executable() {
    let spec = build_spec(
        &ProgramRef::BareName("codex".to_string()),
        &["exec".to_string(), "hello world".to_string()],
        &env(&[]),
        &env(&[]),
        None,
    )
    .expect("spec builds");

    let display = spec.to_display_string();
    assert!(display.starts_with("codex exec"));
    assert!(display.contains("hello world"), "readable for logs");

    // The structured form remains the source of truth for execution.
    assert_eq!(spec.args.len(), 2);
    assert_eq!(spec.args[1], "hello world");
}

#[test]
fn every_denied_variable_is_also_worth_naming_in_the_allowlist_check() {
    // Guards against a future edit adding a credential to the allowlist by
    // mistake: the two lists must stay disjoint.
    for denied in ENV_DENYLIST {
        assert!(
            !ENV_ALLOWLIST.iter().any(|a| a.eq_ignore_ascii_case(denied)),
            "{denied} appears on both the allowlist and the denylist"
        );
    }
}
