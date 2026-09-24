//! The Claude Code program as a runtime (`docs/SPEC.md` section 8.2): the command line a session
//! is started with, the credential and environment it gets, and the oldest version Farik runs on.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use farik_core::governor::permissions::PermissionTier;
use farik_core::team::validate_team;
use farik_store::files::yaml_value;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio_util::sync::CancellationToken;

use crate::daemon::{DaemonInfo, builtin_tool_tier};
use crate::exec::kill_group;
use crate::locked;
use crate::session::{
    EndReason, McpTransport, RuntimeAdapter, RuntimeError, SessionEvent, SessionHandle, SessionSpec,
};
use crate::stream::StreamParser;

/// The oldest Claude Code Farik runs on: the one its flags and stream were measured against.
pub const MIN_CLAUDE_VERSION: &str = "2.1.272";

/// Claude Code's built-in tools, as `claude` 2.1.280 lists them. `allowed_builtins` asks
/// `builtin_tool_tier` about each; a name the program does not have would be dropped from
/// `--tools` without a word, so only names it has are here.
pub const BUILTIN_TOOLS: &[&str] = &[
    "Bash",
    "CronCreate",
    "CronDelete",
    "CronList",
    "Edit",
    "EnterWorktree",
    "ExitWorktree",
    "Glob",
    "Grep",
    "NotebookEdit",
    "Read",
    "ScheduleWakeup",
    "SendMessage",
    "Skill",
    "Task",
    "TaskStop",
    "ToolSearch",
    "WebFetch",
    "WebSearch",
    "Write",
];

/// The name Farik's own MCP server has in every session; its tools are `mcp__farik__<name>`.
const FARIK_SERVER: &str = "farik";
/// The one tool ADR 0004 names: a session's shell is `farik_exec`.
const REFUSED_BUILTIN: &str = "Bash";
const API_KEY: &str = "ANTHROPIC_API_KEY";
const OAUTH_TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";
/// What every session's environment sets, whatever the base says: no update of the program in the
/// middle of a session, and no memory of its own beside the agent's (both read so by `claude`
/// 2.1.280, which takes `1` as true).
const QUIET: &[(&str, &str)] = &[
    ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"),
    ("DISABLE_AUTOUPDATER", "1"),
];
const SYSTEM_PROMPT_FILE: &str = "system-prompt.md";
const MCP_CONFIG_FILE: &str = "mcp.json";
/// How much of the program's standard error an error end keeps.
const STDERR_TAIL_BYTES: usize = 4_096;
/// How long the program has to exit once its result is read.
const EXIT_GRACE: Duration = Duration::from_secs(5);
/// How long `claude --version` has to answer.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
/// How long an error end waits for the rest of standard error once the group is killed.
const TAIL_GRACE: Duration = Duration::from_millis(500);
/// Why `send` refuses while a session runs.
const ONE_MESSAGE: &str = "a Farik session takes one message; resume it for another";

/// A value that must not be printed: its `Debug` says `[redacted]`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Holds `value`.
    #[must_use]
    pub fn new(value: String) -> Secret {
        Secret(value)
    }

    /// The value itself, for the one place that hands it on.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

/// How a session pays: the user's API key, or the token of their subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeCredential {
    /// Passed to the program as `ANTHROPIC_API_KEY`.
    ApiKey(Secret),
    /// Passed to the program as `CLAUDE_CODE_OAUTH_TOKEN`.
    OauthToken(Secret),
}

/// The credential an environment holds: `ANTHROPIC_API_KEY` first, else
/// `CLAUDE_CODE_OAUTH_TOKEN`; a blank value is none.
#[must_use]
pub fn credential_from_env(env: &BTreeMap<String, String>) -> Option<ClaudeCredential> {
    let named = |name: &str| {
        env.get(name)
            .filter(|value| !value.trim().is_empty())
            .map(|value| Secret::new(value.clone()))
    };
    named(API_KEY)
        .map(ClaudeCredential::ApiKey)
        .or_else(|| named(OAUTH_TOKEN).map(ClaudeCredential::OauthToken))
}

/// Where the program is, what its hooks run, and what its sessions are given. Its `Debug` names
/// the environment's variables and not their values, which may be secrets.
#[derive(Clone)]
pub struct ClaudeConfig {
    /// The `claude` program.
    pub claude_path: PathBuf,
    /// The `farik` program the hooks run.
    pub hook_command: PathBuf,
    /// The daemon's `daemon.json`, which the hooks read.
    pub daemon_file: PathBuf,
    /// The daemon itself: its port and token go into each session's MCP config.
    pub daemon: DaemonInfo,
    /// `.farik/local/sessions`: each session's prompt and MCP config, under its id.
    pub sessions_dir: PathBuf,
    /// `.farik/team.yaml`, read at each session start for the protected paths.
    pub team_file: PathBuf,
    /// The whole environment the program gets besides its credential; nothing else is inherited.
    pub env: BTreeMap<String, String>,
}

impl fmt::Debug for ClaudeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeConfig")
            .field("claude_path", &self.claude_path)
            .field("hook_command", &self.hook_command)
            .field("daemon_file", &self.daemon_file)
            .field("daemon", &self.daemon)
            .field("sessions_dir", &self.sessions_dir)
            .field("team_file", &self.team_file)
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Starts Claude Code sessions as child processes, and resumes the ones it started.
pub struct ClaudeAdapter {
    credential: ClaudeCredential,
    config: ClaudeConfig,
    /// Each session this adapter started: its spec, and a token cancelled once its process is
    /// gone.
    specs: Mutex<BTreeMap<String, (SessionSpec, CancellationToken)>>,
}

