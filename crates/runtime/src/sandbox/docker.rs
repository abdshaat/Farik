//! A task's commands in a Docker container of their own, with the worktree mounted at
//! `/workspace` (`docs/SPEC.md` 8.3). Docker is the `docker` program, driven as git is. The
//! container made for Catervas's checks also sees the repository's git store, read-only.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use catervas_core::contract::TaskId;
use sha2::{Digest as _, Sha256};

use crate::exec::{ExecError, ExecResult, Executor, Finished, supervise, workspace_relative};
use crate::preview::docker::container_name as preview_container;
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
    /// this machine (nothing is pulled), and `ContainerFailed` when `docker run` refuses or
    /// `worktree` is relative or holds a comma, which `--mount` cannot name.
    pub fn create(
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
        image: &str,
    ) -> Result<DockerSandbox, SandboxError> {
        Self::start(
            container_name(project_id, task_id),
            project_id,
            task_id,
            (worktree, &[], None),
            network,
            image,
            &[],
        )
    }

    /// `create`, under the container name `name`, with `mounts` as the worktree, the other
    /// repositories it can read, and the git store it sees read-only. With `packages`, the
    /// container runs the image built from `image` with them installed (ADR 0081).
    fn start(
        name: String,
        project_id: &str,
        task_id: &TaskId,
        (worktree, reads, git_store): (&Path, &[(String, PathBuf)], Option<&Path>),
        network: bool,
        image: &str,
        packages: &[String],
    ) -> Result<DockerSandbox, SandboxError> {
        let labels = [
            ("catervas.project".to_string(), project_id.to_string()),
            ("catervas.task".to_string(), task_id.as_str().to_string()),
        ];
        let user = format!("{}:{}", id("-u")?, id("-g")?);
        let run_image = package_image(image, packages);
        let arguments = run_args(
            &name,
            worktree,
            reads,
            git_store,
            network,
            &user,
            &labels,
            run_image.as_deref().unwrap_or(image),
        )?;
        if !docker(&["version"]).is_ok_and(|output| output.status.success()) {
            return Err(SandboxError::DockerUnavailable);
        }
        if !docker(&["image", "inspect", image])?.status.success() {
            return Err(SandboxError::ImageMissing {
                image: image.to_owned(),
            });
        }
        ensure_package_image(image, packages)?;
        // Absent is the usual answer, and not an error.
        docker(&["rm", "-f", &name])?;
        let output = docker(&arguments.iter().map(String::as_str).collect::<Vec<_>>())?;
        if !output.status.success() {
            return Err(SandboxError::ContainerFailed {
                detail: stderr_of(&output),
            });
        }
        Ok(DockerSandbox { name })
    }

    /// The container's name, `catervas-<project>-<task_id>` in Docker's alphabet.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The arguments of the `docker run` that starts a sandbox: `worktree` at `/workspace`, then each
/// of `reads` read-only at `/repos/<name>` (8.3), then `git_store` read-only at its own path, so
/// that a linked worktree's `.git` file resolves (ADR 0069), with the container's `name`, `user`
/// and `labels`.
///
/// # Errors
///
/// `ContainerFailed` when a folder to mount is relative or holds a comma, which `--mount` splits
/// on.
#[allow(
    clippy::too_many_arguments,
    reason = "every part of the one `docker run` line"
)]
pub(crate) fn run_args(
    name: &str,
    worktree: &Path,
    reads: &[(String, PathBuf)],
    git_store: Option<&Path>,
    network: bool,
    user: &str,
    labels: &[(String, String)],
    image: &str,
) -> Result<Vec<String>, SandboxError> {
    // `--mount` splits on commas and wants an absolute source.
    let refuse = |what: &str, path: &Path| SandboxError::ContainerFailed {
        detail: format!(
            "{what} {} cannot be mounted: it must be absolute and hold no comma",
            path.display()
        ),
    };
    let mountable = |path: &Path| path.is_absolute() && !path.to_string_lossy().contains(',');
    if !mountable(worktree) {
        return Err(refuse("the worktree", worktree));
    }
    let mut arguments: Vec<String> = [
        "run",
        "-d",
        "--init",
        "--name",
        name,
        "--network",
        if network { "bridge" } else { "none" },
        "--mount",
        &format!("type=bind,src={},dst=/workspace", worktree.display()),
    ]
    .map(ToString::to_string)
    .to_vec();
    for (member, path) in reads {
        if !mountable(path) {
            return Err(refuse("the repository", path));
        }
        arguments.push("--mount".to_string());
        arguments.push(format!(
            "type=bind,src={},dst=/repos/{member},readonly",
            path.display()
        ));
    }
    if let Some(store) = git_store {
        if !mountable(store) {
            return Err(refuse("the git store", store));
        }
        arguments.push("--mount".to_string());
        arguments.push(format!(
            "type=bind,src={0},dst={0},readonly",
            store.display()
        ));
    }
    arguments.extend(["-w", "/workspace", "--user", user].map(ToString::to_string));
    for (key, value) in labels {
        arguments.push("--label".to_string());
        arguments.push(format!("{key}={value}"));
    }
    arguments.extend([image, "sleep", "infinity"].map(ToString::to_string));
    Ok(arguments)
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
        removed(&docker(&["rm", "-f", &self.name])?)
    }
}

