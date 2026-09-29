//! The human gates' queries and methods for the browser (`docs/SPEC.md` 5.4, 5.7, 5.16): what
//! waits on the human, a plan checked as it is typed and saved by the human, a task's diff, checks
//! and tries, the questions, each agent's activity, and what moved. `web.rs` answers the frames;
//! this module answers what they ask.

use std::fmt::Display;
use std::str::FromStr as _;

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
    RequestError, contract_write, file_request, placeholder_budget_usd, request_from_text,
    summary_of,
};
use farik_store::waiting::{name_of, waiting};
use serde_json::{Value, json};

use super::DaemonState;
use super::web::{Failure, INTERNAL_ERROR, NOT_FOUND, REFUSED, UNKNOWN_QUERY};
use crate::cost::extra_tries;
use crate::tools::ToolDeps;
use crate::tools::contracts::changed_fields;
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

/// The gates' queries, whose params the schema already passed.
pub(super) fn query(deps: &ToolDeps, name: &str, params: &Value) -> Result<Value, Failure> {
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    let team = || deps.files.read_team().map_err(|e| internal(&e));
    match name {
        "waiting.list" => {
            let listed = waiting(&deps.projections, &deps.log, &deps.files, &team()?)
                .map_err(|e| internal(&e))?;
            Ok(json!({ "waiting": listed.iter().map(|item| json!({
                "task_id": item.task_id,
                "kind": item.kind.as_str(),
                "agent_id": item.agent_id,
                "title": item.title,
                "line": item.line,
            })).collect::<Vec<_>>() }))
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
        "questions.list" => questions(deps, params["task_id"].as_str()),
        _ => {
            let task_id = task_of(deps, params)?;
            match name {
                "contract.get" => Ok(json!({ "contract": file_value(deps, &task_id)? })),
                "contract.check" => check(deps, &team()?, &task_id, &params["contract"]),
                "task.history" => Ok(json!({
                    "events": history(deps, &task_id)?.iter().map(event_to_value).collect::<Vec<_>>()
                })),
                "task.diff" => task_diff(deps, &team()?, &task_id),
                "task.checks" => task_checks(deps, &team()?, &task_id),
                "task.tries" => {
                    let row = row(deps, &task_id)?;
                    let contract = contract_of(deps, &task_id)?;
                    let allowed = u32::try_from(contract.budget.max_iterations.get())
                        .unwrap_or(u32::MAX)
                        .saturating_add(extra_tries(&history(deps, &task_id)?));
                    Ok(json!({ "used": row.iteration, "allowed": allowed }))
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
        return tokio::task::spawn_blocking(move || file_words(&deps, &text))
            .await
            .unwrap_or_else(|error| Err(internal(&error)));
    }
    save(state, &deps, params).await
}

/// `request.file`: the person's words filed as a draft request of the human's.
fn file_words(deps: &ToolDeps, text: &str) -> Result<Value, Failure> {
    let team = deps.files.read_team().map_err(|e| internal(&e))?;
    let request = request_from_text(text, placeholder_budget_usd(&team.rules()))
        .map_err(|sentence| Failure::new(REFUSED, sentence))?;
    let filed = file_request(
        &deps.files,
        &deps.log,
        request,
        HUMAN,
        None,
        deps.clock.now(),
        &deps.ids,
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => Failure::new(REFUSED, format!("the request {reason}")),
        other => internal(&other),
    })?;
    deps.projections.catch_up().map_err(|e| internal(&e))?;
    Ok(json!({ "task_id": filed.id }))
}

/// `contract.save`: the human's edit, judged by `check_contract_write`. A frozen plan goes back to
/// `refining` first (5.11), as the escalation's resolve when it awaits approval and as the human's
/// move otherwise; then the fields the human changed are written over the file and
/// `contract.written` is recorded as theirs.
async fn save(state: &DaemonState, deps: &ToolDeps, params: &Value) -> Result<Value, Failure> {
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
    if back {
        let command = if row.status == TaskStatus::Escalated {
            Command::EscalationResolve {
                task_id: task_id.clone(),
                to: TaskStatus::Refining,
                message: EDITED.to_string(),
                extra_tries: None,
            }
        } else {
            Command::TaskTransition {
                task_id: task_id.clone(),
                to: TaskStatus::Refining,
                reason: EDITED.to_string(),
            }
        };
        if let CommandReply::Error { detail, .. } = super::handled(state, command).await {
            return Err(Failure::new(REFUSED, detail));
        }
    }
    if changed.is_empty() {
        return Ok(json!({ "saved": true, "back_to_refining": back }));
    }
    // The move wrote the file's lifecycle fields, so the human's fields go over what it left.
    let mut written = file_value(deps, &task_id)?;
    for field in &changed {
        written[field] = after[field].clone();
        if after.get(field).is_none()
            && let Some(object) = written.as_object_mut()
        {
            object.remove(field);
        }
    }
    let mut written = validate_contract(&written)
        .map_err(|errors| Failure::new(REFUSED, schema_words(&errors)))?;
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
    Ok(json!({ "saved": true, "back_to_refining": back }))
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
        "integration" => vec![
            choice(
                "Add it now",
                json!({ "command": "task_integrate", "body": { "task_id": task_id } }),
            ),
            cancel,
        ],
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
mod tests {
    use std::sync::Arc;

    use farik_core::contract::TaskStatus;
    use farik_protocol::event::EventKind;
    use serde_json::{Value, json};

    use super::choices;
    use crate::daemon::DaemonState;
    use crate::orchestrator::command_handler;
    use crate::orchestrator::fixtures::Harness;

    /// The reply frame to `method` with `params`, answered on a runtime of its own.
    fn rpc(state: &Arc<DaemonState>, method: &str, params: &Value) -> Value {
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
    fn query(state: &Arc<DaemonState>, name: &str, params: &Value, definition: &str) -> Value {
        let reply = rpc(state, "query", &json!({ "name": name, "params": params }));
        conforms(&reply["result"], definition, &reply);
        reply["result"].clone()
    }

    /// The result of the method `method`, checked against `definition` of the RPC schema.
    fn call(state: &Arc<DaemonState>, method: &str, params: &Value, definition: &str) -> Value {
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
    fn driven(name: &str) -> Harness {
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

        let short = rpc(
            &harness.daemon,
            "request.file",
            &json!({ "text": "Make it nicer  " }),
        );
        assert_eq!(short["error"]["code"], -32005, "{short}");
        assert_eq!(
            short["error"]["message"],
            "say a little more: at least 20 characters"
        );
        assert_eq!(harness.project.events(&[EventKind::TaskCreated]).len(), 1);
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

        // Schema-valid, with no architect on the team to review it.
        draft["title"] = json!("Add a login page");
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
        assert!(
            kinds.ends_with(&[
                "task.transitioned",
                "escalation.resolved",
                "contract.written"
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
        for task in ["FRK-1", "FRK-2"] {
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

        // The epic joins its tasks' integrated diffs, in id order, under their ids.
        let second = commit("FRK-2", "three\n");
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
        let tries = query(
            &harness.daemon,
            "task.tries",
            &json!({ "task_id": "FRK-1" }),
            "taskTriesResult",
        );
        assert_eq!(tries, json!({ "used": 4, "allowed": 5 }));

        let none = query(
            &harness.daemon,
            "sprint.current",
            &json!({}),
            "sprintCurrentResult",
        );
        assert_eq!(none, Value::Null);
        harness.accepted("FRK-2");
        harness.ready("FRK-3");
        harness
            .project
            .open_sprint("S1", Some(50.0), &["FRK-2", "FRK-3"]);
        let sprint = query(
            &harness.daemon,
            "sprint.current",
            &json!({}),
            "sprintCurrentResult",
        );
        assert_eq!(sprint, json!({ "sprint_id": "S1", "done": 1, "total": 2 }));

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
        assert_eq!(
            offered("integration"),
            vec![
                json!({ "label": "Add it now", "body": { "command": "task_integrate", "body": { "task_id": "FRK-4" } } }),
                cancel.clone(),
            ]
        );
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
        assert!(offered("risk_gate").is_empty());
    }
}
