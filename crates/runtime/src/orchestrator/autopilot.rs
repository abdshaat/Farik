//! The autopilot rule (`docs/SPEC.md` 5.2 and 5.7, ADR 0082): while the team decides in the owner's
//! place, one `decide` session a tick answers what the team asked itself. A wait is passed over
//! when its task is flagged, when its decider is busy or asleep, or when the day is spent, and a
//! wait that cost `DECIDE_SESSIONS_PER_WAIT` sessions and is still open is flagged by Catervas, so
//! a loop of sessions that decide nothing ends.

use catervas_core::autopilot::{
    BLOCK_WAIT_MINUTES, DECIDE_SESSIONS_PER_WAIT, SELLER_MESSAGES_PER_DAY, answerer, choices_for,
};
use catervas_core::contract::{Role, TaskStatus};
use catervas_core::team::Team;
use catervas_protocol::event::{
    EventBody, EventIds, EventKind, SellerMessagePurpose, SessionStartedBodyPurpose, new_event,
};
use catervas_protocol::generated::event::SellerMessageSentBodySentBy;
use catervas_store::autopilot::{
    TeamQuestion, flags, folder_change_spent, open_team_questions, team_decides, team_spent,
};
use catervas_store::folder_docs::{FolderChange, folder_docs};
use catervas_store::pipelines::{PipelineState, data_pipelines};
use catervas_store::seller_mail::{MessageState, seller_mail};
use catervas_store::waiting::awaits_acceptance;
use catervas_store::{EventQuery, TaskProjection};
use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};

use super::messages::{
    acceptance_message, decide_message, folder_change_message, pipeline_message, question_message,
};
use super::rules::{Waiting, acted, day_is_spent, unavailable};
use super::session::{SessionAsk, run_session};
use super::{OrchestratorDeps, OrchestratorError, TickReport, TickRules, TickScope};
use crate::ceremonies::open_escalations;
use crate::choices::waits_of;
use crate::goals::planner;
use crate::procurement::{Mailer, draft_text, send_message};
use crate::session::SessionPurpose;
use crate::transitions::{last_move_into, result_accepted};

/// The Catervas tools a session that answers a question is offered: the reading tools, a decision
/// to record, a post, the notebook, and the answer.
pub(super) const DECIDE_QUESTION_TOOLS: &[&str] = &[
    "catervas_read_task",
    "catervas_read_board",
    "catervas_read_rules",
    "catervas_read_criteria",
    "catervas_read_decisions",
    "catervas_write_decision",
    "catervas_post_message",
    "catervas_write_memory",
    "catervas_answer_question",
];

/// Sends the seller messages the Product Manager's team drafted since autopilot was turned on,
/// with no model (ADR 0082 decision 6): each message waiting, not an order's (its email waits with
/// its order), with no failed try (retrying on every tick would loop), oldest first, while fewer
/// than `SELLER_MESSAGES_PER_DAY` went out by autopilot today. A message beyond the day's waits on
/// Today with Send, as it does without autopilot, and goes the next day. Answers the numbers of
/// the sends recorded.
///
/// # Errors
///
/// What the log refused.
pub(super) async fn send_seller_messages(
    deps: &OrchestratorDeps,
) -> Result<Vec<u64>, OrchestratorError> {
    let tools = &deps.tools;
    let log = &tools.log;
    let switched = log.read(&EventQuery {
        kinds: vec![EventKind::AutopilotChanged],
        newest_first: true,
        limit: Some(1),
        ..EventQuery::default()
    })?;
    let Some(switched_on) = switched
        .first()
        .filter(|event| matches!(&event.body, EventBody::AutopilotChanged(body) if body.on))
        .map(|event| event.envelope.seq)
    else {
        return Ok(Vec::new());
    };
    let drafted_since: BTreeSet<u64> = log
        .read(&EventQuery {
            kinds: vec![EventKind::SellerMessageDrafted],
            after_seq: Some(switched_on),
            ..EventQuery::default()
        })?
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::SellerMessageDrafted(body) => Some(body.message.get()),
            _ => None,
        })
        .collect();
    let mail = seller_mail(log)?;
    let today = tools.clock.now().date_naive();
    let mut sent_today =
        mail.messages
            .iter()
            .filter(|record| {
                record.sent.as_ref().is_some_and(|sent| {
                    sent.sent_by == Some(SellerMessageSentBodySentBy::Autopilot)
                }) && record.sent_at.is_some_and(|at| at.date_naive() == today)
            })
            .count();
    let mut waiting: Vec<(u64, String)> = mail
        .messages
        .iter()
        .filter(|record| {
            record.state == MessageState::Waiting
                && record.drafted.purpose != SellerMessagePurpose::PurchaseOrder
                && record.failed_at.is_none()
                && drafted_since.contains(&record.message)
        })
        .map(|record| (record.message, record.drafted.subject.to_string()))
        .collect();
    waiting.sort_by_key(|(message, _)| *message);
    if waiting.is_empty() {
        return Ok(Vec::new());
    }
    let refused = |detail: String| OrchestratorError::Refused {
        reason: format!("seller_messages_unsent: {detail}"),
    };
    let secrets = deps.daemon.connector_secrets();
    let mailer = Mailer {
        secrets: &*secrets,
        at: deps
            .daemon
            .mailbox_at(tools.files.root())
            .map_err(|error| refused(error.to_string()))?,
        trust: deps.daemon.mail_trust(),
    };
    let mut sent = Vec::new();
    for (message, subject) in waiting {
        if sent_today >= SELLER_MESSAGES_PER_DAY {
            break;
        }
        let body = draft_text(tools, message);
        match send_message(
            tools,
            &mailer,
            message,
            (&subject, body.trim()),
            Some(SellerMessageSentBodySentBy::Autopilot),
        )
        .await
        {
            Ok((seq, _)) => {
                sent.push(seq);
                sent_today += 1;
            }
            // The day's limit, or no mailbox: nothing more goes now. Any other refusal leaves this
            // message waiting for the owner, and the next may still go.
            Err(refusal)
                if matches!(refusal.code, "seller_send_limit" | "mailbox_not_connected") =>
            {
                break;
            }
            Err(_) => {}
        }
    }
    Ok(sent)
}

/// The Catervas tools the session that decides a wait is offered: the same reading and writing
/// tools, and `catervas_decide` in place of the answer.
pub(super) const DECIDE_TOOLS: &[&str] = &[
    "catervas_read_task",
    "catervas_read_board",
    "catervas_read_rules",
    "catervas_read_criteria",
    "catervas_read_decisions",
    "catervas_write_decision",
    "catervas_post_message",
    "catervas_write_memory",
    "catervas_decide",
];

/// The Catervas tools the session that decides a data pipeline request is offered: the reading
/// tools, a post, the notebook, and the decision.
pub(super) const DECIDE_PIPELINE_TOOLS: &[&str] = &[
    "catervas_read_task",
    "catervas_read_board",
    "catervas_read_rules",
    "catervas_read_criteria",
    "catervas_read_decisions",
    "catervas_post_message",
    "catervas_write_memory",
    "catervas_decide_data_pipeline",
];

