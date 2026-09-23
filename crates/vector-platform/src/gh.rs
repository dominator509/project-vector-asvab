//! The pull-request lane: the official `gh` CLI, an explicit approval, and no way to merge.
//!
//! REQ-032 asks for "official gh; explicit approval; no auto-merge", and until now only the
//! bookkeeping half existed: `PullRequestRepo` records a proposal, refuses to mark it merged
//! without a named approver, and the schema makes an unapproved-but-merged row unrepresentable.
//! Nothing ever *opened* a pull request. This is that half.
//!
//! Three properties are structural rather than documented:
//!
//! * **The official client only.** The lane invokes `gh`, or an absolute path to a program the
//!   caller names, through the same shell-free [`ProcessSpec`](crate::process::ProcessSpec) the
//!   rest of the platform uses; arguments are passed as an argument vector, never as a command
//!   line for a shell to re-parse.
//! * **Credentials are never handled here.** The environment is built from the platform's
//!   allowlist, which strips `GH_TOKEN` and its relatives, so the lane runs on whatever session
//!   `gh auth login` already established. This code cannot leak a token it never sees, and it
//!   cannot authenticate on its own.
//! * **There is no merge.** The lane's command list is built by one function that only ever
//!   produces `pr create` (and `pr view` for the readback); a test asserts that no command it can
//!   build contains `merge`. Merging is a human act on the forge, not a capability of this code.
//!
//! What it deliberately does not do: push the branch. Pushing is a repository operation the
//! repair lane already owns (`repair lane`), and a lane that both pushed and opened pull requests
//! would be doing two reviewable things under one approval.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use crate::process::{build_env, resolve_program, ProcessSpec, ProgramRef};

/// What the lane is allowed to ask `gh` to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhAction {
    /// Open a pull request from a branch that is already pushed.
    CreatePullRequest {
        repo: String,
        head: String,
        base: String,
        title: String,
        body: String,
    },
    /// Read a pull request back, for the independent check after creating one.
    ViewPullRequest { repo: String, number: u64 },
}

/// Why the lane refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhRefusal {
    /// A pull request may not be opened without a named approver.
    NotApproved,
    /// The repository reference is empty or looks like a shell fragment.
    InvalidRepository(String),
    /// The branch reference is empty.
    InvalidBranch(String),
    /// The title is empty: an unnamed pull request is not reviewable.
    EmptyTitle,
    /// The process boundary refused the invocation.
    Process(String),
    /// `gh` itself failed.
    CommandFailed { status: Option<i32>, stderr: String },
    /// The command succeeded but said nothing this lane can act on.
    UnreadableOutput(String),
}

impl std::fmt::Display for GhRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhRefusal::NotApproved => write!(
                f,
                "opening a pull request requires an explicit approver; none was recorded"
            ),
            GhRefusal::InvalidRepository(repo) => write!(f, "repository {repo:?} is not usable"),
            GhRefusal::InvalidBranch(branch) => write!(f, "branch {branch:?} is not usable"),
            GhRefusal::EmptyTitle => write!(f, "a pull request needs a title"),
            GhRefusal::Process(detail) => write!(f, "the process boundary refused it: {detail}"),
            GhRefusal::CommandFailed { status, stderr } => {
                write!(f, "gh exited with {status:?}: {}", stderr.trim())
            }
            GhRefusal::UnreadableOutput(detail) => {
                write!(f, "gh said nothing this lane can use: {detail}")
            }
        }
    }
}

impl std::error::Error for GhRefusal {}

/// The evidence a pull request was opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestOpened {
    pub url: String,
    /// The number parsed out of the URL, when the URL names one.
    pub number: Option<u64>,
    /// The exact argv that produced it, recorded so the evidence shows what ran.
    pub command: Vec<String>,
}

