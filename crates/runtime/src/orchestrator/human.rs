//! The human's door (`docs/SPEC.md` sections 5.2, 5.7, 5.11, 5.14, and 5.16): every command the
//! human gives the orchestrator, each judged and recorded through the store and the governor, so
//! that any process may handle one.

use farik_core::contract::{TaskContract, TaskId, TaskKind, TaskStatus};
use farik_core::governor::done::requires_human_acceptance;
use farik_core::governor::gates::Blocker;
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{AgentStatus, Team};
use farik_protocol::command::{AcceptSubject, Command, RequestSize};
use farik_protocol::event::{
    AgentUpdatedBody, EscalationResolvedBody, EventBody, EventIds, EventKind, HumanAcceptedBody,
    HumanAcceptedBodySubject, QuestionAnsweredBody, new_event,
};
use farik_store::requests::{RequestError, hold_contract, triage_by_human};
use farik_store::{EventQuery, TaskProjection};

use super::requests::HUMAN;
use super::verify::{governor_results, is_human, is_mechanical, since_verifying};
use super::{CommandError, CommandReport, IntegrationOutcome, Orchestrator, OrchestratorError};
use crate::tools::ToolDeps;
use crate::transitions::{
    TransitionAsk, TransitionOutcome, contract_accepted, refusal_details, result_accepted,
    reviewed_by_the_human, status_wire,
};

/// Who the human is in the log.
/// The blocker of a task whose assignee the human paused, and a paused session's stop.
const PAUSED: &str = "agent paused by the user";
/// The blocker of a task whose assignee the human retired, and a retired agent's session's stop.
const RETIRED: &str = "agent retired by the user";
/// What resolves a block a pause made, when the agent is active again.
const RESUMED: &str = "agent resumed by the user";
/// Why the human stopped a session.
const STOPPED: &str = "stopped by the human";

/// Handles one command the human gave, after bringing this process's board up to the log, so that
/// a command another process handled is seen.
pub(super) async fn handle(
    orchestrator: &Orchestrator,
    command: Command,
) -> Result<CommandReport, CommandError> {
    let tools = &orchestrator.deps.tools;
    tools.projections.catch_up().map_err(failed)?;
    match command {
        Command::TaskCreate { .. } => Err(CommandError::Invalid {
            detail: "task_create is not taken here: a request gets its id when it is filed, and \
                     its contract already carries one"
                .to_string(),
        }),
        Command::RequestTriage {
            task_id,
            size,
            reason,
        } => triage(tools, &task_id, size, &reason),
        Command::ContractLock { task_id } => hold(tools, &task_id, true),
        Command::ContractUnlock { task_id } => hold(tools, &task_id, false),
        Command::QuestionAnswer {
            question_id,
            answer,
        } => answer_question(tools, question_id, &answer),
        Command::HumanAccept {
            task_id,
            subject: AcceptSubject::Contract,
            message,
        } => approve(tools, &task_id, message),
        Command::HumanAccept {
            task_id,
            subject: AcceptSubject::Result,
            message,
        } => accept_result(tools, &task_id, message),
        Command::EscalationResolve {
            task_id,
            to,
            message,
        } => resolve(tools, &task_id, to, &message),
        Command::TaskTransition {
            task_id,
            to,
            reason,
        } => transition(tools, &task_id, to, &reason),
        Command::TaskIntegrate { task_id } => integrate(orchestrator, &task_id).await,
        Command::AgentUpdate { agent_id, status } => update_agent(orchestrator, &agent_id, status),
        Command::SessionStop { session_id } => stop_session(orchestrator, &session_id),
        Command::RunStop => {
            orchestrator.stop();
            Ok(CommandReport {
                said: "the run stops before its next tick; a session already running runs to its \
                       end"
                .to_string(),
                events: Vec::new(),
            })
        }
    }
}

/// The human's triage of a draft, through the store (5.16).
fn triage(
    tools: &ToolDeps,
    task_id: &TaskId,
    size: RequestSize,
    reason: &str,
) -> Result<CommandReport, CommandError> {
    written(reason, "a triage's reason")?;
    row_of(tools, task_id)?;
    let event = triage_by_human(
        &tools.files,
        &tools.log,
        &tools.projections,
        task_id,
        size,
        reason,
        tools.clock.now(),
        &tools.ids,
    )
    .map_err(|error| from_request("triage_refused", error))?;
    let size = match size {
        RequestSize::Large => "large: an epic",
        RequestSize::Small => "small: a task",
    };
    Ok(CommandReport {
        said: format!("{} is {size}", task_id.as_str()),
        events: vec![event.envelope.seq],
    })
}

/// The human takes a contract, or gives it back, through the store (5.11).
fn hold(tools: &ToolDeps, task_id: &TaskId, held: bool) -> Result<CommandReport, CommandError> {
    row_of(tools, task_id)?;
    let event = hold_contract(
        &tools.files,
        &tools.log,
        &tools.projections,
        task_id,
        held,
        tools.clock.now(),
        &tools.ids,
    )
    .map_err(|error| from_request("lock_refused", error))?;
    Ok(CommandReport {
        said: if held {
            format!("{} is the human's", task_id.as_str())
        } else {
            format!("{} is the team's again", task_id.as_str())
        },
        events: vec![event.envelope.seq],
    })
}

/// A store refusal as the human's, under `kind`; anything else the store failed at.
fn from_request(kind: &str, error: RequestError) -> CommandError {
    match error {
        RequestError::Refused { reason } => CommandError::Refused {
            reason: format!("{kind}: {reason}"),
        },
        other => failed(other),
    }
}

