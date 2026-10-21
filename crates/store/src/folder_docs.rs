//! The documents agents write and propose in their folders (`docs/SPEC.md` 5.7 and 5.17) and the
//! folder changes that carry them (5.14), folded from the seven `folder_doc.` and `folder_change.`
//! kinds of the project's log. A decision, an integration and an escalation count only from an
//! envelope that names no agent and no session, so that no agent can make any of them happen by
//! recording it.

use std::collections::BTreeMap;

use catervas_core::folders::{SwitchState, agent_twin, twin_is_stale_by_log};
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind, LocalDocumentsWrittenBodyBy};
use catervas_protocol::generated::event::TeamDecidedBodyChoice;
use chrono::{DateTime, Utc};

use crate::{EventLog, EventQuery, StoreError};

/// Where a proposal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Waiting for the owner.
    Pending,
    /// A newer proposal of the same path replaced it before the owner decided.
    Superseded,
    /// The owner approved it.
    Approved,
    /// The owner sent it back.
    Returned,
}

/// One proposal to change a document the owner approves, as the log tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDocProposal {
    /// Its number: the seq of its event.
    pub proposal: u64,
    /// The human document.
    pub path: String,
    /// The version for people. Untrusted.
    pub text: String,
    /// The `.agent.md` version. Untrusted.
    pub agent_text: String,
    /// What changed and why, for the owner. Untrusted.
    pub summary: String,
    /// The sprint whose review proposed it.
    pub sprint_id: String,
    /// The agent that proposed it.
    pub agent_id: String,
    /// When it was proposed.
    pub at: DateTime<Utc>,
    /// Where it stands.
    pub state: ProposalState,
}

/// One decision of the owner's, in log order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDocDecision {
    /// The seq of its event.
    pub seq: u64,
    /// Approved, or sent back.
    pub approved: bool,
    /// The proposals it names.
    pub proposals: Vec<u64>,
    /// The owner's note or reason; empty for none.
    pub words: String,
}

/// One folder change: a commit of folder documents on a branch of its own.
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent facts the log tells of a change, each its own event"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChange {
    /// Its number, the name `catervas integrate folder-<n>` takes.
    pub change: u64,
    /// The branch it was made on: the event's `branch`, else `docs/folder-<change>` (ADR 0066).
    pub branch: String,
    /// The files it holds.
    pub paths: Vec<String>,
    /// Whether it carries the owner's approval.
    pub approved: bool,
    /// The proposals the owner approved in it; empty for a write.
    pub proposals: Vec<u64>,
    /// The agent that wrote it; `None` for an approval.
    pub written_by: Option<String>,
    /// Whether it reached the integration branch.
    pub integrated: bool,
    /// Whether Catervas gave its integration to the human.
    pub escalated: bool,
    /// Whether the owner made it, by saving a document on the Files page.
    pub edited: bool,
    /// Whether it moves the team's documents (ADR 0074): `Some(true)` out of the repository,
    /// `Some(false)` back into it, `None` for any other change.
    pub storage: Option<bool>,
}

/// Everything the log holds of folder documents.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderDocs {
    /// Every proposal, oldest first.
    pub proposals: Vec<FolderDocProposal>,
    /// Every decision of the owner's, in log order.
    pub decisions: Vec<FolderDocDecision>,
    /// Every folder change, in the order they were made.
    pub changes: Vec<FolderChange>,
}

/// Whether `event` was recorded by the owner or by Catervas: its envelope names no agent and no
/// session.
fn is_unattended(event: &CatervasEvent) -> bool {
    event.envelope.ids.agent_id.is_none() && event.envelope.ids.session_id.is_none()
}

fn settle(docs: &mut FolderDocs, proposals: &[u64], state: ProposalState) {
    for record in &mut docs.proposals {
        if proposals.contains(&record.proposal)
            && matches!(
                record.state,
                ProposalState::Pending | ProposalState::Superseded
            )
        {
            record.state = state;
        }
    }
}

