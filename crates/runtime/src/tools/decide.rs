//! The tools the team decides with while autopilot is on (`docs/SPEC.md` 5.2 and 5.7, ADR 0082):
//! `catervas_answer_question`, the Architect's or the Product Manager's answer to a question an
//! agent put to the team, which resumes the task as the owner's answer does. They are offered to a
//! `decide` session alone.

use std::num::NonZeroU64;

use catervas_core::autopilot::{
    DecideChoice, DecideMove, QuestionAbout, Spent, answerer, choices_for, move_of,
};
use catervas_core::contract::{Role, TaskId, TaskKind, TaskStatus};
use catervas_core::governor::transition_table::TransitionActor;
use catervas_protocol::event::{EventBody, EventKind};
use catervas_protocol::generated::event::{
    EscalationResolvedBody, HumanAcceptedBody, HumanAcceptedBodySubject, TaskRoomGrantedBody,
    TeamQuestionAskedBodyAbout,
};
use catervas_store::EventQuery;
use catervas_store::autopilot::{flags, folder_change_spent, team_decides, team_spent};
use catervas_store::folder_docs::folder_docs;
use catervas_store::waiting::awaits_acceptance;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::work::ask_governor;
use super::{Call, ToolError, failed};
use crate::choices::{Waits, waits_of};
use crate::goals::planner;
use crate::transitions::{
    TransitionAsk, mechanical_criteria_passed, result_accepted, result_awaits_human, status_wire,
};

/// `catervas_answer_question`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnswerQuestionInput {
    /// The question's id, the `question_seq` the first message names.
    question_seq: u64,
    /// The answer, in 1 to 4000 characters: decide, and say why.
    answer: String,
}

/// `catervas_decide`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecideInput {
    /// The task whose wait you decide, or leave it out and name `folder_change`.
    #[serde(default)]
    task_id: Option<String>,
    /// The folder change whose failed integration you decide, or leave it out and name `task_id`.
    #[serde(default)]
    folder_change: Option<u64>,
    /// One of the choices the first message offers, by its name.
    choice: String,
    /// Why, in one or two sentences (1 to 600 characters).
    reason: String,
    /// For `answer_block`: the answer the task reads, in 1 to 4000 characters.
    #[serde(default)]
    answer: Option<String>,
    /// For `flag`: what only the owner can give, in 1 to 300 characters.
    #[serde(default)]
    needs: Option<String>,
}

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::Autopilot {
        code,
        detail: detail.into(),
    }
    .into()
}

/// Answers a question put to the team (ADR 0082 decision 2) with `team.question_answered`, about the
/// question's task, so the task is no longer waiting for it and its agent's next session is told
/// the answer. Only the role the question is put to answers it, once, and not a question the owner
/// answered.
pub(super) fn answer_question(
    call: &Call<'_>,
    input: &AnswerQuestionInput,
) -> Result<Value, ToolError> {
    let log = &call.deps().log;
    let events = log
        .read(&EventQuery {
            kinds: vec![
                EventKind::TeamQuestionAsked,
                EventKind::TeamQuestionAnswered,
                EventKind::QuestionAnswered,
            ],
            ..EventQuery::default()
        })
        .map_err(failed)?;
    let seq = input.question_seq;
    let Some((question, body)) = events.iter().find_map(|event| match &event.body {
        EventBody::TeamQuestionAsked(body) if event.envelope.seq == seq => Some((event, body)),
        _ => None,
    }) else {
        return Err(refused(
            "not_a_team_question",
            format!("{seq} is not a question an agent put to the team"),
        ));
    };
    if events.iter().any(|event| match &event.body {
        EventBody::TeamQuestionAnswered(answer) => answer.question_seq.get() == seq,
        EventBody::QuestionAnswered(answer) => answer.question_id.get() == seq,
        _ => false,
    }) {
        return Err(refused(
            "already_answered",
            format!("question {seq} has its answer"),
        ));
    }
    let from = call
        .team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == body.asked_by)
        .map_or(Role::SoftwareDeveloper, |agent| Role::from(agent.role));
    let about = match body.about {
        TeamQuestionAskedBodyAbout::Product => QuestionAbout::Product,
        TeamQuestionAskedBodyAbout::Technical => QuestionAbout::Technical,
    };
    let role = answerer(about, from, call.team.has_active(Role::Architect));
    if call.role() != role {
        return Err(refused(
            "not_yours",
            format!("question {seq} is put to the {}", role_name(role)),
        ));
    }
    let answered = serde_json::from_value(json!({
        "question_seq": seq,
        "answer": &input.answer,
        "answered_by": call.agent_id(),
    }))
    .map_err(|_| ToolError::InvalidInput {
        detail: "the answer is 1 to 4000 characters".to_string(),
    })?;
    call.append(
        question.envelope.ids.task_id.as_ref(),
        EventBody::TeamQuestionAnswered(answered),
    )?;
    Ok(json!({ "answered": seq }))
}