/// How many characters of a question the flag's words keep.
const NEEDS_QUESTION_CHARS: usize = 200;

/// The autopilot rule, after the budget and the channel and before the work (ADR 0082): about no one
/// task, so it runs only under `All` in a tick scoped to none, and only while the team decides.
pub(super) async fn autopilot_rule(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if scope.rules != TickRules::All
        || scope.task_id.is_some()
        || !team_decides(&deps.tools.log, team)?
    {
        return Ok(None);
    }
    if let Some(report) = answer_a_question(deps, team, board, waiting).await? {
        return Ok(Some(report));
    }
    if let Some(report) = decide_a_wait(deps, team, board, waiting).await? {
        return Ok(Some(report));
    }
    if let Some(report) = decide_a_folder_change(deps, team, waiting).await? {
        return Ok(Some(report));
    }
    decide_a_pipeline(deps, team, board, waiting).await
}

/// The oldest data pipeline request that waits on the owner gets a `decide` session of the Product
/// Manager about the task its request names (ADR 0082 decision 5). After
/// `DECIDE_SESSIONS_PER_WAIT` such sessions with no decision Catervas flags that task.
async fn decide_a_pipeline(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(decider) = planner(team) else {
        return Ok(None);
    };
    let log = &deps.tools.log;
    let flagged = flags(log)?;
    for record in data_pipelines(log)?
        .into_iter()
        .filter(|record| record.state == PipelineState::Escalated)
    {
        let Some(row) = board.iter().find(|row| row.task_id == record.task_id) else {
            continue;
        };
        if flagged
            .iter()
            .any(|flag| flag.task_id.as_ref() == Some(&row.task_id))
        {
            continue;
        }
        if decide_sessions_since(deps, row, record.pipeline)? >= DECIDE_SESSIONS_PER_WAIT {
            flag_by_catervas(
                deps,
                row,
                &format!(
                    "a decision on the data source {}",
                    record.requested.name.as_str()
                ),
            )?;
            return Ok(Some(flagged_report(row)));
        }
        if day_is_spent(deps, team, Role::from(decider.role), &mut waiting.day_spent)?
            | unavailable(deps, decider, &mut waiting.slept)?
        {
            continue;
        }
        let contract = deps.tools.files.read_contract(&row.task_id)?;
        let initial_prompt = pipeline_message(decider, &record);
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent: decider,
                contract: Some(&contract),
                purpose: SessionPurpose::Decide,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: true,
                only_tool: None,
                tools: Some(DECIDE_PIPELINE_TOOLS),
                in_reply_to: None,
                thread: None,
                initial_prompt,
            },
        )
        .await?;
        return Ok(Some(acted(row, decider, "decision", &end)));
    }
    Ok(None)
}

/// One thing that waits on the owner and that the Product Manager decides instead (decision 3): an
/// open escalation, or a block that has stood for an hour.
struct Wait {
    /// The task.
    row: TaskProjection,
    /// The reason, as the log words it, or `blocked`.
    reason: String,
    /// What the escalation says, or what the blocker says.
    detail: String,
    /// When the wait began, and the seq of the event that began it.
    began: (DateTime<Utc>, u64),
}

/// The waits on the owner that the team decides, oldest first.
fn waits(
    deps: &OrchestratorDeps,
    board: &[TaskProjection],
) -> Result<Vec<Wait>, OrchestratorError> {
    let log = &deps.tools.log;
    let now = deps.tools.clock.now();
    let mut waits = Vec::new();
    for open in open_escalations(log, &deps.tools.projections)? {
        let Some(row) = board.iter().find(|row| row.task_id == open.task_id) else {
            continue;
        };
        waits.push(Wait {
            row: row.clone(),
            reason: open.reason,
            detail: open.detail,
            began: (open.raised_at, open.raised_seq),
        });
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Blocked && !row.waiting_on_human)
    {
        let moves = log.read(&EventQuery {
            task_id: Some(row.task_id.clone()),
            kinds: vec![EventKind::TaskTransitioned],
            ..EventQuery::default()
        })?;
        let Some(blocking) = last_move_into(&moves, TaskStatus::Blocked) else {
            continue;
        };
        if now - blocking.envelope.recorded_at < Duration::minutes(BLOCK_WAIT_MINUTES) {
            continue;
        }
        let detail = match &blocking.body {
            EventBody::TaskTransitioned(body) => body.blocker.as_ref().map_or_else(
                || "it did not say why".to_string(),
                |blocker| format!("{} (needs: {})", blocker.description, blocker.needed),
            ),
            _ => String::new(),
        };
        waits.push(Wait {
            row: row.clone(),
            reason: catervas_store::waiting::BLOCKED.to_string(),
            detail,
            began: (blocking.envelope.recorded_at, blocking.envelope.seq),
        });
    }
    // A result that waits on the owner's acceptance, once its review passed, from its move into
    // `verifying`.
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Verifying && !row.waiting_on_human)
    {
        if !awaits_acceptance(log, &deps.tools.files, row)? {
            continue;
        }
        let history = log.read(&EventQuery {
            task_id: Some(row.task_id.clone()),
            ..EventQuery::default()
        })?;
        let Some(verifying) = last_move_into(&history, TaskStatus::Verifying) else {
            continue;
        };
        if result_accepted(&history).is_some() {
            continue;
        }
        waits.push(Wait {
            row: row.clone(),
            reason: ACCEPTANCE.to_string(),
            detail: String::new(),
            began: (verifying.envelope.recorded_at, verifying.envelope.seq),
        });
    }
    waits.sort_by_key(|wait| wait.began);
    Ok(waits)
}

/// The reason of a result that waits on the owner's acceptance, as the choices name it.
const ACCEPTANCE: &str = "acceptance";

/// Whether the team already decided `row`'s wait, which began at the event at `began`: its decision
/// is in the log, and whatever it set going is not this rule's.
fn decided_since(
    deps: &OrchestratorDeps,
    row: &TaskProjection,
    began: u64,
) -> Result<bool, OrchestratorError> {
    Ok(!deps
        .tools
        .log
        .read(&EventQuery {
            task_id: Some(row.task_id.clone()),
            kinds: vec![EventKind::TeamDecided],
            after_seq: Some(began),
            ..EventQuery::default()
        })?
        .is_empty())
}

