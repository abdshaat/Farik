//! Farik's agent runtime: a session is started, read, talked to, and stopped through one trait
//! (`docs/SPEC.md` section 8.2), whatever program or recording is behind it.

/// Sessions replayed from recorded transcripts.
pub mod recorded;
/// What a session is, what it reports, and the traits every runtime implements.
pub mod session;
/// The Claude Code program's `stream-json` lines, read as session events.
pub mod stream;

pub use recorded::{RecordedAdapter, Transcript};
pub use session::{
    EndReason, McpServerConfig, McpTransport, RuntimeAdapter, RuntimeError, SessionEvent,
    SessionHandle, SessionPurpose, SessionSpec,
};
pub use stream::StreamParser;