/// Moves `task` to `status` as the Product Manager, as the owner's resolution does (ADR 0082
/// decision 3): the room granted first when the choice gives room, then the governor's move, then
/// the resolution. `ask` and the resolution's message are the decision's words.
fn move_the_task(
    call: &Call<'_>,
    task: &TaskId,
    status: TaskStatus,
    (extra_tries, extra_room): (Option<u8>, bool),
    (ask, message): (TransitionAsk, String),
) -> Result<(), ToolError> {
    if extra_room {
        let contract = call.deps().files.read_contract(task).map_err(failed)?;
        call.append(
            Some(task),
            EventBody::TaskRoomGranted(TaskRoomGrantedBody {
                sessions: contract.budget.max_sessions,
                cost_usd: contract.budget.max_cost_usd,
                granted_by: call.agent_id().to_string(),
            }),
        )?;
    }
    ask_governor(
        call,
        task.clone(),
        status,
        TransitionActor::ProductManager,
        ask,
    )?;
    call.append(
        Some(task),
        EventBody::EscalationResolved(EscalationResolvedBody {
            to: status_wire(status).map_err(failed)?,
            message,
            resolved_by: call.agent_id().to_string(),
            extra_tries: extra_tries.and_then(|tries| NonZeroU64::new(u64::from(tries))),
        }),
    )?;
    Ok(())
}

/// Records the decision, `team.decided`, about `task`, and, for a flag, `task.flagged` after it.
fn record_decision(
    call: &Call<'_>,
    task: &TaskId,
    (choice, reason): (DecideChoice, &str),
    (answer, needs): (Option<String>, Option<String>),
) -> Result<(), ToolError> {
    let decided = serde_json::from_value(json!({
        "choice": choice.wire(),
        "reason": reason,
        "decided_by": call.agent_id(),
        "answer": answer,
        "needs": needs,
    }))
    .map_err(|_| ToolError::InvalidInput {
        detail: "the decision's words are not ones the log can hold".to_string(),
    })?;
    call.append(Some(task), EventBody::TeamDecided(decided))?;
    if let Some(needs) = needs {
        let flagged = serde_json::from_value(json!({
            "needs": needs,
            "flagged_by": call.agent_id(),
        }))
        .map_err(|_| ToolError::InvalidInput {
            detail: "what the owner must give is not words the log can hold".to_string(),
        })?;
        call.append(Some(task), EventBody::TaskFlagged(flagged))?;
    }
    Ok(())
}

/// The owner's approval of a plan, given by the Product Manager (ADR 0082 decision 11): the
/// contract accepted, then `escalated -> ready`, as `human.rs` does for the owner.
fn approve_plan(
    call: &Call<'_>,
    task: &TaskId,
    row: &catervas_store::TaskProjection,
    reason: &str,
) -> Result<(), ToolError> {
    if !row.awaiting_approval {
        return Err(refused(
            "not_awaiting_approval",
            format!(
                "{} is {} and no approval is asked of the human",
                task.as_str(),
                row.status
            ),
        ));
    }
    call.append(
        Some(task),
        EventBody::HumanAccepted(HumanAcceptedBody {
            subject: HumanAcceptedBodySubject::Contract,
            accepted_by: call.agent_id().to_string(),
            message: Some(reason.to_string()),
        }),
    )?;
    ask_governor(
        call,
        task.clone(),
        TaskStatus::Ready,
        TransitionActor::ProductManager,
        TransitionAsk::default(),
    )?;
    Ok(())
}

