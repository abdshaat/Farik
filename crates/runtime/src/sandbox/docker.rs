//! A task's commands in a Docker container of their own, with the worktree mounted at
//! `/workspace` (`docs/SPEC.md` 8.3). Docker is the `docker` program, driven as git is.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use farik_core::contract::TaskId;

use crate::exec::{ExecError, ExecResult, Executor, Finished, supervise, workspace_relative};
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};

/// How long after the deadline the `docker exec` client is killed, in case the container stops
/// answering; the command itself is stopped inside the container by `timeout`.
const CLIENT_GRACE: Duration = Duration::from_secs(5);

/// A task's container, by name.
pub struct DockerSandbox {
    name: String,
}

impl DockerSandbox {
    /// Starts the container for `task_id` of `project_id` from `image`, with `worktree` at
    /// `/workspace` and the network off unless `network`. A container of the same name already
    /// there is removed first: the worktree is the state, the container is disposable.
    ///
    /// # Errors
    ///
    /// `DockerUnavailable` when `docker version` fails, `ImageMissing` when the image is not on
    /// this machine (nothing is pulled), and `ContainerFailed` when `docker run` refuses.
    pub fn create(
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
        image: &str,
    ) -> Result<DockerSandbox, SandboxError> {
        if !docker(&["version"]).is_ok_and(|output| output.status.success()) {
            return Err(SandboxError::DockerUnavailable);
        }
        if !docker(&["image", "inspect", image])?.status.success() {
            return Err(SandboxError::ImageMissing {
                image: image.to_owned(),
            });
        }
        let name = container_name(project_id, task_id);
        // Absent is the usual answer, and not an error.
        docker(&["rm", "-f", &name])?;
        let user = format!("{}:{}", id("-u")?, id("-g")?);
        let mount = format!("type=bind,src={},dst=/workspace", worktree.display());
        let project_label = format!("farik.project={project_id}");
        let task_label = format!("farik.task={}", task_id.as_str());
        let output = docker(&[
            "run",
            "-d",
            "--init",
            "--name",
            &name,
            "--network",
            if network { "bridge" } else { "none" },
            "--mount",
            &mount,
            "-w",
            "/workspace",
            "--user",
            &user,
            "--label",
            &project_label,
            "--label",
            &task_label,
            image,
            "sleep",
            "infinity",
        ])?;
        if !output.status.success() {
            return Err(SandboxError::ContainerFailed {
                detail: stderr_of(&output),
            });
        }
        Ok(DockerSandbox { name })
    }

    /// The container's name, `farik-<project>-<task_id>` in Docker's alphabet.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Executor for DockerSandbox {
    fn run(
        &self,
        command: &str,
        cwd: &str,
        timeout: Duration,
        env: &BTreeMap<String, String>,
    ) -> Result<ExecResult, ExecError> {
        let directory = match workspace_relative(cwd)? {
            None => "/workspace".to_owned(),
            Some(relative) => format!("/workspace/{relative}"),
        };
        let mut client = Command::new("docker");
        client.args(["exec", "-w", &directory]);
        for (name, value) in env {
            client.arg("-e").arg(format!("{name}={value}"));
        }
        client
            .arg(&self.name)
            .args(["timeout", "-k", "2", &whole_seconds(timeout).to_string()])
            .args(["sh", "-c", command]);
        let finished = supervise(
            &mut client,
            timeout.saturating_add(CLIENT_GRACE),
            |child| {
                let _ = child.kill();
            },
            |_| {},
        )?;
        if is_container_gone(&finished) {
            return Err(ExecError::ContainerGone);
        }
        // Busybox and GNU `timeout` exit with different codes, so the host clock decides.
        Ok(ExecResult {
            exit_code: finished.exit_code,
            timed_out: finished.elapsed >= timeout,
            stdout: finished.stdout,
            stderr: finished.stderr,
            truncated: finished.truncated,
        })
    }
}

impl Sandbox for DockerSandbox {
    fn discard(self: Box<Self>) -> Result<(), SandboxError> {
        let output = docker(&["rm", "-f", &self.name])?;
        if !output.status.success() {
            return Err(SandboxError::ContainerFailed {
                detail: stderr_of(&output),
            });
        }
        Ok(())
    }
}

