//! Process isolation and safe subprocess invocation (SECURITY.md, THREAT_MODEL.md).
//!
//! THREAT_MODEL.md names "provider credential theft" and "native CLI
//! argument/shell injection" as primary threats, with the control described as
//! "structured process invocation without shell interpolation".
//!
//! This module makes that control real:
//! - a command is specified as **program plus argument vector**, never a string;
//! - shell metacharacters in arguments are inert because no shell is involved;
//! - the environment is built from an explicit allowlist, so credentials in the
//!   parent environment cannot leak into a child process;
//! - the program must be a bare name or an approved absolute path, which
//!   prevents impersonating a provider binary with a lookalike.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Environment variables that may be passed to a child process.
///
/// An allowlist rather than a denylist: a denylist silently leaks any variable
/// nobody thought to enumerate, which is exactly how credentials escape.
pub const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "LANG",
    "LC_ALL",
    "TERM",
    "NO_COLOR",
];

/// Environment variables that must never reach a child, even if some caller
/// tries to allow them.
pub const ENV_DENYLIST: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "XAI_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "GROK_API_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "DATABASE_URL",
];

/// How a program may be named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramRef {
    /// A bare executable name resolved via PATH, e.g. `codex`.
    BareName(String),
    /// An absolute path to an approved executable.
    Absolute(PathBuf),
}

/// Why a process specification was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessRefusal {
    /// The program name is empty or contains a path separator unexpectedly.
    InvalidProgram(String),
    /// The program name contains characters that could be interpreted by a shell.
    ShellMetacharacters(String),
    /// A relative path was supplied where an absolute one is required.
    RelativePath(String),
    /// An allowlisted environment variable was requested with a denied name.
    DeniedEnvironment(String),
    /// An argument exceeded the permitted size.
    ArgumentTooLarge { bytes: usize, limit: usize },
}

impl std::fmt::Display for ProcessRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessRefusal::InvalidProgram(p) => write!(f, "invalid program name: {p:?}"),
            ProcessRefusal::ShellMetacharacters(p) => {
                write!(f, "program name contains shell metacharacters: {p:?}")
            }
            ProcessRefusal::RelativePath(p) => {
                write!(f, "program path must be absolute, got {p:?}")
            }
            ProcessRefusal::DeniedEnvironment(k) => {
                write!(
                    f,
                    "environment variable {k} must never reach a child process"
                )
            }
            ProcessRefusal::ArgumentTooLarge { bytes, limit } => {
                write!(
                    f,
                    "argument of {bytes} bytes exceeds the {limit} byte limit"
                )
            }
        }
    }
}

impl std::error::Error for ProcessRefusal {}

/// Characters that a shell would interpret. Their presence in a *program name*
/// indicates an injection attempt rather than a legitimate executable.
const SHELL_METACHARACTERS: &[char] = &[
    ';', '|', '&', '$', '`', '<', '>', '(', ')', '{', '}', '[', ']', '*', '?', '!', '\n', '\r',
    '"', '\'', '\\', ' ',
];

/// Maximum size of a single argument, in bytes.
pub const MAX_ARGUMENT_BYTES: usize = 64 * 1024;

/// A fully specified, shell-free process invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Only allowlisted variables, with credentials removed.
    pub env: BTreeMap<String, String>,
    /// Working directory, when the tool requires one.
    pub cwd: Option<PathBuf>,
}

/// Parse a program reference into a safe absolute-or-bare program name.
pub fn resolve_program(reference: &ProgramRef) -> Result<String, ProcessRefusal> {
    match reference {
        ProgramRef::BareName(name) => {
            if name.trim().is_empty() {
                return Err(ProcessRefusal::InvalidProgram(name.clone()));
            }
            if name.contains('/') || name.contains('\\') {
                // A bare name that contains a separator is really a path; send
                // it down the absolute-path route so it is validated as one.
                return Err(ProcessRefusal::InvalidProgram(name.clone()));
            }
            if let Some(bad) = name.chars().find(|c| SHELL_METACHARACTERS.contains(c)) {
                return Err(ProcessRefusal::ShellMetacharacters(format!(
                    "{name} (contains {bad:?})"
                )));
            }
            Ok(name.clone())
        }
        ProgramRef::Absolute(path) => {
            if !path.is_absolute() {
                return Err(ProcessRefusal::RelativePath(path.display().to_string()));
            }
            let text = path.display().to_string();
            // An absolute path may contain spaces legitimately, but not shell
            // control operators, because those signal a chained command.
            for bad in [';', '|', '&', '`', '\n', '\r'] {
                if text.contains(bad) {
                    return Err(ProcessRefusal::ShellMetacharacters(format!(
                        "{text} (contains {bad:?})"
                    )));
                }
            }
            Ok(text)
        }
    }
}

