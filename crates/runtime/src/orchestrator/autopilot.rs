//! The autopilot rule (`docs/SPEC.md` 5.2 and 5.7, ADR 0082): while the team decides in the owner's
//! place, one `decide` session a tick answers what the team asked itself. A wait is passed over
//! when its task is flagged, when its decider is busy or asleep, or when the day is spent, and a
//! wait that cost `DECIDE_SESSIONS_PER_WAIT` sessions and is still open is flagged by Catervas, so
//! a loop of sessions that decide nothing ends.

use catervas_core::autopilot::{DECIDE_SESSIONS_PER_WAIT, answerer};
use catervas_core::contract::Role;
use catervas_core::team::Team;
use catervas_protocol::event::{
    EventBody, EventIds, EventKind, SessionStartedBodyPurpose, new_event,
};
use catervas_store::autopilot::{TeamQuestion, flags, open_team_questions, team_decides};
use catervas_store::{EventQuery, TaskProjection};

use super::messages::question_message;
use super::rules::{Waiting, acted, day_is_spent, unavailable};
use super::session::{SessionAsk, run_session};
use super::{OrchestratorDeps, OrchestratorError, TickReport, TickRules, TickScope};
use crate::session::SessionPurpose;

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
    answer_a_question(deps, team, board, waiting).await
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
            continue;
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

    use serde_json::json;

    use super::DECIDE_QUESTION_TOOLS;
    use crate::orchestrator::fixtures::Harness;
    use crate::recorded::fixtures::{implement_stops_early, tool_calls};
    use crate::session::SessionPurpose;
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
}
