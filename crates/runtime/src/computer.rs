//! What the first-run wizard's "Checking your computer" finds (`docs/SPEC.md` section 4.1):
//! Claude Code, git, Docker, and Catervas's sandbox image, which it can build on request.

use std::collections::{BTreeMap, VecDeque};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::SANDBOX_IMAGE;
use crate::claude::check_version;
use crate::session::RuntimeError;

/// How long each check may take; one that takes longer reads as missing.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// How many more times a program that is busy being written is tried, and how long apart.
const BUSY_TRIES: u32 = 5;
const BUSY_WAIT: Duration = Duration::from_millis(50);

/// The sandbox image's Dockerfile, which has no `COPY`, so it builds from an empty context.
const DOCKERFILE: &str = include_str!("../sandbox/Dockerfile");

/// How one thing on the computer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// There, and new enough.
    Ready,
    /// Not there, or it did not answer in time.
    Missing,
    /// There, but older than Catervas needs.
    TooOld,
    /// Docker is there, but its daemon does not answer.
    NotRunning,
    /// Being built in the background (ADR 0070).
    Preparing,
}

/// One thing checked, and the version it said, when it said one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Item {
    /// How it stands.
    pub state: ItemState,
    /// The version it reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The build step reached, while `Preparing` and the build has said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<u32>,
    /// The build's steps in all, with `done`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<u32>,
    /// Docker's last words, on a `Missing` item whose last build failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

impl Item {
    fn missing() -> Item {
        Item::bare(ItemState::Missing)
    }

    fn bare(state: ItemState) -> Item {
        Item::versioned(state, None)
    }

    fn versioned(state: ItemState, version: Option<String>) -> Item {
        Item {
            state,
            version,
            done: None,
            of: None,
            failed: None,
        }
    }
}

/// What `computer.check` answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ComputerCheck {
    /// Claude Code, which must be at least `MIN_CLAUDE_VERSION`.
    pub claude: Item,
    /// git.
    pub git: Item,
    /// Docker, and whether its daemon answers.
    pub docker: Item,
    /// Catervas's sandbox image, `SANDBOX_IMAGE`; missing whenever Docker is not ready.
    pub sandbox_image: Item,
    /// The UI/UX Designer's browser, the Playwright connector's pinned image; only for a team with
    /// a Designer, and missing whenever Docker is not ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub designer_browser: Option<Item>,
}

/// Checks the computer, with the programs looked for on `env`'s `PATH` and run in `env` alone;
/// `designer` says whether the team has a UI/UX Designer, whose browser is then checked too.
#[must_use]
pub fn check_computer(env: &BTreeMap<String, String>, designer: bool) -> ComputerCheck {
    let claude = match run(env, "claude", &["--version"]) {
        Some((true, output)) => {
            let version = output.split_whitespace().next().map(str::to_string);
            match check_version(&output) {
                Ok(()) => Item::versioned(ItemState::Ready, version),
                Err(RuntimeError::VersionTooOld { .. }) => {
                    Item::versioned(ItemState::TooOld, version)
                }
                Err(_) => Item::missing(),
            }
        }
        _ => Item::missing(),
    };
    let git = match run(env, "git", &["--version"]) {
        Some((true, output)) => Item::versioned(
            ItemState::Ready,
            Some(output.trim().trim_start_matches("git version ").to_string()),
        ),
        _ => Item::missing(),
    };
    let docker = match run(env, "docker", &["version"]) {
        Some((true, _)) => Item::bare(ItemState::Ready),
        Some((false, _)) => Item::bare(ItemState::NotRunning),
        None => Item::missing(),
    };
    let image = |name: &str| match docker.state {
        ItemState::Ready => match run(env, "docker", &["image", "inspect", name]) {
            Some((true, _)) => Item::bare(ItemState::Ready),
            _ => Item::missing(),
        },
        _ => Item::missing(),
    };
    let sandbox_image = image(SANDBOX_IMAGE);
    let designer_browser = designer.then(|| image(&browser_image()));
    ComputerCheck {
        claude,
        git,
        docker,
        sandbox_image,
        designer_browser,
    }
}

