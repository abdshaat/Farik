//! What the first-run wizard's "Checking your computer" finds (`docs/SPEC.md` section 4.1):
//! Claude Code, git, Docker, and Farik's sandbox image, which it can build on request.

use std::collections::BTreeMap;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use crate::SANDBOX_IMAGE;
use crate::claude::check_version;
use crate::session::RuntimeError;

/// How long each check may take; one that takes longer reads as missing.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// There, but older than Farik needs.
    TooOld,
    /// Docker is there, but its daemon does not answer.
    NotRunning,
}

/// One thing checked, and the version it said, when it said one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Item {
    /// How it stands.
    pub state: ItemState,
    /// The version it reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Item {
    fn missing() -> Item {
        Item {
            state: ItemState::Missing,
            version: None,
        }
    }

    fn bare(state: ItemState) -> Item {
        Item {
            state,
            version: None,
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
    /// Farik's sandbox image, `SANDBOX_IMAGE`; missing whenever Docker is not ready.
    pub sandbox_image: Item,
}

/// Checks the computer, with the programs looked for on `env`'s `PATH` and run in `env` alone.
#[must_use]
pub fn check_computer(env: &BTreeMap<String, String>) -> ComputerCheck {
    let claude = match run(env, "claude", &["--version"]) {
        Some((true, output)) => {
            let version = output.split_whitespace().next().map(str::to_string);
            match check_version(&output) {
                Ok(()) => Item {
                    state: ItemState::Ready,
                    version,
                },
                Err(RuntimeError::VersionTooOld { .. }) => Item {
                    state: ItemState::TooOld,
                    version,
                },
                Err(_) => Item::missing(),
            }
        }
        _ => Item::missing(),
    };
    let git = match run(env, "git", &["--version"]) {
        Some((true, output)) => Item {
            state: ItemState::Ready,
            version: Some(output.trim().trim_start_matches("git version ").to_string()),
        },
        _ => Item::missing(),
    };
    let docker = match run(env, "docker", &["version"]) {
        Some((true, _)) => Item::bare(ItemState::Ready),
        Some((false, _)) => Item::bare(ItemState::NotRunning),
        None => Item::missing(),
    };
    let sandbox_image = match docker.state {
        ItemState::Ready => match run(env, "docker", &["image", "inspect", SANDBOX_IMAGE]) {
            Some((true, _)) => Item::bare(ItemState::Ready),
            _ => Item::missing(),
        },
        _ => Item::missing(),
    };
    ComputerCheck {
        claude,
        git,
        docker,
        sandbox_image,
    }
}

/// Builds `SANDBOX_IMAGE` with `docker build -t <image> -`, the Dockerfile on standard input, and
/// answers the image.
///
/// # Errors
///
/// The sentence to show when docker is missing or the build fails.
pub async fn build_sandbox_image(env: &BTreeMap<String, String>) -> Result<String, String> {
    use tokio::io::AsyncWriteExt as _;

    let docker = on_path("docker", env).ok_or("Docker is not installed")?;
    let failed = |why: String| format!("the sandbox image could not be built: {why}");
    let mut child = tokio::process::Command::new(docker)
        .args(["build", "-t", SANDBOX_IMAGE, "-"])
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| failed(error.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(DOCKERFILE.as_bytes())
            .await
            .map_err(|error| failed(error.to_string()))?;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| failed(error.to_string()))?;
    if output.status.success() {
        Ok(SANDBOX_IMAGE.to_string())
    } else {
        Err(failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// Runs `program` with `args`, and answers whether it succeeded and what it printed; `None` when
/// it is not on the `PATH`, cannot be run, or takes longer than `CHECK_TIMEOUT`.
fn run(env: &BTreeMap<String, String>, program: &str, args: &[&str]) -> Option<(bool, String)> {
    let child = std::process::Command::new(on_path(program, env)?)
        .args(args)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
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
fn on_path(program: &str, env: &BTreeMap<String, String>) -> Option<PathBuf> {
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
