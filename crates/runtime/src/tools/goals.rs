//! `catervas_file_goal_work` and `catervas_record_goal_progress` (ADR 0075): what a goal session
//! files and records.

use catervas_core::goal::GOAL_WORK_PER_SESSION;
use catervas_protocol::event::{
    EventBody, EventKind, GoalMeasuredBody, GoalMeasuredBodySource, RequestTriagedBodySize,
};
use catervas_store::EventQuery;
use catervas_store::requests::{
    RequestError, file_goal_request, placeholder_budget_usd, request_from_brief,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::chat::given_request;
use crate::goals::goal_book;
use crate::session::SessionPurpose;

/// How big a step towards the goal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalWorkSize {
    /// An epic, which the Product Manager breaks down.
    Large,
    /// A task.
    Small,
}

/// `catervas_file_goal_work`'s input. Each text is counted trimmed.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileGoalWorkInput {
    /// A short title: 3 to 120 characters.
    title: String,
    /// The request in plain words: 20 to 2,000 characters.
    request: String,
    /// `large` for an epic, `small` for a task. One agent alone files `small` only.
    size: GoalWorkSize,
}

/// `catervas_record_goal_progress`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordGoalProgressInput {
    /// The goal's number as you read it: 0 or more.
    value: f64,
    /// What you counted, in a few words: 1 to 300 characters.
    note: Option<String>,
}

fn refused(code: &str, detail: impl std::fmt::Display) -> ToolError {
    ToolError::Refused {
        reason: format!("{code}: {detail}"),
    }
}

/// The goal that stands and is not paused, in a goal session.
fn working_goal(call: &Call<'_>) -> Result<catervas_core::goal::Goal, ToolError> {
    if call.context.purpose != SessionPurpose::Goal {
        return Err(refused(
            "goal_session_only",
            "this tool is for the session that works towards the owner's goal",
        ));
    }
    goal_book(&call.deps().log)
        .map_err(failed)?
        .standing
        .filter(|goal| !goal.paused)
        .ok_or_else(|| refused("goal_none", "no goal stands, or the owner paused it"))
}

fn out_of_bounds(field: &str, text: &str, least: usize, most: usize) -> Result<(), ToolError> {
    let length = text.trim().chars().count();
    if (least..=most).contains(&length) {
        Ok(())
    } else {
        Err(refused(
            "goal_work_out_of_bounds",
            format!("{field} is {length} characters; it must be {least} to {most}"),
        ))
    }
}

/// Files one step towards the goal: a draft the planner sized, or, for one agent alone, a small
/// task given to it.
pub(super) fn file_goal_work(
    call: &Call<'_>,
    input: &FileGoalWorkInput,
) -> Result<Value, ToolError> {
    let goal = working_goal(call)?;
    let deps = call.deps();
    let session = &call.context.session_id;
    let filed = deps
        .log
        .read(&EventQuery {
            kinds: vec![EventKind::TaskCreated],
            agent_id: Some(call.agent_id().to_string()),
            ..EventQuery::default()
        })
        .map_err(failed)?
        .iter()
        .filter(|event| {
            event.envelope.ids.session_id.as_deref() == Some(session.as_str())
                && matches!(
                    &event.body,
                    EventBody::TaskCreated(body)
                        if body.goal.as_ref().is_some_and(|id| id.as_str() == goal.id)
                )
        })
        .count();
    if filed >= GOAL_WORK_PER_SESSION {
        return Err(refused(
            "goal_work_limit",
            format!(
                "you have filed {GOAL_WORK_PER_SESSION} steps this session, the most one files"
            ),
        ));
    }
    let alone = call.team.is_solo();
    if alone && input.size == GoalWorkSize::Large {
        return Err(refused(
            "goal_large_in_solo",
            "one agent alone files small steps: it has no epics",
        ));
    }
    out_of_bounds("title", &input.title, 3, 120)?;
    out_of_bounds("request", &input.request, 20, 2_000)?;

    let rules = call.team.rules();
    let wire = if alone {
        let mut wire = given_request(&call.team, &call.agent, input.request.trim(), &rules)
            .map_err(|reason| Refusal::RequestRefused { reason })?;
        wire["title"] = json!(input.title.trim());
        wire
    } else {
        request_from_brief(
            input.title.trim(),
            input.request.trim(),
            placeholder_budget_usd(&rules),
        )
        .map_err(|reason| ToolError::InvalidInput { detail: reason })?
    };
    let task = file_goal_request(
        &deps.files,
        &deps.log,
        wire,
        call.agent_id(),
        &goal.id,
        match input.size {
            GoalWorkSize::Large => RequestTriagedBodySize::Large,
            GoalWorkSize::Small => RequestTriagedBodySize::Small,
        },
        alone.then(|| call.agent_id()),
        deps.clock.now(),
        &call.ids(None),
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => Refusal::RequestRefused { reason }.into(),
        other => failed(other),
    })?;
    deps.projections.catch_up().map_err(failed)?;
    Ok(json!({
        "task_id": task.id.as_str(),
        "status": "draft",
        "left": GOAL_WORK_PER_SESSION - filed - 1,
    }))
}