fn add_change(
    docs: &mut FolderDocs,
    change: u64,
    branch: Option<String>,
    paths: Vec<String>,
    by: Result<Vec<u64>, String>,
) {
    if docs.changes.iter().all(|known| known.change != change) {
        docs.changes.push(FolderChange {
            change,
            branch: branch.unwrap_or_else(|| format!("docs/folder-{change}")),
            paths,
            approved: by.is_ok(),
            proposals: by.clone().unwrap_or_default(),
            written_by: by.err(),
            integrated: false,
            escalated: false,
            edited: false,
            storage: None,
        });
    }
}

fn change_of(docs: &mut FolderDocs, change: u64) -> Option<&mut FolderChange> {
    docs.changes.iter_mut().find(|known| known.change == change)
}

/// Every proposal, decision and folder change the log holds. A proposal is settled by the first
/// decision of the owner's that names it, and a newer proposal of its path replaces one that is
/// not settled. An approval's change holds each proposal's path and its twin.
///
/// # Errors
///
/// What the log refused.
#[allow(clippy::too_many_lines, reason = "one arm per kind of event")]
pub fn folder_docs(log: &EventLog) -> Result<FolderDocs, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::FolderDocWritten,
            EventKind::FolderDocProposed,
            EventKind::FolderDocApproved,
            EventKind::FolderDocReturned,
            EventKind::FolderDocEdited,
            EventKind::DocumentStorageChanged,
            EventKind::FolderChangeIntegrated,
            EventKind::FolderChangeEscalated,
            EventKind::TeamDecided,
        ],
        ..EventQuery::default()
    })?;
    let mut docs = FolderDocs::default();
    for event in &events {
        let seq = event.envelope.seq;
        match &event.body {
            EventBody::FolderDocProposed(body) => {
                let path = body.path.to_string();
                for older in &mut docs.proposals {
                    if older.path == path && older.state == ProposalState::Pending {
                        older.state = ProposalState::Superseded;
                    }
                }
                docs.proposals.push(FolderDocProposal {
                    proposal: seq,
                    path,
                    text: body.text.to_string(),
                    agent_text: body.agent_text.to_string(),
                    summary: body.summary.to_string(),
                    sprint_id: body.sprint_id.to_string(),
                    agent_id: body.proposed_by.clone(),
                    at: event.envelope.recorded_at,
                    state: ProposalState::Pending,
                });
            }
            EventBody::FolderDocWritten(body) => {
                add_change(
                    &mut docs,
                    body.change.get(),
                    body.branch.as_ref().map(|branch| branch.to_string()),
                    vec![body.path.to_string()],
                    Err(body.written_by.clone()),
                );
            }
            EventBody::FolderDocEdited(body) if is_unattended(event) => {
                let change = body.change.get();
                if change_of(&mut docs, change).is_none() {
                    docs.changes.push(FolderChange {
                        change,
                        branch: body.branch.as_ref().map_or_else(
                            || format!("docs/folder-{change}"),
                            |branch| branch.to_string(),
                        ),
                        paths: vec![body.path.to_string()],
                        approved: false,
                        proposals: Vec::new(),
                        written_by: None,
                        integrated: false,
                        escalated: false,
                        edited: true,
                        storage: None,
                    });
                }
            }
            EventBody::DocumentStorageChanged(body) if is_unattended(event) => {
                if let Some(change) = &body.change {
                    let change = change.get();
                    if change_of(&mut docs, change).is_none() {
                        docs.changes.push(FolderChange {
                            change,
                            branch: body.branch.as_ref().map_or_else(
                                || format!("docs/folder-{change}"),
                                |branch| branch.as_str().to_string(),
                            ),
                            paths: Vec::new(),
                            approved: false,
                            proposals: Vec::new(),
                            written_by: None,
                            integrated: false,
                            escalated: false,
                            edited: false,
                            storage: Some(body.local),
                        });
                    }
                }
            }
            EventBody::FolderDocApproved(body) if is_unattended(event) => {
                let proposals: Vec<u64> = body.proposals.iter().map(|n| n.get()).collect();
                docs.decisions.push(FolderDocDecision {
                    seq,
                    approved: true,
                    proposals: proposals.clone(),
                    words: body
                        .note
                        .as_ref()
                        .map(|note| note.as_str().to_string())
                        .unwrap_or_default(),
                });
                if let Some(change) = &body.change {
                    let mut paths: Vec<String> = Vec::new();
                    for record in docs
                        .proposals
                        .iter()
                        .filter(|p| proposals.contains(&p.proposal))
                    {
                        for path in
                            std::iter::once(record.path.clone()).chain(agent_twin(&record.path))
                        {
                            if !paths.contains(&path) {
                                paths.push(path);
                            }
                        }
                    }
                    add_change(
                        &mut docs,
                        change.get(),
                        body.branch.as_ref().map(|branch| branch.to_string()),
                        paths,
                        Ok(proposals.clone()),
                    );
                }
                settle(&mut docs, &proposals, ProposalState::Approved);
            }
            EventBody::FolderDocReturned(body) if is_unattended(event) => {
                let proposals: Vec<u64> = body.proposals.iter().map(|n| n.get()).collect();
                docs.decisions.push(FolderDocDecision {
                    seq,
                    approved: false,
                    proposals: proposals.clone(),
                    words: body.reason.to_string(),
                });
                settle(&mut docs, &proposals, ProposalState::Returned);
            }
            EventBody::FolderChangeIntegrated(body) if is_unattended(event) => {
                if let Some(change) = change_of(&mut docs, body.change.get()) {
                    change.integrated = true;
                }
            }
            EventBody::FolderChangeEscalated(body) if is_unattended(event) => {
                if let Some(change) = change_of(&mut docs, body.change.get()) {
                    change.escalated = true;
                }
            }
            // The Product Manager's decision to try a failed change again (ADR 0082 decision 11).
            EventBody::TeamDecided(body)
                if body.choice == TeamDecidedBodyChoice::RetryFolderChange =>
            {
                if let Some(change) = body
                    .folder_change
                    .as_ref()
                    .and_then(|number| change_of(&mut docs, number.get()))
                {
                    change.escalated = false;
                }
            }
            _ => {}
        }
    }
    Ok(docs)
}

