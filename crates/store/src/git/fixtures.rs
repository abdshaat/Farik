//! A repository of its own, for tests in this crate and in others.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::Git;

/// A git repository of its own, removed when the value is dropped however the test ends.
pub struct TempRepo {
    /// The repository root.
    pub path: PathBuf,
}

impl TempRepo {
    /// A repository with one commit on `main`, holding `README.md`.
    ///
    /// # Panics
    ///
    /// When git is not installed or refuses, which is the machine refusing rather than the code. A
    /// test that uses this is `#[ignore]`d and run by `cargo xtask check --integration`.
    #[must_use]
    pub fn new(name: &str) -> Self {
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

    /// The adapter for this repository.
    #[must_use]
    pub fn adapter(&self) -> Git {
        Git::open(self.path.clone())
    }

    /// Writes a file, making the directory it lives in.
    ///
    /// # Panics
    ///
    /// When the file cannot be written.
    pub fn write(&self, name: &str, text: &str) {
        let path = self.path.join(name);
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory).expect("the directory the file is in");
        }
        std::fs::write(path, text).expect("the file is written");
    }

    /// Stages everything and commits it.
    ///
    /// # Panics
    ///
    /// When git refuses.
    pub fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
    }

    /// Stages everything and commits it as made at `date`, an ISO 8601 moment, so that a test
    /// that reads how long ago the last commit was does not read the wall clock.
    ///
    /// # Panics
    ///
    /// When git refuses.
    pub fn commit_at(&self, message: &str, date: &str) {
        self.git(&["add", "-A"]);
        let output = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Runs git in this repository, for the setup a test needs.
    ///
    /// # Panics
    ///
    /// When git refuses, which is a fixture that cannot be built rather than a test that failed.
    pub fn git(&self, arguments: &[&str]) {
        git_in(&self.path, arguments);
    }

    /// Asks git something in this repository, for a test that checks the adapter against git itself.
    ///
    /// # Panics
    ///
    /// When git refuses.
    #[must_use]
    pub fn git_output(&self, arguments: &[&str]) -> String {
        git_output_in(&self.path, arguments)
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
/// purpose, which is the whole reason Farik shells out to git at all. So no assertion resting on
/// anything a setting can reshape is safe — where one would, the adapter names the flag that makes
/// the answer its own rather than the configuration's.
///
/// # Panics
///
/// When git is not installed or refuses.
pub fn git_in(directory: &Path, arguments: &[&str]) {
    let _ = git_output_in(directory, arguments);
}

/// Asks git something in a directory that is not a `TempRepo` — a clone, an origin, a directory that
/// is not a repository at all.
///
/// # Panics
///
/// When git is not installed or refuses.
#[must_use]
pub fn git_output_in(directory: &Path, arguments: &[&str]) -> String {
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