impl fmt::Debug for ClaudeAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeAdapter")
            .field("credential", &self.credential)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ClaudeAdapter {
    /// An adapter for the program at `config.claude_path`, once `<claude> --version` says it is
    /// no older than `MIN_CLAUDE_VERSION`.
    ///
    /// # Errors
    ///
    /// `Spawn` when the program cannot be run or names no version; `VersionTooOld` when it is
    /// older than the minimum.
    pub fn new(
        credential: ClaudeCredential,
        config: ClaudeConfig,
    ) -> Result<ClaudeAdapter, RuntimeError> {
        check_version(&version_output(&config)?)?;
        Ok(ClaudeAdapter {
            credential,
            config,
            specs: Mutex::new(BTreeMap::new()),
        })
    }

    fn run(
        &self,
        spec: &SessionSpec,
        prompt: &str,
        resume: bool,
    ) -> Result<(Box<dyn SessionHandle>, CancellationToken), RuntimeError> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(RuntimeError::Spawn {
                detail: "a session is started inside a tokio runtime".to_string(),
            });
        }
        let session_dir = self.config.sessions_dir.join(&spec.session_id);
        let args = claude_args(spec, &self.config, &session_dir, resume)?;
        write_session_files(spec, &self.config, &session_dir)?;
        let mut child = tokio::process::Command::new(&self.config.claude_path)
            .args(&args)
            .current_dir(&spec.cwd)
            .env_clear()
            .envs(child_env(&self.credential, &self.config.env))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own group, so that a kill reaches the program's children, which would otherwise
            // hold its standard output open.
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| RuntimeError::Spawn {
                detail: format!(
                    "{} cannot be run: {error}",
                    self.config.claude_path.display()
                ),
            })?;
        let (Some(pid), Some(stdin), Some(stdout), Some(stderr)) = (
            child.id(),
            child.stdin.take(),
            child.stdout.take(),
            child.stderr.take(),
        ) else {
            return Err(RuntimeError::Spawn {
                detail: "the program's pipes were not opened".to_string(),
            });
        };
        let (sender, receiver) = channel(64);
        let ended = Arc::new(Mutex::new(None));
        let cancel = CancellationToken::new();
        let done = CancellationToken::new();
        let process = Process {
            child,
            pid,
            stdin,
            stdout,
            tail: collect_tail(stderr),
        };
        tokio::spawn(supervise(
            process,
            user_line(prompt),
            spec.limits.max_wall_clock,
            sender,
            Arc::clone(&ended),
            cancel.clone(),
            done.clone(),
        ));
        let handle: Box<dyn SessionHandle> = Box::new(ClaudeSession {
            session_id: spec.session_id.clone(),
            receiver,
            ended,
            cancel,
        });
        Ok((handle, done))
    }
}

impl RuntimeAdapter for ClaudeAdapter {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let (handle, done) = self.run(&spec, &spec.initial_prompt, false)?;
        locked(&self.specs).insert(spec.session_id.clone(), (spec, done));
        Ok(handle)
    }

    fn resume(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let (spec, done) = locked(&self.specs)
            .get(session_id)
            .cloned()
            .ok_or_else(|| RuntimeError::Spawn {
                detail: format!("this adapter did not start that session: {session_id}"),
            })?;
        if !done.is_cancelled() {
            return Err(RuntimeError::Spawn {
                detail: format!("that session is still running: {session_id}"),
            });
        }
        let (handle, done) = self.run(&spec, prompt, true)?;
        locked(&self.specs).insert(spec.session_id.clone(), (spec, done));
        Ok(handle)
    }
}

/// A running Claude Code session.
struct ClaudeSession {
    session_id: String,
    receiver: Receiver<SessionEvent>,
    ended: Arc<Mutex<Option<EndReason>>>,
    cancel: CancellationToken,
}

impl SessionHandle for ClaudeSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn events(&mut self) -> &mut Receiver<SessionEvent> {
        &mut self.receiver
    }

    fn send(&self, _text: &str) -> Result<(), RuntimeError> {
        // A message sent mid-turn makes the program answer with a second result nobody reads.
        Err(match *locked(&self.ended) {
            Some(EndReason::Aborted) => RuntimeError::Aborted,
            Some(EndReason::Limit) => RuntimeError::Limit,
            _ => RuntimeError::Spawn {
                detail: ONE_MESSAGE.to_string(),
            },
        })
    }

    fn abort(&self) -> Result<(), RuntimeError> {
        self.cancel.cancel();
        Ok(())
    }
}

/// The program, and the pipes the supervisor reads and writes.
struct Process {
    child: Child,
    pid: u32,
    stdin: ChildStdin,
    stdout: ChildStdout,
    tail: tokio::task::JoinHandle<Vec<u8>>,
}

/// How the supervisor stopped reading.
enum Stop {
    /// The program's `result` line: the session is over, and the program may exit on its own.
    Result,
    /// Farik stopped it; the group is killed.
    Killed(EndReason, String),
    /// The program closed its output without a result.
    Exited,
}

/// Kills the program's group if the supervisor is dropped before it has seen the group killed,
/// as it is when the runtime it runs on shuts down, so that nothing the program started outlives
/// the runtime.
/// Whether or not it kills, it says the session is over through `done` as it goes.
struct GroupGuard {
    pid: u32,
    is_armed: bool,
    done: CancellationToken,
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        if self.is_armed {
            kill_group(self.pid);
        }
        self.done.cancel();
    }
}

