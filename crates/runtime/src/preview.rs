//! The project's preview (`docs/SPEC.md` 4.1, 8.3): the team's `prepare` and `start` commands run
//! in the sandbox image, and the confined Playwright connector a session's browser runs in beside
//! it.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use farik_core::contract::TaskId;
use farik_core::governor::gates::DesignerBrowser;
use farik_core::governor::permissions::ConnectorTag;
use farik_core::team::{Preview, Team};
use farik_protocol::event::Violation;
use farik_roles::ConnectorDefinition;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::session::{McpServerConfig, McpTransport};

/// The preview's containers in Docker.
#[cfg(unix)]
pub mod docker;

/// A dead port on the preview's loopback: every request the browser does not make to `localhost`
/// goes to it, and fails, whatever the namespace lets through (step 12's confinement).
pub const BLACKHOLE_PROXY: &str = "http://127.0.0.1:9";

/// axe-core, as `packages/ui` pins it for the web tests, which the page check runs in the page.
pub const AXE_SOURCE: &str = include_str!("../assets/axe.min.js");

/// The axe tags the page check runs: WCAG 2.2 A and AA, rule by rule as axe files them.
pub const AXE_TAGS: [&str; 5] = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

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

/// Why the page check did not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckError {
    /// What went wrong, with the check's last lines of output when it ran.
    pub detail: String,
}

impl fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "the page check failed: {}", self.detail)
    }
}

impl std::error::Error for CheckError {}

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
    /// Runs `docker` with `args` and `stdin` on its input, and answers what it printed.
    ///
    /// # Errors
    ///
    /// When docker cannot be run, or the check exits non-zero.
    fn run_check(&self, args: &[String], stdin: &str) -> Result<String, CheckError> {
        run_docker(args, stdin)
    }
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

/// Farik's page check, run by `node` in the connector's image: `AXE_SOURCE` is put before it.
const CHECK_SCRIPT: &str = include_str!("../assets/check-page.mjs");

/// The width `farik_check_page` opens a page at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckWidth {
    /// 360 CSS pixels.
    Phone,
    /// 1280 CSS pixels.
    Desktop,
}

impl CheckWidth {
    /// Its name on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Desktop => "desktop",
        }
    }

    fn pixels(self) -> u16 {
        match self {
            Self::Phone => 360,
            Self::Desktop => 1280,
        }
    }
}

/// The colour scheme `farik_check_page` asks the page for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckTheme {
    /// `prefers-color-scheme: light`.
    Light,
    /// `prefers-color-scheme: dark`.
    Dark,
}