/// What `docker rm -f` answering `output` means for `discard`: a container already gone is
/// discarded, since that is all `discard` asks. Older dockers refuse a missing one; newer ones do
/// not. So is one Docker is already removing, as it does a `--rm` container whose process ended.
pub(crate) fn removed(output: &Output) -> Result<(), SandboxError> {
    let said = stderr_of(output);
    let gone = said.contains("No such container") || said.contains("is already in progress");
    if !output.status.success() && !gone {
        return Err(SandboxError::ContainerFailed {
            detail: stderr_of(output),
        });
    }
    Ok(())
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
        reads: &[(String, PathBuf)],
        packages: &[String],
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        let sandbox = DockerSandbox::start(
            container_name(project_id, task_id),
            project_id,
            task_id,
            (worktree, reads, None),
            network,
            &self.image,
            packages,
        )?;
        Ok(Box::new(sandbox))
    }

    fn create_checking(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
        reads: &[(String, PathBuf)],
        packages: &[String],
        git_store: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        let sandbox = DockerSandbox::start(
            container_name(project_id, task_id),
            project_id,
            task_id,
            (worktree, reads, Some(git_store)),
            network,
            &self.image,
            packages,
        )?;
        Ok(Box::new(sandbox))
    }

    fn create_base(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        let sandbox = DockerSandbox::start(
            base_container_name(project_id, task_id),
            project_id,
            task_id,
            (worktree, &[], None),
            false,
            &self.image,
            &[],
        )?;
        Ok(Box::new(sandbox))
    }

    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError> {
        // The task's preview, its browser, and its prepare go with it, so that a session killed
        // before it stopped its preview leaves none behind (step 12's F5).
        let preview = preview_container("preview", project_id, task_id);
        for name in [
            container_name(project_id, task_id),
            base_container_name(project_id, task_id),
            crate::preview::browser_container(&preview),
            preview,
            preview_container("prepare", project_id, task_id),
        ] {
            removed(&docker(&["rm", "-f", &name])?)?;
        }
        Ok(())
    }
}

/// The tag of the image with `packages` installed over `base`: `<base>-p<12 hex>` of the sorted
/// list, so that the same packages in any order are the same image. None without packages (ADR 0081).
pub(crate) fn package_image(base: &str, packages: &[String]) -> Option<String> {
    if packages.is_empty() {
        return None;
    }
    let mut sorted = packages.to_vec();
    sorted.sort();
    let digest = Sha256::digest(sorted.join("\n").as_bytes());
    let hex = digest[..6].iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    Some(format!("{base}-p{hex}"))
}

/// The Dockerfile that installs `packages` over `base`, as the root user: containers run as the
/// owner's own user anyway (`--user`).
pub(crate) fn package_dockerfile(base: &str, packages: &[String]) -> String {
    let mut sorted = packages.to_vec();
    sorted.sort();
    format!(
        "FROM {base}\nUSER root\nRUN apt-get update && apt-get install -y --no-install-recommends \
         {} && rm -rf /var/lib/apt/lists/* && find /tmp -mindepth 1 -delete\n",
        sorted.join(" ")
    )
}