/// Plays the program's output as events until its result, its wall clock, its caller's abort, or
/// its exit, whichever is first; sends `Ended` once the program is stopped; then sees its group
/// gone. Neither a caller that stops reading nor a child that holds a pipe open delays the wall
/// clock or the abort.
async fn supervise(
    process: Process,
    first_line: String,
    wall_clock: Duration,
    sender: Sender<SessionEvent>,
    ended: Arc<Mutex<Option<EndReason>>>,
    cancel: CancellationToken,
    done: CancellationToken,
) {
    let Process {
        mut child,
        pid,
        mut stdin,
        stdout,
        tail,
    } = process;
    let mut guard = GroupGuard {
        pid,
        is_armed: true,
        done,
    };
    // A program that stopped reading ends with its output, which is read below.
    let _ = stdin.write_all(first_line.as_bytes()).await;
    let _ = stdin.flush().await;
    let mut lines = BufReader::new(stdout).lines();
    let stop = play(&mut lines, wall_clock, &sender, &ended, &cancel).await;
    drop(stdin);
    if matches!(stop, Stop::Result) {
        // The program's output closes as it exits; lines after the result are dropped. One that
        // has not exited within its grace is killed below.
        let _ = tokio::time::timeout(EXIT_GRACE, async {
            while let Ok(Some(_)) = lines.next_line().await {}
        })
        .await;
    }
    // The one kill: the program, if it still runs, and whatever it left behind in its group,
    // which may hold its standard error open. It comes before the leader is reaped, so the
    // group's id cannot yet name another group.
    kill_group(pid);
    guard.is_armed = false;
    let status = tokio::time::timeout(EXIT_GRACE, child.wait()).await;
    // The process is gone, so the session may be resumed.
    guard.done.cancel();
    let last = match stop {
        Stop::Result => None,
        Stop::Killed(reason, detail) => Some((reason, detail)),
        Stop::Exited => {
            let tail = tokio::time::timeout(TAIL_GRACE, tail)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            let status = match status {
                Ok(Ok(status)) => status.to_string(),
                Ok(Err(error)) => error.to_string(),
                Err(_) => "it did not exit".to_string(),
            };
            Some((
                EndReason::Error,
                format!(
                    "the program ended without a result ({status}): {}",
                    String::from_utf8_lossy(&tail).trim()
                ),
            ))
        }
    };
    if let Some((reason, detail)) = last {
        *locked(&ended) = Some(reason);
        let _ = sender
            .send(SessionEvent::Ended {
                reason,
                detail,
                resets_at: None,
            })
            .await;
    }
}

/// Plays the program's output to `sender` until its result, its wall clock, its caller's abort,
/// a line the parser refuses, its caller leaving, or its exit, and says which.
async fn play(
    lines: &mut Lines<BufReader<ChildStdout>>,
    wall_clock: Duration,
    sender: &Sender<SessionEvent>,
    ended: &Mutex<Option<EndReason>>,
    cancel: &CancellationToken,
) -> Stop {
    let mut parser = StreamParser::default();
    let deadline = tokio::time::sleep(wall_clock);
    tokio::pin!(deadline);
    let aborted = || Stop::Killed(EndReason::Aborted, "aborted by its caller".to_string());
    let limit = || {
        Stop::Killed(
            EndReason::Limit,
            format!(
                "the session ran past its wall clock of {} s",
                wall_clock.as_secs()
            ),
        )
    };
    let unread = || Stop::Killed(EndReason::Aborted, "nobody reads the session".to_string());
    'read: loop {
        let line = tokio::select! {
            () = cancel.cancelled() => break aborted(),
            () = &mut deadline => break limit(),
            () = sender.closed() => break unread(),
            line = lines.next_line() => line,
        };
        let Ok(Some(line)) = line else {
            break Stop::Exited;
        };
        if line.trim().is_empty() {
            continue;
        }
        let events = match parser.parse_line(&line) {
            Ok(events) => events,
            Err(error) => break Stop::Killed(EndReason::Error, error.to_string()),
        };
        let mut is_over = false;
        for event in events {
            // A caller that does not read must not hold the wall clock or the abort off.
            let permit = tokio::select! {
                () = cancel.cancelled() => break 'read aborted(),
                () = &mut deadline => break 'read limit(),
                permit = sender.reserve() => permit,
            };
            let Ok(permit) = permit else {
                break 'read unread();
            };
            if let SessionEvent::Ended { reason, .. } = &event {
                *locked(ended) = Some(*reason);
                is_over = true;
            }
            permit.send(event);
        }
        if is_over {
            break Stop::Result;
        }
    }
}

/// Reads `stderr` to its end, keeping the last `STDERR_TAIL_BYTES`.
fn collect_tail(
    mut stderr: impl AsyncRead + Unpin + Send + 'static,
) -> tokio::task::JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        let mut tail = Vec::new();
        let mut buffer = [0_u8; 4_096];
        while let Ok(read) = stderr.read(&mut buffer).await {
            if read == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..read]);
            if tail.len() > STDERR_TAIL_BYTES {
                tail.drain(..tail.len() - STDERR_TAIL_BYTES);
            }
        }
        tail
    })
}

/// The one `stream-json` user line a session is given.
fn user_line(prompt: &str) -> String {
    format!(
        "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":{}}}}}\n",
        Value::String(prompt.to_string())
    )
}

