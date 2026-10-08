//! The human gates' queries and methods for the browser (`docs/SPEC.md` 5.4, 5.7, 5.16): what
//! waits on the human, a plan checked as it is typed and saved by the human, a task's diff, checks
//! and tries, the questions, each agent's activity, and what moved. `web.rs` answers the frames;
//! this module answers what they ask.

use std::fmt::Display;
use std::str::FromStr as _;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik_core::contract::{Role, TaskContract, TaskId, TaskKind, TaskStatus, validate_contract};
use farik_core::governor::gates::{ContractWriteActor, ContractWriteOutcome, check_contract_write};
use farik_core::governor::plain::plain_readiness;
use farik_core::governor::readiness::{ReadinessFailure, evaluate_readiness, rules_evaluated};
use farik_core::governor::sites::site_of;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::marketing::{
    CapScope, PlanSpend, RaiseAsk, Raised, active_plan, check_raise, parse_amount,
};
use farik_core::team::Team;
use farik_protocol::command::{Command, CommandReply};
use farik_protocol::event::{
    ContractWrittenBody, EventBody, EventKind, FarikEvent, event_to_value, new_event,
};
use farik_store::EventQuery;
use farik_store::activity::{ActivityState, activity, moved_since};
use farik_store::diff::diff_of;
use farik_store::marketing::{
    BudgetReached, MarketingPlan, budgets_reached, created_campaigns, marketing_plans, social_posts,
};
use farik_store::purchase_orders::purchase_orders;
use farik_store::requests::{
    RequestError, TOO_SHORT, contract_write, file_raise_request, file_request,
    placeholder_budget_usd, request_from_brief, request_from_text, summary_of,
};
use farik_store::waiting::{name_of, waiting};
use serde_json::{Value, json};

use super::DaemonState;
use super::web::{Failure, INTERNAL_ERROR, NOT_FOUND, REFUSED, UNKNOWN_QUERY};
use crate::cost::extra_tries;
use crate::marketing::ads::{ads_rows, open_raise, spend_and_pauses};
use crate::marketing::{going_out, kinds_made, known_spend, list_row, states_today, whole};
use crate::procurement::{add_pipeline_fields, pipeline_text};
use crate::tools::ToolDeps;
use crate::tools::contracts::changed_fields;
use crate::tools::design::ReviewState;
use crate::tools::media::fetch_picture;
use crate::transitions::last_move_into;

/// The methods this module answers.
pub(super) const METHODS: [&str; 5] = [
    "request.file",
    "contract.save",
    "social_post.media",
    "marketing_budget.raise",
    "purchase_order.file",
];

/// Who the human is in the log.
const HUMAN: &str = "human";

/// What the next session is told when the human's edit sends a frozen plan back.
const EDITED: &str = "The person changed the plan themselves. Check it as they left it, and ask \
                      for their approval again.";

fn internal(error: &dyn Display) -> Failure {
    Failure::new(INTERNAL_ERROR, error.to_string())
}

/// The UI changes in `verifying` whose design review waits on the human (step 12): for the
/// team's preview, for Docker's sandbox, or for the Designer's Playwright to be turned on.
fn design_reviews_waiting(deps: &ToolDeps, team: &Team) -> Result<Vec<Value>, Failure> {
    let board = deps.projections.board().map_err(|e| internal(&e))?;
    let designer = team
        .agents
        .iter()
        .find(|agent| agent.role == farik_core::team::RoleWire::UiUxDesigner)
        .map(|agent| agent.id.to_string());
    let mut rows = Vec::new();
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Verifying)
    {
        let mut agent_id = designer.clone();
        let (kind, line) = match deps
            .transitions
            .design_review(team, &row.task_id)
            .map_err(|e| internal(&e))?
            .1
            .state
        {
            ReviewState::PreviewMissing => (
                "preview_missing",
                format!(
                    "{} needs to know how to open your app",
                    name_of(team, designer.as_deref().unwrap_or("The UI/UX Designer"))
                ),
            ),
            ReviewState::DesignerNeedsSandbox => (
                "designer_needs_sandbox",
                "The UI/UX Designer needs Docker's sandbox to open your app. Turn the sandbox on, \
                 or retire the Designer"
                    .to_string(),
            ),
            ReviewState::DesignerNeedsBrowser => {
                // The active Designer, whose connector is off; a paused first one is not it.
                agent_id = team.designer().map(|agent| agent.id.to_string());
                let name = name_of(team, agent_id.as_deref().unwrap_or("The UI/UX Designer"));
                (
                    "designer_needs_browser",
                    format!(
                        "{name} has Playwright off, so Farik gives {name} no work. Turn \
                         Playwright on for {name} on the Team page"
                    ),
                )
            }
            _ => continue,
        };
        rows.push(json!({
            "task_id": row.task_id,
            "kind": kind,
            "agent_id": agent_id,
            "title": row.title,
            "line": line,
        }));
    }
    Ok(rows)
}

/// One row of `waiting.list`; a connector call's also names its approval, server, tool and input,
/// a post's its number, network, text, pictures and time, and a site request's its number, site,
/// address and reason.
fn waiting_row(deps: &ToolDeps, item: &farik_store::waiting::Waiting) -> Value {
    let mut row = json!({
        "task_id": item.task_id,
        "kind": item.kind.as_str(),
        "agent_id": item.agent_id,
        "title": item.title,
        "line": item.line,
    });
    if let Some(ask) = &item.approval {
        row["approval"] = json!(ask.approval);
        row["server"] = json!(ask.server);
        row["tool"] = json!(ask.tool);
        row["input"] = json!(ask.input);
    }
    if let Some(ask) = &item.plan {
        row["plan"] = json!(ask.plan);
        row["summary"] = json!(ask.summary);
        row["total"] = json!(ask.total);
        row["currency"] = json!(ask.currency);
        row["starts_on"] = json!(ask.starts_on.to_string());
        row["ends_on"] = json!(ask.ends_on.to_string());
    }
    if let Some(ask) = &item.site {
        row["request"] = json!(ask.request);
        row["host"] = json!(ask.host);
        row["url"] = json!(ask.url);
        row["why"] = json!(ask.why);
    }
    if let Some(ask) = &item.order {
        let time = |at: DateTime<Utc>| at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
        row["order"] = json!(ask.order);
        row["seller"] = json!(ask.seller);
        row["seller_contact"] = json!(ask.seller_contact);
        row["lines"] = json!(
            ask.lines
                .iter()
                .map(|line| json!({
                    "item": line.item, "quantity": line.quantity, "unit": line.unit,
                    "unit_price": line.unit_price, "line_total": line.line_total,
                }))
                .collect::<Vec<_>>()
        );
        row["currency"] = json!(ask.currency);
        row["period"] = json!(ask.period);
        row["total"] = json!(ask.total);
        row["delivery"] = json!(ask.delivery);
        row["terms"] = json!(ask.terms);
        row["url"] = json!(ask.url);
        // The site the address names, in the form Farik keeps: the page shows it, so that the
        // owner can tell a look-alike from the seller's own.
        if let Ok(host) = site_of(&ask.url) {
            row["host"] = json!(host);
        }
        row["evaluation"] = json!(ask.evaluation);
        row["why"] = json!(ask.why);
        row["at"] = json!(time(ask.at));
        row["expires_at"] = json!(time(ask.expires_at));
    }
    if let Some(ask) = &item.pipeline {
        // What the team would be asked, so that the owner reads it before they approve: the same
        // text approving files.
        let kit = (deps.kits)(Role::ProcurementSpecialist).ok();
        let text = pipeline_text(&deps.log, kit.as_ref(), ask.pipeline)
            .ok()
            .flatten();
        add_pipeline_fields(&mut row, ask, text.as_deref());
    }
    if let Some(ask) = &item.post {
        row["post"] = json!(ask.post);
        row["channel"] = json!(ask.channel.as_str());
        row["text"] = json!(ask.text);
        row["media"] = json!(
            ask.media
                .iter()
                .map(|media| json!({
                    "url": media.url,
                    "kind": if media.video { "video" } else { "image" },
                }))
                .collect::<Vec<_>>()
        );
        row["at"] = json!(ask.at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true));
    }
    row
}

/// The gates' queries, whose params the schema already passed.
#[allow(
    clippy::too_many_lines,
    reason = "one arm for each query the page asks"
)]
pub(super) fn query(
    state: &DaemonState,
    deps: &ToolDeps,
    name: &str,
    params: &Value,
) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let team = || deps.files.read_team().map_err(|e| internal(&e));
    match name {
        "waiting.list" => {
            let team = team()?;
            let listed = waiting(&deps.projections, &deps.log, &deps.files, &team)
                .map_err(|e| internal(&e))?;
            // What Farik could not keep within a budget or keep paused comes first: it is money.
            let mut rows = ads_rows(state, deps).map_err(|e| internal(&e))?;
            rows.extend(listed.iter().map(|item| waiting_row(deps, item)));
            rows.extend(design_reviews_waiting(deps, &team)?);
            Ok(json!({ "waiting": rows }))
        }
        "team.activity" => {
            let now = deps.clock.now();
            let all = activity(&deps.log, &deps.projections, &deps.files, &team()?, now)
                .map_err(|e| internal(&e))?;
            Ok(json!({ "activity": all.iter().map(|one| {
                let mut wire = json!({
                    "agent_id": one.agent_id,
                    "state": match one.state {
                        ActivityState::Working => "working",
                        ActivityState::Resting => "resting",
                        ActivityState::Waiting => "waiting",
                        ActivityState::Paused => "paused",
                        ActivityState::Idle => "idle",
                    },
                    "line": one.line,
                });
                if let Some(task) = &one.task_id {
                    wire["task_id"] = json!(task);
                }
                if let Some(until) = one.until {
                    wire["until"] = json!(until);
                }
                if let (Some(session), Some(purpose)) = (&one.session_id, one.purpose) {
                    wire["session_id"] = json!(session);
                    wire["purpose"] = json!(purpose.to_string());
                }
                wire
            }).collect::<Vec<_>>() }))
        }
        "moved.since" => {
            let since = params["since"]
                .as_str()
                .and_then(|since| DateTime::parse_from_rfc3339(since).ok())
                .ok_or_else(|| Failure::new(REFUSED, "since is not a date and time"))?
                .with_timezone(&Utc);
            let moved = moved_since(&deps.log, &deps.projections, &team()?, since)
                .map_err(|e| internal(&e))?;
            Ok(
                json!({ "moved": moved.iter().map(|one| json!({ "at": one.at, "line": one.line })).collect::<Vec<_>>() }),
            )
        }
        "marketing_plan.list" => marketing_plan_list(deps),
        "marketing_plan.get" => {
            marketing_plan_get(state, deps, params["plan"].as_str().unwrap_or(""))
        }
        "sites.list" => crate::tools::sites::site_list(&deps.log).map_err(|e| internal(&e)),
        "procurement_mailbox.get" => {
            crate::procurement::mailbox_state(deps).map_err(|e| internal(&e))
        }
        "purchase_orders.list" => {
            crate::procurement::purchase_orders_list(&deps.log, deps.clock.now().date_naive())
                .map_err(|e| internal(&e))
        }
        "renewals.list" => crate::procurement::renewals_list(&deps.log).map_err(|e| internal(&e)),
        "purchase_order.evaluation" => order_evaluation(deps, params),
        "social_posts.list" => Ok(json!({
            "posts": going_out(&social_posts(&deps.log).map_err(|e| internal(&e))?, deps.clock.now())
        })),
        "sprint.current" => sprint_current(deps),
        "backlog.summary" => backlog_summary(deps, &team()?),
        "questions.list" => questions(deps, params["task_id"].as_str()),
        _ => {
            let task_id = task_of(deps, params)?;
            match name {
                "contract.get" => Ok(json!({ "contract": file_value(deps, &task_id)? })),
                "contract.check" => check(deps, &team()?, &task_id, &params["contract"]),
                // A session makes hundreds of tool calls; they are the log's, not the task's
                // history. A refused one stays, since the governor stopped something.
                "task.history" => Ok(json!({
                    "events": history(deps, &task_id)?
                        .iter()
                        .filter(|event| !matches!(
                            event.body,
                            EventBody::ToolCalled(_) | EventBody::ToolReturned(_)
                        ))
                        .map(event_to_value)
                        .collect::<Vec<_>>()
                })),
                "task.diff" => task_diff(deps, &team()?, &task_id),
                "task.checks" => task_checks(deps, &team()?, &task_id),
                "task.tries" => {
                    // `iteration` counts returns, and the limit bounds them (5.2), so the try in
                    // progress is one more than the returns, of one more than the limit.
                    let row = row(deps, &task_id)?;
                    let contract = contract_of(deps, &task_id)?;
                    let of = u32::try_from(contract.budget.max_iterations.get())
                        .unwrap_or(u32::MAX)
                        .saturating_add(extra_tries(&history(deps, &task_id)?))
                        .saturating_add(1);
                    Ok(json!({ "try": row.iteration.saturating_add(1), "of": of }))
                }
                "escalation.choices" => escalation_choices(deps, &team()?, &task_id),
                _ => Err(Failure::new(
                    UNKNOWN_QUERY,
                    format!("there is no query {name}"),
                )),
            }
        }
    }
}

/// `marketing_plan.list`: every marketing plan, newest first, with where each stands today.
fn marketing_plan_list(deps: &ToolDeps) -> Result<Value, Failure> {
    let plans = marketing_plans(&deps.log).map_err(|e| internal(&e))?;
    let states = states_today(&plans, deps.clock.now().date_naive());
    Ok(json!({
        "plans": plans
            .iter()
            .zip(states)
            .rev()
            .map(|(plan, state)| list_row(plan, state))
            .collect::<Vec<_>>()
    }))
}

/// `marketing_plan.get`: one marketing plan whole, or `not_found`, with what Farik's watch knows of
/// its ads' spend, the budgets they reached and the campaigns Farik paused.
fn marketing_plan_get(state: &DaemonState, deps: &ToolDeps, id: &str) -> Result<Value, Failure> {
    let plans = marketing_plans(&deps.log).map_err(|e| internal(&e))?;
    let states = states_today(&plans, deps.clock.now().date_naive());
    let posts = social_posts(&deps.log).map_err(|e| internal(&e))?;
    let (plan, state_of) = plans
        .iter()
        .zip(states)
        .find(|(plan, _)| plan.record.id == id)
        .ok_or_else(|| Failure::new(NOT_FOUND, format!("there is no marketing plan {id}")))?;
    let made = created_campaigns(&deps.log).map_err(|e| internal(&e))?;
    let mut whole = whole(
        plan,
        state_of,
        &posts,
        deps.clock.now().date_naive(),
        &kinds_made(&plans, &made, plan),
    );
    if let (Some(whole), more) = (
        whole.as_object_mut(),
        spend_and_pauses(state, deps, plan).map_err(|e| internal(&e))?,
    ) {
        whole.extend(more);
    }
    Ok(whole)
}

/// The task the params name, which the board must hold.
fn task_of(deps: &ToolDeps, params: &Value) -> Result<TaskId, Failure> {
    let asked = params["task_id"].as_str().unwrap_or_default();
    let task_id: TaskId = asked
        .parse()
        .map_err(|_| Failure::new(NOT_FOUND, format!("there is no task {asked}")))?;
    row(deps, &task_id)?;
    Ok(task_id)
}

fn row(deps: &ToolDeps, task_id: &TaskId) -> Result<farik_store::TaskProjection, Failure> {
    deps.projections
        .task(task_id)
        .map_err(|e| internal(&e))?
        .ok_or_else(|| Failure::new(NOT_FOUND, format!("there is no task {}", task_id.as_str())))
}

fn contract_of(deps: &ToolDeps, task_id: &TaskId) -> Result<TaskContract, Failure> {
    deps.files.read_contract(task_id).map_err(|e| internal(&e))
}

fn file_value(deps: &ToolDeps, task_id: &TaskId) -> Result<Value, Failure> {
    serde_json::to_value(contract_of(deps, task_id)?).map_err(|e| internal(&e))
}

fn history(deps: &ToolDeps, task_id: &TaskId) -> Result<Vec<FarikEvent>, Failure> {
    deps.log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))
}

/// A readiness rule as the wire names it: its name in `snake_case`.
fn rule_name(failure: &ReadinessFailure) -> String {
    let mut name = String::new();
    for (i, c) in format!("{:?}", failure.rule).chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            name.push('_');
        }
        name.push(c.to_ascii_lowercase());
    }
    name
}

/// The Definition of Ready of `contract` as given, and how many rules it ran.
fn readiness(
    deps: &ToolDeps,
    team: &Team,
    contract: &TaskContract,
) -> Result<(Vec<ReadinessFailure>, usize), Failure> {
    let context = deps
        .transitions
        .readiness_context(team, contract)
        .map_err(|e| internal(&e))?;
    let result = evaluate_readiness(contract, &context);
    let total = rules_evaluated(&result);
    Ok((result.err().unwrap_or_default(), total))
}

/// `contract.check`: the schema's errors, as rule `schema`, or else the Definition of Ready's
/// failures with their plain sentences; `total` counts the schema as one check. Nothing is written.
fn check(deps: &ToolDeps, team: &Team, task_id: &TaskId, draft: &Value) -> Result<Value, Failure> {
    let mut draft = draft.clone();
    draft["id"] = json!(task_id);
    let contract = match validate_contract(&draft) {
        Err(errors) => {
            return Ok(json!({
                "failures": errors.iter().map(|error| json!({
                    "rule": "schema",
                    "message": format!("{} {}", error.path, error.message),
                    "plain": error.message,
                })).collect::<Vec<_>>(),
                "total": 1,
            }));
        }
        Ok(contract) => contract,
    };
    let (failures, total) = readiness(deps, team, &contract)?;
    Ok(json!({
        "failures": failures.iter().map(|failure| json!({
            "rule": rule_name(failure),
            "message": failure.message,
            "plain": plain_readiness(failure.rule),
        })).collect::<Vec<_>>(),
        "total": total + 1,
    }))
}

/// `task.diff`: the task's diff, or an epic's tasks' joined; for a task in a private folder, the
/// names of the files that changed in it and no diff.
fn task_diff(deps: &ToolDeps, team: &Team, task_id: &TaskId) -> Result<Value, Failure> {
    let contract = contract_of(deps, task_id)?;
    let mut children = Vec::new();
    if contract.kind == TaskKind::Epic {
        for id in deps.files.list_contracts().map_err(|e| internal(&e))? {
            let child = contract_of(deps, &id)?;
            if child.parent.as_ref().map(|parent| parent.as_str()) == Some(task_id.as_str()) {
                let events = history(deps, &id)?;
                children.push((child, events));
            }
        }
    }
    let diff = diff_of(
        &deps.git,
        team,
        &contract,
        &history(deps, task_id)?,
        &children,
    )
    .map_err(|sentence| Failure::new(REFUSED, sentence))?;
    let mut answer = json!({
        "diff": diff.diff, "files": diff.files, "added": diff.added, "removed": diff.removed,
    });
    // A task in a private folder shows its files' names and no diff (6.6).
    if diff.private_folder {
        answer["private_folder"] = json!(true);
    }
    Ok(answer)
}

/// `task.checks`: each criterion's latest result since the task last entered `verifying`, in the
/// contract's order; for a plan awaiting approval, the Definition of Ready's failures.
fn task_checks(deps: &ToolDeps, team: &Team, task_id: &TaskId) -> Result<Value, Failure> {
    let contract = contract_of(deps, task_id)?;
    if row(deps, task_id)?.awaiting_approval {
        let (failures, _) = readiness(deps, team, &contract)?;
        return Ok(json!({ "checks": failures.iter().map(|failure| json!({
            "criterion_id": rule_name(failure),
            "text": plain_readiness(failure.rule),
            "passed": false,
            "evidence": failure.message,
        })).collect::<Vec<_>>() }));
    }
    let events = history(deps, task_id)?;
    let since =
        last_move_into(&events, TaskStatus::Verifying).map_or(0, |event| event.envelope.seq);
    let checks: Vec<Value> = contract
        .exit_criteria
        .iter()
        .filter_map(|criterion| {
            events
                .iter()
                .rev()
                .take_while(|event| event.envelope.seq > since)
                .find_map(|event| match &event.body {
                    EventBody::CriterionRecorded(body)
                        if body.criterion_id == criterion.id.as_str() =>
                    {
                        Some(json!({
                            "criterion_id": body.criterion_id,
                            "text": criterion.text,
                            "passed": body.passed,
                            "evidence": body.evidence,
                        }))
                    }
                    _ => None,
                })
        })
        .collect();
    Ok(json!({ "checks": checks }))
}

