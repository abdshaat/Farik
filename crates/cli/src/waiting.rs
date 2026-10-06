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
    /// A connector call's whole input, which the human is to see before allowing it.
    input: Option<String>,
    /// The id of the marketing plan that waits, for a script to act on.
    plan: Option<String>,
}

impl Waiting {
    /// The lines a person reads.
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut lines = if self.question {
            vec![self.what.clone(), format!("  {}", self.command)]
        } else {
            vec![format!("{} {}: {}", self.task_id, self.what, self.command)]
        };
        lines.extend(self.input.iter().map(|input| format!("  input: {input}")));
        lines
    }

    /// The same, as JSON.
    pub(crate) fn json(&self) -> Value {
        let mut item =
            json!({ "task_id": self.task_id, "what": self.what, "command": self.command });
        if let Some(input) = &self.input {
            item["input"] = json!(input);
        }
        if let Some(plan) = &self.plan {
            item["plan"] = json!(plan);
        }
        item
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
    Ok(listed.into_iter().map(|item| describe(&item)).collect())
}

/// What waits, and the command that answers it.
fn describe(item: &farik_store::waiting::Waiting) -> Waiting {
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
        WaitingKind::MarketingPlan => {
            let plan = item.plan.as_ref().map_or("", |ask| ask.plan.as_str());
            (
                format!("waits: {}", item.line),
                format!(
                    "farik marketing plan approve {plan}, or farik marketing plan return {plan} \
                     --reason <text>"
                ),
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
        input: item.approval.as_ref().map(|ask| ask.input.clone()),
        plan: item.plan.as_ref().map(|ask| ask.plan.clone()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use farik_core::contract::TaskId;
    use farik_store::waiting::{PlanAsk, Waiting as Listed, WaitingKind};

    use super::describe;

    fn a_plan_waiting() -> Listed {
        Listed {
            task_id: "FRK-1".parse::<TaskId>().expect("a task id"),
            kind: WaitingKind::MarketingPlan,
            agent_id: Some("kai".to_string()),
            title: "Spring launch".to_string(),
            line: "Kai proposes a marketing plan: Spring launch".to_string(),
            question_id: None,
            reason: None,
            approval: None,
            plan: Some(PlanAsk {
                plan: "MP-1".to_string(),
                summary: "Two weeks of posts.".to_string(),
                total: "2000.00".to_string(),
                currency: "USD".to_string(),
                starts_on: NaiveDate::from_ymd_opt(2026, 11, 2).expect("a date"),
                ends_on: NaiveDate::from_ymd_opt(2026, 11, 15).expect("a date"),
            }),
        }
    }

    #[test]
    fn a_run_says_which_plan_waits() {
        let waiting = describe(&a_plan_waiting());

        assert_eq!(
            waiting.lines(),
            [
                "FRK-1 waits: Kai proposes a marketing plan: Spring launch: farik marketing plan approve MP-1, or farik marketing plan return MP-1 --reason <text>"
            ]
        );
        let json = waiting.json();
        assert_eq!(json["plan"], "MP-1");
        assert_eq!(json["task_id"], "FRK-1");
        assert_eq!(
            json["command"],
            "farik marketing plan approve MP-1, or farik marketing plan return MP-1 --reason <text>"
        );
        // The kinds that name no plan carry no `plan`.
        let mut question = a_plan_waiting();
        question.kind = WaitingKind::Integration;
        question.plan = None;
        assert!(describe(&question).json().get("plan").is_none());
    }
}