/// The built-in tools `tiers` grant, from `BUILTIN_TOOLS`, in name order. Never `Bash`, which
/// has no tier.
#[must_use]
pub fn allowed_builtins(tiers: &BTreeSet<PermissionTier>) -> Vec<String> {
    let mut allowed: Vec<String> = BUILTIN_TOOLS
        .iter()
        .filter(|tool| builtin_tool_tier(tool).is_some_and(|tier| tiers.contains(&tier)))
        .map(|tool| (*tool).to_string())
        .collect();
    allowed.sort();
    allowed
}

/// The arguments `claude` is run with for `spec`, its files in `session_dir`: a new session, or
/// with `resume` the same line with `--resume <id>` in place of `--session-id <id>`. The settings
/// are built from the team file as it is now, so a protected path added between sessions applies
/// to the next one. No argument holds the daemon's token, which is in the MCP config file.
///
/// # Errors
///
/// `Spawn` when the spec names its own `farik` server, or the team file cannot be read, since a
/// session without its protected paths would not be governed.
pub fn claude_args(
    spec: &SessionSpec,
    config: &ClaudeConfig,
    session_dir: &Path,
    resume: bool,
) -> Result<Vec<String>, RuntimeError> {
    refuse_a_farik_server(spec)?;
    let protected = protected_paths(&config.team_file)?;
    refuse_an_unexpressible_glob(&protected)?;
    let settings = settings_json(config, &protected);
    let session_flag = if resume { "--resume" } else { "--session-id" };
    let args = [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        session_flag,
        &spec.session_id,
        "--model",
        &spec.model,
        "--effort",
        &spec.effort.to_string(),
        "--append-system-prompt-file",
        &session_dir.join(SYSTEM_PROMPT_FILE).display().to_string(),
        "--tools",
        &spec.builtin_tools.join(","),
        "--disallowedTools",
        REFUSED_BUILTIN,
        "--mcp-config",
        &session_dir.join(MCP_CONFIG_FILE).display().to_string(),
        "--strict-mcp-config",
        "--permission-prompt-tool",
        "mcp__farik__permission",
        "--max-turns",
        &spec.limits.max_tool_calls.saturating_add(1).to_string(),
        "--setting-sources",
        "",
        "--settings",
        &settings.to_string(),
    ];
    Ok(args.iter().map(|arg| (*arg).to_string()).collect())
}

/// Writes `session_dir`'s two files: `system-prompt.md`, the spec's exact prompt, kept after the
/// session as the record of what it was told; and `mcp.json`, mode 0600, naming Farik's server
/// with the daemon's token and the session's id, and the spec's other servers.
///
/// # Errors
///
/// `Spawn` when the spec names its own `farik` server or a file cannot be written.
pub fn write_session_files(
    spec: &SessionSpec,
    config: &ClaudeConfig,
    session_dir: &Path,
) -> Result<(), RuntimeError> {
    refuse_a_farik_server(spec)?;
    let io = |path: &Path, error: std::io::Error| RuntimeError::Spawn {
        detail: format!("{} cannot be written: {error}", path.display()),
    };
    std::fs::create_dir_all(session_dir).map_err(|error| io(session_dir, error))?;
    let prompt = session_dir.join(SYSTEM_PROMPT_FILE);
    std::fs::write(&prompt, &spec.system_prompt).map_err(|error| io(&prompt, error))?;
    let mcp = session_dir.join(MCP_CONFIG_FILE);
    crate::write_private(
        &mcp,
        mcp_config(spec, &config.daemon).to_string().as_bytes(),
    )
    .map_err(|error| io(&mcp, error))
}

/// The program's whole environment: `base`, the credential's one variable, and `QUIET`.
#[must_use]
pub fn child_env(
    credential: &ClaudeCredential,
    base: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = base.clone();
    env.remove(API_KEY);
    env.remove(OAUTH_TOKEN);
    let (name, secret) = match credential {
        ClaudeCredential::ApiKey(secret) => (API_KEY, secret),
        ClaudeCredential::OauthToken(secret) => (OAUTH_TOKEN, secret),
    };
    env.insert(name.to_string(), secret.expose().to_string());
    for (name, value) in QUIET {
        env.insert((*name).to_string(), (*value).to_string());
    }
    env
}

/// What `<claude> --version` prints, run with only the base environment and given
/// `VERSION_TIMEOUT` to answer.
fn version_output(config: &ClaudeConfig) -> Result<String, RuntimeError> {
    let refused = |detail: String| RuntimeError::Spawn {
        detail: format!("`{} --version` {detail}", config.claude_path.display()),
    };
    let mut child = std::process::Command::new(&config.claude_path)
        .arg("--version")
        .env_clear()
        .envs(&config.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|error| refused(format!("cannot be run: {error}")))?;
    let deadline = std::time::Instant::now() + VERSION_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                kill_group(child.id());
                let _ = child.wait();
                return Err(refused(format!(
                    "did not answer within {} s",
                    VERSION_TIMEOUT.as_secs()
                )));
            }
            Err(error) => return Err(refused(format!("cannot be waited for: {error}"))),
        }
    }
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        std::io::Read::read_to_string(&mut stdout, &mut output)
            .map_err(|error| refused(format!("cannot be read: {error}")))?;
    }
    Ok(output)
}