/// Turn an action into an argv, with no shell anywhere in it.
pub fn command_for(action: &GhAction) -> Vec<String> {
    match action {
        GhAction::CreatePullRequest {
            repo,
            head,
            base,
            title,
            body,
        } => vec![
            "pr".to_string(),
            "create".to_string(),
            "--repo".to_string(),
            repo.clone(),
            "--head".to_string(),
            head.clone(),
            "--base".to_string(),
            base.clone(),
            "--title".to_string(),
            title.clone(),
            "--body".to_string(),
            body.clone(),
        ],
        GhAction::ViewPullRequest { repo, number } => vec![
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--repo".to_string(),
            repo.clone(),
            "--json".to_string(),
            "number,url,state,title,headRefName,baseRefName".to_string(),
        ],
    }
}

/// Where `gh` keeps its own configuration, per platform.
///
/// `gh` finds its session from its configuration directory, not from a token: on Windows that is
/// `%APPDATA%\GitHub CLI`, elsewhere `$XDG_CONFIG_HOME/gh` or `~/.config/gh`. The platform's
/// environment allowlist carries `HOME` and `USERPROFILE` but not `APPDATA`, so this lane forwards
/// the configuration-location variables explicitly rather than widening the allowlist for every
/// child process.
///
/// None of these is a credential, and the token variables (`GH_TOKEN`, `GITHUB_TOKEN`) stay on the
/// denylist: this lane runs on the session `gh auth login` established and cannot authenticate by
/// itself. That is not a theoretical property -- the first real run of this lane failed with
/// *"To get started with GitHub CLI, please run: gh auth login"* because `APPDATA` was missing,
/// which is exactly the failure a credential-passing design would have hidden.
pub const CONFIG_LOCATION_VARIABLES: &[&str] = &[
    "APPDATA",
    "LOCALAPPDATA",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

/// Build the process specification for an action.
pub fn spec_for(
    action: &GhAction,
    program: &ProgramRef,
    parent_env: &BTreeMap<String, String>,
) -> Result<ProcessSpec, GhRefusal> {
    let program =
        resolve_program(program).map_err(|error| GhRefusal::Process(error.to_string()))?;
    let extra: BTreeMap<String, String> = CONFIG_LOCATION_VARIABLES
        .iter()
        .filter_map(|key| {
            parent_env
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(name, value)| (name.clone(), value.clone()))
        })
        .collect();
    let env =
        build_env(parent_env, &extra).map_err(|error| GhRefusal::Process(error.to_string()))?;
    Ok(ProcessSpec {
        program,
        args: command_for(action),
        env,
        cwd: None,
    })
}

