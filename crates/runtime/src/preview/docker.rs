//! A task's preview in Docker (`docs/SPEC.md` 8.3): `prepare` in a container of the sandbox image
//! with the network on, `start` in one on the bridge too, the owner's computer at
//! `host.docker.internal`, and all, with the browser's, removed by name.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use catervas_core::contract::TaskId;
use catervas_core::team::Preview;

use crate::exec::supervise;
use crate::preview::{
    AVAILABLE_FOR, PreviewEnv, PreviewError, PreviewFactory, RunningPreview, browser_container,
};
use crate::sandbox::SandboxError;
use crate::sandbox::docker::{docker, docker_name, ensure_package_image, id, removed, stderr_of};

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
const PREPARE_SCRIPT: &str = "timeout -k 2 900 sh -c \"$1\" > /tmp/catervas-prepare.log 2>&1; \
     code=$?; tail -n 40 /tmp/catervas-prepare.log; exit $code";

/// The owner's computer as the preview's containers name it (ADR 0067).
const HOST_MAPPING: &str = "host.docker.internal:host-gateway";

/// What `prepare`'s and `start`'s `docker run` share.
pub(crate) struct Launch<'a> {
    pub(crate) name: &'a str,
    pub(crate) mount: &'a str,
    pub(crate) user: &'a str,
    pub(crate) labels: &'a [String],
    pub(crate) image: &'a str,
    /// The owner's environment file, given to Docker as `--env-file`.
    pub(crate) env: Option<&'a Path>,
}

/// The options both containers take before their image: the network, the owner's computer, the
/// owner's environment file, the worktree, the user.
fn shared_args(launch: &Launch<'_>) -> Vec<String> {
    let mut args: Vec<String> = ["--network", "bridge", "--add-host", HOST_MAPPING]
        .map(String::from)
        .to_vec();
    if let Some(env) = launch.env {
        args.extend(["--env-file".to_string(), env.display().to_string()]);
    }
    args.extend(
        [
            "--mount",
            launch.mount,
            "-w",
            "/workspace",
            "--user",
            launch.user,
        ]
        .map(String::from),
    );
    for label in launch.labels {
        args.extend(["--label".to_string(), label.clone()]);
    }
    args
}

/// `start`'s `docker run`, detached, labelled with the tree it is of.
pub(crate) fn start_args(launch: &Launch<'_>, tree_label: &str, start: &str) -> Vec<String> {
    let mut args: Vec<String> = ["run", "-d", "--init", "--name", launch.name]
        .map(String::from)
        .to_vec();
    args.extend(shared_args(launch));
    args.extend(["--label".to_string(), tree_label.to_string()]);
    args.extend([launch.image, "sh", "-c", start].map(String::from));
    args
}

/// `prepare`'s `docker run`, removed when it ends, its command run by `PREPARE_SCRIPT`.
pub(crate) fn prepare_args(launch: &Launch<'_>, prepare: &str) -> Vec<String> {
    let mut args: Vec<String> = ["run", "--rm", "--name", launch.name]
        .map(String::from)
        .to_vec();
    args.extend(shared_args(launch));
    args.extend([launch.image, "sh", "-c", PREPARE_SCRIPT, "sh", prepare].map(String::from));
    args
}