/// `sprint.current`: the open sprint and how many of its tasks are accepted or cancelled.
fn sprint_current(deps: &ToolDeps) -> Result<Value, Failure> {
    let Some(open) = deps.projections.open_sprint().map_err(|e| internal(&e))? else {
        return Ok(Value::Null);
    };
    let board = deps.projections.board().map_err(|e| internal(&e))?;
    let tasks: Vec<_> = board
        .iter()
        .filter(|row| row.sprint.as_deref() == Some(open.sprint_id.as_str()))
        .collect();
    let done = tasks
        .iter()
        .filter(|row| matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
        .count();
    Ok(json!({ "sprint_id": open.sprint_id, "done": done, "total": tasks.len() }))
}

/// `backlog.summary`: whether `team` plans in sprints, and how many of the Backlog's rows have no
/// parent, so an epic counts once.
fn backlog_summary(deps: &ToolDeps, team: &Team) -> Result<Value, Failure> {
    let open = deps.projections.open_sprint().map_err(|e| internal(&e))?;
    let open = open.as_ref().map(|open| open.sprint_id.as_str());
    let board = deps.projections.board().map_err(|e| internal(&e))?;
    let count = crate::sprints::backlog(team, open, &board).count();
    Ok(json!({ "plan_in_sprints": team.plans_in_sprints(), "count": count }))
}

/// `questions.list`: every question, of `task` when one is named, oldest first, with its answer.
fn questions(deps: &ToolDeps, task: Option<&str>) -> Result<Value, Failure> {
    let events = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::QuestionAsked, EventKind::QuestionAnswered],
            ..EventQuery::default()
        })
        .map_err(|e| internal(&e))?;
    let listed: Vec<Value> = events
        .iter()
        .filter(|event| {
            task.is_none_or(|task| {
                event.envelope.ids.task_id.as_ref().map(|id| id.as_str()) == Some(task)
            })
        })
        .filter_map(|event| {
            let EventBody::QuestionAsked(body) = &event.body else {
                return None;
            };
            let seq = event.envelope.seq;
            let answer = events.iter().rev().find_map(|later| match &later.body {
                EventBody::QuestionAnswered(answer) if answer.question_id.get() == seq => {
                    Some(answer.answer.clone())
                }
                _ => None,
            });
            Some(json!({
                "question_id": seq,
                "task_id": event.envelope.ids.task_id,
                "agent_id": body.asked_by,
                "text": body.question,
                "choices": body.choices,
                "answer": answer,
            }))
        })
        .collect();
    Ok(json!({ "questions": listed }))
}

/// `escalation.choices`: what the human may do about the task's escalation, or nothing when it
/// is not escalated.
fn escalation_choices(deps: &ToolDeps, team: &Team, task_id: &TaskId) -> Result<Value, Failure> {
    if row(deps, task_id)?.status != TaskStatus::Escalated {
        return Ok(json!({ "choices": [] }));
    }
    let events = history(deps, task_id)?;
    let reason = events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            EventBody::EscalationRaised(body) => Some(body.reason.to_string()),
            _ => None,
        })
        .unwrap_or_default();
    let before =
        last_move_into(&events, TaskStatus::Escalated).and_then(|event| match &event.body {
            EventBody::TaskTransitioned(body) => TaskStatus::from_str(&body.from.to_string()).ok(),
            _ => None,
        });
    let product_manager = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
        .map_or_else(
            || "the Product Manager".to_string(),
            |agent| name_of(team, agent.id.as_str()),
        );
    Ok(json!({ "choices": choices(task_id.as_str(), &reason, before, &product_manager) }))
}

/// The gates' methods, whose params the schema already passed.
pub(super) async fn call(
    state: &DaemonState,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    let Some(deps) = state.deps().cloned() else {
        return Err(Failure::new(super::web::NO_PROJECT, super::NO_PROJECT));
    };
    if method == "social_post.media" {
        return post_picture(&deps, params).await;
    }
    if method == "marketing_budget.raise" {
        let plan = params["plan"].as_str().unwrap_or_default().to_string();
        let spent = known_spend(state, &deps, &plan).map_err(|e| internal(&e))?;
        let params = params.clone();
        let filed = off_the_worker(move || raise_budget(&deps, &spent, &params)).await?;
        state.wakes().notify_one();
        return Ok(filed);
    }
    if method == "purchase_order.file" {
        let order = params["order"].as_u64().unwrap_or_default();
        return off_the_worker(move || order_file(&deps, order)).await;
    }
    if method == "request.file" {
        let text = params["text"].as_str().unwrap_or_default().to_string();
        let link = params["from_chat_message"].as_u64();
        let filed = off_the_worker(move || file_words(&deps, &text, link)).await?;
        state.wakes().notify_one();
        return Ok(filed);
    }
    save(state, &deps, params).await
}

/// The file `path` of the Procurement Specialist's private folder, read up to `most` bytes, when
/// it is a file the folder's rules reach: no link on the way, and the resolved path inside the
/// folder. `not_found` for any other.
fn procurement_file(deps: &ToolDeps, path: &str, most: u64) -> Result<Vec<u8>, Failure> {
    use std::io::Read as _;

    let nothing = || Failure::new(NOT_FOUND, format!("there is no file at {path}"));
    let folder =
        farik_core::team::private_folder(Role::ProcurementSpecialist).ok_or_else(nothing)?;
    let at = crate::tools::sheets::private_path(deps.files.root(), folder, path)
        .map_err(|_| nothing())?;
    if !at.is_file() {
        return Err(nothing());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&at)
        .and_then(|file| file.take(most + 1).read_to_end(&mut bytes))
        .map_err(|_| nothing())?;
    if bytes.len() as u64 > most {
        return Err(nothing());
    }
    Ok(bytes)
}

/// `purchase_order.evaluation`: the comparison order `order` rests on, as text. `not_found` for
/// an order nobody drafted, a path that is not `evaluations/<name>.md`, a note that is gone or
/// lies behind a link, and one past 64 KiB.
fn order_evaluation(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    let order = params["order"].as_u64().unwrap_or_default();
    let record = purchase_orders(&deps.log)
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|record| record.order == order)
        .ok_or_else(|| Failure::new(NOT_FOUND, format!("there is no order PO-{order}")))?;
    let path = record.drafted.evaluation.to_string();
    if !path.starts_with("evaluations/") {
        return Err(Failure::new(
            NOT_FOUND,
            format!("there is no comparison at {path}"),
        ));
    }
    let bytes = procurement_file(deps, &path, 64 * 1024)?;
    Ok(json!({ "text": String::from_utf8_lossy(&bytes) }))
}

/// `purchase_order.file`: order `order`'s workbook, `orders/PO-<n>.xlsx`, for the owner to
/// download. `not_found` for an order nobody drafted, a workbook that is gone, or one that is, or
/// lies behind, a link.
fn order_file(deps: &ToolDeps, order: u64) -> Result<Value, Failure> {
    use base64::Engine as _;

    let known = purchase_orders(&deps.log)
        .map_err(|e| internal(&e))?
        .iter()
        .any(|record| record.order == order);
    if !known {
        return Err(Failure::new(
            NOT_FOUND,
            format!("there is no order PO-{order}"),
        ));
    }
    let bytes = procurement_file(deps, &format!("orders/PO-{order}.xlsx"), 10 * 1024 * 1024)?;
    Ok(json!({
        "media_type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "base64": base64::engine::general_purpose::STANDARD.encode(bytes),
        "name": format!("PO-{order}.xlsx"),
    }))
}

/// `social_post.media`: one picture of a post, fetched here since the browser may not load pictures
/// from other sites. `not_found` for an unknown post or picture, for a clip, and for an address
/// that is not public, is gone, is not a picture of a kind Farik shows, or is too big.
async fn post_picture(deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
    use base64::Engine as _;

    let nothing = || Failure::new(NOT_FOUND, "there is no picture to show");
    let number = params["post"].as_u64().unwrap_or_default();
    let index = usize::try_from(params["index"].as_u64().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
    let post = social_posts(&deps.log)
        .map_err(|e| internal(&e))?
        .into_iter()
        .find(|post| post.post == number)
        .ok_or_else(nothing)?;
    let picture = post
        .media
        .get(index)
        .filter(|media| !media.video)
        .ok_or_else(nothing)?;
    let (media_type, bytes) = fetch_picture(&picture.url).await.map_err(|_| nothing())?;
    Ok(json!({
        "media_type": media_type,
        "base64": base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

/// `work`, which reads and writes the store, run where it cannot hold up the daemon's worker.
async fn off_the_worker<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|error| Err(internal(&error)))
}

/// `request.file`: the person's words filed as a draft request of the human's, from a chat's
/// proposed request when `from_chat_message` names its reply.
fn file_words(
    deps: &ToolDeps,
    text: &str,
    from_chat_message: Option<u64>,
) -> Result<Value, Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let request =
        request_from_text(text, placeholder_budget_usd(&team.rules())).map_err(|sentence| {
            let mut refused = Failure::new(REFUSED, sentence.clone());
            // Coded as team.save's refusals are, so the page words it without matching words.
            if sentence == TOO_SHORT {
                refused.data = Some(json!({ "errors": [{
                    "path": "/text", "message": sentence, "code": "too_short",
                }] }));
            }
            refused
        })?;
    let filed = file_request(
        &deps.files,
        &deps.log,
        request,
        HUMAN,
        None,
        deps.clock.now(),
        &deps.ids,
        from_chat_message,
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => Failure::new(REFUSED, format!("the request {reason}")),
        other => internal(&other),
    })?;
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    Ok(json!({ "task_id": filed.id }))
}

/// A refusal of a raise, coded as `team.save`'s are, so that the page words it without matching
/// words.
fn raise_refusal(path: &str, code: &str, sentence: &str) -> Failure {
    let mut failure = Failure::new(REFUSED, sentence);
    failure.data = Some(json!({ "errors": [{ "path": path, "message": sentence, "code": code }] }));
    failure
}

/// The plan `plan_id`, which a raise may only name when it is the one running now and has reached
/// a budget, with the budgets it reached, and when no raise of it is open: a request that raises
/// it, not yet `accepted` or `cancelled`.
fn plan_to_raise(
    deps: &ToolDeps,
    plan_id: &str,
) -> Result<(MarketingPlan, Vec<BudgetReached>), Failure> {
    let plans = marketing_plans(&deps.log).map_err(|e| internal(&e))?;
    let records: Vec<farik_core::marketing::PlanRecord> =
        plans.iter().map(|plan| plan.record.clone()).collect();
    let active = active_plan(&records, deps.clock.now().date_naive())
        .filter(|record| record.id == plan_id)
        .and_then(|record| plans.iter().find(|plan| plan.record.id == record.id))
        .ok_or_else(|| {
            raise_refusal(
                "/plan",
                "raise_refused",
                &format!("{plan_id} is not the marketing plan that is running now."),
            )
        })?;
    let reached: Vec<BudgetReached> = budgets_reached(&deps.log)
        .map_err(|e| internal(&e))?
        .into_iter()
        .filter(|reached| reached.plan == plan_id)
        .collect();
    if reached.is_empty() {
        return Err(raise_refusal(
            "/plan",
            "raise_refused",
            &format!("{plan_id} has not reached a budget, so there is nothing to raise."),
        ));
    }
    let open = open_raise(deps, plan_id)
        .map_err(|e| internal(&e))?
        .is_some();
    if open {
        return Err(raise_refusal(
            "/plan",
            "raise_open",
            &format!("A new version of {plan_id} with a raised budget is being written already."),
        ));
    }
    Ok((active.clone(), reached))
}

/// What the owner asked in a raise's params: the amounts, read.
fn raise_asked(params: &Value) -> Result<RaiseAsk, Failure> {
    let amount = |path: &str, text: &Value, example: &str| {
        parse_amount(text.as_str().unwrap_or_default()).ok_or_else(|| {
            raise_refusal(
                path,
                "raise_refused",
                &format!("Give an amount such as {example}."),
            )
        })
    };
    Ok(RaiseAsk {
        google_ads: amount("/google_ads", &params["google_ads"], "1200.00")?,
        campaigns: params["campaigns"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|each| {
                let key = each["key"].as_str().unwrap_or_default().to_string();
                let budget = amount(&format!("/campaigns/{key}"), &each["budget"], "700.00")?;
                Ok((key, budget))
            })
            .collect::<Result<Vec<_>, Failure>>()?,
    })
}

/// The title and the words of the request for a new version of `plan_id` with `raised`, in
/// `currency`: the agent proposes a plan that replaces `plan_id` from today with these budgets,
/// keeps its campaigns and slots that are not over, and, once the owner approves it, raises each
/// paused campaign's budget at Google and enables it. The plan's id stands for the plan, since the
/// proposal names the plan it replaces by it.
fn raise_words(plan_id: &str, currency: &str, raised: &Raised) -> (String, String) {
    use std::fmt::Write as _;

    let clauses = raised
        .campaigns
        .iter()
        .fold(String::new(), |mut text, (key, budget)| {
            let _ = write!(text, ", and {key}'s budget {budget}");
            text
        });
    (
        format!("New version of {plan_id} with a raised budget"),
        format!(
            "Propose a new version of {plan_id} that replaces it (replaces: {plan_id}), starting \
             today, with its Google Ads budget {} {currency} and its total {}{clauses}; keep its \
             campaigns and post slots that are not over, each with its key and what it advertises, \
             their dates from today on. Once the owner approves it, raise each paused campaign's \
             budget at Google with set_campaign_budget, then enable it.",
            raised.google_ads, raised.total
        ),
    )
}

/// `marketing_budget.raise`: the owner raises the budget of the active marketing plan whose ads
/// reached it. The plan must be the active one and have a `marketing_budget.reached`, no raise of
/// it may be open, and the amounts must pass `check_raise` (`raise_refused`, `raise_open`). Files
/// the request for a new version of the plan as a request of the human's that skips triage and the
/// sprint queue (`file_raise_request`), and answers its task.
fn raise_budget(deps: &ToolDeps, spent: &PlanSpend, params: &Value) -> Result<Value, Failure> {
    let plan_id = params["plan"].as_str().unwrap_or_default();
    let (plan, reached) = plan_to_raise(deps, plan_id)?;
    let ask = raise_asked(params)?;
    let capped: Vec<String> = reached
        .iter()
        .filter(|reached| reached.scope == CapScope::Campaign)
        .filter_map(|reached| reached.key.clone())
        .collect();
    let raised = check_raise(&plan.proposal, spent, &capped, &ask).map_err(|faults| {
        let mut failure = Failure::new(REFUSED, faults[0].message.clone());
        failure.data = Some(json!({ "errors": faults.iter().map(|fault| json!({
            "path": fault.path, "message": fault.message, "code": "raise_refused",
        })).collect::<Vec<_>>() }));
        failure
    })?;
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let (title, words) = raise_words(plan_id, &plan.proposal.currency, &raised);
    let request = request_from_brief(&title, &words, placeholder_budget_usd(&team.rules()))
        .map_err(|sentence| Failure::new(REFUSED, sentence))?;
    let filed = file_raise_request(
        &deps.files,
        &deps.log,
        request,
        (HUMAN, plan_id),
        (deps.clock.now(), &deps.ids),
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => Failure::new(REFUSED, format!("the request {reason}")),
        other => internal(&other),
    })?;
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    Ok(json!({ "task_id": filed.id }))
}

/// `contract.save`: the human's edit, judged by `check_contract_write`. One the team is working to
/// is refused; otherwise the fields the human changed are written over the file and
/// `contract.written` is recorded as theirs, and then a frozen plan goes back to `refining`
/// (5.11), by the escalation's resolve.
async fn save(state: &DaemonState, deps: &Arc<ToolDeps>, params: &Value) -> Result<Value, Failure> {
    let (held, edit) = (deps.clone(), params.clone());
    let (task_id, changed, after, back) = off_the_worker(move || judged(&held, &edit)).await?;
    // Written while the task is still held, so a session the move wakes reads the human's plan.
    // If the move is then refused, the edit sits on a held task, which nobody works to.
    if !changed.is_empty() {
        let (held, task_id) = (deps.clone(), task_id.clone());
        off_the_worker(move || written_over(&held, &task_id, &changed, &after)).await?;
    }
    if back {
        let command = Command::EscalationResolve {
            task_id,
            to: TaskStatus::Refining,
            message: EDITED.to_string(),
            extra_tries: None,
        };
        if let CommandReply::Error { detail, .. } = super::handled(state, command).await {
            return Err(Failure::new(REFUSED, detail));
        }
    }
    Ok(json!({ "saved": true, "back_to_refining": back }))
}

/// The human's edit judged before anything moves: the task, the fields it changes, the contract
/// as it would be, and whether it first goes back to `refining`.
fn judged(deps: &ToolDeps, params: &Value) -> Result<(TaskId, Vec<String>, Value, bool), Failure> {
    let task_id = task_of(deps, params)?;
    let before = file_value(deps, &task_id)?;
    let contract = contract_of(deps, &task_id)?;
    let row = row(deps, &task_id)?;
    let edited = &params["contract"];
    let changed = changed_fields(&before, edited);
    // Locking is recorded as `contract.locked`/`contract.unlocked`, which a save does not write.
    if changed.iter().any(|field| field == "locked") {
        return Err(Failure::new(
            REFUSED,
            "a save does not lock or unlock the plan; lock or unlock it on its own",
        ));
    }
    let outcome = check_contract_write(
        contract.kind,
        row.status,
        contract.locked,
        &ContractWriteActor {
            kind: TransitionActor::Human,
            agent_id: None,
        },
        &changed,
    )
    .map_err(|refusal| Failure::new(REFUSED, contract_write(&refusal)))?;
    // Held to the contract's rules before anything moves.
    let mut after = before.clone();
    for field in &changed {
        match edited.get(field) {
            Some(value) => after[field] = value.clone(),
            None => {
                if let Some(object) = after.as_object_mut() {
                    object.remove(field);
                }
            }
        }
    }
    validate_contract(&after).map_err(|errors| Failure::new(REFUSED, schema_words(&errors)))?;
    let back = outcome == ContractWriteOutcome::ReturnsToRefining;
    // The human's only way into `refining` from a frozen plan is an escalation's resolve.
    if back && row.status != TaskStatus::Escalated {
        return Err(Failure::new(
            REFUSED,
            "the team is working to this plan; hold the work first, then change it",
        ));
    }
    Ok((task_id, changed, after, back))
}

/// The human's fields written over the contract as it is now, and `contract.written` recorded.
fn written_over(
    deps: &ToolDeps,
    task_id: &TaskId,
    changed: &[String],
    after: &Value,
) -> Result<(), Failure> {
    // The human's fields go over the file as it is now, lifecycle fields and all.
    let mut written = file_value(deps, task_id)?;
    for field in changed {
        written[field] = after[field].clone();
        if after.get(field).is_none()
            && let Some(object) = written.as_object_mut()
        {
            object.remove(field);
        }
    }
    let mut written = validate_contract(&written)
        .map_err(|errors| Failure::new(REFUSED, schema_words(&errors)))?;
    // The board holds the status (5.2), and `contract.written` carries it to the board.
    written.status = row(deps, task_id)?.status;
    written.updated_at = Some(deps.clock.now());
    deps.files
        .write_contract(&written)
        .map_err(|e| internal(&e))?;
    let event = new_event(
        EventBody::ContractWritten(ContractWrittenBody {
            summary: summary_of(&written),
            written_by: HUMAN.to_string(),
        }),
        deps.clock.now(),
        farik_protocol::event::EventIds {
            task_id: Some(task_id.clone()),
            ..deps.ids.clone()
        },
    )
    .map_err(|error| internal(&format!("{error:?}")))?;
    let recorded = deps.log.append(&event).map_err(|e| internal(&e))?;
    deps.projections
        .apply(&recorded)
        .map_err(|e| internal(&e))?;
    Ok(())
}