/// The newest switch of where the documents are kept, as the log tells it (ADR 0074): `None` when
/// the owner never switched. It is waiting while its folder change is neither integrated nor
/// escalated; a switch with no change (nothing to move) never waits.
///
/// # Errors
///
/// What the log refused.
pub fn switch_state(docs: &FolderDocs, log: &EventLog) -> Result<Option<SwitchState>, StoreError> {
    let newest = log.read(&EventQuery {
        kinds: vec![EventKind::DocumentStorageChanged],
        newest_first: true,
        ..EventQuery::default()
    })?;
    Ok(newest
        .iter()
        .find(|event| is_unattended(event))
        .and_then(|event| match &event.body {
            EventBody::DocumentStorageChanged(body) => Some(SwitchState {
                local: body.local,
                waiting: body.change.as_ref().is_some_and(|change| {
                    docs.changes.iter().any(|held| {
                        held.change == change.get() && !held.integrated && !held.escalated
                    })
                }),
            }),
            _ => None,
        }))
}

/// Who wrote a document kept on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalWriter {
    /// The owner, by saving it on the Files page.
    Owner,
    /// An agent, through `catervas_write_folder_doc`.
    Agent,
    /// A task, whose documents landed when it was accepted.
    Task,
}

/// The newest write of a document kept on this computer, as the log tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalWrite {
    /// The seq of its event.
    pub seq: i64,
    /// When it was written.
    pub at: DateTime<Utc>,
    /// Who wrote it.
    pub by: LocalWriter,
    /// The agent that wrote it, for `Agent`.
    pub written_by: Option<String>,
    /// The task whose landing wrote it, for `Task`.
    pub task_id: Option<String>,
}

