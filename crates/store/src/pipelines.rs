//! The data pipelines the Procurement Specialist asks for (`docs/SPEC.md` 6.10, ADR 0039), folded
//! from the four `data_pipeline.` kinds of the project's log and the `session.started` events
//! that name a pipeline. A request is made by the agent; passed to the owner by the Product
//! Manager's decision session, or by Farik when that session did not decide; approved or declined
//! by the Product Manager from that session, or by the owner. Only a decision whose envelope fits
//! its `by` counts, so that no agent and no other session can approve, decline or escalate a
//! request.

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_core::pipeline::{PipelineCost, pipeline_needs_owner};
use farik_protocol::event::{
    DataPipelineCost, DataPipelineDecidedBy, DataPipelineRequestedBody, EventBody, EventKind,
    FarikEvent,
};

use crate::{EventLog, EventQuery, StoreError};

/// Where a request stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    /// The agent asked; the Product Manager has not decided or passed it on.
    Open,
    /// The request is the owner's to decide: the Product Manager passed it on, or did not decide.
    Escalated,
    /// Approved, and the team asked to set the source up.
    Approved,
    /// Declined.
    Declined,
}

impl PipelineState {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Escalated => "escalated",
            Self::Approved => "approved",
            Self::Declined => "declined",
        }
    }
}

/// Who decided a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecidedBy {
    /// The Product Manager, in the session that was asked to decide it.
    ProductManager,
    /// The owner.
    Human,
}

impl DecidedBy {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProductManager => "product_manager",
            Self::Human => "human",
        }
    }
}

impl From<DataPipelineDecidedBy> for DecidedBy {
    fn from(by: DataPipelineDecidedBy) -> Self {
        match by {
            DataPipelineDecidedBy::ProductManager => Self::ProductManager,
            DataPipelineDecidedBy::Human => Self::Human,
        }
    }
}

/// One data pipeline request, as the log tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineRecord {
    /// The request's number, the seq of its `data_pipeline.requested`.
    pub pipeline: u64,
    /// The task whose agent asked, and which went on.
    pub task_id: TaskId,
    /// The agent that asked.
    pub agent_id: String,
    /// What the agent wrote. Untrusted.
    pub requested: DataPipelineRequestedBody,
    /// When it asked.
    pub requested_at: DateTime<Utc>,
    /// Where it stands.
    pub state: PipelineState,
    /// The reason it was passed to the owner, when it was: the Product Manager's words, or Farik's
    /// sentence when `by_farik`.
    pub escalated_reason: Option<String>,
    /// Whether Farik passed it on after the Product Manager's tries, and not the Product Manager.
    pub by_farik: bool,
    /// Who decided it, once it is approved or declined.
    pub by: Option<DecidedBy>,
    /// The Product Manager's reason or the owner's note, when the decision had one.
    pub reason: Option<String>,
    /// The ordinary request an approval filed.
    pub request: Option<TaskId>,
    /// The sessions that were asked to decide it, oldest first.
    pub tries: Vec<String>,
}

/// A request's cost as the governor's rule takes it.
#[must_use]
pub fn cost_of(cost: DataPipelineCost) -> PipelineCost {
    match cost {
        DataPipelineCost::Free => PipelineCost::Free,
        DataPipelineCost::Paid => PipelineCost::Paid,
        DataPipelineCost::Unknown => PipelineCost::Unknown,
    }
}

/// Whether `event` was recorded by the owner or by Farik: its envelope names no agent and no
/// session.
fn is_unattended(event: &FarikEvent) -> bool {
    event.envelope.ids.agent_id.is_none() && event.envelope.ids.session_id.is_none()
}

/// Whether `event` was recorded in one of the sessions asked to decide `record`.
fn is_its_deciding_session(record: &PipelineRecord, event: &FarikEvent) -> bool {
    event
        .envelope
        .ids
        .session_id
        .as_ref()
        .is_some_and(|session| record.tries.contains(session))
}