/// The oldest wait the Product Manager can decide now gets its `decide` session (decision 3).
async fn decide_a_wait(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(decider) = planner(team) else {
        return Ok(None);
    };
    let log = &deps.tools.log;
    let flagged = flags(log)?;
    for wait in waits(deps, board)? {
        let row = &wait.row;
        if flagged
            .iter()
            .any(|flag| flag.task_id.as_ref() == Some(&row.task_id))
            || decided_since(deps, row, wait.began.1)?
        {
            continue;
        }
        if decide_sessions_since(deps, row, wait.began.1)? >= DECIDE_SESSIONS_PER_WAIT {
            flag_by_catervas(deps, row, "a decision the team could not make")?;
            return Ok(Some(flagged_report(row)));
        }
        if day_is_spent(deps, team, Role::from(decider.role), &mut waiting.day_spent)?
            | unavailable(deps, decider, &mut waiting.slept)?
        {
            continue;
        }
        let (before, blocked_from) = if wait.reason == ACCEPTANCE {
            (None, None)
        } else {
            let Some(waits) = waits_of(log, &deps.tools.projections, &row.task_id)? else {
                continue;
            };
            (waits.before, waits.blocked_from)
        };
        let offered = choices_for(
            &wait.reason,
            before,
            blocked_from,
            team_spent(log, &row.task_id)?,
        );
        let contract = deps.tools.files.read_contract(&row.task_id)?;
        let state = crate::cost::budget_state(
            &deps.tools.projections,
            team,
            contract.assignee_role,
            contract.assignee.as_deref(),
            Some(&contract),
            &catervas_core::budget::SessionLedger::default(),
            deps.tools.clock.now(),
        )?;
        let initial_prompt = if wait.reason == ACCEPTANCE {
            acceptance_message(decider, &contract, &results_of(deps, row)?, &offered)
        } else {
            decide_message(
                decider,
                &contract,
                row.status,
                &wait.reason,
                &wait.detail,
                (
                    state.task_sessions,
                    state.task_max_sessions,
                    state.task_spent_usd,
                ),
                &offered,
            )
        };
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent: decider,
                contract: Some(&contract),
                purpose: SessionPurpose::Decide,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: true,
                only_tool: None,
                tools: Some(DECIDE_TOOLS),
                in_reply_to: None,
                thread: None,
                initial_prompt,
            },
        )
        .await?;
        return Ok(Some(acted(row, decider, "decision", &end)));
    }
    Ok(None)
}

/// The team's oldest open question that can be answered now gets its answerer's `decide` session.
async fn answer_a_question(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let log = &deps.tools.log;
    let flagged = flags(log)?;
    for question in open_team_questions(log)? {
        let Some(task_id) = &question.task_id else {
            continue;
        };
        let Some(row) = board.iter().find(|row| &row.task_id == task_id) else {
            continue;
        };
        if flagged
            .iter()
            .any(|flag| flag.task_id.as_ref() == Some(task_id))
        {
            continue;
        }
        let asker = team
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == question.asked_by);
        let role = answerer(
            question.about,
            asker.map_or(Role::SoftwareDeveloper, |agent| Role::from(agent.role)),
            team.has_active(Role::Architect),
        );
        let Some(agent) = team
            .active_agents()
            .find(|agent| Role::from(agent.role) == role)
        else {
            continue;
        };
        if decide_sessions_since(deps, row, question.seq)? >= DECIDE_SESSIONS_PER_WAIT {
            flag_by_catervas(deps, row, &needs_of(&question))?;
            return Ok(Some(flagged_report(row)));
        }
        if day_is_spent(deps, team, role, &mut waiting.day_spent)?
            | unavailable(deps, agent, &mut waiting.slept)?
        {
            continue;
        }
        let contract = deps.tools.files.read_contract(&row.task_id)?;
        let asked_by = name_of_asker(team, &question);
        let initial_prompt = question_message(
            agent,
            &asked_by,
            question.about,
            &contract,
            question.seq,
            &question.question,
        );
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent,
                contract: Some(&contract),
                purpose: SessionPurpose::Decide,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: true,
                only_tool: None,
                tools: Some(DECIDE_QUESTION_TOOLS),
                in_reply_to: None,
                thread: None,
                initial_prompt,
            },
        )
        .await?;
        return Ok(Some(acted(row, agent, "decision", &end)));
    }
    Ok(None)
}

/// The oldest folder change whose integration failed gets a `decide` session about no task
/// (ADR 0082 decision 11): to try it again, or to flag it. After `DECIDE_SESSIONS_PER_WAIT` such
/// sessions since its escalation Catervas flags it.
async fn decide_a_folder_change(
    deps: &OrchestratorDeps,
    team: &Team,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(decider) = planner(team) else {
        return Ok(None);
    };
    let log = &deps.tools.log;
    let flagged = flags(log)?;
    let changes: Vec<FolderChange> = folder_docs(log)?
        .changes
        .into_iter()
        .filter(|change| change.escalated && !change.integrated)
        .collect();
    for change in changes {
        if flagged
            .iter()
            .any(|flag| flag.folder_change == Some(change.change))
        {
            continue;
        }
        let escalations = log.read(&EventQuery {
            kinds: vec![EventKind::FolderChangeEscalated],
            ..EventQuery::default()
        })?;
        let Some((began, detail)) = escalations
            .iter()
            .rev()
            .find_map(|event| match &event.body {
                EventBody::FolderChangeEscalated(body) if body.change.get() == change.change => {
                    Some((event.envelope.seq, body.detail.to_string()))
                }
                _ => None,
            })
        else {
            continue;
        };
        let sessions = log
            .read(&EventQuery {
                kinds: vec![EventKind::SessionStarted],
                after_seq: Some(began),
                ..EventQuery::default()
            })?
            .iter()
            .filter(|event| {
                event.envelope.ids.task_id.is_none()
                    && matches!(&event.body, EventBody::SessionStarted(body)
                        if body.purpose == SessionStartedBodyPurpose::Decide)
            })
            .count();
        if u32::try_from(sessions).unwrap_or(u32::MAX) >= DECIDE_SESSIONS_PER_WAIT {
            flag_folder_change_by_catervas(deps, change.change)?;
            return Ok(Some(TickReport::FolderChange {
                change: change.change,
                what: "flagged it for the owner: the team could not decide it".to_string(),
            }));
        }
        if day_is_spent(deps, team, Role::from(decider.role), &mut waiting.day_spent)?
            | unavailable(deps, decider, &mut waiting.slept)?
        {
            continue;
        }
        let offered = choices_for(
            "folder_change",
            None,
            None,
            folder_change_spent(log, change.change)?,
        );
        let initial_prompt = folder_change_message(decider, &change, &detail, &offered);
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent: decider,
                contract: None,
                purpose: SessionPurpose::Decide,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: true,
                only_tool: None,
                tools: Some(DECIDE_TOOLS),
                in_reply_to: None,
                thread: None,
                initial_prompt,
            },
        )
        .await?;
        return Ok(Some(TickReport::FolderChange {
            change: change.change,
            what: super::rules::ran(decider, "decision", &end),
        }));
    }
    Ok(None)
}