impl CheckTheme {
    /// Its name on the wire, which is also what the page is told it prefers.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// One page of the preview, checked at one width in one theme.
#[derive(Debug, Clone, PartialEq)]
pub struct PageCheck {
    /// The width it was opened at.
    pub width: CheckWidth,
    /// The theme it was opened in.
    pub theme: CheckTheme,
    /// Its path on the preview.
    pub path: String,
    /// What axe found against WCAG 2.2 A and AA, one per element.
    pub violations: Vec<Violation>,
    /// The screenshot the check saved.
    pub screenshot: PathBuf,
}

/// Checks the page at `path` of `preview` with axe, at `width` and in `theme`, and saves its
/// screenshot at `out`. The check runs in `definition`'s image beside the browser: as the user, in
/// the preview's network namespace, behind the same dead proxy (R4).
///
/// # Errors
///
/// When the check cannot run, the page does not load, or the check answers nothing Farik reads.
pub fn check_page(
    definition: &ConnectorDefinition,
    preview: &dyn RunningPreview,
    path: &str,
    width: CheckWidth,
    theme: CheckTheme,
    out: &Path,
) -> Result<PageCheck, CheckError> {
    let error = |detail: String| CheckError { detail };
    let (Some(folder), Some(file)) = (out.parent(), out.file_name()) else {
        return Err(error(format!("{} names no file", out.display())));
    };
    let mut args: Vec<String> = ["run", "--rm", "-i", "--init"].map(String::from).to_vec();
    for label in preview.labels() {
        args.extend(["--label".to_string(), label]);
    }
    args.extend([
        "--user".to_string(),
        preview.user(),
        "--network".to_string(),
        format!("container:{}", preview.container()),
        "--mount".to_string(),
        format!("type=bind,src={},dst=/output", folder.display()),
        "-w".to_string(),
        "/output".to_string(),
        "-e".to_string(),
        "HOME=/output".to_string(),
        "--entrypoint".to_string(),
        "node".to_string(),
        definition.image.clone(),
        // The script comes on the standard input, as a module.
        "--input-type=module".to_string(),
        "-".to_string(),
        "--url".to_string(),
        format!("{}{path}", preview.origin()),
        "--width".to_string(),
        width.pixels().to_string(),
        "--theme".to_string(),
        theme.as_str().to_string(),
        "--screenshot".to_string(),
        format!("/output/{}", file.to_string_lossy()),
        "--module-root".to_string(),
        definition.module_root.clone(),
        "--proxy-server".to_string(),
        BLACKHOLE_PROXY.to_string(),
        "--proxy-bypass".to_string(),
        "localhost".to_string(),
        "--tags".to_string(),
        AXE_TAGS.join(","),
    ]);
    let axe = serde_json::to_string(AXE_SOURCE).map_err(|failed| error(failed.to_string()))?;
    let printed =
        preview.run_check(&args, &format!("const AXE_SOURCE = {axe};\n{CHECK_SCRIPT}"))?;
    let last = printed.lines().last().unwrap_or_default();
    let Printed { violations } = serde_json::from_str(last)
        .map_err(|failed| error(format!("the check printed {last:?}: {failed}")))?;
    if !out.is_file() {
        return Err(error(format!(
            "no screenshot was saved at {}",
            out.display()
        )));
    }
    Ok(PageCheck {
        width,
        theme,
        path: path.to_string(),
        violations,
        screenshot: out.to_path_buf(),
    })
}

/// What the page check prints.
#[derive(Deserialize)]
struct Printed {
    violations: Vec<Violation>,
}

/// How many lines of a failed check's output are kept.
const CHECK_TAIL_LINES: usize = 40;

/// Runs `docker` with `args` and `stdin`, and answers its standard output.
// ponytail: no host-side deadline; the script's own watchdog ends the container within 90 s.
fn run_docker(args: &[String], stdin: &str) -> Result<String, CheckError> {
    let error = |detail: String| CheckError { detail };
    let mut child = Command::new("docker")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|failed| error(format!("docker could not be run: {failed}")))?;
    // Node reads the whole program before it prints anything, so writing first cannot block.
    if let Some(mut input) = child.stdin.take() {
        input.write_all(stdin.as_bytes()).map_err(|failed| {
            error(format!("the check could not be given its script: {failed}"))
        })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|failed| error(format!("docker did not finish: {failed}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = said.trim_end().lines().collect();
    Err(error(
        lines[lines.len().saturating_sub(CHECK_TAIL_LINES)..].join("\n"),
    ))
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;
    use std::sync::Mutex;

    use farik_core::contract::TaskId;
    use farik_core::team::Preview;

    use super::{CheckError, PreviewError, PreviewFactory, RunningPreview};

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

    /// A preview whose page check prints `printed` and saves a screenshot where its arguments
    /// say, keeping each check's arguments.
    pub(crate) struct CheckedPreview {
        pub(crate) printed: String,
        pub(crate) runs: Mutex<Vec<Vec<String>>>,
    }

    impl CheckedPreview {
        pub(crate) fn printing(printed: &str) -> Self {
            Self {
                printed: printed.to_string(),
                runs: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn runs(&self) -> Vec<Vec<String>> {
            crate::locked(&self.runs).clone()
        }
    }

    /// The value after each `flag` in `args`.
    pub(crate) fn after<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
        args.windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
            .collect()
    }

    impl RunningPreview for CheckedPreview {
        fn origin(&self) -> String {
            "http://localhost:4400".to_string()
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

        fn run_check(&self, args: &[String], _stdin: &str) -> Result<String, CheckError> {
            crate::locked(&self.runs).push(args.to_vec());
            let mounted = after(args, "--mount")
                .iter()
                .find_map(|mount| {
                    mount
                        .strip_prefix("type=bind,src=")?
                        .strip_suffix(",dst=/output")
                })
                .expect("the output folder is mounted");
            let file = after(args, "--screenshot")[0]
                .strip_prefix("/output/")
                .expect("the screenshot is saved in the output folder");
            std::fs::write(Path::new(mounted).join(file), SCREENSHOT)
                .expect("the screenshot is written");
            Ok(self.printed.clone())
        }
    }

    /// What `CheckedPreview` saves as a screenshot: a PNG's signature.
    pub(crate) const SCREENSHOT: &[u8] = b"\x89PNG\r\n\x1a\n";

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

    use super::fixtures::NamedPreview;
    use super::{AXE_SOURCE, AXE_TAGS, connector_server};
    use crate::session::McpTransport;

    fn after<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
        args.windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
            .collect()
    }

    #[test]
    fn bundles_the_axe_the_web_tests_pin() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../packages/ui/package.json"))
                .expect("packages/ui's manifest is JSON");
        let pinned = manifest["devDependencies"]["axe-core"]
            .as_str()
            .expect("packages/ui pins axe-core");
        assert_eq!(
            AXE_SOURCE.lines().next(),
            Some(format!("/*! axe v{pinned}").as_str())
        );
        assert!(include_str!("../assets/axe-LICENSE.txt").contains("Mozilla Public License"));
        assert_eq!(
            AXE_TAGS,
            ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]
        );
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
