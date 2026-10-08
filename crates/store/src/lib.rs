//! Farik's memory: the append-only event log, the projections read from it, and the files under
//! `.farik/` (`docs/SPEC.md` sections 5.1 and 8.4).

/// What each agent does, and what moved.
pub mod activity;
/// The copy of a private folder a task is judged against.
pub mod baseline;
/// A task's diff, and an epic's.
pub mod diff;
/// What the store refuses, and why.
pub mod error;
/// The event log.
pub mod event_log;
/// The files under `.farik/`.
pub mod files;
/// The repository Farik works in.
pub mod git;
/// The marketing plans the log holds.
pub mod marketing;
/// The harness metrics, from the projections.
pub mod metrics;
/// The database's shape, as SQL applied in order.
pub mod migrations;
/// The data pipelines the log holds.
pub mod pipelines;
/// The board, derived from the log.
pub mod projections;
/// The purchase orders the log holds.
pub mod purchase_orders;
/// Where the files and the log disagree.
pub mod reconcile;
/// The renewals Farik flagged for the owner.
pub mod renewals;
/// Filing a request, for every caller.
pub mod requests;
/// What the repository says it is.
pub mod scan;
/// The sites the Procurement Specialist may read.
pub mod sites;
/// What waits on the human.
pub mod waiting;

pub use error::StoreError;
pub use event_log::{EventLog, EventQuery, IN_MEMORY, open_event_log};
pub use git::{Git, GitError, HeadSummary, MergeOutcome};
pub use metrics::{CostSplit, HarnessMetrics, MetricsError};
pub use projections::{
    CostProjection, CostScope, CostWindow, Projections, SprintProjection, TaskProjection,
    open_projections,
};
pub use reconcile::{Drift, ReconcileError, reconcile};
pub use scan::{
    ProjectScan, ScanError, ScanFacts, material, names_of, project_document, scan_project,
    seeded_library,
};