/// Flags a folder change for the owner in Catervas's own name: its decisions cost the team its
/// sessions and it is still escalated. A folder change has no task, so the flag is a decision.
fn flag_folder_change_by_catervas(
    deps: &OrchestratorDeps,
    change: u64,
) -> Result<(), OrchestratorError> {
    let refused = || OrchestratorError::Refused {
        reason: "flag_unrecorded: the flag's words are not ones the log can hold".to_string(),
    };
    let body = serde_json::from_value(serde_json::json!({
        "choice": "flag",
        "reason": "The team could not decide it in three sessions.",
        "decided_by": "catervas",
        "needs": "a decision the team could not make",
        "folder_change": change,
    }))
    .map_err(|_| refused())?;
    let event = new_event(
        EventBody::TeamDecided(body),
        deps.tools.clock.now(),
        deps.tools.ids.clone(),
    )
    .map_err(|_| refused())?;
    let appended = deps.tools.log.append(&event)?;
    deps.tools.projections.apply(&appended)?;
    Ok(())
}

/// The results recorded for `row`'s task since it moved into `verifying`, one line each, the
/// agents' words, for the session that decides whether to accept them.
fn results_of(deps: &OrchestratorDeps, row: &TaskProjection) -> Result<String, OrchestratorError> {
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(row.task_id.clone()),
        ..EventQuery::default()
    })?;
    let since =
        last_move_into(&history, TaskStatus::Verifying).map_or(0, |event| event.envelope.seq);
    let lines: Vec<String> = history
        .iter()
        .filter(|event| event.envelope.seq > since)
        .filter_map(|event| match &event.body {
            EventBody::CriterionRecorded(body) => Some(format!(
                "{}: {} ({})",
                body.criterion_id,
                if body.passed { "passed" } else { "failed" },
                body.evidence
            )),
            _ => None,
        })
        .collect();
    Ok(lines.join("\n"))
}

/// The name the answerer reads for the agent that asked.
fn name_of_asker(team: &Team, question: &TeamQuestion) -> String {
    catervas_store::waiting::name_of(team, &question.asked_by)
}

/// What the owner would have to give for a question nobody could answer.
fn needs_of(question: &TeamQuestion) -> String {
    let cut: String = question
        .question
        .chars()
        .take(NEEDS_QUESTION_CHARS)
        .collect();
    format!("an answer to: {cut}")
}

