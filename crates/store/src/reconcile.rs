//! Where the files and the log disagree (`docs/SPEC.md` section 8.4).
//!
//! The log is the source of truth for what happened; the files under `.farik/` are the source of
//! truth for what the team knows. Two sources of truth about one project can come apart — a process
//! stopped between a write and an append, a person edited a contract by hand, a file was restored
//! from a backup — and this says where, without picking a side. Nothing here writes anything: a
//! person decides what to do about a disagreement, and `farik doctor` is where they are told of one.
//!
//! Only what the log is authoritative about is a disagreement. A contract's `status` and its `locked`
//! flag are both moved by events (5.2, 5.11), so a file that says something else is a file to fix.
//! Its title, kind, risk and parent are the contract's own, and the board's copy of them is a cache
//! of the last event that mentioned the task — one that has fallen behind is a projection to rebuild,
//! not a disagreement about the project.

use std::fmt;

use farik_core::contract::TaskId;

use crate::error::StoreError;
use crate::files::{FilesError, ProjectFiles};
use crate::projections::Projections;

/// One disagreement between the files and the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drift {
    /// There is a contract file and the log has never heard of the task. Nothing created it, so no
    /// transition of it was ever governed.
    ContractWithoutEvents {
        /// Which task.
        task_id: TaskId,
        /// What is missing, in words a person can act on.
        detail: String,
    },
    /// The log knows the task and there is no contract file. Every session is assembled from the
    /// contract, so a task on the board with no contract cannot be worked on.
    EventsWithoutContract {
        /// Which task.
        task_id: TaskId,
        /// What is missing.
        detail: String,
    },
    /// The file and the log disagree about where the task is in the lifecycle of 5.2.
    StatusMismatch {
        /// Which task.
        task_id: TaskId,
        /// Both answers, the log's first.
        detail: String,
    },
    /// The file and the log disagree about whether the human is holding the contract (5.11).
    LockMismatch {
        /// Which task.
        task_id: TaskId,
        /// Both answers, the log's first.
        detail: String,
    },
    /// A contract file with that id was listed and could not then be read as a contract: broken by
    /// hand, unreadable to this user, or — if it went away between the listing and the read — no
    /// longer there at all. Which of those it was is in `detail`, in the file adapter's own words,
    /// because no test can win that race and a branch nothing can reach is worse than one variant
    /// whose detail tells the truth.
    ///
    /// Reported here rather than refused, so that one file a person broke does not hide every other
    /// disagreement.
    ContractUnreadable {
        /// Which task.
        task_id: TaskId,
        /// Why it could not be read, in the words the file adapter used.
        detail: String,
    },
}

impl Drift {
    /// Which task this is about.
    #[must_use]
    pub fn task_id(&self) -> &TaskId {
        match self {
            Self::ContractWithoutEvents { task_id, .. }
            | Self::EventsWithoutContract { task_id, .. }
            | Self::StatusMismatch { task_id, .. }
            | Self::LockMismatch { task_id, .. }
            | Self::ContractUnreadable { task_id, .. } => task_id,
        }
    }

    /// What is wrong, in words a person can act on.
    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::ContractWithoutEvents { detail, .. }
            | Self::EventsWithoutContract { detail, .. }
            | Self::StatusMismatch { detail, .. }
            | Self::LockMismatch { detail, .. }
            | Self::ContractUnreadable { detail, .. } => detail,
        }
    }
}

impl fmt::Display for Drift {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.task_id().as_str(), self.detail())
    }
}

/// Why the files and the log could not be compared at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileError {
    /// The files could not be listed.
    Files {
        /// What the file adapter said.
        detail: String,
    },
    /// The board could not be read.
    Store {
        /// What the store said.
        detail: String,
    },
}

impl fmt::Display for ReconcileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Files { detail } => write!(formatter, "the files could not be read: {detail}"),
            Self::Store { detail } => write!(formatter, "the board could not be read: {detail}"),
        }
    }
}

impl std::error::Error for ReconcileError {}

impl From<FilesError> for ReconcileError {
    fn from(error: FilesError) -> Self {
        Self::Files {
            detail: error.to_string(),
        }
    }
}

impl From<StoreError> for ReconcileError {
    fn from(error: StoreError) -> Self {
        Self::Store {
            detail: error.to_string(),
        }
    }
}

/// Every disagreement between the contracts on disk and what the log says of them.
///
/// The answer is ordered by the number in the task id, so that the tenth task does not come before
/// the ninth, and the sort is stable, so two drifts about one task keep the order they were found in:
/// a status before a lock. Two runs over one project therefore print the same thing and a person can
/// diff them.
///
/// What the log says is read from the projections rather than from the log itself, so this leans on
/// the handle being caught up. `open_projections` catches up when it opens, and a command that opens
/// them per run therefore always is; a handle held across appends without `apply` would report its
/// own lag as a `StatusMismatch`.
///
/// # Errors
///
/// `Files` when the contracts cannot be listed, `Store` when the board cannot be read. A single
/// contract that cannot be read is a `Drift`, not an error: one file a person broke does not hide
/// every other disagreement.
pub fn reconcile(
    files: &ProjectFiles,
    projections: &Projections,
) -> Result<Vec<Drift>, ReconcileError> {
    let on_disk = files.list_contracts()?;
    let board = projections.board()?;
    let mut found: Vec<Drift> = Vec::new();

    for task_id in &on_disk {
        if board.iter().any(|row| row.task_id == *task_id) {
            continue;
        }
        found.push(Drift::ContractWithoutEvents {
            task_id: task_id.clone(),
            detail: "there is a contract file and the log has never heard of this task, so no \
                     transition of it was ever governed"
                .to_string(),
        });
    }

    for row in &board {
        if on_disk.contains(&row.task_id) {
            continue;
        }
        found.push(Drift::EventsWithoutContract {
            task_id: row.task_id.clone(),
            detail: format!(
                "the log has this task at {} and there is no contract file for it, so no session \
                 can be assembled for it",
                row.status
            ),
        });
    }

    for row in &board {
        if !on_disk.contains(&row.task_id) {
            continue;
        }
        let contract = match files.read_contract(&row.task_id) {
            Ok(contract) => contract,
            Err(error) => {
                found.push(Drift::ContractUnreadable {
                    task_id: row.task_id.clone(),
                    detail: error.to_string(),
                });
                continue;
            }
        };
        if contract.status != row.status {
            found.push(Drift::StatusMismatch {
                task_id: row.task_id.clone(),
                detail: format!(
                    "the log has it at {} and the file says {}",
                    row.status, contract.status
                ),
            });
        }
        if contract.locked != row.locked {
            found.push(Drift::LockMismatch {
                task_id: row.task_id.clone(),
                detail: format!(
                    "the log has it {} and the file says {}",
                    held(row.locked),
                    held(contract.locked)
                ),
            });
        }
    }

    // Stable, so what the loops above found about one task stays in the order they found it. There
    // is no second ranking to disagree with that one.
    found.sort_by_key(|drift| number_in(drift.task_id()));
    Ok(found)
}

/// Whether the human is holding the contract, in words rather than in a boolean.
fn held(locked: bool) -> &'static str {
    if locked {
        "held by the human"
    } else {
        "not held"
    }
}

/// The number in a task id, for ordering, so that the tenth task does not come before the ninth.
///
/// The parse cannot fail: a `TaskId` is `FRK-` and one to six digits, which is what let it be built.
fn number_in(task_id: &TaskId) -> u64 {
    task_id
        .as_str()
        .trim_start_matches("FRK-")
        .parse()
        .unwrap_or_default()
}
