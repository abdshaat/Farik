//! The owner's decisions on marketing plans (`docs/SPEC.md` 6.5, ADR 0042): approving or sending
//! one back, ending an approved one, and the ends that dates bring. The check that a plan may be
//! decided and the write of the decision are one step, as for a connector call's approval: the
//! lock `PLANS` is held by a decision, by the owner's end and by the dated ends alike, so that two
//! of them never act on one reading of the plans.

use std::fmt::Display;
use std::sync::{Mutex, MutexGuard, PoisonError};

use farik_core::marketing::{EndReason, PlanRecord, plans_to_end};
use farik_protocol::event::{EventBody, EventIds, FarikEvent, new_event};
use farik_store::marketing::{MarketingPlan, marketing_plans};
use serde_json::{Value, json};

use crate::orchestrator::{CommandError, CommandReport};
use crate::tools::ToolDeps;

/// Held by whoever decides a plan or ends one. One lock for every plan: decisions are rare.
static PLANS: Mutex<()> = Mutex::new(());

/// The proof that `PLANS` is held, which `record_plan_end` asks for.
pub(crate) struct PlansHeld(
    #[allow(dead_code, reason = "held for its drop")] MutexGuard<'static, ()>,
);

/// Takes `PLANS`, waiting for whoever holds it.
pub(crate) fn hold_plans() -> PlansHeld {
    PlansHeld(PLANS.lock().unwrap_or_else(PoisonError::into_inner))
}

fn failed(error: impl Display) -> CommandError {
    CommandError::Failed {
        detail: error.to_string(),
    }
}

fn refused(reason: String) -> CommandError {
    CommandError::Refused { reason }
}

/// The plan `plan`, or `unknown_marketing_plan`.
fn find<'a>(plans: &'a [MarketingPlan], plan: &str) -> Result<&'a MarketingPlan, CommandError> {
    plans
        .iter()
        .find(|found| found.record.id == plan)
        .ok_or_else(|| {
            refused(format!(
                "unknown_marketing_plan: {plan} is no marketing plan"
            ))
        })
}

/// Records `body` as the owner's or Farik's own act: the task on the envelope when it names one,
/// no agent, no session.
fn append(
    tools: &ToolDeps,
    task: Option<farik_core::contract::TaskId>,
    body: EventBody,
) -> Result<FarikEvent, String> {
    let ids = EventIds {
        task_id: task,
        ..tools.ids.clone()
    };
    let event = new_event(body, tools.clock.now(), ids)
        .map_err(|error| format!("the event cannot be recorded: {error:?}"))?;
    let appended = tools
        .log
        .append(&event)
        .map_err(|error| error.to_string())?;
    tools
        .projections
        .apply(&appended)
        .map_err(|error| error.to_string())?;
    Ok(appended)
}

/// What the owner said, left as they wrote it; nothing when it is blank.
fn owner_words(note: Option<String>) -> Option<String> {
    note.filter(|note| !note.trim().is_empty())
}

/// Approves the plan `plan` or sends it back, as the owner (`marketing_plan_decide`): refused
/// `unknown_marketing_plan`, `marketing_plan_decided` for a plan decided before,
/// `marketing_plan_expired` for approving one whose last day is past, and
/// `marketing_plan_reason_needed` for sending one back without a reason. An approval also ends,
/// in the same step, every approved plan the new one replaces at once.
pub(crate) fn decide_plan(
    tools: &ToolDeps,
    plan: &str,
    approve: bool,
    note: Option<String>,
) -> Result<CommandReport, CommandError> {
    let held = hold_plans();
    let plans = marketing_plans(&tools.log).map_err(failed)?;
    let found = find(&plans, plan)?;
    if found.decided.is_some() {
        return Err(refused(format!(
            "marketing_plan_decided: {plan} was decided already"
        )));
    }
    let today = tools.clock.now().date_naive();
    if approve && found.record.ends_on < today {
        return Err(refused(format!(
            "marketing_plan_expired: {plan} ended on {}, so it cannot be approved",
            found.record.ends_on
        )));
    }
    let note = owner_words(note);
    let task = Some(found.task_id.clone());
    let (body, report) = if approve {
        let body = json!({ "plan": plan, "note": note.unwrap_or_default() });
        (
            serde_json::from_value(body).map(EventBody::MarketingPlanApproved),
            format!("approved marketing plan {plan}"),
        )
    } else {
        let Some(reason) = note else {
            return Err(refused(format!(
                "marketing_plan_reason_needed: say why {plan} is sent back; the agent reads it"
            )));
        };
        (
            serde_json::from_value(json!({ "plan": plan, "reason": reason }))
                .map(EventBody::MarketingPlanReturned),
            format!("sent back marketing plan {plan}"),
        )
    };
    let decision = append(tools, task, body.map_err(failed)?).map_err(failed)?;
    let mut events = vec![decision.envelope.seq];
    let mut said = report;
    if approve {
        // The plans this one replaces at once, in the same step: the lock is still held.
        let records = records_of(tools)?;
        for (older, _, _) in plans_to_end(&records, today)
            .into_iter()
            .filter(|(_, why, by)| *why == EndReason::Replaced && by.as_deref() == Some(plan))
        {
            let ended =
                record_plan_end(&held, tools, &older, EndReason::Replaced, Some(plan), None)
                    .map_err(failed)?;
            events.extend(ended.iter().map(|event| event.envelope.seq));
            said.push_str(", which replaces ");
            said.push_str(&older);
        }
    }
    Ok(CommandReport { said, events })
}