/// Answers the question asked at `question_id` once (5.7): `question.answered`, with the
/// question's task on its envelope.
fn answer_question(
    tools: &ToolDeps,
    question_id: u64,
    answer: &str,
) -> Result<CommandReport, CommandError> {
    written(answer, "an answer")?;
    let asked = tools
        .log
        .read(&EventQuery {
            after_seq: Some(question_id.saturating_sub(1)),
            limit: Some(1),
            ..EventQuery::default()
        })
        .map_err(failed)?
        .into_iter()
        .find(|event| event.envelope.seq == question_id)
        .ok_or_else(|| CommandError::NotFound {
            what: format!("question {question_id}"),
        })?;
    if asked.body.kind() != EventKind::QuestionAsked {
        return Err(CommandError::Refused {
            reason: format!(
                "not_a_question: event {question_id} is a {}, not a question.asked",
                asked.body.kind()
            ),
        });
    }
    let answered = tools
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::QuestionAnswered],
            ..EventQuery::default()
        })
        .map_err(failed)?
        .iter()
        .any(|event| {
            matches!(&event.body, EventBody::QuestionAnswered(body) if body.question_id.get() == question_id)
        });
    if answered {
        return Err(CommandError::Refused {
            reason: format!("already_answered: question {question_id} has its answer"),
        });
    }
    let question_id_wire =
        std::num::NonZeroU64::new(question_id).ok_or_else(|| CommandError::NotFound {
            what: "question 0".to_string(),
        })?;
    let seq = append(
        tools,
        asked.envelope.ids.task_id.clone(),
        EventBody::QuestionAnswered(QuestionAnsweredBody {
            question_id: question_id_wire,
            answer: answer.to_string(),
            answered_by: HUMAN.to_string(),
        }),
    )?;
    Ok(CommandReport {
        said: format!("question {question_id} is answered"),
        events: vec![seq],
    })
}

/// Approves a contract awaiting approval (5.16 item 2): `human.accepted { contract }`, then
/// `escalated -> ready` as the human. The approval is recorded first because it is the authority
/// the move is made on; a move the governor refuses leaves it on a task still escalated and still
/// awaiting approval, which a second approval moves.
fn approve(
    tools: &ToolDeps,
    task_id: &TaskId,
    message: Option<String>,
) -> Result<CommandReport, CommandError> {
    let row = row_of(tools, task_id)?;
    if !row.awaiting_approval {
        return Err(CommandError::Refused {
            reason: format!(
                "not_awaiting_approval: {} is {} and no approval is asked of the human",
                task_id.as_str(),
                row.status
            ),
        });
    }
    let team = tools.files.read_team().map_err(failed)?;
    let mut events = vec![append(
        tools,
        Some(task_id.clone()),
        EventBody::HumanAccepted(HumanAcceptedBody {
            subject: HumanAcceptedBodySubject::Contract,
            accepted_by: HUMAN.to_string(),
            message: message.filter(|text| !text.trim().is_empty()),
        }),
    )?];
    events.extend(human_moves(
        tools,
        &team,
        task_id,
        TaskStatus::Ready,
        &TransitionAsk::default(),
    )?);
    Ok(CommandReport {
        said: format!("{} is approved and ready", task_id.as_str()),
        events,
    })
}

/// Accepts the result of a `verifying` task that waits for the human (5.4): one of risk `high`
/// or with a `human` criterion, which moves nothing, or an epic the human reviews (ADR 0013), once
/// Farik has run and passed each of its mechanical criteria and with the human's words.
fn accept_result(
    tools: &ToolDeps,
    task_id: &TaskId,
    message: Option<String>,
) -> Result<CommandReport, CommandError> {
    let row = row_of(tools, task_id)?;
    let contract = tools.files.read_contract(task_id).map_err(failed)?;
    let team = tools.files.read_team().map_err(failed)?;
    if row.status != TaskStatus::Verifying {
        return Err(not_waiting(&row));
    }
    let history = history_of(tools, task_id)?;
    if result_accepted(&history).is_some() {
        return Err(CommandError::Refused {
            reason: format!(
                "already_accepted: the human accepted {}'s result in this verification",
                task_id.as_str()
            ),
        });
    }
    let message = message.filter(|text| !text.trim().is_empty());
    if reviewed_by_the_human(&contract, &team) {
        mechanical_criteria_passed(&contract, &history)?;
        if message.is_none() {
            return Err(CommandError::Invalid {
                detail: "an epic's acceptance is its review note: say what you checked".to_string(),
            });
        }
    } else if !requires_human_acceptance(&contract) && !has_human_criterion(&contract) {
        return Err(not_waiting(&row));
    }
    let seq = append(
        tools,
        Some(task_id.clone()),
        EventBody::HumanAccepted(HumanAcceptedBody {
            subject: HumanAcceptedBodySubject::Result,
            accepted_by: HUMAN.to_string(),
            message,
        }),
    )?;
    Ok(CommandReport {
        said: format!("{}'s result is accepted by the human", task_id.as_str()),
        events: vec![seq],
    })
}

/// Every event of the task, oldest first.
fn history_of(
    tools: &ToolDeps,
    task_id: &TaskId,
) -> Result<Vec<farik_protocol::event::FarikEvent>, CommandError> {
    tools
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            ..EventQuery::default()
        })
        .map_err(failed)
}

fn not_waiting(row: &TaskProjection) -> CommandError {
    CommandError::Refused {
        reason: format!(
            "not_waiting_for_the_human: {} is {}, and its result does not wait for the human",
            row.task_id.as_str(),
            row.status
        ),
    }
}

fn has_human_criterion(contract: &TaskContract) -> bool {
    contract.exit_criteria.iter().any(is_human)
}

/// Every `command`, `test`, or `artifact` criterion of the epic has Farik's passing result since
/// it last moved into `verifying` (ADR 0013).
fn mechanical_criteria_passed(
    contract: &TaskContract,
    history: &[farik_protocol::event::FarikEvent],
) -> Result<(), CommandError> {
    let results = governor_results(history, since_verifying(history));
    let mechanical: Vec<String> = contract
        .exit_criteria
        .iter()
        .filter(|criterion| is_mechanical(criterion))
        .map(|criterion| criterion.id.to_string())
        .collect();
    let not_run: Vec<&str> = mechanical
        .iter()
        .filter(|id| !results.iter().any(|result| &result.criterion_id == *id))
        .map(String::as_str)
        .collect();
    if !not_run.is_empty() {
        return Err(CommandError::Refused {
            reason: format!(
                "criteria_not_run: Farik has not yet run {} on the integration branch",
                not_run.join(", ")
            ),
        });
    }
    let failed: Vec<&str> = results
        .iter()
        .filter(|result| mechanical.contains(&result.criterion_id) && !result.passed)
        .map(|result| result.criterion_id.as_str())
        .collect();
    if !failed.is_empty() {
        return Err(CommandError::Refused {
            reason: format!(
                "criterion_failed: {} failed on the integration branch: escalate the epic and \
                 send it back to in_progress with what is missing",
                failed.join(", ")
            ),
        });
    }
    Ok(())
}

