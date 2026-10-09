//! A task's preview in Docker (`docs/SPEC.md` 8.3): `prepare` in a container of the sandbox image
//! with the network on, `start` in one with it off, and both, with the browser's, removed by name.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use farik_core::contract::TaskId;
use farik_core::team::Preview;

use crate::exec::supervise;
use crate::preview::{
    AVAILABLE_FOR, PreviewError, PreviewFactory, RunningPreview, browser_container,
};
use crate::sandbox::SandboxError;
use crate::sandbox::docker::{docker, docker_name, id, removed, stderr_of};

/// How long `prepare` may run: installs and builds, with the network on.
const PREPARE_LIMIT: Duration = Duration::from_mins(15);
/// How long `start` has to answer.
const ANSWER_LIMIT: Duration = Duration::from_secs(120);
/// How long one probe of the preview may take before it counts as no answer.
const PROBE_LIMIT: Duration = Duration::from_secs(5);
/// How long after a deadline the docker client is killed, beyond the command's own `timeout`.
const CLIENT_GRACE: Duration = Duration::from_secs(30);
/// How long `docker info` has to answer before Docker counts as not there.
const INFO_LIMIT: Duration = Duration::from_secs(10);
/// How many lines of output a failure keeps.
const TAIL_LINES: usize = 40;

/// Runs `prepare` as `$1` under `timeout`, its output kept in a file and only its last lines
/// printed, so that a long install still ends with what failed.
const PREPARE_SCRIPT: &str = "timeout -k 2 900 sh -c \"$1\" > /tmp/farik-prepare.log 2>&1; \
     code=$?; tail -n 40 /tmp/farik-prepare.log; exit $code";

/// Whether `program info` succeeds within `limit`. A client that has not answered by then is
/// killed and counts as no answer: a daemon that hangs would otherwise hold the caller for as long
/// as it likes.
fn daemon_answers(program: &str, limit: Duration) -> bool {
    let mut client = Command::new(program);
    client.arg("info");
    supervise(
        &mut client,
        limit,
        |child| {
            let _ = child.kill();
        },
        |_| {},
    )
    .is_ok_and(|finished| finished.exit_code == 0 && !finished.killed)
}

/// Makes previews in containers of one image.
pub struct DockerPreviewFactory {
    /// The image `prepare` and `start` run in, `SANDBOX_IMAGE` outside tests.
    pub image: String,
    /// The browser's image, the Playwright connector's pinned one outside tests.
    pub browser: String,
    /// The client `available` asks, `docker` outside tests.
    program: String,
    /// How long that client has to answer, `INFO_LIMIT` outside tests.
    info_limit: Duration,
    /// The last answer of `docker info`, and when it came.
    asked: Mutex<Option<(Instant, bool)>>,
}

impl DockerPreviewFactory {
    /// A factory of previews in containers of `image`, with `browser` for the Designer.
    #[must_use]
    pub fn new(image: String, browser: String) -> Self {
        Self {
            image,
            browser,
            program: "docker".to_string(),
            info_limit: INFO_LIMIT,
            asked: Mutex::new(None),
        }
    }
}

impl PreviewFactory for DockerPreviewFactory {
    fn available(&self) -> bool {
        let mut last = crate::locked(&self.asked);
        // The driver's `PolledPreviews` holds its answer as long, and records it after this one,
        // so that when it asks again this answer is out of date and Docker is asked.
        match *last {
            Some((at, answer)) if at.elapsed() < AVAILABLE_FOR => answer,
            _ => {
                let answer = daemon_answers(&self.program, self.info_limit);
                *last = Some((Instant::now(), answer));
                answer
            }
        }
    }