/// How many `decide` sessions about `row`'s task started after the event at `since`.
fn decide_sessions_since(
    deps: &OrchestratorDeps,
    row: &TaskProjection,
    since: u64,
) -> Result<u32, OrchestratorError> {
    let started = deps.tools.log.read(&EventQuery {
        task_id: Some(row.task_id.clone()),
        kinds: vec![EventKind::SessionStarted],
        after_seq: Some(since),
        ..EventQuery::default()
    })?;
    let count = started
        .iter()
        .filter(|event| {
            matches!(&event.body, EventBody::SessionStarted(body)
                if body.purpose == SessionStartedBodyPurpose::Decide)
        })
        .count();
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// What a tick says of a flag Catervas raised. It ends the tick, so that no rule works the task
/// the flag now holds.
fn flagged_report(row: &TaskProjection) -> TickReport {
    TickReport::Acted {
        task_id: row.task_id.clone(),
        what: "flagged it for the owner: the team could not decide it".to_string(),
    }
}

/// Flags `row`'s task for the owner in Catervas's own name (ADR 0082 decision 4): its wait cost the
/// team its sessions and is still open.
fn flag_by_catervas(
    deps: &OrchestratorDeps,
    row: &TaskProjection,
    needs: &str,
) -> Result<(), OrchestratorError> {
    let refused = || OrchestratorError::Refused {
        reason: "flag_unrecorded: the flag's words are not ones the log can hold".to_string(),
    };
    let body = serde_json::from_value(serde_json::json!({
        "needs": needs,
        "flagged_by": "catervas",
    }))
    .map_err(|_| refused())?;
    let ids = EventIds {
        task_id: Some(row.task_id.clone()),
        ..deps.tools.ids.clone()
    };
    let event = new_event(EventBody::TaskFlagged(body), deps.tools.clock.now(), ids)
        .map_err(|_| refused())?;
    let appended = deps.tools.log.append(&event)?;
    deps.tools.projections.apply(&appended)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use catervas_protocol::event::{EventBody, EventKind, GateWire, TransitionActorWire};
    use serde_json::json;

    use super::DECIDE_QUESTION_TOOLS;
    use crate::orchestrator::fixtures::Harness;
    use crate::recorded::fixtures::{ends_without_a_decision, implement_stops_early, tool_calls};
    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::at;

    /// The team of three, and an Architect `ada` beside it.
    fn with_an_architect(wire: &mut serde_json::Value) {
        wire["agents"]
            .as_array_mut()
            .expect("a list of agents")
            .push(catervas_core::team::fixtures::an_agent_wire(
                "ada",
                "architect",
            ));
    }

    fn autopilot(harness: &Harness, on: bool) {
        harness
            .project
            .record("", "autopilot.changed", &json!({ "on": on }));
    }

    /// CTV-1 in progress with the Developer's open technical question, which it returns the seq of.
    fn a_question_on_ctv_1(harness: &Harness) -> u64 {
        harness.in_progress("CTV-1", "dev-a", "dev-b");
        harness
            .project
            .record_by(
                Some("dev-a"),
                at(),
                "CTV-1",
                "team.question_asked",
                &json!({
                    "question": "Which database should the login store use?",
                    "asked_by": "dev-a",
                    "about": "technical"
                }),
            )
            .envelope
            .seq
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_architect_answers_a_technical_question_and_the_task_resumes() {
        let harness = Harness::new("autopilot-question", with_an_architect);
        let seq = a_question_on_ctv_1(&harness);
        autopilot(&harness, true);
        assert!(harness.row("CTV-1").waiting_on_human);
        let answer = tool_calls(&[(
            "catervas_answer_question",
            json!({ "question_seq": seq, "answer": "Use SQLite: it is already a dependency." }),
        )]);
        let adapter = harness.recorded(vec![answer, implement_stops_early()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(started[0].agent_id, "ada");
        assert_eq!(started[0].purpose, SessionPurpose::Decide);
        let offered: BTreeSet<&str> = started[0]
            .catervas_tools
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(
            offered,
            BTreeSet::from_iter(DECIDE_QUESTION_TOOLS.iter().copied())
        );
        let answered = harness.events(&[catervas_protocol::event::EventKind::TeamQuestionAnswered]);
        assert_eq!(answered.len(), 1);
        assert_eq!(
            answered[0]
                .envelope
                .ids
                .task_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("CTV-1")
        );
        assert!(!harness.row("CTV-1").waiting_on_human);

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started[1].agent_id, "dev-a");
        assert_eq!(started[1].purpose, SessionPurpose::Implement);
        let from_the_human = started[1]
            .system_prompt
            .split("## From the human")
            .nth(1)
            .expect("a From the human section");
        assert!(
            from_the_human.contains("Use SQLite: it is already a dependency."),
            "{from_the_human}"
        );
        assert!(
            from_the_human.contains("asked of the team"),
            "{from_the_human}"
        );

        // With autopilot off the question waits for the owner, and no decide session runs.
        let off = Harness::new("autopilot-question-off", with_an_architect);
        let seq = a_question_on_ctv_1(&off);
        let adapter = off.recorded(vec![implement_stops_early()]);
        off.orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.purpose != SessionPurpose::Decide)
        );
        let team = off.project.deps.files.read_team().expect("a team");
        let listed = catervas_store::waiting::waiting(
            &off.project.deps.projections,
            &off.project.deps.log,
            &off.project.deps.files,
            &team,
        )
        .expect("the list reads");
        assert!(
            listed.iter().any(|row| row.question_id == Some(seq)),
            "{listed:?}"
        );
    }

    /// The offered tools of the Product Manager's session that decides a wait.
    fn decide_tools() -> BTreeSet<&'static str> {
        BTreeSet::from_iter(super::DECIDE_TOOLS.iter().copied())
    }

    /// CTV-`n` blocked `minutes` before the clock's now, held by `dev-a`.
    fn blocked_minutes_ago(harness: &Harness, task: &str, minutes: i64) {
        harness.assigned(task, "dev-a", "dev-b");
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        harness
            .project
            .moved(task, "assigned", "in_progress", &people);
        let mut body = people;
        body["blocker"] = json!({ "description": "the API is down", "needed": "the API" });
        harness.project.moved_at(
            at() - chrono::Duration::minutes(minutes),
            task,
            "in_progress",
            "blocked",
            &body,
        );
    }

    async fn decided(
        harness: &Harness,
        task: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, crate::tools::ToolError> {
        crate::tools::call_tool(
            &harness.project.context("pm", Some(task)),
            "catervas_decide",
            input,
        )
        .await
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_product_manager_decides_an_escalation_with_the_owners_choices() {
        let harness = Harness::new("autopilot-escalation", |_| {});
        harness.escalated_hours_ago("CTV-1", "iterations", 0);
        autopilot(&harness, true);
        let adapter = harness.recorded(vec![tool_calls(&[(
            "catervas_decide",
            json!({ "task_id": "CTV-1", "choice": "more_tries",
                    "reason": "One more try is cheap; the failure was small." }),
        )])]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(started[0].agent_id, "pm");
        assert_eq!(started[0].purpose, SessionPurpose::Decide);
        let offered: BTreeSet<&str> = started[0]
            .catervas_tools
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(offered, decide_tools());
        for choice in ["more_tries", "back_to_plan", "cancel", "flag"] {
            assert!(
                started[0].initial_prompt.contains(choice),
                "{}",
                started[0].initial_prompt
            );
        }
        assert!(started[0].initial_prompt.contains("catervas_decide"));
        assert_eq!(
            harness.row("CTV-1").status,
            catervas_core::contract::TaskStatus::InProgress
        );
        let moves = harness.events(&[EventKind::TaskTransitioned]);
        let EventBody::TaskTransitioned(last) = &moves[moves.len() - 1].body else {
            panic!("a move");
        };
        assert_eq!(last.actor, TransitionActorWire::ProductManager);
        assert_eq!(last.gate, GateWire::Autopilot);
        let resolved = harness.events(&[EventKind::EscalationResolved]);
        let EventBody::EscalationResolved(body) = &resolved[0].body else {
            panic!("a resolution");
        };
        assert_eq!(body.resolved_by, "pm");
        assert_eq!(body.extra_tries.map(std::num::NonZeroU64::get), Some(2));
        let decisions = harness.events(&[EventKind::TeamDecided]);
        assert_eq!(decisions.len(), 1);

        // After three grants from the team only the choices that change the plan are offered.
        harness.escalated_hours_ago("CTV-2", "iterations", 0);
        for _ in 0..3 {
            harness.project.record_by(
                Some("pm"),
                at(),
                "CTV-2",
                "team.decided",
                &json!({ "choice": "more_tries", "reason": "Again.", "decided_by": "pm" }),
            );
        }
        match decided(
            &harness,
            "CTV-2",
            json!({ "task_id": "CTV-2", "choice": "more_tries", "reason": "Once more." }),
        )
        .await
        {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with("choice_not_offered: "), "{reason}");
                assert!(reason.contains("back_to_plan"), "{reason}");
            }
            other => panic!("expected choice_not_offered, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_block_waits_an_hour_and_a_flag_holds_it_for_the_owner() {
        let harness = Harness::new("autopilot-block", |_| {});
        autopilot(&harness, true);
        // Blocked 59 minutes, and exactly an hour.
        blocked_minutes_ago(&harness, "CTV-1", 59);
        let adapter = harness.recorded(Vec::new());
        harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());

        blocked_minutes_ago(&harness, "CTV-2", 60);
        let adapter = harness.recorded(vec![tool_calls(&[(
            "catervas_decide",
            json!({ "task_id": "CTV-2", "choice": "flag", "needs": "a Stripe key",
                    "reason": "Only the owner has the key." }),
        )])]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(adapter.started().len(), 1);
        let flagged = harness.events(&[EventKind::TaskFlagged]);
        assert_eq!(flagged.len(), 1);
        let EventBody::TaskFlagged(body) = &flagged[0].body else {
            panic!("a flag");
        };
        assert_eq!(
            (body.needs.as_str(), body.flagged_by.as_str()),
            ("a Stripe key", "pm")
        );

        // The flagged task gets no decide session, and rule 4 does not escalate it either.
        blocked_minutes_ago(&harness, "CTV-3", 30 * 60);
        harness.project.record(
            "CTV-3",
            "task.flagged",
            &json!({ "needs": "a sign-in", "flagged_by": "catervas" }),
        );
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(adapter.started().len(), 1, "{:?}", adapter.started());
        assert_eq!(
            harness.row("CTV-2").status,
            catervas_core::contract::TaskStatus::Blocked
        );
        assert_eq!(
            harness.row("CTV-3").status,
            catervas_core::contract::TaskStatus::Blocked
        );

        // Three decide sessions that decide nothing end with a flag from Catervas.
        let stuck = Harness::new("autopilot-block-stuck", |_| {});
        autopilot(&stuck, true);
        blocked_minutes_ago(&stuck, "CTV-1", 120);
        let adapter = stuck.recorded(vec![
            ends_without_a_decision(),
            ends_without_a_decision(),
            ends_without_a_decision(),
        ]);
        let orchestrator = stuck.orchestrator(adapter.clone());
        for _ in 0..4 {
            orchestrator.tick().await.expect("the tick runs");
        }
        assert_eq!(adapter.started().len(), 3);
        let flagged = stuck.events(&[EventKind::TaskFlagged]);
        let EventBody::TaskFlagged(body) = &flagged[0].body else {
            panic!("a flag");
        };
        assert_eq!(body.flagged_by, "catervas");
        assert_eq!(flagged.len(), 1);

        // An answer to a block needs its words.
        let answerless = Harness::new("autopilot-block-answer", |_| {});
        autopilot(&answerless, true);
        blocked_minutes_ago(&answerless, "CTV-1", 120);
        match decided(
            &answerless,
            "CTV-1",
            json!({ "task_id": "CTV-1", "choice": "answer_block", "reason": "It is clear." }),
        )
        .await
        {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with("answer_required: "), "{reason}");
            }
            other => panic!("expected answer_required, got {other:?}"),
        }
    }

    /// CTV-`n` `in_progress`, held by `dev-a`, then escalated for `reason` at the clock's now.
    fn escalated_from_work(harness: &Harness, task: &str, reason: &str) {
        harness.in_progress(task, "dev-a", "dev-b");
        harness
            .project
            .moved(task, "in_progress", "escalated", &json!({}));
        harness.project.record(
            task,
            "escalation.raised",
            &json!({ "reason": reason, "detail": format!("{task} waits") }),
        );
    }

    fn offered_in(error: ToolError) -> String {
        match error {
            ToolError::Refused { reason } => {
                assert!(reason.starts_with("choice_not_offered: "), "{reason}");
                reason
            }
            other => panic!("expected choice_not_offered, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(clippy::too_many_lines, reason = "one story, step by step")]
    async fn the_product_manager_approves_a_plan_and_accepts_a_result_for_the_owner() {
        use catervas_core::contract::TaskStatus;
        let harness = Harness::new("autopilot-approve", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(4);
        });
        autopilot(&harness, true);
        // An epic escalated for the owner's approval.
        harness.project.filed("CTV-1", "refining", "epic", None);
        harness
            .project
            .moved("CTV-1", "refining", "escalated", &json!({}));
        harness.project.record(
            "CTV-1",
            "escalation.raised",
            &json!({ "reason": "approval", "detail": "the plan waits" }),
        );
        let adapter = harness.recorded(vec![
            tool_calls(&[(
                "catervas_decide",
                json!({ "task_id": "CTV-1", "choice": "approve_plan",
                        "reason": "The plan matches the product plan." }),
            )]),
            tool_calls(&[(
                "catervas_decide",
                json!({ "task_id": "CTV-2", "choice": "accept_result",
                        "reason": "I read the work: it does what the contract says." }),
            )]),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let prompt = adapter.started()[0].initial_prompt.clone();
        for choice in ["approve_plan", "back_to_plan", "cancel", "flag"] {
            assert!(prompt.contains(choice), "{prompt}");
        }
        let accepted = harness.events(&[EventKind::HumanAccepted]);
        let EventBody::HumanAccepted(body) = &accepted[0].body else {
            panic!("an acceptance");
        };
        assert_eq!(body.accepted_by, "pm");
        assert_eq!(body.subject.to_string(), "contract");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Ready);
        let moves = harness.events(&[EventKind::TaskTransitioned]);
        let EventBody::TaskTransitioned(last) = &moves[moves.len() - 1].body else {
            panic!("a move");
        };
        assert_eq!(last.gate, GateWire::Autopilot);

        // A task with a `human` criterion, whose review passed, waits for the owner's acceptance.
        harness.verifying_with("CTV-2", true, true, |wire| {
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
        harness.project.record(
            "CTV-2",
            "review.recorded",
            &json!({ "reviewer": "dev-b", "criteria_run": 1, "passed": true }),
        );
        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        let accepting = started
            .iter()
            .find(|spec| spec.initial_prompt.contains("CTV-2"));
        let accepting = accepting.expect("a decide session about CTV-2");
        assert!(accepting.initial_prompt.contains("accept_result"));
        let accepted = harness.events(&[EventKind::HumanAccepted]);
        let EventBody::HumanAccepted(body) = &accepted[1].body else {
            panic!("an acceptance");
        };
        assert_eq!(
            (body.subject.to_string(), body.accepted_by.as_str()),
            ("result".to_string(), "pm")
        );
        // The Product Manager's own acceptance of the task now passes the Definition of Done,
        // beside Catervas's run of C1 and the reviewer's note.
        harness.project.record(
            "CTV-2",
            "criterion.recorded",
            &json!({ "criterion_id": "C1", "passed": true, "evidence": "at abc: exit 0",
                     "run_by": "reviewer", "recorded_by": "governor" }),
        );
        harness.project.record_by(
            Some("dev-b"),
            at(),
            "CTV-2",
            "note.written",
            &json!({ "kind": "review", "written_by": "dev-b",
                     "text": "The login page does what its contract says, and the founder's question is theirs.\n\nC1: test -f done.txt passed." }),
        );
        let team = harness.project.deps.files.read_team().expect("a team");
        let outcome = harness
            .project
            .deps
            .transitions
            .request(
                &catervas_core::governor::transition::TransitionRequest {
                    task_id: "CTV-2".parse().expect("a task id"),
                    to: TaskStatus::Accepted,
                    actor:
                        catervas_core::governor::transition_table::TransitionActor::ProductManager,
                    agent_id: Some("pm".to_string()),
                },
                &crate::transitions::TransitionAsk::default(),
                &team,
            )
            .expect("the governor answers");
        assert!(
            matches!(outcome, crate::transitions::TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        // A second acceptance is refused.
        match decided(
            &harness,
            "CTV-2",
            json!({ "task_id": "CTV-2", "choice": "accept_result", "reason": "Again." }),
        )
        .await
        {
            Err(ToolError::Refused { reason }) => {
                assert!(
                    reason.starts_with("not_waiting_on_the_team: ")
                        || reason.starts_with("already_accepted: "),
                    "{reason}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(clippy::too_many_lines, reason = "one story, step by step")]
    async fn the_product_manager_carries_on_and_retries_a_folder_change() {
        use catervas_core::contract::TaskStatus;
        const CADENCE: &str = "docs/catervas/delivery/cadence.md";
        let harness = Harness::new("autopilot-carry-on", |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(4);
        });
        autopilot(&harness, true);
        escalated_from_work(&harness, "CTV-1", "permission");
        let adapter = harness.recorded(vec![
            tool_calls(&[(
                "catervas_decide",
                json!({ "task_id": "CTV-1", "choice": "carry_on",
                        "reason": "The permission is not needed for this." }),
            )]),
            tool_calls(&[(
                "catervas_decide",
                json!({ "folder_change": 1, "choice": "retry_folder_change",
                        "reason": "The conflict is gone." }),
            )]),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the tick runs");
        let prompt = adapter.started()[0].initial_prompt.clone();
        assert!(
            prompt.find("carry_on").expect("offered")
                < prompt.find("back_to_plan").expect("offered"),
            "{prompt}"
        );
        assert_eq!(harness.row("CTV-1").status, TaskStatus::InProgress);

        // The preview's Settings choice is flagged, not offered.
        escalated_from_work(&harness, "CTV-2", "preview");
        let refusal = offered_in(
            decided(
                &harness,
                "CTV-2",
                json!({ "task_id": "CTV-2", "choice": "back_to_plan", "reason": "x" }),
            )
            .await
            .expect_err("back_to_plan is not offered"),
        );
        assert!(
            refusal.ends_with("offered: carry_on, cancel, flag"),
            "{refusal}"
        );
        // Neither both nor none of the two names.
        for input in [
            json!({ "task_id": "CTV-2", "folder_change": 1, "choice": "flag", "reason": "x" }),
            json!({ "choice": "flag", "reason": "x" }),
        ] {
            match decided(&harness, "CTV-2", input).await {
                Err(ToolError::Refused { reason }) => assert!(reason.starts_with("name_one: ")),
                other => panic!("expected name_one, got {other:?}"),
            }
        }

        // A folder change whose integration failed.
        let origin = harness.with_origin();
        let change = harness.folder_change(CADENCE, "a\n");
        harness.project.record(
            "",
            "folder_change.escalated",
            &json!({ "change": change, "detail": "the branch conflicts with main" }),
        );
        harness
            .project
            .moved("CTV-2", "escalated", "cancelled", &json!({}));
        for _ in 0..4 {
            if adapter.started().len() == 2 {
                break;
            }
            orchestrator.tick().await.expect("the tick runs");
        }
        let started = adapter.started();
        let session = started.last().expect("a session");
        assert_eq!(session.purpose, SessionPurpose::Decide);
        assert_eq!(session.task_id, None);
        assert!(
            session
                .initial_prompt
                .contains("the branch conflicts with main")
        );
        let decisions = harness.events(&[EventKind::TeamDecided]);
        let EventBody::TeamDecided(body) = &decisions[decisions.len() - 1].body else {
            panic!("a decision");
        };
        assert_eq!(body.folder_change.as_ref().map(|n| n.get()), Some(change));
        harness.gh.answers("list", "[]", "", 0);
        harness
            .gh
            .answers("create", "https://github.com/o/r/pull/7\n", "", 0);
        let report = orchestrator.tick().await.expect("the tick runs");
        assert!(
            matches!(&report, crate::orchestrator::TickReport::FolderChange { what, .. }
                if what.starts_with("opened the pull request")),
            "{report:?}"
        );

        // After three retries only a flag is offered, and a folder change's flag has no task.
        for _ in 0..2 {
            harness.project.record_by(
                Some("pm"),
                at(),
                "",
                "team.decided",
                &json!({ "choice": "retry_folder_change", "reason": "Again.", "decided_by": "pm",
                         "folder_change": change }),
            );
        }
        harness.project.record(
            "",
            "folder_change.escalated",
            &json!({ "change": change, "detail": "closed without merging" }),
        );
        let refusal = offered_in(
            decided(
                &harness,
                "CTV-1",
                json!({ "folder_change": change, "choice": "retry_folder_change", "reason": "x" }),
            )
            .await
            .expect_err("a fourth retry is not offered"),
        );
        assert!(refusal.ends_with("offered: flag"), "{refusal}");
        decided(
            &harness,
            "CTV-1",
            json!({ "folder_change": change, "choice": "flag", "needs": "a look at the branch",
                    "reason": "It keeps failing." }),
        )
        .await
        .expect("the flag is recorded");
        assert!(harness.events(&[EventKind::TaskFlagged]).is_empty());
        let flagged = catervas_store::autopilot::flags(&harness.project.deps.log).expect("reads");
        assert_eq!(flagged.len(), 1);
        assert_eq!(
            (flagged[0].task_id.is_none(), flagged[0].folder_change),
            (true, Some(change))
        );
        let _ = std::fs::remove_dir_all(&origin);
    }

    /// The Procurement Specialist's request for Firecrawl on CTV-1, which costs money, and Catervas
    /// passing it to the owner: the request's number.
    fn a_paid_pipeline_request(harness: &Harness) -> u64 {
        let seq = harness
            .project
            .record_by(
                Some("proc"),
                at(),
                "CTV-1",
                "data_pipeline.requested",
                &json!({
                    "name": "Firecrawl",
                    "what": "Prices as clean text from the seller pages the task has to compare.",
                    "source_url": "https://www.firecrawl.dev/pricing",
                    "why": "Three sellers hide their prices behind scripts the plain fetch cannot read.",
                    "cost": "paid",
                    "needs_account": true,
                    "sends_project_data": false
                }),
            )
            .envelope
            .seq;
        harness.project.record(
            "",
            "data_pipeline.escalated",
            &json!({ "pipeline": seq, "reason": "It costs money, so you decide (ADR 0061)." }),
        );
        seq
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(clippy::too_many_lines, reason = "one story, step by step")]
    async fn the_product_manager_decides_a_paid_data_pipeline() {
        use catervas_protocol::command::Command;

        let waiting_pipelines = |harness: &Harness| {
            let team = harness.project.deps.files.read_team().expect("a team");
            catervas_store::waiting::waiting(
                &harness.project.deps.projections,
                &harness.project.deps.log,
                &harness.project.deps.files,
                &team,
            )
            .expect("the list reads")
            .into_iter()
            .filter(|row| row.pipeline.is_some())
            .count()
        };
        // Off, no session runs and the owner is asked.
        let off = Harness::with_procurement("autopilot-pipeline-off");
        off.procurement_task("CTV-1", Some("in_progress"));
        a_paid_pipeline_request(&off);
        let adapter = off.recorded(vec![ends_without_a_decision()]);
        off.orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.purpose != SessionPurpose::Decide)
        );
        assert_eq!(waiting_pipelines(&off), 1);

        let harness = Harness::with_procurement("autopilot-pipeline");
        harness.procurement_task("CTV-1", Some("in_progress"));
        autopilot(&harness, true);
        let pipeline = a_paid_pipeline_request(&harness);
        assert_eq!(
            waiting_pipelines(&harness),
            0,
            "the team decides, not the owner"
        );
        let adapter = harness.recorded(vec![tool_calls(&[(
            "catervas_decide_data_pipeline",
            json!({ "pipeline": pipeline, "approve": true,
                    "reason": "It saves three sessions of reading, for a few cents." }),
        )])]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(
            (started[0].agent_id.as_str(), started[0].purpose),
            ("pm", SessionPurpose::Decide)
        );
        let offered: BTreeSet<&str> = started[0]
            .catervas_tools
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(
            offered,
            BTreeSet::from_iter(super::DECIDE_PIPELINE_TOOLS.iter().copied())
        );
        assert!(started[0].initial_prompt.contains("Firecrawl"));
        assert!(
            started[0]
                .initial_prompt
                .contains("catervas_decide_data_pipeline")
        );
        let approved = harness.events(&[EventKind::DataPipelineApproved]);
        assert_eq!(approved.len(), 1);
        let EventBody::DataPipelineApproved(body) = &approved[0].body else {
            panic!("an approval");
        };
        assert_eq!(body.by.to_string(), "product_manager");
        assert_eq!(approved[0].envelope.ids.agent_id.as_deref(), Some("pm"));
        assert_eq!(
            approved[0].envelope.ids.session_id.as_deref(),
            Some("session-1")
        );
        assert_eq!(
            harness.events(&[EventKind::TaskCreated]).len(),
            2,
            "the request is filed"
        );
        assert_eq!(waiting_pipelines(&harness), 0);
        // The owner's decision comes too late.
        let refusal = orchestrator
            .handle(Command::DataPipelineDecide {
                pipeline,
                approve: false,
                note: None,
            })
            .await
            .expect_err("decided already");
        assert!(
            format!("{refusal:?}").contains("pipeline_decided"),
            "{refusal:?}"
        );

        // Three sessions that decide nothing end with a flag on the requesting task.
        let stuck = Harness::with_procurement("autopilot-pipeline-stuck");
        stuck.procurement_task("CTV-1", Some("in_progress"));
        autopilot(&stuck, true);
        a_paid_pipeline_request(&stuck);
        let adapter = stuck.recorded(vec![
            ends_without_a_decision(),
            ends_without_a_decision(),
            ends_without_a_decision(),
        ]);
        let orchestrator = stuck.orchestrator(adapter.clone());
        for _ in 0..4 {
            orchestrator.tick().await.expect("the tick runs");
        }
        assert_eq!(adapter.started().len(), 3);
        let flagged = stuck.events(&[EventKind::TaskFlagged]);
        let EventBody::TaskFlagged(body) = &flagged[0].body else {
            panic!("a flag");
        };
        assert_eq!(
            body.needs.as_str(),
            "a decision on the data source Firecrawl"
        );
        assert_eq!(body.flagged_by, "catervas");
    }

    const SUBJECT: &str = "Quote for 500 printed pie boxes";
    const BODY: &str = "Hello,\n\nCould you quote 500 printed pie boxes, 9 inch?\n\nThank you.";

    /// `count` messages of the Procurement Specialist's recorded as sent by autopilot on `when`'s
    /// day, from number `first`, without files or a server.
    fn sent_by_autopilot(
        harness: &Harness,
        when: chrono::DateTime<chrono::Utc>,
        first: u64,
        count: u64,
    ) {
        for message in first..first + count {
            harness.project.record_in(
                Some("proc"),
                Some("session-1"),
                "CTV-1",
                "seller_message.drafted",
                &json!({
                    "message": message, "seller": "Pie Box Pros",
                    "to": crate::procurement::story::DANA.address,
                    "subject": SUBJECT, "purpose": "quote_request",
                    "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c"
                }),
            );
            harness.project.record_at(
                when,
                "",
                "seller_message.sent",
                &json!({
                    "message": message,
                    "message_id": format!("m{message}@bakery.test"),
                    "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c",
                    "edited": false,
                    "sent_by": "autopilot"
                }),
            );
        }
    }

    #[tokio::test]
    #[ignore = "needs Docker, the GreenMail image and the git program: cargo xtask check --integration"]
    #[allow(clippy::too_many_lines, reason = "one story, step by step")]
    async fn sends_drafted_seller_messages_under_autopilot_fifty_a_day() {
        use crate::procurement::story::Story;

        let story = Story::new("autopilot-send").await;
        let harness = &story.harness;
        let before = story.draft(SUBJECT, BODY);
        autopilot(harness, true);
        // Drafted after the switch: one to send, an order's email, and one whose try failed.
        let quote = story.draft(SUBJECT, BODY);
        harness.project.record_in(
            Some("proc"),
            Some("session-1"),
            "CTV-1",
            "purchase_order.drafted",
            &json!({
                "order": 12, "seller": "Pie Box Pros",
                "seller_contact": crate::procurement::story::DANA.address,
                "lines": [{ "item": "Pie box", "quantity": 500, "unit": "piece",
                    "unit_price": "0.40", "line_total": "200.00" }],
                "currency": "USD", "period": "once", "total": "200.00", "delivery": "",
                "terms": "", "url": "", "evaluation": "evaluations/boxes.md",
                "why": "It is the cheapest seller that ships to us."
            }),
        );
        let orders = harness.procurement_folder().join("orders");
        std::fs::create_dir_all(&orders).expect("the orders folder");
        std::fs::write(orders.join("PO-12.xlsx"), b"PK the workbook's bytes").expect("workbook");
        let order = story.draft_as(json!({
            "seller": "Pie Box Pros", "to": crate::procurement::story::DANA.address,
            "subject": "Order PO-12", "body": "Please find our order attached.",
            "purpose": "purchase_order", "purchase_order": 12
        }));
        let failing = story.draft(SUBJECT, BODY);
        harness.project.record(
            "CTV-1",
            "seller_message.failed",
            &json!({ "message": failing, "why": "the server refused it" }),
        );

        // A paused team sends nothing.
        harness
            .project
            .record("", "team.paused", &json!({ "by": "human" }));
        // The Procurement Specialist's task goes on between the sends: its sessions say nothing.
        let quiet = || harness.recorded((0..4).map(|_| ends_without_a_decision()).collect());
        let orchestrator = harness.orchestrator(quiet());
        orchestrator.tick().await.expect("the tick runs");
        assert!(story.events(&[EventKind::SellerMessageSent]).is_empty());
        harness
            .project
            .record("", "team.resumed", &json!({ "by": "human" }));

        orchestrator.tick().await.expect("the tick runs");

        let sent = story.events(&[EventKind::SellerMessageSent]);
        assert_eq!(sent.len(), 1, "{sent:?}");
        let EventBody::SellerMessageSent(body) = &sent[0].body else {
            panic!("a send");
        };
        assert_eq!(body.message.get(), quote);
        assert_eq!(
            body.sent_by.map(|by| by.to_string()),
            Some("autopilot".to_string())
        );
        assert_eq!(story.dana_has().len(), 1);
        for left in [before, order, failing] {
            let mail =
                catervas_store::seller_mail::seller_mail(&harness.project.deps.log).expect("folds");
            let record = mail
                .messages
                .iter()
                .find(|m| m.message == left)
                .expect("kept");
            assert_eq!(
                record.state,
                catervas_store::seller_mail::MessageState::Waiting,
                "{left}"
            );
        }

        // Fifty a day: with forty-eight more sent today the next goes, and the one after waits.
        let today = crate::tools::fixtures::at();
        sent_by_autopilot(harness, today, 200, 48);
        let fiftieth = story.draft(SUBJECT, BODY);
        let next = story.draft(SUBJECT, BODY);
        orchestrator.tick().await.expect("the tick runs");
        let sent_ids: Vec<u64> = story
            .events(&[EventKind::SellerMessageSent])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::SellerMessageSent(body) => Some(body.message.get()),
                _ => None,
            })
            .collect();
        assert!(sent_ids.contains(&fiftieth), "{sent_ids:?}");
        assert!(!sent_ids.contains(&next), "{sent_ids:?}");

        // On the next UTC day it goes.
        let tomorrow = harness.orchestrator_at(quiet(), today + chrono::Duration::days(1));
        tomorrow.tick().await.expect("the tick runs");
        assert!(story
            .events(&[EventKind::SellerMessageSent])
            .iter()
            .any(|event| matches!(&event.body, EventBody::SellerMessageSent(body) if body.message.get() == next)));
    }
}