/// Resolves an escalation (5.7): `escalated -> to` as the human, then, on the move,
/// `escalation.resolved { to, message }`. Never to `ready` while the contract awaits approval,
/// which is `HumanAccept`'s, nor for an epic whose contract the human has not approved, which
/// reaches `ready` only by that approval (5.16 item 2).
fn resolve(
    tools: &ToolDeps,
    task_id: &TaskId,
    to: TaskStatus,
    message: &str,
) -> Result<CommandReport, CommandError> {
    written(message, "a resolution's message")?;
    let row = row_of(tools, task_id)?;
    if row.status != TaskStatus::Escalated {
        return Err(CommandError::Refused {
            reason: format!(
                "not_escalated: {} is {}, and only an escalation is resolved",
                task_id.as_str(),
                row.status
            ),
        });
    }
    if to == TaskStatus::Ready && row.awaiting_approval {
        return Err(CommandError::Refused {
            reason: format!(
                "use_human_accept: {} awaits the human's approval, which human_accept of its \
                 contract gives",
                task_id.as_str()
            ),
        });
    }
    if to == TaskStatus::Ready
        && row.kind == TaskKind::Epic
        && !contract_accepted(&history_of(tools, task_id)?)
    {
        return Err(CommandError::Refused {
            reason: format!(
                "use_human_accept: {} is an epic whose contract the human has not approved, and \
                 an epic reaches ready only by human_accept of its contract: resolve it to \
                 refining, and once its contract passes it waits for that approval",
                task_id.as_str()
            ),
        });
    }
    let team = tools.files.read_team().map_err(failed)?;
    let mut events = human_moves(
        tools,
        &team,
        task_id,
        to,
        &TransitionAsk {
            reason: Some(message.to_string()),
            ..TransitionAsk::default()
        },
    )?;
    events.push(append(
        tools,
        Some(task_id.clone()),
        EventBody::EscalationResolved(EscalationResolvedBody {
            to: status_wire(to).map_err(failed)?,
            message: message.to_string(),
            resolved_by: HUMAN.to_string(),
        }),
    )?);
    Ok(CommandReport {
        said: format!("{} is resolved to {to}", task_id.as_str()),
        events,
    })
}

/// Moves a task as the human (5.2), from any status but `escalated`, whose way out is `resolve`.
/// The reason is recorded on the move, and out of `blocked` it is also the blocker's resolution.
fn transition(
    tools: &ToolDeps,
    task_id: &TaskId,
    to: TaskStatus,
    reason: &str,
) -> Result<CommandReport, CommandError> {
    written(reason, "a move's reason")?;
    let row = row_of(tools, task_id)?;
    if row.status == TaskStatus::Escalated {
        return Err(CommandError::Refused {
            reason: format!(
                "use_escalation_resolve: {} is escalated, and escalation_resolve moves it with a \
                 message for the next session",
                task_id.as_str()
            ),
        });
    }
    if row.status == to {
        return Err(CommandError::Refused {
            reason: format!("same_status: {} is already {to}", task_id.as_str()),
        });
    }
    let team = tools.files.read_team().map_err(failed)?;
    let events = human_moves(
        tools,
        &team,
        task_id,
        to,
        &TransitionAsk {
            reason: Some(reason.to_string()),
            blocker_resolution: (row.status == TaskStatus::Blocked).then(|| reason.to_string()),
            ..TransitionAsk::default()
        },
    )?;
    Ok(CommandReport {
        said: format!("{} moved from {} to {to}", task_id.as_str(), row.status),
        events,
    })
}

/// Integrates an accepted task now (step 13's `integrate`).
async fn integrate(
    orchestrator: &Orchestrator,
    task_id: &TaskId,
) -> Result<CommandReport, CommandError> {
    row_of(&orchestrator.deps.tools, task_id)?;
    let before = last_seq(&orchestrator.deps.tools, task_id)?;
    let said = match orchestrator.integrate(task_id).await {
        Ok(IntegrationOutcome::Merged { sha }) => {
            format!(
                "{} merged into the integration branch at {sha}",
                task_id.as_str()
            )
        }
        Ok(IntegrationOutcome::PullRequestOpened { url }) => {
            format!("{}: pull request opened at {url}", task_id.as_str())
        }
        Ok(IntegrationOutcome::AwaitingForge) => format!(
            "{}'s pull request is open on the forge, waiting for its merge",
            task_id.as_str()
        ),
        Ok(IntegrationOutcome::Escalated { detail }) => {
            format!("{} could not be integrated: {detail}", task_id.as_str())
        }
        Err(OrchestratorError::Refused { reason }) => {
            return Err(CommandError::Refused { reason });
        }
        Err(error) => return Err(failed(error)),
    };
    Ok(CommandReport {
        said,
        events: seqs_since(&orchestrator.deps.tools, task_id, before)?,
    })
}

/// Changes an agent's status in the team file and records `agent.updated` (F1). A pause or a
/// retirement stops the agent's registered sessions at once and its sessions elsewhere at their
/// next tool call, and blocks each of its `in_progress` tasks on its behalf; a return to `active`
/// takes each task a pause blocked back to `in_progress`, as the human.
fn update_agent(
    orchestrator: &Orchestrator,
    agent_id: &str,
    status: AgentStatus,
) -> Result<CommandReport, CommandError> {
    let tools = &orchestrator.deps.tools;
    let mut team = tools.files.read_team().map_err(failed)?;
    let Some(agent) = team
        .agents
        .iter_mut()
        .find(|agent| agent.id.as_str() == agent_id)
    else {
        return Err(CommandError::NotFound {
            what: format!("agent {agent_id}"),
        });
    };
    if agent.status == status {
        return Err(CommandError::Refused {
            reason: format!("same_status: {agent_id} is already {status}"),
        });
    }
    agent.status = status;
    tools.files.write_team(&team).map_err(failed)?;
    let mut events = vec![append(
        tools,
        None,
        EventBody::AgentUpdated(AgentUpdatedBody {
            agent_id: agent_id.to_string(),
            status: status
                .to_string()
                .parse()
                .map_err(|_| CommandError::Failed {
                    detail: format!("the event vocabulary has no agent status {status}"),
                })?,
            updated_by: HUMAN.to_string(),
        }),
    )?];
    let board = tools.projections.board().map_err(failed)?;
    let held = board
        .iter()
        .filter(|row| row.assignee_id.as_deref() == Some(agent_id));
    match status {
        AgentStatus::Paused | AgentStatus::Retired => {
            let (words, needed) = if status == AgentStatus::Paused {
                (PAUSED, format!("the human resumes {agent_id}"))
            } else {
                (RETIRED, "the human reassigns the task".to_string())
            };
            for session_id in orchestrator.deps.daemon.sessions_of(agent_id) {
                orchestrator.deps.daemon.request_stop(&session_id, words);
            }
            for row in held.filter(|row| row.status == TaskStatus::InProgress) {
                events.extend(moved_for(
                    tools,
                    &team,
                    &TransitionRequest {
                        task_id: row.task_id.clone(),
                        to: TaskStatus::Blocked,
                        actor: TransitionActor::Assignee,
                        agent_id: Some(agent_id.to_string()),
                    },
                    &TransitionAsk {
                        blocker: Some(Blocker {
                            description: words.to_string(),
                            needed: needed.clone(),
                        }),
                        ..TransitionAsk::default()
                    },
                )?);
            }
        }
        AgentStatus::Active => {
            for row in held.filter(|row| row.status == TaskStatus::Blocked) {
                if last_blocker(tools, &row.task_id)?.as_deref() != Some(PAUSED) {
                    continue;
                }
                events.extend(moved_for(
                    tools,
                    &team,
                    &TransitionRequest {
                        task_id: row.task_id.clone(),
                        to: TaskStatus::InProgress,
                        actor: TransitionActor::Human,
                        agent_id: None,
                    },
                    &TransitionAsk {
                        blocker_resolution: Some(RESUMED.to_string()),
                        ..TransitionAsk::default()
                    },
                )?);
            }
        }
    }
    Ok(CommandReport {
        said: format!("{agent_id} is {status}"),
        events,
    })
}

