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

/// A repository of its own, for tests in this crate and in others.
pub mod fixtures;

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

/// What came of a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// It merged, leaving this commit.
    Merged {
        /// The merge commit's hash.
        sha: String,
    },
    /// It did not merge, and these paths are why. The tree is left as it was: a conflicted
    /// working tree nobody is watching is worse than a refusal (`docs/SPEC.md` 5.14).
    Conflicts(Vec<String>),
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

    /// The directory this adapter was opened on, which may be inside a repository rather than at
    /// its root. `top_level` is what git says the root is.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the repository begins, as git reports it.
    ///
    /// `Git::open` accepts any directory inside a repository, so the two can differ, and a caller
    /// that means the project rather than a subtree has to ask.
    ///
    /// # Errors
    ///
    /// `NotARepository`, `NotInstalled`, or `CommandFailed` when git refuses.
    pub fn top_level(&self) -> Result<String, GitError> {
        self.require_repository()?;
        self.at_root(&["rev-parse", "--show-toplevel"])
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

    /// Makes a branch at `from`, without checking it out.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when the name is taken or `from` names nothing.
    pub fn create_branch(&self, name: &str, from: &str) -> Result<(), GitError> {
        self.require_repository()?;
        self.at_root(&["branch", name, from])?;
        Ok(())
    }

    /// Makes a worktree at `path` on a new branch `branch`, starting from `from`.
    ///
    /// One worktree per task is what keeps two tasks from sharing a working tree (5.14).
    ///
    /// # Errors
    ///
    /// `CommandFailed` when the path is taken, the branch exists, or `from` names nothing.
    pub fn create_worktree(&self, path: &Path, branch: &str, from: &str) -> Result<(), GitError> {
        self.require_repository()?;
        let path = path_argument(path)?;
        self.at_root(&["worktree", "add", "-b", branch, &path, from])?;
        Ok(())
    }

    /// Removes the worktree at `path`, keeping its branch.
    ///
    /// Forced, because a task's worktree holds whatever its session built — untracked output that
    /// git would otherwise refuse to remove, and that nothing wants kept once the task is done.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when no worktree is registered at `path`.
    pub fn remove_worktree(&self, path: &Path) -> Result<(), GitError> {
        self.require_repository()?;
        let path = path_argument(path)?;
        self.at_root(&["worktree", "remove", "--force", &path])?;
        Ok(())
    }

    /// Makes a worktree at `path` with `at` checked out and no branch: a place to look at a commit
    /// that nothing will commit to, such as the base a task's new tests are run against (5.4).
    ///
    /// # Errors
    ///
    /// `CommandFailed` when the path is taken or `at` names nothing.
    pub fn create_detached_worktree(&self, path: &Path, at: &str) -> Result<(), GitError> {
        self.require_repository()?;
        let path = path_argument(path)?;
        self.at_root(&["worktree", "add", "--detach", &path, at])?;
        Ok(())
    }

    /// The commit `a` and `b` last agreed on: the one `a...b` diffs from.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when either name is unknown or they share no history.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<String, GitError> {
        self.require_repository()?;
        self.at_root(&["merge-base", a, b])
    }

    /// The file at `path` as `rev` has it, byte for byte (read as UTF-8 with replacement).
    ///
    /// # Errors
    ///
    /// `CommandFailed` when `rev` names nothing or has no such file.
    pub fn file_at(&self, rev: &str, path: &str) -> Result<String, GitError> {
        self.require_repository()?;
        // Not trimmed, unlike every other answer: a file's trailing newlines are its content.
        let object = format!("{rev}:{path}");
        run_git_untrimmed(&self.root, &["show", "--no-textconv", &object])
    }

    /// The paths `head` added or modified since it and `base` last agreed; a deleted path is not
    /// one, and a renamed file counts as added.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when either name is unknown.
    pub fn added_or_modified_paths(&self, base: &str, head: &str) -> Result<Vec<String>, GitError> {
        self.require_repository()?;
        let range = format!("{base}...{head}");
        let listed = self.at_root(&[
            "diff",
            "--no-renames",
            "--name-only",
            "--diff-filter=AM",
            "-z",
            &range,
        ])?;
        Ok(paths_of(&listed))
    }

    /// Whether the tree at `path` has nothing uncommitted, untracked files included.
    ///
    /// `--untracked-files=normal` is what makes that promise true rather than hopeful: a user with
    /// `status.showUntrackedFiles = no` in their own configuration would otherwise be told a
    /// worktree full of a session's output is clean.
    ///
    /// A file git ignores is not untracked and does not make a worktree dirty, deliberately: an
    /// ignore rule is how a person says a path is not content, and `.DS_Store` next to a task's
    /// work is not the task's work. Farik counts what git counts.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when `path` is not a working tree of this repository, including when it is
    /// not there at all.
    pub fn is_clean(&self, path: &Path) -> Result<bool, GitError> {
        Ok(self.status(path)?.is_empty())
    }

    /// What is uncommitted in the tree at `path`, as `git status --porcelain` prints it: one line
    /// per path, and nothing at all when the tree is clean. Untracked files are listed whatever
    /// the user configured, for the reason `is_clean` gives.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when `path` is not a working tree of this repository, including when it is
    /// not there at all.
    pub fn status(&self, path: &Path) -> Result<String, GitError> {
        self.require_repository()?;
        self.require_worktree_of_this_repository(path)?;
        run_git(path, &["status", "--porcelain", "--untracked-files=normal"])
    }

    /// Commits `paths` of the tree at `path` with `message`, and answers the new commit's sha.
    ///
    /// Only the named paths are staged (`git add -- <paths>`), so what else a session left in its
    /// worktree stays out of the commit; a new file is staged as readily as a changed one.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when `path` is not a working tree of this repository, a path matches
    /// nothing, or there is nothing to commit.
    pub fn commit(&self, path: &Path, message: &str, paths: &[String]) -> Result<String, GitError> {
        self.require_repository()?;
        self.require_worktree_of_this_repository(path)?;
        let mut add = vec!["add", "--"];
        add.extend(paths.iter().map(String::as_str));
        run_git(path, &add)?;
        run_git(path, &["commit", "-m", message])?;
        run_git(path, &["rev-parse", "HEAD"])
    }

    /// Pushes the local branch `branch` to `remote`, under the same name there.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when the remote or the branch is unknown, or the remote refuses the push.
    pub fn push(&self, remote: &str, branch: &str) -> Result<(), GitError> {
        self.require_repository()?;
        self.at_root(&["push", remote, branch])?;
        Ok(())
    }

    /// How many commits `head` has that `base` does not.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when either name is unknown; `InvalidCount` never — git counts.
    pub fn commit_count(&self, base: &str, head: &str) -> Result<u32, GitError> {
        self.require_repository()?;
        let range = format!("{base}..{head}");
        let counted = self.at_root(&["rev-list", "--count", &range])?;
        counted.parse().map_err(|_| GitError::CommandFailed {
            command: format!("rev-list --count {range}"),
            stderr: format!("answered {counted:?}, which is not a number of commits"),
        })
    }

    /// Every path `head` changed since it and `base` last agreed.
    ///
    /// Renames are reported as both sides, a removal and an addition, because the governor's
    /// allowed-paths rule is asked about each of them separately (`docs/SPEC.md` 5.6).
    ///
    /// # Errors
    ///
    /// `CommandFailed` when either name is unknown.
    pub fn changed_paths(&self, base: &str, head: &str) -> Result<Vec<String>, GitError> {
        self.require_repository()?;
        let range = format!("{base}...{head}");
        let listed = self.at_root(&["diff", "--no-renames", "--name-only", "-z", &range])?;
        Ok(paths_of(&listed))
    }

    /// Every path the repository tracks, in git's own order.
    ///
    /// Through git rather than by walking the directory, so that `.gitignore` decides what is not
    /// content without Farik having to know that `node_modules/` and `target/` exist. What git
    /// tracks is the index, so a repository with no commit yet lists what has been staged.
    ///
    /// # Errors
    ///
    /// `NotARepository`, `NotInstalled`, or `CommandFailed` when git refuses.
    pub fn tracked_paths(&self) -> Result<Vec<String>, GitError> {
        self.require_repository()?;
        let listed = self.at_root(&["ls-files", "-z"])?;
        Ok(paths_of(&listed))
    }

    /// What `head` changed since it and `base` last agreed, as a patch.
    ///
    /// The flags are what make this git's own patch in git's own shape, whatever the user has
    /// configured: `--no-ext-diff` because a personal external differ is not a patch, and the two
    /// prefixes because `diff.noprefix` would otherwise hand a reviewer a patch nothing can apply.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when either name is unknown.
    pub fn diff(&self, base: &str, head: &str) -> Result<String, GitError> {
        self.require_repository()?;
        let range = format!("{base}...{head}");
        self.at_root(&[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            &range,
        ])
    }

    /// Merges `from` into `into` with a merge commit, or reports what conflicted.
    ///
    /// A conflict leaves nothing behind: the merge is undone before this returns, because the tree
    /// it would leave is one nobody is watching and every later command would trip over it. The
    /// task is escalated with reason `integration` instead (5.14).
    ///
    /// The repository is left on the branch it was found on, whether the merge took or not. `root`
    /// is the user's own checkout rather than a task's worktree — worktrees are where tasks work
    /// (5.14) and the integration branch lives here — so a merge that moved it would change what a
    /// person has open in front of them.
    ///
    /// **One task integrates at a time, and nothing here enforces it** (`docs/SPEC.md` 5.14). This
    /// reads HEAD, moves it, and puts it back, so two of these at once on one repository interleave:
    /// measured on this code, sixteen runs in forty left the checkout on the wrong branch and one in
    /// forty landed the merge commit on a branch nobody named, while the caller was told it merged.
    /// The lock belongs to whatever drives integration — phase 3 step 10 — rather than to a method
    /// that cannot see the other caller. Everything else here is safe side by side: a worktree per
    /// task touches no shared head.
    ///
    /// A merge that lands and then cannot put the branch back comes back as the error from the
    /// checkout, and the merge commit stays where it landed. Nothing has reached that case, and the
    /// alternative — reporting success while the repository sits somewhere the caller did not ask
    /// for — is worse to be wrong about.
    ///
    /// # Errors
    ///
    /// `CommandFailed` when either name is unknown, when the head is detached and there is no
    /// branch to put back, or when the tree is not clean enough to switch branches.
    pub fn merge(&self, into: &str, from: &str, message: &str) -> Result<MergeOutcome, GitError> {
        self.require_repository()?;
        let was_on = self.current_branch()?;
        self.at_root(&["checkout", into])?;
        let outcome = self.merge_what_is_checked_out(from, message);
        if was_on == into {
            return outcome;
        }
        let restored = self.at_root(&["checkout", &was_on]);
        // The merge's own answer comes first. A branch that could not be put back is worth
        // reporting, but not in place of the reason the merge itself refused.
        outcome.and_then(|merged| restored.map(|_| merged))
    }

    /// The merge itself, with `into` already checked out.
    fn merge_what_is_checked_out(
        &self,
        from: &str,
        message: &str,
    ) -> Result<MergeOutcome, GitError> {
        match self.at_root(&["merge", "--no-ff", "-m", message, from]) {
            Ok(_) => Ok(MergeOutcome::Merged {
                sha: self.at_root(&["rev-parse", "HEAD"])?,
            }),
            Err(refusal) => {
                let conflicted = self.at_root(&["diff", "--name-only", "--diff-filter=U", "-z"])?;
                let conflicts = paths_of(&conflicted);
                if conflicts.is_empty() {
                    // It refused for some other reason, and that reason is the answer.
                    return Err(refusal);
                }
                self.at_root(&["merge", "--abort"])?;
                Ok(MergeOutcome::Conflicts(conflicts))
            }
        }
    }

    /// Refuses a path that is not a working tree of this repository: another repository's own tree,
    /// or a directory that is not one at all.
    ///
    /// Every worktree of one repository shares its common directory and no two repositories share
    /// theirs, so that is what "of this repository" means. Asked because `is_clean` is the only
    /// method that takes a path rather than working at the root, and a task's worktree is a path
    /// the runtime hands in (5.14) — an answer about somebody else's repository would be a task
    /// passed or failed on a tree nobody looked at.
    fn require_worktree_of_this_repository(&self, path: &Path) -> Result<(), GitError> {
        let mine = self.at_root(&COMMON_DIRECTORY)?;
        let theirs = run_git(path, &COMMON_DIRECTORY)?;
        if mine == theirs {
            return Ok(());
        }
        Err(GitError::CommandFailed {
            command: COMMON_DIRECTORY.join(" "),
            stderr: format!(
                "{} is not a working tree of this repository",
                path.display()
            ),
        })
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

/// What every worktree of one repository shares and no two repositories do. `--path-format` makes
/// the answer absolute, so a main worktree's `.git` and a linked worktree's path to the same
/// directory are the same string; it wants git 2.31 or newer, which `docs/standards/code.md`
/// records.
const COMMON_DIRECTORY: [&str; 3] = ["rev-parse", "--path-format=absolute", "--git-common-dir"];

/// Runs git in `directory` and hands back what it said on standard output, with the newline git
/// ends it with taken off.
///
/// Trimmed at the end only. A path may begin with a space, and `changed_paths` hands what comes
/// back to the rule that asks about each path a change touched (5.6): a path a byte short is a
/// change checked against a rule it never matched.
fn run_git(directory: &Path, arguments: &[&str]) -> Result<String, GitError> {
    Ok(run_git_untrimmed(directory, arguments)?
        .trim_end()
        .to_string())
}

/// Runs git in `directory` and hands back what it said on standard output, exactly.
fn run_git_untrimmed(directory: &Path, arguments: &[&str]) -> Result<String, GitError> {
    if !directory.is_dir() {
        // Said here, because the operating system answers a missing working directory with the same
        // "not found" it answers a missing program with, and "git could not be run" is the wrong
        // thing to tell someone whose worktree a crashed session took away.
        return Err(GitError::CommandFailed {
            command: arguments.join(" "),
            stderr: format!("there is no directory at {}", directory.display()),
        });
    }
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
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

/// A path as git takes it, refusing one that is not text.
///
/// Every path Farik hands git it built itself, from `.farik/local/worktrees/` and a task id, so
/// this is about a repository somewhere a user's own path is not UTF-8.
fn path_argument(path: &Path) -> Result<String, GitError> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| GitError::CommandFailed {
            command: "worktree".to_string(),
            stderr: format!("the path {} is not text git can be given", path.display()),
        })
}

