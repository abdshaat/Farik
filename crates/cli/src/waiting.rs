//! What waits on the human when a process driving the project ends (`docs/SPEC.md` 5.7): the
//! questions nobody answered, the contracts awaiting approval, the escalations, the results that
//! may need the human's acceptance, the tasks waiting to be integrated by hand, the posts outside
//! the plan that wait for the owner, the purchase orders the Procurement Specialist set up and the data sources it asked for.

use catervas_core::team::Team;
use catervas_runtime::choices::offer_for;
use catervas_store::files::ProjectFiles;
use catervas_store::waiting::WaitingKind;
use catervas_store::{EventLog, Projections};
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
    /// The number of the post that waits, for a script to act on.
    post: Option<u64>,
    /// The number of the purchase order that waits, for a script to act on.
    order: Option<u64>,
    /// The number of the data pipeline request that waits, for a script to act on.
    pipeline: Option<u64>,
    /// An escalation's choices: the command, what happens next, and whether it is suggested.
    choices: Vec<(String, String, bool)>,
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
        lines.extend(self.choices.iter().map(|(command, next, suggested)| {
            let mark = if *suggested { " (suggested)" } else { "" };
            format!("  {command}  # {next}{mark}")
        }));
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
        if let Some(post) = self.post {
            item["post"] = json!(post);
        }
        if let Some(order) = self.order {
            item["order"] = json!(order);
        }
        if let Some(pipeline) = self.pipeline {
            item["pipeline"] = json!(pipeline);
        }
        if !self.choices.is_empty() {
            item["choices"] = self
                .choices
                .iter()
                .map(|(command, next, suggested)| {
                    json!({ "command": command, "next": next, "suggested": suggested })
                })
                .collect();
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
    let listed = catervas_store::waiting::waiting(projections, log, files, team)
        .map_err(|e| e.to_string())?;
    let mut described = Vec::new();
    for item in listed {
        let mut waiting = describe(&item);
        if item.kind == WaitingKind::Help {
            let offered = offer_for(log, projections, files, team, &item.task_id)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            waiting.choices = offered
                .iter()
                .map(|choice| {
                    (
                        choice.command_line(item.task_id.as_str()),
                        choice.next.clone(),
                        choice.suggested,
                    )
                })
                .collect();
        }
        described.push(waiting);
    }
    Ok(described)
}

