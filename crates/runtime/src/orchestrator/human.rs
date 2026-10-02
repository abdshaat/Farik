//! The human's door (`docs/SPEC.md` sections 5.2, 5.7, 5.11, 5.14, and 5.16): every command the
//! human gives the orchestrator, each judged and recorded through the store and the governor, so
//! that any process may handle one.

use std::num::NonZeroU64;

use farik_core::contract::{Role, TaskContract, TaskId, TaskKind, TaskStatus};
use farik_core::governor::gates::{Blocker, Rejection};
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::sprint::{Sprint, SprintStatus};
use farik_core::team::{Agent, AgentStatus, Team, custom_server, plain_role};
use farik_protocol::command::{AcceptSubject, Command, RequestSize};
use farik_protocol::event::{
    AgentUpdatedBody, ConnectorDisconnectedBody, EscalationRaisedBodyReason,
    EscalationResolvedBody, EventBody, EventIds, EventKind, HumanAcceptedBody,
    HumanAcceptedBodySubject, MessageKind, QuestionAnsweredBody, new_event,
};
use farik_store::requests::{RequestError, hold_contract, triage_by_human};
use farik_store::{EventQuery, TaskProjection};

use super::requests::HUMAN;
use super::verify::{governor_results, is_mechanical, since_verifying};
use super::{CommandError, CommandReport, IntegrationOutcome, Orchestrator, OrchestratorError};
use crate::channel::{ChannelError, NewMessage, mentions_in, post};
use crate::chat::{ChatError, NewChatMessage, post_chat};
use crate::daemon::DaemonState;
use crate::daemon::{secret_at, with_server};
use crate::pause::paused;
use crate::sprints::{EndedBy, SprintError, end_sprint, start_sprint};
use crate::tools::ToolDeps;
use crate::transitions::{
    TransitionAsk, TransitionOutcome, contract_accepted, refusal_details, result_accepted,
    result_awaits_human, review_passed, status_wire,
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
            extra_tries,
        } => resolve(tools, &task_id, to, &message, extra_tries),
        Command::HumanSendBack {
            task_id,
            subject: AcceptSubject::Contract,
            message,
            failed_criteria: _,
        } => send_plan_back(tools, &task_id, &message),
        Command::HumanSendBack {
            task_id,
            subject: AcceptSubject::Result,
            message,
            failed_criteria,
        } => send_result_back(tools, &task_id, &message, failed_criteria),
        Command::TaskTransition {
            task_id,
            to,
            reason,
        } => transition(tools, &task_id, to, &reason),
        Command::TaskIntegrate { task_id } => integrate(orchestrator, &task_id).await,
        Command::AgentUpdate { agent_id, status } => update_agent(orchestrator, &agent_id, status),
        Command::SessionStop { session_id } => stop_session(orchestrator, &session_id),
        Command::SprintStart { budget_usd } => sprint(
            tools,
            start_sprint(tools, budget_usd, HUMAN),
            EventKind::SprintStarted,
        ),
        Command::SprintEnd => sprint(
            tools,
            end_sprint(tools, EndedBy::Human),
            EventKind::SprintEnded,
        ),
        Command::TeamPause => pause(tools, true),
        Command::TeamResume => pause(tools, false),
        Command::MessagePost { text } => post_message(tools, text),
        Command::ChatMessagePost { agent_id, text } => post_chat_message(tools, &agent_id, text),
        Command::ConnectorConnect {
            agent,
            server,
            spec_sha256,
        } => connect_server(
            tools,
            &orchestrator.deps.daemon,
            &agent,
            server,
            &spec_sha256,
        ),
        Command::ConnectorDisconnect { agent, server } => {
            disconnect_server(tools, &orchestrator.deps.daemon, &agent, &server)
        }
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

/// The human's pause of the whole team, or its resume: recorded once, and refused when the team
/// is already so.
fn pause(tools: &ToolDeps, pausing: bool) -> Result<CommandReport, CommandError> {
    if paused(&tools.log).map_err(failed)? == pausing {
        return Err(CommandError::Refused {
            reason: if pausing {
                "already_paused: the team is already paused"
            } else {
                "not_paused: the team is not paused"
            }
            .to_string(),
        });
    }
    let by = serde_json::from_value(serde_json::json!({ "by": HUMAN })).map_err(failed)?;
    let (body, said) = if pausing {
        (EventBody::TeamPaused(by), "paused the team")
    } else {
        (EventBody::TeamResumed(by), "resumed the team")
    };
    Ok(CommandReport {
        said: said.to_string(),
        events: vec![append(tools, None, body)?],
    })
}

/// The human's message in the team's channel (5.9), its mentions found by Farik.
fn post_message(tools: &ToolDeps, text: String) -> Result<CommandReport, CommandError> {
    let team = tools.files.read_team().map_err(failed)?;
    let mentions = mentions_in(&text, &team, HUMAN);
    let seq = post(
        &tools.log,
        tools.clock.as_ref(),
        &tools.ids,
        NewMessage {
            author: HUMAN.to_string(),
            agent_id: None,
            kind: MessageKind::Human,
            text,
            mentions,
            task_id: None,
            thread: None,
            in_reply_to: None,
            session_id: None,
        },
    )
    .map_err(|error| match error {
        ChannelError::Refused { reason } => CommandError::Invalid { detail: reason },
        other => failed(other),
    })?;
    Ok(CommandReport {
        said: "posted in the channel".to_string(),
        events: vec![seq],
    })
}

/// The human's message in their one-to-one chat with `agent_id` (4.3), which is refused for an
/// agent not on the team or retired.
fn post_chat_message(
    tools: &ToolDeps,
    agent_id: &str,
    text: String,
) -> Result<CommandReport, CommandError> {
    let team = tools.files.read_team().map_err(failed)?;
    let Some(agent) = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == agent_id)
    else {
        return Err(CommandError::NotFound {
            what: format!("agent {agent_id}, who is not on the team"),
        });
    };
    if agent.status == AgentStatus::Retired {
        return Err(CommandError::Refused {
            reason: format!(
                "agent_retired: {} has retired, and a past teammate's chat is read-only",
                agent.display_name.as_str()
            ),
        });
    }
    let seq = post_chat(
        &tools.log,
        tools.clock.as_ref(),
        &tools.ids,
        NewChatMessage {
            chat: agent_id.to_string(),
            author: HUMAN.to_string(),
            text,
            in_reply_to: None,
            request: None,
            session_id: None,
        },
    )
    .map_err(|error| match error {
        ChatError::Refused { reason } => CommandError::Invalid { detail: reason },
        ChatError::Store(error) => failed(error),
    })?;
    Ok(CommandReport {
        said: format!("sent to {}", agent.display_name.as_str()),
        events: vec![seq],
    })
}

