//! Farik's agent runtime: a session is started, read, talked to, and stopped through one trait
//! (`docs/SPEC.md` section 8.2), whatever program or recording is behind it.

/// The team's channel: messages, and whom they mention.
pub mod channel;
/// The Claude Code program as a runtime: its command line, credential, and version.
#[cfg(unix)]
pub mod claude;
/// What a session cost, and what each budget has left.
pub mod cost;
/// Contract exit criteria, run and judged.
pub mod criteria;
/// The local service: the hooks around every tool call, and Farik's tools over MCP.
#[cfg(unix)]
pub mod daemon;
/// Commands run on an agent's behalf, and what came of them.
pub mod exec;
/// The forge pull requests live on, driven through the `gh` program.
pub mod forge;
/// Farik running its team: the board read, the next thing on it done, one session at a time.
#[cfg(unix)]
pub mod orchestrator;
/// A session's system prompt, assembled in one fixed order.
pub mod prompt;
/// Sessions replayed from recorded transcripts.
pub mod recorded;
/// Where a task's commands run: a container per task, or the host in no-sandbox mode.
pub mod sandbox;
/// What a session is, what it reports, and the traits every runtime implements.
pub mod session;
/// When each session started and how it ended, in the log.
pub mod sessions;
/// An agent asleep until its model provider's limit resets.
pub mod sleep;
/// Starting and ending a sprint.
pub mod sprints;
/// The Claude Code program's `stream-json` lines, read as session events.
pub mod stream;
/// Farik's own tools, each checked against the agent's tier and the rule that owns it.
pub mod tools;
/// Transition requests, judged by the governor on the store's facts and recorded either way.
pub mod transitions;

/// A poisoned lock only means a task panicked while holding it; what it guards is still whole.
pub(crate) fn locked<Value>(mutex: &std::sync::Mutex<Value>) -> std::sync::MutexGuard<'_, Value> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes `bytes` to `path` readable by its owner alone, replacing whatever was there. The file is
/// removed first, because a mode is only given to a file as it is created, and a file left
/// behind may be readable by others.
#[cfg(unix)]
pub(crate) fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)
}

pub use exec::{ExecError, ExecResult, Executor, OUTPUT_LIMIT_BYTES};
pub use recorded::{RecordedAdapter, Transcript};
#[cfg(unix)]
pub use sandbox::docker::{DockerSandbox, DockerSandboxFactory};
#[cfg(unix)]
pub use sandbox::host::{HostSandbox, HostSandboxFactory};
pub use sandbox::{SANDBOX_IMAGE, Sandbox, SandboxError, SandboxFactory};
pub use session::{
    EndReason, McpServerConfig, McpTransport, RuntimeAdapter, RuntimeError, SessionEvent,
    SessionHandle, SessionPurpose, SessionSpec,
};
pub use stream::StreamParser;
pub use tools::{FarikTool, ToolContext, ToolDeps, ToolError, call_tool, tool_descriptors};