/// What waits, and the command that answers it.
fn describe(item: &catervas_store::waiting::Waiting) -> Waiting {
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
                format!("catervas answer {seq} <your answer>"),
            )
        }
        WaitingKind::Approval => (
            "awaits your approval".to_string(),
            format!("catervas approve {id}, or catervas resolve {id} refining <why>"),
        ),
        WaitingKind::Help => (
            format!(
                "is escalated ({})",
                item.reason.as_deref().unwrap_or("no reason recorded")
            ),
            "choose one".to_string(),
        ),
        WaitingKind::Acceptance => (
            "may need your acceptance".to_string(),
            format!("catervas accept {id} --message <your review>"),
        ),
        WaitingKind::ToolApproval => {
            let seq = item.approval.as_ref().map_or(0, |ask| ask.approval);
            (
                format!("waits: {}", item.line),
                format!("catervas tool approve {seq}, or catervas tool refuse {seq}"),
            )
        }
        WaitingKind::MarketingPlan => {
            let plan = item.plan.as_ref().map_or("", |ask| ask.plan.as_str());
            (
                format!("waits: {}", item.line),
                format!(
                    "catervas marketing plan approve {plan}, or catervas marketing plan return {plan} \
                     --reason <text>"
                ),
            )
        }
        WaitingKind::SocialPost => {
            let post = item.post.as_ref().map_or(0, |ask| ask.post);
            (
                format!("waits: {}", item.line),
                format!(
                    "catervas marketing post send {post}, or catervas marketing post decline {post}"
                ),
            )
        }
        WaitingKind::PurchaseOrder => {
            let order = item.order.as_ref().map_or(0, |ask| ask.order);
            (
                format!("waits: {}", item.line),
                format!("catervas order approve {order}, or catervas order reject {order}"),
            )
        }
        WaitingKind::DataPipeline => {
            let pipeline = item.pipeline.as_ref().map_or(0, |ask| ask.pipeline);
            (
                format!("waits: {}", item.line),
                format!(
                    "catervas pipeline approve {pipeline}, or catervas pipeline decline {pipeline}"
                ),
            )
        }
        WaitingKind::Integration => (
            "waits for you to integrate it".to_string(),
            format!("catervas integrate {id}"),
        ),
    };
    Waiting {
        task_id: id.to_string(),
        what,
        command,
        question: item.kind == WaitingKind::Question,
        input: item.approval.as_ref().map(|ask| ask.input.clone()),
        plan: item.plan.as_ref().map(|ask| ask.plan.clone()),
        post: item.post.as_ref().map(|ask| ask.post),
        order: item.order.as_ref().map(|ask| ask.order),
        pipeline: item.pipeline.as_ref().map(|ask| ask.pipeline),
        choices: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use catervas_core::contract::TaskId;
    use catervas_store::waiting::{OrderAsk, PlanAsk, PostAsk, Waiting as Listed, WaitingKind};
    use chrono::NaiveDate;

    use super::describe;

    fn a_plan_waiting() -> Listed {
        Listed {
            task_id: "CTV-1".parse::<TaskId>().expect("a task id"),
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
            post: None,
            order: None,
            pipeline: None,
        }
    }

    fn a_post_waiting() -> Listed {
        Listed {
            kind: WaitingKind::SocialPost,
            line: "Kai wants to post on Instagram".to_string(),
            plan: None,
            post: Some(PostAsk {
                post: 42,
                channel: catervas_core::marketing::PostChannel::Instagram,
                text: "We open on Wednesday.".to_string(),
                media: Vec::new(),
                at: chrono::DateTime::parse_from_rfc3339("2026-11-04T10:00:00+01:00")
                    .expect("a time"),
            }),
            ..a_plan_waiting()
        }
    }

    fn an_order_waiting() -> Listed {
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-04T10:00:00Z")
            .expect("a time")
            .to_utc();
        Listed {
            kind: WaitingKind::PurchaseOrder,
            line: "Theo set up an order from Acme: 59.98 USD".to_string(),
            plan: None,
            order: Some(OrderAsk {
                order: 23,
                seller: "Acme".to_string(),
                seller_contact: "sales@acme.example".to_string(),
                lines: Vec::new(),
                currency: "USD".to_string(),
                period: "once".to_string(),
                total: "59.98".to_string(),
                delivery: String::new(),
                terms: String::new(),
                url: String::new(),
                evaluation: "evaluations/mirrors.md".to_string(),
                why: "It is the cheapest.".to_string(),
                at,
                expires_at: at + chrono::Duration::days(30),
                send: None,
            }),
            ..a_plan_waiting()
        }
    }

    #[test]
    fn a_run_says_which_order_waits() {
        let waiting = describe(&an_order_waiting());

        assert_eq!(
            waiting.lines(),
            [
                "CTV-1 waits: Theo set up an order from Acme: 59.98 USD: catervas order approve 23, or catervas order reject 23"
            ]
        );
        let json = waiting.json();
        assert_eq!(json["order"], 23);
        assert_eq!(json["task_id"], "CTV-1");
        assert_eq!(
            json["command"],
            "catervas order approve 23, or catervas order reject 23"
        );
        assert!(
            describe(&a_post_waiting()).json().get("order").is_none(),
            "a post names no order"
        );
    }

    #[test]
    fn a_run_says_which_plan_waits() {
        let waiting = describe(&a_plan_waiting());

        assert_eq!(
            waiting.lines(),
            [
                "CTV-1 waits: Kai proposes a marketing plan: Spring launch: catervas marketing plan approve MP-1, or catervas marketing plan return MP-1 --reason <text>"
            ]
        );
        let json = waiting.json();
        assert_eq!(json["plan"], "MP-1");
        assert_eq!(json["task_id"], "CTV-1");
        assert_eq!(
            json["command"],
            "catervas marketing plan approve MP-1, or catervas marketing plan return MP-1 --reason <text>"
        );
        // The kinds that name no plan carry no `plan`.
        let mut question = a_plan_waiting();
        question.kind = WaitingKind::Integration;
        question.plan = None;
        assert!(describe(&question).json().get("plan").is_none());
    }

    #[test]
    fn a_run_says_which_post_waits() {
        let waiting = describe(&a_post_waiting());

        assert_eq!(
            waiting.lines(),
            [
                "CTV-1 waits: Kai wants to post on Instagram: catervas marketing post send 42, or catervas marketing post decline 42"
            ]
        );
        let json = waiting.json();
        assert_eq!(json["post"], 42);
        assert_eq!(json["task_id"], "CTV-1");
        assert_eq!(
            json["command"],
            "catervas marketing post send 42, or catervas marketing post decline 42"
        );
        assert!(json.get("plan").is_none(), "a post names no plan");
    }
}
