//! What waits on the human when a process driving the project ends (`docs/SPEC.md` 5.7): the
//! questions nobody answered, the contracts awaiting approval, the escalations, the results that
//! may need the human's acceptance, and the tasks waiting to be integrated by hand.

use farik_core::team::Team;
use farik_store::files::ProjectFiles;
use farik_store::waiting::WaitingKind;
use farik_store::{EventLog, Projections};
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
/// acceptances, integrations), each by task id: the store's list, with the command that answers
/// each.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub(crate) fn waiting(
    log: &EventLog,
    projections: &Projections,
    files: &ProjectFiles,
    team: &Team,
) -> Result<Vec<Waiting>, String> {
    let listed =
        farik_store::waiting::waiting(projections, log, files, team).map_err(|e| e.to_string())?;
    Ok(listed
        .into_iter()
        .map(|item| {
            let id = item.task_id.as_str();
            let (what, command) = match item.kind {
                WaitingKind::Question => {
                    let seq = item.question_id.unwrap_or_default();
                    (
                        format!(
                            "question {seq} on {id} from {}: {}",
                            item.agent_id.as_deref().unwrap_or_default(),
                            item.line
                        ),
                        format!("farik answer {seq} <your answer>"),
                    )
                }
                WaitingKind::Approval => (
                    "awaits your approval".to_string(),
                    format!("farik approve {id}, or farik resolve {id} refining <why>"),
                ),
                WaitingKind::Help => (
                    format!(
                        "is escalated ({})",
                        item.reason.as_deref().unwrap_or("no reason recorded")
                    ),
                    format!("farik resolve {id} <status> <message>"),
                ),
                WaitingKind::Acceptance => (
                    "may need your acceptance".to_string(),
                    format!("farik accept {id} --message <your review>"),
                ),
                WaitingKind::ToolApproval => {
                    let seq = item.approval.as_ref().map_or(0, |ask| ask.approval);
                    (
                        format!("waits: {}", item.line),
                        format!("farik tool approve {seq}, or farik tool refuse {seq}"),
                    )
                }
                WaitingKind::Integration => (
                    "waits for you to integrate it".to_string(),
                    format!("farik integrate {id}"),
                ),
            };
            Waiting {
                task_id: id.to_string(),
                what,
                command,
                question: item.kind == WaitingKind::Question,
            }
        })
        .collect())
}