/// Every request the log holds, oldest first. A request is open until a decision it takes: an
/// escalation counts from Farik (an envelope with no agent and no session) or from a session
/// asked to decide this request. A decision counts from the state it may happen in: the owner's
/// (`by: human`, an envelope with no agent and no session) only on an escalated request, and the
/// Product Manager's (`by: product_manager`, a session asked to decide this request) only on an
/// open one, an approval only when the request needs no owner (`pipeline_needs_owner`). The first
/// decision a request takes is the only one. A try is a `session.started` that names the request.
///
/// # Errors
///
/// What the log refused.
pub fn data_pipelines(log: &EventLog) -> Result<Vec<PipelineRecord>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::DataPipelineRequested,
            EventKind::DataPipelineEscalated,
            EventKind::DataPipelineApproved,
            EventKind::DataPipelineDeclined,
            EventKind::SessionStarted,
        ],
        ..EventQuery::default()
    })?;
    let mut records: Vec<PipelineRecord> = Vec::new();
    for event in &events {
        match &event.body {
            EventBody::DataPipelineRequested(body) => {
                if let Some(task_id) = event.envelope.ids.task_id.clone() {
                    records.push(PipelineRecord {
                        pipeline: event.envelope.seq,
                        task_id,
                        agent_id: event.envelope.ids.agent_id.clone().unwrap_or_default(),
                        requested: body.clone(),
                        requested_at: event.envelope.recorded_at,
                        state: PipelineState::Open,
                        escalated_reason: None,
                        by_farik: false,
                        by: None,
                        reason: None,
                        request: None,
                        tries: Vec::new(),
                    });
                }
            }
            EventBody::SessionStarted(body) => {
                let (Some(pipeline), Some(session)) = (
                    body.pipeline.as_ref().map(|number| number.get()),
                    event.envelope.ids.session_id.clone(),
                ) else {
                    continue;
                };
                if let Some(record) = records
                    .iter_mut()
                    .find(|record| record.pipeline == pipeline)
                {
                    record.tries.push(session);
                }
            }
            EventBody::DataPipelineEscalated(body) => {
                let Some(record) = records
                    .iter_mut()
                    .find(|record| record.pipeline == body.pipeline.get())
                else {
                    continue;
                };
                let by_farik = is_unattended(event);
                if record.state == PipelineState::Open
                    && (by_farik || is_its_deciding_session(record, event))
                {
                    record.state = PipelineState::Escalated;
                    record.escalated_reason = Some(body.reason.to_string());
                    record.by_farik = by_farik;
                }
            }
            EventBody::DataPipelineApproved(body) => decide(
                &mut records,
                event,
                (body.pipeline.get(), body.by.into()),
                (PipelineState::Approved, body.reason.as_str()),
                body.request.as_str().parse().ok(),
            ),
            EventBody::DataPipelineDeclined(body) => decide(
                &mut records,
                event,
                (body.pipeline.get(), body.by.into()),
                (PipelineState::Declined, body.reason.as_str()),
                None,
            ),
            _ => {}
        }
    }
    Ok(records)
}