/// Open a pull request through the official client.
///
/// `approver` is the human who approved opening it, and it is required: the lane refuses without
/// one, which is the same rule the database enforces for a merge.
pub fn open_pull_request(
    action: &GhAction,
    program: &ProgramRef,
    parent_env: &BTreeMap<String, String>,
    approver: Option<&str>,
) -> Result<PullRequestOpened, GhRefusal> {
    let approver = approver.map(str::trim).filter(|name| !name.is_empty());
    if approver.is_none() {
        return Err(GhRefusal::NotApproved);
    }
    let GhAction::CreatePullRequest {
        repo,
        head,
        base,
        title,
        ..
    } = action
    else {
        return Err(GhRefusal::Process(
            "only a pull-request creation carries an approval".to_string(),
        ));
    };
    if repo.trim().is_empty() || repo.contains(' ') && !repo.contains('/') {
        return Err(GhRefusal::InvalidRepository(repo.clone()));
    }
    if head.trim().is_empty() || base.trim().is_empty() {
        return Err(GhRefusal::InvalidBranch(if head.trim().is_empty() {
            head.clone()
        } else {
            base.clone()
        }));
    }
    if title.trim().is_empty() {
        return Err(GhRefusal::EmptyTitle);
    }

    let spec = spec_for(action, program, parent_env)?;
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .env_clear()
        .envs(&spec.env)
        .output()
        .map_err(|error| GhRefusal::Process(error.to_string()))?;
    if !output.status.success() {
        return Err(GhRefusal::CommandFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout
        .split_whitespace()
        .find(|token| token.starts_with("http"))
        .ok_or_else(|| GhRefusal::UnreadableOutput(stdout.trim().to_string()))?
        .trim()
        .to_string();
    Ok(PullRequestOpened {
        number: url
            .rsplit('/')
            .next()
            .and_then(|tail| tail.parse::<u64>().ok()),
        url,
        command: {
            let mut command = vec![spec.program.clone()];
            command.extend(spec.args.clone());
            command
        },
    })
}

/// Where `gh` lives, as the platform's process boundary wants it.
pub fn default_program() -> ProgramRef {
    ProgramRef::BareName("gh".to_string())
}

/// An absolute path to a `gh` binary, for a caller that has one.
pub fn program_at(path: impl Into<PathBuf>) -> ProgramRef {
    ProgramRef::Absolute(path.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action() -> GhAction {
        GhAction::CreatePullRequest {
            repo: "dominator509/project-vector-asvab".to_string(),
            head: "lane/proof".to_string(),
            base: "main".to_string(),
            title: "A reviewed change".to_string(),
            body: "What it does and why.".to_string(),
        }
    }

    #[test]
    fn no_command_the_lane_can_build_merges_anything() {
        // The requirement is "no auto-merge", and the way to hold it is for the capability not to
        // exist. Every action this lane can express is enumerated here, and none of them merges.
        for action in [
            action(),
            GhAction::ViewPullRequest {
                repo: "dominator509/project-vector-asvab".to_string(),
                number: 7,
            },
        ] {
            let argv = command_for(&action);
            let joined = argv.join(" ");
            assert!(
                !joined.contains("merge"),
                "the lane built a merging command: {joined}"
            );
            assert!(
                !joined.contains("close") && !joined.contains("delete"),
                "the lane built a destructive command: {joined}"
            );
        }
        // And the creation argv is exactly the documented shape.
        assert_eq!(
            command_for(&action()),
            vec![
                "pr",
                "create",
                "--repo",
                "dominator509/project-vector-asvab",
                "--head",
                "lane/proof",
                "--base",
                "main",
                "--title",
                "A reviewed change",
                "--body",
                "What it does and why.",
            ]
        );
    }

    #[test]
    fn an_unapproved_pull_request_is_refused_before_any_process_runs() {
        let env = BTreeMap::new();
        // No approver, an empty approver name, and a whitespace-only one are all refusals.
        for approver in [None, Some(""), Some("   ")] {
            let error = open_pull_request(&action(), &default_program(), &env, approver)
                .expect_err("must refuse");
            assert_eq!(error, GhRefusal::NotApproved, "{approver:?}");
        }
    }

    #[test]
    fn the_child_environment_carries_no_credentials() {
        let mut parent = BTreeMap::new();
        parent.insert("GH_TOKEN".to_string(), "secret".to_string());
        parent.insert("GITHUB_TOKEN".to_string(), "secret".to_string());
        parent.insert("PATH".to_string(), "/usr/bin".to_string());
        parent.insert("HOME".to_string(), "/home/learner".to_string());

        let spec = spec_for(&action(), &default_program(), &parent).expect("a spec");
        assert!(
            !spec.env.contains_key("GH_TOKEN") && !spec.env.contains_key("GITHUB_TOKEN"),
            "the lane must run on the session gh already has, not on a token: {:?}",
            spec.env.keys().collect::<Vec<_>>()
        );
        assert_eq!(spec.env.get("PATH").map(String::as_str), Some("/usr/bin"));
        assert_eq!(spec.program, "gh");

        // The configuration-location variables are forwarded, because gh finds its session
        // there; the token variables are not, because this lane runs on the session gh already
        // has. The first real run of the lane failed for want of APPDATA, so this is pinned.
        let mut with_config = parent.clone();
        with_config.insert("APPDATA".to_string(), "/home/learner/.config".to_string());
        with_config.insert(
            "XDG_CONFIG_HOME".to_string(),
            "/home/learner/.config".to_string(),
        );
        with_config.insert("GH_TOKEN".to_string(), "secret".to_string());
        let spec = spec_for(&action(), &default_program(), &with_config).expect("a spec");
        assert_eq!(
            spec.env.get("APPDATA").map(String::as_str),
            Some("/home/learner/.config")
        );
        assert_eq!(
            spec.env.get("XDG_CONFIG_HOME").map(String::as_str),
            Some("/home/learner/.config")
        );
        assert!(!spec.env.contains_key("GH_TOKEN"));
    }

    #[test]
    fn a_repository_or_title_that_is_not_usable_is_refused() {
        let env = BTreeMap::new();
        let mut bad_repo = action();
        if let GhAction::CreatePullRequest { repo, .. } = &mut bad_repo {
            *repo = "  ".to_string();
        }
        assert!(matches!(
            open_pull_request(&bad_repo, &default_program(), &env, Some("reviewer")),
            Err(GhRefusal::InvalidRepository(_))
        ));

        let mut bad_head = action();
        if let GhAction::CreatePullRequest { head, .. } = &mut bad_head {
            *head = String::new();
        }
        assert!(matches!(
            open_pull_request(&bad_head, &default_program(), &env, Some("reviewer")),
            Err(GhRefusal::InvalidBranch(_))
        ));

        let mut bad_title = action();
        if let GhAction::CreatePullRequest { title, .. } = &mut bad_title {
            *title = "   ".to_string();
        }
        assert!(matches!(
            open_pull_request(&bad_title, &default_program(), &env, Some("reviewer")),
            Err(GhRefusal::EmptyTitle)
        ));
    }

    #[test]
    fn the_lane_runs_a_real_process_and_parses_its_output() {
        // A stand-in for `gh`: the point is that the lane invokes a real program with an argument
        // vector and reads its stdout, so a repository whose client is missing fails loudly
        // instead of pretending a pull request exists.
        let dir = std::env::temp_dir().join(format!("vector-gh-lane-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        #[cfg(windows)]
        let script = {
            let script = dir.join("fake-gh.cmd");
            std::fs::write(
                &script,
                "@echo off\r\necho https://github.com/dominator509/project-vector-asvab/pull/4242\r\n",
            )
            .expect("write");
            script
        };
        #[cfg(not(windows))]
        let script = {
            use std::os::unix::fs::PermissionsExt;
            let script = dir.join("fake-gh");
            std::fs::write(
                &script,
                "#!/bin/sh\necho https://github.com/dominator509/project-vector-asvab/pull/4242\n",
            )
            .expect("write");
            let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&script, permissions).expect("chmod");
            script
        };

        let opened = open_pull_request(
            &action(),
            &program_at(script.clone()),
            &BTreeMap::new(),
            Some("release-reviewer"),
        )
        .expect("the lane runs the program and reads the URL");
        assert_eq!(
            opened.url,
            "https://github.com/dominator509/project-vector-asvab/pull/4242"
        );
        assert_eq!(opened.number, Some(4242));
        assert_eq!(
            opened.command.first().map(String::as_str),
            Some(script.to_str().expect("path"))
        );
        assert!(opened.command.contains(&"create".to_string()));

        // A program that fails is reported with its own stderr, not swallowed.
        #[cfg(windows)]
        let failing = {
            let failing = dir.join("failing-gh.cmd");
            std::fs::write(
                &failing,
                "@echo off\r\necho no pull request for you 1>&2\r\nexit /b 1\r\n",
            )
            .expect("write");
            failing
        };
        #[cfg(not(windows))]
        let failing = {
            use std::os::unix::fs::PermissionsExt;
            let failing = dir.join("failing-gh");
            std::fs::write(
                &failing,
                "#!/bin/sh\necho no pull request for you 1>&2\nexit 1\n",
            )
            .expect("write");
            let mut permissions = std::fs::metadata(&failing).expect("metadata").permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&failing, permissions).expect("chmod");
            failing
        };
        match open_pull_request(
            &action(),
            &program_at(failing),
            &BTreeMap::new(),
            Some("release-reviewer"),
        ) {
            Err(GhRefusal::CommandFailed { status, stderr }) => {
                assert_eq!(status, Some(1));
                assert!(stderr.contains("no pull request"), "{stderr}");
            }
            other => panic!("expected a command failure, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