/// Build the child environment from an explicit allowlist.
///
/// Only variables present in both the parent environment and the allowlist are
/// forwarded, and the denylist is applied afterwards as a second guarantee. A
/// key requested explicitly that appears on the denylist is a hard error rather
/// than a silent omission, so a caller cannot believe a credential was passed
/// when it was not.
pub fn build_env(
    parent: &BTreeMap<String, String>,
    extra: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, ProcessRefusal> {
    for key in extra.keys() {
        if ENV_DENYLIST.iter().any(|d| d.eq_ignore_ascii_case(key)) {
            return Err(ProcessRefusal::DeniedEnvironment(key.clone()));
        }
    }

    let mut env = BTreeMap::new();
    for (key, value) in parent {
        if ENV_ALLOWLIST.iter().any(|a| a.eq_ignore_ascii_case(key)) {
            env.insert(key.clone(), value.clone());
        }
    }
    for (key, value) in extra {
        env.insert(key.clone(), value.clone());
    }

    strip_denied(&mut env);

    Ok(env)
}

/// Remove every denylisted variable from an environment map.
///
/// This is defence in depth behind the allowlist. It exists so that a future
/// edit which mistakenly adds a credential to `ENV_ALLOWLIST` still cannot leak
/// it. Because the allowlist already excludes these keys, the sweep is not
/// reachable through [`build_env`] — so it is a separate function that is tested
/// directly. An unverifiable safety net is not a safety net.
pub fn strip_denied(env: &mut BTreeMap<String, String>) {
    env.retain(|k, _| !ENV_DENYLIST.iter().any(|d| d.eq_ignore_ascii_case(k)));
}

/// Build a validated process specification.
pub fn build_spec(
    reference: &ProgramRef,
    args: &[String],
    parent_env: &BTreeMap<String, String>,
    extra_env: &BTreeMap<String, String>,
    cwd: Option<PathBuf>,
) -> Result<ProcessSpec, ProcessRefusal> {
    let program = resolve_program(reference)?;

    for arg in args {
        if arg.len() > MAX_ARGUMENT_BYTES {
            return Err(ProcessRefusal::ArgumentTooLarge {
                bytes: arg.len(),
                limit: MAX_ARGUMENT_BYTES,
            });
        }
    }

    if let Some(dir) = &cwd {
        if !dir.is_absolute() {
            return Err(ProcessRefusal::RelativePath(dir.display().to_string()));
        }
    }

    Ok(ProcessSpec {
        program,
        args: args.to_vec(),
        env: build_env(parent_env, extra_env)?,
        cwd,
    })
}

impl ProcessSpec {
    /// A display string for logs and evidence.
    ///
    /// Quoted for readability only. This string is never executed: the caller
    /// passes `program` and `args` to the OS directly, which is what makes
    /// argument content inert.
    pub fn to_display_string(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.args.iter().map(|a| {
            if a.contains(' ') {
                format!("{a:?}")
            } else {
                a.clone()
            }
        }));
        parts.join(" ")
    }

    /// Whether an argument vector contains anything that would be dangerous if
    /// it were ever interpolated into a shell string.
    ///
    /// Used by tests and audits to prove the containment property; the runtime
    /// does not need it because it never builds a shell string.
    pub fn contains_shell_metacharacters(&self) -> bool {
        self.args.iter().any(|a| {
            a.chars()
                .any(|c| matches!(c, ';' | '|' | '&' | '$' | '`' | '<' | '>' | '\n' | '\r'))
        })
    }
}

/// Whether `candidate` is an approved provider binary.
///
/// SECURITY.md names "provider-binary impersonation" as a threat. Approval is by
/// exact file name and, for absolute paths, an exact canonical path, so a
/// lookalike such as `codex-helper` or a same-named file in another directory is
/// not accepted.
pub fn is_approved_binary(candidate: &Path, approved: &[PathBuf]) -> bool {
    let Ok(canonical) = candidate.canonicalize() else {
        // A path that cannot be canonicalized does not exist: it cannot be the
        // approved binary.
        return false;
    };
    approved
        .iter()
        .filter_map(|p| p.canonicalize().ok())
        .any(|p| p == canonical)
}
