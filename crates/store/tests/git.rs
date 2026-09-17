//! The git adapter against a real repository.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`. They are ignored rather than compiled out: the default check
//! still builds them, so `cargo fmt` and `clippy` hold them to the same standard as everything else
//! and they cannot rot unnoticed (`docs/standards/code.md`, "Rust integration test").

use std::path::{Path, PathBuf};
use std::process::Command;

use farik_store::Git;

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
