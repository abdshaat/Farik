//! The project's preview (`docs/SPEC.md` 4.1, 8.3): the team's `prepare` and `start` commands run
//! in the sandbox image, and the confined Playwright connector a session's browser runs in beside
//! it.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use catervas_core::contract::TaskId;
use catervas_core::governor::gates::DesignerBrowser;
use catervas_core::governor::permissions::ConnectorTag;
use catervas_core::team::{Agent, Preview, Team};
use catervas_protocol::event::Violation;
use catervas_roles::ConnectorDefinition;
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
    /// The labels the preview's containers carry, `catervas.project=<id>` and `catervas.task=<id>`.
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
    /// Waits until `available` answers what was found rather than for want of an answer: at once
    /// for every factory but `PolledPreviews`.
    fn settle(&self) {}
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
            detail: "Catervas runs without Docker's sandbox".to_string(),
        })
    }
}

/// How long `PolledPreviews` holds an answer before it asks again.
pub const AVAILABLE_FOR: Duration = Duration::from_secs(60);

/// A factory whose `available` never waits on `inner`'s, which for Docker is `docker info` and
/// can take 10 seconds (8.3): it answers the last answer `inner` gave, and asks again on a thread
/// of its own, one ask at a time, once that answer is older than `holds`. The first ask is made
/// with it, and until its answer comes no preview can run, as without Docker; `settle` waits for
/// that answer, which the driver does before its daemon listens, so that no request waits on
/// Docker and none is answered on the guess.
pub struct PolledPreviews {
    inner: Arc<dyn PreviewFactory>,
    holds: Duration,
    polled: Arc<(Mutex<Polled>, Condvar)>,
    spawn: Spawn,
}

/// What `PolledPreviews` knows: its last answer and when it came, and whether an ask is under way.
#[derive(Default)]
struct Polled {
    last: Option<(Instant, bool)>,
    is_asking: bool,
}

/// What runs an ask of `PolledPreviews` apart from its caller: a thread of its own outside tests.
pub(crate) type Spawn = Arc<dyn Fn(Box<dyn FnOnce() + Send>) -> std::io::Result<()> + Send + Sync>;

impl PolledPreviews {
    /// Asks `inner` at once, whose answers hold for `holds`.
    #[must_use]
    pub fn new(inner: Arc<dyn PreviewFactory>, holds: Duration) -> Self {
        Self::spawning(
            inner,
            holds,
            Arc::new(|ask| {
                std::thread::Builder::new()
                    .name("catervas-previews".to_string())
                    .spawn(ask)
                    .map(drop)
            }),
        )
    }

    /// `new`, its asks run by `spawn`.
    pub(crate) fn spawning(inner: Arc<dyn PreviewFactory>, holds: Duration, spawn: Spawn) -> Self {
        let previews = Self {
            inner,
            holds,
            polled: Arc::default(),
            spawn,
        };
        previews.last_answer();
        previews
    }

    /// The last answer, false before the first; an ask is started when the answer is missing or
    /// out of date and none is under way.
    fn last_answer(&self) -> bool {
        let mut polled = crate::locked(&self.polled.0);
        let is_due = polled.last.is_none_or(|(at, _)| at.elapsed() >= self.holds);
        if is_due && !polled.is_asking {
            polled.is_asking = true;
            let (inner, shared) = (Arc::clone(&self.inner), Arc::clone(&self.polled));
            let asked = (self.spawn)(Box::new(move || {
                // An ask that panics answers no, so that it is asked again and `settle` ends.
                let available =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inner.available()))
                        .unwrap_or(false);
                let (polled, answered) = &*shared;
                *crate::locked(polled) = Polled {
                    last: Some((Instant::now(), available)),
                    is_asking: false,
                };
                answered.notify_all();
            }));
            // Without a thread to ask on, no preview can run until the next ask.
            if asked.is_err() {
                *polled = Polled {
                    last: Some((Instant::now(), false)),
                    is_asking: false,
                };
                self.polled.1.notify_all();
            }
        }
        polled.last.is_some_and(|(_, available)| available)
    }
}

impl PreviewFactory for PolledPreviews {
    fn available(&self) -> bool {
        self.last_answer()
    }

