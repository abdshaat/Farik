//! No-sandbox mode: a task's commands run on the host, inside its worktree, with nothing isolating
//! them but the governor's own checks (`docs/SPEC.md` 8.3).

use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use farik_core::contract::TaskId;

use crate::exec::{ExecError, ExecResult, Executor, supervise, workspace_relative};
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};

/// The variables a host command keeps from Farik's own environment, when they are set. Nothing
/// else passes, so no credential in the user's shell reaches a command (`docs/SPEC.md` 8.6).
const PASSED_THROUGH: [&str; 4] = ["PATH", "HOME", "LANG", "TMPDIR"];

/// A task's commands, run on the host in the worktree at `root`.
pub struct HostSandbox {
    root: PathBuf,
}

impl HostSandbox {
    /// A sandbox rooted at `root`, the task's worktree.
    #[must_use]
    pub fn new(root: PathBuf) -> HostSandbox {
        HostSandbox { root }
    }
}

impl Executor for HostSandbox {
    fn run(
        &self,
        command: &str,
        cwd: &str,
        timeout: Duration,
        env: &BTreeMap<String, String>,
    ) -> Result<ExecResult, ExecError> {
        let directory = match workspace_relative(cwd)? {
            None => self.root.clone(),
            Some(relative) => self.root.join(relative),
        };
        let mut shell = Command::new("sh");
        shell
            .arg("-c")
            .arg(command)
            .current_dir(directory)
            .env_clear()
            .process_group(0);
        for name in PASSED_THROUGH {
            if let Some(value) = std::env::var_os(name) {
                shell.env(name, value);
            }
        }
        shell.envs(env);
        // The group is killed at the deadline, and again once `sh` has gone, so that a background
        // child still holding a pipe cannot keep the readers waiting.
        let finished = supervise(&mut shell, timeout, |child| kill_group(child), kill_group)?;
        Ok(ExecResult {
            exit_code: finished.exit_code,
            stdout: finished.stdout,
            stderr: finished.stderr,
            timed_out: finished.killed,
            truncated: finished.truncated,
        })
    }
}

impl Sandbox for HostSandbox {
    fn discard(self: Box<Self>) -> Result<(), SandboxError> {
        Ok(())
    }
}

fn kill_group(child: &Child) {
    crate::exec::kill_group(child.id());
}

/// Makes host sandboxes: no-sandbox mode.
pub struct HostSandboxFactory;

impl SandboxFactory for HostSandboxFactory {
    fn create(
        &self,
        _project_id: &str,
        _task_id: &TaskId,
        worktree: &Path,
        _network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        Ok(Box::new(HostSandbox::new(worktree.to_path_buf())))
    }

    fn create_base(
        &self,
        _project_id: &str,
        _task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        Ok(Box::new(HostSandbox::new(worktree.to_path_buf())))
    }

    /// A host sandbox holds nothing to end.
    fn remove(&self, _project_id: &str, _task_id: &TaskId) -> Result<(), SandboxError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use farik_core::contract::TaskId;

    use super::{HostSandbox, HostSandboxFactory};
    use crate::exec::{ExecError, Executor, OUTPUT_LIMIT_BYTES};
    use crate::sandbox::{SandboxError, SandboxFactory};

    const SECOND: Duration = Duration::from_secs(1);

    fn fresh_root(test: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("farik-runtime-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temporary directory can be made");
        root
    }

    fn canonical(path: &Path) -> String {
        path.canonicalize()
            .expect("the directory exists")
            .display()
            .to_string()
    }

    fn no_env() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[test]
    fn runs_a_command_in_the_workspace_and_reports_its_output() {
        let sandbox = HostSandbox::new(fresh_root("output"));
        let result = sandbox
            .run(
                "printf hi; printf err >&2; exit 3",
                "",
                5 * SECOND,
                &no_env(),
            )
            .expect("the command runs");
        assert_eq!(result.exit_code, 3);
        assert_eq!(result.stdout, "hi");
        assert_eq!(result.stderr, "err");
        assert!(!result.timed_out);
        assert!(!result.truncated);
    }