    fn start(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        preview: &Preview,
        tree: &str,
    ) -> Result<Box<dyn RunningPreview>, PreviewError> {
        let unavailable = |detail: String| PreviewError::DockerUnavailable { detail };
        // `--mount` splits on commas and wants an absolute source.
        if !worktree.is_absolute() || worktree.to_string_lossy().contains(',') {
            return Err(unavailable(format!(
                "the worktree {} cannot be mounted: it must be absolute and hold no comma",
                worktree.display()
            )));
        }
        if !docker(&["version"]).is_ok_and(|output| output.status.success()) {
            return Err(unavailable(
                "docker is not available: the program is missing or its daemon does not answer"
                    .to_string(),
            ));
        }
        if !run(&["image", "inspect", &self.image])?.status.success() {
            return Err(unavailable(format!(
                "the sandbox image {} is not on this machine",
                self.image
            )));
        }
        // The browser runs `--pull never`: without its image every session would fail to browse.
        if !run(&["image", "inspect", &self.browser])?.status.success() {
            return Err(unavailable(format!(
                "the Designer's browser {} is not on this computer. Open the computer check in Setup \
                 and choose Fetch it",
                self.browser
            )));
        }
        let name = container_name("preview", project_id, task_id);
        let preparing = container_name("prepare", project_id, task_id);
        for left in [&name, &browser_container(&name), &preparing] {
            run(&["rm", "-f", left])?;
        }
        let user = format!(
            "{}:{}",
            id("-u").map_err(|error| docker_error(&error))?,
            id("-g").map_err(|error| docker_error(&error))?
        );
        let labels = vec![
            format!("farik.project={project_id}"),
            format!("farik.task={}", task_id.as_str()),
        ];
        let running = DockerPreview {
            name,
            port: preview.port,
            labels,
            user,
        };
        let mount = format!("type=bind,src={},dst=/workspace", worktree.display());
        if let Some(prepare) = &preview.prepare {
            running.prepare(&preparing, &mount, &self.image, prepare)?;
        }
        let tree_label = format!("farik.tree={tree}");
        let mut args = vec!["run", "-d", "--init", "--name", &running.name];
        args.extend(["--network", "none", "--mount", &mount, "-w", "/workspace"]);
        args.extend(["--user", &running.user]);
        for label in &running.labels {
            args.extend(["--label", label]);
        }
        args.extend([
            "--label",
            &tree_label,
            &self.image,
            "sh",
            "-c",
            &preview.start,
        ]);
        let started = run(&args)?;
        if !started.status.success() {
            return Err(unavailable(stderr_of(&started)));
        }
        let url = format!("http://localhost:{}{}", preview.port, preview.path);
        if running.answers(&url) {
            return Ok(Box::new(running));
        }
        let logs = run(&["logs", "--tail", &TAIL_LINES.to_string(), &running.name])?;
        let tail = last_lines(&format!(
            "{}{}",
            String::from_utf8_lossy(&logs.stdout),
            String::from_utf8_lossy(&logs.stderr)
        ));
        let _ = running.stop("the preview never answered");
        Err(PreviewError::NeverAnswered { tail })
    }
}

/// `farik-<kind>-<project>-<task_id>` in Docker's alphabet: the task's `preview` container, and
/// its `prepare` one.
pub(crate) fn container_name(kind: &str, project_id: &str, task_id: &TaskId) -> String {
    docker_name(&format!("farik-{kind}-{project_id}-{}", task_id.as_str()))
}

/// A preview container, by name, and what its browser needs to join it.
struct DockerPreview {
    name: String,
    port: u16,
    labels: Vec<String>,
    user: String,
}

impl DockerPreview {
    /// Runs `prepare` in a container of `image` named `preparing`, with the network on and the
    /// worktree mounted, for at most `PREPARE_LIMIT`.
    fn prepare(
        &self,
        preparing: &str,
        mount: &str,
        image: &str,
        prepare: &str,
    ) -> Result<(), PreviewError> {
        let mut client = Command::new("docker");
        client.args(["run", "--rm", "--name", preparing, "--network", "bridge"]);
        client.args(["--mount", mount, "-w", "/workspace", "--user", &self.user]);
        for label in &self.labels {
            client.args(["--label", label]);
        }
        client.args([image, "sh", "-c", PREPARE_SCRIPT, "sh", prepare]);
        let finished = supervise(
            &mut client,
            PREPARE_LIMIT.saturating_add(CLIENT_GRACE),
            |child| {
                let _ = child.kill();
            },
            |_| {},
        )
        .map_err(|error| PreviewError::DockerUnavailable {
            detail: format!("docker could not be run: {error:?}"),
        })?;
        if finished.exit_code == 0 && !finished.killed {
            return Ok(());
        }
        let _ = docker(&["rm", "-f", preparing]);
        let mut tail = last_lines(&format!("{}{}", finished.stdout, finished.stderr));
        // GNU and busybox `timeout` both exit 124 when the command ran out of time.
        if finished.exit_code == 124 || finished.killed {
            tail.push_str("\nprepare ran past its 15 minutes");
        }
        Err(PreviewError::Prepare { tail })
    }

    /// Whether the preview answers at `url` inside its own namespace within `ANSWER_LIMIT`,
    /// probed once a second; false as soon as its container is gone.
    fn answers(&self, url: &str) -> bool {
        let probe = format!("curl -fsS -o /dev/null {url} || wget -q -O /dev/null {url}");
        let deadline = Instant::now() + ANSWER_LIMIT;
        while Instant::now() < deadline {
            let mut client = Command::new("docker");
            client.args(["exec", &self.name, "sh", "-c", &probe]);
            let answered = supervise(
                &mut client,
                PROBE_LIMIT,
                |child| {
                    let _ = child.kill();
                },
                |_| {},
            );
            match answered {
                Ok(finished) if finished.exit_code == 0 && !finished.killed => return true,
                Ok(finished) if finished.stderr.contains("is not running") => return false,
                Ok(finished) if finished.stderr.contains("No such container") => return false,
                _ => std::thread::sleep(Duration::from_secs(1)),
            }
        }
        false
    }
}

