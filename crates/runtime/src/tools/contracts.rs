//! Triage, contract writing, filing tasks, and planning a sprint: the tools that decide what a task
//! is and when it is worked on.

use farik_core::contract::{Role, TaskContract, TaskId, TaskKind, TaskStatus, validate_contract};
use farik_core::criteria::expand_criteria;
use farik_core::governor::gates::{
    ContractWriteActor, ContractWriteOutcome, ParentEpic, check_child_creation,
    check_contract_write,
};
use farik_core::governor::transition_table::TransitionActor;
use farik_protocol::event::{
    ContractJudgedBody, ContractWrittenBody, EventBody, EventKind, RequestTriagedBody,
    RequestTriagedBodySize,
};
use farik_store::EventQuery;
use farik_store::requests::{RequestError, file_request, summary_of};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::sprints::{self, PlannedBy, SprintError};

/// How big a request is (5.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Size {
    /// An epic, broken down into tasks.
    Large,
    /// One task.
    Small,
}

/// `farik_triage_request`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TriageInput {
    /// `large` for an epic, `small` for a task.
    size: Size,
    /// Why, in a sentence the log keeps.
    reason: String,
}

/// `farik_record_judgment`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordJudgmentInput {
    /// Whether the task fits its budget.
    pub(crate) fits_budget: bool,
    /// Whether its criteria would detect the failure its intent worries about.
    pub(crate) criteria_detect_failure: bool,
    /// Why, in a sentence the log keeps.
    pub(crate) reason: String,
}

/// A criterion from the library, and the id it takes in the contract.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CriterionRef {
    /// The id in the contract, `C<n>`.
    id: String,
    /// The criterion's name in the library.
    name: String,
}

/// `farik_write_contract`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteContractInput {
    /// Top-level fields of the contract, each replacing the current value.
    #[serde(default)]
    fields: Map<String, Value>,
    /// Criteria from the library, appended to the exit criteria.
    #[serde(default)]
    criteria: Vec<CriterionRef>,
}

/// `farik_create_task`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTaskInput {
    /// The contract as its author writes it: no id, status, stamps, kind, or parent.
    contract: Map<String, Value>,
    /// The epic this is a task of, when it is one.
    parent: Option<String>,
}

/// `farik_plan_sprint`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanSprintInput {
    /// The ready tasks and approved epics to put in the open sprint.
    task_ids: Vec<String>,
}

/// Sizes the session's request (5.16). The Scrum Master triages an untriaged `draft` without a
/// parent, and so does the Product Manager when the team has no active Scrum Master; the Product
/// Manager may also re-size a `refining` standalone task as `large`, which makes it an epic.
/// Everything else is refused, a `draft` the human already triaged included: the human's triage
/// wins when it comes first. The kind is written to the file and `request.triaged` recorded.
pub(super) fn triage(call: &Call<'_>, input: &TriageInput) -> Result<Value, ToolError> {
    let task = call.task()?;
    if input.reason.trim().is_empty() {
        return Err(Refusal::BlankReason.into());
    }
    let (mut contract, row) = call.contract(task)?;
    let role = call.role();
    let untriaged_request = row.status == TaskStatus::Draft && row.parent.is_none() && !row.triaged;
    let allowed = match role {
        Role::ScrumMaster => untriaged_request,
        Role::ProductManager => {
            (untriaged_request && !call.team.has_active(Role::ScrumMaster))
                || (row.status == TaskStatus::Refining
                    && row.parent.is_none()
                    && row.kind == TaskKind::Task
                    && input.size == Size::Large)
        }
        _ => false,
    };
    if !allowed {
        return Err(Refusal::TriageNotAllowed {
            role,
            status: row.status,
            has_parent: row.parent.is_some(),
            triaged: row.triaged,
        }
        .into());
    }
    contract.kind = match input.size {
        Size::Large => TaskKind::Epic,
        Size::Small => TaskKind::Task,
    };
    contract.updated_at = Some(call.deps().clock.now());
    call.deps()
        .files
        .write_contract(&contract)
        .map_err(failed)?;
    let event = call.append(
        Some(task),
        EventBody::RequestTriaged(RequestTriagedBody {
            size: match input.size {
                Size::Large => RequestTriagedBodySize::Large,
                Size::Small => RequestTriagedBodySize::Small,
            },
            reason: input.reason.trim().to_string(),
            triaged_by: call.agent_id().to_string(),
        }),
    )?;
    Ok(
        json!({ "task_id": task.as_str(), "kind": contract.kind.to_string(), "seq": event.envelope.seq }),
    )
}

