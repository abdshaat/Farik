//! The sites the Procurement Specialist may read (`docs/SPEC.md` 6.10, ADR 0039), folded from the
//! four `site.` kinds of the project's log.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_protocol::event::{EventBody, EventKind, FarikEvent};

use crate::{EventLog, EventQuery, StoreError};

/// Whether `event` was recorded by the owner: its envelope names no agent and no session, since
/// only the owner decides which sites the team may read.
fn is_the_owners(event: &FarikEvent) -> bool {
    event.envelope.ids.agent_id.is_none() && event.envelope.ids.session_id.is_none()
}

/// The sites the team's Procurement Specialist may read now: `farik`, the hosts of the running
/// release's own list, then each owner-recorded `site.approved` adding its host and each
/// `site.removed` taking one away, in sequence order. Farik's list is never copied into the log,
/// so a site a release adds is open on upgrade and one it drops closes unless the owner allowed
/// it, while a site the owner turned off stays off and one they allowed stays allowed. An event
/// that names an agent or a session decides nothing.
///
/// # Errors
///
/// What the log refused.
pub fn approved_sites(
    log: &EventLog,
    farik: &BTreeSet<String>,
) -> Result<BTreeSet<String>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::SiteApproved, EventKind::SiteRemoved],
        ..EventQuery::default()
    })?;
    let mut approved = farik.clone();
    for event in events.iter().filter(|event| is_the_owners(event)) {
        match &event.body {
            EventBody::SiteApproved(body) => {
                approved.insert(body.host.to_string());
            }
            EventBody::SiteRemoved(body) => {
                approved.remove(body.host.as_str());
            }
            _ => {}
        }
    }
    Ok(approved)
}

/// What the owner decided about a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteDecision {
    /// The owner allowed the site.
    Allowed,
    /// The owner did not allow it, with their own words when they said any.
    Declined {
        /// What the owner said, when they said anything.
        note: Option<String>,
    },
}

/// One request to read a site, as the log tells it: its `site.requested` and the owner's first
/// decision on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteRequest {
    /// The request's number, the seq of its `site.requested`.
    pub request: u64,
    /// The site, in its ASCII form.
    pub host: String,
    /// The first page the agent wanted, exactly as it wrote it.
    pub url: String,
    /// Why, in the agent's words.
    pub why: String,
    /// The task that waited for the answer.
    pub task_id: TaskId,
    /// The agent that asked.
    pub agent_id: String,
    /// When it asked.
    pub at: DateTime<Utc>,
    /// The owner's decision, when they made one.
    pub decision: Option<SiteDecision>,
}

/// Every request the log holds, oldest first. A decision counts only when its envelope names no
/// agent and no session, and the first decision on a request is the only one.
///
/// # Errors
///
/// What the log refused.
pub fn site_requests(log: &EventLog) -> Result<Vec<SiteRequest>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::SiteRequested,
            EventKind::SiteApproved,
            EventKind::SiteDeclined,
        ],
        ..EventQuery::default()
    })?;
    let mut requests: Vec<SiteRequest> = Vec::new();
    let decide = |requests: &mut Vec<SiteRequest>, request: u64, decision: SiteDecision| {
        if let Some(asked) = requests
            .iter_mut()
            .find(|asked| asked.request == request && asked.decision.is_none())
        {
            asked.decision = Some(decision);
        }
    };
    for event in &events {
        match &event.body {
            EventBody::SiteRequested(body) => {
                if let Some(task_id) = event.envelope.ids.task_id.clone() {
                    requests.push(SiteRequest {
                        request: event.envelope.seq,
                        host: body.host.to_string(),
                        url: body.url.to_string(),
                        why: body.why.to_string(),
                        task_id,
                        agent_id: event.envelope.ids.agent_id.clone().unwrap_or_default(),
                        at: event.envelope.recorded_at,
                        decision: None,
                    });
                }
            }
            EventBody::SiteApproved(body) if is_the_owners(event) => {
                if let Some(request) = body.request {
                    decide(&mut requests, request.get(), SiteDecision::Allowed);
                }
            }
            EventBody::SiteDeclined(body) if is_the_owners(event) => decide(
                &mut requests,
                body.request.get(),
                SiteDecision::Declined {
                    note: Some(body.note.to_string()).filter(|note| !note.is_empty()),
                },
            ),
            _ => {}
        }
    }
    Ok(requests)
}