/// Stops a session registered in this process, and escalates its task as the human unless the
/// task is finished or escalated already (5.2: a user's stop takes the human's row).
fn stop_session(
    orchestrator: &Orchestrator,
    session_id: &str,
) -> Result<CommandReport, CommandError> {
    let daemon = &orchestrator.deps.daemon;
    let task_id = daemon
        .tool_context(session_id)
        .and_then(|context| context.task_id);
    if !daemon.request_stop(session_id, STOPPED) {
        return Err(CommandError::NotFound {
            what: format!("session {session_id} running in this process"),
        });
    }
    let tools = &orchestrator.deps.tools;
    let mut events = Vec::new();
    if let Some(task_id) = task_id {
        let row = row_of(tools, &task_id)?;
        if !matches!(
            row.status,
            TaskStatus::Accepted | TaskStatus::Cancelled | TaskStatus::Escalated
        ) {
            let team = tools.files.read_team().map_err(failed)?;
            events = moved_for(
                tools,
                &team,
                &TransitionRequest {
                    task_id,
                    to: TaskStatus::Escalated,
                    actor: TransitionActor::Human,
                    agent_id: None,
                },
                &TransitionAsk {
                    reason: Some(STOPPED.to_string()),
                    ..TransitionAsk::default()
                },
            )?;
        }
    }
    Ok(CommandReport {
        said: format!("session {session_id} is stopped"),
        events,
    })
}

/// The description of the task's last blocker, as its last move into `blocked` recorded it.
fn last_blocker(tools: &ToolDeps, task_id: &TaskId) -> Result<Option<String>, CommandError> {
    Ok(tools
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![EventKind::TaskTransitioned],
            ..EventQuery::default()
        })
        .map_err(failed)?
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            EventBody::TaskTransitioned(body) if body.to.to_string() == "blocked" => Some(
                body.blocker
                    .as_ref()
                    .map(|blocker| blocker.description.clone()),
            ),
            _ => None,
        })
        .flatten())
}

/// Asks for a move on an actor's behalf, and answers the events it appended; a refusal is left
/// in the log, as every refusal is, and answers none.
fn moved_for(
    tools: &ToolDeps,
    team: &Team,
    request: &TransitionRequest,
    ask: &TransitionAsk,
) -> Result<Vec<u64>, CommandError> {
    let before = last_seq(tools, &request.task_id)?;
    match tools
        .transitions
        .request(request, ask, team)
        .map_err(failed)?
    {
        TransitionOutcome::Moved(_) => seqs_since(tools, &request.task_id, before),
        TransitionOutcome::Refused(_) => Ok(Vec::new()),
    }
}

/// Asks the governor to move the task to `to` as the human, and answers the events it appended;
/// a refusal is `transition_refused` with the governor's details, the `transition.refused` left in
/// the log.
fn human_moves(
    tools: &ToolDeps,
    team: &Team,
    task_id: &TaskId,
    to: TaskStatus,
    ask: &TransitionAsk,
) -> Result<Vec<u64>, CommandError> {
    let before = last_seq(tools, task_id)?;
    let outcome = tools
        .transitions
        .request(
            &TransitionRequest {
                task_id: task_id.clone(),
                to,
                actor: TransitionActor::Human,
                agent_id: None,
            },
            ask,
            team,
        )
        .map_err(failed)?;
    match outcome {
        TransitionOutcome::Moved(_) => seqs_since(tools, task_id, before),
        TransitionOutcome::Refused(refusal) => Err(CommandError::Refused {
            reason: format!(
                "transition_refused: {}",
                refusal_details(&refusal).join("; ")
            ),
        }),
    }
}

/// The task's row, or `NotFound` naming it.
fn row_of(tools: &ToolDeps, task_id: &TaskId) -> Result<TaskProjection, CommandError> {
    tools
        .projections
        .task(task_id)
        .map_err(failed)?
        .ok_or_else(|| CommandError::NotFound {
            what: format!("task {}", task_id.as_str()),
        })
}

/// `Invalid` when the human's words are blank.
fn written(text: &str, what: &str) -> Result<(), CommandError> {
    if text.trim().is_empty() {
        return Err(CommandError::Invalid {
            detail: format!("{what} is blank, and the log is where somebody reads it back"),
        });
    }
    Ok(())
}

/// The sequence number of the last event about the task, or 0.
fn last_seq(tools: &ToolDeps, task_id: &TaskId) -> Result<u64, CommandError> {
    Ok(tools
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            ..EventQuery::default()
        })
        .map_err(failed)?
        .last()
        .map_or(0, |event| event.envelope.seq))
}

/// The sequence numbers of the events about the task after `after`.
fn seqs_since(tools: &ToolDeps, task_id: &TaskId, after: u64) -> Result<Vec<u64>, CommandError> {
    Ok(tools
        .log
        .read(&EventQuery {
            after_seq: Some(after),
            task_id: Some(task_id.clone()),
            ..EventQuery::default()
        })
        .map_err(failed)?
        .iter()
        .map(|event| event.envelope.seq)
        .collect())
}