/// What the log says of every plan, for the rules that decide which is active.
fn records_of(tools: &ToolDeps) -> Result<Vec<PlanRecord>, CommandError> {
    Ok(marketing_plans(&tools.log)
        .map_err(failed)?
        .into_iter()
        .map(|plan| plan.record)
        .collect())
}

/// Ends the approved plan `plan`, as the owner (`marketing_plan_end`): refused
/// `unknown_marketing_plan`, `marketing_plan_not_approved` for one never approved and
/// `marketing_plan_ended` for one that has ended.
pub(crate) fn end_plan(
    tools: &ToolDeps,
    plan: &str,
    note: Option<String>,
) -> Result<CommandReport, CommandError> {
    let held = hold_plans();
    let plans = marketing_plans(&tools.log).map_err(failed)?;
    let found = find(&plans, plan)?;
    if found.record.ended.is_some() {
        return Err(refused(format!("marketing_plan_ended: {plan} has ended")));
    }
    if found.record.approved_seq.is_none() {
        return Err(refused(format!(
            "marketing_plan_not_approved: {plan} was never approved, so there is nothing to end"
        )));
    }
    let ended = record_plan_end(
        &held,
        tools,
        plan,
        EndReason::ByOwner,
        None,
        owner_words(note),
    )
    .map_err(failed)?;
    Ok(CommandReport {
        said: format!("ended marketing plan {plan}"),
        events: ended.iter().map(|event| event.envelope.seq).collect(),
    })
}

/// Records the end of the approved plan `plan` for `why` (a replaced one names `replaced_by`),
/// unless it has ended already or was never approved, which records nothing. Every end goes
/// through here, the owner's and the dates', so that it is read again under the lock `held`
/// proves is held: two ends of one plan are never both recorded.
///
/// # Errors
///
/// What the log or the board refused, in words.
pub(crate) fn record_plan_end(
    _held: &PlansHeld,
    tools: &ToolDeps,
    plan: &str,
    why: EndReason,
    replaced_by: Option<&str>,
    note: Option<String>,
) -> Result<Vec<FarikEvent>, String> {
    let plans = marketing_plans(&tools.log).map_err(|error| error.to_string())?;
    let due = plans
        .iter()
        .find(|found| found.record.id == plan)
        .is_some_and(|found| found.record.approved_seq.is_some() && found.record.ended.is_none());
    if !due {
        return Ok(Vec::new());
    }
    let mut body = json!({
        "plan": plan,
        "why": match why {
            EndReason::Replaced => "replaced",
            EndReason::ByOwner => "by_owner",
            EndReason::Expired => "expired",
        },
    });
    for (name, value) in [
        ("replaced_by", replaced_by.map(str::to_string)),
        ("note", note),
    ] {
        if let Some(value) = value {
            body[name] = Value::String(value);
        }
    }
    let body = serde_json::from_value(body).map_err(|error| error.to_string())?;
    Ok(vec![append(
        tools,
        None,
        EventBody::MarketingPlanEnded(body),
    )?])
}
