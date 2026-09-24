//! Farik's memory: the append-only event log, the projections read from it, and the files under
//! `.farik/` (`docs/SPEC.md` sections 5.1 and 8.4).

/// What the store refuses, and why.
pub mod error;
/// The event log.
pub mod event_log;
/// The files under `.farik/`.
pub mod files;
/// The repository Farik works in.
pub mod git;
/// The harness metrics, from the projections.
pub mod metrics;
/// The database's shape, as SQL applied in order.
pub mod migrations;
/// The board, derived from the log.
pub mod projections;
/// Where the files and the log disagree.
pub mod reconcile;
/// Filing a request, for every caller.
pub mod requests;
/// What the repository says it is.
pub mod scan;

pub use error::StoreError;
pub use event_log::{EventLog, EventQuery, IN_MEMORY, open_event_log};
pub use git::{Git, GitError, HeadSummary, MergeOutcome};
pub use metrics::{CostSplit, HarnessMetrics, MetricsError};
pub use projections::{
    CostProjection, CostScope, Projections, SprintProjection, TaskProjection, open_projections,
};
pub use reconcile::{Drift, ReconcileError, reconcile};
pub use scan::{
    ProjectScan, ScanError, material, names_of, project_document, scan_project, seeded_library,
};