/// Records the Scrum Master's judgment of the session's contract against the Definition of
/// Ready's judgment rules (5.3): whether it fits its budget and whether its criteria would detect
/// the failure its intent worries about, with the reason. Only the Scrum Master judges, and only
/// a task `refining`.
pub(super) fn record_judgment(
    call: &Call<'_>,
    input: &RecordJudgmentInput,
) -> Result<Value, ToolError> {
    let task = call.task()?;
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(Refusal::BlankReason.into());
    }
    let (_, row) = call.contract(task)?;
    if call.role() != Role::ScrumMaster || row.status != TaskStatus::Refining {
        return Err(Refusal::JudgmentNotAllowed {
            role: call.role(),
            status: row.status,
        }
        .into());
    }
    let event = call.append(
        Some(task),
        EventBody::ContractJudged(ContractJudgedBody {
            judged_by: call.agent_id().to_string(),
            fits_budget: input.fits_budget,
            criteria_detect_failure: input.criteria_detect_failure,
            reason: reason.to_string(),
        }),
    )?;
    Ok(json!({ "task_id": task.as_str(), "seq": event.envelope.seq }))
}

/// Writes fields of the session's contract and appends criteria from the library by name, as
/// `check_contract_write` allows the caller: by role first (Product Manager, Scrum Master), then by
/// relation (assignee, reviewer). An epic's contract waits while a question about it is
/// unanswered. The result is held to the contract's rules, written, and recorded as
/// `contract.written`.
pub(super) fn write_contract(
    call: &Call<'_>,
    input: WriteContractInput,
) -> Result<Value, ToolError> {
    let task = call.task()?;
    let (contract, row) = call.contract(task)?;
    let actor = contract_writer(call, &contract)?;
    if contract.kind == TaskKind::Epic && has_asked(call, task)? {
        return Err(Refusal::QuestionUnanswered {
            task_id: task.to_string(),
        }
        .into());
    }
    let before = serde_json::to_value(&contract).map_err(failed)?;
    let mut after = before.clone();
    let object = after.as_object_mut().ok_or_else(|| ToolError::Failed {
        detail: "a contract serialised as something other than a mapping".to_string(),
    })?;
    for (field, value) in input.fields {
        object.insert(field, value);
    }
    if !input.criteria.is_empty() {
        let refs: Vec<(String, String)> = input
            .criteria
            .into_iter()
            .map(|one| (one.id, one.name))
            .collect();
        let library = call.deps().files.read_criteria().map_err(failed)?;
        let expanded = expand_criteria(&refs, &library).map_err(Refusal::Criteria)?;
        let mut criteria = object
            .get("exit_criteria")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for criterion in expanded {
            criteria.push(serde_json::to_value(criterion).map_err(failed)?);
        }
        object.insert("exit_criteria".to_string(), Value::Array(criteria));
    }
    fill_reviewer_role(call, object, contract.kind);
    let changed = changed_fields(&before, &after);
    match check_contract_write(contract.kind, row.status, row.locked, &actor, &changed) {
        Ok(ContractWriteOutcome::Allowed) => {}
        Ok(ContractWriteOutcome::ReturnsToRefining) => {
            return Err(ToolError::Failed {
                detail:
                    "the governor sent an agent's write back to refining, which only a human's does"
                        .to_string(),
            });
        }
        Err(refusal) => return Err(Refusal::ContractWrite(refusal).into()),
    }
    let mut written = validate_contract(&after).map_err(|errors| Refusal::ContractInvalid {
        details: errors
            .iter()
            .map(|error| format!("{} {}", error.path, error.message))
            .collect(),
    })?;
    written.updated_at = Some(call.deps().clock.now());
    call.deps().files.write_contract(&written).map_err(failed)?;
    let event = call.append(
        Some(task),
        EventBody::ContractWritten(ContractWrittenBody {
            summary: summary_of(&written),
            written_by: call.agent_id().to_string(),
        }),
    )?;
    Ok(json!({ "task_id": task.as_str(), "changed": changed, "seq": event.envelope.seq }))
}