    fn settle(&self) {
        let (polled, answered) = &*self.polled;
        drop(
            answered
                .wait_while(crate::locked(polled), |polled| polled.last.is_none())
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    fn start(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        preview: &Preview,
        tree: &str,
    ) -> Result<Box<dyn RunningPreview>, PreviewError> {
        self.inner
            .start(project_id, task_id, worktree, preview, tree)
    }
}

/// Whether the team's Designer can have its browser (D4): not without a preview to open, nor
/// where no preview can run, nor with its Playwright connector off.
#[must_use]
pub fn designer_browser(team: &Team, previews: &dyn PreviewFactory) -> DesignerBrowser {
    if !previews.available() {
        DesignerBrowser::NoSandbox
    } else if team.preview().is_none() {
        DesignerBrowser::NoPreview
    } else if team
        .designer()
        .is_some_and(|designer| !has_playwright(designer))
    {
        DesignerBrowser::NoConnector
    } else {
        DesignerBrowser::Ready
    }
}

/// Whether `agent` has the Playwright connector on in its `mcp_servers`.
#[must_use]
pub fn has_playwright(agent: &Agent) -> bool {
    agent
        .mcp_servers
        .iter()
        .flatten()
        .any(|server| server.name.as_str() == PLAYWRIGHT)
}

/// The one connector Catervas ships.
pub const PLAYWRIGHT: &str = "playwright";

/// The name of the browser container beside the preview container `preview`.
#[must_use]
pub fn browser_container(preview: &str) -> String {
    match preview.strip_prefix("catervas-preview-") {
        Some(rest) => format!("catervas-browser-{rest}"),
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
        "--pull",
        "never",
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

/// Catervas's page check, run by `node` in the connector's image: `AXE_SOURCE` is put before it.
const CHECK_SCRIPT: &str = include_str!("../assets/check-page.mjs");

/// The width `catervas_check_page` opens a page at.
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

/// The colour scheme `catervas_check_page` asks the page for.
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
/// When the check cannot run, the page does not load, or the check answers nothing Catervas reads.
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
    // `--pull never`: a missing image fails the check at once, not after gigabytes pulled unseen.
    let mut args: Vec<String> = ["run", "--rm", "-i", "--init", "--pull", "never"]
        .map(String::from)
        .to_vec();
    // Its own name, so a check Catervas gives up on can be removed (`run_docker`).
    let suffix = crate::daemon::random_token().map_err(|failed| error(format!("{failed:?}")))?;
    args.extend([
        "--name".to_string(),
        format!("catervas-check-{}-{}", preview.container(), &suffix[..12]),
    ]);
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

/// How long a page check may take on the host's clock: the script's own 90 seconds, and time for
/// docker to start and remove the container.
const CHECK_LIMIT: std::time::Duration = std::time::Duration::from_secs(150);

/// Runs `docker` with `args` and `stdin`, and answers its standard output.
fn run_docker(args: &[String], stdin: &str) -> Result<String, CheckError> {
    let mut docker = Command::new("docker");
    docker.args(args);
    // Killing the client leaves its container running until the script's own watchdog, so a
    // check given up on is removed by its name.
    run_within(docker, stdin, CHECK_LIMIT, || {
        if let Some(removal) = removal(args) {
            let mut docker = Command::new("docker");
            docker.args(removal);
            remove_within(docker, REMOVAL_LIMIT);
        }
    })
}

/// How long removing a check's container may take: a wedged docker daemon gives up the check.
const REMOVAL_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// Runs `removal`, given up on after `limit`; what it says is not read.
fn remove_within(removal: Command, limit: std::time::Duration) {
    let _ = run_within(removal, "", limit, || {});
}

/// The `docker` arguments that remove the container `args` name.
fn removal(args: &[String]) -> Option<Vec<String>> {
    let at = args.iter().position(|arg| arg == "--name")?;
    let name = args.get(at + 1)?;
    Some(vec!["rm".to_string(), "-f".to_string(), name.clone()])
}

/// Runs `command` with `stdin` on its input, killed once it runs past `limit`, and answers its
/// standard output.
fn run_within(
    mut command: Command,
    stdin: &str,
    limit: std::time::Duration,
    on_kill: impl FnOnce(),
) -> Result<String, CheckError> {
    let error = |detail: String| CheckError { detail };
    let finished = crate::exec::supervise_with_input(
        &mut command,
        stdin.as_bytes().to_vec(),
        limit,
        |child| {
            let _ = child.kill();
        },
        |_| {},
    )
    .map_err(|failed| error(format!("docker could not be run: {failed:?}")))?;
    if finished.killed {
        on_kill();
        return Err(error(format!(
            "the check ran past its {} seconds",
            limit.as_secs_f64()
        )));
    }
    if finished.exit_code == 0 {
        return Ok(finished.stdout);
    }
    let said = format!("{}{}", finished.stdout, finished.stderr);
    let lines: Vec<&str> = said.trim_end().lines().collect();
    Err(error(
        lines[lines.len().saturating_sub(CHECK_TAIL_LINES)..].join("\n"),
    ))
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;
    use std::sync::{Arc, Condvar, Mutex, PoisonError};
    use std::time::Duration;

    use catervas_core::contract::TaskId;
    use catervas_core::team::Preview;

    use super::{CheckError, PreviewError, PreviewFactory, RunningPreview, Spawn};

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

    /// A factory that is never to be asked: its `available` panics, as a slow `docker info` would
    /// hold up the caller, so that a test finds the one that asks it.
    pub(crate) struct UnaskedPreviews;

    impl PreviewFactory for UnaskedPreviews {
        fn available(&self) -> bool {
            panic!("a preview factory was asked whether a preview can run");
        }

        fn start(
            &self,
            _project_id: &str,
            _task_id: &TaskId,
            _worktree: &Path,
            _preview: &Preview,
            _tree: &str,
        ) -> Result<Box<dyn RunningPreview>, PreviewError> {
            panic!("a preview factory was asked to start a preview");
        }
    }

    /// How long a test waits on a caller or an ask before it fails rather than hang.
    const BOUND: Duration = Duration::from_secs(30);

    /// A factory whose `available` waits while it is held, as a `docker info` that has not
    /// answered yet, then answers `answer`. It counts the asks begun and notes one begun while
    /// another was under way.
    pub(crate) struct HeldPreviews {
        state: Mutex<Gate>,
        changed: Condvar,
    }

    struct Gate {
        held: bool,
        answer: bool,
        asks: usize,
        under_way: usize,
        overlapped: bool,
    }

    impl HeldPreviews {
        /// Held, answering `answer` once released.
        pub(crate) fn held(answer: bool) -> Self {
            Self {
                state: Mutex::new(Gate {
                    held: true,
                    answer,
                    asks: 0,
                    under_way: 0,
                    overlapped: false,
                }),
                changed: Condvar::new(),
            }
        }

        pub(crate) fn hold(&self) {
            crate::locked(&self.state).held = true;
        }

        pub(crate) fn release(&self) {
            crate::locked(&self.state).held = false;
            self.changed.notify_all();
        }

        /// Waits until `asks` asks have begun, failing past the bound.
        pub(crate) fn wait_for_asks(&self, asks: usize) {
            let waited = self
                .changed
                .wait_timeout_while(crate::locked(&self.state), BOUND, |state| state.asks < asks)
                .unwrap_or_else(PoisonError::into_inner);
            assert!(!waited.1.timed_out(), "{asks} asks did not begin");
        }

        pub(crate) fn asks(&self) -> usize {
            crate::locked(&self.state).asks
        }

        /// Whether an ask began while another was under way.
        pub(crate) fn overlapped(&self) -> bool {
            crate::locked(&self.state).overlapped
        }
    }

    impl PreviewFactory for HeldPreviews {
        fn available(&self) -> bool {
            let mut state = crate::locked(&self.state);
            state.asks += 1;
            state.overlapped |= state.under_way > 0;
            state.under_way += 1;
            self.changed.notify_all();
            let mut state = self
                .changed
                .wait_while(state, |state| state.held)
                .unwrap_or_else(PoisonError::into_inner);
            state.under_way -= 1;
            state.answer
        }

        fn start(
            &self,
            _project_id: &str,
            _task_id: &TaskId,
            _worktree: &Path,
            _preview: &Preview,
            _tree: &str,
        ) -> Result<Box<dyn RunningPreview>, PreviewError> {
            panic!("a held factory was asked to start a preview");
        }
    }

    /// The asks a `PolledPreviews` started, each kept until the test runs it.
    #[derive(Default)]
    pub(crate) struct QueuedAsks {
        asks: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
    }

    impl QueuedAsks {
        /// What keeps each ask here, or, `refused`, starts none.
        pub(crate) fn spawn(self: &Arc<Self>, refused: bool) -> Spawn {
            let queued = Arc::clone(self);
            Arc::new(move |ask| {
                if refused {
                    return Err(std::io::Error::other("no thread to ask on"));
                }
                crate::locked(&queued.asks).push(ask);
                Ok(())
            })
        }

        /// How many asks wait to be run.
        pub(crate) fn queued(&self) -> usize {
            crate::locked(&self.asks).len()
        }

        /// Runs the oldest ask here.
        pub(crate) fn run_next(&self) {
            let ask = crate::locked(&self.asks).remove(0);
            ask();
        }
    }

    /// Waits for `previews` to settle, failing past the bound rather than hang.
    pub(crate) fn settled(previews: &Arc<dyn PreviewFactory>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let settling = Arc::clone(previews);
        std::thread::spawn(move || {
            settling.settle();
            let _ = sender.send(());
        });
        receiver
            .recv_timeout(BOUND)
            .expect("the first answer came within the bound");
    }

    /// What `ask` answers, which must come while `docker` still holds its ask: past the bound
    /// `docker` is released, so that a caller it holds ends, and the test fails.
    pub(crate) fn at_once<T: Send>(docker: &HeldPreviews, ask: impl FnOnce() -> T + Send) -> T {
        std::thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::channel();
            scope.spawn(move || {
                let _ = sender.send(ask());
            });
            receiver.recv_timeout(BOUND).unwrap_or_else(|_| {
                docker.release();
                panic!("the caller waited on Docker's answer");
            })
        })
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
            "catervas-preview-p-frk-1".to_string()
        }

        fn labels(&self) -> Vec<String> {
            vec![
                "catervas.project=p".to_string(),
                "catervas.task=FRK-1".to_string(),
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
            save_screenshot(args);
            Ok(self.printed.clone())
        }
    }

    /// Writes `SCREENSHOT` where a check's arguments say its screenshot goes.
    fn save_screenshot(args: &[String]) {
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
            "catervas-preview-p-frk-1".to_string()
        }

        fn labels(&self) -> Vec<String> {
            vec![
                "catervas.project=p".to_string(),
                "catervas.task=FRK-1".to_string(),
            ]
        }

        fn user(&self) -> String {
            "1000:1000".to_string()
        }

        fn stop(&self, _reason: &str) -> Result<(), PreviewError> {
            Ok(())
        }

        /// A page with nothing wrong on it.
        fn run_check(&self, args: &[String], _stdin: &str) -> Result<String, CheckError> {
            save_screenshot(args);
            Ok(r#"{"violations":[]}"#.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use catervas_roles::builtin_connector;

    use super::fixtures::{
        HeldPreviews, NamedPreview, QueuedAsks, UnaskedPreviews, at_once, settled,
    };
    use super::{
        AVAILABLE_FOR, AXE_SOURCE, AXE_TAGS, PolledPreviews, PreviewFactory, connector_server,
    };
    use crate::session::McpTransport;

    fn after<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
        args.windows(2)
            .filter(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
            .collect()
    }

    #[test]
    fn says_no_preview_can_run_at_once_while_docker_is_first_asked() {
        let docker = Arc::new(HeldPreviews::held(true));
        let previews: Arc<dyn PreviewFactory> =
            Arc::new(PolledPreviews::new(Arc::clone(&docker) as _, AVAILABLE_FOR));
        // The first ask is made with the factory, before anyone wants the answer.
        docker.wait_for_asks(1);
        assert!(!at_once(&docker, || previews.available()));

        docker.release();
        settled(&previews);
        assert!(previews.available());
    }

    #[test]
    fn answers_the_last_answer_while_docker_is_asked_again_one_ask_at_a_time() {
        let docker = Arc::new(HeldPreviews::held(true));
        docker.release();
        // An answer that holds for no time is out of date as soon as it comes.
        let previews: Arc<dyn PreviewFactory> = Arc::new(PolledPreviews::new(
            Arc::clone(&docker) as _,
            Duration::ZERO,
        ));
        settled(&previews);
        docker.hold();

        assert!(at_once(&docker, || previews.available()));
        docker.wait_for_asks(2);
        for _ in 0..3 {
            assert!(at_once(&docker, || previews.available()));
        }
        assert_eq!(docker.asks(), 2);
        docker.release();
        assert!(!docker.overlapped());
    }

    #[test]
    fn asks_again_only_once_the_answer_is_out_of_date() {
        let docker = Arc::new(HeldPreviews::held(true));
        docker.release();
        let asks = Arc::new(QueuedAsks::default());

        let holding =
            PolledPreviews::spawning(Arc::clone(&docker) as _, AVAILABLE_FOR, asks.spawn(false));
        assert_eq!(asks.queued(), 1, "the first ask is made with the factory");
        asks.run_next();
        assert!(holding.available());
        assert_eq!(asks.queued(), 0, "an answer that holds starts no ask");

        let stale = PolledPreviews::spawning(docker, Duration::ZERO, asks.spawn(false));
        asks.run_next();
        assert!(stale.available());
        assert_eq!(asks.queued(), 1, "an answer out of date starts one");
        assert!(stale.available());
        assert_eq!(asks.queued(), 1, "and no other while it is under way");
    }

    #[test]
    fn says_no_preview_can_run_when_an_ask_fails() {
        // An ask that panics is an answer too, or the driver would wait for one forever.
        let previews: Arc<dyn PreviewFactory> = Arc::new(PolledPreviews::new(
            Arc::new(UnaskedPreviews),
            AVAILABLE_FOR,
        ));
        settled(&previews);
        assert!(!previews.available());
    }

    #[test]
    fn says_no_preview_can_run_without_a_thread_to_ask_on() {
        let docker = Arc::new(HeldPreviews::held(true));
        docker.release();
        let asks = Arc::new(QueuedAsks::default());
        let previews: Arc<dyn PreviewFactory> = Arc::new(PolledPreviews::spawning(
            docker,
            AVAILABLE_FOR,
            asks.spawn(true),
        ));
        settled(&previews);
        assert!(!previews.available());
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
    #[cfg(unix)]
    fn ends_a_check_that_runs_past_its_deadline() {
        // A wedged docker client ends the tool call by Catervas's own clock, not the script's.
        let started = std::time::Instant::now();
        let mut sleeping = std::process::Command::new("sleep");
        sleeping.arg("30");
        let removed = std::cell::Cell::new(false);
        let error = super::run_within(sleeping, "", std::time::Duration::from_millis(200), || {
            removed.set(true);
        })
        .expect_err("the check ran out of time");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert!(error.detail.contains("ran past"), "{error}");
        // Killing the client leaves its container running, so the container is removed too.
        assert!(removed.get());
        // Its input still reaches it, and a check that ends removes nothing.
        let mut echoing = std::process::Command::new("cat");
        echoing.arg("-");
        let removed = std::cell::Cell::new(false);
        assert_eq!(
            super::run_within(echoing, "{}\n", std::time::Duration::from_secs(5), || {
                removed.set(true);
            }),
            Ok("{}\n".to_string())
        );
        assert!(!removed.get());
    }

    #[test]
    #[cfg(unix)]
    fn gives_up_on_a_removal_that_runs_past_its_deadline() {
        // A wedged docker daemon would hold the check here after its own deadline.
        let started = std::time::Instant::now();
        let mut sleeping = std::process::Command::new("sleep");
        sleeping.arg("30");
        super::remove_within(sleeping, std::time::Duration::from_millis(200));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn removes_a_check_container_by_its_name() {
        let args: Vec<String> = ["run", "--rm", "--name", "catervas-check-x-1", "image"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            super::removal(&args),
            Some(
                ["rm", "-f", "catervas-check-x-1"]
                    .map(String::from)
                    .to_vec()
            )
        );
        assert_eq!(super::removal(&args[..2]), None);
    }

    #[test]
    fn launches_the_browser_confined() {
        let definition = builtin_connector("playwright").expect("shipped");
        let server = connector_server(
            &definition,
            &NamedPreview { port: 4400 },
            Path::new("/p/.catervas/local/browser/FRK-1/s-1"),
        );
        assert_eq!(server.name, "playwright");
        let McpTransport::Stdio { command, args } = server.transport else {
            panic!("the browser is a child process");
        };
        assert_eq!(command, "docker");
        assert_eq!(args[..2], ["run", "--rm"]);
        assert_eq!(after(&args, "--name"), ["catervas-browser-p-frk-1"]);
        assert_eq!(
            after(&args, "--label"),
            ["catervas.project=p", "catervas.task=FRK-1"]
        );
        assert_eq!(after(&args, "--user"), ["1000:1000"]);
        assert_eq!(
            after(&args, "--network"),
            ["container:catervas-preview-p-frk-1"]
        );
        assert_eq!(after(&args, "--pull"), ["never"]);
        assert_eq!(
            after(&args, "--mount"),
            ["type=bind,src=/p/.catervas/local/browser/FRK-1/s-1,dst=/output"]
        );
        // The image, then its own arguments, then Catervas's confinement.
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
        for flag in [
            "--name",
            "--label",
            "--user",
            "--network",
            "--mount",
            "--pull",
        ] {
            assert!(!server_args.contains(&flag.to_string()), "{flag}: {args:?}");
        }
    }
}