/// Builds the image `package_image` names, unless this machine has it, and answers its tag; the
/// shipped `base` when there are no packages. The build needs the network.
///
/// # Errors
///
/// `DockerUnavailable`, or `Unavailable` with the last lines of the build when it fails.
pub(crate) fn ensure_package_image(
    base: &str,
    packages: &[String],
) -> Result<String, SandboxError> {
    let Some(tag) = package_image(base, packages) else {
        return Ok(base.to_owned());
    };
    if docker(&["image", "inspect", &tag])?.status.success() {
        return Ok(tag);
    }
    let unavailable = |said: &str| {
        SandboxError::Unavailable(format!(
            "the sandbox's system tools could not be installed: {said}"
        ))
    };
    let mut build = Command::new("docker")
        .args(["build", "-t", &tag, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| SandboxError::DockerUnavailable)?;
    let dockerfile = package_dockerfile(base, packages);
    if let Some(mut stdin) = build.stdin.take() {
        stdin
            .write_all(dockerfile.as_bytes())
            .map_err(|error| unavailable(&error.to_string()))?;
    }
    let output = build
        .wait_with_output()
        .map_err(|error| unavailable(&error.to_string()))?;
    if output.status.success() {
        return Ok(tag);
    }
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = said
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    Err(unavailable(
        &lines[lines.len().saturating_sub(8)..].join("\n"),
    ))
}

/// `catervas-<project>-<task_id>-base`: the task's own name with a suffix, so the base run never
/// replaces the container the verify session is holding.
fn base_container_name(project_id: &str, task_id: &TaskId) -> String {
    format!("{}-base", container_name(project_id, task_id))
}

/// `catervas-<project>-<task_id>`, lowercased with everything outside `[a-z0-9_.-]` made `-`.
fn container_name(project_id: &str, task_id: &TaskId) -> String {
    docker_name(&format!("catervas-{project_id}-{}", task_id.as_str()))
}

/// `raw` lowercased with everything outside `[a-z0-9_.-]` made `-`: Docker's alphabet for names.
pub(crate) fn docker_name(raw: &str) -> String {
    raw.to_lowercase()
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

pub(crate) fn docker(args: &[&str]) -> Result<Output, SandboxError> {
    Command::new("docker")
        .args(args)
        .output()
        .map_err(|_| SandboxError::DockerUnavailable)
}

/// `id -u` or `id -g`, the user's own, so that what a command writes in the worktree is theirs.
pub(crate) fn id(flag: &str) -> Result<String, SandboxError> {
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

pub(crate) fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use catervas_core::contract::TaskId;

    use std::collections::BTreeMap;
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::{ExitStatus, Output};

    use super::{
        DockerSandbox, SandboxError, base_container_name, container_name, is_container_gone,
        package_dockerfile, package_image, removed, run_args, whole_seconds,
    };
    use crate::exec::{Executor, Finished};
    use crate::sandbox::SANDBOX_IMAGE;

    #[test]
    fn names_an_image_for_the_packages() {
        let list = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();
        let tag = package_image(SANDBOX_IMAGE, &list(&["ffmpeg", "libnss3"]))
            .expect("a list names an image");
        let suffix = tag
            .strip_prefix(&format!("{SANDBOX_IMAGE}-p"))
            .expect("the tag extends the shipped image's");
        assert_eq!(suffix.len(), 12, "{tag}");
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()), "{tag}");
        assert_eq!(
            package_image(SANDBOX_IMAGE, &list(&["libnss3", "ffmpeg"])).as_ref(),
            Some(&tag),
            "the order does not matter"
        );
        assert_ne!(
            package_image(SANDBOX_IMAGE, &list(&["ffmpeg"])),
            Some(tag),
            "another list is another image"
        );
        assert_eq!(
            package_image(SANDBOX_IMAGE, &[]),
            None,
            "no packages, no image"
        );

        let dockerfile = package_dockerfile(SANDBOX_IMAGE, &list(&["libnss3", "ffmpeg"]));
        assert!(dockerfile.starts_with(&format!("FROM {SANDBOX_IMAGE}\nUSER root\n")));
        assert!(dockerfile.contains(
            "apt-get install -y --no-install-recommends ffmpeg libnss3 && \
             rm -rf /var/lib/apt/lists/* && find /tmp -mindepth 1 -delete"
        ));
    }

    #[test]
    fn mounts_each_other_member_read_only_after_the_worktree() {
        let labels = [("catervas.task".to_string(), "CTV-1".to_string())];
        let reads = [(
            "infra".to_string(),
            std::path::PathBuf::from("/home/me/infra"),
        )];
        let store = Path::new("/home/me/p/.git");
        let arguments = run_args(
            "c",
            Path::new("/home/me/wt"),
            &reads,
            Some(store),
            false,
            "1000:1000",
            &labels,
            "image",
        )
        .expect("the arguments");
        let own = "type=bind,src=/home/me/wt,dst=/workspace".to_string();
        let sibling = "type=bind,src=/home/me/infra,dst=/repos/infra,readonly".to_string();
        // The checks' container also sees the git store, read-only at its own path (ADR 0069).
        let git = "type=bind,src=/home/me/p/.git,dst=/home/me/p/.git,readonly".to_string();
        let mounts: Vec<&String> = arguments
            .windows(2)
            .filter(|pair| pair[0] == "--mount")
            .map(|pair| &pair[1])
            .collect();
        assert_eq!(mounts, [&own, &sibling, &git]);
        // A folder with a comma cannot be named to `--mount`.
        let comma = [("x".to_string(), std::path::PathBuf::from("/home/me/a,b"))];
        let refused = run_args(
            "c",
            Path::new("/home/me/wt"),
            &comma,
            None,
            false,
            "1000:1000",
            &labels,
            "image",
        );
        assert!(refused.is_err(), "{refused:?}");
        let refused = run_args(
            "c",
            Path::new("/home/me/wt"),
            &[],
            Some(Path::new("/home/me/a,b/.git")),
            false,
            "1000:1000",
            &labels,
            "image",
        );
        assert!(refused.is_err(), "{refused:?}");
        // With no reads and no store the arguments are the worktree's alone.
        let alone = run_args(
            "c",
            Path::new("/home/me/wt"),
            &[],
            None,
            true,
            "1000:1000",
            &labels,
            "image",
        )
        .expect("the arguments");
        assert_eq!(alone.iter().filter(|arg| *arg == "--mount").count(), 1);
        assert_eq!(alone.last().map(String::as_str), Some("infinity"));
    }

    #[test]
    fn counts_a_container_already_gone_as_discarded() {
        let answer = |stderr: &str| Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        };
        let missing = "Error response from daemon: No such container: catervas-p-ctv-1\n";
        assert_eq!(removed(&answer(missing)), Ok(()));
        // A `--rm` container whose process just ended is being removed by Docker itself.
        let going = "Error response from daemon: removal of container catervas-browser-p-ctv-1 is \
                     already in progress\n";
        assert_eq!(removed(&answer(going)), Ok(()));
        let refused = "Error response from daemon: could not kill: permission denied\n";
        assert_eq!(
            removed(&answer(refused)),
            Err(SandboxError::ContainerFailed {
                detail: refused.trim().to_owned()
            })
        );
    }

    #[test]
    fn refuses_a_worktree_the_mount_cannot_name() {
        let task = TaskId::try_from("CTV-1").expect("an id");
        for worktree in ["/tmp/a,b", "relative/worktree"] {
            let refused = DockerSandbox::create(
                "p",
                &task,
                Path::new(worktree),
                false,
                "catervas/no-such-image:0",
            );
            match refused.err() {
                Some(SandboxError::ContainerFailed { detail }) => {
                    assert!(detail.contains(worktree), "{detail}");
                }
                other => panic!("{worktree}: {other:?}"),
            }
        }
    }

    #[test]
    fn takes_the_longest_timeout_without_overflowing() {
        assert_eq!(whole_seconds(Duration::MAX), u64::MAX);
        let sandbox = DockerSandbox {
            name: format!("catervas-no-such-container-{}", std::process::id()),
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
        let missing = "Error response from daemon: No such container: catervas-p-ctv-1";
        let stopped = "Error response from daemon: container 0123abcd is not running";
        assert!(is_container_gone(&with_stderr(missing)));
        assert!(is_container_gone(&with_stderr(&format!(
            "noise\n{stopped}\n"
        ))));
        let own = "No such container: ctv-1\nthe server is not running\n";
        assert!(!is_container_gone(&with_stderr(own)));
        assert!(!is_container_gone(&with_stderr("")));
    }

    #[test]
    fn names_the_base_container_apart_from_the_task_s_own() {
        let task = TaskId::try_from("CTV-12").expect("an id");
        assert_eq!(
            base_container_name("My Project/1", &task),
            "catervas-my-project-1-ctv-12-base"
        );
        assert_ne!(base_container_name("p", &task), container_name("p", &task));
    }

    #[test]
    fn names_a_container_by_docker_rule_and_rounds_the_timeout_up() {
        let task = TaskId::try_from("CTV-12").expect("an id");
        assert_eq!(
            container_name("My Project/1", &task),
            "catervas-my-project-1-ctv-12"
        );
        assert_eq!(container_name("a_b.c-d", &task), "catervas-a_b.c-d-ctv-12");
        assert_eq!(whole_seconds(Duration::from_millis(200)), 1);
        assert_eq!(whole_seconds(Duration::ZERO), 1);
        assert_eq!(whole_seconds(Duration::from_millis(2001)), 3);
        assert_eq!(whole_seconds(Duration::from_secs(4)), 4);
    }
}
