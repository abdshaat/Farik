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
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::Team;
use farik_protocol::command::{Command, CommandReply};
use farik_protocol::event::{
    ContractWrittenBody, EventBody, EventKind, FarikEvent, event_to_value, new_event,
};
use farik_store::EventQuery;
use farik_store::activity::{ActivityState, activity, moved_since};
use farik_store::diff::diff_of;
use farik_store::requests::{
    RequestError, TOO_SHORT, contract_write, file_request, placeholder_budget_usd,
    request_from_text, summary_of,
};
use farik_store::waiting::{name_of, waiting};
use serde_json::{Value, json};

use super::DaemonState;
use super::web::{Failure, INTERNAL_ERROR, NOT_FOUND, REFUSED, UNKNOWN_QUERY};
use crate::cost::extra_tries;
use crate::tools::ToolDeps;
use crate::tools::contracts::changed_fields;
use crate::tools::design::ReviewState;
use crate::transitions::last_move_into;

/// The methods this module answers.
pub(super) const METHODS: [&str; 2] = ["request.file", "contract.save"];

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

/// One row of `waiting.list`; a connector call's also names its approval, server, tool and input.
fn waiting_row(item: &farik_store::waiting::Waiting) -> Value {
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
    row
}

/// The gates' queries, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str, params: &Value) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let team = || deps.files.read_team().map_err(|e| internal(&e));
    match name {
        "waiting.list" => {
            let team = team()?;
            let listed = waiting(&deps.projections, &deps.log, &deps.files, &team)
                .map_err(|e| internal(&e))?;
            let mut rows: Vec<Value> = listed.iter().map(waiting_row).collect();
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

/// `task.diff`: the task's diff, or an epic's tasks' joined.
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
    Ok(
        json!({ "diff": diff.diff, "files": diff.files, "added": diff.added, "removed": diff.removed }),
    )
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
    if method == "request.file" {
        let text = params["text"].as_str().unwrap_or_default().to_string();
        let link = params["from_chat_message"].as_u64();
        let filed = off_the_worker(move || file_words(&deps, &text, link)).await?;
        state.wakes().notify_one();
        return Ok(filed);
    }
    save(state, &deps, params).await
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

    fn conforms(value: &Value, definition: &str, reply: &Value) {
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
        assert_eq!(checked["total"], 20, "{checked}");
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
        assert_eq!(checked, json!({ "failures": [], "total": 22 }));

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
}
