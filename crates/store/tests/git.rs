//! The git adapter against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`. They are ignored rather than compiled out: the default check
//! still builds them, so `cargo fmt` and `clippy` hold them to the same standard as everything else
//! and they cannot rot unnoticed (`docs/standards/code.md`, "Rust integration test").

use farik_store::git::fixtures::{TempRepo, git_in, git_output_in};
use farik_store::{Git, GitError, MergeOutcome};

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
    git_in(&empty, &["init", "-b", "main"]);
    assert_eq!(
        Git::open(empty.clone())
            .head_summary()
            .expect("the read works"),
        None,
        "a repository farik init has just made has no commit"
    );
    let _ = std::fs::remove_dir_all(&empty);

    let repository = TempRepo::new("head-summary");
    // Two commits, so that the tip is the tip rather than the only thing there is.
    repository.write("second.txt", "and a second\n");
    repository.commit("the second commit");
    let summary = repository
        .adapter()
        .head_summary()
        .expect("the read works")
        .expect("a repository with a commit has one");
    assert_eq!(summary.sha, repository.git_output(&["rev-parse", "HEAD"]));
    assert_eq!(summary.subject, "the second commit");
    // Strict ISO 8601, which is what a board and a log can order and a person can read. The
    // ordinary format git prints is neither: it separates the date from the time with a space.
    assert_eq!(
        summary.committed_at,
        repository.git_output(&["log", "-1", "--format=%cI"])
    );
    assert!(
        summary.committed_at.contains('T'),
        "{}",
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
fn reads_the_default_branch_from_the_remote_that_records_it() {
    // A repository with a remote records its default branch, and that is the integration branch
    // (5.14) — not whatever a session happens to have checked out at the time.
    let origin = TempRepo::new("default-origin");
    origin.git(&["branch", "-m", "trunk"]);
    let clone = std::env::temp_dir().join(format!(
        "farik-git-clone-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&clone);
    git_in(
        &std::env::temp_dir(),
        &[
            "clone",
            "--quiet",
            origin.path.to_str().expect("a path that is text"),
            clone.to_str().expect("a path that is text"),
        ],
    );
    let git = Git::open(clone.clone());
    git_in(&clone, &["checkout", "-b", "farik/FRK-1"]);
    assert_eq!(git.current_branch().expect("the read works"), "farik/FRK-1");
    assert_eq!(
        git.default_branch().expect("the read works"),
        "trunk",
        "what the remote records, not what is checked out"
    );
    let _ = std::fs::remove_dir_all(&clone);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn hands_back_a_patch_with_no_colour_in_it() {
    // Whatever the repository is configured to show. A patch is read by a reviewer and by whatever
    // reads a reviewer's answer; escape codes are neither.
    let repository = TempRepo::new("colour");
    let git = repository.adapter();
    repository.git(&["checkout", "-b", "farik/FRK-1"]);
    repository.write("src/added.rs", "fn added() {}\n");
    repository.commit("feat: add a thing");
    repository.git(&["config", "color.ui", "always"]);

    let patch = git.diff("main", "farik/FRK-1").expect("the read works");
    assert!(patch.contains("fn added()"), "the patch holds the change");
    assert!(
        !patch.contains('\u{1b}'),
        "and nothing a terminal would paint: {patch:?}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn makes_a_branch_and_a_worktree_for_a_task_and_takes_the_worktree_away_again() {
    // One worktree per task is what keeps two tasks from sharing a working tree (5.14), and the
    // branch outlives it: the work is on the branch, not in the directory.
    let repository = TempRepo::new("worktree");
    let git = repository.adapter();
    // From `main`, whatever this repository has checked out: a task's branch starts from the
    // integration branch as it is at that moment (5.14), and starting it somewhere else is not an
    // error anyone would see.
    repository.git(&["checkout", "-b", "elsewhere"]);
    repository.write("only-on-elsewhere.txt", "not where a task starts\n");
    repository.commit("docs: somewhere else entirely");

    git.create_branch("farik/FRK-1", "main")
        .expect("the branch is made");
    assert_eq!(
        repository.git_output(&["rev-parse", "farik/FRK-1"]),
        repository.git_output(&["rev-parse", "main"]),
        "the branch starts where it was told to"
    );
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
    assert!(
        !worktree.join("only-on-elsewhere.txt").exists(),
        "and the worktree starts from main too, not from what was checked out"
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
            .git_output(&["branch", "--list", "farik/FRK-2"])
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
fn refuses_a_tree_that_is_not_a_worktree_of_this_repository() {
    // `is_clean` is asked whether a task's work is finished, so another repository's answer is
    // worse than no answer at all — and a worktree a crashed session took away with it is not the
    // git program failing to run, which is what a user would otherwise be told.
    let repository = TempRepo::new("membership");
    let elsewhere = TempRepo::new("membership-elsewhere");
    let git = repository.adapter();

    let foreign = git.is_clean(&elsewhere.path);
    let Err(GitError::CommandFailed { stderr, .. }) = foreign else {
        panic!("another repository is not an answer about this one: {foreign:?}");
    };
    assert!(
        stderr.contains("not a working tree of this repository"),
        "{stderr}"
    );

    let gone = git.is_clean(&repository.path.join(".farik/local/worktrees/FRK-9"));
    let Err(GitError::CommandFailed { stderr, .. }) = gone else {
        panic!("a worktree that is not there is not git failing to run: {gone:?}");
    };
    assert!(stderr.contains("there is no directory at"), "{stderr}");

    // And a worktree of this repository is still an answer, which is the point of asking by
    // repository rather than by path.
    let worktree = repository.path.join(".farik/local/worktrees/FRK-1");
    git.create_worktree(&worktree, "farik/FRK-1", "main")
        .expect("the worktree is made");
    assert!(git.is_clean(&worktree).expect("the read works"));
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
fn lists_what_the_repository_tracks_and_not_what_it_ignores() {
    let repository = TempRepo::new("tracked");
    assert_eq!(
        repository.adapter().root(),
        repository.path,
        "the adapter answers about the directory it was opened on"
    );
    repository.write(".gitignore", "ignored/\n*.log\n");
    repository.write("src/lib.rs", "// code\n");
    repository.write("a path with a space.md", "# spaces\n");
    repository.write("ignored/secret.txt", "not content\n");
    repository.write("noisy.log", "not content\n");
    repository.commit("a tree to scan");

    assert_eq!(
        repository.adapter().tracked_paths().expect("it lists"),
        [
            ".gitignore",
            "README.md",
            "a path with a space.md",
            "src/lib.rs"
        ],
        "what git ignores is not content, and Farik does not have to know what to ignore"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_where_the_repository_begins_whatever_directory_it_was_opened_on() {
    // `Git::open` takes any directory inside a repository, so `root` and the repository's own root
    // are two different questions. A caller that means the project rather than a subtree has to be
    // able to tell.
    let repository = TempRepo::new("top-level");
    repository.write("crates/core/src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repository.commit("a subdirectory");
    let inside = Git::open(repository.path.join("crates/core"));

    assert_eq!(
        inside.root(),
        repository.path.join("crates/core"),
        "root is the directory it was opened on"
    );
    assert_eq!(
        std::fs::canonicalize(inside.top_level().expect("git says where it begins"))
            .expect("a real directory"),
        std::fs::canonicalize(&repository.path).expect("a real directory"),
        "and top_level is where the repository does"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn lists_what_is_staged_in_a_repository_with_no_commit() {
    // A repository `farik init` has just made has an index and no commit, and onboarding scans it.
    // The fixture commits, so the commit goes: `git rm --cached` alone would only unstage the file
    // and leave HEAD where it was, and the test would prove nothing about a repository with none.
    let repository = TempRepo::new("tracked-staged");
    repository.git(&["update-ref", "-d", "HEAD"]);
    repository.git(&["rm", "--cached", "-q", "README.md"]);
    assert_eq!(
        repository.adapter().tracked_paths().expect("it lists"),
        Vec::<String>::new(),
        "nothing is tracked once the only file is out of the index"
    );
    repository.write("staged.rs", "// staged\n");
    repository.git(&["add", "staged.rs"]);
    assert_eq!(
        repository.adapter().tracked_paths().expect("it lists"),
        ["staged.rs"]
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
    assert_eq!(sha, repository.git_output(&["rev-parse", "main"]));
    assert_eq!(
        git.current_branch().expect("the read works"),
        "farik/FRK-1",
        "and left the repository on the branch it found it on"
    );
    assert_eq!(
        repository.git_output(&["log", "-1", "--format=%s", "main"]),
        "integrate FRK-1",
        "with a merge commit, so the task's commits survive"
    );
    assert_eq!(
        repository.git_output(&["rev-list", "--count", "--merges", "main"]),
        "1"
    );
    assert_eq!(
        repository.git_output(&["show", "main:src/added.rs"]),
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
    let before = repository.git_output(&["rev-parse", "main"]);
    repository.git(&["checkout", "farik/FRK-1"]);

    let outcome = git
        .merge("main", "farik/FRK-1", "integrate FRK-1")
        .expect("the merge runs and reports");
    assert_eq!(
        outcome,
        MergeOutcome::Conflicts(vec!["README.md".to_string()])
    );
    assert_eq!(
        repository.git_output(&["rev-parse", "main"]),
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
        repository.git_output(&["show", "main:README.md"]),
        "main's line",
        "the integration branch's own work is untouched"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn commits_the_named_paths_and_returns_the_sha() {
    let repository = TempRepo::new("commit-paths");
    repository.write("named.txt", "the one to commit\n");
    repository.write("other.txt", "the one to leave\n");
    let git = repository.adapter();
    assert!(
        git.status(&repository.path)
            .expect("the status reads")
            .contains("named.txt"),
        "the status shows what is changed"
    );

    let sha = git
        .commit(
            &repository.path,
            "add the named file",
            &["named.txt".to_string()],
        )
        .expect("the commit is made");

    assert_eq!(sha, repository.git_output(&["rev-parse", "HEAD"]));
    assert_eq!(
        repository.git_output(&["show", "--name-only", "--format=", "HEAD"]),
        "named.txt",
        "only the named path is in the commit"
    );
    assert_eq!(
        git.status(&repository.path).expect("the status reads"),
        "?? other.txt",
        "and the other is left as it was"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn pushes_a_branch_to_a_remote() {
    let repository = TempRepo::new("push");
    let origin = repository.path.with_extension("origin.git");
    let _ = std::fs::remove_dir_all(&origin);
    std::fs::create_dir_all(&origin).expect("a directory for the remote");
    git_in(&origin, &["init", "--bare", "-b", "main"]);
    repository.git(&["remote", "add", "origin", origin.to_str().expect("a path")]);
    repository.git(&["branch", "farik/FRK-1"]);

    repository
        .adapter()
        .push("origin", "farik/FRK-1")
        .expect("the branch is pushed");

    assert_eq!(
        git_output_in(&origin, &["rev-parse", "farik/FRK-1"]),
        repository.git_output(&["rev-parse", "HEAD"])
    );
    let _ = std::fs::remove_dir_all(&origin);
}