/// git's `-z` output as paths. Nothing is split on a newline, because a path may hold one.
fn paths_of(listed: &str) -> Vec<String> {
    listed
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        Git, GitError, HeadSummary, default_branch_of, head_summary_of, path_argument, paths_of,
    };

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
        // One empty field each, so that both halves of the guard are held to something.
        assert_eq!(
            head_summary_of("\u{1f}2026-09-17T10:00:00+00:00\u{1f}a subject"),
            None,
            "no sha"
        );
        assert_eq!(
            head_summary_of("a1b2c3\u{1f}\u{1f}a subject"),
            None,
            "no time"
        );
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
    fn reads_the_paths_git_separated_by_nothing() {
        assert_eq!(
            paths_of("src/lib.rs\0docs/SPEC.md\0"),
            ["src/lib.rs", "docs/SPEC.md"]
        );
        assert_eq!(paths_of(""), Vec::<String>::new());
        // A newline in a path is what `-z` is for: nothing here splits on one.
        assert_eq!(
            paths_of("a file\nwith a newline\0other.rs\0"),
            ["a file\nwith a newline", "other.rs"]
        );
    }

    #[test]
    fn refuses_a_path_that_is_not_text_git_can_be_given() {
        assert!(path_argument(&PathBuf::from("crates/store")).is_ok());
        #[cfg(unix)]
        {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;
            let not_text = PathBuf::from(OsString::from_vec(vec![0xff, 0xfe]));
            assert!(matches!(
                path_argument(&not_text),
                Err(GitError::CommandFailed { .. })
            ));
        }
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

    #[test]
    fn says_there_is_no_repository_before_it_runs_anything() {
        // Every method asks first, so that a user is told the one thing that is wrong rather than
        // whatever git prints about a directory it has never heard of.
        let nowhere = Git::open(std::env::temp_dir().join("farik-not-a-repository-at-all"));
        assert!(!nowhere.is_repository());
        assert_eq!(nowhere.head_summary(), Err(GitError::NotARepository));
        assert_eq!(nowhere.default_branch(), Err(GitError::NotARepository));
        assert_eq!(nowhere.current_branch(), Err(GitError::NotARepository));
        assert_eq!(
            nowhere.create_branch("farik/FRK-1", "main"),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.create_worktree(Path::new("worktrees/FRK-1"), "farik/FRK-1", "main"),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.remove_worktree(Path::new("worktrees/FRK-1")),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.is_clean(Path::new(".")),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.commit_count("main", "farik/FRK-1"),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.changed_paths("main", "farik/FRK-1"),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.diff("main", "farik/FRK-1"),
            Err(GitError::NotARepository)
        );
        assert_eq!(
            nowhere.merge("main", "farik/FRK-1", "a message"),
            Err(GitError::NotARepository)
        );
    }
}
