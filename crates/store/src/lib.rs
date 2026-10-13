//! Catervas's memory: the append-only event log, the projections read from it, and the files under
//! `.catervas/` (`docs/SPEC.md` sections 5.1 and 8.4).

/// What each agent does, and what moved.
pub mod activity;
/// What the log says about autopilot (ADR 0082).
pub mod autopilot;
/// The copy of a private folder a task is judged against.
pub mod baseline;
/// The branch names a new branch may not take.
pub mod branches;
/// Reading Claude Code's transcripts into the machine index, and placing each session in its project.
pub mod claude_history;
/// A task's diff, and an epic's.
pub mod diff;
/// Where the team's documents are read from.
pub mod document_source;
/// What the store refuses, and why.
pub mod error;
/// The event log.
pub mod event_log;
/// The files under `.catervas/`.
pub mod files;
/// The documents agents write and propose in their folders, and the folder changes that carry them.
pub mod folder_docs;
/// The repository Catervas works in.
pub mod git;
/// The team's documents kept on this computer.
pub mod local_documents;
/// Finding the owner's jobs in their repositories, on this computer and in Claude Code's records.
pub mod jobs;
/// The machine-level index of the owner's Claude Code history, apart from every project's log.
pub mod machine;
/// The marketing plans the log holds.
pub mod marketing;
/// Whether Catervas may merge its pull requests on this computer.
pub mod merging;
/// The harness metrics, from the projections.
pub mod metrics;
/// The database's shape, as SQL applied in order.
pub mod migrations;
/// The data pipelines the log holds.
pub mod pipelines;
/// Whether the project was started from a paragraph, and which task drafts its product plan.
pub mod product_plan;
/// The board, derived from the log.
pub mod projections;
/// The purchase orders the log holds.
pub mod purchase_orders;
/// Where the files and the log disagree.
pub mod reconcile;
/// The renewals Catervas flagged for the owner.
pub mod renewals;
/// A project folder's member repositories on the file system.
pub mod repos;
/// Filing a request, for every caller.
pub mod requests;
/// What the repository says it is.
pub mod scan;
/// The messages to sellers and the replies the log holds.
pub mod seller_mail;
/// What waits on the human.
pub mod waiting;

pub use error::StoreError;
pub use event_log::{EventLog, EventQuery, IN_MEMORY, open_event_log};
pub use git::{CommitOutcome, Git, GitError, HeadSummary, MergeOutcome, Uncommitted};
pub use metrics::{CostSplit, HarnessMetrics, MetricsError};
pub use projections::{
    CostProjection, CostScope, CostWindow, Projections, TaskProjection, counted_status,
    open_projections,
};
pub use reconcile::{Drift, ReconcileError, reconcile};
pub use scan::{
    AppStart, CATERVAS_ACTOR, LIBRARY_CAP, MembersScan, MembersScanError, ProjectScan, ScanError,
    ScanFacts, material, names_of, project_document, renamed, rescan_members, reseeded,
    scan_project, seeded_library,
};