/// Files a new `draft` request as `farik task create` does, or with `parent` a task of that epic
/// once `check_child_creation` allows the caller. A `reviewer_role` left out is filled with the
/// role the team can staff, when there is one.
pub(super) fn create_task(call: &Call<'_>, input: CreateTaskInput) -> Result<Value, ToolError> {
    let parent = match &input.parent {
        None => None,
        Some(parent) => {
            let parent: TaskId = parent.parse().map_err(|error| ToolError::InvalidInput {
                detail: format!("{parent} is not a task id: {error}"),
            })?;
            let row = call.row(&parent)?;
            if row.kind != TaskKind::Epic {
                return Err(Refusal::NotAnEpic {
                    task_id: parent.to_string(),
                }
                .into());
            }
            let actor = ContractWriteActor {
                kind: match call.role() {
                    Role::ProductManager => TransitionActor::ProductManager,
                    Role::ScrumMaster => TransitionActor::ScrumMaster,
                    _ => TransitionActor::Assignee,
                },
                agent_id: Some(call.agent_id().to_string()),
            };
            check_child_creation(
                &ParentEpic {
                    status: row.status,
                    assignee_id: row.assignee_id.clone().unwrap_or_default(),
                },
                &actor,
            )
            .map_err(|details| Refusal::GateFailed { details })?;
            Some(parent)
        }
    };
    let mut wire = input.contract;
    fill_reviewer_role(call, &mut wire, TaskKind::Task);
    let deps = call.deps();
    let filed = file_request(
        &deps.files,
        &deps.log,
        Value::Object(wire),
        call.agent_id(),
        parent.as_ref(),
        deps.clock.now(),
        &call.ids(None),
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => Refusal::RequestRefused { reason }.into(),
        other => failed(other),
    })?;
    deps.projections.catch_up().map_err(failed)?;
    let mut answer = json!({ "task_id": filed.id.as_str(), "status": "draft" });
    // The task is filed whatever the join says: an error here would have the agent file it again.
    if parent.is_some()
        && let Err(error) = sprints::join_epics_sprint(deps, &filed.id)
    {
        answer["warning"] = json!(format!(
            "{} is filed and joined no sprint: {error}",
            filed.id.as_str()
        ));
    }
    Ok(answer)
}

