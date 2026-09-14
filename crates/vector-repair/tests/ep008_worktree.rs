//! EP-008 acceptance: real git worktree isolation (REQ-031, ARCH-010).
//!
//! `ARCHITECTURE.md` ARCH-010: "Repair agents run in isolated git worktrees,
//! never against installed learner state." The broker's stage machine orders the
//! pipeline, but ordering is not isolation — the previous `prepare_worktree`
//! assigned an enum variant and touched nothing. These tests operate on real
//! repositories and real worktrees, and every claim is read back from the
//! filesystem or from git.
//!
//! ## Why the scratch repository matters
//!
//! These run against a small throwaway repository rather than this project. The
//! properties under test — a separate top level, no learner state, no leakage
//! back into the main checkout, removal on drop — are properties of git
//! worktrees, and a tiny repository demonstrates them in under a second instead
//! of checking out the whole product for each case.
//!
//! The one test that must use *this* repository, because it is about this
//! repository's own state, is `the_project_worktree_excludes_learner_state`.

use std::path::{Path, PathBuf};
use std::process::Command;

use vector_repair::worktree::{RepairWorktree, WorktreeError, WorktreeSpec};

/// A throwaway git repository with one commit.
struct Scratch {
    root: PathBuf,
    repository: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "vector-worktree-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create scratch root");

        let repository = root.join("repo");
        std::fs::create_dir_all(repository.join("src")).expect("create repository");
        std::fs::write(repository.join("AGENTS.md"), "# rules\n").expect("write rules");
        std::fs::write(
            repository.join("src/lib.rs"),
            "pub fn answer() -> u8 { 42 }\n",
        )
        .expect("write source");
        // A learner database in the main checkout, to prove it does not appear
        // in the worktree.
        std::fs::write(repository.join("vector.db"), "pretend local state").expect("write db");

        for args in [
            vec!["init", "--quiet", "--initial-branch=main"],
            vec!["add", "AGENTS.md", "src/lib.rs"],
            vec![
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "user.name=Vector Test",
                "commit",
                "--quiet",
                "-m",
                "initial",
            ],
        ] {
            git(&repository, &args);
        }

        Self { root, repository }
    }

    fn spec(&self, tag: &str) -> WorktreeSpec {
        WorktreeSpec::new(
            &self.repository,
            "HEAD",
            &self.root.join(format!("worktree-{tag}")),
        )
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Run git, asserting success.
fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

// ---------------------------------------------------------------------------
// Creation and isolation
// ---------------------------------------------------------------------------

#[test]
fn a_worktree_is_created_at_the_requested_commit_and_separate_from_the_checkout() {
    let scratch = Scratch::new("create");
    let spec = scratch.spec("create");

    let worktree = RepairWorktree::create(&spec).expect("create worktree");

    // The directory exists and holds the tracked files.
    assert!(worktree.path().exists());
    assert!(worktree.path().join("AGENTS.md").exists());
    assert!(worktree.path().join("src/lib.rs").exists());

    // It is a different top level from the main checkout.
    let reported = git(worktree.path(), &["rev-parse", "--show-toplevel"]);
    assert_ne!(
        std::fs::canonicalize(reported.trim()).expect("canonicalize"),
        std::fs::canonicalize(&scratch.repository).expect("canonicalize"),
        "the worktree must not be the main checkout"
    );

    // It shares the object database, which is what makes it a worktree rather
    // than a clone: a clone would not see the repository's own commits.
    let common = git(worktree.path(), &["rev-parse", "--git-common-dir"]);
    let common_path = PathBuf::from(common.trim());
    let common_abs = if common_path.is_absolute() {
        common_path
    } else {
        worktree.path().join(common_path)
    };
    assert_eq!(
        std::fs::canonicalize(common_abs).expect("canonicalize common dir"),
        std::fs::canonicalize(scratch.repository.join(".git")).expect("canonicalize main .git"),
        "the worktree must share the repository's object store"
    );

    worktree.remove().expect("remove");
}

#[test]
fn learner_state_is_absent_from_the_worktree() {
    // The main checkout holds an untracked `vector.db`. A worktree is a checkout
    // of tracked files, so it must not be there — and the creation must have
    // verified that rather than assuming it.
    let scratch = Scratch::new("learner-state");
    assert!(scratch.repository.join("vector.db").exists());

    let worktree = RepairWorktree::create(&scratch.spec("learner-state")).expect("create");
    assert!(
        !worktree.path().join("vector.db").exists(),
        "the learner database must not appear inside a repair worktree"
    );
    assert!(!worktree.path().join(".env").exists());

    worktree.remove().expect("remove");
}

#[test]
fn a_forbidden_entry_in_the_worktree_is_refused() {
    // If the forbidden path were tracked, it *would* be checked out, and the
    // creation must fail rather than hand an agent the learner's database.
    let scratch = Scratch::new("forbidden");
    std::fs::write(scratch.repository.join("leak.db"), "state").expect("write");
    git(&scratch.repository, &["add", "--force", "leak.db"]);
    git(
        &scratch.repository,
        &[
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=Vector Test",
            "commit",
            "--quiet",
            "-m",
            "track a forbidden file",
        ],
    );

    let mut spec = scratch.spec("forbidden");
    spec.forbidden_entries = vec!["leak.db".to_string()];

    let error = RepairWorktree::create(&spec).expect_err("must refuse");
    assert!(
        matches!(error, WorktreeError::LearnerStatePresent(_)),
        "got {error:?}"
    );
    assert!(
        !spec.destination.exists(),
        "a refused creation must not leave a checkout behind"
    );
}

#[test]
fn an_unknown_commit_is_refused_before_anything_is_created() {
    let scratch = Scratch::new("unknown-commit");
    let spec = WorktreeSpec::new(
        &scratch.repository,
        "0000000000000000000000000000000000000000",
        &scratch.root.join("worktree-unknown"),
    );

    let error = RepairWorktree::create(&spec).expect_err("must refuse");
    assert!(
        matches!(error, WorktreeError::UnknownCommit(_)),
        "got {error:?}"
    );
    assert!(!spec.destination.exists());
}

#[test]
fn a_non_repository_is_refused() {
    let mut root = std::env::temp_dir();
    root.push(format!("vector-not-a-repo-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create dir");
    let spec = WorktreeSpec::new(&root, "HEAD", &root.join("wt"));

    let error = RepairWorktree::create(&spec).expect_err("must refuse");
    assert!(
        matches!(error, WorktreeError::NotARepository(_)),
        "got {error:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_existing_destination_is_refused() {
    let scratch = Scratch::new("existing");
    let spec = scratch.spec("existing");
    std::fs::create_dir_all(&spec.destination).expect("pre-create destination");
    std::fs::write(spec.destination.join("keep.txt"), "x").expect("write");

    let error = RepairWorktree::create(&spec).expect_err("must refuse");
    assert!(
        matches!(error, WorktreeError::DestinationExists(_)),
        "got {error:?}"
    );
    // The pre-existing directory and its contents are untouched.
    assert!(spec.destination.join("keep.txt").exists());
}

// ---------------------------------------------------------------------------
// Isolation in both directions
// ---------------------------------------------------------------------------

#[test]
fn a_write_inside_the_worktree_does_not_reach_the_main_checkout() {
    // This is the property that makes parallel agents safe: two worktrees of the
    // same repository cannot see each other's uncommitted work.
    let scratch = Scratch::new("isolation");
    let a = RepairWorktree::create(&scratch.spec("a")).expect("create a");
    let b = RepairWorktree::create(&scratch.spec("b")).expect("create b");

    let a_file = a.path().join("AGENTS.md");
    let b_file = b.path().join("AGENTS.md");
    let main_file = scratch.repository.join("AGENTS.md");

    std::fs::write(&a_file, "# changed in worktree a\n").expect("write a");

    // Read with line endings normalised: `core.autocrlf` rewrites text on
    // checkout, and that is a property of the machine rather than of isolation.
    let read = |path: &Path| {
        std::fs::read_to_string(path)
            .expect("read")
            .replace("\r\n", "\n")
    };

    // The main checkout and the sibling worktree both still see the original.
    assert_eq!(read(&main_file), "# rules\n");
    assert_eq!(read(&b_file), "# rules\n");
    assert_eq!(read(&a_file), "# changed in worktree a\n");

    // Only the modified worktree reports itself dirty.
    assert!(a.is_dirty(), "worktree a has an uncommitted change");
    assert!(!b.is_dirty(), "worktree b is untouched");

    a.remove().expect("remove a");
    b.remove().expect("remove b");

    // The change never reached the main checkout.
    assert_eq!(read(&main_file), "# rules\n");
}

#[test]
fn tracked_file_queries_reflect_the_checkout() {
    let scratch = Scratch::new("tracked");
    let worktree = RepairWorktree::create(&scratch.spec("tracked")).expect("create");

    let tracked = worktree.tracked_files().expect("list tracked");
    assert!(tracked.iter().any(|f| f == "AGENTS.md"), "got {tracked:?}");
    // The learner database is untracked, so it is not in the checkout.
    assert!(!tracked.iter().any(|f| f == "vector.db"));

    assert!(worktree.is_tracked("AGENTS.md"));
    assert!(
        !worktree.is_tracked("vector.db"),
        "an untracked path cannot appear in a reviewable diff"
    );

    worktree.remove().expect("remove");
}

// ---------------------------------------------------------------------------
// Running commands inside the worktree
// ---------------------------------------------------------------------------

#[test]
fn commands_run_inside_the_worktree_without_a_shell() {
    let scratch = Scratch::new("commands");
    let worktree = RepairWorktree::create(&scratch.spec("commands")).expect("create");

    let output = worktree
        .run("git", &["status", "--porcelain"])
        .expect("run");
    assert!(output.succeeded(), "got {output:?}");
    assert_eq!(output.stdout.trim(), "", "a fresh worktree is clean");

    // Arguments are passed as a vector, so a value that would be a shell
    // metacharacter is an ordinary argument. In a shell this would create a
    // file; here it must simply be rejected by git as an unknown flag.
    let injected = worktree
        .run("git", &["rev-parse", "--verify", "HEAD; touch pwned"])
        .expect("run");
    assert!(
        !injected.succeeded(),
        "a metacharacter-bearing revision must not resolve"
    );
    assert!(
        !worktree.path().join("pwned").exists(),
        "no shell means no command injection"
    );

    worktree.remove().expect("remove");
}

// ---------------------------------------------------------------------------
// Removal
// ---------------------------------------------------------------------------

#[test]
fn removal_deletes_the_checkout_and_gits_record_of_it() {
    let scratch = Scratch::new("remove");
    let spec = scratch.spec("remove");
    let worktree = RepairWorktree::create(&spec).expect("create");
    let path = worktree.path().to_path_buf();

    let listed = git(&scratch.repository, &["worktree", "list"]);
    assert!(
        listed.contains(&path.to_string_lossy().replace('\\', "/"))
            || listed.contains(&path.to_string_lossy().to_string()),
        "git must know about the worktree: {listed}"
    );

    worktree.remove().expect("remove");

    assert!(!path.exists(), "the checkout directory must be gone");
    // Counted rather than substring-matched: the scratch repository's own path
    // contains the tag, so a substring check would match the main worktree.
    let after = git(&scratch.repository, &["worktree", "list"]);
    let entries = after.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(
        entries, 1,
        "only the main checkout may remain, got:\n{after}"
    );
    assert!(
        !after.contains(&path.to_string_lossy().replace('\\', "/")),
        "the removed worktree must not be listed: {after}"
    );
}

#[test]
fn dropping_a_worktree_removes_it_too() {
    // The pipeline can fail at any point; a repair that is abandoned must not
    // leave a checkout behind for the next run to trip over.
    let scratch = Scratch::new("drop");
    let path = {
        let worktree = RepairWorktree::create(&scratch.spec("drop")).expect("create");
        worktree.path().to_path_buf()
    };

    assert!(!path.exists(), "dropping must remove the checkout");
    let listed = git(&scratch.repository, &["worktree", "list"]);
    let entries = listed.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(entries, 1, "record pruned, only main remains:\n{listed}");
}

// ---------------------------------------------------------------------------
// This project's own working tree
// ---------------------------------------------------------------------------

#[test]
fn the_project_worktree_excludes_learner_state() {
    // The same property, against the real repository, starting from the commit
    // this test binary was built from. It exists because the scratch repository
    // above proves the mechanism while this proves the configuration: a
    // `.gitignore` that failed to exclude `vector.db` would show up here.
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("locate the repository");
    assert!(
        repo.join(".git").exists(),
        "expected a git checkout at {}",
        repo.display()
    );

    let mut destination = std::env::temp_dir();
    destination.push(format!(
        "vector-project-worktree-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));

    let spec = WorktreeSpec::new(&repo, "HEAD", &destination);
    let worktree = match RepairWorktree::create(&spec) {
        Ok(worktree) => worktree,
        // A repository with no commits yet cannot have a worktree; that is an
        // environment state, not a product failure, and it must be said rather
        // than silently passed.
        Err(WorktreeError::UnknownCommit(rev)) => {
            panic!("this checkout has no resolvable HEAD ({rev}); commit something first")
        }
        Err(error) => panic!("cannot create a worktree of this repository: {error}"),
    };

    assert!(worktree.path().join("AGENTS.md").exists());
    assert!(worktree.path().join("Cargo.toml").exists());
    for forbidden in ["vector.db", "vector.db-wal", ".env", "target"] {
        assert!(
            !worktree.path().join(forbidden).exists(),
            "{forbidden} must not be present in a repair worktree"
        );
    }

    // The product's own migration files are tracked, so a repair agent can see
    // the schema it is being asked to fix.
    let tracked = worktree.tracked_files().expect("tracked files");
    assert!(
        tracked.iter().any(|f| f == "migrations/001_initial.sql"),
        "migrations must be present in the worktree"
    );

    worktree.remove().expect("remove");
}
