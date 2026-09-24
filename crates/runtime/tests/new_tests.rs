//! The new-tests check against a real repository: a diff that adds a test, the test run on the
//! base branch in a worktree of its own, and that worktree gone again however the run ended.
//!
//! Every test here needs the `git` program, so every one is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use farik_core::contract::fixtures::a_contract_wire;
use farik_core::contract::{TaskId, validate_contract};
use farik_core::governor::done::RunBy;
use farik_runtime::criteria::{
    CriterionError, CriterionOutcome, NewTestsInput, check_new_tests, run_criteria,
};
use farik_runtime::{HostSandbox, HostSandboxFactory, Sandbox, SandboxError, SandboxFactory};
use farik_store::Git;
use farik_store::git::fixtures::TempRepo;
use serde_json::json;

const COMMAND: &str = "sh run_tests.sh";

/// The branch of FRK-1, a Software Developer's feature (5.14).
const BRANCH: &str = "feature/FRK-1";

/// `main` with the code and a runner for `tests/*.sh`, and the task's branch checked out from it.
fn fixture(name: &str) -> TempRepo {
    let repository = TempRepo::new(name);
    repository.write("src/x.sh", "echo 1\n");
    repository.write(
        "run_tests.sh",
        "for t in tests/*.sh; do [ -e \"$t\" ] || continue; sh \"$t\" || exit 1; done\n",
    );
    repository.commit("the code and its runner");
    repository.git(&["checkout", "-b", BRANCH]);
    repository
}

fn task() -> TaskId {
    TaskId::try_from("FRK-1").expect("an id")
}

fn input<'a>(
    git: &'a Git,
    sandboxes: &'a dyn SandboxFactory,
    task_id: &'a TaskId,
) -> NewTestsInput<'a> {
    NewTestsInput {
        git,
        base: "main",
        head: BRANCH,
        sandboxes,
        project_id: "p",
        task_id,
    }
}

fn base_worktree(repository: &TempRepo) -> PathBuf {
    repository.path.join(".farik/local/worktrees/FRK-1-base")
}

fn assert_base_gone(repository: &TempRepo) {
    assert!(!base_worktree(repository).exists(), "the directory is gone");
    let listed = repository.git_output(&["worktree", "list"]);
    assert!(!listed.contains("FRK-1-base"), "{listed}");
}

/// Host sandboxes, counting the base ones asked for.
#[derive(Default)]
struct Counting {
    bases: AtomicUsize,
}

impl SandboxFactory for Counting {
    fn create(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        HostSandboxFactory.create(project_id, task_id, worktree, network)
    }

    fn create_base(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        self.bases.fetch_add(1, Ordering::SeqCst);
        HostSandboxFactory.create_base(project_id, task_id, worktree)
    }

    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError> {
        HostSandboxFactory.remove(project_id, task_id)
    }
}

/// A factory on a machine with no Docker.
struct NoDocker;

impl SandboxFactory for NoDocker {
    fn create(
        &self,
        _project_id: &str,
        _task_id: &TaskId,
        _worktree: &Path,
        _network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        Err(SandboxError::DockerUnavailable)
    }

    fn create_base(
        &self,
        _project_id: &str,
        _task_id: &TaskId,
        _worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        Err(SandboxError::DockerUnavailable)
    }

    fn remove(&self, _project_id: &str, _task_id: &TaskId) -> Result<(), SandboxError> {
        Err(SandboxError::DockerUnavailable)
    }
}