/// The Designer's browser image, the Playwright connector's, pinned by digest.
#[must_use]
pub fn browser_image() -> String {
    catervas_roles::builtin_connector("playwright")
        .map_or_else(String::new, |shipped| shipped.image)
}

/// Pulls the Designer's browser image by its digest, and answers it.
///
/// # Errors
///
/// The sentence to show when docker is missing or the pull fails.
pub fn pull_browser_image(env: &BTreeMap<String, String>) -> Result<String, String> {
    let docker = on_path("docker", env).ok_or("Docker is not installed")?;
    let image = browser_image();
    let failed = |why: String| format!("the browser could not be fetched: {why}");
    let output = std::process::Command::new(docker)
        .args(["pull", "--quiet", &image])
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| failed(error.to_string()))?;
    if output.status.success() {
        Ok(image)
    } else {
        Err(failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// How the background build of `SANDBOX_IMAGE` stands (ADR 0070).
#[derive(Debug, Default)]
enum Building {
    /// None runs; the last one, if any, ended well.
    #[default]
    Idle,
    /// `docker build` runs, at the step it last said.
    Running { done: Option<u32>, of: Option<u32> },
    /// The last build failed, with Docker's last words.
    Failed(String),
}

/// The sandbox image's build, run in the background by the daemon, one at a time (ADR 0070).
#[derive(Debug, Default)]
pub struct ImageBuild {
    now: Arc<Mutex<Building>>,
}

impl ImageBuild {
    /// Starts `docker build -t <image> -`, the Dockerfile on standard input, in the background, or
    /// joins the one that runs. Must be called inside the async runtime.
    ///
    /// # Errors
    ///
    /// The sentence to show when docker is missing.
    pub fn start(&self, env: &BTreeMap<String, String>) -> Result<(), String> {
        let docker = on_path("docker", env).ok_or("Docker is not installed")?;
        {
            let mut now = crate::locked(&self.now);
            if matches!(*now, Building::Running { .. }) {
                return Ok(());
            }
            *now = Building::Running {
                done: None,
                of: None,
            };
        }
        let mut env = env.clone();
        env.insert("BUILDKIT_PROGRESS".to_string(), "plain".to_string());
        let now = Arc::clone(&self.now);
        tokio::spawn(async move {
            let ended = match build(&docker, &env, &now).await {
                Ok(()) => Building::Idle,
                Err(why) => {
                    Building::Failed(format!("the sandbox image could not be built: {why}"))
                }
            };
            *crate::locked(&now) = ended;
        });
        Ok(())
    }

    /// The check's sandbox image item with the job laid over it (ADR 0070, 2): a running build
    /// reads `Preparing`, whatever Docker says of the image; a failed one gives its words to an
    /// image that is still missing.
    #[must_use]
    pub fn over(&self, checked: Item) -> Item {
        match &*crate::locked(&self.now) {
            Building::Running { done, of } => Item {
                done: *done,
                of: *of,
                ..Item::bare(ItemState::Preparing)
            },
            Building::Failed(words) if checked.state != ItemState::Ready => Item {
                failed: Some(words.clone()),
                ..checked
            },
            Building::Idle | Building::Failed(_) => checked,
        }
    }
}

/// How many of Docker's last stderr lines a failed build keeps.
const FAILED_LINES: usize = 20;

/// Runs the build, raising the job's step as the output says it, and answers why it failed.
async fn build(
    docker: &std::path::Path,
    env: &BTreeMap<String, String>,
    now: &Arc<Mutex<Building>>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    let mut child = tokio::process::Command::new(docker)
        .args(["build", "-t", SANDBOX_IMAGE, "-"])
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(DOCKERFILE.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
    }
    let step = |line: &str| {
        if let Some((k, m)) = build_step(line) {
            let mut now = crate::locked(now);
            if let Building::Running { done, of } = &mut *now {
                *done = Some(done.map_or(k, |seen| seen.max(k)));
                *of = Some(m);
            }
        }
    };
    let stdout = child.stdout.take().map(BufReader::new);
    let stderr = child.stderr.take().map(BufReader::new);
    let read_out = async {
        if let Some(out) = stdout {
            let mut lines = out.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                step(&line);
            }
        }
    };
    let read_err = async {
        let mut tail = VecDeque::new();
        if let Some(err) = stderr {
            let mut lines = err.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                step(&line);
                if tail.len() == FAILED_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        }
        tail
    };
    let ((), tail) = tokio::join!(read_out, read_err);
    let status = child.wait().await.map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(tail.into_iter().collect::<Vec<_>>().join("\n"))
    }
}

