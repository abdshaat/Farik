//! What waits on the human when a process driving the project ends (`docs/SPEC.md` 5.7): the
//! questions nobody answered, the contracts awaiting approval, the escalations, the results that
//! may need the human's acceptance, and the tasks waiting to be integrated by hand.

use farik_core::contract::{Risk, TaskKind, TaskStatus};
use farik_core::team::{Integration, Team};
use farik_protocol::event::{EventBody, EventKind};
use farik_store::{EventLog, EventQuery, Projections};
use serde_json::{Value, json};

/// One thing that waits on the human.
pub(crate) struct Waiting {
    /// The task it is about.
    pub(crate) task_id: String,
    /// What waits.
    pub(crate) what: String,
    /// The command that answers it.
    pub(crate) command: String,
    /// Whether it is a question, which is printed on two lines.
    question: bool,
}

impl Waiting {
    /// The lines a person reads.
    pub(crate) fn lines(&self) -> Vec<String> {
        if self.question {
            vec![self.what.clone(), format!("  {}", self.command)]
        } else {
            vec![format!("{} {}: {}", self.task_id, self.what, self.command)]
        }
    }

    /// The same, as JSON.
    pub(crate) fn json(&self) -> Value {
        json!({ "task_id": self.task_id, "what": self.what, "command": self.command })
    }
}

/// Everything that waits on the human, in groups (questions, approvals, other escalations,
/// acceptances, integrations), each by task id.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub(crate) fn waiting(
    log: &EventLog,
    projections: &Projections,
    team: &Team,
) -> Result<Vec<Waiting>, String> {
    projections.catch_up().map_err(|error| error.to_string())?;
    let board = projections.board().map_err(|error| error.to_string())?;
    let history = log
        .read(&EventQuery {
            kinds: vec![
                EventKind::QuestionAsked,
                EventKind::QuestionAnswered,
                EventKind::EscalationRaised,
            ],
            ..EventQuery::default()
        })
        .map_err(|error| error.to_string())?;
    let mut waiting = Vec::new();
    for row in &board {
        for event in &history {
            let EventBody::QuestionAsked(body) = &event.body else {
                continue;
            };
            let seq = event.envelope.seq;
            let answered = history.iter().any(|later| {
                matches!(&later.body, EventBody::QuestionAnswered(answer)
                    if answer.question_id.get() == seq)
            });
            if answered || event.envelope.ids.task_id.as_ref() != Some(&row.task_id) {
                continue;
            }
            waiting.push(Waiting {
                task_id: row.task_id.to_string(),
                what: format!(
                    "question {seq} on {} from {}: {}",
                    row.task_id.as_str(),
                    body.asked_by,
                    body.question
                ),
                command: format!("farik answer {seq} <your answer>"),
                question: true,
            });
        }
    }
    let item = |row: &farik_store::TaskProjection, what: String, command: String| Waiting {
        task_id: row.task_id.to_string(),
        what,
        command,
        question: false,
    };
    for row in board.iter().filter(|row| row.awaiting_approval) {
        let id = row.task_id.as_str();
        waiting.push(item(
            row,
            "awaits your approval".to_string(),
            format!("farik approve {id}, or farik resolve {id} refining <why>"),
        ));
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Escalated && !row.awaiting_approval)
    {
        let reason = history
            .iter()
            .rev()
            .filter(|event| event.envelope.ids.task_id.as_ref() == Some(&row.task_id))
            .find_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason.to_string()),
                _ => None,
            })
            .unwrap_or_else(|| "no reason recorded".to_string());
        waiting.push(item(
            row,
            format!("is escalated ({reason})"),
            format!("farik resolve {} <status> <message>", row.task_id.as_str()),
        ));
    }
    for row in board.iter().filter(|row| {
        row.status == TaskStatus::Verifying
            && (row.kind == TaskKind::Epic || row.risk == Risk::High)
    }) {
        waiting.push(item(
            row,
            "may need your acceptance".to_string(),
            format!(
                "farik accept {} --message <your review>",
                row.task_id.as_str()
            ),
        ));
    }
    if team.policy.integration == Integration::Manual {
        for row in board.iter().filter(|row| row.awaiting_integration) {
            waiting.push(item(
                row,
                "waits for you to integrate it".to_string(),
                format!("farik integrate {}", row.task_id.as_str()),
            ));
        }
    }
    Ok(waiting)
}