/// Takes a decision about `pipeline` when it is the first and its envelope fits `by`.
fn decide(
    records: &mut [PipelineRecord],
    event: &FarikEvent,
    (pipeline, by): (u64, DecidedBy),
    (state, reason): (PipelineState, &str),
    request: Option<TaskId>,
) {
    let Some(record) = records
        .iter_mut()
        .find(|record| record.pipeline == pipeline)
    else {
        return;
    };
    // The owner decides an escalated request alone. The Product Manager decides an open one from
    // its deciding session, and approves it only when it needs no owner (spec 6.10): the tool
    // refuses the rest, and the fold does not count what the tool would have refused.
    let counts = match by {
        DecidedBy::Human => is_unattended(event) && record.state == PipelineState::Escalated,
        DecidedBy::ProductManager => {
            is_its_deciding_session(record, event)
                && record.state == PipelineState::Open
                && (state != PipelineState::Approved
                    || !pipeline_needs_owner(
                        cost_of(record.requested.cost),
                        record.requested.sends_project_data,
                    ))
        }
    };
    if counts {
        record.state = state;
        record.by = Some(by);
        record.reason = Some(reason.to_string()).filter(|reason| !reason.is_empty());
        record.request = request;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{DecidedBy, PipelineState, data_pipelines};
    use crate::waiting::fixtures::{Board, at};

    /// What the agent asks for in the story: Firecrawl, which costs money.
    fn firecrawl() -> Value {
        json!({
            "name": "Firecrawl",
            "what": "Prices as clean text from the seller pages the task has to compare.",
            "source_url": "https://www.firecrawl.dev/pricing",
            "why": "Three sellers hide their prices behind scripts the plain fetch cannot read.",
            "cost": "paid",
            "needs_account": true,
            "sends_project_data": false
        })
    }

    /// `proc`'s request for Firecrawl on FRK-1; answers its number.
    fn requested(board: &Board, minute: u32) -> u64 {
        requested_as(board, minute, firecrawl())
    }

    /// `proc`'s request on FRK-1 with `body`; answers its number.
    fn requested_as(board: &Board, minute: u32, body: Value) -> u64 {
        board
            .session(
                at(10, minute),
                Some("FRK-1"),
                "proc",
                "session-proc",
                "data_pipeline.requested",
                body,
            )
            .envelope
            .seq
    }

    /// A request the Product Manager may approve: free, no data out.
    fn free() -> Value {
        let mut body = firecrawl();
        body["cost"] = json!("free");
        body
    }

    /// A session of `ada` that started, deciding `pipeline` when it names one.
    fn started(board: &Board, minute: u32, session: &str, pipeline: Option<u64>) {
        let mut body = json!({
            "purpose": "verify", "model": "claude-opus-5-5", "effort": "high"
        });
        if let Some(pipeline) = pipeline {
            body["pipeline"] = json!(pipeline);
        }
        board.session(
            at(10, minute),
            None,
            "ada",
            session,
            "session.started",
            body,
        );
    }

    /// A decision of kind `kind` about `pipeline`, recorded by `agent` in `session` when named.
    fn decided(
        board: &Board,
        minute: u32,
        (agent, session): (Option<&str>, Option<&str>),
        kind: &str,
        pipeline: u64,
        by: &str,
    ) {
        let mut body = json!({ "pipeline": pipeline, "by": by, "reason": "Because." });
        if kind == "data_pipeline.approved" {
            body["request"] = json!("FRK-9");
        }
        board.put_with(at(10, minute), None, agent, session, kind, body);
    }

    fn only(board: &Board) -> super::PipelineRecord {
        let mut all = data_pipelines(&board.log).expect("the log reads");
        assert_eq!(all.len(), 1);
        all.remove(0)
    }

    #[test]
    fn folds_each_state() {
        let board = Board::new("pipelines-fold");
        let pipeline = requested(&board, 1);
        let open = only(&board);
        assert_eq!(open.pipeline, pipeline);
        assert_eq!(open.state, PipelineState::Open);
        assert_eq!(open.task_id.as_str(), "FRK-1");
        assert_eq!(open.agent_id, "proc");
        assert_eq!(open.requested.name.as_str(), "Firecrawl");
        assert_eq!(open.requested_at, at(10, 1));
        assert!(open.tries.is_empty());

        started(&board, 2, "session-1", Some(pipeline));
        // A session that names no pipeline is no try at it.
        started(&board, 3, "session-2", None);
        assert_eq!(only(&board).tries, vec!["session-1".to_string()]);

        board.session(
            at(10, 4),
            None,
            "ada",
            "session-1",
            "data_pipeline.escalated",
            json!({ "pipeline": pipeline, "reason": "It costs money, so it is yours to decide." }),
        );
        let escalated = only(&board);
        assert_eq!(escalated.state, PipelineState::Escalated);
        assert_eq!(
            escalated.escalated_reason.as_deref(),
            Some("It costs money, so it is yours to decide.")
        );
        assert!(!escalated.by_farik);

        decided(
            &board,
            5,
            (None, None),
            "data_pipeline.approved",
            pipeline,
            "human",
        );
        let approved = only(&board);
        assert_eq!(approved.state, PipelineState::Approved);
        assert_eq!(approved.by, Some(DecidedBy::Human));
        assert_eq!(approved.reason.as_deref(), Some("Because."));
        assert_eq!(
            approved.request.as_ref().map(|id| id.as_str()),
            Some("FRK-9")
        );
        assert_eq!(approved.tries, vec!["session-1".to_string()]);
        assert_eq!(
            approved.escalated_reason.as_deref(),
            Some("It costs money, so it is yours to decide.")
        );

        // The first decision is the decision.
        decided(
            &board,
            6,
            (None, None),
            "data_pipeline.declined",
            pipeline,
            "human",
        );
        decided(
            &board,
            7,
            (Some("ada"), Some("session-1")),
            "data_pipeline.declined",
            pipeline,
            "product_manager",
        );
        // Nor does a late escalation, from Farik or from the deciding session, take it back.
        board.put(
            at(10, 8),
            None,
            None,
            "data_pipeline.escalated",
            json!({ "pipeline": pipeline, "reason": "The Product Manager did not decide" }),
        );
        board.session(
            at(10, 9),
            None,
            "ada",
            "session-1",
            "data_pipeline.escalated",
            json!({ "pipeline": pipeline, "reason": "Too late to pass on." }),
        );
        assert_eq!(only(&board), approved);
    }

    #[test]
    fn the_product_manager_decides_from_its_session_and_farik_escalates_on_none() {
        let board = Board::new("pipelines-pm");
        let first = requested_as(&board, 1, free());
        let second = requested(&board, 2);
        started(&board, 3, "session-1", Some(first));
        started(&board, 4, "session-2", Some(second));

        // The Product Manager's approval of the first, from the session that decides it.
        decided(
            &board,
            5,
            (Some("ada"), Some("session-1")),
            "data_pipeline.approved",
            first,
            "product_manager",
        );
        // Farik escalates the second after its tries, naming no agent and no session.
        board.put(
            at(10, 6),
            None,
            None,
            "data_pipeline.escalated",
            json!({ "pipeline": second, "reason": "The Product Manager did not decide" }),
        );
        let all = data_pipelines(&board.log).expect("the log reads");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].pipeline, first);
        assert_eq!(all[0].state, PipelineState::Approved);
        assert_eq!(all[0].by, Some(DecidedBy::ProductManager));
        assert_eq!(all[1].state, PipelineState::Escalated);
        assert!(all[1].by_farik);
        assert_eq!(
            all[1].escalated_reason.as_deref(),
            Some("The Product Manager did not decide")
        );
        // Declining it later is the owner's, and a decline files nothing. The owner who says
        // nothing leaves no reason.
        board.put(
            at(10, 7),
            None,
            None,
            "data_pipeline.declined",
            json!({ "pipeline": second, "by": "human", "reason": "" }),
        );
        let declined = &data_pipelines(&board.log).expect("the log reads")[1];
        assert_eq!(declined.state, PipelineState::Declined);
        assert_eq!(declined.by, Some(DecidedBy::Human));
        assert_eq!(declined.request, None);
        assert_eq!(declined.reason, None);
    }

    #[test]
    fn only_the_owner_s_decision_counts_as_the_owner_s() {
        let board = Board::new("pipelines-owner");
        let pipeline = requested(&board, 1);
        let other = requested(&board, 2);
        started(&board, 3, "session-1", Some(pipeline));
        started(&board, 4, "session-2", Some(other));
        started(&board, 5, "session-verify", None);

        for kind in ["data_pipeline.approved", "data_pipeline.declined"] {
            // `by: human` from an envelope that names an agent, a session, or both.
            for who in [
                (Some("proc"), None),
                (None, Some("session-proc")),
                (Some("ada"), Some("session-1")),
            ] {
                decided(&board, 6, who, kind, pipeline, "human");
            }
            // `by: product_manager` from a session that names no pipeline, from one that
            // decides another, from an envelope with no session, and from the agent that asked.
            for who in [
                (Some("ada"), Some("session-verify")),
                (Some("ada"), Some("session-2")),
                (Some("ada"), None),
                (None, None),
                (Some("proc"), Some("session-proc")),
            ] {
                decided(&board, 7, who, kind, pipeline, "product_manager");
            }
        }
        // An escalation counts from Farik and from the deciding session, and from no one else.
        for who in [
            (Some("proc"), Some("session-proc")),
            (Some("ada"), Some("session-verify")),
            (Some("ada"), Some("session-2")),
            (Some("ada"), None),
        ] {
            board.put_with(
                at(10, 8),
                None,
                who.0,
                who.1,
                "data_pipeline.escalated",
                json!({ "pipeline": pipeline, "reason": "Mine to pass on." }),
            );
        }
        let all = data_pipelines(&board.log).expect("the log reads");
        assert_eq!(all[0].pipeline, pipeline);
        assert_eq!(all[0].state, PipelineState::Open, "{:?}", all[0]);
        assert_eq!(all[0].escalated_reason, None);
        assert_eq!(all[0].by, None);
        assert_eq!(all[0].request, None);
        assert_eq!(all[1].state, PipelineState::Open);

        // The reviewer's probe: a paid request escalated in its decision session, then approved
        // by the Product Manager from that same session, stays escalated.
        let third = requested(&board, 9);
        started(&board, 10, "session-3", Some(third));
        let escalate = |minute| {
            board.put_with(
                at(10, minute),
                None,
                Some("ada"),
                Some("session-3"),
                "data_pipeline.escalated",
                json!({ "pipeline": third, "reason": "Mine to pass on." }),
            );
        };
        // A paid request the Product Manager approves from `Open` does not count either.
        decided(
            &board,
            11,
            (Some("ada"), Some("session-3")),
            "data_pipeline.approved",
            third,
            "product_manager",
        );
        // The owner's decision counts only on an escalated request: not on an open one.
        decided(
            &board,
            12,
            (None, None),
            "data_pipeline.declined",
            third,
            "human",
        );
        let all = data_pipelines(&board.log).expect("the log reads");
        assert_eq!(all[2].state, PipelineState::Open, "{:?}", all[2]);
        escalate(13);
        decided(
            &board,
            14,
            (Some("ada"), Some("session-3")),
            "data_pipeline.approved",
            third,
            "product_manager",
        );
        let all = data_pipelines(&board.log).expect("the log reads");
        assert_eq!(all[2].state, PipelineState::Escalated, "{:?}", all[2]);
        assert_eq!(all[2].by, None);
        // The owner may then decide it.
        decided(
            &board,
            15,
            (None, None),
            "data_pipeline.declined",
            third,
            "human",
        );
        let all = data_pipelines(&board.log).expect("the log reads");
        assert_eq!(all[2].state, PipelineState::Declined, "{:?}", all[2]);
        assert_eq!(all[2].by, Some(DecidedBy::Human));
    }
}