/// Each path a `local_documents.written` names, written or removed, with the newest write of it.
///
/// # Errors
///
/// What the log refused.
pub fn local_writes(log: &EventLog) -> Result<BTreeMap<String, LocalWrite>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::LocalDocumentsWritten],
        ..EventQuery::default()
    })?;
    let mut writes = BTreeMap::new();
    for event in &events {
        if let EventBody::LocalDocumentsWritten(body) = &event.body {
            let by = match body.by {
                LocalDocumentsWrittenBodyBy::Owner => LocalWriter::Owner,
                LocalDocumentsWrittenBodyBy::Agent => LocalWriter::Agent,
                LocalDocumentsWrittenBodyBy::Task => LocalWriter::Task,
            };
            for path in body.paths.iter().chain(body.removed.iter()) {
                writes.insert(
                    path.to_string(),
                    LocalWrite {
                        seq: i64::try_from(event.envelope.seq).unwrap_or(i64::MAX),
                        at: event.envelope.recorded_at,
                        by,
                        written_by: body.written_by.as_ref().map(|id| id.as_str().to_string()),
                        task_id: body.task_id.as_ref().map(|id| id.as_str().to_string()),
                    },
                );
            }
        }
    }
    Ok(writes)
}

/// The agent versions that are behind the document for people beside them, by the log of local
/// writes: the document was written after the twin did (`twin_is_stale_by_log`). A pair needs both
/// files on disk.
///
/// # Errors
///
/// What the log or the folder refuses.
pub fn local_stale_twins(
    log: &EventLog,
    root: &std::path::Path,
) -> Result<Vec<String>, StoreError> {
    let writes = local_writes(log)?;
    let on_disk = crate::local_documents::document_paths(root)?;
    let seq = |path: &str| writes.get(path).map(|write| write.seq);
    Ok(on_disk
        .iter()
        .filter(|path| catervas_core::folders::is_human_document(path))
        .filter_map(|path| {
            let twin = agent_twin(path).filter(|twin| on_disk.contains(twin))?;
            twin_is_stale_by_log(seq(path), seq(&twin)).then_some(twin)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{FolderChange, ProposalState, folder_docs};
    use crate::waiting::fixtures::{Board, at};

    const ROADMAP: &str = "docs/catervas/product/roadmap.md";
    const SPEC: &str = "docs/catervas/product/spec.md";

    /// `pm` proposes `path` in a review session of sprint S4.
    fn propose(board: &Board, minute: u32, path: &str) {
        board.session(
            at(10, minute),
            None,
            "pm",
            "session-1",
            "folder_doc.proposed",
            json!({
                "path": path, "text": "For people.", "agent_text": "For agents.",
                "summary": "Pie pre-orders are done.", "sprint_id": "S4", "proposed_by": "pm"
            }),
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one log read from proposal to integration"
    )]
    fn folds_each_proposal_and_change_to_where_it_stands() {
        let board = Board::new("folds-folder-docs");
        propose(&board, 1, ROADMAP);
        propose(&board, 2, SPEC);
        propose(&board, 3, ROADMAP);
        let docs = folder_docs(&board.log).expect("folds");
        let states: Vec<_> = docs
            .proposals
            .iter()
            .map(|p| (p.proposal, p.state))
            .collect();
        assert_eq!(
            states,
            [
                (1, ProposalState::Superseded),
                (2, ProposalState::Pending),
                (3, ProposalState::Pending)
            ]
        );
        let first = &docs.proposals[0];
        assert_eq!(
            (
                first.path.as_str(),
                first.text.as_str(),
                first.agent_text.as_str()
            ),
            (ROADMAP, "For people.", "For agents.")
        );
        assert_eq!(
            (
                first.summary.as_str(),
                first.sprint_id.as_str(),
                first.agent_id.as_str(),
                first.at
            ),
            ("Pie pre-orders are done.", "S4", "pm", at(10, 1))
        );

        board.put(
            at(10, 4),
            None,
            None,
            "folder_doc.approved",
            json!({ "proposals": [2], "change": 4, "sha": "abc" }),
        );
        // An approval an agent recorded changes nothing.
        board.session(
            at(10, 5),
            None,
            "pm",
            "session-1",
            "folder_doc.approved",
            json!({ "proposals": [3] }),
        );
        board.put(
            at(10, 6),
            None,
            None,
            "folder_doc.returned",
            json!({ "proposals": [3], "reason": "Keep gift cards in Now" }),
        );
        board.put(
            at(10, 7),
            None,
            Some("pm"),
            "folder_doc.written",
            json!({ "path": "docs/catervas/delivery/cadence.md", "written_by": "sm",
                    "change": 5, "sha": "def" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        let states: Vec<_> = docs.proposals.iter().map(|p| p.state).collect();
        assert_eq!(
            states,
            [
                ProposalState::Superseded,
                ProposalState::Approved,
                ProposalState::Returned
            ]
        );
        let decisions: Vec<_> = docs
            .decisions
            .iter()
            .map(|d| (d.approved, d.proposals.clone(), d.words.as_str()))
            .collect();
        assert_eq!(
            decisions,
            [
                (true, vec![2], ""),
                (false, vec![3], "Keep gift cards in Now")
            ]
        );
        assert!(docs.decisions[0].seq < docs.decisions[1].seq);

        // A later approval of a returned proposal leaves it returned.
        board.put(
            at(10, 8),
            None,
            None,
            "folder_doc.approved",
            json!({ "proposals": [3], "note": "Fine" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(docs.proposals[2].state, ProposalState::Returned);

        let changes: Vec<_> = docs
            .changes
            .iter()
            .map(|c| {
                (
                    c.change,
                    c.paths.clone(),
                    c.approved,
                    c.integrated,
                    c.escalated,
                )
            })
            .collect();
        assert_eq!(
            changes,
            [
                (
                    4,
                    vec![
                        SPEC.to_string(),
                        "docs/catervas/product/spec.agent.md".to_string()
                    ],
                    true,
                    false,
                    false
                ),
                (
                    5,
                    vec!["docs/catervas/delivery/cadence.md".to_string()],
                    false,
                    false,
                    false
                )
            ]
        );
        board.put(
            at(10, 9),
            None,
            None,
            "folder_change.integrated",
            json!({ "change": 5, "sha": "x", "into": "main", "integrated_by": "governor" }),
        );
        board.put(
            at(10, 10),
            None,
            None,
            "folder_change.escalated",
            json!({ "change": 4, "detail": "conflict" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(
            docs.changes
                .iter()
                .map(|c| (c.integrated, c.escalated))
                .collect::<Vec<_>>(),
            [(false, true), (true, false)]
        );

        // An integration an agent recorded changes nothing.
        board.put(
            at(10, 11),
            None,
            Some("pm"),
            "folder_change.integrated",
            json!({ "change": 4, "sha": "y", "into": "main", "integrated_by": "governor" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert!(!docs.changes[0].integrated);
    }

    #[test]
    fn folds_an_owners_edit_as_a_change() {
        let board = Board::new("folds-owners-edit");
        board.put(
            at(10, 1),
            None,
            None,
            "folder_doc.edited",
            json!({ "path": ROADMAP, "change": 2, "sha": "a1" }),
        );
        board.put(
            at(10, 2),
            None,
            Some("pm"),
            "folder_doc.written",
            json!({ "path": "docs/catervas/delivery/cadence.md", "written_by": "sm",
                    "change": 3, "sha": "b2" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(
            docs.changes[0],
            FolderChange {
                change: 2,
                branch: "docs/folder-2".to_string(),
                paths: vec![ROADMAP.to_string()],
                proposals: Vec::new(),
                written_by: None,
                approved: false,
                integrated: false,
                escalated: false,
                edited: true,
                storage: None,
            }
        );
        assert!(!docs.changes[1].edited);
        board.put(
            at(10, 3),
            None,
            None,
            "folder_change.integrated",
            json!({ "change": 2, "sha": "x", "into": "main", "integrated_by": "governor" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert!(docs.changes[0].integrated && docs.changes[0].edited);

        // An edit an agent recorded is not the owner's.
        board.put(
            at(10, 4),
            None,
            Some("pm"),
            "folder_doc.edited",
            json!({ "path": SPEC, "change": 4, "sha": "c3" }),
        );
        assert_eq!(folder_docs(&board.log).expect("folds").changes.len(), 2);

        // The projections pass over it.
        let projections = crate::open_projections(std::sync::Arc::clone(&board.log))
            .expect("the projections open");
        projections.board().expect("the board projects");
    }

    #[test]
    fn folds_the_switch_into_a_folder_change_and_the_local_writes_by_path() {
        use catervas_core::folders::SwitchState;

        use super::{LocalWriter, local_writes, switch_state};

        let board = Board::new("folds-the-switch");
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(switch_state(&docs, &board.log).expect("reads"), None);
        board.put(
            at(10, 1),
            None,
            None,
            "document_storage.changed",
            json!({ "local": true, "change": 4,
                    "branch": "docs/keep-documents-on-this-computer", "sha": "abc" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(docs.changes.len(), 1);
        let change = &docs.changes[0];
        assert_eq!(change.change, 4);
        assert_eq!(change.branch, "docs/keep-documents-on-this-computer");
        assert_eq!(change.storage, Some(true));
        assert!(!change.approved && !change.edited && change.written_by.is_none());
        assert_eq!(
            switch_state(&docs, &board.log).expect("reads"),
            Some(SwitchState {
                local: true,
                waiting: true
            })
        );
        board.put(
            at(10, 2),
            None,
            None,
            "folder_change.integrated",
            json!({ "change": 4, "sha": "x", "into": "main", "integrated_by": "human" }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(
            switch_state(&docs, &board.log).expect("reads"),
            Some(SwitchState {
                local: true,
                waiting: false
            })
        );
        // A switch an agent recorded counts for nothing.
        board.put(
            at(10, 3),
            None,
            Some("pm"),
            "document_storage.changed",
            json!({ "local": false, "change": null, "branch": null, "sha": null }),
        );
        let docs = folder_docs(&board.log).expect("folds");
        assert_eq!(docs.changes.len(), 1);
        assert!(
            switch_state(&docs, &board.log)
                .expect("reads")
                .expect("a switch")
                .local
        );

        board.put(
            at(10, 4),
            None,
            None,
            "local_documents.written",
            json!({ "paths": [SPEC], "removed": [], "by": "owner", "written_by": null,
                    "task_id": null, "set_aside": [] }),
        );
        board.put(
            at(10, 5),
            None,
            None,
            "local_documents.written",
            json!({ "paths": [SPEC, "docs/catervas/product/spec.agent.md"], "removed": [ROADMAP],
                    "by": "task", "written_by": null, "task_id": "CTV-3", "set_aside": [] }),
        );
        let writes = local_writes(&board.log).expect("reads");
        let spec = &writes[SPEC];
        assert_eq!(
            (spec.by, spec.task_id.as_deref()),
            (LocalWriter::Task, Some("CTV-3"))
        );
        assert_eq!(spec.at, at(10, 5));
        assert!(spec.seq > 0);
        assert_eq!(writes[ROADMAP].by, LocalWriter::Task);
        assert_eq!(writes.len(), 3);
    }
}