/// Records the goal's number as the caller read it from the goal's service.
pub(super) fn record_goal_progress(
    call: &Call<'_>,
    input: &RecordGoalProgressInput,
) -> Result<Value, ToolError> {
    let goal = working_goal(call)?;
    let (Some(_), Some(server)) = (goal.target, goal.server.as_deref()) else {
        return Err(refused(
            "goal_not_measured",
            "this goal has no number to read from a service: the owner tells the team",
        ));
    };
    let holds_it = call
        .agent
        .mcp_servers
        .iter()
        .flatten()
        .any(|entry| entry.name.as_str() == server);
    if !holds_it {
        return Err(refused(
            "goal_not_your_service",
            format!("you have not connected {server}, the service this goal is counted from"),
        ));
    }
    if !input.value.is_finite() || input.value < 0.0 {
        return Err(refused(
            "goal_value",
            "a number is 0 or more, and a real number",
        ));
    }
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    if let Some(note) = note {
        out_of_bounds("note", note, 1, 300)?;
    }
    let bad = |field: &str| ToolError::InvalidInput {
        detail: format!("the goal's {field} is not one the log can hold"),
    };
    let body = GoalMeasuredBody {
        goal: goal.id.clone().try_into().map_err(|_| bad("id"))?,
        value: input.value,
        source: GoalMeasuredBodySource::Connector,
        server: Some(server.to_string().try_into().map_err(|_| bad("service"))?),
        note: note
            .map(|note| note.to_string().try_into().map_err(|_| bad("note")))
            .transpose()?,
    };
    call.append(None, EventBody::GoalMeasured(body))?;
    Ok(json!({ "goal": goal.id, "value": input.value }))
}

#[cfg(test)]
mod tests {
    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    fn a_service(name: &str) -> Value {
        json!({
            "name": name, "source": "custom", "transport": "stdio",
            "command": format!("{name}-mcp"), "args": ["stdio"], "credential_keys": ["API_KEY"],
            "tools": { "read": "network" }
        })
    }

    /// A project whose owner set goal-1: ten paying customers, counted from Stripe, which `dev-a`
    /// has connected.
    fn a_project_with_a_goal(name: &str) -> TestProject {
        let project = TestProject::new(
            name,
            &a_team_of_three(|wire| {
                wire["budgets"]["daily_usd"] = json!(10);
                wire["agents"][1]["mcp_servers"] = json!([a_service("stripe")]);
            }),
        );
        set_goal(&project);
        project
    }

    fn set_goal(project: &TestProject) {
        project.record(
            "",
            "goal.set",
            &json!({
                "goal": "goal-1", "words": "Reach 10 paying customers", "target": 10.0,
                "unit": "paying customers", "server": "stripe"
            }),
        );
    }

    fn reason(refused: &ToolError) -> &str {
        let ToolError::Refused { reason } = refused else {
            panic!("a refusal, not {refused:?}");
        };
        reason
    }

