//! Farik's agent runtime: a session is started, read, talked to, and stopped through one trait
//! (`docs/SPEC.md` section 8.2), whatever program or recording is behind it.

/// What a session cost, and what each budget has left.
pub mod cost;
/// Commands run on an agent's behalf, and what came of them.
pub mod exec;
/// Sessions replayed from recorded transcripts.
pub mod recorded;
/// Where a task's commands run: a container per task, or the host in no-sandbox mode.
pub mod sandbox;
/// What a session is, what it reports, and the traits every runtime implements.
pub mod session;
/// The Claude Code program's `stream-json` lines, read as session events.
pub mod stream;

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
