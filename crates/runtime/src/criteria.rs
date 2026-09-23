//! A contract's exit criteria, run and judged (`docs/SPEC.md` 5.4, 5.13): commands, tests, and
//! artifacts through an executor, and the questions a reviewer or the human has to answer.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use farik_core::contract::{ExitCriterion, TaskContract, TaskId, Verification};
use farik_core::governor::done::{CriterionResult, RunBy};
use farik_core::governor::paths::normalise;
use farik_store::{Git, GitError};

use crate::exec::{ExecError, ExecResult, Executor};
use crate::sandbox::{SandboxError, SandboxFactory};

/// How long one criterion may run, so that a suite that hangs is a failed criterion rather than
/// a stuck verify session.
pub const CRITERION_TIMEOUT: Duration = Duration::from_mins(15);

/// How much of the end of each stream the evidence keeps: a test runner's summary is at the end.
const TAIL_BYTES: usize = 2000;

/// What came of one criterion: a result, or the question someone has to answer instead (5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionOutcome {
    /// It ran, and this is its verdict and evidence.
    Result(CriterionResult),
    /// A `review` criterion: a reviewer answers each question with a cited reason.
    NeedsReview {
        /// The questions.
        rubric: Vec<String>,
    },
    /// A `human` criterion: only the human's acceptance satisfies it.
    NeedsHuman {
        /// What the human is asked to confirm.
        question: String,
    },
}

/// Why a criterion could not be run at all. A criterion that runs and fails is a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionError {
    /// The executor did not run the command.
    Exec(ExecError),
    /// Git refused, in the new-tests check.
    Git(GitError),
    /// The base run's sandbox could not be made or discarded.
    Sandbox(SandboxError),
    /// A head test file could not be written into the base worktree, or the worktree removed.
    Io {
        /// The file or directory.
        path: String,
        /// What the operating system said.
        detail: String,
    },
}

impl fmt::Display for CriterionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exec(error) => write!(formatter, "the criterion could not be run: {error}"),
            Self::Git(error) => {
                write!(formatter, "the new-tests check could not read git: {error}")
            }
            Self::Sandbox(error) => {
                write!(formatter, "the base-branch run has no sandbox: {error}")
            }
            Self::Io { path, detail } => {
                write!(
                    formatter,
                    "{path} could not be written on the base branch: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for CriterionError {}

impl From<ExecError> for CriterionError {
    fn from(error: ExecError) -> Self {
        Self::Exec(error)
    }
}

impl From<GitError> for CriterionError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

impl From<SandboxError> for CriterionError {
    fn from(error: SandboxError) -> Self {
        Self::Sandbox(error)
    }
}

/// What the new-tests check found in a task's diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTestsCheck {
    /// Whether the diff adds or modifies a test file.
    pub adds_tests: bool,
    /// Whether the test command, with those files, fails on the base branch.
    pub fails_on_base: bool,
    /// The test files the diff adds or modifies, in git's order.
    pub test_files: Vec<String>,
}

/// Where a task's diff is, and how to run its tests against the base branch.
pub struct NewTestsInput<'a> {
    /// The project's repository.
    pub git: &'a Git,
    /// The branch the task's branch is judged against.
    pub base: &'a str,
    /// The task's branch.
    pub head: &'a str,
    /// Where the base run's sandbox comes from.
    pub sandboxes: &'a dyn SandboxFactory,
    /// The project, for naming the base sandbox.
    pub project_id: &'a str,
    /// The task, for naming the base worktree and sandbox.
    pub task_id: &'a TaskId,
}

/// Runs `criterion` in `executor`'s workspace root with `timeout`, and stamps `run_by` on the
/// result. `review` and `human` criteria run nothing and come back as their questions. A `test`
/// criterion that requires new tests fails here, since the diff is not known: `run_criteria` is
/// what runs the base-branch check.
///
/// # Errors
///
/// `Exec` when the executor does not run the command.
pub fn run_criterion(
    criterion: &ExitCriterion,
    executor: &dyn Executor,
    run_by: RunBy,
    timeout: Duration,
) -> Result<CriterionOutcome, CriterionError> {
    judge(criterion, executor, run_by, timeout, None)
}

