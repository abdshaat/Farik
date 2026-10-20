//! The tools the team decides with while autopilot is on (`docs/SPEC.md` 5.2 and 5.7, ADR 0082):
//! `catervas_answer_question`, the Architect's or the Product Manager's answer to a question an
//! agent put to the team, which resumes the task as the owner's answer does. They are offered to a
//! `decide` session alone.

use catervas_core::autopilot::{QuestionAbout, answerer};
use catervas_core::contract::Role;
use catervas_protocol::event::{EventBody, EventKind};
use catervas_protocol::generated::event::TeamQuestionAskedBodyAbout;
use catervas_store::EventQuery;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};

/// `catervas_answer_question`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnswerQuestionInput {
    /// The question's id, the `question_seq` the first message names.
    question_seq: u64,
    /// The answer, in 1 to 4000 characters: decide, and say why.
    answer: String,
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

/// A role's name for a refusal an agent reads.
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Architect => "Architect",
        _ => "Product Manager",
    }
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