/// The step a build line says it is on, as `(step, steps in all)`: `BuildKit`'s
/// `#6 [2/4] RUN ...` and `#6 [stage-1 2/4] ...`, and the legacy builder's `Step 2/4 : ...`.
fn build_step(line: &str) -> Option<(u32, u32)> {
    let pair = |text: &str| {
        let (k, m) = text.split_once('/')?;
        Some((k.parse::<u32>().ok()?, m.parse::<u32>().ok()?))
    };
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("Step ") {
        return pair(rest.split_whitespace().next()?);
    }
    let rest = line.strip_prefix('#')?;
    let after = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    if after.len() == rest.len() {
        return None;
    }
    let inside = after.trim_start().strip_prefix('[')?;
    pair(inside.split(']').next()?.split_whitespace().next_back()?)
}

/// Runs `program` with `args`, and answers whether it succeeded and what it printed; `None` when
/// it is not on the `PATH`, cannot be run, or takes longer than `CHECK_TIMEOUT`.
fn run(env: &BTreeMap<String, String>, program: &str, args: &[&str]) -> Option<(bool, String)> {
    let program = on_path(program, env)?;
    let spawn = || {
        std::process::Command::new(&program)
            .args(args)
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
    };
    // A program being updated as it is checked is busy for a moment; it is tried again then.
    let mut tries = 0;
    let child = loop {
        match spawn() {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < BUSY_TRIES =>
            {
                tries += 1;
                std::thread::sleep(BUSY_WAIT);
            }
            spawned => break spawned.ok()?,
        }
    };
    let pid = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(CHECK_TIMEOUT) {
        Ok(Ok(output)) => Some((
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).to_string(),
        )),
        Ok(Err(_)) => None,
        Err(_) => {
            crate::exec::kill_group(pid);
            None
        }
    }
}