/// What starting or ending a sprint did, as the human's report: the sprint and the event of
/// `kind` it recorded last.
fn sprint(
    tools: &ToolDeps,
    done: Result<Sprint, SprintError>,
    kind: EventKind,
) -> Result<CommandReport, CommandError> {
    let sprint = done.map_err(|error| match error {
        SprintError::AlreadyOpen { .. } => CommandError::Refused {
            reason: format!("sprint_open: {error}"),
        },
        SprintError::NoneOpen => CommandError::Refused {
            reason: format!("no_sprint_open: {error}"),
        },
        SprintError::Refused { reason } => CommandError::Refused { reason },
        other => failed(other),
    })?;
    let recorded = tools
        .log
        .read(&EventQuery {
            kinds: vec![kind],
            ..EventQuery::default()
        })
        .map_err(failed)?;
    let budget = sprint
        .budget_usd
        .map_or_else(|| "no budget".to_string(), |usd| format!("budget ${usd}"));
    Ok(CommandReport {
        said: match sprint.status {
            SprintStatus::Open => format!("{} is open, {budget}", sprint.id.as_str()),
            SprintStatus::Ended => format!("{} ended", sprint.id.as_str()),
        },
        events: recorded
            .last()
            .map(|event| event.envelope.seq)
            .into_iter()
            .collect(),
    })
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
        return Err(not_awaiting_approval(&row));
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
/// or with a `human` criterion, which moves nothing, or any epic, whoever reviews it (ADR 0013),
/// once Farik has run and passed each of its mechanical criteria and with the human's words.
fn accept_result(
    tools: &ToolDeps,
    task_id: &TaskId,
    message: Option<String>,
) -> Result<CommandReport, CommandError> {
    let row = row_of(tools, task_id)?;
    let mut contract = tools.files.read_contract(task_id).map_err(failed)?;
    contract.status = row.status;
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
    if contract.kind == TaskKind::Epic {
        mechanical_criteria_passed(&contract, &history)?;
        if message.is_none() {
            return Err(CommandError::Invalid {
                detail: "an epic's acceptance carries the human's words: say what you checked"
                    .to_string(),
            });
        }
    } else if !result_awaits_human(&contract) {
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

fn not_awaiting_approval(row: &TaskProjection) -> CommandError {
    CommandError::Refused {
        reason: format!(
            "not_awaiting_approval: {} is {} and no approval is asked of the human",
            row.task_id.as_str(),
            row.status
        ),
    }
}

/// Sends a contract awaiting approval back to `refining` with the human's message (ADR 0024): the
/// escalation's own resolve.
fn send_plan_back(
    tools: &ToolDeps,
    task_id: &TaskId,
    message: &str,
) -> Result<CommandReport, CommandError> {
    written(message, "a send-back's message")?;
    let row = row_of(tools, task_id)?;
    if !row.awaiting_approval {
        return Err(not_awaiting_approval(&row));
    }
    resolve(tools, task_id, TaskStatus::Refining, message, None)
}

/// Sends a result that waits on the human back to its assignee (ADR 0024): `verifying ->
/// rejected` as the human, the message its reason, once the reviewer's review has passed; an epic
/// waits on no reviewer. The governor's return to work counts the try.
fn send_result_back(
    tools: &ToolDeps,
    task_id: &TaskId,
    message: &str,
    failed_criteria: Vec<String>,
) -> Result<CommandReport, CommandError> {
    written(message, "a send-back's message")?;
    let row = row_of(tools, task_id)?;
    let mut contract = tools.files.read_contract(task_id).map_err(failed)?;
    contract.status = row.status;
    if !result_awaits_human(&contract) {
        return Err(not_waiting(&row));
    }
    if contract.kind != TaskKind::Epic && !review_passed(&history_of(tools, task_id)?) {
        return Err(CommandError::Refused {
            reason: "review_first: the reviewer has not finished; send back once the review is in"
                .to_string(),
        });
    }
    let team = tools.files.read_team().map_err(failed)?;
    let events = human_moves(
        tools,
        &team,
        task_id,
        TaskStatus::Rejected,
        &TransitionAsk {
            reason: Some(message.to_string()),
            rejection: Some(Rejection {
                failed_criterion_ids: failed_criteria,
                reasons: message.to_string(),
            }),
            ..TransitionAsk::default()
        },
    )?;
    Ok(CommandReport {
        said: format!("{} is sent back to its assignee", task_id.as_str()),
        events,
    })
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
/// `escalation.resolved { to, message, extra_tries }`. Never to `ready` while the contract awaits
/// approval, which is `HumanAccept`'s, nor for an epic whose contract the human has not approved,
/// which reaches `ready` only by that approval (5.16 item 2). `extra_tries` is only for an
/// `iterations` escalation resolved to `in_progress`, and the move it makes counts the attempt it
/// starts (ADR 0024).
fn resolve(
    tools: &ToolDeps,
    task_id: &TaskId,
    to: TaskStatus,
    message: &str,
    extra_tries: Option<u8>,
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
    if let Some(tries) = extra_tries {
        if !(1..=5).contains(&tries) {
            return Err(CommandError::Invalid {
                detail: format!("extra_tries is {tries}, and it is 1 to 5"),
            });
        }
        if to != TaskStatus::InProgress {
            return Err(CommandError::Refused {
                reason: format!(
                    "extra_tries_only_for_tries: more tries resume the work, so they come with a \
                     move to in_progress, not to {to}"
                ),
            });
        }
        let reason = escalated_for(tools, task_id)?;
        if reason != Some(EscalationRaisedBodyReason::Iterations) {
            return Err(CommandError::Refused {
                reason: format!(
                    "extra_tries_only_for_tries: {} escalated for {}, and more tries are granted \
                     only to a task that used its tries",
                    task_id.as_str(),
                    reason.map_or_else(|| "no recorded reason".to_string(), |r| r.to_string())
                ),
            });
        }
    }
    let team = tools.files.read_team().map_err(failed)?;
    let mut events = human_moves(
        tools,
        &team,
        task_id,
        to,
        &TransitionAsk {
            reason: Some(message.to_string()),
            grants_tries: extra_tries.is_some(),
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
            extra_tries: extra_tries.and_then(|tries| NonZeroU64::new(u64::from(tries))),
        }),
    )?);
    Ok(CommandReport {
        said: format!("{} is resolved to {to}", task_id.as_str()),
        events,
    })
}

/// The reason of the task's last escalation.
fn escalated_for(
    tools: &ToolDeps,
    task_id: &TaskId,
) -> Result<Option<EscalationRaisedBodyReason>, CommandError> {
    Ok(history_of(tools, task_id)?
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            EventBody::EscalationRaised(body) => Some(body.reason),
            _ => None,
        }))
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
        Ok(IntegrationOutcome::Merged { sha, scan }) => {
            format!(
                "{} merged into the integration branch at {sha}{scan}",
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
        Ok(IntegrationOutcome::Escalated { detail, scan }) => format!(
            "{} could not be integrated: {detail}{}",
            task_id.as_str(),
            scan.map(|scan| scan.to_string()).unwrap_or_default()
        ),
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
    update_agent_with(
        &orchestrator.deps.tools,
        &orchestrator.deps.daemon,
        agent_id,
        status,
        None,
    )
}

/// `update_agent`, with `newcomer` added to the team in the same write when there is one: a
/// replacement, which never leaves the team without the role the agent held, even for a moment.
pub(crate) fn update_agent_with(
    tools: &ToolDeps,
    daemon: &DaemonState,
    agent_id: &str,
    status: AgentStatus,
    newcomer: Option<Agent>,
) -> Result<CommandReport, CommandError> {
    let _writing = daemon.team_writes();
    update_agent_held(tools, daemon, agent_id, status, newcomer)
}

/// `update_agent_with` for a caller that already holds `daemon.team_writes()`.
pub(crate) fn update_agent_held(
    tools: &ToolDeps,
    daemon: &DaemonState,
    agent_id: &str,
    status: AgentStatus,
    newcomer: Option<Agent>,
) -> Result<CommandReport, CommandError> {
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
    let (name, role) = (agent.display_name.to_string(), Role::from(agent.role));
    agent.status = status;
    team.agents.extend(newcomer);
    if let Some(reason) = leaves_a_gap(&team, &name, role, status) {
        return Err(CommandError::Refused { reason });
    }
    tools.files.write_team(&team).map_err(failed)?;
    Ok(CommandReport {
        said: format!("{agent_id} is {status}"),
        events: status_effects(tools, daemon, &team, agent_id, status)?,
    })
}

/// What follows from `agent_id`'s status becoming `status` in `team`, already written:
/// `agent.updated`; for a pause or a retirement, its sessions stopped and each `in_progress` task it
/// holds blocked, and for a retirement each `assigned` one too; for a resume, each task its pause blocked taken back to `in_progress`. Answers
/// the events' sequence numbers.
pub(crate) fn status_effects(
    tools: &ToolDeps,
    daemon: &DaemonState,
    team: &Team,
    agent_id: &str,
    status: AgentStatus,
) -> Result<Vec<u64>, CommandError> {
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
            // A paused agent still answers its chats (ADR 0026), so a chat answer under way runs
            // to its end; its other sessions stop.
            for (session_id, purpose) in daemon.sessions_of(agent_id) {
                if !crate::tools::may_work(status, purpose) {
                    daemon.request_stop(&session_id, words);
                }
            }
            if status == AgentStatus::Retired {
                forget_connector_keys(tools, daemon, team, agent_id);
            }
            // A retired agent never starts what it was assigned, so that is blocked for the human
            // too; a paused one starts it once resumed, so it waits.
            for row in held.filter(|row| {
                row.status == TaskStatus::InProgress
                    || (status == AgentStatus::Retired && row.status == TaskStatus::Assigned)
            }) {
                events.extend(moved_for(
                    tools,
                    team,
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
                    team,
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
    Ok(events)
}

/// Deletes the keys of each custom server an agent has in `before` and not in `after`: one taken
/// away by a save, or whose agent was removed from the team rather than retired, never runs again
/// either (ADR 0030; re-review N8).
pub(crate) fn forget_removed_keys(
    tools: &ToolDeps,
    daemon: &DaemonState,
    before: &Team,
    after: &Team,
) {
    let custom = |team: &Team| -> Vec<(String, String)> {
        team.agents
            .iter()
            .flat_map(|agent| {
                agent
                    .mcp_servers
                    .iter()
                    .flatten()
                    .filter_map(custom_server)
                    .map(|server| (agent.id.to_string(), server.name))
            })
            .collect()
    };
    let kept = custom(after);
    for (agent, server) in custom(before) {
        if !kept.contains(&(agent.clone(), server.clone()))
            && let Ok(at) = secret_at(daemon, tools, &agent, &server)
        {
            let _ = daemon.connector_secrets().delete(&at);
            daemon.forget_kept(&at);
        }
    }
}

/// Deletes the keys kept for each custom server `agent_id` has in `team`, which a retired agent
/// never uses again (ADR 0030). A store that fails to delete one leaves it, as a refused connect
/// does: it is sent to nothing, since the agent runs no session.
fn forget_connector_keys(tools: &ToolDeps, daemon: &DaemonState, team: &Team, agent_id: &str) {
    let servers = team
        .agents
        .iter()
        .filter(|agent| agent.id.as_str() == agent_id)
        .flat_map(|agent| agent.mcp_servers.iter().flatten())
        .filter_map(custom_server);
    for server in servers {
        if let Ok(at) = secret_at(daemon, tools, agent_id, &server.name) {
            let _ = daemon.connector_secrets().delete(&at);
            daemon.forget_kept(&at);
        }
    }
}

/// Why `name`, of `role`, may not be paused or retired, in `team` as it would be after: it was the
/// last active agent of a role the team cannot work without (D18), or of the role the team named
/// to check plans (spec 5.3). Section 10's foolproof configuration: the refusal says what to do.
fn leaves_a_gap(team: &Team, name: &str, role: Role, status: AgentStatus) -> Option<String> {
    if status == AgentStatus::Active || team.has_active(role) {
        return None;
    }
    let (plain, verb) = (
        plain_role(role),
        if status == AgentStatus::Paused {
            "stops"
        } else {
            "leaves"
        },
    );
    if matches!(role, Role::ProductManager | Role::SoftwareDeveloper) {
        Some(format!(
            "last_of_role: {name} is your only {plain}; add another before {name} {verb}."
        ))
    } else if team.judge() == role {
        Some(format!(
            "last_judge: {name} checks your plans; let Farik choose who checks, or add another \
             {plain}, before {name} {verb}."
        ))
    } else {
        None
    }
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

/// `connector_connect`: `server` given to `agent` in the team file, in place of the one of its
/// name, when the team validates with it and it hashes to `spec_sha256`, the hash kept beside its
/// keys; then `connector.connected`, and the store read once for `team.get` (ADR 0030).
fn connect_server(
    tools: &ToolDeps,
    daemon: &DaemonState,
    agent: &str,
    server: serde_json::Map<String, serde_json::Value>,
    spec_sha256: &str,
) -> Result<CommandReport, CommandError> {
    let entry = serde_json::Value::Object(server);
    let name = entry["name"].as_str().unwrap_or_default().to_string();
    let _writing = daemon.team_writes();
    let team = tools.files.read_team().map_err(failed)?;
    let after = connector_team(&team, agent, &name, Some(&entry))?;
    let custom = after
        .agents
        .iter()
        .filter(|held| held.id.as_str() == agent)
        .flat_map(|held| held.mcp_servers.iter().flatten())
        .find(|held| held.name.as_str() == name)
        .and_then(custom_server)
        .ok_or_else(|| CommandError::Refused {
            reason: format!("connector_not_custom: {name} is not a custom server"),
        })?;
    if farik_core::team::spec_sha256(&custom) != spec_sha256 {
        return Err(CommandError::Refused {
            reason: format!(
                "connector_not_confirmed: {name} was not kept on this machine as it is described \
                 here; connect it again"
            ),
        });
    }
    tools.files.write_team(&after).map_err(failed)?;
    let body = serde_json::from_value(serde_json::json!({
        "agent": agent,
        "server": name,
        "transport": entry["transport"],
        "credential_keys": entry.get("credential_keys").cloned().unwrap_or_else(|| serde_json::json!([])),
        "tools": entry.get("tools").cloned().unwrap_or_else(|| serde_json::json!({})),
        "spec_sha256": spec_sha256,
    }))
    .map_err(failed)?;
    let event = append(tools, None, EventBody::ConnectorConnected(body))?;
    if let Ok(at) = secret_at(daemon, tools, agent, &name) {
        daemon.read_kept(&at);
    }
    Ok(CommandReport {
        said: format!("{agent} has the connector {name}"),
        events: vec![event],
    })
}

/// `connector_disconnect`: the custom server `server` taken away from `agent` in the team file;
/// then `connector.disconnected`. Its keys are deleted by whoever sent it.
fn disconnect_server(
    tools: &ToolDeps,
    daemon: &DaemonState,
    agent: &str,
    server: &str,
) -> Result<CommandReport, CommandError> {
    let _writing = daemon.team_writes();
    let team = tools.files.read_team().map_err(failed)?;
    let custom = team
        .agents
        .iter()
        .filter(|held| held.id.as_str() == agent)
        .flat_map(|held| held.mcp_servers.iter().flatten())
        .any(|held| held.name.as_str() == server && custom_server(held).is_some());
    if !custom {
        return Err(CommandError::NotFound {
            what: format!("{agent}'s custom connector {server}"),
        });
    }
    let after = connector_team(&team, agent, server, None)?;
    tools.files.write_team(&after).map_err(failed)?;
    let event = append(
        tools,
        None,
        EventBody::ConnectorDisconnected(ConnectorDisconnectedBody {
            agent: agent.parse().map_err(failed)?,
            server: server.parse().map_err(failed)?,
        }),
    )?;
    if let Ok(at) = secret_at(daemon, tools, agent, server) {
        daemon.forget_kept(&at);
    }
    Ok(CommandReport {
        said: format!("{agent} no longer has the connector {server}"),
        events: vec![event],
    })
}

/// `team` with `agent`'s connector `name` set to `entry`, or removed, or the refusal naming each
/// rule it breaks.
fn connector_team(
    team: &Team,
    agent: &str,
    name: &str,
    entry: Option<&serde_json::Value>,
) -> Result<Team, CommandError> {
    with_server(team, agent, name, entry).map_err(|errors| CommandError::Refused {
        reason: errors
            .iter()
            .map(|error| format!("{}: {}", error.path, error.message))
            .collect::<Vec<_>>()
            .join("; "),
    })
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
        MessageKind, SessionEndedBodyReason,
    };
    use serde_json::{Value, json};

    use farik_core::sprint::fixtures::an_open_sprint_wire;
    use farik_core::sprint::{SprintStatus, validate_sprint};
    use farik_protocol::event::SprintEndedBodyEndedBy;

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
        // Farik's line in the channel says what the human did, after the move.
        let line = last(&harness, EventKind::MessagePosted).expect("a system line");
        assert_eq!(line.envelope.seq, moved.envelope.seq + 1);
        assert_eq!(report.events.last(), Some(&line.envelope.seq));
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
            extra_tries: None,
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
            extra_tries: None,
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
    async fn lets_a_paused_agent_finish_its_chat_answer() {
        // The founder's rule: a paused agent still answers its chats, so pausing it stops only
        // its other sessions; retiring it stops them all.
        let harness = Harness::new("human-pause-keeps-chat", |wire| {
            wire["agents"].as_array_mut().expect("agents").push(json!({
                "id": "dev-c", "display_name": "dev-c", "role": "software_developer",
                "status": "active",
            }));
        });
        let register = |session: &str, agent: &str, purpose: crate::session::SessionPurpose| {
            harness
                .daemon
                .register_session(crate::daemon::SessionRegistration {
                    session_id: session.to_string(),
                    agent_id: agent.to_string(),
                    task_id: None,
                    purpose,
                    in_reply_to: None,
                    thread: None,
                    cwd: harness.project.repo.path.clone(),
                    executor: None,
                    limits: farik_core::budget::DEFAULT_SESSION_LIMITS,
                    farik_tools: Vec::new(),
                    tiers: Vec::new(),
                    connectors: Vec::new(),
                    preview: None,
                });
        };
        register("chat-a", "dev-a", crate::session::SessionPurpose::Chat);
        register("work-a", "dev-a", crate::session::SessionPurpose::Implement);
        register("chat-c", "dev-c", crate::session::SessionPurpose::Chat);
        let orchestrator = an_orchestrator(&harness);

        handled(&orchestrator, a_pause("dev-a", AgentStatus::Paused)).await;
        handled(&orchestrator, a_pause("dev-c", AgentStatus::Retired)).await;

        assert_eq!(harness.daemon.stop_reason("chat-a"), None);
        assert_eq!(
            harness.daemon.stop_reason("work-a").as_deref(),
            Some(super::PAUSED)
        );
        assert_eq!(
            harness.daemon.stop_reason("chat-c").as_deref(),
            Some(super::RETIRED)
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn blocks_a_retired_agents_assigned_task_and_leaves_a_paused_ones() {
        // A task assigned to an agent that retires is never started: it is blocked for the human,
        // as a task under way is, so no task is left with nobody. A pause leaves it to wait.
        let harness = Harness::new("human-retire-assigned", |wire| {
            wire["agents"].as_array_mut().expect("agents").push(json!({
                "id": "dev-c", "display_name": "dev-c", "role": "software_developer",
                "status": "active",
            }));
        });
        harness.assigned("FRK-1", "dev-a", "dev-c");
        harness.assigned("FRK-2", "dev-b", "dev-c");
        let orchestrator = an_orchestrator(&harness);
        handled(&orchestrator, a_pause("dev-a", AgentStatus::Paused)).await;
        handled(&orchestrator, a_pause("dev-b", AgentStatus::Retired)).await;
        let status_of = |task: &str| {
            harness
                .project
                .deps
                .projections
                .board()
                .expect("the board reads")
                .into_iter()
                .find(|row| row.task_id.as_str() == task)
                .map(|row| row.status)
        };
        assert_eq!(status_of("FRK-1"), Some(TaskStatus::Assigned));
        assert_eq!(status_of("FRK-2"), Some(TaskStatus::Blocked));
        assert_eq!(
            super::last_blocker(&harness.project.deps, &task("FRK-2"))
                .expect("reads")
                .as_deref(),
            Some(super::RETIRED)
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn retiring_an_agent_deletes_its_connector_keys() {
        use crate::connectors::{ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets};

        let server = json!([{
            "name": "github", "source": "custom", "transport": "stdio",
            "command": "github-mcp", "credential_keys": ["API_KEY"],
            "tools": { "search": "network" }
        }]);
        let harness = Harness::new("human-retire-keys", |wire| {
            wire["agents"][1]["mcp_servers"] = server.clone();
            wire["agents"][2]["mcp_servers"] = server.clone();
        });
        let store = Arc::new(MemoryConnectorSecrets::default());
        assert!(harness.daemon.set_connector_secrets(store.clone()));
        let root = harness.project.deps.files.root();
        let at = |agent: &str| {
            harness
                .daemon
                .secret_at(root, agent, "github")
                .expect("an address")
        };
        let entry = ConnectorEntry {
            spec_sha256: "h".to_string(),
            keys: [(
                "API_KEY".to_string(),
                crate::claude::Secret::new("k".to_string()),
            )]
            .into(),
        };
        for agent in ["dev-a", "dev-b"] {
            store.save(&at(agent), &entry).expect("kept");
        }
        let orchestrator = an_orchestrator(&harness);
        handled(&orchestrator, a_pause("dev-b", AgentStatus::Retired)).await;
        // A retired agent never runs again: nothing it was given is kept for it (carry M4).
        assert_eq!(store.load(&at("dev-b")), Ok(None));
        assert_eq!(store.load(&at("dev-a")), Ok(Some(entry)));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn still_retires_one_agent_as_before() {
        let harness = Harness::new("human-retire-one", |_| {});
        harness.in_progress("FRK-1", "dev-b", "dev-a");
        let orchestrator = an_orchestrator(&harness);
        let before = harness.events(&[]).len();
        let report = handled(&orchestrator, a_pause("dev-b", AgentStatus::Retired)).await;
        let after: Vec<Value> = harness.events(&[])[before..]
            .iter()
            .map(farik_protocol::event::event_to_value)
            .collect();
        assert_eq!(
            report.events,
            after
                .iter()
                .map(|event| event["seq"].as_u64().expect("a seq"))
                .collect::<Vec<_>>()
        );
        assert_eq!(report.said, "dev-b is retired");
        assert_eq!(
            after
                .iter()
                .map(|event| (event["kind"].clone(), event["task_id"].clone()))
                .collect::<Vec<_>>(),
            [
                (json!("agent.updated"), Value::Null),
                (json!("task.transitioned"), json!("FRK-1")),
            ]
        );
        assert_eq!(
            after[0]["body"],
            json!({ "agent_id": "dev-b", "status": "retired", "updated_by": "human" })
        );
        assert_eq!(
            (&after[1]["body"]["to"], &after[1]["body"]["blocker"]),
            (
                &json!("blocked"),
                &json!({
                    "description": "agent retired by the user",
                    "needed": "the human reassigns the task"
                })
            )
        );
        let status = harness
            .project
            .deps
            .files
            .read_team()
            .expect("reads")
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "dev-b")
            .map(|agent| agent.status);
        assert_eq!(status, Some(AgentStatus::Retired));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_leaving_the_team_without_a_role_it_needs() {
        let harness = Harness::new("human-last-of-role", |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("agents")
                .push(json!({ "id": "ada", "display_name": "Ada", "role": "architect", "status": "active" }));
            wire["policy"]["judgment"] = json!({ "required": "always", "judge": "architect" });
        });
        let orchestrator = an_orchestrator(&harness);
        let before = harness.project.deps.files.read_team().expect("reads");
        assert_eq!(
            refused(&orchestrator, a_pause("pm", AgentStatus::Retired)).await,
            "last_of_role: pm is your only Product Manager; add another before pm leaves."
        );
        assert_eq!(
            refused(&orchestrator, a_pause("ada", AgentStatus::Paused)).await,
            "last_judge: Ada checks your plans; let Farik choose who checks, or add another \
             Architect, before Ada stops."
        );
        assert_eq!(
            harness.project.deps.files.read_team().expect("reads"),
            before
        );
        assert!(last(&harness, EventKind::AgentUpdated).is_none());
        // One of two Developers may go.
        handled(&orchestrator, a_pause("dev-b", AgentStatus::Retired)).await;
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
    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_a_sprint() {
        let harness = Harness::new("human-sprint-start", |_| {});
        let orchestrator = an_orchestrator(&harness);

        let report = handled(
            &orchestrator,
            Command::SprintStart {
                budget_usd: Some(20.0),
            },
        )
        .await;

        let sprint = harness
            .project
            .deps
            .files
            .read_sprint("S1")
            .expect("S1 is written");
        assert_eq!(sprint.status, SprintStatus::Open);
        assert_eq!(sprint.budget_usd, Some(20.0));
        assert!(sprint.ended_at.is_none() && sprint.task_ids.is_empty());
        let started = last(&harness, EventKind::SprintStarted).expect("the start is recorded");
        let EventBody::SprintStarted(body) = &started.body else {
            panic!("a start");
        };
        assert_eq!(
            (
                body.sprint_id.as_str(),
                body.budget_usd,
                body.started_by.as_str()
            ),
            ("S1", Some(20.0), "human")
        );
        assert_eq!(report.events, vec![started.envelope.seq]);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn numbers_a_sprint_after_the_files_and_the_log() {
        let harness = Harness::new("human-sprint-numbers", |_| {});
        let mut ended = an_open_sprint_wire();
        ended["id"] = json!("S3");
        ended["status"] = json!("ended");
        ended["ended_at"] = json!("2026-09-24T01:00:00Z");
        harness
            .project
            .deps
            .files
            .write_sprint(&validate_sprint(&ended).expect("a sprint"))
            .expect("S3 is written");
        let orchestrator = an_orchestrator(&harness);
        handled(&orchestrator, Command::SprintStart { budget_usd: None }).await;
        assert!(harness.project.deps.files.read_sprint("S4").is_ok());

        let harness = Harness::new("human-sprint-numbers-log", |_| {});
        harness.project.record(
            "",
            "sprint.started",
            &json!({ "sprint_id": "S5", "budget_usd": null, "started_by": "human" }),
        );
        harness.project.record(
            "",
            "sprint.ended",
            &json!({ "sprint_id": "S5", "ended_by": "human", "left": [] }),
        );
        let orchestrator = an_orchestrator(&harness);
        handled(&orchestrator, Command::SprintStart { budget_usd: None }).await;
        assert!(harness.project.deps.files.read_sprint("S6").is_ok());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_second_open_sprint() {
        let harness = Harness::new("human-sprint-second", |_| {});
        harness.open_sprint("S1", &[]);
        let before = harness.events(&[]).len();
        let orchestrator = an_orchestrator(&harness);

        let reason = refused(&orchestrator, Command::SprintStart { budget_usd: None }).await;

        assert!(reason.contains("S1"), "{reason}");
        assert_eq!(harness.events(&[]).len(), before);
        assert!(harness.project.deps.files.read_sprint("S2").is_err());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_a_pause_the_human_asks_for() {
        let harness = Harness::new("human-pause", |_| {});
        let orchestrator = an_orchestrator(&harness);

        let report = handled(&orchestrator, Command::TeamPause).await;

        let paused = last(&harness, EventKind::TeamPaused).expect("the pause is recorded");
        assert_eq!(report.events, vec![paused.envelope.seq]);
        assert_eq!(
            serde_json::to_value(&paused.body).expect("a body")["body"],
            json!({ "by": "human" })
        );
        let again = refused(&orchestrator, Command::TeamPause).await;
        assert_eq!(again, "already_paused: the team is already paused");
        assert_eq!(harness.events(&[EventKind::TeamPaused]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_resume_when_not_paused() {
        let harness = Harness::new("human-resume-refused", |_| {});
        let orchestrator = an_orchestrator(&harness);

        let reason = refused(&orchestrator, Command::TeamResume).await;

        assert_eq!(reason, "not_paused: the team is not paused");
        assert!(harness.events(&[EventKind::TeamResumed]).is_empty());

        handled(&orchestrator, Command::TeamPause).await;
        handled(&orchestrator, Command::TeamResume).await;
        assert_eq!(harness.events(&[EventKind::TeamResumed]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_the_human_commands_while_paused() {
        let harness = Harness::new("human-paused-commands", |_| {});
        harness.open_sprint("S1", &[]);
        let orchestrator = an_orchestrator(&harness);
        handled(&orchestrator, Command::TeamPause).await;

        handled(&orchestrator, Command::SprintEnd).await;

        let ended = last(&harness, EventKind::SprintEnded).expect("the end is recorded");
        let EventBody::SprintEnded(body) = &ended.body else {
            panic!("an end");
        };
        assert_eq!(body.ended_by, SprintEndedBodyEndedBy::Human);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_sprint_leaving_its_unfinished_tasks() {
        let harness = Harness::new("human-sprint-end", |_| {});
        harness.accepted("FRK-1");
        harness.in_progress("FRK-2", "dev-a", "dev-b");
        harness.open_sprint("S1", &["FRK-1", "FRK-2"]);
        let orchestrator = an_orchestrator(&harness);

        let report = handled(&orchestrator, Command::SprintEnd).await;

        let files = &harness.project.deps.files;
        let sprint = files.read_sprint("S1").expect("S1 reads");
        assert_eq!(sprint.status, SprintStatus::Ended);
        assert!(sprint.ended_at.is_some());
        let ended = last(&harness, EventKind::SprintEnded).expect("the end is recorded");
        let EventBody::SprintEnded(body) = &ended.body else {
            panic!("an end");
        };
        assert_eq!(body.ended_by, SprintEndedBodyEndedBy::Human);
        assert_eq!(
            body.left.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            vec!["FRK-2"]
        );
        // With the policy off, what the sprint leaves goes on: no Backlog mark (ADR 0028).
        assert_eq!(body.backlog, None);
        assert!(!harness.row("FRK-2").left_for_the_backlog);
        assert_eq!(report.events, vec![ended.envelope.seq]);
        let contract = files.read_contract(&task("FRK-2")).expect("FRK-2 reads");
        assert_eq!(contract.sprint, None);
        assert_eq!(harness.row("FRK-2").status, TaskStatus::InProgress);
        assert_eq!(harness.row("FRK-2").sprint, None);
        assert_eq!(harness.row("FRK-1").sprint.as_deref(), Some("S1"));

        let again = refused(&orchestrator, Command::SprintEnd).await;
        assert!(again.contains("no sprint is open"), "{again}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn ends_a_sprint_whose_file_omits_a_task_on_the_board() {
        let harness = Harness::new("human-sprint-end-omitted", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.open_sprint("S1", &["FRK-1"]);
        // S1's file restored from before FRK-1 was planned into it.
        let files = &harness.project.deps.files;
        let mut restored = an_open_sprint_wire();
        restored["id"] = json!("S1");
        files
            .write_sprint(&validate_sprint(&restored).expect("a sprint"))
            .expect("S1 is written");
        let orchestrator = an_orchestrator(&harness);

        handled(&orchestrator, Command::SprintEnd).await;

        let ended = last(&harness, EventKind::SprintEnded).expect("the end is recorded");
        let EventBody::SprintEnded(body) = &ended.body else {
            panic!("an end");
        };
        assert_eq!(
            body.left.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            vec!["FRK-1"]
        );
        let contract = files.read_contract(&task("FRK-1")).expect("FRK-1 reads");
        assert_eq!(contract.sprint, None);
        assert_eq!(harness.row("FRK-1").sprint, None);
    }

    /// What `handle` answers for the human's `text` in the channel.
    async fn said_in_the_channel(
        orchestrator: &Orchestrator,
        text: &str,
    ) -> Result<CommandReport, CommandError> {
        orchestrator
            .handle(Command::MessagePost {
                text: text.to_string(),
            })
            .await
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn posts_the_humans_message() {
        let harness = Harness::new("human-message", |_| {});
        let orchestrator = an_orchestrator(&harness);

        let report = said_in_the_channel(&orchestrator, "@dev-a how is FRK-1?")
            .await
            .expect("the message is posted");

        let posted = harness.events(&[EventKind::MessagePosted]);
        assert_eq!(posted.len(), 1);
        let EventBody::MessagePosted(body) = &posted[0].body else {
            panic!("a message");
        };
        assert_eq!(
            (body.author.as_str(), body.kind, body.text.as_str()),
            ("human", MessageKind::Human, "@dev-a how is FRK-1?")
        );
        assert_eq!(body.mentions, ["dev-a"]);
        assert_eq!(posted[0].envelope.ids.agent_id, None);
        assert_eq!(report.events, vec![posted[0].envelope.seq]);
        assert_eq!(report.said, "posted in the channel");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_an_empty_message() {
        let harness = Harness::new("human-message-empty", |_| {});
        let orchestrator = an_orchestrator(&harness);

        let said = said_in_the_channel(&orchestrator, "   ").await;

        assert!(
            matches!(said, Err(CommandError::Invalid { .. })),
            "{said:?}"
        );
        assert!(harness.events(&[EventKind::MessagePosted]).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_message_too_long() {
        let harness = Harness::new("human-message-long", |_| {});
        let orchestrator = an_orchestrator(&harness);

        let said = said_in_the_channel(&orchestrator, &"a".repeat(2_001)).await;

        assert!(
            matches!(said, Err(CommandError::Invalid { .. })),
            "{said:?}"
        );
        assert!(harness.events(&[EventKind::MessagePosted]).is_empty());
        // The limit counts characters, not bytes: 2,000 of a two-byte letter is a message.
        said_in_the_channel(&orchestrator, &"é".repeat(2_000))
            .await
            .expect("2,000 characters are posted");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_a_chat_message_out_of_bounds() {
        let harness = Harness::new("human-chat-bounds", |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("agents")
                .push(json!({ "id": "old", "display_name": "Old", "role": "software_developer", "status": "retired" }));
        });
        let orchestrator = an_orchestrator(&harness);
        let chat = |agent: &str, text: String| Command::ChatMessagePost {
            agent_id: agent.to_string(),
            text,
        };

        for (agent, text) in [
            ("dev-a", " \n\t ".to_string()),
            ("dev-a", "é".repeat(4_001)),
            ("nobody", "Hello?".to_string()),
            ("old", "Hello?".to_string()),
        ] {
            let said = orchestrator.handle(chat(agent, text)).await;
            let sentence = match &said {
                Err(
                    CommandError::Invalid { detail: sentence }
                    | CommandError::Refused { reason: sentence }
                    | CommandError::NotFound { what: sentence },
                ) => sentence.clone(),
                other => panic!("{agent}'s chat is refused, not {other:?}"),
            };
            assert!(sentence.trim().contains(' '), "a sentence: {sentence}");
        }
        assert!(harness.events(&[EventKind::ChatMessagePosted]).is_empty());

        // A proposed request out of its limits is refused too, and nothing is recorded.
        let deps = &harness.project.deps;
        for (title, text) in [
            (
                "t".repeat(121),
                "Add Apple Pay at checkout, beside the card.".to_string(),
            ),
            (
                "Two\nlines".to_string(),
                "Add Apple Pay at checkout, beside the card.".to_string(),
            ),
            ("Apple Pay".to_string(), "Too short, 19 chars".to_string()),
            ("Apple Pay".to_string(), "a".repeat(4_001)),
        ] {
            let proposed = crate::chat::post_chat(
                &deps.log,
                deps.clock.as_ref(),
                &deps.ids,
                crate::chat::NewChatMessage {
                    chat: "dev-a".to_string(),
                    author: "dev-a".to_string(),
                    text: "Here is a request.".to_string(),
                    in_reply_to: None,
                    request: Some(crate::chat::ProposedRequest { title, text }),
                    session_id: None,
                },
            );
            assert!(
                matches!(proposed, Err(crate::chat::ChatError::Refused { .. })),
                "{proposed:?}"
            );
        }
        assert!(harness.events(&[EventKind::ChatMessagePosted]).is_empty());

        // The limit counts code points, not bytes, and line breaks are kept.
        let text = format!("{}\n", "é".repeat(3_999));
        let report = orchestrator
            .handle(chat("dev-a", text.clone()))
            .await
            .expect("4,000 code points are sent");
        let posted = harness.events(&[EventKind::ChatMessagePosted]);
        assert_eq!(report.events, [posted[0].envelope.seq]);
        assert_eq!(posted[0].envelope.ids.agent_id.as_deref(), Some("dev-a"));
        let EventBody::ChatMessagePosted(body) = &posted[0].body else {
            panic!("a chat message");
        };
        assert_eq!(
            (body.chat.as_str(), body.author.as_str(), body.text.as_str()),
            ("dev-a", "human", text.as_str())
        );
    }

    fn send_back(id: &str, subject: AcceptSubject, criteria: &[&str]) -> Command {
        Command::HumanSendBack {
            task_id: task(id),
            subject,
            message: "The button is too small to tap.".to_string(),
            failed_criteria: criteria.iter().map(ToString::to_string).collect(),
        }
    }

    fn a_review(harness: &Harness, id: &str, passed: bool) {
        harness.project.record(
            id,
            "review.recorded",
            &json!({ "reviewer": "dev-b", "criteria_run": 1, "passed": passed }),
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_a_result_back_after_the_review() {
        let harness = Harness::new("human-send-back", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(4);
        });
        harness.verifying_with("FRK-1", true, true, |wire| wire["risk"] = json!("high"));
        harness.verifying("FRK-2");
        let orchestrator = an_orchestrator(&harness);
        let back = send_back("FRK-1", AcceptSubject::Result, &["C1"]);

        let early = refused(&orchestrator, back.clone()).await;
        assert_eq!(
            early,
            "review_first: the reviewer has not finished; send back once the review is in"
        );
        a_review(&harness, "FRK-1", false);
        let failed = refused(&orchestrator, back.clone()).await;
        assert!(failed.starts_with("review_first"), "{failed}");
        a_review(&harness, "FRK-1", true);
        // The human names only criteria the contract has, as the reviewer does.
        let unknown = refused(
            &orchestrator,
            send_back("FRK-1", AcceptSubject::Result, &["C99"]),
        )
        .await;
        assert!(
            unknown.contains("this contract has no criterion C99"),
            "{unknown}"
        );
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Verifying);

        let report = handled(&orchestrator, back).await;
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        assert!(report.events.contains(&moved.envelope.seq));
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (
                body.from.to_string(),
                body.to.to_string(),
                body.actor.to_string(),
                body.gate.to_string()
            ),
            (
                "verifying".to_string(),
                "rejected".to_string(),
                "human".to_string(),
                "human_rejection".to_string()
            )
        );
        let rejection = body.rejection.as_ref().expect("the human's reasons");
        assert_eq!(rejection.failed_criterion_ids, vec!["C1".to_string()]);
        assert_eq!(rejection.reasons, "The button is too small to tap.");
        assert_eq!(
            body.reason.as_deref(),
            Some("The button is too small to tap.")
        );
        // The rejection counts as a try: the governor's return to work counts it.
        orchestrator.tick().await.expect("the tick runs");
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::InProgress);
        assert_eq!(row.iteration, 1);

        let low = refused(
            &orchestrator,
            send_back("FRK-2", AcceptSubject::Result, &[]),
        )
        .await;
        assert!(low.starts_with("not_waiting_for_the_human"), "{low}");
        // An epic is the human's to review, so it goes back with no reviewer's review.
        an_epic(&harness, "FRK-3", "verifying", a_command_criterion());
        handled(
            &orchestrator,
            send_back("FRK-3", AcceptSubject::Result, &[]),
        )
        .await;
        assert_eq!(harness.row("FRK-3").status, TaskStatus::Rejected);
        assert!(matches!(
            orchestrator
                .handle(Command::HumanSendBack {
                    task_id: task("FRK-2"),
                    subject: AcceptSubject::Result,
                    message: " ".to_string(),
                    failed_criteria: Vec::new(),
                })
                .await,
            Err(CommandError::Invalid { .. })
        ));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_a_plan_back_to_refining() {
        let harness = Harness::new("human-send-plan-back", |_| {});
        an_epic(&harness, "FRK-1", "refining", a_command_criterion());
        escalated(&harness, "FRK-1", "approval");
        harness.ready("FRK-2");
        let orchestrator = an_orchestrator(&harness);

        handled(
            &orchestrator,
            send_back("FRK-1", AcceptSubject::Contract, &[]),
        )
        .await;
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Refining);
        let resolved = last(&harness, EventKind::EscalationResolved).expect("the resolution");
        let EventBody::EscalationResolved(body) = &resolved.body else {
            panic!("a resolution");
        };
        assert_eq!(body.to.to_string(), "refining");
        assert_eq!(body.message, "The button is too small to tap.");

        let not = refused(
            &orchestrator,
            send_back("FRK-2", AcceptSubject::Contract, &[]),
        )
        .await;
        assert!(not.starts_with("not_awaiting_approval"), "{not}");
    }

    /// FRK-1's sessions allowance as the budgets read it now.
    fn max_sessions(harness: &Harness) -> u32 {
        let deps = &harness.project.deps;
        let contract = deps
            .files
            .read_contract(&task("FRK-1"))
            .expect("the contract reads");
        crate::cost::budget_state(
            &deps.projections,
            &deps.files.read_team().expect("the team reads"),
            contract.assignee_role,
            Some(&contract),
            &farik_core::budget::SessionLedger::default(),
            deps.clock.now(),
        )
        .expect("the budgets read")
        .task_max_sessions
    }

    /// FRK-1, back in `in_progress` at `iteration`, verified and rejected by its reviewer.
    fn rejected_again(harness: &Harness, iteration: u32) {
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": iteration });
        harness
            .project
            .moved("FRK-1", "in_progress", "verifying", &people);
        let mut body = people;
        body["actor"] = json!("reviewer");
        body["requested_by"] = json!("dev-b");
        body["rejection"] = json!({ "failed_criterion_ids": ["C1"], "reasons": "still missing" });
        harness
            .project
            .moved("FRK-1", "verifying", "rejected", &body);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn grants_extra_tries() {
        let harness = Harness::new("human-extra-tries", |_| {});
        harness.rejected("FRK-1", 3, "C1: done.txt missing");
        let orchestrator = an_orchestrator(&harness);
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        let sessions = max_sessions(&harness);

        handled(
            &orchestrator,
            Command::EscalationResolve {
                task_id: task("FRK-1"),
                to: TaskStatus::InProgress,
                message: "Try again with the new API.".to_string(),
                extra_tries: Some(2),
            },
        )
        .await;
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::InProgress);
        // The resumed attempt is the first of the two, so it is counted.
        assert_eq!(row.iteration, 4);
        let resolved = last(&harness, EventKind::EscalationResolved).expect("the resolution");
        let EventBody::EscalationResolved(body) = &resolved.body else {
            panic!("a resolution");
        };
        assert_eq!(body.extra_tries.map(std::num::NonZero::get), Some(2));
        assert_eq!(max_sessions(&harness), sessions + 8);

        // The resumed attempt fails, and the second one runs.
        rejected_again(&harness, 4);
        orchestrator.tick().await.expect("the tick runs");
        let row = harness.row("FRK-1");
        assert_eq!((row.status, row.iteration), (TaskStatus::InProgress, 5));
        // The second fails too, and iterations escalates again.
        rejected_again(&harness, 5);
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        let reasons: Vec<EscalationRaisedBodyReason> = harness
            .events(&[EventKind::EscalationRaised])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason),
                _ => None,
            })
            .collect();
        assert_eq!(
            reasons,
            vec![
                EscalationRaisedBodyReason::Iterations,
                EscalationRaisedBodyReason::Iterations
            ]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_extra_tries_for_other_reasons() {
        let harness = Harness::new("human-extra-tries-refused", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        escalated(&harness, "FRK-1", "readiness_failures");
        let orchestrator = an_orchestrator(&harness);

        let reason = refused(
            &orchestrator,
            Command::EscalationResolve {
                task_id: task("FRK-1"),
                to: TaskStatus::InProgress,
                message: "Split it by page.".to_string(),
                extra_tries: Some(2),
            },
        )
        .await;
        assert!(reason.starts_with("extra_tries_only_for_tries"), "{reason}");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
        assert!(harness.events(&[EventKind::EscalationResolved]).is_empty());

        // More tries resume the work, so they come only with a move back to it.
        harness.rejected("FRK-2", 3, "C1: done.txt missing");
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Escalated);
        for to in [TaskStatus::Cancelled, TaskStatus::Refining] {
            let reason = refused(
                &orchestrator,
                Command::EscalationResolve {
                    task_id: task("FRK-2"),
                    to,
                    message: "Stop here.".to_string(),
                    extra_tries: Some(2),
                },
            )
            .await;
            assert!(reason.starts_with("extra_tries_only_for_tries"), "{reason}");
        }
        // The escalation that counts is the latest one.
        harness.project.record(
            "FRK-2",
            "escalation.raised",
            &json!({ "reason": "explicit_request", "detail": "the PM asks" }),
        );
        let reason = refused(
            &orchestrator,
            Command::EscalationResolve {
                task_id: task("FRK-2"),
                to: TaskStatus::InProgress,
                message: "Go on.".to_string(),
                extra_tries: Some(2),
            },
        )
        .await;
        assert!(
            reason.contains("escalated for explicit_request"),
            "{reason}"
        );
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Escalated);
        assert!(harness.events(&[EventKind::EscalationResolved]).is_empty());
        // The schema holds the number to 1 to 5.
        assert!(
            farik_protocol::command::command_from_value(&json!({
                "command": "escalation_resolve",
                "body": { "task_id": "FRK-1", "to": "in_progress", "message": "Go.", "extra_tries": 6 }
            }))
            .is_err()
        );
    }
}