/// The owner's acceptance of a result, given by the Product Manager (ADR 0082 decision 11), after
/// the owner's own checks: one acceptance a verification, an epic's mechanical criteria passed,
/// and a result that waits on the human at all.
fn accept_result(
    call: &Call<'_>,
    task: &TaskId,
    row: &catervas_store::TaskProjection,
    reason: &str,
) -> Result<(), ToolError> {
    let deps = call.deps();
    let not_waiting = || {
        refused(
            "not_waiting_for_the_human",
            format!(
                "{} is {}, and its result does not wait for the human",
                task.as_str(),
                row.status
            ),
        )
    };
    if row.status != TaskStatus::Verifying {
        return Err(not_waiting());
    }
    let mut contract = deps.files.read_contract(task).map_err(failed)?;
    contract.status = row.status;
    let history = deps
        .log
        .read(&EventQuery {
            task_id: Some(task.clone()),
            ..EventQuery::default()
        })
        .map_err(failed)?;
    if result_accepted(&history).is_some() {
        return Err(refused(
            "already_accepted",
            format!(
                "the human accepted {}'s result in this verification",
                task.as_str()
            ),
        ));
    }
    if contract.kind == TaskKind::Epic {
        mechanical_criteria_passed(&contract, &history).map_err(|said| {
            let (code, detail) = said.split_once(": ").unwrap_or(("criterion_failed", &said));
            refused(
                if code == "criteria_not_run" {
                    "criteria_not_run"
                } else {
                    "criterion_failed"
                },
                detail,
            )
        })?;
    } else if !result_awaits_human(&contract) {
        return Err(not_waiting());
    }
    call.append(
        Some(task),
        EventBody::HumanAccepted(HumanAcceptedBody {
            subject: HumanAcceptedBodySubject::Result,
            accepted_by: call.agent_id().to_string(),
            message: Some(reason.to_string()),
        }),
    )?;
    Ok(())
}

/// A decision about a folder change whose integration failed (ADR 0082 decision 11): a retry the
/// next tick carries out, or a flag. It has no task, so `team.decided` names the change alone.
fn decide_folder_change(
    call: &Call<'_>,
    change: u64,
    input: &DecideInput,
    reason: &str,
) -> Result<Value, ToolError> {
    let deps = call.deps();
    let waiting = folder_docs(&deps.log)
        .map_err(failed)?
        .changes
        .iter()
        .any(|held| held.change == change && held.escalated && !held.integrated);
    let flagged = flags(&deps.log)
        .map_err(failed)?
        .iter()
        .any(|flag| flag.folder_change == Some(change));
    if !waiting || flagged {
        return Err(refused(
            "not_waiting_on_the_team",
            format!("folder change {change} waits on nothing the team decides"),
        ));
    }
    let offered = choices_for(
        "folder_change",
        None,
        None,
        folder_change_spent(&deps.log, change).map_err(failed)?,
    );
    let Some(choice) =
        DecideChoice::from_wire(&input.choice).filter(|choice| offered.contains(choice))
    else {
        return Err(refused(
            "choice_not_offered",
            format!(
                "{} is not offered here; offered: {}",
                input.choice,
                named(&offered)
            ),
        ));
    };
    let needs = if choice == DecideChoice::Flag {
        Some(written(
            input.needs.as_ref(),
            300,
            "needs_required",
            "say what the owner must give",
        )?)
    } else {
        None
    };
    let decided = serde_json::from_value(json!({
        "choice": choice.wire(),
        "reason": reason,
        "decided_by": call.agent_id(),
        "needs": needs,
        "folder_change": change,
    }))
    .map_err(|_| ToolError::InvalidInput {
        detail: "the decision's words are not ones the log can hold".to_string(),
    })?;
    call.append(None, EventBody::TeamDecided(decided))?;
    Ok(json!({ "decided": choice.wire(), "folder_change": change }))
}

