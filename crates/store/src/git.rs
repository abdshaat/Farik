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

/// What the tip of a branch is, as a board shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadSummary {
    /// The full commit hash.
    pub sha: String,
    /// When it was committed, as git's strict ISO 8601.
    pub committed_at: String,
    /// Its subject line.
    pub subject: String,
}

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

    /// The tip of the current branch, or nothing when the repository has no commit yet.
    ///
    /// # Errors
    ///
    /// `NotARepository`, `NotInstalled`, or `CommandFailed` when git refuses for another reason.
    pub fn head_summary(&self) -> Result<Option<HeadSummary>, GitError> {
        self.require_repository()?;
        if self
            .at_root(&["rev-parse", "--verify", "--quiet", "HEAD"])
            .is_err()
        {
            // A repository with no commit is the ordinary state of one `farik init` has just made.
            return Ok(None);
        }
        let line = self.at_root(&["log", "-1", "--no-color", "--format=%H%x1f%cI%x1f%s"])?;
        Ok(head_summary_of(&line))
    }

    /// The branch this repository treats as its default: what `origin/HEAD` points at.
    ///
    /// # Errors
    ///
    /// `NotARepository`, `NotInstalled`, or `CommandFailed` when even the current branch cannot be
    /// read.
    pub fn default_branch(&self) -> Result<String, GitError> {
        self.require_repository()?;
        match self.at_root(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
            // A repository with no remote records its default branch nowhere at all, so the branch
            // HEAD is on is the only answer available. `.farik/team.yaml` is where a team says
            // otherwise, and the integration branch is its to set (5.14).
            Err(_) => self.current_branch(),
            Ok(reference) => Ok(default_branch_of(&reference)),
        }
    }

    /// The branch that is checked out.
    ///
    /// # Errors
    ///
    /// `NotARepository`, `NotInstalled`, or `CommandFailed`; a detached head refuses, because a
    /// detached head is not a branch.
    pub fn current_branch(&self) -> Result<String, GitError> {
        self.require_repository()?;
        self.at_root(&["symbolic-ref", "--short", "HEAD"])
    }

    /// Refuses before running anything when there is no repository, so that every method says the
    /// same thing about it rather than each passing on whatever git happened to print.
    fn require_repository(&self) -> Result<(), GitError> {
        if self.is_repository() {
            Ok(())
        } else {
            Err(GitError::NotARepository)
        }
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

/// One `git log` line as a summary, or nothing when it is not one.
fn head_summary_of(line: &str) -> Option<HeadSummary> {
    let mut fields = line.split('\u{1f}');
    let sha = fields.next()?;
    let committed_at = fields.next()?;
    let subject = fields.next()?;
    if sha.is_empty() || committed_at.is_empty() {
        return None;
    }
    Some(HeadSummary {
        sha: sha.to_string(),
        committed_at: committed_at.to_string(),
        subject: subject.to_string(),
    })
}

/// `origin/main` as `main`: the remote's name is not part of the branch's.
fn default_branch_of(reference: &str) -> String {
    reference
        .split_once('/')
        .map_or_else(|| reference.to_string(), |(_, branch)| branch.to_string())
}

#[cfg(test)]
mod tests {
    use super::{GitError, HeadSummary, default_branch_of, head_summary_of};

    /// What `git log --format=%H%x1f%cI%x1f%s` prints for one commit.
    fn a_log_line(subject: &str) -> String {
        format!("a1b2c3\u{1f}2026-09-17T10:00:00+00:00\u{1f}{subject}")
    }

    #[test]
    fn reads_a_commit_out_of_the_line_git_prints() {
        assert_eq!(
            head_summary_of(&a_log_line("feat(store): add a git adapter")),
            Some(HeadSummary {
                sha: "a1b2c3".to_string(),
                committed_at: "2026-09-17T10:00:00+00:00".to_string(),
                subject: "feat(store): add a git adapter".to_string(),
            })
        );
    }

    #[test]
    fn reads_a_subject_that_holds_the_field_separator_is_not_possible() {
        // The separator is a unit separator, which git will not print inside a subject and no
        // commit message can hold: that is why it is the separator rather than a tab or a space.
        let summary = head_summary_of(&a_log_line("a subject\twith a tab"))
            .expect("a line with a tab in its subject is still a line");
        assert_eq!(summary.subject, "a subject\twith a tab");
    }

    #[test]
    fn refuses_a_line_that_is_not_a_commit() {
        for not_a_line in ["", "a1b2c3", "a1b2c3\u{1f}2026-09-17T10:00:00+00:00"] {
            assert_eq!(head_summary_of(not_a_line), None, "{not_a_line:?}");
        }
        assert_eq!(head_summary_of("\u{1f}\u{1f}a subject"), None, "no sha");
    }

    #[test]
    fn keeps_a_subject_that_is_empty() {
        // `git commit --allow-empty-message` makes one, and a board would rather show a commit with
        // no subject than no commit.
        let summary = head_summary_of(&a_log_line("")).expect("a commit with no subject");
        assert_eq!(summary.subject, "");
    }

    #[test]
    fn takes_the_remote_off_the_default_branch() {
        assert_eq!(default_branch_of("origin/main"), "main");
        assert_eq!(default_branch_of("upstream/trunk"), "trunk");
        // A branch whose own name holds a slash keeps the rest of it.
        assert_eq!(default_branch_of("origin/release/2.0"), "release/2.0");
        assert_eq!(default_branch_of("main"), "main", "no remote to take off");
    }

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