    #[test]
    fn runs_in_a_subdirectory_named_relative_to_the_workspace() {
        let root = fresh_root("subdirectory");
        std::fs::create_dir(root.join("sub")).expect("sub can be made");
        let sandbox = HostSandbox::new(root.clone());
        let result = sandbox
            .run("pwd", "sub", 5 * SECOND, &no_env())
            .expect("the command runs");
        assert_eq!(result.stdout.trim_end(), canonical(&root.join("sub")));
    }

    #[test]
    fn treats_an_empty_or_dot_directory_as_the_workspace() {
        let root = fresh_root("dot");
        let sandbox = HostSandbox::new(root.clone());
        for cwd in ["", ".", "./"] {
            let result = sandbox
                .run("pwd", cwd, 5 * SECOND, &no_env())
                .expect("the command runs");
            assert_eq!(result.stdout.trim_end(), canonical(&root), "cwd {cwd:?}");
        }
    }

    #[test]
    fn refuses_a_directory_outside_the_workspace() {
        let root = fresh_root("outside");
        let sandbox = HostSandbox::new(root.clone());
        let marker = format!("farik-outside-marker-{}", std::process::id());
        for cwd in ["/tmp", "../x"] {
            let refused = sandbox.run(&format!("touch {marker}"), cwd, 5 * SECOND, &no_env());
            assert_eq!(
                refused,
                Err(ExecError::OutsideWorkspace {
                    cwd: cwd.to_owned()
                })
            );
        }
        assert!(!root.join(&marker).exists());
        assert!(!Path::new("/tmp").join(&marker).exists());
    }