/// Prints each file in `$@` that exists and always succeeds: a kernel without IPv6 has no
/// `/proc/net/tcp6`, and `/proc/net/tcp` must still be read (ADR 0054).
const LISTENING_SCRIPT: &str = "cat \"$@\" 2>/dev/null; exit 0";

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
        env: Option<&PreviewEnv>,
        packages: &[String],
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
            return Err(unavailable(
                SandboxError::ImageMissing {
                    image: self.image.clone(),
                }
                .to_string(),
            ));
        }
        // With the owner's system packages the containers run the image built with them.
        let image = ensure_package_image(&self.image, packages)
            .map_err(|error| unavailable(error.to_string()))?;
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
            format!("catervas.project={project_id}"),
            format!("catervas.task={}", task_id.as_str()),
        ];
        let mut running = DockerPreview {
            name,
            port: preview.port.unwrap_or(0),
            labels,
            user,
        };
        let mount = format!("type=bind,src={},dst=/workspace", worktree.display());
        if let Some(prepare) = &preview.prepare {
            running.prepare(
                &preparing,
                &mount,
                &image,
                prepare,
                env.map(PreviewEnv::path),
            )?;
        }
        let tree_label = format!("catervas.tree={tree}");
        let launch = Launch {
            name: &running.name,
            mount: &mount,
            user: &running.user,
            labels: &running.labels,
            image: &image,
            env: env.map(PreviewEnv::path),
        };
        let args = start_args(&launch, &tree_label, &preview.start);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let started = run(&args)?;
        if !started.status.success() {
            return Err(unavailable(stderr_of(&started)));
        }
        if let Some(found) = running.answering_port(preview.port, &preview.path) {
            running.port = found;
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

/// `catervas-<kind>-<project>-<task_id>` in Docker's alphabet: the task's `preview` container, and
/// its `prepare` one.
pub(crate) fn container_name(kind: &str, project_id: &str, task_id: &TaskId) -> String {
    docker_name(&format!(
        "catervas-{kind}-{project_id}-{}",
        task_id.as_str()
    ))
}

/// A preview container, by name, and what its browser needs to join it.
struct DockerPreview {
    name: String,
    port: u16,
    labels: Vec<String>,
    user: String,
}

impl DockerPreview {
    /// Runs `prepare` in a container of `image` named `preparing`, with the network on, the owner's
    /// environment file when there is one, and the
    /// worktree mounted, for at most `PREPARE_LIMIT`.
    fn prepare(
        &self,
        preparing: &str,
        mount: &str,
        image: &str,
        prepare: &str,
        env: Option<&Path>,
    ) -> Result<(), PreviewError> {
        let launch = Launch {
            name: preparing,
            mount,
            user: &self.user,
            labels: &self.labels,
            image,
            env,
        };
        let mut client = Command::new("docker");
        client.args(prepare_args(&launch, prepare));
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

    /// The port on which the preview answers `path` inside its own namespace within
    /// `ANSWER_LIMIT`, tried once a second; `None` as soon as its container is gone. The candidates
    /// are `port` when it is set, else the ports the container listens on (ADR 0054), the first
    /// that answers winning.
    fn answering_port(&self, port: Option<u16>, path: &str) -> Option<u16> {
        let deadline = Instant::now() + ANSWER_LIMIT;
        while Instant::now() < deadline {
            let candidates = match port {
                Some(port) => vec![port],
                None => match self.exec(&[
                    "sh",
                    "-c",
                    LISTENING_SCRIPT,
                    "sh",
                    "/proc/net/tcp",
                    "/proc/net/tcp6",
                ]) {
                    Ok(finished) => listening_ports(&finished.stdout),
                    Err(true) => return None,
                    Err(false) => Vec::new(),
                },
            };
            for candidate in candidates {
                if Instant::now() >= deadline {
                    break;
                }
                let url = format!("http://localhost:{candidate}{path}");
                let probe = format!("curl -fsS -o /dev/null {url} || wget -q -O /dev/null {url}");
                match self.exec(&["sh", "-c", &probe]) {
                    Ok(_) => return Some(candidate),
                    Err(true) => return None,
                    Err(false) => {}
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        None
    }

    /// Runs `command` in the container within `PROBE_LIMIT`: its output when it succeeds, else
    /// `Err(true)` when the container is gone and `Err(false)` for any other failure.
    fn exec(&self, command: &[&str]) -> Result<crate::exec::Finished, bool> {
        let mut client = Command::new("docker");
        client.args(["exec", &self.name]).args(command);
        let answered = supervise(
            &mut client,
            PROBE_LIMIT,
            |child| {
                let _ = child.kill();
            },
            |_| {},
        );
        match answered {
            Ok(finished) if finished.exit_code == 0 && !finished.killed => Ok(finished),
            Ok(finished) => Err(finished.stderr.contains("is not running")
                || finished.stderr.contains("No such container")),
            Err(_) => Err(false),
        }
    }
}

/// The ports at or above 1024 that `proc_net`, the text of `/proc/net/tcp` and `/proc/net/tcp6`
/// together, shows as listening: ascending, each once. Header and malformed rows are skipped.
pub(crate) fn listening_ports(proc_net: &str) -> Vec<u16> {
    let mut ports: Vec<u16> = proc_net
        .lines()
        .filter_map(|row| {
            let mut columns = row.split_whitespace().skip(1);
            let (local, state) = (columns.next()?, columns.nth(1)?);
            let (_, port) = local.rsplit_once(':')?;
            (state == "0A")
                .then(|| u16::from_str_radix(port, 16).ok())
                .flatten()
                .filter(|port| *port >= 1024)
        })
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

impl RunningPreview for DockerPreview {
    fn origin(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    fn port(&self) -> u16 {
        self.port
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

    use super::{
        DockerPreviewFactory, LISTENING_SCRIPT, Launch, daemon_answers, listening_ports,
        prepare_args, start_args,
    };
    use crate::preview::{PREVIEW_ENV, PreviewEnv, PreviewFactory};

    fn occurrences(args: &[String], one: &str) -> usize {
        args.iter().filter(|arg| *arg == one).count()
    }

    fn before_image(args: &[String]) -> &[String] {
        let image = args.iter().position(|arg| arg == "img").expect("the image");
        &args[..image]
    }

    #[test]
    fn starts_the_preview_on_the_network_with_the_owners_environment_file() {
        let root = std::env::temp_dir().join(format!("catervas-envfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".catervas/local")).expect("the folder is made");
        assert!(PreviewEnv::read(&root).is_none());
        let labels = vec!["catervas.project=p".to_string()];
        let launch = |env| Launch {
            name: "n",
            mount: "type=bind,src=/w,dst=/workspace",
            user: "1:1",
            labels: &labels,
            image: "img",
            env,
        };
        let held = |args: &[String]| {
            let before = before_image(args);
            assert_eq!(occurrences(before, "--network"), 1, "{args:?}");
            assert!(
                before
                    .windows(2)
                    .any(|pair| pair[0] == "--network" && pair[1] == "bridge"),
                "{args:?}"
            );
            assert_eq!(occurrences(before, "--add-host"), 1, "{args:?}");
            assert_eq!(
                occurrences(before, "host.docker.internal:host-gateway"),
                1,
                "{args:?}"
            );
        };
        let bare = [
            start_args(&launch(None), "catervas.tree=t", "npm start"),
            prepare_args(&launch(None), "npm ci"),
        ];
        for args in &bare {
            held(args);
            assert_eq!(occurrences(args, "--env-file"), 0, "{args:?}");
        }

        std::fs::write(
            root.join(PREVIEW_ENV),
            "NEXT_PUBLIC_SUPABASE_URL=http://host.docker.internal:54321\nSUPABASE_SERVICE_ROLE=service-role-secret-0123\n",
        )
        .expect("the file is written");
        let env = PreviewEnv::read(&root).expect("the file is there");
        assert!(env.path().is_absolute());
        assert!(env.path().ends_with(PREVIEW_ENV));
        let path = env.path().display().to_string();
        let with = [
            start_args(&launch(Some(env.path())), "catervas.tree=t", "npm start"),
            prepare_args(&launch(Some(env.path())), "npm ci"),
        ];
        for args in &with {
            held(args);
            let before = before_image(args);
            assert_eq!(occurrences(before, "--env-file"), 1, "{args:?}");
            assert_eq!(occurrences(args, "--env-file"), 1, "{args:?}");
            let at = before
                .iter()
                .position(|arg| arg == "--env-file")
                .expect("flag");
            assert_eq!(before[at + 1], path);
            assert!(
                args.iter()
                    .all(|arg| !arg.contains("service-role-secret-0123")),
                "{args:?}"
            );
        }
        let shown = format!("{env:?}");
        assert!(!shown.contains("service-role-secret-0123"), "{shown}");
        assert!(!shown.contains("54321"), "{shown}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A folder holding an executable `docker` that runs `body` under `sh`, answering its path.
    /// A child process writes it, so that no write descriptor lives in this process: another test
    /// thread's fork would inherit it until its exec, and running the script meanwhile fails with
    /// "text file busy".
    fn a_docker(test: &str, body: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("catervas-docker-{}-{test}", std::process::id()));
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
            "/nonexistent/catervas-docker",
            Duration::from_secs(30)
        ));
    }

    const HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

    #[test]
    fn listening_ports_reads_the_listening_rows() {
        let tcp = format!(
            "{HEADER}   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 1 1 0\n\
                \x20  1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 2 1 0\n\
                \x20  2: 0100007F:1F91 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 3 1 0\n\
                \x20  3: 0100007F:0400 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5 1 0\n\
                \x20  4: 0100007F:03FF 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 6 1 0\n"
        );
        let tcp6 = format!(
            "{HEADER}   0: 00000000000000000000000001000000:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4 1 0\n"
        );
        assert_eq!(listening_ports(&format!("{tcp}{tcp6}")), [1024, 8080]);
    }

    #[test]
    fn reads_the_tcp_table_when_tcp6_is_missing() {
        let file = std::env::temp_dir().join(format!("catervas-tcp-{}", std::process::id()));
        std::fs::write(
            &file,
            format!("{HEADER}   0: 0100007F:1F90 00000000:0000 0A 0\n"),
        )
        .expect("written");
        let out = std::process::Command::new("sh")
            .args(["-c", LISTENING_SCRIPT, "sh"])
            .arg(&file)
            .arg("/nonexistent/catervas-tcp6")
            .output()
            .expect("sh ran");
        let _ = std::fs::remove_file(&file);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(
            listening_ports(&String::from_utf8_lossy(&out.stdout)),
            [8080]
        );
    }

    #[test]
    fn listening_ports_orders_and_skips_bad_rows() {
        let rows = "   0: 0100007F:1F91 00000000:0000 0A 0\n   1: 0100007F:1F90 00000000:0000 0A 0\n   2: nonsense 0A\n";
        assert_eq!(listening_ports(rows), [8080, 8081]);
        assert_eq!(listening_ports(""), Vec::<u16>::new());
    }
}