/// A role's name for a refusal an agent reads.
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Architect => "Architect",
        _ => "Product Manager",
    }
}

/// Decides what waits on the owner, in the owner's place (ADR 0082 decision 3): the choices are the
/// owner's, offered within the team's limits, and `flag` leaves what only the owner can give on
/// Today. The move is asked of the governor as the Product Manager, whose `Autopilot` rows open
/// where the owner's would; a refusal records nothing and returns the gate's words. Each decision
/// ends with `team.decided`.
pub(super) fn decide(call: &Call<'_>, input: &DecideInput) -> Result<Value, ToolError> {
    if !team_decides(&call.deps().log, &call.team).map_err(failed)? {
        return Err(refused(
            "autopilot_off",
            "autopilot is off, so the owner decides what waits",
        ));
    }
    if planner(&call.team).map(|agent| agent.id.as_str()) != Some(call.agent_id()) {
        return Err(refused(
            "not_yours",
            "the Product Manager decides for the team, or the one agent when it works alone",
        ));
    }
    let reason = input.reason.trim();
    if reason.is_empty() || reason.chars().count() > 600 {
        return Err(refused(
            "reason_required",
            "say why in 1 to 600 characters: the log is where somebody reads it back",
        ));
    }
    match (&input.task_id, input.folder_change) {
        (Some(task), None) => decide_task(call, task, input, reason),
        (None, Some(change)) => decide_folder_change(call, change, input, reason),
        _ => Err(refused(
            "name_one",
            "name the task (task_id) or the folder change (folder_change) you decide, and one only",
        )),
    }
}