/// Runs every exit criterion of `contract`, in the contract's order, each with
/// `CRITERION_TIMEOUT`, and a `test` criterion that requires new tests with the base-branch check
/// on `new_tests` when it is given.
///
/// # Errors
///
/// The first criterion that could not be run stops the rest, with its error.
pub fn run_criteria(
    contract: &TaskContract,
    executor: &dyn Executor,
    run_by: RunBy,
    new_tests: Option<&NewTestsInput<'_>>,
) -> Result<Vec<CriterionOutcome>, CriterionError> {
    contract
        .exit_criteria
        .iter()
        .map(|criterion| judge(criterion, executor, run_by, CRITERION_TIMEOUT, new_tests))
        .collect()
}

/// Whether the diff from `input.base` to `input.head` adds a test file, and whether `command`
/// fails with the head's test files on the base branch: in a detached worktree at the merge base,
/// `<root>/.farik/local/worktrees/<id>-base` (one left by a crash removed first), in a sandbox
/// from `create_base`. The sandbox is discarded and the worktree removed on every way out; when
/// the run and the cleanup both fail, the run's error is the one returned.
///
/// # Errors
///
/// `Git` when git refuses, `Io` when a test file cannot be written, `Sandbox` when the base
/// sandbox cannot be made or discarded, and `Exec` when the command does not run.
pub fn check_new_tests(
    command: &str,
    input: &NewTestsInput<'_>,
) -> Result<NewTestsCheck, CriterionError> {
    let test_files: Vec<String> = input
        .git
        .added_or_modified_paths(input.base, input.head)?
        .into_iter()
        .filter(|path| is_test_file(path))
        .collect();
    if test_files.is_empty() {
        return Ok(NewTestsCheck {
            adds_tests: false,
            fails_on_base: false,
            test_files,
        });
    }
    let at = input.git.merge_base(input.base, input.head)?;
    let worktree = input
        .git
        .root()
        .join(".farik/local/worktrees")
        .join(format!("{}-base", input.task_id.as_str()));
    remove_base_worktree(input.git, &worktree)?;
    let ran = input
        .git
        .create_detached_worktree(&worktree, &at)
        .map_err(CriterionError::from)
        .and_then(|()| run_on_base(command, input, &test_files, &worktree));
    let removed = remove_base_worktree(input.git, &worktree);
    let fails_on_base = ran?;
    removed?;
    Ok(NewTestsCheck {
        adds_tests: true,
        fails_on_base,
        test_files,
    })
}

/// Whether `path` names a test file: a directory `tests`, `test`, `__tests__`, or `spec` on the
/// way, or a file name `test_*`, `*_test.*`, `*.test.*`, `*.spec.*`, or `*_spec.*`. A unit test
/// inside a source file does not count (a known ceiling; the upgrade is a per-language hunk
/// filter).
#[must_use]
pub fn is_test_file(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let name = segments.last().copied().unwrap_or_default();
    segments
        .iter()
        .any(|segment| matches!(*segment, "tests" | "test" | "__tests__" | "spec"))
        || name.starts_with("test_")
        || ["_test.", ".test.", ".spec.", "_spec."]
            .iter()
            .any(|infix| name.contains(infix))
}