/// Reads what `claude --version` printed and refuses a version older than `MIN_CLAUDE_VERSION`.
///
/// # Errors
///
/// `VersionTooOld` for an older one; `Spawn` when the text starts with no `major.minor.patch`.
pub fn check_version(output: &str) -> Result<(), RuntimeError> {
    let found = output.split_whitespace().next().unwrap_or_default();
    let parsed = parse_version(found).ok_or_else(|| RuntimeError::Spawn {
        detail: format!("`claude --version` printed {output:?}, which names no version"),
    })?;
    let required = parse_version(MIN_CLAUDE_VERSION).ok_or_else(|| RuntimeError::Spawn {
        detail: "the minimum version is not one".to_string(),
    })?;
    if parsed < required {
        return Err(RuntimeError::VersionTooOld {
            found: found.to_string(),
            required: MIN_CLAUDE_VERSION.to_string(),
        });
    }
    Ok(())
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.').map(|part| part.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

fn refuse_a_farik_server(spec: &SessionSpec) -> Result<(), RuntimeError> {
    if spec
        .mcp_servers
        .iter()
        .any(|server| server.name == FARIK_SERVER)
    {
        return Err(RuntimeError::Spawn {
            detail: "the spec names a server `farik`, which is the name of Farik's own and is \
                     added by the runtime"
                .to_string(),
        });
    }
    Ok(())
}

/// The team's protected paths, the shipped ones among them.
fn protected_paths(team_file: &Path) -> Result<Vec<String>, RuntimeError> {
    let refused = |detail: String| RuntimeError::Spawn {
        detail: format!(
            "{} cannot be read, and a session is not started without its protected paths: {detail}",
            team_file.display()
        ),
    };
    let text = std::fs::read_to_string(team_file).map_err(|error| refused(error.to_string()))?;
    let value = yaml_value(text.strip_prefix('\u{feff}').unwrap_or(&text), "team.yaml")
        .map_err(|error| refused(error.to_string()))?;
    let team = validate_team(&value).map_err(|errors| refused(format!("{errors:?}")))?;
    Ok(team.rules().protected_paths)
}

/// Refuses a protected path that a `Read(<glob>)` rule cannot say as it is: Claude Code reads a
/// rule's content up to its parentheses and takes a backslash as an escape, and in `-p` mode it
/// ignores settings it cannot read without a word, so such a path would leave the session with
/// no deny rules at all.
fn refuse_an_unexpressible_glob(protected: &[String]) -> Result<(), RuntimeError> {
    match protected.iter().find(|glob| {
        glob.chars()
            .any(|character| matches!(character, '(' | ')' | '\\') || character.is_control())
    }) {
        Some(glob) => Err(RuntimeError::Spawn {
            detail: format!(
                "the protected path {glob:?} cannot be expressed as a Read rule: it holds a \
                 parenthesis, a backslash, or a control character"
            ),
        }),
        None => Ok(()),
    }
}

/// `--settings`: both hooks on every tool, and a `Read` deny rule for each protected path, which
/// Claude Code also holds its search tools to.
fn settings_json(config: &ClaudeConfig, protected: &[String]) -> Value {
    let hook = |command: String| {
        json!([{
            "matcher": "*",
            "hooks": [{ "type": "command", "command": command }],
        }])
    };
    let line = |event: &str| {
        format!(
            "{} hook {event} --daemon {}",
            shell_quoted(&config.hook_command.display().to_string()),
            shell_quoted(&config.daemon_file.display().to_string())
        )
    };
    json!({
        "hooks": {
            // Claude Code blocks a call only on exit 2; a hook that cannot run at all (127, 126)
            // or fails otherwise would let the call through, so every failure becomes 2.
            "PreToolUse": hook(format!("{} || exit 2", line("pre-tool-use"))),
            "PostToolUse": hook(line("post-tool-use")),
        },
        "permissions": {
            "deny": protected.iter().map(|path| format!("Read({path})")).collect::<Vec<_>>(),
        },
    })
}

/// `text` as one word to a POSIX shell, which is what runs a hook's command.
fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn mcp_config(spec: &SessionSpec, daemon: &DaemonInfo) -> Value {
    let mut servers = serde_json::Map::new();
    servers.insert(
        FARIK_SERVER.to_string(),
        json!({
            "type": "http",
            "url": format!("http://127.0.0.1:{}/mcp", daemon.port),
            "headers": {
                "Authorization": format!("Bearer {}", daemon.token),
                "X-Farik-Session": spec.session_id,
            },
        }),
    );
    for server in &spec.mcp_servers {
        let entry = match &server.transport {
            McpTransport::Http { url } => {
                json!({ "type": "http", "url": url, "headers": server.headers })
            }
            McpTransport::Stdio { command, args } => {
                json!({ "type": "stdio", "command": command, "args": args })
            }
        };
        servers.insert(server.name.clone(), entry);
    }
    json!({ "mcpServers": servers })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use farik_core::governor::permissions::PermissionTier;
    use farik_core::team::fixtures::a_team_wire;
    use farik_core::team::validate_team;
    use farik_store::files::fixtures::{TempProject, a_team};
    use serde_json::{Value, json};

    use super::{
        ClaudeConfig, ClaudeCredential, Secret, allowed_builtins, check_version, child_env,
        claude_args, credential_from_env, write_session_files,
    };
    use crate::daemon::DaemonInfo;
    use crate::recorded::fixtures::a_session_spec;
    use crate::session::{McpServerConfig, McpTransport, RuntimeError, SessionSpec};

    const TOKEN: &str = "0123456789abcdef-the-daemon-token";

    fn config(project: &TempProject) -> ClaudeConfig {
        ClaudeConfig {
            claude_path: PathBuf::from("/usr/local/bin/claude"),
            hook_command: PathBuf::from("/usr/local/bin/farik"),
            daemon_file: project.root.join(".farik/local/daemon.json"),
            daemon: DaemonInfo {
                port: 47_123,
                token: TOKEN.to_string(),
                pid: 1,
            },
            sessions_dir: project.root.join(".farik/local/sessions"),
            team_file: project.root.join(".farik/team.yaml"),
            env: BTreeMap::new(),
        }
    }

    fn a_project(name: &str) -> TempProject {
        let project = TempProject::new(name);
        project
            .files()
            .write_team(&a_team())
            .expect("the team is written");
        project
    }

    fn spec() -> SessionSpec {
        SessionSpec {
            builtin_tools: vec!["Read".to_string(), "Grep".to_string()],
            ..a_session_spec()
        }
    }

    fn session_dir(config: &ClaudeConfig, spec: &SessionSpec) -> PathBuf {
        config.sessions_dir.join(&spec.session_id)
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let at = args
            .iter()
            .position(|arg| arg == flag)
            .unwrap_or_else(|| panic!("{flag} is not in {args:?}"));
        &args[at + 1]
    }

    fn settings(args: &[String]) -> Value {
        serde_json::from_str(value_after(args, "--settings")).expect("the settings are JSON")
    }

    #[test]
    fn builds_the_command_line_claude_code_needs() {
        let project = a_project("claude-args");
        let config = config(&project);
        let spec = spec();
        let dir = session_dir(&config, &spec);
        let args = claude_args(&spec, &config, &dir, false).expect("the args are built");
        let flags: Vec<&str> = args
            .iter()
            .map(String::as_str)
            .filter(|arg| arg.starts_with('-'))
            .collect();
        assert_eq!(
            flags,
            vec![
                "-p",
                "--output-format",
                "--input-format",
                "--verbose",
                "--session-id",
                "--model",
                "--effort",
                "--append-system-prompt-file",
                "--tools",
                "--disallowedTools",
                "--mcp-config",
                "--strict-mcp-config",
                "--permission-prompt-tool",
                "--max-turns",
                "--setting-sources",
                "--settings",
            ]
        );
        assert_eq!(value_after(&args, "--output-format"), "stream-json");
        assert_eq!(value_after(&args, "--input-format"), "stream-json");
        assert_eq!(value_after(&args, "--session-id"), spec.session_id);
        assert_eq!(value_after(&args, "--model"), spec.model);
        assert_eq!(value_after(&args, "--effort"), "high");
        assert_eq!(
            Path::new(value_after(&args, "--append-system-prompt-file")),
            dir.join("system-prompt.md")
        );
        assert_eq!(value_after(&args, "--tools"), "Read,Grep");
        assert_eq!(value_after(&args, "--disallowedTools"), "Bash");
        assert_eq!(
            Path::new(value_after(&args, "--mcp-config")),
            dir.join("mcp.json")
        );
        assert_eq!(
            value_after(&args, "--permission-prompt-tool"),
            "mcp__farik__permission"
        );
        assert_eq!(
            value_after(&args, "--max-turns"),
            (spec.limits.max_tool_calls + 1).to_string()
        );
        assert_eq!(value_after(&args, "--setting-sources"), "");
        assert!(
            args.iter().all(|arg| !arg.contains(TOKEN)),
            "the token is on the command line: {args:?}"
        );
    }

    #[test]
    fn refuses_a_spec_that_names_its_own_farik_server() {
        let project = a_project("claude-farik-server");
        let config = config(&project);
        let spec = SessionSpec {
            mcp_servers: vec![McpServerConfig {
                name: "farik".to_string(),
                transport: McpTransport::Http {
                    url: "http://127.0.0.1:1/mcp".to_string(),
                },
                headers: BTreeMap::new(),
            }],
            ..spec()
        };
        let dir = session_dir(&config, &spec);
        assert!(matches!(
            claude_args(&spec, &config, &dir, false),
            Err(RuntimeError::Spawn { .. })
        ));
        assert!(matches!(
            write_session_files(&spec, &config, &dir),
            Err(RuntimeError::Spawn { .. })
        ));
    }

    #[test]
    fn passes_only_the_base_environment_and_one_credential() {
        let base = BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string())]);
        let api_key = ClaudeCredential::ApiKey(Secret::new("sk-key".to_string()));
        let quiet = |pairs: [(&str, &str); 2]| -> BTreeMap<String, String> {
            pairs
                .into_iter()
                .chain([
                    ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"),
                    ("DISABLE_AUTOUPDATER", "1"),
                ])
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        };
        assert_eq!(
            child_env(&api_key, &base),
            quiet([("ANTHROPIC_API_KEY", "sk-key"), ("PATH", "/usr/bin")])
        );
        let token = ClaudeCredential::OauthToken(Secret::new("oauth".to_string()));
        assert_eq!(
            child_env(&token, &base),
            quiet([("CLAUDE_CODE_OAUTH_TOKEN", "oauth"), ("PATH", "/usr/bin")])
        );
        let overridden = BTreeMap::from([
            ("DISABLE_AUTOUPDATER".to_string(), "0".to_string()),
            (
                "CLAUDE_CODE_DISABLE_AUTO_MEMORY".to_string(),
                "0".to_string(),
            ),
        ]);
        let env = child_env(&token, &overridden);
        assert_eq!(env["DISABLE_AUTOUPDATER"], "1");
        assert_eq!(env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"], "1");
    }

    #[test]
    fn puts_the_session_and_token_in_the_mcp_config_file() {
        let project = a_project("claude-mcp-file");
        let config = config(&project);
        let spec = spec();
        let dir = session_dir(&config, &spec);
        write_session_files(&spec, &config, &dir).expect("the files are written");
        let args = claude_args(&spec, &config, &dir, false).expect("the args are built");
        let path = PathBuf::from(value_after(&args, "--mcp-config"));
        let mode = std::fs::metadata(&path)
            .expect("the file is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let file: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("JSON");
        let farik = &file["mcpServers"]["farik"];
        assert_eq!(farik["type"], "http");
        assert_eq!(farik["url"], "http://127.0.0.1:47123/mcp");
        assert_eq!(farik["headers"]["Authorization"], format!("Bearer {TOKEN}"));
        assert_eq!(
            farik["headers"]["X-Farik-Session"],
            spec.session_id.as_str()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("system-prompt.md")).expect("the prompt file"),
            spec.system_prompt
        );
    }

    #[test]
    fn wires_both_hooks_and_the_protected_paths_into_the_settings() {
        let project = a_project("claude-settings");
        let config = config(&project);
        let spec = spec();
        let args = claude_args(&spec, &config, &session_dir(&config, &spec), false)
            .expect("the args are built");
        let settings = settings(&args);
        let daemon_file = config.daemon_file.display().to_string();
        for (event, command) in [
            ("PreToolUse", "pre-tool-use"),
            ("PostToolUse", "post-tool-use"),
        ] {
            let matchers = settings["hooks"][event].as_array().expect("an array");
            assert_eq!(matchers.len(), 1, "{settings}");
            assert_eq!(matchers[0]["matcher"], "*");
            let hooks = matchers[0]["hooks"].as_array().expect("an array");
            assert_eq!(hooks.len(), 1, "{settings}");
            assert_eq!(hooks[0]["type"], "command");
            let line = hooks[0]["command"].as_str().expect("a command");
            assert!(line.contains("/usr/local/bin/farik"), "{line}");
            assert!(line.contains(&format!("hook {command} --daemon")), "{line}");
            assert!(line.contains(&daemon_file), "{line}");
        }
        let deny: Vec<&str> = settings["permissions"]["deny"]
            .as_array()
            .expect("an array")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(deny.contains(&"Read(.env)"), "{deny:?}");
        assert!(deny.contains(&"Read(**/*.pem)"), "{deny:?}");
    }

    #[test]
    fn makes_the_pre_tool_use_hook_block_when_it_cannot_run() {
        let project = a_project("claude-hook-fails-closed");
        let config = ClaudeConfig {
            hook_command: PathBuf::from("/opt/it's here/farik"),
            daemon_file: PathBuf::from("/tmp/a dir/daemon.json"),
            ..config(&project)
        };
        let spec = spec();
        let args = claude_args(&spec, &config, &session_dir(&config, &spec), false)
            .expect("the args are built");
        let settings = settings(&args);
        assert_eq!(
            settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            r"'/opt/it'\''s here/farik' hook pre-tool-use --daemon '/tmp/a dir/daemon.json' || exit 2"
        );
        assert_eq!(
            settings["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            r"'/opt/it'\''s here/farik' hook post-tool-use --daemon '/tmp/a dir/daemon.json'"
        );
    }

    #[test]
    fn runs_the_hook_command_as_one_word_each_and_blocks_when_it_is_missing() {
        let project = a_project("claude-hook-shell");
        let bin = project.root.join("a 'quoted' dir");
        std::fs::create_dir_all(&bin).expect("the directory");
        let farik = bin.join("farik");
        std::fs::write(&farik, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 1\n").expect("written");
        std::fs::set_permissions(&farik, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        let config = ClaudeConfig {
            hook_command: farik.clone(),
            daemon_file: bin.join("daemon.json"),
            ..config(&project)
        };
        let spec = spec();
        let args = claude_args(&spec, &config, &session_dir(&config, &spec), false)
            .expect("the args are built");
        let command = settings(&args)["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .expect("a command")
            .to_string();
        let run = |line: &str| {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(line)
                .output()
                .expect("sh runs")
        };
        let ran = run(&command);
        assert_eq!(ran.status.code(), Some(2), "a failing hook blocks");
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout),
            format!(
                "hook\npre-tool-use\n--daemon\n{}\n",
                bin.join("daemon.json").display()
            )
        );
        std::fs::remove_file(&farik).expect("removed");
        assert_eq!(
            run(&command).status.code(),
            Some(2),
            "a missing hook blocks"
        );
    }

    #[test]
    fn refuses_a_protected_path_a_read_rule_cannot_express() {
        for glob in ["infra/(old)/**", "a)b", "back\\slash", "new\nline"] {
            let project = TempProject::new("claude-unexpressible-glob");
            let mut wire = a_team_wire();
            wire["rules"]["protected_paths"] = json!([glob]);
            project
                .files()
                .write_team(&validate_team(&wire).expect("a team"))
                .expect("the team is written");
            let config = config(&project);
            let spec = spec();
            match claude_args(&spec, &config, &session_dir(&config, &spec), false) {
                Err(RuntimeError::Spawn { detail }) => {
                    assert!(detail.contains("cannot be expressed"), "{detail}");
                }
                other => panic!("expected {glob:?} refused, got {other:?}"),
            }
        }
    }

    #[test]
    fn refuses_a_session_whose_team_file_cannot_be_read() {
        let project = TempProject::new("claude-no-team");
        let config = config(&project);
        let spec = spec();
        assert!(matches!(
            claude_args(&spec, &config, &session_dir(&config, &spec), false),
            Err(RuntimeError::Spawn { .. })
        ));
    }

    #[test]
    fn resumes_with_the_same_line_and_the_resume_flag() {
        let project = a_project("claude-resume");
        let config = config(&project);
        let spec = spec();
        let dir = session_dir(&config, &spec);
        let args = claude_args(&spec, &config, &dir, true).expect("the args are built");
        assert_eq!(value_after(&args, "--resume"), spec.session_id);
        assert!(!args.iter().any(|arg| arg == "--session-id"), "{args:?}");
        let started = claude_args(&spec, &config, &dir, false).expect("the args are built");
        let without = |args: &[String], flag: &str| -> Vec<String> {
            let at = args.iter().position(|arg| arg == flag).expect("the flag");
            let mut rest = args.to_vec();
            rest.drain(at..at + 2);
            rest
        };
        assert_eq!(
            without(&args, "--resume"),
            without(&started, "--session-id")
        );
    }

    #[test]
    fn allows_only_the_builtins_the_tiers_grant() {
        let read = BTreeSet::from([PermissionTier::Read]);
        assert_eq!(
            allowed_builtins(&read),
            vec!["Glob", "Grep", "Read", "ToolSearch"]
        );
        let write = BTreeSet::from([PermissionTier::Read, PermissionTier::WriteWorkspace]);
        assert_eq!(
            allowed_builtins(&write),
            vec![
                "Edit",
                "Glob",
                "Grep",
                "NotebookEdit",
                "Read",
                "ToolSearch",
                "Write"
            ]
        );
        let everything = BTreeSet::from([
            PermissionTier::Read,
            PermissionTier::WriteWorkspace,
            PermissionTier::Network,
            PermissionTier::Execute,
            PermissionTier::GitLocal,
            PermissionTier::GitRemote,
            PermissionTier::ExternalEffect,
        ]);
        let all = allowed_builtins(&everything);
        assert!(all.contains(&"WebFetch".to_string()), "{all:?}");
        assert!(!all.contains(&"Bash".to_string()), "{all:?}");
    }

    #[test]
    fn prefers_the_api_key_and_ignores_blank_values() {
        let env = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect()
        };
        assert!(matches!(
            credential_from_env(&env(&[
                ("ANTHROPIC_API_KEY", "sk-key"),
                ("CLAUDE_CODE_OAUTH_TOKEN", "oauth")
            ])),
            Some(ClaudeCredential::ApiKey(secret)) if secret.expose() == "sk-key"
        ));
        assert!(matches!(
            credential_from_env(&env(&[
                ("ANTHROPIC_API_KEY", "  "),
                ("CLAUDE_CODE_OAUTH_TOKEN", "oauth")
            ])),
            Some(ClaudeCredential::OauthToken(secret)) if secret.expose() == "oauth"
        ));
        assert!(credential_from_env(&env(&[("CLAUDE_CODE_OAUTH_TOKEN", "")])).is_none());
        assert!(credential_from_env(&env(&[])).is_none());
    }

    #[test]
    fn hides_a_secret_when_printed() {
        let credential = ClaudeCredential::ApiKey(Secret::new("sk-very-secret".to_string()));
        let printed = format!("{credential:?}");
        assert!(printed.contains("[redacted]"), "{printed}");
        assert!(!printed.contains("sk-very-secret"), "{printed}");
        let project = TempProject::new("claude-debug");
        let printed = format!("{:?}", config(&project).daemon);
        assert!(printed.contains("[redacted]"), "{printed}");
        assert!(!printed.contains(TOKEN), "{printed}");
    }

    #[test]
    fn prints_a_config_without_its_environment_s_values() {
        let project = TempProject::new("claude-config-debug");
        let config = ClaudeConfig {
            env: BTreeMap::from([("SOME_TOKEN".to_string(), "a-value-not-to-print".to_string())]),
            ..config(&project)
        };
        let printed = format!("{config:?}");
        assert!(printed.contains("SOME_TOKEN"), "{printed}");
        assert!(!printed.contains("a-value-not-to-print"), "{printed}");
        assert!(!printed.contains(TOKEN), "{printed}");
    }

    #[test]
    fn refuses_a_claude_code_older_than_the_minimum() {
        assert_eq!(
            check_version("2.1.200 (Claude Code)\n"),
            Err(RuntimeError::VersionTooOld {
                found: "2.1.200".to_string(),
                required: "2.1.272".to_string(),
            })
        );
        assert_eq!(check_version("2.1.280 (Claude Code)\n"), Ok(()));
        assert_eq!(check_version("2.1.272 (Claude Code)"), Ok(()));
        assert_eq!(check_version("3.0.0"), Ok(()));
        assert!(matches!(
            check_version("2.0.999 (Claude Code)"),
            Err(RuntimeError::VersionTooOld { .. })
        ));
        for text in ["not a version", "2.1.280.1 (Claude Code)", "2.1", "2.1.x"] {
            assert!(
                matches!(check_version(text), Err(RuntimeError::Spawn { .. })),
                "{text}"
            );
        }
    }
}