/// Makes Docker sandboxes from one image.
pub struct DockerSandboxFactory {
    /// The image every container runs, `SANDBOX_IMAGE` outside tests.
    pub image: String,
}

impl SandboxFactory for DockerSandboxFactory {
    fn create(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        let sandbox = DockerSandbox::create(project_id, task_id, worktree, network, &self.image)?;
        Ok(Box::new(sandbox))
    }
}

/// `farik-<project>-<task_id>`, lowercased with everything outside `[a-z0-9_.-]` made `-`.
fn container_name(project_id: &str, task_id: &TaskId) -> String {
    format!("farik-{project_id}-{}", task_id.as_str())
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'a'..='z' | '0'..='9' | '_' | '.' | '-' => character,
            _ => '-',
        })
        .collect()
}

/// The timeout in whole seconds for `timeout(1)`, rounded up, and at least one.
fn whole_seconds(timeout: Duration) -> u64 {
    timeout
        .as_secs()
        .saturating_add(u64::from(timeout.subsec_nanos() > 0))
        .max(1)
}

/// Whether docker itself, rather than the command, said the container is gone. Only docker's own
/// error lines count, so a command that prints "is not running" is not mistaken for it.
fn is_container_gone(finished: &Finished) -> bool {
    finished.stderr.lines().any(|line| {
        line.starts_with("Error response from daemon")
            && (line.contains("No such container") || line.contains("is not running"))
    })
}

fn docker(args: &[&str]) -> Result<Output, SandboxError> {
    Command::new("docker")
        .args(args)
        .output()
        .map_err(|_| SandboxError::DockerUnavailable)
}

/// `id -u` or `id -g`, the user's own, so that what a command writes in the worktree is theirs.
fn id(flag: &str) -> Result<String, SandboxError> {
    let output =
        Command::new("id")
            .arg(flag)
            .output()
            .map_err(|error| SandboxError::ContainerFailed {
                detail: format!("id {flag} could not be run: {error}"),
            })?;
    if !output.status.success() {
        return Err(SandboxError::ContainerFailed {
            detail: format!("id {flag} refused: {}", stderr_of(&output)),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use farik_core::contract::TaskId;

    use std::collections::BTreeMap;

    use super::{DockerSandbox, container_name, is_container_gone, whole_seconds};
    use crate::exec::{Executor, Finished};

    #[test]
    fn takes_the_longest_timeout_without_overflowing() {
        assert_eq!(whole_seconds(Duration::MAX), u64::MAX);
        let sandbox = DockerSandbox {
            name: format!("farik-no-such-container-{}", std::process::id()),
        };
        // Whatever docker answers here (it may not even be installed), `run` must not panic.
        let _ = sandbox.run("true", "", Duration::MAX, &BTreeMap::new());
    }

    fn with_stderr(stderr: &str) -> Finished {
        Finished {
            exit_code: 1,
            stdout: String::new(),
            stderr: stderr.to_owned(),
            truncated: false,
            killed: false,
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn tells_docker_saying_the_container_is_gone_from_a_command_saying_it() {
        let missing = "Error response from daemon: No such container: farik-p-frk-1";
        let stopped = "Error response from daemon: container 0123abcd is not running";
        assert!(is_container_gone(&with_stderr(missing)));
        assert!(is_container_gone(&with_stderr(&format!(
            "noise\n{stopped}\n"
        ))));
        let own = "No such container: frk-1\nthe server is not running\n";
        assert!(!is_container_gone(&with_stderr(own)));
        assert!(!is_container_gone(&with_stderr("")));
    }

    #[test]
    fn names_a_container_by_docker_rule_and_rounds_the_timeout_up() {
        let task = TaskId::try_from("FRK-12").expect("an id");
        assert_eq!(
            container_name("My Project/1", &task),
            "farik-my-project-1-frk-12"
        );
        assert_eq!(container_name("a_b.c-d", &task), "farik-a_b.c-d-frk-12");
        assert_eq!(whole_seconds(Duration::from_millis(200)), 1);
        assert_eq!(whole_seconds(Duration::ZERO), 1);
        assert_eq!(whole_seconds(Duration::from_millis(2001)), 3);
        assert_eq!(whole_seconds(Duration::from_secs(4)), 4);
    }
}