/// The head fixes the code and adds a test that only the fix passes.
fn add_a_failing_test(repository: &TempRepo) {
    repository.write("src/x.sh", "echo 2\n");
    repository.write("tests/new.sh", "[ \"$(sh src/x.sh)\" = 2 ]\n");
    repository.commit("fix x and test it");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn finds_a_new_test_that_fails_on_the_base_branch() {
    let repository = fixture("new-test-fails-on-base");
    add_a_failing_test(&repository);
    let git = repository.adapter();
    let task_id = task();
    let check = check_new_tests(COMMAND, &input(&git, &HostSandboxFactory, &task_id))
        .expect("the check runs");
    assert!(check.adds_tests);
    assert!(check.fails_on_base);
    assert_eq!(check.test_files, ["tests/new.sh"]);
    assert_base_gone(&repository);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn finds_no_new_test_when_only_code_changed() {
    let repository = fixture("no-new-test");
    repository.write("src/x.sh", "echo 2\n");
    repository.commit("change x only");
    let git = repository.adapter();
    let task_id = task();
    let counting = Counting::default();
    let check =
        check_new_tests(COMMAND, &input(&git, &counting, &task_id)).expect("the check runs");
    assert!(!check.adds_tests);
    assert!(check.test_files.is_empty());
    assert_eq!(counting.bases.load(Ordering::SeqCst), 0, "nothing was run");
    assert_base_gone(&repository);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn says_a_new_test_that_passes_on_base_does_not_count() {
    let repository = fixture("new-test-passes-on-base");
    repository.write("src/x.sh", "echo 2\n");
    repository.write("tests/new.sh", "exit 0\n");
    repository.commit("change x and add a test that tests nothing");
    let git = repository.adapter();
    let task_id = task();
    let sandboxes = HostSandboxFactory;
    let input = input(&git, &sandboxes, &task_id);
    let check = check_new_tests(COMMAND, &input).expect("the check runs");
    assert!(check.adds_tests);
    assert!(!check.fails_on_base);
    assert_base_gone(&repository);

    let mut wire = a_contract_wire();
    wire["exit_criteria"] = json!([{
        "id": "C1",
        "text": "The tests pass, and a new one shows the change.",
        "verification": { "method": "test", "command": COMMAND, "new_tests_required": true },
    }]);
    let contract = validate_contract(&wire).expect("a valid contract");
    let executor = HostSandbox::new(repository.path.clone());
    let outcomes = run_criteria(&contract, &executor, RunBy::Reviewer, Some(&input))
        .expect("the criteria run");
    let [CriterionOutcome::Result(result)] = outcomes.as_slice() else {
        panic!("one result: {outcomes:?}");
    };
    assert!(!result.passed, "{}", result.evidence);
    let lines: Vec<&str> = result.evidence.lines().collect();
    for line in [
        "exit 0",
        "adds tests: yes",
        "fails on base: no",
        "test files: tests/new.sh",
    ] {
        assert!(lines.contains(&line), "{line} in {lines:?}");
    }
    assert_base_gone(&repository);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn removes_the_base_worktree_whatever_happens() {
    let repository = fixture("base-removed-on-error");
    add_a_failing_test(&repository);
    let git = repository.adapter();
    let task_id = task();
    assert_eq!(
        check_new_tests(COMMAND, &input(&git, &NoDocker, &task_id)),
        Err(CriterionError::Sandbox(SandboxError::DockerUnavailable))
    );
    assert_base_gone(&repository);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn replaces_a_base_worktree_left_by_a_crash() {
    for registered in [true, false] {
        let repository = fixture(if registered {
            "base-left-registered"
        } else {
            "base-left-plain"
        });
        add_a_failing_test(&repository);
        let left = base_worktree(&repository);
        if registered {
            let path = left.to_str().expect("a path that is text");
            repository.git(&["worktree", "add", "--detach", path, "main"]);
        } else {
            std::fs::create_dir_all(&left).expect("a directory git does not know");
        }
        std::fs::write(left.join("stale.txt"), "from the crash\n").expect("a stale file");
        let git = repository.adapter();
        let task_id = task();
        let check = check_new_tests(COMMAND, &input(&git, &HostSandboxFactory, &task_id))
            .expect("the check runs");
        assert!(check.fails_on_base, "registered: {registered}");
        assert_base_gone(&repository);
    }
}