/// The first executable called `program` on `env`'s `PATH`.
#[must_use]
pub fn on_path(program: &str, env: &BTreeMap<String, String>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;

    env.get("PATH").and_then(|path| {
        std::env::split_paths(path)
            .map(|directory| directory.join(program))
            .find(|candidate| {
                std::fs::metadata(candidate)
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            })
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::time::Duration;

    use super::{ItemState, build_step, check_computer, run};

    /// A folder of programs, each a shell script, as the `PATH` of an environment.
    fn programs(test: &str, scripts: &[(&str, &str)]) -> BTreeMap<String, String> {
        let bin =
            std::env::temp_dir().join(format!("catervas-computer-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bin);
        std::fs::create_dir_all(&bin).expect("the folder is made");
        for (name, body) in scripts {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o755)
                .open(bin.join(name))
                .expect("the file is made");
            file.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
                .expect("written");
        }
        BTreeMap::from([(
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", bin.display()),
        )])
    }

    #[test]
    fn lists_the_designer_browser_row_only_with_a_designer() {
        let image = catervas_roles::builtin_connector("playwright")
            .expect("shipped")
            .image;
        let pulled = programs(
            "browser-ready",
            &[(
                "docker",
                &format!(
                    "case \"$*\" in\n  version) exit 0 ;;\n  'image inspect {image}') exit 0 ;;\n  *) exit 1 ;;\nesac"
                ),
            )],
        );
        assert_eq!(check_computer(&pulled, false).designer_browser, None);
        let row = check_computer(&pulled, true).designer_browser;
        assert_eq!(row.map(|item| item.state), Some(ItemState::Ready));

        let not_pulled = programs(
            "browser-missing",
            &[(
                "docker",
                "case \"$1\" in\n  version) exit 0 ;;\n  *) exit 1 ;;\nesac",
            )],
        );
        let row = check_computer(&not_pulled, true).designer_browser;
        assert_eq!(row.map(|item| item.state), Some(ItemState::Missing));
        let serialised =
            serde_json::to_value(check_computer(&not_pulled, false)).expect("serialises");
        assert!(serialised.get("designer_browser").is_none(), "{serialised}");
    }

    #[test]
    fn the_image_leaves_nothing_in_tmp() {
        // Root's npm cache under HOME=/tmp would be baked into the image and unwritable by the
        // user's uid, so the last RUN, the one that runs npm, empties /tmp.
        let joined = super::DOCKERFILE.replace("\\\n", " ");
        let runs: Vec<&str> = joined
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("RUN "))
            .collect();
        let last = runs.last().expect("the Dockerfile has a RUN");
        assert!(last.contains("npm "), "{last}");
        assert!(last.ends_with("find /tmp -mindepth 1 -delete"), "{last}");
        let after = joined.rsplit_once(last).expect("the last RUN is there").1;
        assert!(
            !after.lines().any(|line| line.trim().starts_with("RUN ")),
            "{after}"
        );
    }

    #[test]
    fn the_image_carries_the_libraries_a_headless_browser_needs() {
        // A project's own Chrome (Remotion, Playwright, Puppeteer) fails to start on a bare
        // node-slim with "libnspr4.so: cannot open shared object file", and an agent cannot
        // install system packages in the sandbox, so the image's apt-get line names them.
        let joined = super::DOCKERFILE.replace("\\\n", " ");
        let apt = joined
            .lines()
            .find(|line| line.contains("apt-get install"))
            .expect("the Dockerfile installs packages");
        assert!(apt.contains("--no-install-recommends"), "{apt}");
        for package in [
            "libnss3",
            "libnspr4",
            "libdbus-1-3",
            "libatk1.0-0",
            "libatk-bridge2.0-0",
            "libcups2",
            "libdrm2",
            "libgbm1",
            "libxkbcommon0",
            "libxcomposite1",
            "libxdamage1",
            "libxfixes3",
            "libxrandr2",
            "libpango-1.0-0",
            "libcairo2",
            "libasound2",
            "fonts-liberation",
        ] {
            assert!(
                apt.split_whitespace().any(|word| word == package),
                "{package} is missing from {apt}"
            );
        }
    }

    #[test]
    fn retries_a_program_that_is_busy_being_written() {
        let bin =
            std::env::temp_dir().join(format!("catervas-computer-busy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bin);
        std::fs::create_dir_all(&bin).expect("the folder is made");
        // Held open for writing, as while it is being updated: running it fails with ETXTBSY.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(bin.join("git"))
            .expect("the file is made");
        file.write_all(b"#!/bin/sh\necho 'git version 2.43.0'\n")
            .expect("written");
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(file);
        });
        let env = BTreeMap::from([("PATH".to_string(), bin.display().to_string())]);
        let ran = run(&env, "git", &["--version"]);
        writer.join().expect("the writer lets go");
        assert_eq!(ran, Some((true, "git version 2.43.0\n".to_string())));
    }

    #[test]
    fn reads_the_step_from_either_builders_line() {
        assert_eq!(build_step("#6 [2/4] RUN apt-get update"), Some((2, 4)));
        assert_eq!(build_step("#6 [stage-1 3/5] COPY x y"), Some((3, 5)));
        assert_eq!(build_step("Step 2/4 : RUN true"), Some((2, 4)));
        assert_eq!(build_step("#1 [internal] load build definition"), None);
        assert_eq!(build_step("#6 0.5 Reading package lists"), None);
    }
}