/// The wire names of `offered`, for a refusal that lists them.
fn named(offered: &[DecideChoice]) -> String {
    offered
        .iter()
        .map(|choice| choice.wire())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A text of 1 to `most` characters, or `code`'s refusal.
fn written(
    text: Option<&String>,
    most: usize,
    code: &'static str,
    what: &str,
) -> Result<String, ToolError> {
    match text.map(|text| text.trim()) {
        Some(text) if !text.is_empty() && text.chars().count() <= most => Ok(text.to_string()),
        _ => Err(refused(code, format!("{what} in 1 to {most} characters"))),
    }
}

fn decide_task(
    call: &Call<'_>,
    task: &str,
    input: &DecideInput,
    reason: &str,
) -> Result<Value, ToolError> {
    let deps = call.deps();
    let task: TaskId = task.parse().map_err(|_| ToolError::InvalidInput {
        detail: format!("task_id: {task} is not a task id"),
    })?;
    let row = call.row(&task)?;
    let standing = flags(&deps.log).map_err(failed)?;
    let wait: Option<Waits> = if standing
        .iter()
        .any(|flag| flag.task_id.as_ref() == Some(&task))
    {
        None
    } else {
        match waits_of(&deps.log, &deps.projections, &task).map_err(failed)? {
            Some(wait) => Some(wait),
            // A result the owner would accept (5.4) waits on the team as well.
            None if awaits_acceptance(&deps.log, &deps.files, &row).map_err(failed)? => {
                Some(Waits {
                    reason: "acceptance".to_string(),
                    before: None,
                    blocked_from: None,
                })
            }
            None => None,
        }
    };
    let Some(wait) = wait else {
        return Err(refused(
            "not_waiting_on_the_team",
            format!("{} waits on nothing the team decides", task.as_str()),
        ));
    };
    let spent: Spent = team_spent(&deps.log, &task).map_err(failed)?;
    let offered = choices_for(&wait.reason, wait.before, wait.blocked_from, spent);
    let choice = DecideChoice::from_wire(&input.choice).filter(|choice| offered.contains(choice));
    let Some(choice) = choice else {
        return Err(refused(
            "choice_not_offered",
            format!(
                "{} is not offered here; offered: {}",
                input.choice,
                named(&offered)
            ),
        ));
    };
    let answer = if choice == DecideChoice::AnswerBlock {
        Some(written(
            input.answer.as_ref(),
            4000,
            "answer_required",
            "answer the block",
        )?)
    } else {
        None
    };
    let needs = if choice == DecideChoice::Flag {
        Some(written(
            input.needs.as_ref(),
            300,
            "needs_required",
            "say what the owner must give",
        )?)
    } else {
        None
    };
    match move_of(choice, wait.before, wait.blocked_from) {
        Some(DecideMove::To {
            status,
            extra_tries,
            extra_room,
        }) => {
            let text = answer.clone().unwrap_or_else(|| reason.to_string());
            let ask = TransitionAsk {
                reason: Some(text.clone()),
                grants_tries: extra_tries.is_some(),
                blocker_resolution: (row.status == TaskStatus::Blocked).then(|| text.clone()),
                ..TransitionAsk::default()
            };
            move_the_task(call, &task, status, (extra_tries, extra_room), (ask, text))?;
        }
        Some(DecideMove::Reopen | DecideMove::Flag) => {}
        Some(DecideMove::ApprovePlan) => approve_plan(call, &task, &row, reason)?,
        Some(DecideMove::AcceptResult) => accept_result(call, &task, &row, reason)?,
        Some(DecideMove::RetryFolderChange) | None => {
            return Err(refused(
                "choice_not_offered",
                format!(
                    "{} is not offered here; offered: {}",
                    input.choice,
                    named(&offered)
                ),
            ));
        }
    }
    record_decision(call, &task, (choice, reason), (answer, needs))?;
    Ok(json!({ "decided": choice.wire(), "task_id": task.as_str() }))
}

#[cfg(test)]
mod tests {
    use catervas_protocol::command::Command;
    use serde_json::json;

    use crate::orchestrator::fixtures::Harness;
    use crate::tools::ToolError;
    use crate::tools::fixtures::at;

    async fn call(
        harness: &Harness,
        agent: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        crate::tools::call_tool(
            &harness.project.context(agent, Some("CTV-1")),
            "catervas_answer_question",
            input,
        )
        .await
    }

    fn refusal_of(result: Result<serde_json::Value, ToolError>, kind: &str) {
        match result {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with(&format!("{kind}: ")), "{reason}");
            }
            other => panic!("expected a {kind} refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn answers_only_an_open_question_put_to_it() {
        let harness = Harness::new("decide-answer", |wire| {
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(catervas_core::team::fixtures::an_agent_wire(
                    "ada",
                    "architect",
                ));
        });
        harness.in_progress("CTV-1", "dev-a", "dev-b");
        let asked = |question: &str| {
            harness
                .project
                .record_by(
                    Some("dev-a"),
                    at(),
                    "CTV-1",
                    "team.question_asked",
                    &json!({ "question": question, "asked_by": "dev-a", "about": "technical" }),
                )
                .envelope
                .seq
        };
        let (first, second) = (asked("Which database?"), asked("Which queue?"));
        let owners = harness
            .project
            .record(
                "CTV-1",
                "question.asked",
                &json!({ "question": "Should it be empty?", "asked_by": "dev-a" }),
            )
            .envelope
            .seq;

        // A technical question is the Architect's: the Developer who asked, and the Product
        // Manager beside the Architect, are not.
        let input =
            |seq: u64| json!({ "question_seq": seq, "answer": "SQLite: it is there already." });
        refusal_of(call(&harness, "dev-a", input(first)).await, "not_yours");
        refusal_of(call(&harness, "pm", input(first)).await, "not_yours");
        call(&harness, "ada", input(first))
            .await
            .expect("the Architect answers");
        refusal_of(
            call(&harness, "ada", input(first)).await,
            "already_answered",
        );
        refusal_of(
            call(&harness, "ada", input(owners)).await,
            "not_a_team_question",
        );

        // The owner may answer a team question too, once.
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let answer = |question_id| Command::QuestionAnswer {
            question_id,
            answer: "Postgres.".to_string(),
        };
        orchestrator
            .handle(answer(second))
            .await
            .expect("the owner answers the team's question");
        assert!(orchestrator.handle(answer(second)).await.is_err());
        refusal_of(
            call(&harness, "ada", input(second)).await,
            "already_answered",
        );
        assert!(orchestrator.handle(answer(first)).await.is_err());
    }
}
