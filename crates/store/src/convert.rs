//! What holds a one-repo project from becoming a project folder (`docs/SPEC.md` 4.4; phase 9 step
//! 07b): the work in progress that the move would leave in the wrong place.

use std::fmt;

use catervas_core::branch::task_branch;
use catervas_core::contract::{TaskId, TaskKind, TaskStatus};
use catervas_protocol::event::{EventBody, EventKind, TaskCreatedBodyFiledAs};

use crate::files::{FilesError, ProjectFiles};
use crate::folder_docs::folder_docs;
use crate::git::{Git, GitError};
use crate::{EventLog, EventQuery, Projections, StoreError};

/// What holds the move: the work a project folder would leave in the wrong place.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Blockers {
    /// The tasks in progress, in the order of the board.
    pub tasks: Vec<TaskId>,
    /// Whether a Catervafication of the one repository is still open.
    pub catervafication: bool,
    /// The folder changes not yet integrated or escalated.
    pub folder_changes: Vec<u64>,
}

impl Blockers {
    /// Whether nothing holds the move.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty() && !self.catervafication && self.folder_changes.is_empty()
    }
}

/// Why the blockers could not be read.
#[derive(Debug)]
pub enum ConvertError {
    /// The log or the board refused.
    Store(StoreError),
    /// A contract file refused.
    Files(FilesError),
    /// Git refused.
    Git(GitError),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "{error}"),
            Self::Files(error) => write!(formatter, "{error}"),
            Self::Git(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<StoreError> for ConvertError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<FilesError> for ConvertError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

impl From<GitError> for ConvertError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

/// What holds the move, for the query, the method and the move itself.
///
/// Counted: every task that is `assigned`, `in_progress`, `blocked`, `verifying` or `rejected`, or
/// `accepted` and awaiting integration; every `escalated` task whose branch is a local branch of
/// the repository; every task Catervas filed for the owner (`owner_change` or `document_rewrite`)
/// that is neither accepted nor cancelled; every folder change neither integrated nor escalated;
/// and an open Catervafication. An epic other than the Catervafication's is not counted: its
/// children carry the work.
///
/// # Errors
///
/// What the log, the board, a contract file or git refused.
pub fn convert_blockers(
    files: &ProjectFiles,
    log: &EventLog,
    projections: &Projections,
    git: &Git,
) -> Result<Blockers, ConvertError> {
    let filed_for_the_owner: Vec<TaskId> = log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            ..EventQuery::default()
        })?
        .into_iter()
        .filter_map(|event| match &event.body {
            EventBody::TaskCreated(body)
                if matches!(
                    body.filed_as,
                    Some(
                        TaskCreatedBodyFiledAs::OwnerChange
                            | TaskCreatedBodyFiledAs::DocumentRewrite
                    )
                ) =>
            {
                event.envelope.ids.task_id
            }
            _ => None,
        })
        .collect();
    let mut tasks = Vec::new();
    for row in projections.board()? {
        if row.kind == TaskKind::Epic {
            continue;
        }
        let holds = match row.status {
            TaskStatus::Assigned
            | TaskStatus::InProgress
            | TaskStatus::Blocked
            | TaskStatus::Verifying
            | TaskStatus::Rejected => true,
            TaskStatus::Accepted => row.awaiting_integration,
            TaskStatus::Escalated => match files.read_contract(&row.task_id) {
                Ok(contract) => git.has_local_branch(&task_branch(&contract))?,
                Err(_) => false,
            },
            _ => false,
        } || (filed_for_the_owner.contains(&row.task_id)
            && !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled));
        if holds {
            tasks.push(row.task_id);
        }
    }
    let catervafication = log
        .read(&EventQuery {
            kinds: vec![EventKind::CatervaficationStarted],
            limit: Some(1),
            ..EventQuery::default()
        })?
        .into_iter()
        .find_map(|event| event.envelope.ids.task_id)
        .map(|epic| projections.task(&epic))
        .transpose()?
        .flatten()
        .is_some_and(|row| !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled));
    let folder_changes = folder_docs(log)?
        .changes
        .into_iter()
        .filter(|change| !change.integrated && !change.escalated)
        .map(|change| change.change)
        .collect();
    Ok(Blockers {
        tasks,
        catervafication,
        folder_changes,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::convert_blockers;
    use crate::git::fixtures::TempRepo;
    use crate::waiting::fixtures::{Board, at};

    /// Files `task` as a draft and moves it to `status`.
    fn at_status(board: &Board, task: &str, status: &str) {
        board.file(task, &format!("Task {task}"), |_| {});
        board.moved(
            at(9, 1),
            task,
            ("draft", status),
            "human",
            (Some("linus"), None),
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_what_holds_the_move() {
        let board = Board::new("convert-blockers");
        let repository = TempRepo::new("convert-blockers");
        at_status(&board, "CTV-1", "in_progress");
        at_status(&board, "CTV-2", "rejected");
        at_status(&board, "CTV-3", "escalated");
        at_status(&board, "CTV-4", "escalated");
        at_status(&board, "CTV-5", "accepted");
        // A task filed for the owner by the Files page is counted in any status but a finished one.
        board.put(
            at(9, 0),
            Some("CTV-6"),
            None,
            "task.created",
            json!({ "created_by": "human", "filed_as": "document_rewrite", "summary": {
                "kind": "task", "title": "Rewrite as HTML", "status": "draft", "risk": "low"
            }}),
        );
        board.moved(at(9, 1), "CTV-6", ("draft", "ready"), "human", (None, None));
        // Not counted: ready, refining, a plain draft, and an accepted task already integrated.
        at_status(&board, "CTV-7", "ready");
        at_status(&board, "CTV-8", "refining");
        board.file("CTV-9", "A draft", |_| {});
        at_status(&board, "CTV-10", "accepted");
        board.put(
            at(9, 2),
            Some("CTV-10"),
            None,
            "task.integrated",
            json!({ "sha": "abc123", "into": "main", "integrated_by": "governor" }),
        );
        // CTV-3's branch is a local branch of the repository, and CTV-4's is not.
        let contract = board.files.read_contract(&"CTV-3".parse().expect("an id"));
        let branch = catervas_core::branch::task_branch(&contract.expect("a contract"));
        repository.git(&["branch", &branch]);
        // A folder change waiting for its integration, and one that reached the branch.
        board.put(
            at(9, 3),
            None,
            Some("grace"),
            "folder_doc.written",
            json!({ "path": "docs/catervas/architecture/overview.md", "written_by": "grace",
                    "change": 2, "sha": "def456" }),
        );
        board.put(
            at(9, 3),
            None,
            Some("grace"),
            "folder_doc.written",
            json!({ "path": "docs/catervas/architecture/map.md", "written_by": "grace",
                    "change": 3, "sha": "def457" }),
        );
        board.put(
            at(9, 4),
            None,
            None,
            "folder_change.integrated",
            json!({ "change": 3, "sha": "def458", "into": "main", "integrated_by": "governor" }),
        );
        // An open Catervafication.
        board.file("CTV-11", "Document the project", |wire| {
            wire["kind"] = json!("epic");
        });
        board.put(
            at(9, 5),
            Some("CTV-11"),
            None,
            "catervafication.started",
            json!({ "project": "existing" }),
        );
        board.moved(
            at(9, 6),
            "CTV-11",
            ("draft", "in_progress"),
            "human",
            (Some("ada"), None),
        );

        let blockers = convert_blockers(
            &board.files,
            &board.log,
            &board.projections,
            &repository.adapter(),
        )
        .expect("the blockers are read");
        let ids: Vec<&str> = blockers.tasks.iter().map(|id| id.as_str()).collect();
        assert_eq!(ids, ["CTV-1", "CTV-2", "CTV-3", "CTV-5", "CTV-6"]);
        assert_eq!(blockers.folder_changes, [2]);
        assert!(blockers.catervafication);
        assert!(!blockers.is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn holds_nothing_when_the_team_is_idle() {
        let board = Board::new("convert-blockers-idle");
        let repository = TempRepo::new("convert-blockers-idle");
        at_status(&board, "CTV-1", "ready");
        let blockers = convert_blockers(
            &board.files,
            &board.log,
            &board.projections,
            &repository.adapter(),
        )
        .expect("the blockers are read");
        assert!(blockers.is_empty());
        assert!(!blockers.catervafication);
    }
}
