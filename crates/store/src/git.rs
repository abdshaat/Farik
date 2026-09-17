//! The repository Farik works in, driven through the `git` program rather than reimplemented
//! (`docs/SPEC.md` sections 8.1 and 5.14).

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Why a git operation did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    /// The path Farik was given is not inside a git repository. `farik init` is what reports this
    /// to a user; nothing else in Farik can do anything useful without one.
    NotARepository,
    /// The `git` program could not be run at all.
    NotInstalled {
        /// What the operating system said.
        detail: String,
    },
    /// `git` ran and refused.
    CommandFailed {
        /// The arguments it was given, so that a person can run the same thing by hand.
        command: String,
        /// What it said on standard error, trimmed.
        stderr: String,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARepository => write!(formatter, "there is no git repository here"),
            Self::NotInstalled { detail } => write!(formatter, "git could not be run: {detail}"),
            Self::CommandFailed { command, stderr } => {
                write!(formatter, "git {command} refused: {stderr}")
            }
        }
    }
}

impl std::error::Error for GitError {}

/// One repository, at a path.
///
/// Every method runs `git` as a child process. Farik does not reimplement git: a repository is the
/// user's own, and the only behaviour anyone can rely on is the program's.
pub struct Git {
    root: PathBuf,
}

impl Git {
    /// The repository at `root`. Nothing is run and nothing is checked until a method is called, so
    /// this cannot fail; `is_repository` is what asks.
    #[must_use]
    pub fn open(root: PathBuf) -> Self {
        Self { root }
    }

    /// Whether `root` is inside a git repository.
    #[must_use]
    pub fn is_repository(&self) -> bool {
        self.at_root(&["rev-parse", "--git-dir"]).is_ok()
    }

    /// Runs git at the repository's root.
    fn at_root(&self, arguments: &[&str]) -> Result<String, GitError> {
        run_git(&self.root, arguments)
    }
}

/// Runs git in `directory` and hands back what it said on standard output, trimmed.
fn run_git(directory: &Path, arguments: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .map_err(|error| GitError::NotInstalled {
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(GitError::CommandFailed {
            command: arguments.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::GitError;

    #[test]
    fn says_what_it_refused_and_why_in_plain_words() {
        let said: Vec<String> = [
            GitError::NotARepository,
            GitError::NotInstalled {
                detail: "No such file or directory (os error 2)".to_string(),
            },
            GitError::CommandFailed {
                command: "branch farik/FRK-1 main".to_string(),
                stderr: "fatal: a branch named 'farik/FRK-1' already exists".to_string(),
            },
        ]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
        assert_eq!(
            said,
            [
                "there is no git repository here",
                "git could not be run: No such file or directory (os error 2)",
                "git branch farik/FRK-1 main refused: fatal: a branch named 'farik/FRK-1' already \
                 exists",
            ]
        );
    }
}