/// A site the owner did not allow for a task, with their own words when they said any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclinedSite {
    /// The site, in its ASCII form.
    pub host: String,
    /// What the owner said, when they said anything.
    pub note: Option<String>,
}

/// The sites the owner did not allow for `task`, oldest decision first: its owner-recorded
/// `site.declined` of a host, unless a later owner-recorded `site.approved` of that host, for any
/// task or for none, allowed it after all. A later decline of the same host replaces the earlier.
///
/// # Errors
///
/// What the log refused.
pub fn declined_sites(log: &EventLog, task: &TaskId) -> Result<Vec<DeclinedSite>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::SiteDeclined, EventKind::SiteApproved],
        ..EventQuery::default()
    })?;
    let mut declined: Vec<DeclinedSite> = Vec::new();
    for event in events.iter().filter(|event| is_the_owners(event)) {
        match &event.body {
            EventBody::SiteDeclined(body) if event.envelope.ids.task_id.as_ref() == Some(task) => {
                declined.retain(|site| site.host != body.host.as_str());
                declined.push(DeclinedSite {
                    host: body.host.to_string(),
                    note: Some(body.note.to_string()).filter(|note| !note.is_empty()),
                });
            }
            EventBody::SiteApproved(body) => {
                declined.retain(|site| site.host != body.host.as_str());
            }
            _ => {}
        }
    }
    Ok(declined)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::approved_sites;
    use crate::waiting::fixtures::{Board, at};

    fn set(hosts: &[&str]) -> BTreeSet<String> {
        hosts.iter().map(ToString::to_string).collect()
    }

    /// The owner approves or removes `host`: an event with no agent and no session.
    fn owner(board: &Board, minute: u32, kind: &str, host: &str) {
        board.put(at(10, minute), None, None, kind, json!({ "host": host }));
    }

    #[test]
    fn the_approved_sites_are_the_log_s() {
        let board = Board::new("sites-fold");
        let farik = set(&["f.com"]);
        assert_eq!(approved_sites(&board.log, &farik), Ok(set(&["f.com"])));

        owner(&board, 1, "site.approved", "a.com");
        owner(&board, 2, "site.approved", "b.com");
        owner(&board, 3, "site.removed", "a.com");
        owner(&board, 4, "site.removed", "f.com");
        assert_eq!(approved_sites(&board.log, &farik), Ok(set(&["b.com"])));

        owner(&board, 5, "site.approved", "a.com");
        owner(&board, 6, "site.approved", "f.com");
        assert_eq!(
            approved_sites(&board.log, &farik),
            Ok(set(&["a.com", "b.com", "f.com"]))
        );

        // An approval that names an agent adds nothing, and a removal that names a session
        // removes nothing: only the owner decides.
        board.put(
            at(10, 7),
            None,
            Some("kai"),
            "site.approved",
            json!({ "host": "evil.com" }),
        );
        board.put_with(
            at(10, 8),
            None,
            None,
            Some("session-1"),
            "site.removed",
            json!({ "host": "b.com" }),
        );
        board.session(
            at(10, 9),
            None,
            "kai",
            "session-1",
            "site.removed",
            json!({ "host": "a.com" }),
        );
        assert_eq!(
            approved_sites(&board.log, &farik),
            Ok(set(&["a.com", "b.com", "f.com"]))
        );
    }

    #[test]
    fn a_new_farik_site_arrives_and_a_turn_off_stays() {
        let board = Board::new("sites-release");
        owner(&board, 1, "site.removed", "f.com");
        owner(&board, 2, "site.approved", "o.com");

        // The next release adds h.com and keeps g.com: the new one is open, the turned-off one
        // is still off.
        assert_eq!(
            approved_sites(&board.log, &set(&["f.com", "g.com", "h.com"])),
            Ok(set(&["g.com", "h.com", "o.com"]))
        );
        // A release that drops g.com closes it, and the owner's own stays.
        assert_eq!(
            approved_sites(&board.log, &set(&["f.com", "h.com"])),
            Ok(set(&["h.com", "o.com"]))
        );
    }
}
