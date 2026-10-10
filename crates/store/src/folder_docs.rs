//! The documents agents write and propose in their folders (`docs/SPEC.md` 5.7 and 5.17) and the
//! folder changes that carry them (5.14), folded from the seven `folder_doc.` and `folder_change.`
//! kinds of the project's log. A decision, an integration and an escalation count only from an
//! envelope that names no agent and no session, so that no agent can make any of them happen by
//! recording it.

use catervas_core::folders::agent_twin;
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChange {
    /// Its number (`docs/folder-<n>`).
    pub change: u64,
    /// The files it holds.
    pub paths: Vec<String>,
    /// Whether it carries the owner's approval.
    pub approved: bool,
    /// Whether it reached the integration branch.
    pub integrated: bool,
    /// Whether Catervas gave its integration to the human.
    pub escalated: bool,
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

fn add_change(docs: &mut FolderDocs, change: u64, paths: Vec<String>, approved: bool) {
    if docs.changes.iter().all(|known| known.change != change) {
        docs.changes.push(FolderChange {
            change,
            paths,
            approved,
            integrated: false,
            escalated: false,
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
pub fn folder_docs(log: &EventLog) -> Result<FolderDocs, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::FolderDocWritten,
            EventKind::FolderDocProposed,
            EventKind::FolderDocApproved,
            EventKind::FolderDocReturned,
            EventKind::FolderChangeIntegrated,
            EventKind::FolderChangeEscalated,
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
                    vec![body.path.to_string()],
                    false,
                );
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
                    add_change(&mut docs, change.get(), paths, true);
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
            _ => {}
        }
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ProposalState, folder_docs};
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
}
