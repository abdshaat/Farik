//! The project's preview (`docs/SPEC.md` 4.1, 8.3): the team's `prepare` and `start` commands run
//! in the sandbox image, and the confined Playwright connector a session's browser runs in beside
//! it.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use farik_core::contract::TaskId;
use farik_core::governor::gates::DesignerBrowser;
use farik_core::governor::permissions::ConnectorTag;
use farik_core::team::{Preview, Team};
use farik_roles::ConnectorDefinition;

use crate::session::{McpServerConfig, McpTransport};

/// The preview's containers in Docker.
#[cfg(unix)]
pub mod docker;

/// A dead port on the preview's loopback: every request the browser does not make to `localhost`
/// goes to it, and fails, whatever the namespace lets through (step 12's confinement).
pub const BLACKHOLE_PROXY: &str = "http://127.0.0.1:9";

/// Why a preview could not be made ready. Each `tail` is the output's last 40 lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    /// `prepare` exited non-zero or ran past its 15 minutes.
    Prepare {
        /// The output's last 40 lines.
        tail: String,
    },
    /// `start` ran, and the preview did not answer within 120 seconds.
    NeverAnswered {
        /// The output's last 40 lines.
        tail: String,
    },
    /// Docker is missing, does not answer, or refused a container.
    DockerUnavailable {
        /// What went wrong.
        detail: String,
    },
}

impl fmt::Display for PreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Prepare { tail } => write!(formatter, "the preview's prepare failed:\n{tail}"),
            Self::NeverAnswered { tail } => write!(
                formatter,
                "the preview's start did not answer within 120 seconds:\n{tail}"
            ),
            Self::DockerUnavailable { detail } => {
                write!(formatter, "Docker could not run the preview: {detail}")
            }
        }
    }
}

impl std::error::Error for PreviewError {}

/// A preview that answers, until it is stopped.
pub trait RunningPreview: Send + Sync {
    /// Where the browser finds it: `http://localhost:<port>`.
    fn origin(&self) -> String;
    /// The container whose network namespace the browser joins.
    fn container(&self) -> String;
    /// The labels the preview's containers carry, `farik.project=<id>` and `farik.task=<id>`.
    fn labels(&self) -> Vec<String>;
    /// The user the preview's containers run as, `<uid>:<gid>`.
    fn user(&self) -> String;
    /// Removes the preview's container and its browser's, by name.
    ///
    /// # Errors
    ///
    /// `DockerUnavailable` when docker cannot remove one.
    fn stop(&self, reason: &str) -> Result<(), PreviewError>;
}

/// Prepares and starts a task's preview.
pub trait PreviewFactory: Send + Sync {
    /// Whether a preview can run here at all: false in no-sandbox mode, and without Docker.
    fn available(&self) -> bool;
    /// Prepares the preview when `preview.prepare` is set, then starts it and waits for it to
    /// answer. `tree` is the task branch's `HEAD^{tree}` the preview is of.
    ///
    /// # Errors
    ///
    /// `Prepare`, `NeverAnswered`, or `DockerUnavailable`.
    fn start(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        preview: &Preview,
        tree: &str,
    ) -> Result<Box<dyn RunningPreview>, PreviewError>;
}

/// The factory of no-sandbox mode, where the Designer has no browser (D3).
pub struct NoPreviews;

impl PreviewFactory for NoPreviews {
    fn available(&self) -> bool {
        false
    }

    fn start(
        &self,
        _project_id: &str,
        _task_id: &TaskId,
        _worktree: &Path,
        _preview: &Preview,
        _tree: &str,
    ) -> Result<Box<dyn RunningPreview>, PreviewError> {
        Err(PreviewError::DockerUnavailable {
            detail: "Farik runs without Docker's sandbox".to_string(),
        })
    }
}

/// Whether the team's Designer can have its browser (D4): not without a preview to open, nor
/// where no preview can run.
#[must_use]
pub fn designer_browser(team: &Team, previews: &dyn PreviewFactory) -> DesignerBrowser {
    if !previews.available() {
        DesignerBrowser::NoSandbox
    } else if team.preview().is_none() {
        DesignerBrowser::NoPreview
    } else {
        DesignerBrowser::Ready
    }
}

/// The name of the browser container beside the preview container `preview`.
#[must_use]
pub fn browser_container(preview: &str) -> String {
    match preview.strip_prefix("farik-preview-") {
        Some(rest) => format!("farik-browser-{rest}"),
        None => format!("{preview}-browser"),
    }
}

/// The tools of `definition` a session may never call, as the program names them:
/// `mcp__<server>__<tool>` for each one not tagged `network`.
#[must_use]
pub fn disallowed_tools(definition: &ConnectorDefinition) -> Vec<String> {
    definition
        .tools
        .iter()
        .filter(|(_, tag)| **tag != ConnectorTag::Network)
        .map(|(tool, _)| format!("mcp__{}__{tool}", definition.name))
        .collect()
}