fn judge(
    criterion: &ExitCriterion,
    executor: &dyn Executor,
    run_by: RunBy,
    timeout: Duration,
    new_tests: Option<&NewTestsInput<'_>>,
) -> Result<CriterionOutcome, CriterionError> {
    let (passed, evidence) = match Verification::from(&criterion.verification) {
        Verification::Review { rubric } => return Ok(CriterionOutcome::NeedsReview { rubric }),
        Verification::Human { question } => return Ok(CriterionOutcome::NeedsHuman { question }),
        Verification::Command {
            command,
            exit_code,
            stdout_contains,
            stdout_not_contains,
        } => {
            let ran = run(executor, &command, timeout)?;
            let mut lines = opening(&command, &ran);
            let mut passed = !ran.timed_out && ran.exit_code == exit_code;
            if let Some(wanted) = stdout_contains {
                let holds = ran.stdout.contains(&wanted);
                lines.push(answer(&format!("stdout contains \"{wanted}\""), holds));
                passed &= holds;
            }
            if let Some(unwanted) = stdout_not_contains {
                let holds = !ran.stdout.contains(&unwanted);
                lines.push(answer(
                    &format!("stdout does not contain \"{unwanted}\""),
                    holds,
                ));
                passed &= holds;
            }
            (passed, closing(lines, &ran))
        }
        Verification::Test {
            command,
            new_tests_required,
        } => {
            let ran = run(executor, &command, timeout)?;
            let mut lines = opening(&command, &ran);
            let mut passed = !ran.timed_out && ran.exit_code == 0;
            if new_tests_required {
                if let Some(input) = new_tests {
                    let check = check_new_tests(&command, input)?;
                    lines.push(answer("adds tests", check.adds_tests));
                    lines.push(answer("fails on base", check.fails_on_base));
                    lines.push(format!("test files: {}", check.test_files.join(", ")));
                    passed &= check.adds_tests && check.fails_on_base;
                } else {
                    lines.push("the base-branch check was not run: no diff was given".to_owned());
                    passed = false;
                }
            }
            (passed, closing(lines, &ran))
        }
        Verification::Artifact { path, must_contain } => {
            let Some(relative) = normalise(&path) else {
                let why = outside_why(&path);
                let evidence = format!("the artifact path \"{path}\" {why}: nothing was read");
                return Ok(verdict(criterion, run_by, false, evidence));
            };
            let command = format!("cat -- '{}'", relative.replace('\'', r"'\''"));
            let ran = run(executor, &command, timeout)?;
            let mut lines = opening(&command, &ran);
            let mut passed = !ran.timed_out && ran.exit_code == 0 && !ran.truncated;
            if ran.truncated {
                lines.push("file larger than 1 MiB".to_owned());
            }
            for wanted in &must_contain {
                let holds = ran.stdout.contains(wanted.as_str());
                lines.push(answer(&format!("contains \"{wanted}\""), holds));
                passed &= holds;
            }
            (passed, closing(lines, &ran))
        }
    };
    Ok(verdict(criterion, run_by, passed, evidence))
}

fn verdict(
    criterion: &ExitCriterion,
    run_by: RunBy,
    passed: bool,
    evidence: String,
) -> CriterionOutcome {
    CriterionOutcome::Result(CriterionResult {
        criterion_id: criterion.id.to_string(),
        passed,
        evidence,
        run_by,
    })
}

/// Runs `command` in the workspace root with no added environment.
fn run(executor: &dyn Executor, command: &str, timeout: Duration) -> Result<ExecResult, ExecError> {
    executor.run(command, "", timeout, &BTreeMap::new())
}

/// The command, and how it ended.
fn opening(command: &str, ran: &ExecResult) -> Vec<String> {
    let ended = if ran.timed_out {
        "timed out".to_owned()
    } else {
        format!("exit {}", ran.exit_code)
    };
    vec![format!("$ {command}"), ended]
}

fn answer(question: &str, holds: bool) -> String {
    format!("{question}: {}", if holds { "yes" } else { "no" })
}

/// The lines so far, then the tail of each stream that said anything.
fn closing(mut lines: Vec<String>, ran: &ExecResult) -> String {
    for (name, text) in [("stdout", &ran.stdout), ("stderr", &ran.stderr)] {
        if !text.is_empty() {
            lines.push(format!("{name} (last {TAIL_BYTES} bytes):"));
            lines.push(tail(text).to_owned());
        }
    }
    lines.join("\n")
}