/// Plans the open sprint as the assigner (5.5), through `sprints::plan_sprint`, and answers the
/// sprint and every task now in it.
pub(super) fn plan_sprint(call: &Call<'_>, input: &PlanSprintInput) -> Result<Value, ToolError> {
    let task_ids = input
        .task_ids
        .iter()
        .map(|id| {
            id.parse::<TaskId>()
                .map_err(|error| ToolError::InvalidInput {
                    detail: format!("{id} is not a task id: {error}"),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sprint = sprints::plan_sprint(
        call.deps(),
        &task_ids,
        &PlannedBy::Assigner(call.agent_id().to_string()),
    )
    .map_err(sprint_failed)?;
    Ok(json!({
        "sprint_id": sprint.id.as_str(),
        "task_ids": sprint.task_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
    }))
}

/// A sprint's refusal as the tool's, and its files' or store's failure as the tool's failure.
fn sprint_failed(error: SprintError) -> ToolError {
    match error {
        SprintError::Files(error) => failed(error),
        SprintError::Store(error) => failed(error),
        SprintError::Refused { reason } => ToolError::Refused { reason },
        other => ToolError::Refused {
            reason: format!("sprint_plan_refused: {other}"),
        },
    }
}

/// Which actor a contract write is judged as: by role first, then by relation to the contract.
fn contract_writer(
    call: &Call<'_>,
    contract: &TaskContract,
) -> Result<ContractWriteActor, ToolError> {
    let me = call.agent_id();
    let kind = match call.role() {
        Role::ProductManager => TransitionActor::ProductManager,
        Role::ScrumMaster => TransitionActor::ScrumMaster,
        _ if contract.assignee.as_deref() == Some(me) => TransitionActor::Assignee,
        _ if contract.reviewer.as_deref() == Some(me) => TransitionActor::Reviewer,
        _ => {
            return Err(Refusal::NotAContractWriter {
                agent_id: me.to_string(),
                task_id: contract.id.to_string(),
            }
            .into());
        }
    };
    Ok(ContractWriteActor {
        kind,
        agent_id: Some(me.to_string()),
    })
}

/// Whether a question about `task` is still open: asked, and named by no `question.answered`
/// (5.7).
fn has_asked(call: &Call<'_>, task: &TaskId) -> Result<bool, ToolError> {
    let events = call
        .deps()
        .log
        .read(&EventQuery {
            task_id: Some(task.clone()),
            kinds: vec![EventKind::QuestionAsked, EventKind::QuestionAnswered],
            ..EventQuery::default()
        })
        .map_err(failed)?;
    let answered: Vec<u64> = events
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::QuestionAnswered(body) => Some(body.question_id.get()),
            _ => None,
        })
        .collect();
    Ok(events.iter().any(|event| {
        event.body.kind() == EventKind::QuestionAsked && !answered.contains(&event.envelope.seq)
    }))
}

/// Fills `reviewer_role` with the role the team can staff (D7) when `assignee_role` is there and
/// `reviewer_role` is not; a role the team cannot staff leaves it out, so that the contract's
/// rules name it and the writer learns there is no reviewer.
fn fill_reviewer_role(call: &Call<'_>, wire: &mut Map<String, Value>, kind: TaskKind) {
    if wire
        .get("reviewer_role")
        .is_some_and(|role| !role.is_null())
    {
        return;
    }
    let Some(assignee_role) = wire
        .get("assignee_role")
        .and_then(Value::as_str)
        .and_then(|role| serde_json::from_value::<Role>(json!(role)).ok())
    else {
        return;
    };
    if let Some(reviewer) = farik_roles::default_reviewer_role(&call.team, kind, assignee_role) {
        wire.insert("reviewer_role".to_string(), json!(reviewer.to_string()));
    }
}

/// The top-level fields whose values differ between the two contracts, in `after`'s order, then
/// any `before` had that `after` dropped.
fn changed_fields(before: &Value, after: &Value) -> Vec<String> {
    let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
        return Vec::new();
    };
    after
        .iter()
        .filter(|(field, value)| before.get(*field) != Some(*value))
        .map(|(field, _)| field.clone())
        .chain(
            before
                .keys()
                .filter(|field| !after.contains_key(*field))
                .cloned(),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use farik_protocol::event::{EventBody, EventKind, FarikEvent};
    use serde_json::{Value, json};

    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    fn a_project(name: &str) -> TestProject {
        TestProject::new(name, &a_team_of_three(|_| {}))
    }

    fn refused_with(result: Result<Value, ToolError>, kind: &str) {
        match result {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with(&format!("{kind}: ")), "{reason}");
            }
            other => panic!("expected a {kind} refusal, got {other:?}"),
        }
    }

    fn triage(size: &str) -> Value {
        json!({ "size": size, "reason": "One deliverable and one reviewer." })
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lets_the_product_manager_triage_a_draft_without_a_scrum_master() {
        let project = a_project("tools-triage-pm");
        project.filed("FRK-1", "draft", "task", None);
        project
            .call("pm", Some("FRK-1"), "farik_triage_request", triage("large"))
            .expect("the Product Manager triages when there is no Scrum Master");

        assert_eq!(project.file("FRK-1")["kind"], "epic");
        let triaged = project.events(&[EventKind::RequestTriaged]);
        assert_eq!(triaged.len(), 1);
        let EventBody::RequestTriaged(body) = &triaged[0].body else {
            panic!("a triage");
        };
        assert_eq!(body.triaged_by, "pm");
        assert_eq!(
            triaged[0].envelope.ids.session_id.as_deref(),
            Some("session-1")
        );
        let row = project.deps.projections.board().expect("a board");
        assert!(row[0].triaged);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_triage_from_a_developer() {
        let project = a_project("tools-triage-dev");
        project.filed("FRK-1", "draft", "task", None);
        let before = project.event_count();
        refused_with(
            project.call(
                "dev-a",
                Some("FRK-1"),
                "farik_triage_request",
                triage("small"),
            ),
            "triage_not_allowed",
        );
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_small_on_a_refining_task() {
        let project = a_project("tools-triage-small");
        project.filed("FRK-1", "refining", "task", None);
        project.record(
            "FRK-1",
            "request.triaged",
            &json!({ "size": "small", "reason": "one thing", "triaged_by": "human" }),
        );
        let before = project.event_count();
        refused_with(
            project.call("pm", Some("FRK-1"), "farik_triage_request", triage("small")),
            "triage_not_allowed",
        );
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_triage_what_the_human_already_triaged() {
        let project = a_project("tools-triage-human");
        project.filed("FRK-1", "draft", "task", None);
        project.record(
            "FRK-1",
            "request.triaged",
            &json!({ "size": "small", "reason": "one thing", "triaged_by": "human" }),
        );
        let before = project.event_count();
        refused_with(
            project.call("pm", Some("FRK-1"), "farik_triage_request", triage("large")),
            "triage_not_allowed",
        );
        assert_eq!(project.event_count(), before);
        assert_eq!(project.file("FRK-1")["kind"], "task");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_to_triage_a_child() {
        let project = a_project("tools-triage-child");
        project.filed("FRK-1", "in_progress", "epic", None);
        project.filed("FRK-2", "draft", "task", Some("FRK-1"));
        let before = project.event_count();
        refused_with(
            project.call("pm", Some("FRK-2"), "farik_triage_request", triage("small")),
            "triage_not_allowed",
        );
        assert_eq!(project.event_count(), before);
    }

    /// `a_team_of_three`, with an active Scrum Master `sm` besides.
    fn a_project_with_scrum_master(name: &str) -> TestProject {
        TestProject::new(
            name,
            &a_team_of_three(|wire| {
                wire["agents"].as_array_mut().expect("agents").push(
                    farik_core::team::fixtures::an_agent_wire("sm", "scrum_master"),
                );
            }),
        )
    }

    fn judgment(fits_budget: bool, criteria_detect_failure: bool, reason: &str) -> Value {
        json!({
            "fits_budget": fits_budget,
            "criteria_detect_failure": criteria_detect_failure,
            "reason": reason
        })
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_judgment_by_the_scrum_master() {
        let project = a_project_with_scrum_master("tools-judge-sm");
        project.filed("FRK-1", "refining", "task", None);
        let result = project
            .call(
                "sm",
                Some("FRK-1"),
                "farik_record_judgment",
                judgment(
                    true,
                    false,
                    "It fits the sprint, but the criteria only check the happy path.",
                ),
            )
            .expect("the Scrum Master judges a refining task");
        let judged = project.events(&[EventKind::ContractJudged]);
        assert_eq!(judged.len(), 1);
        let EventBody::ContractJudged(body) = &judged[0].body else {
            panic!("a contract.judged");
        };
        assert_eq!(body.judged_by, "sm");
        assert!(body.fits_budget);
        assert!(!body.criteria_detect_failure);
        assert_eq!(
            body.reason,
            "It fits the sprint, but the criteria only check the happy path."
        );
        assert_eq!(result["seq"], judged[0].envelope.seq);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_judgment_by_anyone_else() {
        let project = a_project_with_scrum_master("tools-judge-others");
        project.filed("FRK-1", "refining", "task", None);
        let before = project.event_count();
        for agent in ["pm", "dev-a"] {
            refused_with(
                project.call(
                    agent,
                    Some("FRK-1"),
                    "farik_record_judgment",
                    judgment(
                        true,
                        true,
                        "Fits the sprint and the criteria would catch it.",
                    ),
                ),
                "judgment_not_allowed",
            );
        }
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_judgment_outside_refining() {
        let project = a_project_with_scrum_master("tools-judge-status");
        project.filed("FRK-1", "ready", "task", None);
        let before = project.event_count();
        refused_with(
            project.call(
                "sm",
                Some("FRK-1"),
                "farik_record_judgment",
                judgment(
                    true,
                    true,
                    "Fits the sprint and the criteria would catch it.",
                ),
            ),
            "judgment_not_allowed",
        );
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_blank_reason() {
        let project = a_project_with_scrum_master("tools-judge-blank");
        project.filed("FRK-1", "refining", "task", None);
        let before = project.event_count();
        refused_with(
            project.call(
                "sm",
                Some("FRK-1"),
                "farik_record_judgment",
                judgment(true, true, "  "),
            ),
            "blank_reason",
        );
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_contract_fields_and_expands_criteria_by_name() {
        let project = a_project("tools-write");
        project.filed("FRK-1", "refining", "task", None);
        project
            .call(
                "pm",
                Some("FRK-1"),
                "farik_write_contract",
                json!({
                    "fields": { "intent": "A person signs in and sees only their own work." },
                    "criteria": [{ "id": "C2", "name": "unit-tests" }]
                }),
            )
            .expect("the Product Manager writes a refining contract");

        let file = project.file("FRK-1");
        assert_eq!(
            file["intent"],
            "A person signs in and sees only their own work."
        );
        assert_eq!(file["exit_criteria"][1]["id"], "C2");
        assert_eq!(
            file["exit_criteria"][1]["text"],
            "The unit tests pass, and this change adds one."
        );
        let written = project.events(&[EventKind::ContractWritten]);
        assert_eq!(written.len(), 1);
        let EventBody::ContractWritten(body) = &written[0].body else {
            panic!("a contract.written");
        };
        assert_eq!(body.written_by, "pm");
        assert_eq!(body.summary.status.to_string(), "refining");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_write_to_a_locked_contract() {
        let project = a_project("tools-write-locked");
        project.filed_with("FRK-1", "refining", "task", None, |wire| {
            wire["locked"] = json!(true);
        });
        project.record("FRK-1", "contract.locked", &json!({ "locked_by": "human" }));
        let before = project.event_count();
        refused_with(
            project.call(
                "pm",
                Some("FRK-1"),
                "farik_write_contract",
                json!({ "fields": { "intent": "Something else entirely, and longer." } }),
            ),
            "contract_locked",
        );
        assert_eq!(project.event_count(), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn fills_the_reviewer_role_the_team_can_staff() {
        let project = a_project("tools-reviewer-role");
        let mut request = a_request();
        request.remove("reviewer_role");
        let filed = project
            .call(
                "pm",
                None,
                "farik_create_task",
                json!({ "contract": request }),
            )
            .expect("the request is filed");
        let task = filed["task_id"].as_str().expect("an id").to_string();
        assert_eq!(
            project.file(&task)["reviewer_role"],
            "software_developer",
            "no Architect, and two Developers: one reviews the other"
        );

        project
            .call(
                "pm",
                Some(&task),
                "farik_write_contract",
                json!({ "fields": { "assignee_role": "software_developer", "reviewer_role": "architect" } }),
            )
            .expect("a draft is the Product Manager's to write");
        assert_eq!(
            project.file(&task)["reviewer_role"],
            "architect",
            "kept as given"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_contract_write_from_a_bystander() {
        let project = a_project("tools-write-bystander");
        project.filed("FRK-1", "in_progress", "task", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-b", "reviewer": "pm" }),
        );
        refused_with(
            project.call(
                "dev-a",
                Some("FRK-1"),
                "farik_write_contract",
                json!({ "fields": { "intent": "Something else entirely, and longer." } }),
            ),
            "not_a_contract_writer",
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_epic_write_while_a_question_is_unanswered() {
        let project = a_project("tools-write-question");
        project.filed("FRK-1", "refining", "epic", None);
        project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Who signs in?", "asked_by": "pm" }),
        );
        refused_with(
            project.call(
                "pm",
                Some("FRK-1"),
                "farik_write_contract",
                json!({ "fields": { "intent": "Something else entirely, and longer." } }),
            ),
            "question_unanswered",
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lets_an_epic_be_written_once_its_question_is_answered() {
        let project = a_project("tools-write-answered");
        project.filed("FRK-1", "refining", "epic", None);
        let question = project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Who signs in?", "asked_by": "pm" }),
        );
        let write = || {
            project.call(
                "pm",
                Some("FRK-1"),
                "farik_write_contract",
                json!({ "fields": { "intent": "Something else entirely, and longer." } }),
            )
        };
        refused_with(write(), "question_unanswered");

        project.record(
            "FRK-1",
            "question.answered",
            &json!({
                "question_id": question.envelope.seq,
                "answer": "Anyone with an account.",
                "answered_by": "human"
            }),
        );
        write().expect("the question is answered, so the epic is written");
        assert_eq!(
            project.file("FRK-1")["intent"],
            "Something else entirely, and longer."
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn waits_for_every_question_before_an_epic_is_written() {
        let project = a_project("tools-write-two-questions");
        project.filed("FRK-1", "refining", "epic", None);
        let asked = |question: &str| {
            project.record(
                "FRK-1",
                "question.asked",
                &json!({ "question": question, "asked_by": "pm" }),
            )
        };
        let first = asked("Who signs in?");
        let second = asked("Do they stay signed in?");
        let answer = |question: &FarikEvent| {
            project.record(
                "FRK-1",
                "question.answered",
                &json!({
                    "question_id": question.envelope.seq,
                    "answer": "Yes.",
                    "answered_by": "human"
                }),
            );
        };
        let write = || {
            project.call(
                "pm",
                Some("FRK-1"),
                "farik_write_contract",
                json!({ "fields": { "intent": "Something else entirely, and longer." } }),
            )
        };
        answer(&first);
        refused_with(write(), "question_unanswered");
        answer(&second);
        write().expect("both questions are answered, so the epic is written");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_child_of_an_epic_its_assignee_breaks_down() {
        let project = a_project("tools-child");
        project.filed("FRK-1", "in_progress", "epic", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm" }),
        );
        let filed = project
            .call(
                "pm",
                None,
                "farik_create_task",
                json!({ "contract": a_request(), "parent": "FRK-1" }),
            )
            .expect("the epic's assignee files its tasks");
        assert_eq!(filed["task_id"], "FRK-2");
        let child = project.file("FRK-2");
        assert_eq!(child["parent"], "FRK-1");
        assert_eq!(child["kind"], "task");
        let board = project.deps.projections.board().expect("a board");
        assert!(board[1].triaged, "the board has the child, triaged");

        let before = project.event_count();
        refused_with(
            project.call(
                "dev-a",
                None,
                "farik_create_task",
                json!({ "contract": a_request(), "parent": "FRK-1" }),
            ),
            "gate_failed",
        );
        assert_eq!(project.event_count(), before);
    }

    /// `filed` in `status`, of `kind`, with a budget of `usd` dollars.
    fn filed_at(project: &TestProject, task: &str, status: &str, kind: &str, usd: f64) {
        project.filed_with(task, status, kind, None, |wire| {
            wire["budget"]["max_cost_usd"] = json!(usd);
        });
    }

    fn plan(project: &TestProject, agent: &str, tasks: &[&str]) -> Result<Value, ToolError> {
        project.call(
            agent,
            None,
            "farik_plan_sprint",
            json!({ "task_ids": tasks }),
        )
    }

    /// The sprint the contract file of `task` names.
    fn sprint_of(project: &TestProject, task: &str) -> Value {
        project.file(task)["sprint"].clone()
    }

    /// Each `sprint.planned`, as its wire.
    fn planned(project: &TestProject) -> Vec<Value> {
        project
            .events(&[EventKind::SprintPlanned])
            .iter()
            .map(|event| match &event.body {
                EventBody::SprintPlanned(body) => serde_json::to_value(body).expect("a body"),
                other => panic!("a sprint.planned, got {other:?}"),
            })
            .collect()
    }

    /// The task ids sprint `sprint`'s file lists.
    fn held_by(project: &TestProject, sprint: &str) -> Vec<String> {
        project
            .deps
            .files
            .read_sprint(sprint)
            .expect("the sprint reads")
            .task_ids
            .iter()
            .map(|id| id.as_str().to_string())
            .collect()
    }

    /// A refusal of `sprint_plan_refused` that says each of `words`, with nothing recorded.
    fn plan_refused(project: &TestProject, agent: &str, tasks: &[&str], words: &[&str]) {
        let before = project.event_count();
        match plan(project, agent, tasks) {
            Err(ToolError::Refused { reason }) => {
                assert!(reason.starts_with("sprint_plan_refused: "), "{reason}");
                for word in words {
                    assert!(reason.contains(word), "{word} in {reason}");
                }
            }
            other => panic!("expected a sprint_plan_refused, got {other:?}"),
        }
        assert_eq!(project.event_count(), before, "nothing is recorded");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn plans_ready_tasks_into_the_sprint() {
        let project = a_project_with_scrum_master("tools-plan");
        project.filed("FRK-1", "ready", "task", None);
        project.filed("FRK-2", "ready", "task", None);
        project.open_sprint("S1", None, &[]);

        let answer = plan(&project, "sm", &["FRK-1", "FRK-2"]).expect("the Scrum Master plans");

        assert_eq!(
            answer,
            json!({ "sprint_id": "S1", "task_ids": ["FRK-1", "FRK-2"] })
        );
        assert_eq!(sprint_of(&project, "FRK-1"), "S1");
        assert_eq!(sprint_of(&project, "FRK-2"), "S1");
        assert_eq!(held_by(&project, "S1"), ["FRK-1", "FRK-2"]);
        assert_eq!(
            planned(&project),
            [json!({ "sprint_id": "S1", "task_ids": ["FRK-1", "FRK-2"], "planned_by": "sm" })]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_plan_past_the_sprint_budget() {
        let project = a_project_with_scrum_master("tools-plan-budget");
        filed_at(&project, "FRK-1", "ready", "task", 6.0);
        filed_at(&project, "FRK-2", "ready", "task", 5.0);
        project.open_sprint("S1", Some(10.0), &["FRK-1"]);

        plan_refused(&project, "sm", &["FRK-2"], &["budget", "S1"]);

        assert_eq!(sprint_of(&project, "FRK-2"), Value::Null);
        assert_eq!(held_by(&project, "S1"), ["FRK-1"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_task_that_is_not_ready() {
        let project = a_project_with_scrum_master("tools-plan-not-ready");
        project.filed("FRK-1", "refining", "task", None);
        project.open_sprint("S1", None, &[]);

        plan_refused(&project, "sm", &["FRK-1"], &["FRK-1", "refining"]);
        assert_eq!(sprint_of(&project, "FRK-1"), Value::Null);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_task_with_a_parent() {
        let project = a_project_with_scrum_master("tools-plan-parent");
        project.filed("FRK-1", "in_progress", "epic", None);
        project.filed("FRK-2", "ready", "task", Some("FRK-1"));
        project.open_sprint("S1", None, &[]);

        plan_refused(&project, "sm", &["FRK-2"], &["FRK-2", "FRK-1"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_task_already_in_a_sprint() {
        let project = a_project_with_scrum_master("tools-plan-in-a-sprint");
        project.filed("FRK-1", "ready", "task", None);
        project.open_sprint("S1", None, &["FRK-1"]);

        plan_refused(&project, "sm", &["FRK-1"], &["FRK-1", "already in S1"]);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_plan_by_anyone_but_the_assigner() {
        let project = a_project_with_scrum_master("tools-plan-assigner");
        project.filed("FRK-1", "ready", "task", None);
        project.open_sprint("S1", None, &[]);

        plan_refused(&project, "dev-a", &["FRK-1"], &["dev-a"]);
        plan_refused(&project, "pm", &["FRK-1"], &["pm", "Scrum Master"]);
        assert_eq!(sprint_of(&project, "FRK-1"), Value::Null);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_second_plan() {
        let project = a_project_with_scrum_master("tools-plan-second");
        project.filed("FRK-1", "ready", "task", None);
        project.filed("FRK-2", "ready", "task", None);
        project.open_sprint("S1", None, &["FRK-1"]);

        plan_refused(&project, "sm", &["FRK-2"], &["S1"]);
        assert_eq!(sprint_of(&project, "FRK-2"), Value::Null);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_plan_into_a_sprint_that_is_ending() {
        let project = a_project_with_scrum_master("tools-plan-ending");
        project.filed("FRK-1", "ready", "task", None);
        project.open_sprint("S1", None, &[]);
        // An end has written S1's file and not yet recorded `sprint.ended`.
        let mut ending = farik_core::sprint::fixtures::an_open_sprint_wire();
        ending["id"] = json!("S1");
        ending["status"] = json!("ended");
        ending["ended_at"] = json!("2026-09-24T01:00:00Z");
        project
            .deps
            .files
            .write_sprint(&farik_core::sprint::validate_sprint(&ending).expect("a sprint"))
            .expect("S1 is written");

        plan_refused(&project, "sm", &["FRK-1"], &["S1", "ended"]);
        assert_eq!(sprint_of(&project, "FRK-1"), Value::Null);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn plans_an_epics_tasks_with_it() {
        let project = a_project_with_scrum_master("tools-plan-epic");
        project.filed("FRK-1", "ready", "epic", None);
        project.filed("FRK-3", "in_progress", "epic", None);
        project.moved(
            "FRK-3",
            "assigned",
            "in_progress",
            &json!({ "assignee": "sm", "reviewer": "pm" }),
        );
        project.filed("FRK-4", "ready", "task", Some("FRK-3"));
        project.open_sprint("S1", None, &[]);

        plan(&project, "sm", &["FRK-1"]).expect("an approved epic is planned");

        assert_eq!(held_by(&project, "S1"), ["FRK-1"]);
        assert_eq!(sprint_of(&project, "FRK-4"), Value::Null);
        // Work under an epic that began before the sprint goes on.
        project
            .call(
                "sm",
                None,
                "farik_assign_task",
                json!({ "task_id": "FRK-4", "assignee_id": "dev-a", "reviewer_id": "dev-b" }),
            )
            .expect("FRK-4 is assigned while S1 is open");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn puts_an_epics_new_task_in_its_sprint() {
        let project = a_project("tools-child-sprint");
        project.filed("FRK-1", "in_progress", "epic", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm" }),
        );
        project.open_sprint("S1", None, &["FRK-1"]);

        project
            .call(
                "pm",
                None,
                "farik_create_task",
                json!({ "contract": a_request(), "parent": "FRK-1" }),
            )
            .expect("the epic's assignee files its tasks");

        assert_eq!(sprint_of(&project, "FRK-2"), "S1");
        let row = project
            .deps
            .projections
            .task(&"FRK-2".parse().expect("a task id"))
            .expect("the board reads")
            .expect("a row");
        assert_eq!(row.sprint.as_deref(), Some("S1"));
        assert_eq!(held_by(&project, "S1"), ["FRK-1", "FRK-2"]);
        assert_eq!(
            planned(&project).last(),
            Some(&json!({ "sprint_id": "S1", "task_ids": ["FRK-2"], "planned_by": "governor" }))
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_task_whose_join_fails_and_says_so() {
        let project = a_project("tools-child-sprint-ending");
        project.filed("FRK-1", "in_progress", "epic", None);
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm" }),
        );
        project.open_sprint("S1", None, &["FRK-1"]);
        // The human's end has written S1's file and not yet recorded `sprint.ended`.
        let mut ending = farik_core::sprint::fixtures::an_open_sprint_wire();
        ending["id"] = json!("S1");
        ending["task_ids"] = json!(["FRK-1"]);
        ending["status"] = json!("ended");
        ending["ended_at"] = json!("2026-09-24T01:00:00Z");
        project
            .deps
            .files
            .write_sprint(&farik_core::sprint::validate_sprint(&ending).expect("a sprint"))
            .expect("S1 is written");

        let answer = project
            .call(
                "pm",
                None,
                "farik_create_task",
                json!({ "contract": a_request(), "parent": "FRK-1" }),
            )
            .expect("the task is filed though it joins no sprint");

        assert_eq!(answer["task_id"], "FRK-2");
        let warning = answer["warning"].as_str().expect("a warning");
        assert!(warning.contains("S1 has ended"), "{warning}");
        assert_eq!(sprint_of(&project, "FRK-2"), Value::Null);
        assert_eq!(planned(&project).len(), 1, "only the fixture's plan");
    }

    /// A request as its author writes it.
    fn a_request() -> serde_json::Map<String, Value> {
        let mut wire = farik_core::contract::fixtures::a_contract_wire();
        let object = wire.as_object_mut().expect("a mapping");
        object.remove("id");
        object.remove("status");
        object.clone()
    }
}
