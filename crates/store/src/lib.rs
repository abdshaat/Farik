//! Farik's memory: the append-only event log, the projections read from it, and the files under
//! `.farik/` (`docs/SPEC.md` sections 5.1 and 8.4).

/// What the store refuses, and why.
pub mod error;
/// The event log.
pub mod event_log;
/// The repository Farik works in.
pub mod git;
/// The database's shape, as SQL applied in order.
pub mod migrations;
/// The board, derived from the log.
pub mod projections;

pub use error::StoreError;
pub use event_log::{EventLog, EventQuery, IN_MEMORY, open_event_log};
pub use git::{Git, GitError};
pub use projections::{Projections, TaskProjection, open_projections};
