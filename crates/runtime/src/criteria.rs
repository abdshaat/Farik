//! A contract's exit criteria, run rather than described (`docs/SPEC.md` 5.4 and 5.13): a command
//! judged by its exit code and output, a test command, a file's presence and content. A `review`
//! or `human` criterion is handed back as what somebody has to answer.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use farik_core::contract::{ExitCriterion, TaskContract, Verification};
use farik_core::governor::done::{CriterionResult, RunBy};
use farik_core::governor::paths::normalise;
use farik_store::GitError;

use crate::exec::{ExecError, ExecResult, Executor, OUTPUT_LIMIT_BYTES};
use crate::sandbox::SandboxError;

/// How long one criterion may run, so that a test suite that hangs is a failed criterion rather
/// than a stuck verify session.
pub const CRITERION_TIMEOUT: Duration = Duration::from_mins(15);

/// How much of each stream the evidence keeps: the end, where a test runner prints its summary.
const TAIL_BYTES: usize = 2000;

/// What came of a criterion: a result, or the question somebody has to answer instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionOutcome {
    /// It ran, and this is what came of it.
    Result(CriterionResult),
    /// A `review` criterion: the reviewer answers each question with a cited reason.
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

/// Why a criterion could not be run at all. A criterion that fails is a result, not this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CriterionError {
    /// The command could not be run.
    Exec(ExecError),
    /// Git refused something the base-branch check needed.
    Git(GitError),
    /// The base-branch check's sandbox could not be made or discarded.
    Sandbox(SandboxError),
}

impl fmt::Display for CriterionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exec(error) => write!(formatter, "the criterion could not be run: {error}"),
            Self::Git(error) => write!(
                formatter,
                "the base-branch check could not be made: {error}"
            ),
            Self::Sandbox(error) => {
                write!(formatter, "the base-branch check had no sandbox: {error}")
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

/// Runs one criterion in the executor's workspace root, stopping any command at `timeout`, and
/// stamps the result with `run_by`. A `review` or `human` criterion runs nothing.
///
/// # Errors
///
/// `Exec` when the command could not be run at all.
pub fn run_criterion(
    criterion: &ExitCriterion,
    executor: &dyn Executor,
    run_by: RunBy,
    timeout: Duration,
) -> Result<CriterionOutcome, CriterionError> {
    let (passed, evidence) = match Verification::from(&criterion.verification) {
        Verification::Command {
            command,
            exit_code,
            stdout_contains,
            stdout_not_contains,
        } => {
            let result = executor.run(&command, "", timeout, &BTreeMap::new())?;
            let mut evidence = Evidence::ran(&command, &result);
            let mut passed = !result.timed_out && result.exit_code == exit_code;
            if let Some(wanted) = stdout_contains {
                let holds = result.stdout.contains(&wanted);
                evidence.line(format!("stdout contains {wanted:?}: {}", yes(holds)));
                passed &= holds;
            }
            if let Some(unwanted) = stdout_not_contains {
                let lacks = !result.stdout.contains(&unwanted);
                evidence.line(format!(
                    "stdout does not contain {unwanted:?}: {}",
                    yes(lacks)
                ));
                passed &= lacks;
            }
            (passed, evidence.with_output(&result))
        }
        Verification::Test { command, .. } => (
            false,
            format!("$ {command}\ntest criteria are not run by this build"),
        ),
        Verification::Artifact { path, must_contain } => {
            let Some(relative) = normalise(&path) else {
                return Ok(result_of(
                    criterion,
                    run_by,
                    false,
                    format!(
                        "the path {path:?} {}, so nothing was read",
                        why_outside(&path)
                    ),
                ));
            };
            let command = format!("cat -- {}", single_quoted(&relative));
            let result = executor.run(&command, "", timeout, &BTreeMap::new())?;
            let mut evidence = Evidence::ran(&command, &result);
            let mut passed = !result.timed_out && result.exit_code == 0;
            if result.truncated {
                evidence.line(format!(
                    "file larger than {} MiB",
                    OUTPUT_LIMIT_BYTES / (1024 * 1024)
                ));
                passed = false;
            }
            for wanted in &must_contain {
                let holds = result.stdout.contains(wanted.as_str());
                evidence.line(format!("contains {wanted:?}: {}", yes(holds)));
                passed &= holds;
            }
            (passed, evidence.with_output(&result))
        }
        Verification::Review { rubric } => return Ok(CriterionOutcome::NeedsReview { rubric }),
        Verification::Human { question } => {
            return Ok(CriterionOutcome::NeedsHuman { question });
        }
    };
    Ok(result_of(criterion, run_by, passed, evidence))
}

/// Runs every criterion of `contract` in its order, each with `CRITERION_TIMEOUT`.
///
/// # Errors
///
/// The first criterion that could not be run at all ends the run with its error.
pub fn run_criteria(
    contract: &TaskContract,
    executor: &dyn Executor,
    run_by: RunBy,
) -> Result<Vec<CriterionOutcome>, CriterionError> {
    contract
        .exit_criteria
        .iter()
        .map(|criterion| run_criterion(criterion, executor, run_by, CRITERION_TIMEOUT))
        .collect()
}

fn result_of(
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

/// The evidence's lines, in the order the plan fixes: the command, how it ended, each
/// expectation, and then the tail of each stream.
struct Evidence {
    lines: Vec<String>,
}

impl Evidence {
    fn ran(command: &str, result: &ExecResult) -> Evidence {
        let ended = if result.timed_out {
            "timed out".to_owned()
        } else {
            format!("exit {}", result.exit_code)
        };
        Evidence {
            lines: vec![format!("$ {command}"), ended],
        }
    }

    fn line(&mut self, line: String) {
        self.lines.push(line);
    }

    /// The lines with each stream's tail after them; a stream that printed nothing is left out.
    fn with_output(mut self, result: &ExecResult) -> String {
        for (name, text) in [("stdout", &result.stdout), ("stderr", &result.stderr)] {
            if !text.is_empty() {
                self.lines
                    .push(format!("{name} (last {TAIL_BYTES} bytes):\n{}", tail(text)));
            }
        }
        self.lines.join("\n")
    }
}

/// The last `TAIL_BYTES` of `text`, or a little fewer so as to start on a character.
fn tail(text: &str) -> &str {
    let mut start = text.len().saturating_sub(TAIL_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

fn yes(holds: bool) -> &'static str {
    if holds { "yes" } else { "no" }
}

/// `text` as one shell word: single-quoted, with a `'` inside written `'\''`.
fn single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Why `normalise` refused `path`, in words.
fn why_outside(path: &str) -> &'static str {
    let unified = path.replace('\\', "/");
    if unified.starts_with('/') || unified.chars().nth(1) == Some(':') {
        "is absolute"
    } else if unified.split('/').any(|segment| segment == "..") {
        "climbs out of the workspace"
    } else {
        "names the workspace itself rather than a file in it"
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

    use super::{CriterionOutcome, run_criterion};
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
}
