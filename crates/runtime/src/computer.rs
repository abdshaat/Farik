//! What the first-run wizard's "Checking your computer" finds (`docs/SPEC.md` section 4.1):
//! Claude Code, git, Docker, and Catervas's sandbox image, which it can build on request.

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

    use super::{ItemState, check_computer, run};

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
}
