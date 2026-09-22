//! Farik's agent runtime: a session is started, read, talked to, and stopped through one trait
//! (`docs/SPEC.md` section 8.2), whatever program or recording is behind it.

/// What a session is, what it reports, and the traits every runtime implements.
pub mod session;

pub use session::{
    EndReason, McpServerConfig, McpTransport, RuntimeAdapter, RuntimeError, SessionEvent,
    SessionHandle, SessionPurpose, SessionSpec,
};