impl RunningPreview for DockerPreview {
    fn origin(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    fn container(&self) -> String {
        self.name.clone()
    }

    fn labels(&self) -> Vec<String> {
        self.labels.clone()
    }

    fn user(&self) -> String {
        self.user.clone()
    }

    fn stop(&self, _reason: &str) -> Result<(), PreviewError> {
        // The browser first: it lives in the preview's namespace.
        for name in [browser_container(&self.name), self.name.clone()] {
            removed(&run(&["rm", "-f", &name])?).map_err(|error| docker_error(&error))?;
        }
        Ok(())
    }
}

fn run(args: &[&str]) -> Result<Output, PreviewError> {
    docker(args).map_err(|error| docker_error(&error))
}

fn docker_error(error: &SandboxError) -> PreviewError {
    PreviewError::DockerUnavailable {
        detail: error.to_string(),
    }
}

/// The last `TAIL_LINES` lines of `text`.
fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{DockerPreviewFactory, daemon_answers};
    use crate::preview::PreviewFactory;

    /// A folder holding an executable `docker` that runs `body` under `sh`, answering its path.
    /// A child process writes it, so that no write descriptor lives in this process: another test
    /// thread's fork would inherit it until its exec, and running the script meanwhile fails with
    /// "text file busy".
    fn a_docker(test: &str, body: &str) -> String {
        let dir = std::env::temp_dir().join(format!("farik-docker-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is made");
        let program: PathBuf = dir.join("docker");
        let written = std::process::Command::new("sh")
            .args([
                "-c",
                "printf '%s\\n' \"$2\" > \"$1\" && chmod 755 \"$1\"",
                "sh",
            ])
            .arg(&program)
            .arg(format!("#!/bin/sh\n{body}"))
            .status()
            .expect("sh runs");
        assert!(written.success());
        program.display().to_string()
    }

    #[test]
    fn says_docker_is_not_there_when_its_daemon_does_not_answer_in_time() {
        // `info` that would answer after 20 s, asked to answer within a fifth of a second.
        let docker = a_docker("hangs", "exec sleep 20");
        let started = Instant::now();
        assert!(!daemon_answers(&docker, Duration::from_millis(200)));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the probe waited {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn says_docker_is_there_when_its_daemon_answers() {
        let docker = a_docker("answers", "[ \"$1\" = info ] && exit 0\nexit 1");
        assert!(daemon_answers(&docker, Duration::from_secs(30)));
    }

    /// A factory whose `docker` is `program`, giving it `limit` to answer `info`.
    fn a_factory(program: String, limit: Duration) -> DockerPreviewFactory {
        let mut factory = DockerPreviewFactory::new("image".to_string(), "browser".to_string());
        factory.program = program;
        factory.info_limit = limit;
        factory
    }

    #[test]
    fn asks_whether_docker_is_there_through_the_bounded_probe() {
        // The wiring of `available` to `daemon_answers`: a `docker` that hangs is given its limit
        // and no more, and one that answers `info` counts as there. Asking `docker info` bare
        // would run the real client, which hangs without a limit or answers whatever the
        // machine's docker does.
        let hangs = a_factory(
            a_docker("asks-hangs", "exec sleep 20"),
            Duration::from_millis(200),
        );
        let started = Instant::now();
        assert!(!hangs.available());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the probe waited {:?}",
            started.elapsed()
        );
        let answers = a_factory(
            a_docker("asks-answers", "[ \"$1\" = info ] && exit 0\nexit 1"),
            Duration::from_secs(30),
        );
        assert!(answers.available());
    }

    #[test]
    fn asks_docker_once_for_a_minute_of_answers() {
        // Each ask of the client leaves a line beside the script.
        let docker = a_docker("asks-once", "echo asked >> \"$0.asked\"\nexit 0");
        let factory = a_factory(docker.clone(), Duration::from_secs(30));
        assert!(factory.available());
        assert!(factory.available());
        let asked = std::fs::read_to_string(format!("{docker}.asked")).expect("it was asked");
        assert_eq!(asked.lines().count(), 1, "{asked}");
    }

    #[test]
    fn says_docker_is_not_there_when_its_daemon_refuses_or_the_program_is_missing() {
        let refuses = a_docker("refuses", "echo 'Cannot connect' >&2\nexit 1");
        assert!(!daemon_answers(&refuses, Duration::from_secs(30)));
        assert!(!daemon_answers(
            "/nonexistent/farik-docker",
            Duration::from_secs(30)
        ));
    }
}