/// The MCP server a session's browser is: `definition`'s image run by docker in the preview's
/// network namespace, as the user, named and labelled, with every request that is not to
/// `localhost` sent to `BLACKHOLE_PROXY`, and only the preview's origin allowed. `output_dir` is
/// where it writes what it saves.
#[must_use]
pub fn connector_server(
    definition: &ConnectorDefinition,
    preview: &dyn RunningPreview,
    output_dir: &Path,
) -> McpServerConfig {
    let container = preview.container();
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "-i",
        "--init",
        "--name",
        &browser_container(&container),
    ]
    .map(String::from)
    .to_vec();
    for label in preview.labels() {
        args.extend(["--label".to_string(), label]);
    }
    args.extend([
        "--user".to_string(),
        preview.user(),
        "--network".to_string(),
        format!("container:{container}"),
        "--mount".to_string(),
        format!("type=bind,src={},dst=/output", output_dir.display()),
        // The user has no home in the image; the server and the browser write under /output.
        "-w".to_string(),
        "/output".to_string(),
        "-e".to_string(),
        "HOME=/output".to_string(),
        definition.image.clone(),
    ]);
    args.extend(definition.args.iter().cloned());
    args.extend([
        "--proxy-server".to_string(),
        BLACKHOLE_PROXY.to_string(),
        // Playwright drops Chromium's own loopback bypass once a proxy is set, so it is named.
        "--proxy-bypass".to_string(),
        "localhost".to_string(),
        "--allowed-origins".to_string(),
        preview.origin(),
        "--output-dir".to_string(),
        "/output".to_string(),
    ]);
    McpServerConfig {
        name: definition.name.clone(),
        transport: McpTransport::Stdio {
            command: "docker".to_string(),
            args,
        },
        headers: BTreeMap::new(),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;
    use std::sync::Mutex;

    use farik_core::contract::TaskId;
    use farik_core::team::Preview;

    use super::{PreviewError, PreviewFactory, RunningPreview};

    /// A factory that starts `NamedPreview`s, or fails with `fails`, and keeps each `prepare` it
    /// was asked to run.
    pub(crate) struct FakePreviews {
        pub(crate) fails: Option<PreviewError>,
        pub(crate) prepared: Mutex<Vec<Option<String>>>,
    }

    impl FakePreviews {
        pub(crate) fn ready() -> Self {
            Self {
                fails: None,
                prepared: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn failing(error: PreviewError) -> Self {
            Self {
                fails: Some(error),
                prepared: Mutex::new(Vec::new()),
            }
        }
    }

    impl PreviewFactory for FakePreviews {
        fn available(&self) -> bool {
            true
        }

        fn start(
            &self,
            _project_id: &str,
            _task_id: &TaskId,
            _worktree: &Path,
            preview: &Preview,
            _tree: &str,
        ) -> Result<Box<dyn RunningPreview>, PreviewError> {
            crate::locked(&self.prepared).push(preview.prepare.clone());
            match &self.fails {
                Some(error) => Err(error.clone()),
                None => Ok(Box::new(NamedPreview { port: preview.port })),
            }
        }
    }

    /// A preview that is only its names.
    pub(crate) struct NamedPreview {
        pub(crate) port: u16,
    }

    impl RunningPreview for NamedPreview {
        fn origin(&self) -> String {
            format!("http://localhost:{}", self.port)
        }

        fn container(&self) -> String {
            "farik-preview-p-frk-1".to_string()
        }

        fn labels(&self) -> Vec<String> {
            vec![
                "farik.project=p".to_string(),
                "farik.task=FRK-1".to_string(),
            ]
        }

        fn user(&self) -> String {
            "1000:1000".to_string()
        }

        fn stop(&self, _reason: &str) -> Result<(), PreviewError> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use farik_roles::builtin_connector;

    use super::connector_server;
    use super::fixtures::NamedPreview;
    use crate::session::McpTransport;

    fn after<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
        args.windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
            .collect()
    }

    #[test]
    fn launches_the_browser_confined() {
        let definition = builtin_connector("playwright").expect("shipped");
        let server = connector_server(
            &definition,
            &NamedPreview { port: 4400 },
            Path::new("/p/.farik/local/screenshots/FRK-1"),
        );
        assert_eq!(server.name, "playwright");
        let McpTransport::Stdio { command, args } = server.transport else {
            panic!("the browser is a child process");
        };
        assert_eq!(command, "docker");
        assert_eq!(args[..2], ["run", "--rm"]);
        assert_eq!(after(&args, "--name"), ["farik-browser-p-frk-1"]);
        assert_eq!(
            after(&args, "--label"),
            ["farik.project=p", "farik.task=FRK-1"]
        );
        assert_eq!(after(&args, "--user"), ["1000:1000"]);
        assert_eq!(
            after(&args, "--network"),
            ["container:farik-preview-p-frk-1"]
        );
        assert_eq!(
            after(&args, "--mount"),
            ["type=bind,src=/p/.farik/local/screenshots/FRK-1,dst=/output"]
        );
        // The image, then its own arguments, then Farik's confinement.
        let image = args
            .iter()
            .position(|arg| *arg == definition.image)
            .expect("the pinned image");
        let server_args = &args[image + 1..];
        assert_eq!(after(server_args, "--proxy-server"), ["http://127.0.0.1:9"]);
        assert_eq!(after(server_args, "--proxy-bypass"), ["localhost"]);
        assert_eq!(
            after(server_args, "--allowed-origins"),
            ["http://localhost:4400"]
        );
        assert_eq!(after(server_args, "--output-dir"), ["/output"]);
        assert!(server_args.contains(&"--isolated".to_string()), "{args:?}");
        // Docker's own options all come before the image.
        for flag in ["--name", "--label", "--user", "--network", "--mount"] {
            assert!(!server_args.contains(&flag.to_string()), "{flag}: {args:?}");
        }
    }
}
