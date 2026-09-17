//! The git adapter against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`. They are ignored rather than compiled out: the default check
//! still builds them, so `cargo fmt` and `clippy` hold them to the same standard as everything else
//! and they cannot rot unnoticed (`docs/standards/code.md`, "Rust integration test").

use std::path::{Path, PathBuf};
use std::process::Command;

use farik_store::{Git, GitError, MergeOutcome};

/// A repository of its own, removed when the test ends however the test ends.
struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    /// A repository with one commit on `main`, holding `README.md`.
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "farik-git-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a directory under the temporary directory");
        let repository = Self { path };
        repository.git(&["init", "-b", "main"]);
        // An identity, because a machine that has none cannot commit at all, and no signing,
        // because that would ask for a key nobody here has.
        repository.git(&["config", "user.name", "Farik Test"]);
        repository.git(&["config", "user.email", "test@farik.invalid"]);
        repository.git(&["config", "commit.gpgsign", "false"]);
        // And nothing of the person's own runs or is read inside a fixture. `Git` spawns its own
        // children and honours their configuration on purpose, so the fixture's own configuration
        // is where this has to be said: local beats global, and both the helper's git and the
        // adapter's read it. A global `core.hooksPath` would otherwise run a contributor's hooks
        // here, a global `core.excludesFile` would make a session's output an ignored file, and a
        // global `core.attributesFile` marking `*.rs -diff` would print a patch as "Binary files
        // differ" — the three ways a person's own configuration says what a path is.
        repository.git(&["config", "core.hooksPath", "/dev/null"]);
        repository.git(&["config", "core.excludesFile", "/dev/null"]);
        repository.git(&["config", "core.attributesFile", "/dev/null"]);
        repository.write("README.md", "the first line\n");
        repository.commit("the first commit");
        repository
    }

    fn adapter(&self) -> Git {
        Git::open(self.path.clone())
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.path.join(name);
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory).expect("the directory the file is in");
        }
        std::fs::write(path, text).expect("the file is written");
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
    }

    fn git(&self, arguments: &[&str]) -> String {
        run_git(&self.path, arguments)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Runs git directly, for the setup a test needs before the adapter is the thing under test.
///
/// The setup is held away from whatever the person running the tests has configured — a global
/// `core.hooksPath` would otherwise run their hooks inside the fixture. What this cannot do is
/// isolate the adapter: `Git` spawns its own children, and it honours the user's configuration on
/// purpose, which is the whole reason Farik shells out to git at all. So no assertion in this file
/// may rest on anything a setting can reshape — where one would, the adapter names the flag that
/// makes the answer its own rather than the configuration's.
fn run_git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("LC_ALL", "C")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {}: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn knows_a_repository_from_a_directory_that_is_not_one() {
    let repository = TempRepo::new("is-a-repository");
    assert!(repository.adapter().is_repository());
    let plain = repository.path.join("not-a-repository");
    std::fs::create_dir_all(&plain).expect("a plain directory");
    // A directory inside the repository is still inside it; one outside is not.
    assert!(Git::open(plain).is_repository());
    assert!(!Git::open(std::env::temp_dir().join("farik-nowhere-at-all")).is_repository());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn reads_the_commit_at_the_tip_and_says_when_there_is_none() {
    let empty = std::env::temp_dir().join(format!("farik-git-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).expect("a directory");
    run_git(&empty, &["init", "-b", "main"]);
    assert_eq!(
        Git::open(empty.clone())
            .head_summary()
            .expect("the read works"),
        None,
        "a repository farik init has just made has no commit"
    );
    let _ = std::fs::remove_dir_all(&empty);

    let repository = TempRepo::new("head-summary");
    let summary = repository
        .adapter()
        .head_summary()
        .expect("the read works")
        .expect("a repository with a commit has one");
    assert_eq!(summary.sha, repository.git(&["rev-parse", "HEAD"]));
    assert_eq!(summary.subject, "the first commit");
    assert!(
        summary.committed_at.starts_with("20"),
        "an ISO 8601 date: {}",
        summary.committed_at
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn answers_with_the_branch_it_is_on_when_there_is_no_remote_to_ask() {
    // A repository with no remote records its default branch nowhere, so the branch HEAD is on is
    // the only answer there is; the team file is where a team says otherwise (5.14).
    let repository = TempRepo::new("default-branch");
    let git = repository.adapter();
    assert_eq!(git.current_branch().expect("the read works"), "main");
    assert_eq!(git.default_branch().expect("the read works"), "main");
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    assert_eq!(git.current_branch().expect("the read works"), "farik/FRK-1");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn makes_a_branch_and_a_worktree_for_a_task_and_takes_the_worktree_away_again() {
    // One worktree per task is what keeps two tasks from sharing a working tree (5.14), and the
    // branch outlives it: the work is on the branch, not in the directory.
    let repository = TempRepo::new("worktree");
    let git = repository.adapter();
    git.create_branch("farik/FRK-1", "main")
        .expect("the branch is made");
    let taken = git.create_branch("farik/FRK-1", "main");
    let Err(GitError::CommandFailed { command, stderr }) = taken else {
        panic!("a name is taken once: {taken:?}");
    };
    assert_eq!(command, "branch farik/FRK-1 main");
    // What git said, not how it said it: the sentence is translated, the branch name is not.
    assert!(stderr.contains("farik/FRK-1"), "{stderr}");

    let worktree = repository.path.join(".farik/local/worktrees/FRK-2");
    git.create_worktree(&worktree, "farik/FRK-2", "main")
        .expect("the worktree is made");
    assert!(
        worktree.join("README.md").is_file(),
        "it has the work in it"
    );
    assert!(git.is_clean(&worktree).expect("the read works"));

    std::fs::write(worktree.join("built-by-the-session.txt"), "output\n")
        .expect("something the session built");
    assert!(
        !git.is_clean(&worktree).expect("the read works"),
        "an untracked file is not clean either"
    );
    // Forced, because that untracked file is exactly what a finished task leaves behind.
    git.remove_worktree(&worktree).expect("it is taken away");
    assert!(!worktree.exists());
    assert!(
        repository
            .git(&["branch", "--list", "farik/FRK-2"])
            .contains("farik/FRK-2"),
        "and the branch is kept"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn calls_a_worktree_dirty_whatever_the_repository_is_configured_to_show() {
    // A user with `status.showUntrackedFiles = no` does not get to tell Farik that a worktree full
    // of a session's output is clean. Its own fixture, because a setting stays in force for
    // everything after it: left in the test above, it would take the dirtiness out of the very
    // worktree whose removal that test forces.
    let repository = TempRepo::new("untracked-hidden");
    let git = repository.adapter();
    let worktree = repository.path.join(".farik/local/worktrees/FRK-3");
    git.create_worktree(&worktree, "farik/FRK-3", "main")
        .expect("the worktree is made");
    std::fs::write(worktree.join("built-by-the-session.txt"), "output\n")
        .expect("something the session built");
    repository.git(&["config", "status.showUntrackedFiles", "no"]);
    assert!(
        !git.is_clean(&worktree).expect("the read works"),
        "an untracked file is not clean whatever the repository is configured to show"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn counts_the_commits_a_branch_added_and_names_every_path_it_touched() {
    let repository = TempRepo::new("changed-paths");
    let git = repository.adapter();
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    repository.write("src/added.rs", "fn added() {}\n");
    repository.write("README.md", "the first line\nand a second\n");
    repository.commit("feat: add a thing");
    repository.git(&["rm", "--quiet", "README.md"]);
    repository.commit("feat: and take one away");
    // main moves on after the branch left it, as it does whenever another task lands first. What
    // this branch changed is what it changed since the two last agreed, so none of main's own work
    // belongs to it: counted from the tip instead, the governor would be handed
    // src/only-on-main.rs as a path this task touched (5.6).
    repository.git(&["checkout", "main"]);
    repository.write("src/only-on-main.rs", "fn elsewhere() {}\n");
    repository.commit("feat: something else entirely");
    repository.git(&["checkout", "farik/FRK-1"]);

    assert_eq!(git.commit_count("main", "farik/FRK-1"), Ok(2));
    assert_eq!(
        git.commit_count("farik/FRK-1", "main"),
        Ok(1),
        "the other way is main's own commit, not the branch's two"
    );
    let mut changed = git
        .changed_paths("main", "farik/FRK-1")
        .expect("the read works");
    changed.sort();
    assert_eq!(changed, ["README.md", "src/added.rs"]);
    // git's own prefixes, whatever this repository is configured to use: a patch without them is
    // one nothing can apply, and a reviewer is handed this patch to read (5.6).
    repository.git(&["config", "diff.noprefix", "true"]);
    let patch = git.diff("main", "farik/FRK-1").expect("the read works");
    assert!(patch.contains("fn added()"), "the patch holds the change");
    assert!(patch.contains("--- a/README.md"), "and the removal");
    assert!(patch.contains("+++ b/src/added.rs"), "with both prefixes");
    assert!(
        !patch.contains("only-on-main"),
        "and nothing main did after the branch left it: {patch}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn keeps_a_path_whose_name_begins_with_a_space() {
    // The allowed-paths rule is asked about each path a change touched (5.6), so a path that comes
    // back a byte short is a change checked against a rule it never matched. `-z` is what keeps a
    // newline in a path; this is what keeps a space at the front of one.
    let repository = TempRepo::new("odd-path");
    let git = repository.adapter();
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    repository.write(" leading.txt", "a path that starts with a space\n");
    repository.commit("feat: a path only a person could name");

    assert_eq!(
        git.changed_paths("main", "farik/FRK-1")
            .expect("the read works"),
        [" leading.txt"]
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn names_both_sides_of_a_file_that_moved() {
    // The governor asks about each path a change touched (5.6), and a move touches two: a rename
    // reported only by its new name would let work land at a path nobody allowed.
    let repository = TempRepo::new("renames");
    let git = repository.adapter();
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    std::fs::create_dir_all(repository.path.join("docs")).expect("the directory it moves into");
    repository.git(&["mv", "README.md", "docs/README.md"]);
    repository.commit("docs: move the readme");

    let mut changed = git
        .changed_paths("main", "farik/FRK-1")
        .expect("the read works");
    changed.sort();
    assert_eq!(changed, ["README.md", "docs/README.md"]);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn merges_a_finished_task_into_the_integration_branch() {
    let repository = TempRepo::new("merge");
    let git = repository.adapter();
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    repository.write("src/added.rs", "fn added() {}\n");
    repository.commit("feat: add a thing");

    // Called from the task's own branch, which is where a finished task leaves the repository. The
    // merge goes to the integration branch and comes back: this checkout is the user's own, not a
    // task's worktree, so what they have open is not the merge's to move.
    let outcome = git
        .merge("main", "farik/FRK-1", "integrate FRK-1")
        .expect("the merge runs");
    let MergeOutcome::Merged { sha } = outcome else {
        panic!("it merged: {outcome:?}");
    };
    assert_eq!(sha, repository.git(&["rev-parse", "main"]));
    assert_eq!(
        git.current_branch().expect("the read works"),
        "farik/FRK-1",
        "and left the repository on the branch it found it on"
    );
    assert_eq!(
        repository.git(&["log", "-1", "--format=%s", "main"]),
        "integrate FRK-1",
        "with a merge commit, so the task's commits survive"
    );
    assert_eq!(
        repository.git(&["rev-list", "--count", "--merges", "main"]),
        "1"
    );
    assert_eq!(
        repository.git(&["show", "main:src/added.rs"]),
        "fn added() {}",
        "and the work is on the integration branch"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_merge_that_failed_for_something_other_than_a_conflict() {
    // Nothing conflicted, so there is nothing to report as a conflict: an empty list of conflicted
    // paths would be a refusal wearing the shape of an answer, and the task would be integrated on
    // paper without a single commit having moved.
    let repository = TempRepo::new("merge-refused");
    let git = repository.adapter();
    let refusal = git.merge("main", "farik/FRK-404", "integrate FRK-404");
    let Err(GitError::CommandFailed { command, stderr }) = refusal else {
        panic!("it refused: {refusal:?}");
    };
    assert_eq!(
        command, "merge --no-ff -m integrate FRK-404 farik/FRK-404",
        "the merge's own refusal, not whatever an abort with nothing to abort says"
    );
    assert!(stderr.contains("farik/FRK-404"), "{stderr}");
    assert_eq!(
        git.current_branch().expect("the read works"),
        "main",
        "and the repository is where it was"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_to_merge_from_a_detached_head_because_there_is_no_branch_to_put_back() {
    // The repository is put back on the branch it was found on, and a detached head is not one.
    // Refusing before anything has been run is better than merging and leaving a person somewhere
    // they never were.
    let repository = TempRepo::new("detached");
    let git = repository.adapter();
    repository.git(&["checkout", "--detach"]);
    let answer = git.merge("main", "main", "integrate nothing at all");
    assert!(
        matches!(answer, Err(GitError::CommandFailed { .. })),
        "{answer:?}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn names_what_conflicted_and_leaves_the_tree_as_it_was() {
    // A conflict escalates the task with reason `integration` (5.14). What it must not do is leave
    // a half-merged working tree behind for the next command to trip over.
    let repository = TempRepo::new("conflict");
    let git = repository.adapter();
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    repository.write("README.md", "the branch's line\n");
    repository.commit("docs: the branch writes it");
    repository.git(&["checkout", "main"]);
    repository.write("README.md", "main's line\n");
    repository.commit("docs: main writes it too");
    let before = repository.git(&["rev-parse", "main"]);
    repository.git(&["checkout", "farik/FRK-1"]);

    let outcome = git
        .merge("main", "farik/FRK-1", "integrate FRK-1")
        .expect("the merge runs and reports");
    assert_eq!(
        outcome,
        MergeOutcome::Conflicts(vec!["README.md".to_string()])
    );
    assert_eq!(
        repository.git(&["rev-parse", "main"]),
        before,
        "nothing was committed"
    );
    assert_eq!(
        git.current_branch().expect("the read works"),
        "farik/FRK-1",
        "and the repository is back on the branch it was on"
    );
    assert!(
        git.is_clean(&repository.path).expect("the read works"),
        "and nothing was left half-merged"
    );
    assert_eq!(
        repository.git(&["show", "main:README.md"]),
        "main's line",
        "the integration branch's own work is untouched"
    );
}