    #[test]
    fn kills_a_command_and_its_children_at_the_deadline() {
        let sandbox = HostSandbox::new(fresh_root("deadline"));
        let started = Instant::now();
        let result = sandbox
            .run(
                "sleep 31 & sleep 32; wait",
                "",
                Duration::from_millis(200),
                &no_env(),
            )
            .expect("the command runs");
        assert!(started.elapsed() < 5 * SECOND);
        assert!(result.timed_out);
        assert_eq!(result.exit_code, 137);
        let deadline = Instant::now() + SECOND;
        loop {
            let found = std::process::Command::new("pgrep")
                .args(["-f", "sleep 3[12]"])
                .output()
                .expect("pgrep runs");
            if !found.status.success() {
                break;
            }
            assert!(Instant::now() < deadline, "a sleep outlived the deadline");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn returns_by_the_deadline_when_an_escaped_child_holds_the_pipe() {
        let sandbox = HostSandbox::new(fresh_root("escaped"));
        let timeout = Duration::from_millis(1500);
        let started = Instant::now();
        let result = sandbox
            .run("setsid sleep 4 & echo started", "", timeout, &no_env())
            .expect("the command runs");
        assert!(
            started.elapsed() < timeout + SECOND,
            "returned after {:?}",
            started.elapsed()
        );
        assert_eq!(result.stdout, "started\n");
    }

    #[test]
    fn ends_a_background_child_once_the_shell_has_exited() {
        let sandbox = HostSandbox::new(fresh_root("background"));
        let started = Instant::now();
        let result = sandbox
            .run("sleep 33 & echo started", "", 60 * SECOND, &no_env())
            .expect("the command runs");
        assert!(started.elapsed() < 5 * SECOND);
        assert!(!result.timed_out);
        assert_eq!(result.stdout, "started\n");
        let deadline = Instant::now() + SECOND;
        loop {
            let found = std::process::Command::new("pgrep")
                .args(["-f", "sleep 33"])
                .output()
                .expect("pgrep runs");
            if !found.status.success() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the background sleep outlived sh"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn does_not_cut_output_of_exactly_a_mebibyte() {
        let sandbox = HostSandbox::new(fresh_root("exact"));
        let result = sandbox
            .run(
                &format!("head -c {OUTPUT_LIMIT_BYTES} /dev/zero | tr '\\0' a"),
                "",
                30 * SECOND,
                &no_env(),
            )
            .expect("the command runs");
        assert_eq!(result.stdout.len(), OUTPUT_LIMIT_BYTES);
        assert!(!result.truncated);
    }

    #[test]
    fn says_it_cut_when_only_standard_error_overflowed() {
        let sandbox = HostSandbox::new(fresh_root("stderr"));
        let result = sandbox
            .run(
                "head -c 3000000 /dev/zero | tr '\\0' a >&2",
                "",
                30 * SECOND,
                &no_env(),
            )
            .expect("the command runs");
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr.len(), OUTPUT_LIMIT_BYTES);
        assert!(result.truncated);
    }

    #[test]
    fn keeps_the_first_mebibyte_of_output_and_says_it_cut() {
        let sandbox = HostSandbox::new(fresh_root("mebibyte"));
        let result = sandbox
            .run(
                "head -c 3000000 /dev/zero | tr '\\0' a",
                "",
                30 * SECOND,
                &no_env(),
            )
            .expect("the command runs");
        assert_eq!(result.stdout.len(), OUTPUT_LIMIT_BYTES);
        assert!(result.truncated);
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn passes_only_the_environment_it_was_given() {
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        let sandbox = HostSandbox::new(fresh_root("environment"));
        let env = BTreeMap::from([("GIVEN".to_owned(), "yes".to_owned())]);
        let result = sandbox
            .run("env", "", 5 * SECOND, &env)
            .expect("the command runs");
        let lines: Vec<&str> = result.stdout.lines().collect();
        assert!(!result.stdout.contains("CARGO_MANIFEST_DIR"));
        assert!(lines.contains(&"GIVEN=yes"));
        assert!(lines.iter().any(|line| line.starts_with("PATH=")));
    }

    #[test]
    fn creates_a_host_sandbox_rooted_at_the_worktree() {
        let root = fresh_root("factory");
        let task_id = TaskId::try_from("FRK-1").expect("an id");
        let sandbox = HostSandboxFactory
            .create("p", &task_id, &root, false)
            .expect("a host sandbox is always available");
        let result = sandbox
            .run("pwd", "", 5 * SECOND, &no_env())
            .expect("the command runs");
        assert_eq!(result.stdout.trim_end(), canonical(&root));
    }

    #[test]
    fn creates_a_base_sandbox_rooted_at_the_worktree_it_is_given() {
        let root = fresh_root("base");
        let task_id = TaskId::try_from("FRK-1").expect("an id");
        let sandbox = HostSandboxFactory
            .create_base("p", &task_id, &root)
            .expect("a host sandbox is always available");
        let result = sandbox
            .run("pwd", "", 5 * SECOND, &no_env())
            .expect("the command runs");
        assert_eq!(result.stdout.trim_end(), canonical(&root));
    }

    #[test]
    fn discards_a_host_sandbox_without_touching_the_workspace() {
        let root = fresh_root("discard");
        let task_id = TaskId::try_from("FRK-1").expect("an id");
        let sandbox = HostSandboxFactory
            .create("p", &task_id, &root, false)
            .expect("a host sandbox is always available");
        assert_eq!(sandbox.discard(), Ok(()));
        assert!(root.exists());
    }

    #[test]
    fn displays_each_error_in_words() {
        let image = SandboxError::ImageMissing {
            image: "farik/sandbox:0.0.0".to_owned(),
        }
        .to_string();
        assert!(image.contains("farik/sandbox:0.0.0"), "{image}");
        let docker = SandboxError::DockerUnavailable.to_string();
        assert!(docker.contains("docker"), "{docker}");
        let outside = ExecError::OutsideWorkspace {
            cwd: "../x".to_owned(),
        }
        .to_string();
        assert!(outside.contains("../x"), "{outside}");
    }
}