    fn work(size: &str) -> Value {
        json!({
            "title": "Take card payments",
            "request": "Add card payments to the checkout, so a visitor can become a paying customer.",
            "size": size
        })
    }

    fn file(
        project: &TestProject,
        purpose: SessionPurpose,
        agent: &str,
        size: &str,
    ) -> Result<Value, ToolError> {
        project.call_in(purpose, agent, None, "catervas_file_goal_work", work(size))
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_work_and_records_progress_in_a_goal_session() {
        let project = a_project_with_a_goal("tools-goal");

        let first = file(&project, SessionPurpose::Goal, "pm", "large").expect("the planner files");
        assert_eq!(
            first,
            json!({ "task_id": "CTV-1", "status": "draft", "left": 4 })
        );
        let second =
            file(&project, SessionPurpose::Goal, "pm", "small").expect("the planner files");
        assert_eq!(second["task_id"], "CTV-2");
        let created = project.events(&[EventKind::TaskCreated]);
        assert_eq!(created.len(), 2);
        for event in &created {
            let EventBody::TaskCreated(body) = &event.body else {
                panic!("a task.created");
            };
            assert_eq!(body.goal.as_ref().map(|goal| goal.as_str()), Some("goal-1"));
        }
        for _ in 0..3 {
            file(&project, SessionPurpose::Goal, "pm", "small").expect("up to five");
        }
        let sixth = file(&project, SessionPurpose::Goal, "pm", "small").expect_err("the sixth");
        assert!(reason(&sixth).starts_with("goal_work_limit: "), "{sixth:?}");

        let outside = file(&project, SessionPurpose::Implement, "pm", "small")
            .expect_err("not a goal session");
        assert!(
            reason(&outside).starts_with("goal_session_only: "),
            "{outside:?}"
        );

        let record = |agent: &str, value: f64| {
            project.call_in(
                SessionPurpose::Goal,
                agent,
                None,
                "catervas_record_goal_progress",
                json!({ "value": value }),
            )
        };
        let foreign = record("pm", 4.0).expect_err("pm has no Stripe");
        assert!(
            reason(&foreign).starts_with("goal_not_your_service: "),
            "{foreign:?}"
        );
        assert_eq!(
            record("dev-a", 4.0).expect("dev-a reads Stripe"),
            json!({ "goal": "goal-1", "value": 4.0 })
        );
        let measured = project.events(&[EventKind::GoalMeasured]);
        assert_eq!(measured.len(), 1);
        assert_eq!(measured[0].envelope.ids.agent_id.as_deref(), Some("dev-a"));
        let EventBody::GoalMeasured(body) = &measured[0].body else {
            panic!("a goal.measured");
        };
        assert_eq!(
            (body.value, body.source.to_string()),
            (4.0, "connector".to_string())
        );
        assert_eq!(
            body.server.as_ref().map(|server| server.as_str()),
            Some("stripe")
        );
        let negative = record("dev-a", -1.0).expect_err("below zero");
        assert!(
            reason(&negative).starts_with("goal_value: "),
            "{negative:?}"
        );

        let alone = TestProject::new(
            "tools-goal-solo",
            &a_team_of_three(|wire| {
                wire["agents"] = json!([catervas_core::team::fixtures::an_agent_wire(
                    "theo",
                    "software_developer"
                )]);
                wire["policy"]["solo"] = json!(true);
                wire["budgets"]["daily_usd"] = json!(10);
            }),
        );
        set_goal(&alone);
        let large =
            file(&alone, SessionPurpose::Goal, "theo", "large").expect_err("no epics alone");
        assert!(
            reason(&large).starts_with("goal_large_in_solo: "),
            "{large:?}"
        );
        let small = file(&alone, SessionPurpose::Goal, "theo", "small").expect("a small task");
        assert_eq!(small["task_id"], "CTV-1");
        let created = alone.events(&[EventKind::TaskCreated]);
        let EventBody::TaskCreated(body) = &created[0].body else {
            panic!("a task.created");
        };
        assert_eq!(
            body.given_to.as_ref().map(|agent| agent.as_str()),
            Some("theo")
        );
    }
}