fn schema_words(errors: &[farik_core::contract::ValidationError]) -> String {
    errors
        .iter()
        .map(|error| format!("{} {}", error.path, error.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The choices a task escalated for `reason` offers the human (ADR 0024): the label, and the
/// command the page sends, to which it adds the message. `before` is the status the task held
/// before it escalated, and `product_manager` the name the plan's writer goes by.
pub(crate) fn choices(
    task_id: &str,
    reason: &str,
    before: Option<TaskStatus>,
    product_manager: &str,
) -> Vec<Value> {
    let resolve = |to: String| json!({ "command": "escalation_resolve", "body": { "task_id": task_id, "to": to } });
    let choice = |label: &str, body: Value| json!({ "label": label, "body": body });
    let cancel = choice("Cancel the task", resolve("cancelled".to_string()));
    let change = || choice("Change the plan", resolve("refining".to_string()));
    match reason {
        "iterations" => {
            let mut more = resolve("in_progress".to_string());
            more["body"]["extra_tries"] = json!(2);
            vec![
                choice("Give 2 more tries", more),
                choice(
                    &format!("Ask {product_manager} to change the plan"),
                    resolve("refining".to_string()),
                ),
                cancel,
            ]
        }
        // Spec 5.7 raises these again on a resume, so only the plan's budget changing helps.
        "budget" | "sessions" => vec![change(), cancel],
        "blocker_age" | "permission" | "readiness_failures" | "explicit_request" => {
            let mut offered = Vec::new();
            if let Some(before) = before {
                offered.push(choice("Carry on", resolve(before.to_string())));
            }
            offered.extend([change(), cancel]);
            offered
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;

    use farik_core::contract::TaskStatus;
    use farik_protocol::event::EventKind;
    use serde_json::{Value, json};

    use super::choices;
    use crate::daemon::DaemonState;
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{Waited, command_handler};
    use crate::tools::fixtures::at;

    /// The reply frame to `method` with `params`, answered on a runtime of its own.
    pub(crate) fn rpc(state: &Arc<DaemonState>, method: &str, params: &Value) -> Value {
        let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime is made")
            .block_on(crate::daemon::web::answer(
                state,
                &frame.to_string(),
                &mut None,
            ))
    }

    /// The result of the query `name`, checked against `definition` of the RPC schema.
    pub(crate) fn query(
        state: &Arc<DaemonState>,
        name: &str,
        params: &Value,
        definition: &str,
    ) -> Value {
        let reply = rpc(state, "query", &json!({ "name": name, "params": params }));
        conforms(&reply["result"], definition, &reply);
        reply["result"].clone()
    }

    /// The result of the method `method`, checked against `definition` of the RPC schema.
    pub(crate) fn call(
        state: &Arc<DaemonState>,
        method: &str,
        params: &Value,
        definition: &str,
    ) -> Value {
        let reply = rpc(state, method, params);
        conforms(&reply["result"], definition, &reply);
        reply["result"].clone()
    }

    /// Checks `value` against `definition` of the RPC schema.
    fn conforms_to(value: &Value, definition: &str) {
        conforms(value, definition, value);
    }

    pub(crate) fn conforms(value: &Value, definition: &str, reply: &Value) {
        let schema: Value =
            serde_json::from_str(farik_protocol::rpc::SCHEMA_JSON).expect("the schema is JSON");
        let root = json!({
            "$schema": schema["$schema"],
            "$ref": format!("#/$defs/{definition}"),
            "$defs": schema["$defs"],
        });
        let validator = jsonschema::options()
            .build(&root)
            .expect("the definition compiles");
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{definition}: {errors:?} in {reply}");
    }

    /// A harness whose daemon takes commands through its orchestrator.
    pub(crate) fn driven(name: &str) -> Harness {
        let harness = Harness::new(name, |_| {});
        let orchestrator = Arc::new(harness.orchestrator(harness.recorded(Vec::new())));
        assert!(
            harness
                .daemon
                .set_command_handler(command_handler(orchestrator))
        );
        harness
    }

    /// Kai's plans MP-1 to MP-5 on FRK-1, the fixture's today being 2026-09-22: MP-1 approved and
    /// running, MP-2 waiting, MP-3 sent back, MP-4 approved and ended by the owner, MP-5 approved
    /// and not started.
    fn plans_in_every_state(harness: &Harness) {
        let project = &harness.project;
        project.plan_proposed("FRK-1", "MP-1", "2026-09-20", "2026-10-10");
        project.record(
            "FRK-1",
            "marketing_plan.approved",
            &json!({ "plan": "MP-1", "note": "Start small" }),
        );
        project.plan_proposed("FRK-1", "MP-2", "2026-10-20", "2026-11-10");
        project.plan_proposed("FRK-1", "MP-3", "2026-11-01", "2026-11-30");
        project.record(
            "FRK-1",
            "marketing_plan.returned",
            &json!({ "plan": "MP-3", "reason": "Too early." }),
        );
        project.plan_proposed("FRK-1", "MP-4", "2026-10-01", "2026-10-20");
        project.plan_approved("FRK-1", "MP-4", "");
        project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-4", "why": "by_owner", "note": "We close early." }),
        );
        project.plan_proposed("FRK-1", "MP-5", "2026-12-01", "2026-12-31");
        project.plan_approved("FRK-1", "MP-5", "");
        project.plan_proposed("FRK-1", "MP-6", "2026-12-10", "2026-12-20");
        project.plan_approved("FRK-1", "MP-6", "");
        project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-6", "why": "replaced", "replaced_by": "MP-5" }),
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one plan in every state, listed and then got, each answer read whole"
    )]
    fn lists_and_gets_plans() {
        let harness = Harness::new(
            "gates-plans",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        harness.in_progress("FRK-1", "kai", "pm");
        plans_in_every_state(&harness);

        let listed = query(
            &harness.daemon,
            "marketing_plan.list",
            &json!({}),
            "marketingPlanListResult",
        );

        let rows = listed["plans"].as_array().expect("a list of plans");
        let seen: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| {
                (
                    row["plan"].as_str().unwrap_or(""),
                    row["state"].as_str().unwrap_or(""),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("MP-6", "ended"),
                ("MP-5", "approved"),
                ("MP-4", "ended"),
                ("MP-3", "returned"),
                ("MP-2", "proposed"),
                ("MP-1", "active"),
            ],
            "newest first"
        );
        assert_eq!(
            rows[5],
            json!({
                "plan": "MP-1", "title": "Spring launch", "state": "active",
                "starts_on": "2026-09-20", "ends_on": "2026-10-10", "currency": "USD",
                "total": "2000.00", "agent_id": "kai", "task_id": "FRK-1",
                "proposed_at": at().to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            })
        );

        let get = |plan: &str| {
            query(
                &harness.daemon,
                "marketing_plan.get",
                &json!({ "plan": plan }),
                "marketingPlanGetResult",
            )
        };
        let running = get("MP-1");
        assert_eq!(running["state"], "active");
        assert_eq!(
            running["summary"],
            "Two weeks of posts and one small search campaign."
        );
        assert_eq!(running["text"], "x".repeat(300));
        assert_eq!(
            running["budget"],
            json!({ "total": "2000.00", "google_ads": "0.00" })
        );
        assert_eq!(
            running["measures"],
            json!(["New customers who say they found us online"])
        );
        assert_eq!(running["decided"]["decision"], "approved");
        assert_eq!(running["decided"]["note"], "Start small");
        assert_eq!(running["ended"], Value::Null);
        let waiting = get("MP-2");
        assert_eq!(waiting["state"], "proposed");
        assert_eq!(waiting["decided"], Value::Null);
        let sent_back = get("MP-3");
        assert_eq!(sent_back["decided"]["decision"], "returned");
        assert_eq!(sent_back["decided"]["reason"], "Too early.");
        assert!(sent_back["decided"].get("note").is_none(), "{sent_back}");
        let ended = get("MP-4");
        assert_eq!(ended["ended"]["why"], "by_owner");
        assert_eq!(
            ended["ended"]["note"], "We close early.",
            "the owner's words when they ended it"
        );
        assert!(ended["ended"].get("replaced_by").is_none(), "{ended}");
        let replaced = get("MP-6");
        assert_eq!(replaced["ended"]["why"], "replaced");
        assert_eq!(
            replaced["ended"]["replaced_by"], "MP-5",
            "the newer plan, named"
        );
        assert!(replaced["ended"].get("note").is_none(), "{replaced}");
        assert_eq!(ended["decided"]["decision"], "approved");
        assert!(
            ended["decided"].get("note").is_none(),
            "an empty note is no note"
        );

        let missing = rpc(
            &harness.daemon,
            "query",
            &json!({ "name": "marketing_plan.get", "params": { "plan": "MP-9" } }),
        );
        assert_eq!(
            missing["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{missing}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "plans proposed and approved side by side, each answer read whole"
    )]
    fn the_plan_says_each_campaign_s_price() {
        // The fixture clock reads 2026-09-22: a campaign made today starts two days ahead.
        let harness = Harness::new(
            "gates-plan-price",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        harness.in_progress("FRK-1", "kai", "pm");
        let propose = |plan: &str, campaigns: Value| {
            let mut body =
                farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
            body["plan"] = json!(plan);
            body["starts_on"] = json!("2026-09-20");
            body["ends_on"] = json!("2027-01-31");
            body["budget"] = json!({ "total": "2000.00", "google_ads": "1000.00" });
            body["posts"] = json!([]);
            body["campaigns"] = campaigns;
            body["google_ads_account"] = json!("123-456-7890");
            harness
                .project
                .record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &body);
        };
        let campaign = |key: &str, starts_on: &str, ends_on: &str, advertises: Option<&str>| {
            let mut wire = json!({
                "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
                "budget": "300.00", "starts_on": starts_on, "ends_on": ends_on
            });
            if let Some(advertises) = advertises {
                wire["advertises"] = json!(advertises);
            }
            wire
        };
        let get = |plan: &str| {
            query(
                &harness.daemon,
                "marketing_plan.get",
                &json!({ "plan": plan }),
                "marketingPlanGetResult",
            )
        };
        let shown = |plan: &Value| -> Vec<(String, String, String)> {
            plan["campaigns"]
                .as_array()
                .expect("campaigns")
                .iter()
                .map(|campaign| {
                    (
                        campaign["key"].as_str().unwrap_or("").to_string(),
                        campaign["advertises"].as_str().unwrap_or("?").to_string(),
                        campaign["price"].as_str().unwrap_or("?").to_string(),
                    )
                })
                .collect()
        };

        // While it waits, the price is as of today: from the 24th, three days are a total budget
        // (fixed), two a daily one (not fixed). A plan proposed before the field existed has no
        // words for it, and shows none.
        propose(
            "MP-1",
            json!([
                campaign(
                    "three",
                    "2026-09-23",
                    "2026-09-26",
                    Some("Handmade candles")
                ),
                campaign("two", "2026-09-23", "2026-09-25", Some("Gift boxes")),
                campaign("older", "2026-09-23", "2026-09-26", None),
            ]),
        );
        let waiting = get("MP-1");
        assert_eq!(
            shown(&waiting),
            [
                ("three".into(), "Handmade candles".into(), "fixed".into()),
                ("two".into(), "Gift boxes".into(), "not_fixed".into()),
                ("older".into(), String::new(), "fixed".into()),
            ]
        );

        // Once approved, as of the day the owner approved it: on the 18th this one runs from the
        // 22nd, four days and fixed, though today it would be two and daily.
        propose(
            "MP-2",
            json!([campaign(
                "shrinks",
                "2026-09-22",
                "2026-09-25",
                Some("Candle gifts")
            )]),
        );
        assert_eq!(
            shown(&get("MP-2"))[0].2,
            "not_fixed",
            "as of today while it waits"
        );
        let approved_on = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 9, 18, 9, 0, 0)
            .single()
            .expect("a time");
        harness.project.record_by(
            None,
            approved_on,
            "FRK-1",
            "marketing_plan.approved",
            &json!({ "plan": "MP-2", "note": "" }),
        );
        assert_eq!(
            shown(&get("MP-2")),
            [("shrinks".into(), "Candle gifts".into(), "fixed".into())],
            "as of the day it was approved"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_raised_version_prices_a_campaign_already_made_by_the_budget_it_has() {
        // The fixture clock reads 2026-09-22.
        let harness = Harness::new(
            "gates-plan-price-made",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        harness.in_progress("FRK-1", "kai", "pm");
        let propose = |plan: &str, replaces: Option<&str>, ends_on: &str| {
            let mut body =
                farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
            body["plan"] = json!(plan);
            body["starts_on"] = json!("2026-09-20");
            body["ends_on"] = json!("2027-01-31");
            body["budget"] = json!({ "total": "2000.00", "google_ads": "1000.00" });
            body["posts"] = json!([]);
            body["google_ads_account"] = json!("123-456-7890");
            if let Some(replaces) = replaces {
                body["replaces"] = json!(replaces);
            }
            let campaign = |key: &str| {
                json!({
                    "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
                    "advertises": "Handmade candles", "budget": "300.00",
                    "starts_on": "2026-09-24", "ends_on": ends_on
                })
            };
            body["campaigns"] = json!([campaign("long-run"), campaign("short-run")]);
            harness
                .project
                .record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &body);
        };
        let prices = |plan: &str| -> Vec<(String, String)> {
            query(
                &harness.daemon,
                "marketing_plan.get",
                &json!({ "plan": plan }),
                "marketingPlanGetResult",
            )["campaigns"]
                .as_array()
                .expect("campaigns")
                .iter()
                .map(|campaign| {
                    (
                        campaign["key"].as_str().unwrap_or("").to_string(),
                        campaign["price"].as_str().unwrap_or("?").to_string(),
                    )
                })
                .collect()
        };
        let made = |plan: &str, key: &str, number: u64, kind: &str| {
            harness.project.record_by(
                Some("kai"),
                at(),
                "FRK-1",
                "marketing_campaign.created",
                &json!({
                    "plan": plan, "key": key, "account": "123-456-7890",
                    "campaign": format!("customers/1234567890/campaigns/{number}"),
                    "budget": format!("customers/1234567890/campaignBudgets/{}", number + 100),
                    "budget_kind": kind, "amount": "300.00",
                }),
            );
        };

        // MP-1 runs 98 days (daily budgets) and Farik made `long-run` under it as a daily one;
        // `short-run` was never made.
        propose("MP-1", None, "2026-12-31");
        harness.project.plan_approved("FRK-1", "MP-1", "");
        made("MP-1", "long-run", 11, "daily");
        assert_eq!(
            prices("MP-1"),
            [
                ("long-run".into(), "not_fixed".into()),
                ("short-run".into(), "not_fixed".into())
            ],
            "98 days is a daily budget"
        );

        // MP-2 replaces it with the same keys over 30 days, a run that a new campaign would have
        // a total budget for. `long-run` is made already, and keeps the daily budget it has.
        propose("MP-2", Some("MP-1"), "2026-10-23");
        assert_eq!(
            prices("MP-2"),
            [
                ("long-run".into(), "not_fixed".into()),
                ("short-run".into(), "fixed".into())
            ],
            "the campaign that exists keeps the kind it was made with"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "the refusals one after another, then the request filed, then the second refused"
    )]
    fn raise_files_a_request_that_skips_triage() {
        use farik_core::marketing::{Amount, PlanSpend};

        use crate::daemon::SpendRead;

        let harness = Harness::new(
            "gates-raise",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        harness.in_progress("FRK-1", "kai", "pm");
        let project = &harness.project;
        // MP-1, active: 2000.00 in all, 800.00 for Google Ads, two campaigns.
        let mut body =
            farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
        body["plan"] = json!("MP-1");
        body["starts_on"] = json!("2026-09-20");
        body["ends_on"] = json!("2027-01-31");
        body["budget"] = json!({ "total": "2000.00", "google_ads": "800.00" });
        body["posts"] = json!([]);
        body["google_ads_account"] = json!("123-456-7890");
        let campaign = |key: &str, budget: &str| {
            json!({
                "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
                "budget": budget, "starts_on": "2026-09-22", "ends_on": "2026-12-22"
            })
        };
        body["campaigns"] = json!([
            campaign("search-launch", "500.00"),
            campaign("search-long", "400.00")
        ]);
        project.record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &body);
        project.plan_approved("FRK-1", "MP-1", "");
        let raise = |google_ads: &str, campaigns: &Value| {
            rpc(
                &harness.daemon,
                "marketing_budget.raise",
                &json!({ "plan": "MP-1", "google_ads": google_ads, "campaigns": campaigns }),
            )
        };
        let code = |reply: &Value| {
            reply["error"]["data"]["errors"][0]["code"]
                .as_str()
                .unwrap_or_else(|| panic!("a coded refusal: {reply}"))
                .to_string()
        };
        let launch = |budget: &str| json!({ "key": "search-launch", "budget": budget });

        // No budget reached yet: nothing to raise.
        assert_eq!(code(&raise("1200.00", &json!([]))), "raise_refused");

        // The plan and search-launch reached theirs, and Farik read 800.00 spent in all.
        project.record(
            "",
            "marketing_budget.reached",
            &json!({
                "plan": "MP-1", "scope": "plan", "spent": "800.00", "budget": "800.00",
                "currency": "USD", "paused": []
            }),
        );
        project.record(
            "",
            "marketing_budget.reached",
            &json!({
                "plan": "MP-1", "scope": "campaign", "key": "search-launch", "spent": "500.00",
                "budget": "500.00", "currency": "USD", "paused": []
            }),
        );
        harness.daemon.spend_reads().insert(
            "MP-1".to_string(),
            SpendRead {
                attempted_at: at(),
                spend: Some((
                    PlanSpend {
                        by_key: [
                            ("search-launch".to_string(), Amount(50_000)),
                            ("search-long".to_string(), Amount(30_000)),
                        ]
                        .into(),
                        total: Amount(80_000),
                    },
                    at(),
                )),
                failed: None,
                unstopped: None,
            },
        );
        // The same campaign reached its budget under a plan that ended, MP-0.
        project.record(
            "",
            "marketing_budget.reached",
            &json!({
                "plan": "MP-0", "scope": "campaign", "key": "search-launch", "spent": "500.00",
                "budget": "500.00", "currency": "USD", "paused": []
            }),
        );
        let before = project.event_count();

        // A plan that is not the one running now is refused, whatever it reached.
        let reply = rpc(
            &harness.daemon,
            "marketing_budget.raise",
            &json!({ "plan": "MP-0", "google_ads": "1200.00", "campaigns": [launch("700.00")] }),
        );
        assert_eq!(code(&reply), "raise_refused", "{reply}");

        // Each of these is refused, and nothing is filed: Google Ads not above what is spent, a
        // campaign not above its own spend, the campaigns above the Google Ads budget, the
        // campaign at its cap left out, one that is not at its cap offered, one named twice.
        for (google_ads, campaigns) in [
            ("800.00", json!([launch("700.00")])),
            ("1200.00", json!([launch("500.00")])),
            ("1000.00", json!([launch("700.00")])),
            ("1200.00", json!([])),
            (
                "1200.00",
                json!([launch("700.00"), { "key": "search-long", "budget": "450.00" }]),
            ),
            ("1200.00", json!([launch("700.00"), launch("710.00")])),
        ] {
            let reply = raise(google_ads, &campaigns);
            assert_eq!(
                code(&reply),
                "raise_refused",
                "{google_ads} {campaigns}: {reply}"
            );
        }
        assert_eq!(project.event_count(), before, "nothing was filed");

        // The raise: 1200.00 for Google Ads, so the total rises by the 400.00 from 2000.00.
        let reply = raise("1200.00", &json!([launch("700.00")]));
        let task = reply["result"]["task_id"]
            .as_str()
            .unwrap_or_else(|| panic!("a task: {reply}"))
            .to_string();
        let contract = project
            .deps
            .files
            .read_contract(&task.parse().expect("a task id"))
            .expect("the request is a file");
        assert_eq!(
            contract.title.to_string(),
            "New version of MP-1 with a raised budget"
        );
        assert_eq!(
            contract.intent.as_str(),
            "Propose a new version of MP-1 that replaces it (replaces: MP-1), starting today, \
             with its Google Ads budget 1200.00 USD and its total 2400.00, and search-launch's \
             budget 700.00; keep its campaigns and post slots that are not over, each with its \
             key and what it advertises, their dates from today on. Once the owner approves it, \
             raise each paused campaign's budget at Google with set_campaign_budget, then enable \
             it."
        );
        let events = project.events(&[EventKind::TaskCreated, EventKind::RequestTriaged]);
        let filed: Vec<&farik_protocol::event::FarikEvent> = events
            .iter()
            .filter(|event| {
                event.envelope.ids.task_id.as_ref().map(|id| id.as_str()) == Some(task.as_str())
            })
            .collect();
        assert_eq!(filed.len(), 2, "filed, then triaged at once");
        let farik_protocol::event::EventBody::TaskCreated(created) = &filed[0].body else {
            panic!("a task.created first");
        };
        assert_eq!(created.created_by, "human");
        assert_eq!(
            created.raises.as_ref().map(|plan| plan.as_str()),
            Some("MP-1")
        );
        let farik_protocol::event::EventBody::RequestTriaged(triaged) = &filed[1].body else {
            panic!("a request.triaged next");
        };
        assert_eq!(
            (
                triaged.size.to_string().as_str(),
                triaged.reason.as_str(),
                triaged.triaged_by.as_str()
            ),
            ("small", "a raised marketing budget for MP-1", "farik")
        );
        let row = harness.row(&task);
        assert!(row.skips_sprints && row.triaged, "{row:?}");

        // While it is open, no second one.
        let reply = raise("1200.00", &json!([launch("700.00")]));
        assert_eq!(code(&reply), "raise_open", "{reply}");
        assert_eq!(
            project.events(&[EventKind::TaskCreated]).len(),
            2,
            "FRK-1 and the raise"
        );

        // Once the owner cancels it, it is not open any more: another may be asked.
        project.moved(&task, "draft", "cancelled", &json!({ "actor": "human" }));
        let reply = raise("1200.00", &json!([launch("700.00")]));
        let second = reply["result"]["task_id"]
            .as_str()
            .unwrap_or_else(|| panic!("a task: {reply}"))
            .to_string();
        assert_eq!(project.events(&[EventKind::TaskCreated]).len(), 3);

        // Nor once it is accepted, the new version being written.
        project.moved(&second, "draft", "accepted", &json!({ "actor": "human" }));
        let reply = raise("1200.00", &json!([launch("700.00")]));
        assert!(reply["result"]["task_id"].is_string(), "{reply}");
        assert_eq!(project.events(&[EventKind::TaskCreated]).len(), 4);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waiting_lists_a_plan_to_approve() {
        let harness = Harness::new("gates-plan-waits", |wire| {
            crate::tools::fixtures::with_the_marketing_specialist(wire);
            wire["agents"][3]["display_name"] = json!("Kai");
        });
        harness.in_progress("FRK-1", "kai", "pm");
        harness
            .project
            .plan_proposed("FRK-1", "MP-1", "2026-09-22", "2026-10-20");
        let waiting = || {
            query(
                &harness.daemon,
                "waiting.list",
                &json!({}),
                "waitingListResult",
            )["waiting"]
                .clone()
        };

        assert_eq!(
            waiting(),
            json!([{
                "task_id": "FRK-1", "kind": "marketing_plan", "agent_id": "kai",
                "title": "Spring launch",
                "line": "Kai proposes a marketing plan: Spring launch",
                "plan": "MP-1",
                "summary": "Two weeks of posts and one small search campaign.",
                "total": "2000.00", "currency": "USD",
                "starts_on": "2026-09-22", "ends_on": "2026-10-20"
            }])
        );

        harness.project.plan_approved("FRK-1", "MP-1", "");
        assert_eq!(waiting(), json!([]), "gone once decided");
    }

    const LAUNCH: &str = "customers/1234567890/campaigns/11";
    const LONG: &str = "customers/1234567890/campaigns/12";
    const UNAVAILABLE: &str = "Google answered “The service is currently unavailable.”";

    /// MP-1, running today: 2000.00 in all, 800.00 for Google Ads, and two campaigns Farik made at
    /// Google, `search-launch` (500.00) and `search-long` (400.00), on FRK-1 by Kai.
    fn a_running_ads_plan(harness: &Harness) {
        harness.in_progress("FRK-1", "kai", "pm");
        let project = &harness.project;
        let mut body =
            farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
        body["plan"] = json!("MP-1");
        body["starts_on"] = json!("2026-09-20");
        body["ends_on"] = json!("2027-01-31");
        body["budget"] = json!({ "total": "2000.00", "google_ads": "800.00" });
        body["posts"] = json!([]);
        body["google_ads_account"] = json!("123-456-7890");
        let campaign = |key: &str, budget: &str| {
            json!({
                "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
                "advertises": "Candles", "budget": budget, "starts_on": "2026-09-22",
                "ends_on": "2026-12-22"
            })
        };
        body["campaigns"] = json!([
            campaign("search-launch", "500.00"),
            campaign("search-long", "400.00")
        ]);
        project.record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &body);
        project.plan_approved("FRK-1", "MP-1", "");
        for (key, campaign, budget) in [
            ("search-launch", LAUNCH, "500.00"),
            ("search-long", LONG, "400.00"),
        ] {
            project.record(
                "",
                "marketing_campaign.created",
                &json!({
                    "plan": "MP-1", "key": key, "account": "123-456-7890",
                    "campaign": campaign,
                    "budget": campaign.replace("campaigns", "campaignBudgets"),
                    "budget_kind": "total", "amount": budget
                }),
            );
        }
    }

    /// The `waiting.list` rows of `kind`s that are about a plan's ads.
    fn ads_rows(harness: &Harness) -> Vec<Value> {
        query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        )["waiting"]
            .as_array()
            .expect("a list")
            .iter()
            .filter(|row| {
                row["kind"]
                    .as_str()
                    .is_some_and(|kind| kind.starts_with("marketing_") && kind != "marketing_plan")
            })
            .cloned()
            .collect()
    }

    fn reached(scope: &str, key: Option<&str>, (spent, budget): (&str, &str)) -> Value {
        let mut body = json!({
            "plan": "MP-1", "scope": scope, "spent": spent, "budget": budget,
            "currency": "USD", "paused": [],
        });
        if let Some(key) = key {
            body["key"] = json!(key);
        }
        body
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "the row in each of its forms, one after another, until the plan ends"
    )]
    fn waiting_lists_a_reached_budget_until_the_plan_ends() {
        let harness = Harness::new(
            "gates-budget-waits",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        a_running_ads_plan(&harness);
        let project = &harness.project;
        let kinds = || -> Vec<String> {
            query(
                &harness.daemon,
                "waiting.list",
                &json!({}),
                "waitingListResult",
            )["waiting"]
                .as_array()
                .expect("a list")
                .iter()
                .map(|row| row["kind"].as_str().unwrap_or_default().to_string())
                .collect()
        };
        // Another plan waits for the owner, and the row about the money comes before it.
        project.plan_proposed("FRK-1", "MP-2", "2027-02-01", "2027-02-28");
        assert!(ads_rows(&harness).is_empty(), "nothing reached yet");

        // search-launch reached its own budget, and Farik paused it.
        let mut cap = reached("campaign", Some("search-launch"), ("500.00", "500.00"));
        cap["paused"] = json!([LAUNCH]);
        project.record("", "marketing_budget.reached", &cap);
        let launch_row = json!({
            "task_id": "FRK-1", "kind": "marketing_budget", "agent_id": "kai",
            "title": "Ads budget reached: Spring launch",
            "line": "Its campaign search-launch reached its budget: 500.00 of 500.00 USD. \
                     Farik paused it.",
            "plan": "MP-1", "plan_title": "Spring launch", "ends_on": "2027-01-31",
            "currency": "USD", "google_ads": "800.00", "spent": "500.00",
            "cap": {
                "scope": "campaign", "key": "search-launch", "name": "search-launch",
                "spent": "500.00", "budget": "500.00"
            },
            "caps": [{
                "scope": "campaign", "key": "search-launch", "name": "search-launch",
                "spent": "500.00", "budget": "500.00"
            }],
            "campaigns": [
                { "key": "search-launch", "name": "search-launch", "budget": "500.00", "spent": "500.00" },
                { "key": "search-long", "name": "search-long", "budget": "400.00", "spent": "0.00" }
            ],
        });
        assert_eq!(ads_rows(&harness), [launch_row]);
        assert_eq!(kinds(), ["marketing_budget", "marketing_plan"]);

        // Then the plan's own, and Google would not take the pause of search-long (search-launch
        // was already paused, which counts as paused).
        let mut cap = reached("plan", None, ("800.00", "800.00"));
        cap["paused"] = json!([LAUNCH]);
        cap["failed"] = json!(UNAVAILABLE);
        project.record("", "marketing_budget.reached", &cap);
        let rows = ads_rows(&harness);
        assert_eq!(rows.len(), 1, "one row for the plan");
        assert_eq!(
            rows[0]["line"],
            "Its ads reached their budget: 800.00 of 800.00 USD. Farik could not pause them: \
             Google answered “The service is currently unavailable.” Farik tries again every 15 \
             minutes; pause them in Google Ads."
        );
        assert_eq!(rows[0]["reason"], UNAVAILABLE);
        assert_eq!(rows[0]["spent"], "800.00");
        assert_eq!(rows[0]["caps"].as_array().map(Vec::len), Some(2));
        // The line is about the plan's own cap, and the row says which cap that is.
        assert_eq!(
            rows[0]["cap"],
            json!({ "scope": "plan", "spent": "800.00", "budget": "800.00" })
        );

        // A later read paused it: back to the form that Farik paused it.
        project.record(
            "",
            "marketing_campaign.paused",
            &json!({ "plan": "MP-1", "key": "search-long", "campaign": LONG, "why": "budget_reached" }),
        );
        let rows = ads_rows(&harness);
        assert_eq!(
            rows[0]["line"],
            "Its ads reached their budget: 800.00 of 800.00 USD. Farik paused them."
        );
        assert!(rows[0].get("reason").is_none(), "{}", rows[0]);
        assert!(rows[0].get("raising").is_none());

        // Google Ads was removed and the pause was refused: the budget row says so, and is the
        // plan's one row.
        harness.daemon.spend_reads().insert(
            "MP-1".to_string(),
            crate::daemon::SpendRead {
                attempted_at: at(),
                spend: None,
                failed: None,
                unstopped: Some(UNAVAILABLE.to_string()),
            },
        );
        let rows = ads_rows(&harness);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["kind"], "marketing_budget");
        assert_eq!(rows[0]["reason"], UNAVAILABLE);
        assert!(
            rows[0]["line"]
                .as_str()
                .is_some_and(|line| line.contains("Farik could not pause them: Google answered")),
            "{}",
            rows[0]
        );
        harness.daemon.spend_reads().clear();

        // The owner asked for a new version with a raised budget: it is told, and waits.
        let reply = rpc(
            &harness.daemon,
            "marketing_budget.raise",
            &json!({
                "plan": "MP-1", "google_ads": "1200.00",
                "campaigns": [{ "key": "search-launch", "budget": "700.00" }]
            }),
        );
        let task = reply["result"]["task_id"]
            .as_str()
            .unwrap_or_else(|| panic!("a task: {reply}"))
            .to_string();
        assert_eq!(ads_rows(&harness)[0]["raising"], task);

        // Once the new version is written the request is done, and nothing is left to wait for.
        project.moved(&task, "draft", "accepted", &json!({ "actor": "human" }));
        assert!(ads_rows(&harness)[0].get("raising").is_none());

        // The plan ends, and the row goes with it.
        project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-1", "why": "by_owner" }),
        );
        assert!(ads_rows(&harness).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waiting_says_when_ads_keep_running() {
        use crate::daemon::SpendRead;

        let harness = Harness::new(
            "gates-ads-running",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        a_running_ads_plan(&harness);
        let project = &harness.project;
        let unread = |failed: Option<&str>, unstopped: Option<&str>| SpendRead {
            attempted_at: at(),
            spend: None,
            failed: failed.map(|why| (why.to_string(), at())),
            unstopped: unstopped.map(str::to_string),
        };

        // A plan the owner ended whose ads Google would not pause.
        project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-1", "why": "by_owner" }),
        );
        harness
            .daemon
            .spend_reads()
            .insert("MP-1".to_string(), unread(None, Some(UNAVAILABLE)));
        assert_eq!(
            ads_rows(&harness),
            [json!({
                "task_id": "FRK-1", "kind": "marketing_ads_running", "agent_id": "kai",
                "title": "Ads still running: Spring launch",
                "line": "Farik could not pause its ads: Google answered “The service is currently \
                         unavailable.” They keep running at Google until 2027-01-31 or their \
                         budget there. Pause them in Google Ads.",
                "plan": "MP-1", "plan_title": "Spring launch", "ends_on": "2027-01-31",
                "currency": "USD", "reason": UNAVAILABLE,
            })]
        );

        // Once a later pause worked, nothing is left to say.
        harness
            .daemon
            .spend_reads()
            .insert("MP-1".to_string(), unread(None, None));
        assert!(ads_rows(&harness).is_empty());

        // The active plan, whose Google Ads was removed and could not be paused first: the row
        // that says its ads run is in place of the one about the spend it cannot read.
        project.plan_proposed("FRK-1", "MP-2", "2026-09-22", "2026-12-31");
        project.plan_approved("FRK-1", "MP-2", "");
        harness.daemon.spend_reads().insert(
            "MP-2".to_string(),
            unread(
                Some("no Marketing Specialist on the team has Google Ads connected"),
                Some(UNAVAILABLE),
            ),
        );
        let rows = ads_rows(&harness);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["kind"], "marketing_ads_running");
        assert_eq!(rows[0]["plan"], "MP-2");

        // After a restart nothing is remembered, and the first read finds no connection: the
        // active plan's row still says that its ads keep running.
        harness.daemon.spend_reads().clear();
        harness.daemon.spend_reads().insert(
            "MP-2".to_string(),
            unread(
                Some("no Marketing Specialist on the team has Google Ads connected"),
                None,
            ),
        );
        let rows = ads_rows(&harness);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["kind"], "marketing_spend_unread");
        assert!(
            rows[0]["line"]
                .as_str()
                .is_some_and(|line| line.contains("keep running at Google until 2026-12-31")),
            "{}",
            rows[0]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waiting_says_when_the_spend_cannot_be_read() {
        use farik_core::marketing::{Amount, PlanSpend};

        use crate::daemon::SpendRead;

        let harness = Harness::new(
            "gates-spend-unread",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        a_running_ads_plan(&harness);
        let sign_in = "Kai's sign-in to Google has ended; sign Kai in again on Kai's page";
        let earlier = at() - chrono::Duration::minutes(30);
        let read = |failed: Option<&str>| SpendRead {
            attempted_at: at(),
            spend: Some((
                PlanSpend {
                    by_key: [
                        ("search-launch".to_string(), Amount(15_000)),
                        ("search-long".to_string(), Amount(16_865)),
                    ]
                    .into(),
                    total: Amount(31_865),
                },
                earlier,
            )),
            failed: failed.map(|why| (why.to_string(), at())),
            unstopped: None,
        };
        harness
            .daemon
            .spend_reads()
            .insert("MP-1".to_string(), read(Some(sign_in)));
        assert_eq!(
            ads_rows(&harness),
            [json!({
                "task_id": "FRK-1", "kind": "marketing_spend_unread", "agent_id": "kai",
                "title": "Can't read the ad spend: Spring launch",
                "line": "Farik can't read its ad spend: Kai's sign-in to Google has ended; sign \
                         Kai in again on Kai's page. Any of its ads still running keep running at \
                         Google until 2027-01-31 or their budget there; pause them in Google Ads.",
                "plan": "MP-1", "plan_title": "Spring launch", "ends_on": "2027-01-31",
                "currency": "USD", "google_ads": "800.00", "reason": sign_in,
                "spent": "318.65", "read_at": "2026-09-22T11:30:00Z",
            })]
        );

        // A read that worked, and the row goes.
        harness
            .daemon
            .spend_reads()
            .insert("MP-1".to_string(), read(None));
        assert!(ads_rows(&harness).is_empty());

        // A plan that ended is not read any more, and a failure kept from before says nothing.
        harness
            .daemon
            .spend_reads()
            .insert("MP-1".to_string(), read(Some(sign_in)));
        harness.project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-1", "why": "by_owner" }),
        );
        assert!(ads_rows(&harness).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "a raised plan, its caps and the pauses before them, one after another"
    )]
    fn waiting_is_not_settled_by_a_pause_before_the_cap() {
        let harness = Harness::new(
            "gates-budget-before",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        a_running_ads_plan(&harness);
        let project = &harness.project;
        // MP-1 reached its budget and its campaigns were paused: both recorded.
        let mut cap = reached("plan", None, ("800.00", "800.00"));
        cap["paused"] = json!([LAUNCH, LONG]);
        project.record("", "marketing_budget.reached", &cap);
        for (key, campaign) in [("search-launch", LAUNCH), ("search-long", LONG)] {
            project.record(
                "",
                "marketing_campaign.paused",
                &json!({ "plan": "MP-1", "key": key, "campaign": campaign, "why": "budget_reached" }),
            );
        }
        // The owner raised it: MP-2 replaces MP-1, keeps both campaigns, and ends MP-1.
        let mut body =
            farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
        body["plan"] = json!("MP-2");
        body["replaces"] = json!("MP-1");
        body["starts_on"] = json!("2026-09-22");
        body["ends_on"] = json!("2027-01-31");
        body["budget"] = json!({ "total": "3000.00", "google_ads": "1200.00" });
        body["posts"] = json!([]);
        body["google_ads_account"] = json!("123-456-7890");
        body["campaigns"] = json!(["search-launch", "search-long"].map(|key| json!({
            "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
            "advertises": "Candles", "budget": "600.00", "starts_on": "2026-09-22",
            "ends_on": "2026-12-22"
        })));
        project.record_by(Some("kai"), at(), "FRK-1", "marketing_plan.proposed", &body);
        project.plan_approved("FRK-1", "MP-2", "");
        project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-1", "why": "replaced", "replaced_by": "MP-2" }),
        );
        // MP-1 is ended, with its ads paused: no row. Then MP-2 reached its own, and Google
        // refused: the pauses before do not count for it.
        assert!(ads_rows(&harness).is_empty());
        let two = |scope: &str, failed: Option<&str>| {
            let mut cap = reached(scope, None, ("1200.00", "1200.00"));
            cap["plan"] = json!("MP-2");
            if let Some(failed) = failed {
                cap["failed"] = json!(failed);
            }
            cap
        };
        project.record(
            "",
            "marketing_budget.reached",
            &two("plan", Some(UNAVAILABLE)),
        );
        let rows = ads_rows(&harness);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0]["plan"], "MP-2");
        assert_eq!(rows[0]["reason"], UNAVAILABLE);

        // A cap after it that no refusal came of does not say the first one's campaigns are paused.
        project.record("", "marketing_budget.reached", &two("plan", None));
        assert_eq!(ads_rows(&harness)[0]["reason"], UNAVAILABLE);

        // MP-1, which ended first, has ads Farik could not pause: its row follows the active plan's.
        harness.daemon.spend_reads().insert(
            "MP-1".to_string(),
            crate::daemon::SpendRead {
                attempted_at: at(),
                spend: None,
                failed: None,
                unstopped: Some(UNAVAILABLE.to_string()),
            },
        );
        let rows = ads_rows(&harness);
        assert_eq!(
            rows.iter()
                .map(|row| (row["plan"].as_str(), row["kind"].as_str()))
                .collect::<Vec<_>>(),
            [
                (Some("MP-2"), Some("marketing_budget")),
                (Some("MP-1"), Some("marketing_ads_running"))
            ]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_row_names_the_cap_its_words_are_about() {
        let harness = Harness::new(
            "gates-budget-cap",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        a_running_ads_plan(&harness);
        let project = &harness.project;
        // search-launch reached its own budget and Google refused the pause; then the plan's own
        // was reached and its pause worked for the other campaign only. The refusal still stands,
        // so the row speaks of search-launch, with Google's words, and says it is that cap.
        let mut cap = reached("campaign", Some("search-launch"), ("500.00", "500.00"));
        cap["failed"] = json!(UNAVAILABLE);
        project.record("", "marketing_budget.reached", &cap);
        let mut cap = reached("plan", None, ("800.00", "800.00"));
        cap["paused"] = json!([LONG]);
        project.record("", "marketing_budget.reached", &cap);

        let rows = ads_rows(&harness);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(
            rows[0]["cap"],
            json!({
                "scope": "campaign", "key": "search-launch", "name": "search-launch",
                "spent": "500.00", "budget": "500.00"
            })
        );
        assert_eq!(rows[0]["reason"], UNAVAILABLE);
        assert!(
            rows[0]["line"]
                .as_str()
                .is_some_and(|line| line.starts_with("Its campaign search-launch reached")),
            "{}",
            rows[0]
        );
        assert_eq!(rows[0]["caps"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_plan_carries_its_spend_and_pauses() {
        use farik_core::marketing::{Amount, PlanSpend};

        use crate::daemon::SpendRead;

        let harness = Harness::new(
            "gates-plan-spend",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        a_running_ads_plan(&harness);
        let project = &harness.project;
        let get = || {
            query(
                &harness.daemon,
                "marketing_plan.get",
                &json!({ "plan": "MP-1" }),
                "marketingPlanGetResult",
            )
        };

        // Nothing is known of the spend, and nothing was reached or paused.
        let plan = get();
        assert!(plan.get("spend").is_none(), "{plan}");
        assert_eq!(
            (&plan["reached"], &plan["paused"]),
            (&json!([]), &json!([]))
        );

        // A read worked, and a later one did not.
        let earlier = at() - chrono::Duration::minutes(15);
        harness.daemon.spend_reads().insert(
            "MP-1".to_string(),
            SpendRead {
                attempted_at: at(),
                spend: Some((
                    PlanSpend {
                        by_key: [
                            ("search-launch".to_string(), Amount(50_000)),
                            ("search-long".to_string(), Amount(1_000)),
                        ]
                        .into(),
                        total: Amount(51_000),
                    },
                    earlier,
                )),
                failed: Some(("Google is down".to_string(), at())),
                unstopped: None,
            },
        );
        assert_eq!(
            get()["spend"],
            json!({
                "read_at": "2026-09-22T11:45:00Z", "total": "510.00",
                "by_key": { "search-launch": "500.00", "search-long": "10.00" },
                "failed": "Google is down", "failed_at": "2026-09-22T12:00:00Z",
            })
        );

        // One campaign was paused for the plan's end; later the other reached its budget and was
        // paused, and the plan's own budget lists it again.
        let minutes = |minutes: i64| at() + chrono::Duration::minutes(minutes);
        project.record_at(
            minutes(5),
            "",
            "marketing_campaign.paused",
            &json!({ "plan": "MP-1", "key": "search-long", "campaign": LONG, "why": "plan_ended" }),
        );
        let mut cap = reached("campaign", Some("search-launch"), ("500.00", "500.00"));
        cap["paused"] = json!([LAUNCH]);
        cap["failed"] = json!("Google answered “No.”");
        project.record_at(minutes(20), "", "marketing_budget.reached", &cap);
        let mut cap = reached("plan", None, ("800.00", "800.00"));
        cap["paused"] = json!([LAUNCH]);
        project.record_at(minutes(30), "", "marketing_budget.reached", &cap);
        let plan = get();
        assert_eq!(
            plan["reached"],
            json!([
                {
                    "scope": "campaign", "key": "search-launch", "spent": "500.00",
                    "budget": "500.00", "failed": "Google answered “No.”",
                    "at": "2026-09-22T12:20:00Z",
                },
                {
                    "scope": "plan", "spent": "800.00", "budget": "800.00",
                    "at": "2026-09-22T12:30:00Z",
                },
            ])
        );
        assert_eq!(
            plan["paused"],
            json!([
                { "key": "search-long", "name": "search-long", "why": "plan_ended", "at": "2026-09-22T12:05:00Z" },
                { "key": "search-launch", "name": "search-launch", "why": "budget_reached", "at": "2026-09-22T12:20:00Z" },
            ])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waiting_lists_a_requested_post() {
        let harness = Harness::new("gates-post-waits", |wire| {
            crate::tools::fixtures::with_the_marketing_specialist(wire);
            wire["agents"][3]["display_name"] = json!("Kai");
        });
        harness.in_progress("FRK-1", "kai", "pm");
        let request = json!({
            "channel": "instagram", "buffer_channel": "chan-1",
            "text": "We open on Wednesday.",
            "media": [{ "url": "https://example.com/a.png", "kind": "image" }],
            "at": "2026-09-22T14:00:00+02:00",
        });
        let post = harness
            .project
            .record_by(
                Some("kai"),
                crate::tools::fixtures::at(),
                "FRK-1",
                "social_post.requested",
                &request,
            )
            .envelope
            .seq;
        let waiting = || {
            query(
                &harness.daemon,
                "waiting.list",
                &json!({}),
                "waitingListResult",
            )["waiting"]
                .clone()
        };

        assert_eq!(
            waiting(),
            json!([{
                "task_id": "FRK-1", "kind": "social_post", "agent_id": "kai",
                "title": waiting()[0]["title"],
                "line": "Kai wants to post on Instagram",
                "post": post, "channel": "instagram",
                "text": "We open on Wednesday.",
                "media": [{ "url": "https://example.com/a.png", "kind": "image" }],
                "at": "2026-09-22T14:00:00+02:00",
            }])
        );

        harness.project.record(
            "",
            "social_post.stopped",
            &json!({ "post": post, "by": "declined" }),
        );
        assert_eq!(waiting(), json!([]), "gone once decided");
    }

    /// Kai's harness, with a task FRK-1 she works on.
    fn kai_working(name: &str) -> Harness {
        let harness = Harness::new(name, |wire| {
            crate::tools::fixtures::with_the_marketing_specialist(wire);
            wire["agents"][3]["display_name"] = json!("Kai");
        });
        harness.in_progress("FRK-1", "kai", "pm");
        harness
    }

    /// Kai schedules a post in plan MP-1, recorded at `when`; answers its number.
    fn kai_wrote(
        harness: &Harness,
        when: chrono::DateTime<chrono::Utc>,
        (slot, going_out): (&str, &str),
        media: &Value,
    ) -> u64 {
        harness
            .project
            .record_by(
                Some("kai"),
                when,
                "FRK-1",
                "social_post.scheduled",
                &json!({
                    "channel": "instagram", "buffer_channel": "chan-1",
                    "text": format!("Post for {slot}"), "media": media,
                    "at": going_out, "approved_by": "plan", "plan": "MP-1", "slot": slot,
                }),
            )
            .envelope
            .seq
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "a post in every state, then the list"
    )]
    fn social_posts_list_answers_what_goes_out() {
        let harness = kai_working("gates-posts-list");
        let one = json!([{ "url": "https://example.com/a.png", "kind": "image" }]);
        let hours = |hours: i64| at() + chrono::Duration::hours(hours);
        let then = |when: chrono::DateTime<chrono::Utc>, task: &str, kind: &str, body: Value| {
            harness.project.record_at(when, task, kind, &body)
        };

        // A failure of more than a day ago, and one of this morning.
        let old = kai_wrote(
            &harness,
            hours(-26),
            ("post-0", "2026-09-21T09:00:00Z"),
            &one,
        );
        then(
            hours(-25),
            "",
            "social_post.failed",
            json!({ "post": old, "reason": "Long ago." }),
        );
        let failed = kai_wrote(
            &harness,
            hours(-5),
            ("post-1", "2026-09-22T08:00:00Z"),
            &one,
        );
        then(
            hours(-2),
            "",
            "social_post.failed",
            json!({ "post": failed, "reason": "Buffer did not take it: \u{201c}No\u{201d}" }),
        );
        // Going out: later today in another offset (18:00 UTC), already with Buffer (14:00 UTC).
        // The strings sort the other way round.
        let later = kai_wrote(
            &harness,
            at(),
            ("post-2", "2026-09-22T13:00:00-05:00"),
            &one,
        );
        let sent = kai_wrote(
            &harness,
            at(),
            ("post-3", "2026-09-22T16:00:00+02:00"),
            &one,
        );
        then(
            at(),
            "",
            "social_post.sent",
            json!({ "post": sent, "buffer_post": "buf-1" }),
        );
        // A request the owner allowed at once: 12:30 UTC.
        let request = harness
            .project
            .record_by(
                Some("kai"),
                at(),
                "FRK-1",
                "social_post.requested",
                &json!({
                    "channel": "x", "buffer_channel": "chan-2", "text": "Outside the plan.",
                    "media": [], "at": "2026-09-22T12:30:00Z",
                }),
            )
            .envelope
            .seq;
        then(
            at(),
            "FRK-1",
            "social_post.scheduled",
            json!({
                "post": request, "channel": "x", "buffer_channel": "chan-2",
                "text": "Outside the plan.", "media": [], "at": "2026-09-22T12:30:00Z",
                "approved_by": "owner",
            }),
        );
        // What is not listed: a post whose time has passed, one stopped, one still asked about.
        kai_wrote(&harness, at(), ("post-4", "2026-09-22T11:00:00Z"), &one);
        let stopped = kai_wrote(&harness, at(), ("post-5", "2026-09-22T20:00:00Z"), &one);
        then(
            at(),
            "",
            "social_post.stopped",
            json!({ "post": stopped, "by": "owner" }),
        );
        harness.project.record_by(
            Some("kai"),
            at(),
            "FRK-1",
            "social_post.requested",
            &json!({
                "channel": "x", "buffer_channel": "chan-2", "text": "Waiting.",
                "media": [], "at": "2026-09-23T12:30:00Z",
            }),
        );
        // Missed just now.
        let missed = kai_wrote(&harness, at(), ("post-6", "2026-09-22T12:40:00Z"), &one);
        then(
            at(),
            "",
            "social_post.missed",
            json!({ "post": missed, "why": "paused" }),
        );

        let listed = query(
            &harness.daemon,
            "social_posts.list",
            &json!({}),
            "socialPostsListResult",
        );

        let rows = listed["posts"].as_array().expect("a list");
        let numbers: Vec<u64> = rows
            .iter()
            .map(|row| row["post"].as_u64().expect("n"))
            .collect();
        assert_eq!(
            numbers,
            [request, sent, later, missed, failed],
            "going out soonest first, then what did not go out, the latest first: {listed}"
        );
        assert_eq!(
            rows[0],
            json!({
                "post": request, "agent_id": "kai", "channel": "x", "text": "Outside the plan.",
                "media": [], "at": "2026-09-22T12:30:00Z",
                "hands_over_at": "2026-09-22T12:00:00Z", "state": "scheduled",
                "approved_by": "owner",
            }),
            "an allowed request is handed over when it was allowed, no later than its hour"
        );
        assert_eq!(
            rows[1],
            json!({
                "post": sent, "agent_id": "kai", "channel": "instagram", "text": "Post for post-3",
                "media": one, "at": "2026-09-22T16:00:00+02:00",
                "hands_over_at": "2026-09-22T13:00:00Z", "state": "sent",
                "plan": "MP-1", "slot": "post-3", "approved_by": "plan",
            })
        );
        assert_eq!(rows[3]["state"], "missed");
        assert_eq!(rows[3]["missed_why"], "paused");
        assert!(rows[3].get("reason").is_none(), "{}", rows[3]);
        assert_eq!(rows[4]["state"], "failed");
        assert_eq!(
            rows[4]["reason"],
            "Buffer did not take it: \u{201c}No\u{201d}"
        );
        assert!(rows[4].get("missed_why").is_none(), "{}", rows[4]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_plan_page_lists_its_posts() {
        let harness = kai_working("gates-plan-posts");
        harness
            .project
            .plan_proposed("FRK-1", "MP-1", "2026-09-22", "2026-10-20");
        harness.project.plan_approved("FRK-1", "MP-1", "");
        let none = json!([]);
        let then = |kind: &str, body: Value| harness.project.record("", kind, &body);

        // Slot post-1 failed first and was written again and sent; post-2 was stopped by the
        // owner, post-3 missed, and post-4 stopped with the plan.
        let failed = kai_wrote(&harness, at(), ("post-1", "2026-09-23T09:00:00Z"), &none);
        then(
            "social_post.failed",
            json!({ "post": failed, "reason": "No." }),
        );
        let sent = kai_wrote(&harness, at(), ("post-1", "2026-09-23T10:00:00Z"), &none);
        then(
            "social_post.sent",
            json!({ "post": sent, "buffer_post": "buf-1" }),
        );
        let stopped = kai_wrote(&harness, at(), ("post-2", "2026-09-24T09:00:00Z"), &none);
        then(
            "social_post.stopped",
            json!({ "post": stopped, "by": "owner" }),
        );
        let missed = kai_wrote(&harness, at(), ("post-3", "2026-09-25T09:00:00Z"), &none);
        then(
            "social_post.missed",
            json!({ "post": missed, "why": "not_running" }),
        );
        let ended = kai_wrote(&harness, at(), ("post-4", "2026-09-26T09:00:00Z"), &none);
        then(
            "social_post.stopped",
            json!({ "post": ended, "by": "plan_ended" }),
        );
        // Not this plan's: a post of another plan.
        harness.project.record_by(
            Some("kai"),
            at(),
            "FRK-1",
            "social_post.scheduled",
            &json!({
                "channel": "x", "buffer_channel": "chan-1", "text": "Elsewhere.", "media": [],
                "at": "2026-09-27T09:00:00Z", "approved_by": "plan", "plan": "MP-2",
                "slot": "post-1",
            }),
        );

        let plan = query(
            &harness.daemon,
            "marketing_plan.get",
            &json!({ "plan": "MP-1" }),
            "marketingPlanGetResult",
        );

        let written: Vec<(u64, &str, &str)> = plan["written_posts"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|post| {
                (
                    post["post"].as_u64().expect("a number"),
                    post["slot"].as_str().expect("a slot"),
                    post["state"].as_str().expect("a state"),
                )
            })
            .collect();
        assert_eq!(
            written,
            [
                (failed, "post-1", "failed"),
                (sent, "post-1", "sent"),
                (stopped, "post-2", "stopped"),
                (missed, "post-3", "missed"),
                (ended, "post-4", "stopped"),
            ],
            "this plan's posts, oldest first: {plan}"
        );
        let posts = &plan["written_posts"];
        assert_eq!(
            posts[1],
            json!({
                "post": sent, "slot": "post-1", "text": "Post for post-1",
                "at": "2026-09-23T10:00:00Z", "state": "sent",
                "state_at": at().to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            })
        );
        assert_eq!(posts[2]["stopped_by"], "owner");
        assert_eq!(posts[3]["missed_why"], "not_running");
        assert_eq!(posts[4]["stopped_by"], "plan_ended");
        assert!(posts[1].get("stopped_by").is_none(), "{}", posts[1]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_daemon_fetches_a_post_s_picture() {
        use base64::Engine as _;

        let _here = crate::tools::media::LoopbackAllowed::new();
        let harness = kai_working("gates-post-media");
        let (pictures, asked) = crate::tools::media::fixtures::serving().await;
        let at_pictures =
            |path: &str, kind: &str| json!({ "url": format!("{pictures}/{path}"), "kind": kind });
        let four = kai_wrote(
            &harness,
            at(),
            ("post-1", "2026-09-22T20:00:00Z"),
            &json!([
                at_pictures("ok.png", "image"),
                at_pictures("clip.mp4", "video"),
                { "url": "https://10.0.0.5/a.png", "kind": "image" },
                at_pictures("pic.svg", "image"),
            ]),
        );
        let gone = kai_wrote(
            &harness,
            at(),
            ("post-2", "2026-09-22T21:00:00Z"),
            &json!([
                at_pictures("gone.png", "image"),
                at_pictures("huge.png", "image")
            ]),
        );
        let fetch = async |post: u64, index: u64| {
            let params = json!({ "post": post, "index": index });
            super::call(&harness.daemon, "social_post.media", &params).await
        };
        let not_found = async |post: u64, index: u64| {
            let failure = fetch(post, index).await.expect_err("nothing to show");
            assert_eq!(
                failure.code,
                crate::daemon::web::NOT_FOUND,
                "{post}/{index}"
            );
        };

        let picture = fetch(four, 0).await.expect("a picture");
        conforms_to(&picture, "socialPostMediaResult");
        assert_eq!(
            picture,
            json!({
                "media_type": "image/png",
                "base64": base64::engine::general_purpose::STANDARD
                    .encode([0x89, b'P', b'N', b'G']),
            })
        );

        let before = asked.load(std::sync::atomic::Ordering::SeqCst);
        not_found(four, 1).await; // a clip is opened in a tab, not fetched
        not_found(four, 2).await; // an address that is not public
        not_found(four, 4).await; // past the list
        not_found(999, 0).await; // no such post
        assert_eq!(
            asked.load(std::sync::atomic::Ordering::SeqCst),
            before,
            "none of these reached the picture server"
        );
        not_found(four, 3).await; // an SVG is not a picture
        not_found(gone, 0).await; // an address that no longer opens
        not_found(gone, 1).await; // one over the bound
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_picture_method_is_one_the_schema_knows() {
        let harness = kai_working("gates-post-media-rpc");
        let clip = kai_wrote(
            &harness,
            at(),
            ("post-1", "2026-09-22T20:00:00Z"),
            &json!([{ "url": "https://example.com/c.mp4", "kind": "video" }]),
        );
        let ask = |params: Value| rpc(&harness.daemon, "social_post.media", &params);

        // A clip is not fetched: asked over the wire, it is `not_found`, and so is no such post.
        assert_eq!(
            ask(json!({ "post": clip, "index": 0 }))["error"]["code"],
            crate::daemon::web::NOT_FOUND
        );
        assert_eq!(
            ask(json!({ "post": 999, "index": 0 }))["error"]["code"],
            crate::daemon::web::NOT_FOUND
        );
        // The params are the schema's: no post number 0, and no fifth picture.
        for params in [
            json!({ "post": 0, "index": 0 }),
            json!({ "post": clip, "index": 4 }),
            json!({ "post": clip }),
        ] {
            assert_eq!(
                ask(params.clone())["error"]["code"],
                crate::daemon::web::INVALID_PARAMS,
                "{params}"
            );
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_a_design_review_that_waits_on_the_human() {
        let harness = Harness::new("gates-design-review-waits", |wire| {
            crate::tools::fixtures::browsing(wire);
            wire.as_object_mut().expect("a team").remove("preview");
            // The line names the Designer as the person named it, not by its id.
            wire["agents"][3]["display_name"] = json!("Iris");
        });
        harness.verifying_a_ui_change("FRK-1");
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(
            waiting["waiting"],
            json!([{
                "task_id": "FRK-1", "kind": "preview_missing", "agent_id": "iris",
                "title": "Add a login page", "line": "Iris needs to know how to open your app"
            }])
        );

        let harness = Harness::new(
            "gates-design-review-sandbox",
            crate::tools::fixtures::browsing,
        );
        harness
            .project
            .deps
            .transitions
            .set_previews(Arc::new(crate::preview::NoPreviews));
        harness.verifying_a_ui_change("FRK-1");
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(
            waiting["waiting"],
            json!([{
                "task_id": "FRK-1", "kind": "designer_needs_sandbox", "agent_id": "iris",
                "title": "Add a login page",
                "line": "The UI/UX Designer needs Docker's sandbox to open your app. Turn the \
                         sandbox on, or retire the Designer"
            }])
        );

        // Playwright off for Iris: she gets no work, and the row says how to turn it on.
        let harness = Harness::new("gates-design-review-browser", |wire| {
            crate::tools::fixtures::browsing(wire);
            wire["agents"][3]["display_name"] = json!("Iris");
            wire["agents"][3]
                .as_object_mut()
                .expect("an agent")
                .remove("mcp_servers");
        });
        harness
            .project
            .deps
            .transitions
            .set_previews(Arc::new(crate::preview::fixtures::FakePreviews::ready()));
        harness.verifying_a_ui_change("FRK-1");
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(
            waiting["waiting"],
            json!([{
                "task_id": "FRK-1", "kind": "designer_needs_browser", "agent_id": "iris",
                "title": "Add a login page",
                "line": "Iris has Playwright off, so Farik gives Iris no work. Turn Playwright \
                         on for Iris on the Team page"
            }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_a_change_that_needs_no_design_review_without_asking_docker() {
        // `UnaskedPreviews` panics when asked whether a preview can run, as a slow `docker info`
        // would hold up the page: Today's three queries, the board and the task's page all read
        // a design review, and a task that is not a UI change needs no answer from Docker.
        let harness = Harness::new(
            "gates-design-review-unasked",
            crate::tools::fixtures::browsing,
        );
        harness
            .project
            .deps
            .transitions
            .set_previews(Arc::new(crate::preview::fixtures::UnaskedPreviews));
        harness.verifying("FRK-1");

        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(waiting["waiting"], json!([]));
        let listed = query(&harness.daemon, "tasks.list", &json!({}), "tasksListResult");
        assert_eq!(listed["tasks"][0]["task_id"], "FRK-1", "{listed}");
        let got = query(
            &harness.daemon,
            "task.get",
            &json!({ "task_id": "FRK-1" }),
            "taskGetResult",
        );
        assert_eq!(got["task"]["task_id"], "FRK-1", "{got}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_open_approval_waits_on_the_human() {
        use farik_protocol::event::{NewEvent, event_from_value};

        let harness = Harness::new("gates-tool-approval", |wire| {
            wire["agents"][1]["display_name"] = json!("Theo");
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let event = event_from_value(&json!({
            "seq": 1, "recorded_at": "2026-09-17T10:00:00Z", "team_id": "farik",
            "project_id": "farik", "task_id": "FRK-1", "agent_id": "dev-a", "session_id": "s-1",
            "kind": "tool_approval.requested",
            "body": {
                "server": "github", "tool": "create_issue", "input": "{\"title\":\"x\"}",
                "input_sha256": "0".repeat(64)
            },
        }))
        .expect("schema-valid");
        let deps = &harness.project.deps;
        let appended = deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        deps.projections.apply(&appended).expect("projects");
        let approval = appended.envelope.seq;
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(
            waiting["waiting"],
            json!([{
                "task_id": "FRK-1", "kind": "tool_approval", "agent_id": "dev-a",
                "title": "Add a login page", "line": "Theo wants to use github",
                "approval": approval, "server": "github", "tool": "create_issue",
                "input": "{\"title\":\"x\"}"
            }])
        );
        assert!(harness.row("FRK-1").waiting_on_human);
        let adapter = harness.recorded(vec![crate::recorded::fixtures::reads_a_file()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime is made")
            .block_on(orchestrator.tick())
            .expect("the tick runs");
        assert!(adapter.started().is_empty(), "no session starts for it");

        // Once the human has decided, the row is gone, so that a second click cannot meet
        // `approval_decided`.
        harness.project.record(
            "FRK-1",
            "tool_approval.refused",
            &json!({ "approval": approval }),
        );
        let after = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(after["waiting"], json!([]));
        assert!(!harness.row("FRK-1").waiting_on_human);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn names_the_active_designer_whose_browser_is_off() {
        // Iris is paused and Kai, the active Designer, has it off: the row is Kai's, not Iris's.
        let harness = Harness::new("gates-design-review-kai", |wire| {
            crate::tools::fixtures::browsing(wire);
            wire["agents"][3]["status"] = json!("paused");
            let mut kai = farik_core::team::fixtures::an_agent_wire("kai", "ui_ux_designer");
            kai["display_name"] = json!("Kai");
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(kai);
        });
        harness
            .project
            .deps
            .transitions
            .set_previews(Arc::new(crate::preview::fixtures::FakePreviews::ready()));
        harness.verifying_a_ui_change("FRK-1");
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(
            waiting["waiting"],
            json!([{
                "task_id": "FRK-1", "kind": "designer_needs_browser", "agent_id": "kai",
                "title": "Add a login page",
                "line": "Kai has Playwright off, so Farik gives Kai no work. Turn Playwright \
                         on for Kai on the Team page"
            }])
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_request_from_plain_words() {
        let harness = driven("gates-file");
        let text = "Add a dark mode to the settings page so that people who work late at night \
                    are not dazzled by the screen\nThe toggle sits under Appearance.";
        let filed = call(
            &harness.daemon,
            "request.file",
            &json!({ "text": text }),
            "requestFileResult",
        );
        assert_eq!(filed, json!({ "task_id": "FRK-1" }));
        let contract = harness.project.file("FRK-1");
        assert_eq!(
            contract["title"],
            "Add a dark mode to the settings page so that people who work late at night are"
        );
        assert_eq!(contract["intent"], text);
        assert_eq!(contract["created_by"], "human");
        assert_eq!(contract["status"], "draft");
        assert_eq!(contract["budget"]["max_cost_usd"], json!(20.0));

        // Twenty characters are enough, trimmed; nineteen are not.
        let short = rpc(
            &harness.daemon,
            "request.file",
            &json!({ "text": "Make the page nicer  " }),
        );
        assert_eq!(short["error"]["code"], -32005, "{short}");
        assert_eq!(
            short["error"]["message"],
            "say a little more: at least 20 characters"
        );
        // The page words the refusal by its code, not by its message.
        assert_eq!(
            short["error"]["data"],
            json!({ "errors": [{
                "path": "/text",
                "message": "say a little more: at least 20 characters",
                "code": "too_short",
            }] })
        );
        call(
            &harness.daemon,
            "request.file",
            &json!({ "text": "Make the pages nicer" }),
            "requestFileResult",
        );
        assert_eq!(harness.project.events(&[EventKind::TaskCreated]).len(), 2);
    }

    /// A message in dev-a's chat by `author`, proposing a request when `proposes`.
    fn chat_line(harness: &Harness, author: &str, proposes: bool) -> u64 {
        let deps = &harness.project.deps;
        crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: "dev-a".to_string(),
                author: author.to_string(),
                text: "Could customers also pay with Apple Pay?".to_string(),
                in_reply_to: None,
                request: proposes.then(|| crate::chat::ProposedRequest {
                    title: "Let customers pay with Apple Pay".to_string(),
                    text: "Add Apple Pay to the checkout beside the card form.".to_string(),
                }),
                session_id: None,
            },
        )
        .expect("the chat message is recorded")
    }

    /// The `from_chat_message` of every `task.created` in the log.
    fn links(harness: &Harness) -> Vec<Option<u64>> {
        harness
            .project
            .events(&[EventKind::TaskCreated])
            .iter()
            .map(|event| match &event.body {
                farik_protocol::event::EventBody::TaskCreated(body) => {
                    body.from_chat_message.map(std::num::NonZeroU64::get)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_chat_proposal() {
        let harness = driven("gates-file-proposal");
        let reply = chat_line(&harness, "dev-a", true);
        // The user's edit of the prefilled box is what is filed.
        let text = "Let customers pay with Apple Pay and Google Pay\n\nAdd both to the checkout.";
        let filed = call(
            &harness.daemon,
            "request.file",
            &json!({ "text": text, "from_chat_message": reply }),
            "requestFileResult",
        );
        assert_eq!(filed, json!({ "task_id": "FRK-1" }));
        let contract = harness.project.file("FRK-1");
        assert_eq!(contract["intent"], text);
        assert_eq!(
            contract["title"],
            "Let customers pay with Apple Pay and Google Pay"
        );
        assert_eq!(contract["created_by"], "human");
        let created = harness.project.events(&[EventKind::TaskCreated]);
        match &created[0].body {
            farik_protocol::event::EventBody::TaskCreated(body) => {
                assert_eq!(body.created_by, "human");
                assert_eq!(
                    body.from_chat_message.map(std::num::NonZeroU64::get),
                    Some(reply)
                );
            }
            other => panic!("not a task.created: {other:?}"),
        }
        // A request filed from the Today page links to nothing.
        call(
            &harness.daemon,
            "request.file",
            &json!({ "text": "Add a dark mode to the settings page" }),
            "requestFileResult",
        );
        assert_eq!(links(&harness), [Some(reply), None]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_proposal_link() {
        let harness = driven("gates-file-proposal-refused");
        let asked = chat_line(&harness, "human", false);
        let plain = chat_line(&harness, "dev-a", false);
        let proposal = chat_line(&harness, "dev-a", true);
        let text = "Let customers pay with Apple Pay at the checkout";
        call(
            &harness.daemon,
            "request.file",
            &json!({ "text": text, "from_chat_message": proposal }),
            "requestFileResult",
        );
        // A channel message is no chat reply either.
        let channel = harness
            .project
            .deps
            .log
            .read(&farik_store::EventQuery::default())
            .expect("the log reads")
            .last()
            .map_or(0, |event| event.envelope.seq)
            + 100;
        for (seq, sentence) in [
            (
                asked,
                format!(
                    "the request links to message {asked}, which is not an agent's reply in a chat"
                ),
            ),
            (
                channel,
                format!(
                    "the request links to message {channel}, which is not an agent's reply in a chat"
                ),
            ),
            (
                plain,
                format!("the request links to message {plain}, a reply that proposes no request"),
            ),
            (
                proposal,
                "the request was already sent, as FRK-1".to_string(),
            ),
        ] {
            let refused = rpc(
                &harness.daemon,
                "request.file",
                &json!({ "text": text, "from_chat_message": seq }),
            );
            assert_eq!(refused["error"]["code"], -32005, "{refused}");
            assert_eq!(refused["error"]["message"], sentence.as_str());
        }
        assert_eq!(links(&harness), [Some(proposal)]);
        assert_eq!(
            harness
                .project
                .deps
                .files
                .list_contracts()
                .expect("listed")
                .len(),
            1
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_proposal_once() {
        let harness = driven("gates-file-proposal-once");
        // Raced a few times, so that a check outside the lock is caught every run, not by luck.
        for round in 0..5 {
            let reply = chat_line(&harness, "dev-a", true);
            let start = Arc::new(std::sync::Barrier::new(2));
            let racers: Vec<_> = (0..2)
                .map(|_| {
                    let (deps, start) = (harness.project.deps.clone(), start.clone());
                    std::thread::spawn(move || {
                        start.wait();
                        super::file_words(
                            &deps,
                            "Let customers pay with Apple Pay at the checkout",
                            Some(reply),
                        )
                    })
                })
                .collect();
            let answers: Vec<_> = racers
                .into_iter()
                .map(|racer| racer.join().expect("the racer ends"))
                .collect();
            let filed = answers.iter().filter(|answer| answer.is_ok()).count();
            assert_eq!(filed, 1, "round {round}: {answers:?}");
            let refused = format!(
                "{:?}",
                answers
                    .iter()
                    .find_map(|answer| answer.as_ref().err())
                    .expect("one is refused")
            );
            assert!(
                refused.contains("code: -32005")
                    && refused.contains("the request was already sent, as FRK-"),
                "{refused}"
            );
        }
        assert_eq!(links(&harness).len(), 5);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn wakes_the_team_when_a_request_is_filed() {
        let harness = Harness::new("gates-file-wakes", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let (ended, filed) = tokio::join!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                orchestrator.wait_until(at() + chrono::Duration::hours(1)),
            ),
            async {
                tokio::task::yield_now().await;
                super::call(
                    &harness.daemon,
                    "request.file",
                    &json!({ "text": "Add a done.txt at the root of the project" }),
                )
                .await
            }
        );

        assert!(filed.is_ok(), "the request is filed");
        assert_eq!(ended.expect("the wait ends"), Waited::Woken);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn checks_a_draft_without_saving() {
        let harness = driven("gates-check");
        harness.file("FRK-1", "refining", |_| {});
        let before = harness.project.file("FRK-1");

        let mut draft = before.clone();
        draft["title"] = json!("");
        draft["reviewer_role"] = json!("architect");
        let checked = query(
            &harness.daemon,
            "contract.check",
            &json!({ "task_id": "FRK-1", "contract": draft }),
            "contractCheckResult",
        );
        assert_eq!(checked["failures"][0]["rule"], "schema", "{checked}");
        assert!(
            checked["failures"][0]["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("/title")),
            "{checked}"
        );
        assert_eq!(checked["total"], 1, "{checked}");

        // Schema-valid, with no architect on the team to review it. The draft is checked as the
        // task it is asked about, whatever id it carries.
        draft["title"] = json!("Add a login page");
        draft["id"] = json!("FRK-9");
        let checked = query(
            &harness.daemon,
            "contract.check",
            &json!({ "task_id": "FRK-1", "contract": draft }),
            "contractCheckResult",
        );
        assert_eq!(checked["total"], 23, "{checked}");
        let failures = checked["failures"].as_array().expect("failures");
        assert_eq!(failures.len(), 1, "{checked}");
        assert_eq!(failures[0]["rule"], "reviewer_available");
        assert_eq!(
            failures[0]["plain"],
            "Nobody on the team is free to review the work but the one doing it."
        );
        assert!(
            failures[0]["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("no agent can review")),
            "{checked}"
        );
        // Nothing was written.
        assert_eq!(harness.project.file("FRK-1"), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "the save, its history without the tool calls, a second save, and a refusal"
    )]
    fn saves_the_human_edit_back_to_refining() {
        let harness = driven("gates-save");
        harness.file("FRK-1", "refining", |_| {});
        harness
            .project
            .moved("FRK-1", "refining", "escalated", &json!({}));
        harness.project.record(
            "FRK-1",
            "escalation.raised",
            &json!({ "reason": "approval", "detail": "the plan waits" }),
        );
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(
            waiting["waiting"],
            json!([{
                "task_id": "FRK-1", "kind": "approval", "agent_id": "pm",
                "title": "Add a login page", "line": "pm wrote a plan for you to approve"
            }])
        );
        let got = query(
            &harness.daemon,
            "contract.get",
            &json!({ "task_id": "FRK-1" }),
            "contractGetResult",
        );
        assert_eq!(got["contract"], harness.project.file("FRK-1"));

        let mut edited = got["contract"].clone();
        edited["intent"] =
            json!("A person signs in with an email and a password, and stays signed in.");
        let saved = call(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-1", "contract": edited }),
            "contractSaveResult",
        );
        assert_eq!(saved, json!({ "saved": true, "back_to_refining": true }));
        let row = harness
            .project
            .deps
            .projections
            .task(&"FRK-1".parse().expect("an id"))
            .expect("the board reads")
            .expect("a row");
        assert_eq!(row.status, TaskStatus::Refining);
        assert_eq!(harness.project.file("FRK-1")["intent"], edited["intent"]);
        // A session's tool calls are the log's, not the task's history; a refused one is.
        harness.project.record(
            "FRK-1",
            "tool.called",
            &json!({ "tool": "Read", "input": "{}" }),
        );
        harness.project.record(
            "FRK-1",
            "tool.returned",
            &json!({ "tool": "Read", "output": "" }),
        );
        harness.project.record(
            "FRK-1",
            "tool.denied",
            &json!({ "tool": "Bash", "reason": "not in the allowed paths" }),
        );
        let history = query(
            &harness.daemon,
            "task.history",
            &json!({ "task_id": "FRK-1" }),
            "taskHistoryResult",
        );
        let kinds: Vec<&str> = history["events"]
            .as_array()
            .expect("events")
            .iter()
            .filter_map(|event| event["kind"].as_str())
            // Farik's line about the move in the channel is not the task's.
            .filter(|kind| *kind != "message.posted")
            .collect();
        // Written before the move, so a session the move wakes reads the human's plan.
        assert!(
            kinds.ends_with(&[
                "contract.written",
                "task.transitioned",
                "escalation.resolved",
                "tool.denied",
            ]),
            "{kinds:?}"
        );
        let written = harness.project.events(&[EventKind::ContractWritten]);
        assert_eq!(
            farik_protocol::event::event_to_value(&written[0])["body"]["written_by"],
            "human"
        );

        // Refining is not frozen: the edit, made on the plan as it is now, saves in place.
        let mut edited = harness.project.file("FRK-1");
        edited["intent"] = json!("A person signs in with an email and a password, and signs out.");
        let saved = call(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-1", "contract": edited }),
            "contractSaveResult",
        );
        assert_eq!(saved, json!({ "saved": true, "back_to_refining": false }));

        // A field the human does not write is refused in the gate's words, and nothing changes.
        let before = harness.project.file("FRK-1");
        let mut wrong = before.clone();
        wrong["iteration"] = json!(2);
        let refused = rpc(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-1", "contract": wrong }),
        );
        assert_eq!(refused["error"]["code"], -32005, "{refused}");
        assert!(
            refused["error"]["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("iteration is the governor's")),
            "{refused}"
        );
        assert_eq!(harness.project.file("FRK-1"), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_save_it_cannot_make() {
        let harness = driven("gates-save-refused");
        harness.file("FRK-1", "refining", |_| {});
        let before = harness.project.file("FRK-1");

        // A save never locks or unlocks the plan: that is recorded by its own command.
        let mut locking = before.clone();
        locking["locked"] = json!(true);
        let refused = rpc(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-1", "contract": locking }),
        );
        assert_eq!(refused["error"]["code"], -32005, "{refused}");
        assert_eq!(
            refused["error"]["message"],
            "a save does not lock or unlock the plan; lock or unlock it on its own"
        );
        assert_eq!(harness.project.file("FRK-1"), before);

        // A plan the team works to changes only once the work is held (escalated), in plain words.
        harness.ready("FRK-2");
        let before = harness.project.file("FRK-2");
        let mut edited = before.clone();
        edited["intent"] = json!("A person signs in with an email and a password, and signs out.");
        let refused = rpc(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-2", "contract": edited }),
        );
        assert_eq!(refused["error"]["code"], -32005, "{refused}");
        assert_eq!(
            refused["error"]["message"],
            "the team is working to this plan; hold the work first, then change it"
        );
        assert_eq!(harness.project.file("FRK-2"), before);

        // Holding the work is the human's move into `escalated` (5.2); the same edit then saves.
        let held = rpc(
            &harness.daemon,
            "command",
            &json!({ "command": { "command": "task_transition", "body": {
                "task_id": "FRK-2", "to": "escalated", "reason": "Held by you to change the plan"
            } } }),
        );
        assert!(held["result"].get("said").is_some(), "{held}");
        let mut edited = harness.project.file("FRK-2");
        edited["intent"] = json!("A person signs in with an email and a password, and signs out.");
        let saved = call(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-2", "contract": edited }),
            "contractSaveResult",
        );
        assert_eq!(saved, json!({ "saved": true, "back_to_refining": true }));

        // A plan awaiting approval that the edit would break stays where it is.
        harness.file("FRK-3", "refining", |_| {});
        harness
            .project
            .moved("FRK-3", "refining", "escalated", &json!({}));
        harness.project.record(
            "FRK-3",
            "escalation.raised",
            &json!({ "reason": "approval", "detail": "the plan waits" }),
        );
        let mut broken = harness.project.file("FRK-3");
        broken["title"] = json!("");
        let refused = rpc(
            &harness.daemon,
            "contract.save",
            &json!({ "task_id": "FRK-3", "contract": broken }),
        );
        assert_eq!(refused["error"]["code"], -32005, "{refused}");
        let row = harness
            .project
            .deps
            .projections
            .task(&"FRK-3".parse().expect("an id"))
            .expect("the board reads")
            .expect("a row");
        assert_eq!(row.status, TaskStatus::Escalated);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "a task's diff and checks, an epic's, and a plan's"
    )]
    fn answers_the_diff_and_checks() {
        let harness = driven("gates-diff");
        let git = &harness.project.deps.git;
        let commit = |task: &str, text: &str| {
            let worktree = harness.worktree(task);
            std::fs::write(worktree.join("done.txt"), text).expect("written");
            git.commit(&worktree, "Add done.txt", &["done.txt".to_string()])
                .expect("committed")
        };
        harness.project.filed("FRK-3", "in_progress", "epic", None);
        for task in ["FRK-1", "FRK-2", "FRK-5"] {
            harness.verifying_with(task, false, false, |wire| {
                wire["parent"] = json!("FRK-3");
            });
        }
        commit("FRK-1", "one\ntwo\n");
        harness.project.record(
            "FRK-1",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "exit 0",
                     "run_by": "reviewer", "recorded_by": "dev-b" }),
        );

        let diff = query(
            &harness.daemon,
            "task.diff",
            &json!({ "task_id": "FRK-1" }),
            "taskDiffResult",
        );
        assert_eq!(diff["files"], json!(["done.txt"]), "{diff}");
        assert_eq!(
            (diff["added"].clone(), diff["removed"].clone()),
            (json!(2), json!(0))
        );
        assert!(
            diff["diff"]
                .as_str()
                .is_some_and(|text| text.contains("+one")),
            "{diff}"
        );

        let checks = query(
            &harness.daemon,
            "task.checks",
            &json!({ "task_id": "FRK-1" }),
            "taskChecksResult",
        );
        assert_eq!(
            checks["checks"],
            json!([{ "criterion_id": "C1", "text": "done.txt exists.", "passed": true, "evidence": "exit 0" }])
        );
        // A result from before the task last entered verifying is not this verification's.
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        harness
            .project
            .moved("FRK-1", "verifying", "in_progress", &people);
        harness
            .project
            .moved("FRK-1", "in_progress", "verifying", &people);
        let checks = query(
            &harness.daemon,
            "task.checks",
            &json!({ "task_id": "FRK-1" }),
            "taskChecksResult",
        );
        assert_eq!(checks["checks"], json!([]));

        // The epic joins its tasks' integrated diffs, in id order, under their ids.
        let second = commit("FRK-2", "three\n");
        // FRK-5's work is not added to the project, so it is not the epic's yet.
        commit("FRK-5", "four\n");
        let first = git
            .merge_base(&harness.branch("FRK-1"), &harness.branch("FRK-1"))
            .expect("the branch names a commit");
        for (task, sha) in [("FRK-2", second), ("FRK-1", first)] {
            harness.project.record(
                task,
                "task.integrated",
                &json!({ "sha": sha, "into": "main", "integrated_by": "governor" }),
            );
        }
        let epic = query(
            &harness.daemon,
            "task.diff",
            &json!({ "task_id": "FRK-3" }),
            "taskDiffResult",
        );
        let text = epic["diff"].as_str().expect("a diff");
        let (one, two) = (
            text.find("# FRK-1\n").expect("FRK-1's part"),
            text.find("# FRK-2\n").expect("FRK-2's part"),
        );
        assert!(one < two && text[one..two].contains("+one"), "{text}");
        assert!(text[two..].contains("+three"), "{text}");
        assert!(!text.contains("FRK-5") && !text.contains("+four"), "{text}");
        assert_eq!(epic["files"], json!(["done.txt"]), "{epic}");
        assert_eq!(
            (epic["added"].clone(), epic["removed"].clone()),
            (json!(3), json!(0))
        );

        // A plan awaiting approval answers its readiness.
        harness.file("FRK-4", "refining", |wire| {
            wire["reviewer_role"] = json!("architect");
        });
        harness
            .project
            .moved("FRK-4", "refining", "escalated", &json!({}));
        harness.project.record(
            "FRK-4",
            "escalation.raised",
            &json!({ "reason": "approval", "detail": "the plan waits" }),
        );
        let plan = query(
            &harness.daemon,
            "task.checks",
            &json!({ "task_id": "FRK-4" }),
            "taskChecksResult",
        );
        assert_eq!(
            plan["checks"][0]["criterion_id"], "reviewer_available",
            "{plan}"
        );
        assert_eq!(
            plan["checks"][0]["text"],
            "Nobody on the team is free to review the work but the one doing it."
        );
        assert_eq!(plan["checks"][0]["passed"], false);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_question_choices() {
        let harness = driven("gates-questions");
        harness.file("FRK-1", "refining", |_| {});
        harness.file("FRK-2", "refining", |_| {});
        let ask = |task: &str, input: Value| {
            harness
                .project
                .call("pm", Some(task), "farik_ask_human", input)
        };
        let question = "Which colour should the button be?";
        let long = "x".repeat(81);
        let longer = "x".repeat(161);
        for choices in [
            json!([{ "label": "" }]),
            json!([{ "label": long }]),
            json!([{ "label": "Blue", "hint": longer }]),
            json!([{ "label": "A" }, { "label": "B" }, { "label": "C" }, { "label": "D" }, { "label": "E" }]),
        ] {
            assert!(
                ask("FRK-1", json!({ "question": question, "choices": choices })).is_err(),
                "{choices}"
            );
        }
        assert!(
            harness
                .project
                .events(&[EventKind::QuestionAsked])
                .is_empty()
        );

        let asked = ask(
            "FRK-1",
            json!({ "question": question, "choices": [
                { "label": "Blue" },
                { "label": "Green", "hint": "x".repeat(160) },
            ] }),
        )
        .expect("the question is asked");
        let other = ask("FRK-2", json!({ "question": "Is a week soon enough?" }))
            .expect("the question is asked");
        harness.project.record(
            "FRK-2",
            "question.answered",
            &json!({ "question_id": other["question_id"], "answer": "Yes.", "answered_by": "human" }),
        );

        let listed = query(
            &harness.daemon,
            "questions.list",
            &json!({}),
            "questionsListResult",
        );
        assert_eq!(
            listed["questions"],
            json!([
                {
                    "question_id": asked["question_id"], "task_id": "FRK-1", "agent_id": "pm",
                    "text": question,
                    "choices": [{ "label": "Blue" }, { "label": "Green", "hint": "x".repeat(160) }],
                    "answer": null
                },
                {
                    "question_id": other["question_id"], "task_id": "FRK-2", "agent_id": "pm",
                    "text": "Is a week soon enough?", "choices": [], "answer": "Yes."
                },
            ])
        );
        let one = query(
            &harness.daemon,
            "questions.list",
            &json!({ "task_id": "FRK-2" }),
            "questionsListResult",
        );
        assert_eq!(one["questions"].as_array().map(Vec::len), Some(1), "{one}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_what_a_private_folder_task_changed_and_no_diff() {
        let harness = Harness::with_finance("gates-folder-diff");
        harness.finance_task("FRK-1", Some("in_progress"));
        let folder = harness.finance_folder();
        std::fs::create_dir_all(folder.join("2026")).expect("the folder is made");
        std::fs::write(folder.join("books.xlsx"), "books").expect("written");
        std::fs::write(folder.join("2026/pricing.xlsx"), "pricing").expect("written");
        let task: farik_core::contract::TaskId = "FRK-1".parse().expect("a task id");
        farik_store::baseline::copy_baseline(&folder, &task).expect("the copy is taken");
        std::fs::write(folder.join("books.xlsx"), "edited books").expect("written");
        std::fs::write(folder.join("forecast.xlsx"), "forecast").expect("written");
        std::fs::remove_file(folder.join("2026/pricing.xlsx")).expect("removed");

        let diff = query(
            &harness.daemon,
            "task.diff",
            &json!({ "task_id": "FRK-1" }),
            "taskDiffResult",
        );

        assert_eq!(
            diff,
            json!({
                "diff": "",
                "files": ["2026/pricing.xlsx", "books.xlsx", "forecast.xlsx"],
                "added": 0,
                "removed": 0,
                "private_folder": true,
            })
        );
        // A task of any other role answers as it did: no such key.
        harness.in_progress("FRK-2", "dev-a", "dev-b");
        let other = query(
            &harness.daemon,
            "task.diff",
            &json!({ "task_id": "FRK-2" }),
            "taskDiffResult",
        );
        assert!(other.get("private_folder").is_none(), "{other}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_no_changes_of_a_private_folder_task_that_has_no_copy_yet() {
        // A task not yet assigned has no copy of the folder to differ from, so it changed
        // nothing: its page does not list the books as its new files.
        let harness = Harness::with_finance("gates-folder-no-copy");
        harness.finance_task("FRK-1", None);
        let folder = harness.finance_folder();
        std::fs::create_dir_all(&folder).expect("the folder is made");
        std::fs::write(folder.join("books.xlsx"), "books").expect("written");

        let diff = query(
            &harness.daemon,
            "task.diff",
            &json!({ "task_id": "FRK-1" }),
            "taskDiffResult",
        );

        assert_eq!(
            diff,
            json!({
                "diff": "", "files": [], "added": 0, "removed": 0, "private_folder": true,
            })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn answers_tries_and_the_sprint() {
        let harness = driven("gates-tries");
        harness.rejected("FRK-1", 3, "C1 still fails");
        harness.project.moved(
            "FRK-1",
            "rejected",
            "escalated",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": 3 }),
        );
        harness.project.record(
            "FRK-1",
            "escalation.raised",
            &json!({ "reason": "iterations", "detail": "three tries" }),
        );
        let tries = |task: &str| {
            query(
                &harness.daemon,
                "task.tries",
                &json!({ "task_id": task }),
                "taskTriesResult",
            )
        };
        // Three returns are allowed, so four tries: the fourth, sent back, escalated.
        assert_eq!(tries("FRK-1"), json!({ "try": 4, "of": 4 }));
        let offered = query(
            &harness.daemon,
            "escalation.choices",
            &json!({ "task_id": "FRK-1" }),
            "escalationChoicesResult",
        );
        assert_eq!(
            offered["choices"][0]["label"], "Give 2 more tries",
            "{offered}"
        );
        assert_eq!(
            offered["choices"][1]["label"], "Ask pm to change the plan",
            "{offered}"
        );
        let resolve = offered["choices"][0]["body"].clone();
        let mut command = resolve;
        command["body"]["message"] = json!("Try once more, smaller.");
        let reply = rpc(&harness.daemon, "command", &json!({ "command": command }));
        assert!(reply["result"]["said"].is_string(), "{reply}");
        assert_eq!(tries("FRK-1"), json!({ "try": 5, "of": 6 }));

        let none = query(
            &harness.daemon,
            "sprint.current",
            &json!({}),
            "sprintCurrentResult",
        );
        assert_eq!(none, Value::Null);
        harness.accepted("FRK-2");
        harness.ready("FRK-3");
        // Nothing sent back yet: the first try.
        assert_eq!(tries("FRK-3"), json!({ "try": 1, "of": 4 }));
        // A cancelled task is done with, as an accepted one is.
        harness.file("FRK-5", "refining", |_| {});
        harness
            .project
            .moved("FRK-5", "refining", "cancelled", &json!({}));
        harness
            .project
            .open_sprint("S1", Some(50.0), &["FRK-2", "FRK-3", "FRK-5"]);
        let sprint = query(
            &harness.daemon,
            "sprint.current",
            &json!({}),
            "sprintCurrentResult",
        );
        assert_eq!(sprint, json!({ "sprint_id": "S1", "done": 2, "total": 3 }));

        let contract = harness.project.file("FRK-3");
        let checked = query(
            &harness.daemon,
            "contract.check",
            &json!({ "task_id": "FRK-3", "contract": contract }),
            "contractCheckResult",
        );
        assert_eq!(checked, json!({ "failures": [], "total": 25 }));

        // The activity and what moved answer in their shapes.
        query(
            &harness.daemon,
            "team.activity",
            &json!({}),
            "teamActivityResult",
        );
        let moved = query(
            &harness.daemon,
            "moved.since",
            &json!({ "since": "2026-09-22T00:00:00Z" }),
            "movedSinceResult",
        );
        assert!(
            !moved["moved"].as_array().expect("moved").is_empty(),
            "{moved}"
        );
    }

    /// A ready task FRK-1, and an epic FRK-2 broken down into the ready FRK-3, FRK-4 and FRK-5,
    /// on a team that plans in sprints when `on`.
    fn a_backlog(name: &str, on: bool) -> Harness {
        let harness = Harness::new(name, |wire| {
            if on {
                wire["policy"]["plan_in_sprints"] = json!(true);
            }
        });
        harness.ready("FRK-1");
        harness.project.filed("FRK-2", "in_progress", "epic", None);
        for task in ["FRK-3", "FRK-4", "FRK-5"] {
            harness.project.filed(task, "ready", "task", Some("FRK-2"));
        }
        harness
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn summarises_the_backlog() {
        let summary = |harness: &Harness| {
            query(
                &harness.daemon,
                "backlog.summary",
                &json!({}),
                "backlogSummaryResult",
            )
        };
        // The epic counts once, its tasks with it.
        assert_eq!(
            summary(&a_backlog("gates-backlog-on", true)),
            json!({ "plan_in_sprints": true, "count": 2 })
        );
        assert_eq!(
            summary(&a_backlog("gates-backlog-off", false)),
            json!({ "plan_in_sprints": false, "count": 0 })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn leaves_the_backlog_out_of_waiting() {
        let harness = a_backlog("gates-backlog-waiting", true);
        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        // The Backlog waits for a sprint, which Today says in the team band (answer 2).
        assert_eq!(waiting["waiting"], json!([]), "{waiting}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn offers_to_carry_on_where_the_task_was() {
        let harness = driven("gates-carry-on");
        harness.file("FRK-4", "refining", |_| {});
        harness
            .project
            .moved("FRK-4", "refining", "escalated", &json!({}));
        harness.project.record(
            "FRK-4",
            "escalation.raised",
            &json!({ "reason": "explicit_request", "detail": "the PM asks" }),
        );
        let offered = query(
            &harness.daemon,
            "escalation.choices",
            &json!({ "task_id": "FRK-4" }),
            "escalationChoicesResult",
        );
        assert_eq!(
            offered["choices"][0],
            json!({ "label": "Carry on", "body": resolve("refining") })
        );
    }

    fn resolve(to: &str) -> Value {
        json!({ "command": "escalation_resolve", "body": { "task_id": "FRK-4", "to": to } })
    }

    #[test]
    fn offers_the_choices_for_each_reason() {
        let cancel = json!({ "label": "Cancel the task", "body": resolve("cancelled") });
        let change = json!({ "label": "Change the plan", "body": resolve("refining") });
        let offered = |reason| choices("FRK-4", reason, Some(TaskStatus::InProgress), "Ada");

        let mut more = resolve("in_progress");
        more["body"]["extra_tries"] = json!(2);
        assert_eq!(
            offered("iterations"),
            vec![
                json!({ "label": "Give 2 more tries", "body": more }),
                json!({ "label": "Ask Ada to change the plan", "body": resolve("refining") }),
                cancel.clone(),
            ]
        );
        for reason in ["budget", "sessions"] {
            assert_eq!(
                offered(reason),
                vec![change.clone(), cancel.clone()],
                "{reason}"
            );
        }
        for reason in [
            "blocker_age",
            "permission",
            "readiness_failures",
            "explicit_request",
        ] {
            assert_eq!(
                offered(reason),
                vec![
                    json!({ "label": "Carry on", "body": resolve("in_progress") }),
                    change.clone(),
                    cancel.clone(),
                ],
                "{reason}"
            );
        }
        // Carrying on goes back to where the task was.
        assert_eq!(
            choices("FRK-4", "blocker_age", Some(TaskStatus::Blocked), "Ada")[0]["body"],
            resolve("blocked")
        );
        // A plan to approve is the plan page's, and is no help case.
        assert!(offered("approval").is_empty());
        // Integration is no escalation of a task: an accepted task waits to be added (5.7).
        assert!(offered("integration").is_empty());
        assert!(offered("risk_gate").is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_a_chat_cost_as_chat() {
        use farik_core::budget::SessionLedger;
        use farik_core::contract::Role;
        use farik_protocol::event::{CostRecordedBodyPurpose, EventBody};

        let harness = Harness::new("gates-chat-cost", |_| {});
        let deps = &harness.project.deps;
        crate::chat::post_chat(
            &deps.log,
            deps.clock.as_ref(),
            &deps.ids,
            crate::chat::NewChatMessage {
                chat: "pm".to_string(),
                author: "human".to_string(),
                text: "Could customers also pay with Apple Pay?".to_string(),
                in_reply_to: None,
                request: None,
                session_id: None,
            },
        )
        .expect("recorded");
        let adapter = harness.recorded(vec![
            crate::recorded::fixtures::chat_answers_with_a_request(),
        ]);
        harness
            .orchestrator(adapter)
            .tick()
            .await
            .expect("the chat runs");

        let costs: Vec<_> = harness
            .events(&[EventKind::CostRecorded])
            .into_iter()
            .filter_map(|event| match event.body {
                EventBody::CostRecorded(body) => Some((event.envelope.ids.task_id, body)),
                _ => None,
            })
            .collect();
        assert!(!costs.is_empty(), "the chat's usage is costed");
        for (task, body) in &costs {
            assert_eq!(body.purpose, CostRecordedBodyPurpose::Chat, "{body:?}");
            assert_eq!(*task, None);
        }
        let spent: f64 = costs.iter().map(|(_, body)| body.cost_usd).sum();
        assert!(spent > 0.0, "{costs:?}");
        // It counts toward the day that `check_budgets` reads.
        let team = deps.files.read_team().expect("the team");
        let day = crate::cost::budget_state(
            &deps.projections,
            &team,
            Role::SoftwareDeveloper,
            None,
            &SessionLedger::default(),
            at(),
        )
        .expect("the budgets read")
        .day_spent_usd;
        assert!((day - spent).abs() < 1e-9, "{day} against {spent}");

        // While a chat runs, the agent is answering it, and no chat text is shown.
        let running = farik_protocol::event::event_from_value(&json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "agent_id": "dev-a",
            "session_id": "chat-session",
            "kind": "session.started",
            "body": { "purpose": "chat", "model": "claude-opus-5", "effort": "low", "in_reply_to": 1, "chat": "dev-a" },
        }))
        .expect("the fixture is schema-valid");
        deps.log
            .append(&farik_protocol::event::NewEvent {
                recorded_at: running.envelope.recorded_at,
                ids: running.envelope.ids,
                body: running.body,
            })
            .expect("appends");
        let activity = tokio::task::spawn_blocking({
            let daemon = Arc::clone(&harness.daemon);
            move || query(&daemon, "team.activity", &json!({}), "teamActivityResult")
        })
        .await
        .expect("the query runs");
        let dev_a = activity["activity"]
            .as_array()
            .expect("activity")
            .iter()
            .find(|one| one["agent_id"] == "dev-a")
            .expect("dev-a")
            .clone();
        assert_eq!(dev_a["line"], "Answering your chat", "{dev_a}");
        assert_eq!(dev_a["purpose"], "chat", "{dev_a}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn renewals_list_answers_open_and_unreadable() {
        let harness = Harness::with_procurement("gates-renewals-list");
        let ask = || {
            query(
                &harness.daemon,
                "renewals.list",
                &json!({}),
                "renewalsListResult",
            )
        };
        assert_eq!(
            ask(),
            json!({ "open": [], "unreadable": 0 }),
            "before any run"
        );

        // A register with one renewal due and two rows Farik cannot read, read by the daily run.
        let folder = harness.procurement_folder();
        let cells = |cells: &[&str]| -> Vec<crate::tools::sheets::CellInput> {
            cells
                .iter()
                .map(|cell| crate::tools::sheets::CellInput::text(cell))
                .collect()
        };
        crate::tools::sheets::write_new_workbook(
            &folder,
            &folder.join("vendors.xlsx"),
            &[crate::tools::sheets::SheetInput::new(
                "Vendors",
                vec![
                    cells(&["vendor", "renews_on", "notice_days", "status"]),
                    cells(&["Vercel", "2026-10-02", "7", "active"]),
                    cells(&["Notion", "next month", "", "active"]),
                    cells(&["Slack", "2026-10-10", "-3", "trial"]),
                ],
            )],
        )
        .expect("a register");
        let deps = &harness.project.deps;
        let team = deps.files.read_team().expect("the team");
        crate::procurement::check_renewals(deps, &team, at()).expect("the daily run");

        let listed = ask();
        assert_eq!(listed["unreadable"], 2);
        let open = listed["open"].as_array().expect("a list");
        assert_eq!(open.len(), 1);
        assert_eq!(open[0]["vendor"], "Vercel");
        assert_eq!(open[0]["renews_on"], "2026-10-02");
        assert_eq!(open[0]["decide_by"], "2026-09-25");
        assert_eq!(open[0]["flagged_at"], "2026-09-22T12:00:00Z");
        let number = open[0]["renewal"].as_u64().expect("a number");

        // Dismissed, it is not open; what Farik could not read is still said.
        harness
            .project
            .record("", "renewal.dismissed", &json!({ "renewal": number }));
        assert_eq!(ask(), json!({ "open": [], "unreadable": 2 }));
    }

    /// `proc`'s order 1 on FRK-1 from `seller`, drafted in its session as the tool records it, with
    /// its comparison written and its workbook made; a page on a site Farik ships, or none.
    fn order_drafted(harness: &Harness, seller: &str, page: bool) -> Value {
        let project = &harness.project;
        project
            .call(
                "proc",
                Some("FRK-1"),
                "farik_write_evaluation",
                json!({ "name": "mirrors", "text": "# Baby car mirrors\n\nAcme is <b>cheapest</b>." }),
            )
            .expect("the comparison is written");
        let shipped = &farik_roles::sites::farik_sites()[0].host;
        project
            .call(
                "proc",
                Some("FRK-1"),
                "farik_draft_purchase_order",
                json!({
                    "seller": seller,
                    "seller_contact": "sales@acme.example",
                    "lines": [
                        { "item": "Baby car mirror", "quantity": 3, "unit_price": "19.99", "unit": "piece" },
                        { "item": "Mounting kit", "quantity": 1, "unit_price": "0.01", "unit": "" }
                    ],
                    "currency": "USD", "period": "month", "delivery": "3 days", "terms": "Net 30",
                    "url": if page { format!("https://www.{shipped}/mirrors?size=big") } else { String::new() },
                    "evaluation": "evaluations/mirrors.md",
                    "why": "It is the cheapest seller that ships to us."
                }),
            )
            .expect("the order is drafted")
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waiting_list_gives_the_host() {
        let harness = Harness::with_procurement("gates-order-waits");
        harness.procurement_task("FRK-1", Some("in_progress"));
        order_drafted(&harness, "Acme", true);
        order_drafted(&harness, "Bolt", false);
        let name = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team")
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "proc")
            .map(|agent| agent.display_name.to_string())
            .expect("proc");
        let shipped = farik_roles::sites::farik_sites()[0].host.clone();

        let waiting = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        )["waiting"]
            .clone();

        let lines = json!([
            { "item": "Baby car mirror", "quantity": 3, "unit": "piece", "unit_price": "19.99", "line_total": "59.97" },
            { "item": "Mounting kit", "quantity": 1, "unit": "", "unit_price": "0.01", "line_total": "0.01" }
        ]);
        let row = |order: u64, seller: &str, url: &str| {
            let mut row = json!({
                "task_id": "FRK-1", "kind": "purchase_order", "agent_id": "proc",
                "title": "Add a login page",
                "line": format!("{name} set up an order from {seller}: 59.98 USD"),
                "order": order, "seller": seller, "seller_contact": "sales@acme.example",
                "lines": lines, "currency": "USD", "period": "month", "total": "59.98",
                "delivery": "3 days", "terms": "Net 30", "url": url,
                "evaluation": "evaluations/mirrors.md",
                "why": "It is the cheapest seller that ships to us.",
                "at": "2026-09-22T12:00:00Z", "expires_at": "2026-10-22T12:00:00Z"
            });
            if !url.is_empty() {
                // The site the address names, in its ASCII form and without `www.`.
                row["host"] = json!(shipped);
            }
            row
        };
        assert_eq!(
            waiting,
            json!([
                row(
                    1,
                    "Acme",
                    &format!("https://www.{shipped}/mirrors?size=big")
                ),
                row(2, "Bolt", ""),
            ])
        );
        assert!(waiting[1].get("host").is_none(), "no page, no host");

        // Approved, it is gone from Today.
        harness.project.record(
            "FRK-1",
            "purchase_order.approved",
            &json!({ "order": 1, "note": "" }),
        );
        let after = query(
            &harness.daemon,
            "waiting.list",
            &json!({}),
            "waitingListResult",
        );
        assert_eq!(after["waiting"].as_array().map(Vec::len), Some(1));
        assert_eq!(after["waiting"][0]["order"], 2);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one row for each thing the owner reads of a request"
    )]
    fn waiting_list_gives_an_escalated_requests_host_and_the_text_it_would_file() {
        let harness = Harness::with_procurement("gates-pipeline-waits");
        harness.procurement_task("FRK-1", Some("in_progress"));
        let name = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team")
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "proc")
            .map(|agent| agent.display_name.to_string())
            .expect("proc");
        let kit = farik_roles::load_kit(farik_core::contract::Role::ProcurementSpecialist)
            .expect("the shipped kit");
        let farik_roles::KitConnector::Server { copy, .. } = &kit.connectors[0] else {
            panic!("the kit's first service is a server");
        };
        let asked = |source: &str, url: &str| {
            harness
                .project
                .record_in(
                    Some("proc"),
                    Some("session-proc"),
                    "FRK-1",
                    "data_pipeline.requested",
                    &json!({
                        "name": source,
                        "what": "Reads a seller's page as text.\nEven where prices need a browser.",
                        "source_url": url,
                        "why": "Two of the five sellers show their prices only in a full browser.",
                        "cost": "paid", "needs_account": true, "sends_project_data": false
                    }),
                )
                .envelope
                .seq
        };
        let rows = |harness: &Harness| {
            query(
                &harness.daemon,
                "waiting.list",
                &json!({}),
                "waitingListResult",
            )["waiting"]
                .clone()
        };
        let first = asked("Firecrawl", "https://www.Firecrawl.dev/pricing?plan=big");
        let second = asked(&copy.title, "https://goshippo.com/shipping-api");
        // Open, a request waits on the Product Manager and is not listed.
        assert_eq!(rows(&harness), json!([]));

        // The Product Manager passes the first on in its decision session; Farik the second.
        harness.project.record_in(
            Some("pm"),
            Some("session-pm"),
            "",
            "session.started",
            &json!({ "purpose": "verify", "model": "claude-opus-5-5", "effort": "high",
                     "pipeline": first }),
        );
        harness.project.record_in(
            Some("pm"),
            Some("session-pm"),
            "",
            "data_pipeline.escalated",
            &json!({ "pipeline": first, "reason": "It needs a paid plan, so it is your call." }),
        );
        harness.project.record(
            "",
            "data_pipeline.escalated",
            &json!({ "pipeline": second, "reason": "The Product Manager did not decide" }),
        );

        let row = |pipeline: u64, source: &str, url: &str, host: &str, text: &str| {
            json!({
                "task_id": "FRK-1", "kind": "data_pipeline", "agent_id": "proc",
                "title": "Add a login page",
                "line": format!("{name} asks for a data source: {source}"),
                "pipeline": pipeline, "name": source,
                "what": "Reads a seller's page as text.\nEven where prices need a browser.",
                "url": url, "host": host,
                "why": "Two of the five sellers show their prices only in a full browser.",
                "cost": "paid", "needs_account": true, "sends_project_data": false,
                "at": "2026-09-22T12:00:00Z", "request_text": text,
            })
        };
        let mut listed = row(
            first,
            "Firecrawl",
            "https://www.Firecrawl.dev/pricing?plan=big",
            "firecrawl.dev",
            "Set up Firecrawl for the Procurement Specialist.\n\
             What it gives: Reads a seller's page as text. Even where prices need a browser.\n\
             Source: https://www.Firecrawl.dev/pricing?plan=big\n\
             Asked because: Two of the five sellers show their prices only in a full browser.",
        );
        listed["reason"] = json!("It needs a paid plan, so it is your call.");
        // A source of the kit says so in the text the owner reads, and Farik's own passing on
        // gives no reason of the manager's.
        let kit_text = row(
            second,
            &copy.title,
            "https://goshippo.com/shipping-api",
            "goshippo.com",
            &format!(
                "Set up {title} for the Procurement Specialist.\n\
                 What it gives: Reads a seller's page as text. Even where prices need a browser.\n\
                 Source: https://goshippo.com/shipping-api\n\
                 Asked because: Two of the five sellers show their prices only in a full browser.\n\
                 Connect {title} on the Procurement Specialist's page.",
                title = copy.title
            ),
        );
        assert_eq!(rows(&harness), json!([listed, kit_text]));

        // Decided, it is gone from Today.
        harness.project.record(
            "",
            "data_pipeline.declined",
            &json!({ "pipeline": first, "by": "human", "reason": "No." }),
        );
        let after = rows(&harness);
        assert_eq!(after.as_array().map(Vec::len), Some(1));
        assert_eq!(after[0]["pipeline"], second);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_each_order_with_its_state() {
        let harness = Harness::with_procurement("gates-orders-list");
        harness.procurement_task("FRK-1", Some("in_progress"));
        for seller in ["Acme", "Bolt", "Cog"] {
            order_drafted(&harness, seller, false);
        }
        let project = &harness.project;
        let minute = |n: i64| at() + chrono::Duration::minutes(n);
        let stamp = |n: i64| minute(n).to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
        project.record_at(
            minute(1),
            "FRK-1",
            "purchase_order.approved",
            &json!({ "order": 1, "note": "Go." }),
        );
        project.record_at(
            minute(2),
            "FRK-1",
            "purchase_order.placed",
            &json!({ "order": 1, "placed_on": "2026-09-01", "paid": "100.00", "currency": "EUR" }),
        );
        project.record_by(
            Some("proc"),
            minute(3),
            "FRK-1",
            "purchase_order.updated",
            &json!({ "order": 1, "status": "shipped", "note": "On its way.", "expected_on": "2026-09-30" }),
        );
        project.record_at(
            minute(4),
            "FRK-1",
            "purchase_order.rejected",
            &json!({ "order": 2, "note": "Too dear." }),
        );

        let listed = query(
            &harness.daemon,
            "purchase_orders.list",
            &json!({}),
            "purchaseOrdersListResult",
        );

        let orders = listed["orders"].as_array().expect("a list");
        let states: Vec<&str> = orders
            .iter()
            .map(|order| order["state"].as_str().expect("a state"))
            .collect();
        assert_eq!(states, ["placed", "rejected", "drafted"], "oldest first");
        assert_eq!(
            orders[0],
            json!({
                "order": 1, "state": "placed", "seller": "Acme", "total": "59.98",
                "currency": "USD", "period": "month", "task_id": "FRK-1", "agent_id": "proc",
                "drafted_at": stamp(0), "decided_at": stamp(1), "note": "Go.",
                "placed_on": "2026-09-01", "paid": "100.00", "paid_currency": "EUR",
                "status": { "status": "shipped", "note": "On its way.", "expected_on": "2026-09-30",
                            "by": "agent", "at": stamp(3) },
                "overdue": false
            })
        );
        assert_eq!(orders[1]["ended_at"], stamp(4));
        assert_eq!(orders[1]["note"], "Too dear.");
        assert_eq!(orders[1]["overdue"], false);
        // A drafted order says when it closes by itself.
        assert_eq!(
            orders[2]["expires_at"],
            (at() + chrono::Duration::days(30))
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
        );
        assert!(orders[2].get("decided_at").is_none());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "each way a comparison or a workbook can be out of reach, one after another"
    )]
    fn the_evaluation_and_the_file_read_only_their_own() {
        use base64::Engine as _;
        let harness = Harness::with_procurement("gates-order-reads");
        harness.procurement_task("FRK-1", Some("in_progress"));
        order_drafted(&harness, "Acme", true);
        let ask = |name: &str, order: u64| {
            rpc(
                &harness.daemon,
                "query",
                &json!({ "name": name, "params": { "order": order } }),
            )
        };

        let read = query(
            &harness.daemon,
            "purchase_order.evaluation",
            &json!({ "order": 1 }),
            "purchaseOrderEvaluationResult",
        );
        assert_eq!(
            read,
            json!({ "text": "# Baby car mirrors\n\nAcme is <b>cheapest</b>." })
        );
        let file = call(
            &harness.daemon,
            "purchase_order.file",
            &json!({ "order": 1 }),
            "purchaseOrderFileResult",
        );
        assert_eq!(
            file["media_type"],
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(file["name"], "PO-1.xlsx");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file["base64"].as_str().expect("base64"))
            .expect("base64 decodes");
        assert_eq!(&bytes[..2], b"PK", "a workbook is a zip");
        assert_eq!(
            bytes,
            std::fs::read(harness.procurement_folder().join("orders/PO-1.xlsx")).expect("the file")
        );

        // An order nobody drafted has neither.
        for name in ["purchase_order.evaluation", "purchase_order.file"] {
            let reply = if name == "purchase_order.file" {
                rpc(&harness.daemon, name, &json!({ "order": 9 }))
            } else {
                ask(name, 9)
            };
            assert_eq!(
                reply["error"]["code"],
                crate::daemon::web::NOT_FOUND,
                "{name}: {reply}"
            );
        }
        // A comparison reached through a link is not read, and one that is gone is none.
        let evaluations = harness.procurement_folder().join("evaluations");
        let elsewhere = harness.project.repo.path.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("a folder");
        std::fs::write(elsewhere.join("mirrors.md"), "# Not the folder's").expect("a note");
        std::fs::remove_dir_all(&evaluations).expect("the notes go");
        std::os::unix::fs::symlink(&elsewhere, &evaluations).expect("a link");
        let linked = ask("purchase_order.evaluation", 1);
        assert_eq!(
            linked["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{linked}"
        );
        std::fs::remove_file(&evaluations).expect("the link goes");
        let gone = ask("purchase_order.evaluation", 1);
        assert_eq!(
            gone["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{gone}"
        );
        // A comparison past 64 KiB is not read, and one of exactly 64 KiB is.
        let note = harness.procurement_folder().join("evaluations/mirrors.md");
        std::fs::create_dir_all(note.parent().expect("a folder")).expect("the folder");
        std::fs::write(&note, "e".repeat(64 * 1024)).expect("a note");
        let exact = ask("purchase_order.evaluation", 1);
        assert_eq!(
            exact["result"]["text"].as_str().map(str::len),
            Some(64 * 1024),
            "{exact}"
        );
        std::fs::write(&note, "e".repeat(64 * 1024 + 1)).expect("a note");
        let big = ask("purchase_order.evaluation", 1);
        assert_eq!(big["error"]["code"], crate::daemon::web::NOT_FOUND, "{big}");
        // A workbook that is there for an order nobody drafted is none.
        std::fs::copy(
            harness.procurement_folder().join("orders/PO-1.xlsx"),
            harness.procurement_folder().join("orders/PO-9.xlsx"),
        )
        .expect("a workbook");
        let stray = rpc(
            &harness.daemon,
            "purchase_order.file",
            &json!({ "order": 9 }),
        );
        assert_eq!(
            stray["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{stray}"
        );
        // A workbook past 10 MiB is not sent, and one of exactly 10 MiB is.
        let workbook = harness.procurement_folder().join("orders/PO-1.xlsx");
        let big = std::fs::OpenOptions::new()
            .write(true)
            .open(&workbook)
            .expect("the file");
        big.set_len(10 * 1024 * 1024).expect("10 MiB");
        let at_the_limit = rpc(
            &harness.daemon,
            "purchase_order.file",
            &json!({ "order": 1 }),
        );
        assert!(
            at_the_limit.get("result").is_some(),
            "{}",
            at_the_limit.to_string().len()
        );
        big.set_len(10 * 1024 * 1024 + 1)
            .expect("10 MiB and a byte");
        let over = rpc(
            &harness.daemon,
            "purchase_order.file",
            &json!({ "order": 1 }),
        );
        assert_eq!(
            over["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{}",
            over.to_string().len()
        );
        std::fs::write(&workbook, b"PK restored").expect("a file");
        // A workbook that is gone is none, and so is one that is a link.
        let workbook = harness.procurement_folder().join("orders/PO-1.xlsx");
        std::fs::remove_file(&workbook).expect("the workbook goes");
        let missing = rpc(
            &harness.daemon,
            "purchase_order.file",
            &json!({ "order": 1 }),
        );
        assert_eq!(
            missing["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{missing}"
        );
        std::fs::write(elsewhere.join("secret.xlsx"), b"PKsecret").expect("a file");
        std::os::unix::fs::symlink(elsewhere.join("secret.xlsx"), &workbook).expect("a link");
        let linked = rpc(
            &harness.daemon,
            "purchase_order.file",
            &json!({ "order": 1 }),
        );
        assert_eq!(
            linked["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{linked}"
        );
        // An order that names another file as its comparison does not have it read.
        std::fs::write(
            harness.procurement_folder().join("vendors.xlsx"),
            b"PKregister",
        )
        .expect("the register");
        harness.project.record_by(
            Some("proc"),
            at(),
            "FRK-1",
            "purchase_order.drafted",
            &json!({
                "order": 2, "seller": "Bolt", "seller_contact": "",
                "lines": [{ "item": "x", "quantity": 1, "unit": "", "unit_price": "1.00", "line_total": "1.00" }],
                "currency": "USD", "period": "once", "total": "1.00", "delivery": "", "terms": "",
                "url": "", "evaluation": "vendors.xlsx", "why": "An order whose comparison is another file."
            }),
        );
        let register = ask("purchase_order.evaluation", 2);
        assert_eq!(
            register["error"]["code"],
            crate::daemon::web::NOT_FOUND,
            "{register}"
        );
        // The numbers are the schema's.
        for params in [json!({ "order": 0 }), json!({})] {
            let reply = rpc(&harness.daemon, "purchase_order.file", &params);
            assert_eq!(
                reply["error"]["code"],
                crate::daemon::web::INVALID_PARAMS,
                "{params}"
            );
        }
    }

    /// `proc`'s request, on FRK-1, to read `host`: the request's number.
    fn site_asked(harness: &Harness, host: &str) -> u64 {
        harness
            .project
            .record_by(
                Some("proc"),
                at(),
                "FRK-1",
                "site.requested",
                &json!({ "host": host, "url": format!("https://www.{host}/boxes"), "why": "A maker." }),
            )
            .envelope
            .seq
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waiting_lists_a_site_request() {
        let harness = Harness::with_procurement("gates-site-waits");
        harness.procurement_task("FRK-1", Some("in_progress"));
        let request = site_asked(&harness, "shop.example");
        let name = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team")
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "proc")
            .map(|agent| agent.display_name.to_string())
            .expect("proc");
        let waiting = || {
            query(
                &harness.daemon,
                "waiting.list",
                &json!({}),
                "waitingListResult",
            )["waiting"]
                .clone()
        };

        assert_eq!(
            waiting(),
            json!([{
                "task_id": "FRK-1", "kind": "site_request", "agent_id": "proc",
                "title": "Add a login page",
                "line": format!("{name} asks to read shop.example"),
                "request": request, "host": "shop.example",
                "url": "https://www.shop.example/boxes", "why": "A maker."
            }])
        );

        harness.project.record(
            "FRK-1",
            "site.approved",
            &json!({ "host": "shop.example", "request": request }),
        );
        assert_eq!(waiting(), json!([]), "gone once decided");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lists_the_sites() {
        let harness = Harness::with_procurement("gates-sites-list");
        harness.procurement_task("FRK-1", Some("in_progress"));
        let farik = farik_roles::sites::farik_sites();
        let minute = |n: i64| at() + chrono::Duration::minutes(n);
        let stamp = |n: i64| minute(n).to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
        // The owner turned one of Farik's off, and another off and on again, allowed a site a
        // request named, added one unasked, and two requests wait.
        harness.project.record_at(
            minute(1),
            "",
            "site.removed",
            &json!({ "host": farik[1].host }),
        );
        harness.project.record_at(
            minute(2),
            "",
            "site.removed",
            &json!({ "host": farik[2].host }),
        );
        harness.project.record_at(
            minute(3),
            "",
            "site.approved",
            &json!({ "host": farik[2].host }),
        );
        let allowed = site_asked(&harness, "allowed.example");
        harness.project.record_at(
            minute(4),
            "FRK-1",
            "site.approved",
            &json!({ "host": "allowed.example", "request": allowed }),
        );
        harness.project.record_at(
            minute(5),
            "",
            "site.approved",
            &json!({ "host": "added.example" }),
        );
        let later = site_asked(&harness, "wait.example");
        let earlier = site_asked(&harness, "again.example");

        let listed = query(&harness.daemon, "sites.list", &json!({}), "sitesListResult");

        let rows = listed["farik"].as_array().expect("a list");
        assert_eq!(rows.len(), farik.len(), "every entry, in the file's order");
        for (row, site) in rows.iter().zip(farik) {
            assert_eq!(row["host"], site.host);
            assert_eq!(row["shop"], site.shop);
            assert_eq!(row["category"], site.category.to_string());
        }
        assert_eq!(rows[0]["on"], true);
        assert!(
            rows[0].get("at").is_none(),
            "nothing was ever decided: {}",
            rows[0]
        );
        assert_eq!(
            (&rows[1]["on"], &rows[1]["at"]),
            (&json!(false), &json!(stamp(1)))
        );
        assert_eq!(
            (&rows[2]["on"], &rows[2]["at"]),
            (&json!(true), &json!(stamp(3))),
            "turned back on"
        );
        assert_eq!(
            listed["owner"],
            json!([
                { "host": "added.example", "at": stamp(5) },
                { "host": "allowed.example", "at": stamp(4), "request": allowed },
            ])
        );
        let waiting = listed["waiting"].as_array().expect("a list");
        let hosts: Vec<&str> = waiting
            .iter()
            .map(|row| row["host"].as_str().expect("a host"))
            .collect();
        assert_eq!(hosts, ["again.example", "wait.example"], "by host");
        assert_eq!(
            waiting[1],
            json!({
                "request": later, "host": "wait.example",
                "url": "https://www.wait.example/boxes", "why": "A maker.",
                "task_id": "FRK-1", "agent_id": "proc", "at": stamp(0)
            })
        );
        assert_eq!(waiting[0]["request"], earlier);
    }
}