/// The last `TAIL_BYTES` of `text`, or fewer, cut on a character boundary.
fn tail(text: &str) -> &str {
    let mut start = text.len().saturating_sub(TAIL_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Why `normalise` refused `path`, in the words the evidence uses.
fn outside_why(path: &str) -> &'static str {
    let unified = path.replace('\\', "/");
    if unified.starts_with('/') || unified.chars().nth(1) == Some(':') {
        "is absolute"
    } else if unified
        .split('/')
        .all(|segment| segment.is_empty() || segment == ".")
    {
        "names the workspace itself"
    } else {
        "climbs out of the workspace"
    }
}

/// Writes the head's `test_files` into the base `worktree` and runs `command` there; whether it
/// failed, a timeout counting as a failure.
fn run_on_base(
    command: &str,
    input: &NewTestsInput<'_>,
    test_files: &[String],
    worktree: &Path,
) -> Result<bool, CriterionError> {
    for path in test_files {
        let content = input.git.file_at(input.head, path)?;
        let target = worktree.join(path);
        let written = match target.parent() {
            Some(parent) => std::fs::create_dir_all(parent),
            None => Ok(()),
        }
        .and_then(|()| std::fs::write(&target, content));
        written.map_err(|error| CriterionError::Io {
            path: path.clone(),
            detail: error.to_string(),
        })?;
    }
    let sandbox = input
        .sandboxes
        .create_base(input.project_id, input.task_id, worktree)?;
    let ran = run(sandbox.as_ref(), command, CRITERION_TIMEOUT);
    let discarded = sandbox.discard();
    let ran = ran?;
    discarded?;
    Ok(ran.timed_out || ran.exit_code != 0)
}

/// Removes the base worktree at `worktree`, whether git has it registered or it is a directory a
/// crash left behind that git no longer knows.
pub(crate) fn remove_base_worktree(git: &Git, worktree: &Path) -> Result<(), CriterionError> {
    match git.remove_worktree(worktree) {
        // ponytail: git's English words for a path it has no worktree at; a translated git
        // refuses here instead, and pinning the store's git to one locale is the upgrade.
        Err(GitError::CommandFailed { stderr, .. }) if stderr.contains("is not a working tree") => {
        }
        other => other?,
    }
    match std::fs::remove_dir_all(worktree) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(CriterionError::Io {
            path: worktree.display().to_string(),
            detail: error.to_string(),
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use farik_core::contract::ExitCriterion;
    use farik_core::governor::done::RunBy;
    use serde_json::{Value, json};

    use super::{CriterionOutcome, is_test_file, run_criterion};
    use crate::exec::{ExecError, ExecResult, Executor};
    use crate::sandbox::host::HostSandbox;

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn fresh_root(test: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("farik-criteria-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temporary directory can be made");
        root
    }

    fn criterion(verification: &Value) -> ExitCriterion {
        serde_json::from_value(json!({
            "id": "C1",
            "text": "The criterion holds.",
            "verification": verification,
        }))
        .expect("a criterion the schema accepts")
    }

    fn command(command: &str, expect: &Value) -> ExitCriterion {
        criterion(&json!({ "method": "command", "command": command, "expect": expect }))
    }

    fn artifact(path: &str, must_contain: &[&str]) -> ExitCriterion {
        criterion(&json!({ "method": "artifact", "path": path, "must_contain": must_contain }))
    }

    /// The result, and its evidence as lines.
    fn run(
        criterion: &ExitCriterion,
        executor: &dyn Executor,
        timeout: Duration,
    ) -> (bool, RunBy, Vec<String>) {
        match run_criterion(criterion, executor, RunBy::Reviewer, timeout).expect("it runs") {
            CriterionOutcome::Result(result) => {
                assert_eq!(result.criterion_id, "C1");
                let lines = result.evidence.lines().map(ToOwned::to_owned).collect();
                (result.passed, result.run_by, lines)
            }
            other => panic!("not a result: {other:?}"),
        }
    }

    fn has(lines: &[String], line: &str) -> bool {
        lines.iter().any(|each| each == line)
    }

    #[test]
    fn passes_a_command_that_exits_as_expected_with_the_output_asked_for() {
        let sandbox = HostSandbox::new(fresh_root("command-passes"));
        let (passed, run_by, lines) = run(
            &command(
                "printf ok",
                &json!({ "exit_code": 0, "stdout_contains": "ok" }),
            ),
            &sandbox,
            TIMEOUT,
        );
        assert!(passed, "{lines:?}");
        assert_eq!(run_by, RunBy::Reviewer);
        for line in ["$ printf ok", "exit 0", "stdout contains \"ok\": yes"] {
            assert!(has(&lines, line), "{line} in {lines:?}");
        }
    }

    #[test]
    fn fails_a_command_whose_output_lacks_what_it_must_contain() {
        let sandbox = HostSandbox::new(fresh_root("command-lacks"));
        let (passed, _, lines) = run(
            &command(
                "printf ok",
                &json!({ "exit_code": 0, "stdout_contains": "done" }),
            ),
            &sandbox,
            TIMEOUT,
        );
        assert!(!passed);
        assert!(has(&lines, "stdout contains \"done\": no"), "{lines:?}");
    }

    #[test]
    fn fails_a_command_whose_output_holds_what_it_must_not() {
        let sandbox = HostSandbox::new(fresh_root("command-holds"));
        let (passed, _, lines) = run(
            &command(
                "printf error",
                &json!({ "exit_code": 0, "stdout_not_contains": "error" }),
            ),
            &sandbox,
            TIMEOUT,
        );
        assert!(!passed);
        assert!(
            has(&lines, "stdout does not contain \"error\": no"),
            "{lines:?}"
        );
    }

    #[test]
    fn fails_a_command_that_times_out_whatever_its_code() {
        let sandbox = HostSandbox::new(fresh_root("command-times-out"));
        let (passed, _, lines) = run(
            &command("sleep 5", &json!({ "exit_code": 137 })),
            &sandbox,
            Duration::from_secs(1),
        );
        assert!(!passed);
        assert!(has(&lines, "timed out"), "{lines:?}");
    }

    #[test]
    fn passes_an_artifact_that_holds_every_string() {
        let root = fresh_root("artifact-holds");
        std::fs::write(root.join("notes.txt"), "alpha beta").expect("the file is written");
        let sandbox = HostSandbox::new(root);
        let (passed, _, lines) = run(
            &artifact("notes.txt", &["alpha", "beta"]),
            &sandbox,
            TIMEOUT,
        );
        assert!(passed, "{lines:?}");
        assert!(has(&lines, "contains \"beta\": yes"), "{lines:?}");
    }

    #[test]
    fn fails_an_artifact_that_is_missing() {
        let sandbox = HostSandbox::new(fresh_root("artifact-missing"));
        let (passed, _, lines) = run(&artifact("absent.txt", &[]), &sandbox, TIMEOUT);
        assert!(!passed);
        assert!(has(&lines, "$ cat -- 'absent.txt'"), "{lines:?}");
    }

    /// An executor that only counts what it was asked to run.
    #[derive(Default)]
    struct Counting {
        runs: AtomicUsize,
    }

    impl Executor for Counting {
        fn run(
            &self,
            _command: &str,
            _cwd: &str,
            _timeout: Duration,
            _env: &BTreeMap<String, String>,
        ) -> Result<ExecResult, ExecError> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            Ok(ExecResult {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                truncated: false,
            })
        }
    }

    #[test]
    fn fails_an_artifact_outside_the_workspace() {
        let counting = Counting::default();
        for (path, why) in [
            ("../etc/passwd", "climbs out of the workspace"),
            ("/etc/passwd", "is absolute"),
            (".", "names the workspace itself"),
        ] {
            let (passed, _, lines) = run(&artifact(path, &[]), &counting, TIMEOUT);
            assert!(!passed, "{path}");
            assert!(
                lines.iter().any(|line| line.contains(why)),
                "{path}: {lines:?}"
            );
        }
        assert_eq!(counting.runs.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reads_an_artifact_whose_name_has_a_quote() {
        let root = fresh_root("artifact-quote");
        std::fs::write(root.join("it's.txt"), "alpha").expect("the file is written");
        let sandbox = HostSandbox::new(root);
        let (passed, _, lines) = run(&artifact("it's.txt", &["alpha"]), &sandbox, TIMEOUT);
        assert!(passed, "{lines:?}");
    }

    #[test]
    fn asks_for_a_review_or_a_human_instead_of_running() {
        let counting = Counting::default();
        let review = criterion(&json!({ "method": "review", "rubric": ["Is it tidy?"] }));
        assert_eq!(
            run_criterion(&review, &counting, RunBy::Reviewer, TIMEOUT),
            Ok(CriterionOutcome::NeedsReview {
                rubric: vec!["Is it tidy?".to_owned()]
            })
        );
        let human = criterion(&json!({ "method": "human", "question": "Did it work?" }));
        assert_eq!(
            run_criterion(&human, &counting, RunBy::Reviewer, TIMEOUT),
            Ok(CriterionOutcome::NeedsHuman {
                question: "Did it work?".to_owned()
            })
        );
        assert_eq!(counting.runs.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn keeps_the_last_two_thousand_bytes_of_output() {
        let sandbox = HostSandbox::new(fresh_root("tail"));
        let criterion = command(
            "head -c 5000 /dev/zero | tr '\\0' a; printf END",
            &json!({ "exit_code": 0 }),
        );
        let CriterionOutcome::Result(result) =
            run_criterion(&criterion, &sandbox, RunBy::Assignee, TIMEOUT).expect("it runs")
        else {
            panic!("not a result");
        };
        assert!(result.evidence.ends_with("END"), "{}", result.evidence);
        let (_, stdout) = result
            .evidence
            .split_once("stdout (last 2000 bytes):\n")
            .expect("the stdout section");
        assert!(stdout.len() <= 2000, "{} bytes", stdout.len());
    }

    #[test]
    fn names_test_files_by_their_path() {
        for path in [
            "tests/a.rs",
            "src/__tests__/x.js",
            "pkg/test_util.py",
            "a_test.go",
            "b.test.ts",
            "c.spec.js",
            "d_spec.rb",
        ] {
            assert!(is_test_file(path), "{path} is a test file");
        }
        for path in ["src/lib.rs", "latest.txt", "contest/x.rs"] {
            assert!(!is_test_file(path), "{path} is not a test file");
        }
    }

    fn test(command: &str, new_tests_required: bool) -> ExitCriterion {
        criterion(&json!({
            "method": "test",
            "command": command,
            "new_tests_required": new_tests_required,
        }))
    }

    #[test]
    fn passes_a_test_criterion_that_exits_zero() {
        let sandbox = HostSandbox::new(fresh_root("test-passes"));
        let (passed, _, lines) = run(&test("printf '3 passed'", false), &sandbox, TIMEOUT);
        assert!(passed, "{lines:?}");
        for line in ["$ printf '3 passed'", "exit 0", "3 passed"] {
            assert!(has(&lines, line), "{line} in {lines:?}");
        }
    }

    #[test]
    fn fails_one_that_does_not() {
        let sandbox = HostSandbox::new(fresh_root("test-fails"));
        let (passed, _, lines) = run(&test("exit 1", false), &sandbox, TIMEOUT);
        assert!(!passed);
        assert!(has(&lines, "exit 1"), "{lines:?}");
    }

    #[test]
    fn fails_a_new_tests_criterion_run_without_the_diff() {
        let sandbox = HostSandbox::new(fresh_root("test-no-diff"));
        let (passed, _, lines) = run(&test("true", true), &sandbox, TIMEOUT);
        assert!(!passed, "a test that exits 0 is not enough: {lines:?}");
        assert!(
            has(
                &lines,
                "the base-branch check was not run: no diff was given"
            ),
            "{lines:?}"
        );
    }
}
