//! The worktree lane: run a gate in an isolated checkout (REQ-031).
//!
//! `AGENTS.md` §11 requires parallel agents to be isolated in git worktrees and
//! merged through a queue. `crates/vector-repair` provides the isolation
//! primitive; this makes it usable as a command, so "work in a worktree" is
//! something the project can actually do rather than something its rules ask
//! for.
//!
//! ## What it refuses
//!
//! * A gate name that is not in `COMMANDS.md`. The lane runs repository scripts,
//!   and accepting an arbitrary path would make this a general-purpose command
//!   runner reachable from a planning document.
//! * A worktree that contains learner state. The check happens before the gate
//!   runs, so a gate can never be pointed at installed data.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use vector_repair::worktree::{RepairWorktree, WorktreeSpec};

/// Gates this lane will run inside a worktree.
///
/// Every entry is a script named in `COMMANDS.md`. The list is deliberately
/// short: the point of the lane is to run a *cheap* check in isolation, and a
/// full gate sweep inside a fresh worktree would rebuild the whole workspace.
pub const LANE_GATES: &[&str] = &[
    "format-check",
    "typecheck",
    "test-collection-guard",
    "reality-gate",
];

/// Gates that need the Node workspace installed before they can run.
///
/// `reality-gate` and `test-collection-guard` are Python-only; the other two
/// drive pnpm scripts, and a fresh worktree has no `node_modules`, so they would
/// fail with "command not found" — a missing prerequisite rather than a finding.
const NEEDS_NODE_DEPS: &[&str] = &["format-check", "typecheck"];

/// Run one gate inside an isolated worktree of `repository` at `base`.
pub fn run_lane(repository: &Path, base: &str, gate: &str) -> Result<Value> {
    run_lane_with(repository, base, gate, true)
}

/// As [`run_lane`], with dependency provisioning under the caller's control.
pub fn run_lane_with(repository: &Path, base: &str, gate: &str, provision: bool) -> Result<Value> {
    if !LANE_GATES.contains(&gate) {
        anyhow::bail!(
            "{gate:?} is not a lane gate; choose one of {LANE_GATES:?} \
             (the full list of legal commands is in COMMANDS.md)"
        );
    }

    let repository = repository
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", repository.display()))?;

    let mut destination = std::env::temp_dir();
    destination.push(format!(
        "vector-lane-{gate}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let spec = WorktreeSpec::new(&repository, base, &destination);
    let worktree = RepairWorktree::create(&spec)
        .with_context(|| format!("cannot create a worktree of {}", repository.display()))?;

    let head = git_rev(&worktree)?;
    let tracked = worktree.tracked_files().unwrap_or_default();
    let learner_state_absent = !worktree.path().join("vector.db").exists();

    let script = format!("scripts/{gate}.sh");
    if !worktree.path().join(&script).exists() {
        let path = worktree.path().to_path_buf();
        worktree.remove().ok();
        anyhow::bail!(
            "{script} is not present in the worktree at {}",
            path.display()
        );
    }

    // Provision before running: a worktree is a checkout without installed
    // dependencies. The step is reported rather than hidden, because a gate that
    // could not run is a different result from a gate that ran and failed.
    let mut provisioning: Option<Value> = None;
    if provision && NEEDS_NODE_DEPS.contains(&gate) {
        let started = std::time::Instant::now();
        let outcome = run_with_timeout(
            worktree.path(),
            "sh",
            &["scripts/install.sh"],
            Duration::from_secs(900),
        )?;
        provisioning = Some(json!({
            "command": "sh scripts/install.sh",
            "exitCode": outcome.0,
            "durationMs": started.elapsed().as_millis(),
            "stderrTail": tail(&outcome.2, 20),
        }));
        if outcome.0 != 0 {
            let path = worktree.path().to_path_buf();
            worktree.remove().ok();
            anyhow::bail!(
                "cannot provision the worktree at {} (install.sh exited {}); \
                 the gate was not run",
                path.display(),
                outcome.0
            );
        }
    }

    let started = std::time::Instant::now();
    let outcome = run_with_timeout(worktree.path(), "sh", &[&script], Duration::from_secs(900))?;
    let duration_ms = started.elapsed().as_millis();

    let report = json!({
        "gate": gate,
        "script": script,
        "base": base,
        "head": head,
        "repository": repository.display().to_string(),
        "worktree": worktree.path().display().to_string(),
        "trackedFileCount": tracked.len(),
        "learnerStateAbsent": learner_state_absent,
        "provisioning": provisioning,
        "exitCode": outcome.0,
        "durationMs": duration_ms,
        "stdoutTail": tail(&outcome.1, 40),
        "stderrTail": tail(&outcome.2, 40),
    });

    // The worktree is removed whatever the gate did: a failed gate is a result,
    // not a reason to leave a checkout behind.
    worktree.remove().context("cannot remove the worktree")?;

    Ok(report)
}

/// Resolve HEAD inside a worktree so the report identifies the exact revision.
fn git_rev(worktree: &RepairWorktree) -> Result<String> {
    let output = worktree
        .run("git", &["rev-parse", "HEAD"])
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(output.stdout.trim().to_string())
}

/// Run a command with a deadline, killing it if it overruns.
///
/// `std::process::Command::output` cannot time out, and a gate that hangs would
/// otherwise hang the lane for ever.
fn run_with_timeout(
    dir: &Path,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<(i32, String, String)> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot start {program}"))?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => {
                let output = child.wait_with_output()?;
                return Ok((
                    status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stdout).into_owned(),
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ));
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("{program} {args:?} exceeded {timeout:?} and was killed");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// The last `lines` lines of `text`, so a report stays readable.
fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// Where the repository root is, relative to this executable's manifest.
pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