/// Appends one event of the human's, about `task_id` when there is one, projects it, and answers
/// its sequence number.
fn append(tools: &ToolDeps, task_id: Option<TaskId>, body: EventBody) -> Result<u64, CommandError> {
    let ids = EventIds {
        task_id,
        ..tools.ids.clone()
    };
    let event = new_event(body, tools.clock.now(), ids).map_err(|error| CommandError::Failed {
        detail: format!("the event cannot be recorded: {error:?}"),
    })?;
    let appended = tools.log.append(&event).map_err(failed)?;
    tools.projections.apply(&appended).map_err(failed)?;
    Ok(appended.envelope.seq)
}

fn failed(error: impl std::fmt::Display) -> CommandError {
    CommandError::Failed {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use farik_core::contract::TaskStatus;
    use farik_core::pricing::Usage;
    use farik_core::team::AgentStatus;
    use farik_protocol::command::{AcceptSubject, Command, RequestSize};
    use farik_protocol::event::{
        EscalationRaisedBodyReason, EventBody, EventKind, FarikEvent, HumanAcceptedBodySubject,
        SessionEndedBodyReason,
    };
    use serde_json::{Value, json};

    use crate::orchestrator::fixtures::{Harness, UsageThenWaitAdapter};
    use crate::orchestrator::{CommandError, CommandReport, Orchestrator};

    fn task(id: &str) -> farik_core::contract::TaskId {
        id.parse().expect("a task id")
    }

    fn an_orchestrator(harness: &Harness) -> Orchestrator {
        harness.orchestrator(harness.recorded(Vec::new()))
    }

    async fn handled(orchestrator: &Orchestrator, command: Command) -> CommandReport {
        orchestrator
            .handle(command.clone())
            .await
            .unwrap_or_else(|error| panic!("{command:?} is handled: {error:?}"))
    }

    async fn refused(orchestrator: &Orchestrator, command: Command) -> String {
        match orchestrator.handle(command.clone()).await {
            Err(CommandError::Refused { reason }) => reason,
            other => panic!("{command:?} is refused, not {other:?}"),
        }
    }

    fn last(harness: &Harness, kind: EventKind) -> Option<FarikEvent> {
        harness.events(&[kind]).pop()
    }

    /// An epic the Product Manager wrote for the human to review, with `criteria`, filed in
    /// `status`.
    fn an_epic(harness: &Harness, id: &str, status: &str, criteria: Value) {
        harness
            .project
            .filed_with(id, status, "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
                wire["allowed_paths"] = json!(["done.txt"]);
                wire["exit_criteria"] = criteria;
            });
    }

    fn a_command_criterion() -> Value {
        json!([{
            "id": "C1",
            "text": "done.txt exists.",
            "verification": { "method": "command", "command": "test -f done.txt", "expect": {} }
        }])
    }

    /// A passing or failing result for C1 of `id`, as Farik's run for the reviewer.
    fn governor_result(harness: &Harness, id: &str, passed: bool) {
        harness.project.record(
            id,
            "criterion.recorded",
            &json!({
                "criterion_id": "C1",
                "passed": passed,
                "evidence": "at abc: exit 0",
                "run_by": "reviewer",
                "recorded_by": "governor"
            }),
        );
    }

    /// `id` escalated with `reason`, from `refining`.
    fn escalated(harness: &Harness, id: &str, reason: &str) {
        harness
            .project
            .moved(id, "refining", "escalated", &json!({}));
        harness.project.record(
            id,
            "escalation.raised",
            &json!({ "reason": reason, "detail": "contract_requires_human" }),
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_each_of_two_questions_on_its_own() {
        let harness = Harness::new("human-two-answers", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        let asked = |question: &str| {
            harness
                .project
                .record(
                    "FRK-1",
                    "question.asked",
                    &json!({ "question": question, "asked_by": "pm" }),
                )
                .envelope
                .seq
        };
        let first = asked("Should done.txt be empty?");
        let second = asked("Should it end with a newline?");
        let orchestrator = an_orchestrator(&harness);
        handled(
            &orchestrator,
            Command::QuestionAnswer {
                question_id: first,
                answer: "Yes.".to_string(),
            },
        )
        .await;
        assert!(harness.row("FRK-1").waiting_on_human);

        let report = handled(
            &orchestrator,
            Command::QuestionAnswer {
                question_id: second,
                answer: "No.".to_string(),
            },
        )
        .await;
        let answer = last(&harness, EventKind::QuestionAnswered).expect("the answer is recorded");
        assert_eq!(report.events, vec![answer.envelope.seq]);
        let EventBody::QuestionAnswered(body) = &answer.body else {
            panic!("an answer");
        };
        assert_eq!(body.question_id.get(), second);
        assert!(!harness.row("FRK-1").waiting_on_human);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_a_question_once() {
        let harness = Harness::new("human-answer", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        let question = harness.project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Should done.txt be empty?", "asked_by": "pm" }),
        );
        let n = question.envelope.seq;
        let orchestrator = an_orchestrator(&harness);

        let report = handled(
            &orchestrator,
            Command::QuestionAnswer {
                question_id: n,
                answer: "Yes.".to_string(),
            },
        )
        .await;
        let answer = last(&harness, EventKind::QuestionAnswered).expect("the answer is recorded");
        assert_eq!(report.events, vec![answer.envelope.seq]);
        assert_eq!(answer.envelope.ids.task_id, Some(task("FRK-1")));
        let EventBody::QuestionAnswered(body) = &answer.body else {
            panic!("an answer");
        };
        assert_eq!(body.question_id.get(), n);
        assert_eq!(body.answer, "Yes.");
        assert_eq!(body.answered_by, "human");

        let again = refused(
            &orchestrator,
            Command::QuestionAnswer {
                question_id: n,
                answer: "No.".to_string(),
            },
        )
        .await;
        assert!(again.starts_with("already_answered"), "{again}");
        let created = harness.events(&[EventKind::TaskCreated])[0].envelope.seq;
        let not_one = refused(
            &orchestrator,
            Command::QuestionAnswer {
                question_id: created,
                answer: "Yes.".to_string(),
            },
        )
        .await;
        assert!(not_one.starts_with("not_a_question"), "{not_one}");
        assert!(matches!(
            orchestrator
                .handle(Command::QuestionAnswer {
                    question_id: answer.envelope.seq + 10,
                    answer: "Yes.".to_string(),
                })
                .await,
            Err(CommandError::NotFound { .. })
        ));
        assert!(matches!(
            orchestrator
                .handle(Command::QuestionAnswer {
                    question_id: n,
                    answer: "  ".to_string(),
                })
                .await,
            Err(CommandError::Invalid { .. })
        ));
        assert_eq!(harness.events(&[EventKind::QuestionAnswered]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn approves_a_contract_awaiting_approval() {
        let harness = Harness::new("human-approve", |_| {});
        an_epic(&harness, "FRK-1", "refining", a_command_criterion());
        escalated(&harness, "FRK-1", "approval");
        harness.ready("FRK-2");
        assert!(harness.row("FRK-1").awaiting_approval);
        let orchestrator = an_orchestrator(&harness);

        let report = handled(
            &orchestrator,
            Command::HumanAccept {
                task_id: task("FRK-1"),
                subject: AcceptSubject::Contract,
                message: None,
            },
        )
        .await;
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (
                body.from.to_string(),
                body.to.to_string(),
                body.actor.to_string()
            ),
            (
                "escalated".to_string(),
                "ready".to_string(),
                "human".to_string()
            )
        );
        // The approval is the authority the move is made on, so it is recorded first.
        let accepted = last(&harness, EventKind::HumanAccepted).expect("the approval");
        assert!(accepted.envelope.seq < moved.envelope.seq);
        let EventBody::HumanAccepted(body) = &accepted.body else {
            panic!("an acceptance");
        };
        assert_eq!(body.subject, HumanAcceptedBodySubject::Contract);
        assert_eq!(report.events.first(), Some(&accepted.envelope.seq));
        assert_eq!(report.events.last(), Some(&moved.envelope.seq));
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::Ready);
        assert!(!row.awaiting_approval);

        let not = refused(
            &orchestrator,
            Command::HumanAccept {
                task_id: task("FRK-2"),
                subject: AcceptSubject::Contract,
                message: None,
            },
        )
        .await;
        assert!(not.starts_with("not_awaiting_approval"), "{not}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn accepts_a_task_result_only_where_the_human_is_asked() {
        let harness = Harness::new("human-accept-result", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(4);
        });
        harness.verifying_with("FRK-1", true, true, |wire| wire["risk"] = json!("high"));
        harness.verifying("FRK-2");
        harness.file("FRK-3", "in_progress", |wire| wire["risk"] = json!("high"));
        harness.verifying_with("FRK-4", true, true, |wire| {
            wire["exit_criteria"]
                .as_array_mut()
                .expect("a list of criteria")
                .push(json!({
                    "id": "C2",
                    "text": "The founder read it.",
                    "satisfies": ["R1"],
                    "verification": { "method": "human", "question": "Is it right?" }
                }));
        });
        let orchestrator = an_orchestrator(&harness);
        let accept = |id: &str| Command::HumanAccept {
            task_id: task(id),
            subject: AcceptSubject::Result,
            message: None,
        };
        let moves = harness.events(&[EventKind::TaskTransitioned]).len();

        handled(&orchestrator, accept("FRK-1")).await;
        let accepted = last(&harness, EventKind::HumanAccepted).expect("the acceptance");
        let EventBody::HumanAccepted(body) = &accepted.body else {
            panic!("an acceptance");
        };
        assert_eq!(body.subject, HumanAcceptedBodySubject::Result);
        assert_eq!(harness.events(&[EventKind::TaskTransitioned]).len(), moves);
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Verifying);

        let again = refused(&orchestrator, accept("FRK-1")).await;
        assert!(again.starts_with("already_accepted"), "{again}");
        let low = refused(&orchestrator, accept("FRK-2")).await;
        assert!(low.starts_with("not_waiting_for_the_human"), "{low}");
        // A high-risk task waits for the human only once it is verifying.
        let working = refused(&orchestrator, accept("FRK-3")).await;
        assert!(
            working.starts_with("not_waiting_for_the_human"),
            "{working}"
        );
        // A low-risk task with a human criterion waits for the human's answer.
        handled(&orchestrator, accept("FRK-4")).await;
        let accepted = last(&harness, EventKind::HumanAccepted).expect("the acceptance");
        assert_eq!(accepted.envelope.ids.task_id, Some(task("FRK-4")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn accepts_an_epic_only_after_farik_ran_its_criteria() {
        let harness = Harness::new("human-accept-epic", |_| {});
        an_epic(&harness, "FRK-1", "in_progress", a_command_criterion());
        let mut with_an_artifact = a_command_criterion();
        with_an_artifact
            .as_array_mut()
            .expect("a list of criteria")
            .push(json!({
                "id": "C2",
                "text": "done.txt says done.",
                "verification": { "method": "artifact", "path": "done.txt", "must_contain": ["done"] }
            }));
        an_epic(&harness, "FRK-2", "in_progress", with_an_artifact);
        harness.project.moved(
            "FRK-2",
            "in_progress",
            "verifying",
            &json!({ "assignee": "pm" }),
        );
        governor_result(&harness, "FRK-2", true);
        harness.project.moved(
            "FRK-1",
            "in_progress",
            "verifying",
            &json!({ "assignee": "pm" }),
        );
        let orchestrator = an_orchestrator(&harness);
        let accept = |message: Option<&str>| Command::HumanAccept {
            task_id: task("FRK-1"),
            subject: AcceptSubject::Result,
            message: message.map(str::to_string),
        };

        let not_run = refused(&orchestrator, accept(Some("Looks right."))).await;
        assert!(not_run.starts_with("criteria_not_run"), "{not_run}");
        // An artifact criterion is Farik's to run too.
        let artifact = refused(
            &orchestrator,
            Command::HumanAccept {
                task_id: task("FRK-2"),
                subject: AcceptSubject::Result,
                message: Some("Looks right.".to_string()),
            },
        )
        .await;
        assert!(
            artifact.starts_with("criteria_not_run") && artifact.contains("C2"),
            "{artifact}"
        );
        governor_result(&harness, "FRK-1", false);
        let failed = refused(&orchestrator, accept(Some("Looks right."))).await;
        assert!(failed.starts_with("criterion_failed"), "{failed}");
        governor_result(&harness, "FRK-1", true);
        assert!(matches!(
            orchestrator.handle(accept(None)).await,
            Err(CommandError::Invalid { .. })
        ));
        assert!(harness.events(&[EventKind::HumanAccepted]).is_empty());

        handled(&orchestrator, accept(Some("Looks right."))).await;
        let accepted = last(&harness, EventKind::HumanAccepted).expect("the acceptance");
        let EventBody::HumanAccepted(body) = &accepted.body else {
            panic!("an acceptance");
        };
        assert_eq!(body.message.as_deref(), Some("Looks right."));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn resolves_an_escalation_with_a_status_and_a_message() {
        let harness = Harness::new("human-resolve", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        escalated(&harness, "FRK-1", "readiness_failures");
        an_epic(&harness, "FRK-2", "refining", a_command_criterion());
        escalated(&harness, "FRK-2", "approval");
        harness.ready("FRK-3");
        let orchestrator = an_orchestrator(&harness);
        let resolve = |id: &str, to: TaskStatus, message: &str| Command::EscalationResolve {
            task_id: task(id),
            to,
            message: message.to_string(),
        };

        handled(
            &orchestrator,
            resolve("FRK-1", TaskStatus::Refining, "Split it by page."),
        )
        .await;
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("escalated".to_string(), "refining".to_string())
        );
        let resolved = last(&harness, EventKind::EscalationResolved).expect("the resolution");
        assert!(resolved.envelope.seq > moved.envelope.seq);
        let EventBody::EscalationResolved(body) = &resolved.body else {
            panic!("a resolution");
        };
        assert_eq!(body.to.to_string(), "refining");
        assert_eq!(body.message, "Split it by page.");
        assert_eq!(body.resolved_by, "human");

        let approval = refused(&orchestrator, resolve("FRK-2", TaskStatus::Ready, "Go.")).await;
        assert!(approval.starts_with("use_human_accept"), "{approval}");
        let ready = refused(&orchestrator, resolve("FRK-3", TaskStatus::Refining, "Go.")).await;
        assert!(ready.starts_with("not_escalated"), "{ready}");
        assert!(matches!(
            orchestrator
                .handle(resolve("FRK-2", TaskStatus::Refining, " "))
                .await,
            Err(CommandError::Invalid { .. })
        ));
        let governor = refused(
            &orchestrator,
            resolve("FRK-2", TaskStatus::Escalated, "Stay."),
        )
        .await;
        assert!(governor.starts_with("transition_refused"), "{governor}");
        assert_eq!(harness.events(&[EventKind::EscalationResolved]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn keeps_an_unapproved_epic_from_ready() {
        let harness = Harness::new("human-resolve-epic", |_| {});
        an_epic(&harness, "FRK-1", "refining", a_command_criterion());
        escalated(&harness, "FRK-1", "readiness_failures");
        an_epic(&harness, "FRK-2", "refining", a_command_criterion());
        harness.project.record(
            "FRK-2",
            "human.accepted",
            &json!({ "subject": "contract", "accepted_by": "human" }),
        );
        escalated(&harness, "FRK-2", "budget");
        let orchestrator = an_orchestrator(&harness);
        let to_ready = |id: &str| Command::EscalationResolve {
            task_id: task(id),
            to: TaskStatus::Ready,
            message: "Go.".to_string(),
        };

        let unapproved = refused(&orchestrator, to_ready("FRK-1")).await;
        assert!(unapproved.starts_with("use_human_accept"), "{unapproved}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert!(harness.events(&[EventKind::EscalationResolved]).is_empty());

        handled(&orchestrator, to_ready("FRK-2")).await;
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Ready);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn moves_a_task_for_the_human() {
        let harness = Harness::new("human-move", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(4);
        });
        harness.blocked("FRK-1", "dev-a", "dev-b");
        harness.ready("FRK-2");
        harness.file("FRK-3", "refining", |_| {});
        escalated(&harness, "FRK-3", "readiness_failures");
        harness.in_progress("FRK-4", "dev-a", "dev-b");
        let orchestrator = an_orchestrator(&harness);
        let transition = |id: &str, to: TaskStatus, reason: &str| Command::TaskTransition {
            task_id: task(id),
            to,
            reason: reason.to_string(),
        };

        handled(
            &orchestrator,
            transition("FRK-1", TaskStatus::InProgress, "Key rotated."),
        )
        .await;
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(body.to.to_string(), "in_progress");
        assert_eq!(body.blocker_resolution.as_deref(), Some("Key rotated."));
        assert_eq!(body.reason.as_deref(), Some("Key rotated."));

        handled(
            &orchestrator,
            transition("FRK-2", TaskStatus::Cancelled, "Not needed."),
        )
        .await;
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Cancelled);

        let escalated = refused(
            &orchestrator,
            transition("FRK-3", TaskStatus::Refining, "Again."),
        )
        .await;
        assert!(
            escalated.starts_with("use_escalation_resolve"),
            "{escalated}"
        );

        let governor = refused(
            &orchestrator,
            transition("FRK-4", TaskStatus::Accepted, "Looks done."),
        )
        .await;
        assert!(governor.starts_with("transition_refused: "), "{governor}");
        assert!(governor.contains("no such transition"), "{governor}");
        assert!(last(&harness, EventKind::TransitionRefused).is_some());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn locks_triages_and_integrates_through_one_door() {
        let harness = Harness::new("human-door", |_| {});
        harness.file("FRK-1", "draft", |_| {});
        harness.accepted("FRK-2");
        let orchestrator = an_orchestrator(&harness);

        let locked = handled(
            &orchestrator,
            Command::ContractLock {
                task_id: task("FRK-1"),
            },
        )
        .await;
        assert_eq!(
            locked.events,
            vec![
                last(&harness, EventKind::ContractLocked)
                    .expect("the lock")
                    .envelope
                    .seq
            ]
        );
        handled(
            &orchestrator,
            Command::ContractUnlock {
                task_id: task("FRK-1"),
            },
        )
        .await;
        assert!(last(&harness, EventKind::ContractUnlocked).is_some());

        handled(
            &orchestrator,
            Command::RequestTriage {
                task_id: task("FRK-1"),
                size: RequestSize::Large,
                reason: "Three deliverables.".to_string(),
            },
        )
        .await;
        let triaged = last(&harness, EventKind::RequestTriaged).expect("the triage");
        let EventBody::RequestTriaged(body) = &triaged.body else {
            panic!("a triage");
        };
        assert_eq!(body.triaged_by, "human");
        for command in [
            Command::ContractLock {
                task_id: task("FRK-9"),
            },
            Command::ContractUnlock {
                task_id: task("FRK-9"),
            },
            Command::RequestTriage {
                task_id: task("FRK-9"),
                size: RequestSize::Small,
                reason: "One file.".to_string(),
            },
        ] {
            let answer = orchestrator.handle(command.clone()).await;
            assert!(
                matches!(&answer, Err(CommandError::NotFound { what }) if what == "task FRK-9"),
                "{command:?}: {answer:?}"
            );
        }

        let integrated = handled(
            &orchestrator,
            Command::TaskIntegrate {
                task_id: task("FRK-2"),
            },
        )
        .await;
        assert!(integrated.said.contains("merged"), "{}", integrated.said);
        let event = last(&harness, EventKind::TaskIntegrated).expect("the integration");
        let EventBody::TaskIntegrated(body) = &event.body else {
            panic!("an integration");
        };
        assert_eq!(body.integrated_by.to_string(), "human");

        let contract = farik_core::contract::validate_contract(
            &farik_core::contract::fixtures::a_contract_wire(),
        )
        .expect("a contract");
        assert!(matches!(
            orchestrator
                .handle(Command::TaskCreate {
                    contract: Box::new(contract),
                })
                .await,
            Err(CommandError::Invalid { .. })
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_the_run_through_handle() {
        let harness = Harness::new("human-run-stop", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = handled(&orchestrator, Command::RunStop).await;
        assert!(report.events.is_empty());
        orchestrator
            .run_until_idle()
            .await
            .expect("a stopped run ends at once");
        assert!(adapter.started().is_empty(), "no tick ran");
    }

    /// Ticks `orchestrator` while `command` is handled from a spawned task once the adapter has
    /// started a session, and answers both; a session nobody stops waits for ever, so both are
    /// bounded at ten seconds.
    async fn handled_mid_session(
        orchestrator: &std::sync::Arc<Orchestrator>,
        adapter: &UsageThenWaitAdapter,
        command: Command,
    ) -> (
        Result<crate::orchestrator::TickReport, crate::orchestrator::OrchestratorError>,
        Result<CommandReport, CommandError>,
    ) {
        let handling = {
            let orchestrator = std::sync::Arc::clone(orchestrator);
            let started = adapter.started_count();
            tokio::spawn(async move {
                while started.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                orchestrator.handle(command).await
            })
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            let ticked = orchestrator.tick().await;
            let handled = handling.await.expect("the command's task ends");
            (ticked, handled)
        })
        .await
        .expect("the session was stopped")
    }

    fn a_pause(agent: &str, status: AgentStatus) -> Command {
        Command::AgentUpdate {
            agent_id: agent.to_string(),
            status,
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn aborts_the_session_of_an_agent_paused_mid_session() {
        let harness = Harness::new("human-pause-mid-session", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage::default()));
        let orchestrator = Arc::new(harness.orchestrator(adapter.clone()));

        let (ticked, handled) = handled_mid_session(
            &orchestrator,
            &adapter,
            a_pause("dev-a", AgentStatus::Paused),
        )
        .await;
        ticked.expect("the tick ends with its session");
        handled.expect("the pause is handled");

        assert_eq!(adapter.aborts(), 1);
        let updated = last(&harness, EventKind::AgentUpdated).expect("the update");
        let EventBody::AgentUpdated(body) = &updated.body else {
            panic!("an agent.updated");
        };
        assert_eq!(
            (
                body.agent_id.as_str(),
                body.status.to_string(),
                body.updated_by.as_str()
            ),
            ("dev-a", "paused".to_string(), "human")
        );
        let ended = last(&harness, EventKind::SessionEnded).expect("the end");
        assert!(matches!(
            &ended.body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Aborted
        ));
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("in_progress".to_string(), "blocked".to_string())
        );
        assert_eq!(
            body.blocker
                .as_ref()
                .map(|blocker| blocker.description.as_str()),
            Some("agent paused by the user")
        );
        assert!(harness.events(&[EventKind::TeamUpdated]).is_empty());
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        assert_eq!(
            team.agents
                .iter()
                .find(|agent| agent.id.as_str() == "dev-a")
                .map(|agent| agent.status),
            Some(AgentStatus::Paused)
        );
        assert!(matches!(
            orchestrator.tick().await.expect("the tick runs"),
            crate::orchestrator::TickReport::Idle { .. }
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn resumes_what_the_pause_blocked() {
        let harness = Harness::new("human-resume", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(2);
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.blocked("FRK-2", "dev-a", "dev-b");
        let orchestrator = an_orchestrator(&harness);
        handled(&orchestrator, a_pause("dev-a", AgentStatus::Paused)).await;
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Blocked);

        handled(&orchestrator, a_pause("dev-a", AgentStatus::Active)).await;
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(moved.envelope.ids.task_id, Some(task("FRK-1")));
        assert_eq!(
            (
                body.from.to_string(),
                body.to.to_string(),
                body.actor.to_string()
            ),
            (
                "blocked".to_string(),
                "in_progress".to_string(),
                "human".to_string()
            )
        );
        assert_eq!(
            body.blocker_resolution.as_deref(),
            Some("agent resumed by the user")
        );
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Blocked);

        let same = refused(&orchestrator, a_pause("dev-a", AgentStatus::Active)).await;
        assert!(same.starts_with("same_status"), "{same}");
        assert!(matches!(
            orchestrator
                .handle(a_pause("nobody", AgentStatus::Paused))
                .await,
            Err(CommandError::NotFound { .. })
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_a_running_session_and_escalates_its_task() {
        let harness = Harness::new("human-session-stop", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage::default()));
        let orchestrator = Arc::new(harness.orchestrator(adapter.clone()));

        let (ticked, handled) = handled_mid_session(
            &orchestrator,
            &adapter,
            Command::SessionStop {
                session_id: "session-1".to_string(),
            },
        )
        .await;
        ticked.expect("the tick ends with its session");
        handled.expect("the stop is handled");

        assert_eq!(adapter.started()[0].session_id, "session-1");
        assert_eq!(adapter.aborts(), 1);
        let ended = last(&harness, EventKind::SessionEnded).expect("the end");
        assert!(matches!(
            &ended.body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Aborted
        ));
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        let EventBody::EscalationRaised(body) = &escalation.body else {
            panic!("an escalation");
        };
        assert_eq!(body.reason, EscalationRaisedBodyReason::ExplicitRequest);
        assert!(
            body.detail.ends_with("stopped by the human"),
            "{}",
            body.detail
        );
        assert!(matches!(
            orchestrator
                .handle(Command::SessionStop {
                    session_id: "session-9".to_string(),
                })
                .await,
            Err(CommandError::NotFound { .. })
        ));
    }
}
