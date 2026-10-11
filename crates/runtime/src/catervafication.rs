//! The Catervafication (`docs/SPEC.md` 5.16, ADR 0051): the epic that has the team document the
//! project, the fixed tasks it is filed with, and when it starts.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the command, the rules and the tick use these in the tasks that follow"
    )
)]

use std::fmt;

use catervas_core::contract::{Role, TaskId, TaskKind, TaskStatus};
use catervas_core::folders::role_folder;
use catervas_core::governor::gates::DesignerBrowser;
use catervas_core::governor::transition::TransitionRequest;
use catervas_core::governor::transition_table::TransitionActor;
use catervas_core::team::Team;
use catervas_protocol::event::{
    CatervasEvent, EventBody, EventIds, EventKind, HumanAcceptedBody, HumanAcceptedBodySubject,
    new_event,
};
use catervas_store::files::FilesError;
use catervas_store::requests::{
    RequestError, file_catervafication_epic, file_request, placeholder_budget_usd,
};
use catervas_store::{EventLog, EventQuery, StoreError, TaskProjection};
use serde_json::{Value, json};

use crate::sprints::{PlannedBy, SprintError, join_epics_sprint, plan_sprint, start_sprint};
use crate::tools::ToolDeps;
use crate::transitions::{TransitionAsk, TransitionError, TransitionOutcome};

/// What the Catervafication documents: a repository that exists, or a project made from a
/// paragraph.
pub(crate) use catervas_protocol::event::CatervaficationStartedBodyProject as Project;

/// The roles that get a task of an existing repository's Catervafication, in the order they are
/// filed.
const EXISTING: [Role; 5] = [
    Role::ProductManager,
    Role::Architect,
    Role::SoftwareDeveloper,
    Role::UiUxDesigner,
    Role::MarketingSpecialist,
];

/// The roles that get a task of a new project's Catervafication: the product plan is step 06's task.
const NEW: [Role; 3] = [
    Role::Architect,
    Role::SoftwareDeveloper,
    Role::MarketingSpecialist,
];

const FOLLOW_THE_SKILL: &str = "Follow your skill catervafying-the-repository.";
const ASKS_WHERE_IT_READ: &str = "Does each document say only what the project, its history or its owner's words show, and name where it read it?";
const ASKS_FOR_CONTRADICTIONS: &str = "Does any document contradict another, or the code?";
const ASKS_ABOUT_THE_WORKFLOW: &str = "Does workflow.md say at its top whether it records the project's own process or adopts Catervas's recommended workflow, and does the evidence it names support that choice?";
const ASKS_ABOUT_THE_PLAN: &str =
    "Was a marketing plan proposed from docs/catervas/product/ alone?";

/// The roles that get a task, each with the role that reviews it, in the order they are filed.
///
/// A role is listed when it has an active agent and `default_reviewer_role` names a reviewer for
/// its task, since `ReviewerAvailable` would refuse it otherwise. The Marketing Specialist is
/// listed only with a Product Manager's row for an existing repository: it reads the plan that row
/// writes. The UI/UX Designer is listed only when `designer_ready` says its browser is ready.
#[must_use]
pub(crate) fn catervafication_roles(
    team: &Team,
    project: Project,
    designer_ready: bool,
) -> Vec<(Role, Role)> {
    let candidates: &[Role] = match project {
        Project::Existing => &EXISTING,
        Project::New => &NEW,
    };
    let mut rows: Vec<(Role, Role)> = Vec::new();
    for &role in candidates {
        if team
            .active_agents()
            .all(|agent| Role::from(agent.role) != role)
            || (role == Role::UiUxDesigner && !designer_ready)
            || (role == Role::MarketingSpecialist
                && project == Project::Existing
                && rows
                    .first()
                    .is_none_or(|(first, _)| *first != Role::ProductManager))
        {
            continue;
        }
        if let Some(reviewer) = catervas_roles::default_reviewer_role(team, TaskKind::Task, role) {
            rows.push((role, reviewer));
        }
    }
    rows
}

/// What a role's task says.
struct Row {
    title: &'static str,
    intent: &'static str,
    summary: &'static str,
    documents: &'static [&'static str],
    /// A question the review adds to the two every row asks.
    question: Option<&'static str>,
}

fn row(project: Project, role: Role) -> Row {
    match (project, role) {
        (Project::Existing, Role::ProductManager) => Row {
            title: "Write the product plan from the project",
            intent: "Read the project and write its product plan from what it holds: what the product is, who it is for and what comes next.",
            summary: "The Product Manager writes what your product is and what comes next, as your product plan to approve.",
            documents: &["spec.md", "spec.agent.md", "roadmap.md", "roadmap.agent.md"],
            question: None,
        },
        (Project::Existing, Role::Architect) => Row {
            title: "Document how the project is put together",
            intent: "Read the code and write down how the project is put together: an overview with a diagram, its features, its scheduled jobs and its structure.",
            summary: "The Architect writes how your project is put together, with diagrams.",
            documents: &["overview.md", "features.md", "jobs.md", "structure.md"],
            question: None,
        },
        (Project::Existing, Role::SoftwareDeveloper) => Row {
            title: "Document how the project is built and worked on",
            intent: "Read the project's configuration, tests and history and write down how it is built and worked on: its conventions and its workflow.",
            summary: "The Software Developer writes how your project is built and worked on: its habits and rules.",
            documents: &["conventions.md", "workflow.md"],
            question: Some(ASKS_ABOUT_THE_WORKFLOW),
        },
        (Project::Existing, Role::UiUxDesigner) => Row {
            title: "Write the project's UI inventory",
            intent: "Read the project's screens and styles and write an inventory of its user interface.",
            summary: "The UI/UX Designer writes an inventory of your project's screens and how they look.",
            documents: &["ui-inventory.md"],
            question: None,
        },
        (_, Role::MarketingSpecialist) => Row {
            title: "Propose the first marketing plan from the product plan",
            intent: "Read the approved product plan under docs/catervas/product/ and propose the project's first marketing plan from it.",
            summary: "The Marketing Specialist writes your marketing plan, after you approve the product plan.",
            documents: &[],
            question: Some(ASKS_ABOUT_THE_PLAN),
        },
        (Project::New, Role::Architect) => Row {
            title: "Write the first architecture from the product spec",
            intent: "Read the approved product plan and write this new project's first architecture: an overview with a diagram and its structure.",
            summary: "The Architect writes how your new project will be put together, from your approved product plan, with diagrams.",
            documents: &["overview.md", "structure.md"],
            question: None,
        },
        (Project::New, Role::SoftwareDeveloper) => Row {
            title: "Set the workflow and the stack's conventions",
            intent: "Read the approved product plan and set this new project's workflow and the conventions its stack calls for, and write them down.",
            summary: "The Software Developer sets how your new project will be built and worked on: its habits and rules.",
            documents: &["workflow.md", "conventions.md"],
            question: Some(ASKS_ABOUT_THE_WORKFLOW),
        },
        // `catervafication_roles` lists no other row.
        _ => Row {
            title: "Write down what the project holds",
            intent: "Read the project and write down what it holds in your folder.",
            summary: "This role writes down what the project holds in its own folder.",
            documents: &[],
            question: None,
        },
    }
}

/// The wire of the epic of a Catervafication, as `request_from_brief` makes a request (the store
/// fills in the id, the kind and the stamps): approved by the owner's Start, or by the owner's
/// acceptance of a new project's product plan. Its budget is `max_cost_usd`, the budget of each of
/// its tasks, times the tasks it is filed with (five for an existing repository, else three).
#[must_use]
pub(crate) fn epic_contract(project: Project, max_cost_usd: f64) -> Value {
    let (title, intent, tasks) = match project {
        Project::Existing => (
            "Catervafy the repository",
            "The team reads the project and writes down what it knows, each role in its own folder under docs/catervas/, before building anything.",
            5.0,
        ),
        Project::New => (
            "Catervafy the new project",
            "The team reads the approved product plan and writes down what it knows, each role in its own folder under docs/catervas/, before building anything.",
            3.0,
        ),
    };
    json!({
        "title": title,
        "intent": intent,
        "summary": intent,
        "scope": {
            "in_scope": ["The documents each role keeps in its own folder under docs/catervas/."],
            "out_of_scope": ["The project's code, and everything outside docs/catervas/."]
        },
        "requirements": [{
            "id": "R1",
            "text": "Each role on the team that has a folder documents the project in it."
        }],
        "exit_criteria": [{
            "id": "C1",
            "text": "Every Catervafication task is accepted or cancelled, and each accepted one's work is on the integration branch.",
            "satisfies": ["R1"],
            "verification": {
                "method": "review",
                "rubric": [
                    "Is every task under the epic accepted or cancelled, and is each accepted one's work on the integration branch?"
                ]
            }
        }],
        "assignee_role": "product_manager",
        "reviewer_role": "human",
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd * tasks },
        "allowed_paths": ["docs/catervas/**"]
    })
}

/// The wire of one role's docs task under `epic` (the store fills in the id, the kind, the parent and
/// the stamps): its documents, each under the role's folder, written from the project by the
/// role's skill, and checked by `reviewer`. `role` is one of `catervafication_roles`'s.
#[must_use]
pub(crate) fn task_contract(
    project: Project,
    role: Role,
    reviewer: Role,
    _epic: &TaskId,
    max_cost_usd: f64,
) -> Value {
    let folder = role_folder(role).unwrap_or("docs/catervas");
    let row = row(project, role);
    let paths: Vec<String> = row
        .documents
        .iter()
        .map(|name| format!("{folder}/{name}"))
        .collect();
    let (in_scope, requirement) = if paths.is_empty() {
        (
            vec!["A first marketing plan, proposed with catervas_propose_marketing_plan from docs/catervas/product/.".to_string()],
            "A first marketing plan is proposed from the documents under docs/catervas/product/, as the skill says.".to_string(),
        )
    } else {
        (
            paths.clone(),
            format!(
                "{} exist under {folder}/, each written from what the project holds, as the skill says.",
                row.documents.join(", ")
            ),
        )
    };
    let mut criteria: Vec<Value> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let mut must = vec!["# "];
            if path.ends_with("architecture/overview.md") {
                must.push("```mermaid");
            }
            json!({
                "id": format!("C{}", index + 1),
                "text": format!("{path} exists and opens with a Markdown heading."),
                "satisfies": ["R1"],
                "verification": { "method": "artifact", "path": path, "must_contain": must }
            })
        })
        .collect();
    let mut rubric = vec![ASKS_WHERE_IT_READ, ASKS_FOR_CONTRADICTIONS];
    rubric.extend(row.question);
    criteria.push(json!({
        "id": format!("C{}", paths.len() + 1),
        "text": "Everything written says only what the project, its history or its owner's words show.",
        "satisfies": ["R1"],
        "verification": { "method": "review", "rubric": rubric }
    }));
    json!({
        "title": row.title,
        "intent": format!("{} {FOLLOW_THE_SKILL}", row.intent),
        "summary": row.summary,
        "scope": {
            "in_scope": in_scope,
            "out_of_scope": [format!(
                "Anything outside {folder}/: the project's code and every other folder stay as they are."
            )]
        },
        "requirements": [{ "id": "R1", "text": requirement }],
        "exit_criteria": criteria,
        "assignee_role": role.to_string(),
        "reviewer_role": reviewer.to_string(),
        "risk": "low",
        "budget": { "max_cost_usd": max_cost_usd },
        "allowed_paths": [format!("{folder}/**")]
    })
}

/// Which Catervafication a task's events say it is the epic of: the project in its
/// `catervafication.started`, and `None` for any other task.
#[must_use]
pub(crate) fn catervafication_of(history: &[CatervasEvent]) -> Option<Project> {
    history.iter().find_map(|event| match &event.body {
        EventBody::CatervaficationStarted(body) => Some(body.project),
        _ => None,
    })
}

/// Why a Catervafication could not be started.
#[derive(Debug)]
pub(crate) enum CatervaficationError {
    /// The team has no active Product Manager to move the epic into its plan.
    NoProductManager,
    /// The epic could not be filed.
    Requests(RequestError),
    /// Its sprint could not be opened or planned.
    Sprint(SprintError),
    /// The move into refining failed.
    Transition(TransitionError),
    /// The log or the board failed.
    Store(StoreError),
    /// A contract file failed.
    Files(FilesError),
}

impl fmt::Display for CatervaficationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProductManager => write!(formatter, "the team has no active Product Manager"),
            Self::Requests(error) => write!(formatter, "the epic could not be filed: {error}"),
            Self::Sprint(error) => write!(formatter, "its sprint failed: {error}"),
            Self::Transition(error) => write!(formatter, "the epic could not move: {error}"),
            Self::Store(error) => write!(formatter, "the store failed: {error}"),
            Self::Files(error) => write!(formatter, "the files failed: {error}"),
        }
    }
}

impl std::error::Error for CatervaficationError {}

impl From<RequestError> for CatervaficationError {
    fn from(error: RequestError) -> Self {
        Self::Requests(error)
    }
}

impl From<SprintError> for CatervaficationError {
    fn from(error: SprintError) -> Self {
        Self::Sprint(error)
    }
}

impl From<TransitionError> for CatervaficationError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

impl From<StoreError> for CatervaficationError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<FilesError> for CatervaficationError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

/// What `start` did: the epic, and the sprint it opened for it, when it opened one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Started {
    /// The epic of the Catervafication.
    pub epic: TaskId,
    /// The sprint `start` opened and planned the epic into, absent when one was open already.
    pub sprint: Option<String>,
}

/// The epic of the Catervafication the log holds, when it holds one.
///
/// # Errors
///
/// What the log refused.
pub(crate) fn started(log: &EventLog) -> Result<Option<TaskId>, StoreError> {
    Ok(log
        .read(&EventQuery {
            kinds: vec![EventKind::CatervaficationStarted],
            limit: Some(1),
            ..EventQuery::default()
        })?
        .into_iter()
        .find_map(|event| event.envelope.ids.task_id))
}

/// Starts the Catervafication of `project` as `by` (`human` for the owner's Start, `catervas` for a
/// new project's): files its epic, opens a sprint for it when none is open and plans the epic into
/// it, asks `draft -> refining` as the first active Product Manager, and, only when that moved,
/// records the owner's approval of the epic (`human.accepted`, with `note` as its message): after
/// the move, because an approval counts only from the last move into `refining`.
///
/// # Errors
///
/// `NoProductManager` before anything is recorded; otherwise whichever part failed.
pub(crate) fn start(
    deps: &ToolDeps,
    team: &Team,
    project: Project,
    by: &str,
    note: Option<String>,
) -> Result<Started, CatervaficationError> {
    let Some(manager) = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
    else {
        return Err(CatervaficationError::NoProductManager);
    };
    let epic = file_catervafication_epic(
        &deps.files,
        &deps.log,
        epic_contract(project, placeholder_budget_usd(&team.rules())),
        by,
        project,
        (deps.clock.now(), &deps.ids),
    )?;
    deps.projections.catch_up()?;
    // The sprint holds the epic alone, so nothing else is planned while it is open (spec 3).
    let sprint = if deps.projections.open_sprint()?.is_none() {
        let sprint = start_sprint(deps, None, by)?;
        plan_sprint(deps, std::slice::from_ref(&epic.id), &PlannedBy::Governor)?;
        Some(sprint.id.as_str().to_string())
    } else {
        None
    };
    let outcome = deps.transitions.request(
        &TransitionRequest {
            task_id: epic.id.clone(),
            to: TaskStatus::Refining,
            actor: TransitionActor::ProductManager,
            agent_id: Some(manager.id.to_string()),
        },
        &TransitionAsk::default(),
        team,
    )?;
    if matches!(outcome, TransitionOutcome::Moved(_)) {
        let ids = EventIds {
            task_id: Some(epic.id.clone()),
            ..deps.ids.clone()
        };
        let accepted = new_event(
            EventBody::HumanAccepted(HumanAcceptedBody {
                subject: HumanAcceptedBodySubject::Contract,
                accepted_by: "human".to_string(),
                message: note,
            }),
            deps.clock.now(),
            ids,
        )
        .map_err(|error| StoreError::InvalidEvent {
            detail: format!("the approval cannot be stamped: {error:?}"),
        })?;
        let appended = deps.log.append(&accepted)?;
        deps.projections.apply(&appended)?;
    }
    Ok(Started {
        epic: epic.id,
        sprint,
    })
}

/// What `file_due` found to do for an epic.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileDue {
    /// These tasks were filed.
    Filed(Vec<TaskId>),
    /// Nothing was filed, and the Marketing Specialist's task waits for the product plan, which is
    /// written and not yet on the integration branch.
    MarketingWaits,
    /// Nothing is due.
    Nothing,
}

/// Files, as Catervas, the task of each role `catervafication_roles` lists that has none under
/// `epic` in any status (a task the owner cancelled is not filed again), each into the epic's
/// sprint. On an existing repository the Marketing Specialist's task waits until the Product
/// Manager's task under the epic is accepted and integrated, so that it reads the approved plan; it
/// is never filed while there is no such task, or when that task is cancelled.
///
/// # Errors
///
/// Whichever part failed; the tasks filed before it stay filed, and are not filed again.
pub(crate) fn file_due(
    deps: &ToolDeps,
    team: &Team,
    board: &[TaskProjection],
    epic: &TaskProjection,
    project: Project,
) -> Result<FileDue, CatervaficationError> {
    let designer_ready = deps.transitions.designer_browser(team) == DesignerBrowser::Ready;
    let mut held: Vec<(Role, &TaskProjection)> = Vec::new();
    for task in board
        .iter()
        .filter(|row| row.parent.as_ref() == Some(&epic.task_id))
    {
        held.push((deps.files.read_contract(&task.task_id)?.assignee_role, task));
    }
    let max_cost_usd = placeholder_budget_usd(&team.rules());
    let mut filed: Vec<TaskId> = Vec::new();
    let mut waits = false;
    for (role, reviewer) in catervafication_roles(team, project, designer_ready) {
        if held.iter().any(|(held, _)| *held == role) {
            continue;
        }
        if role == Role::MarketingSpecialist && project == Project::Existing {
            match held
                .iter()
                .find(|(held, _)| *held == Role::ProductManager)
                .map(|(_, row)| *row)
            {
                // The plan is not filed yet, or the owner cancelled it: nothing to read.
                None => continue,
                Some(plan) if plan.status == TaskStatus::Cancelled => continue,
                Some(plan) if plan.status == TaskStatus::Accepted && !plan.awaiting_integration => {
                }
                Some(_) => {
                    waits = true;
                    continue;
                }
            }
        }
        let contract = file_request(
            &deps.files,
            &deps.log,
            task_contract(project, role, reviewer, &epic.task_id, max_cost_usd),
            "catervas",
            Some(&epic.task_id),
            deps.clock.now(),
            &deps.ids,
            None,
        )?;
        deps.projections.catch_up()?;
        join_epics_sprint(deps, &contract.id)?;
        filed.push(contract.id);
    }
    Ok(if !filed.is_empty() {
        FileDue::Filed(filed)
    } else if waits {
        FileDue::MarketingWaits
    } else {
        FileDue::Nothing
    })
}

#[cfg(test)]
mod tests {
    use catervas_core::contract::{Role, TaskId, validate_contract};
    use catervas_core::governor::readiness::fixtures::a_ready_context;
    use catervas_core::governor::readiness::{ParentState, evaluate_readiness};
    use catervas_core::team::Team;
    use catervas_core::team::fixtures::{a_team_wire, an_agent_wire};
    use catervas_core::team::validate_team;
    use catervas_protocol::event::EventKind;
    use catervas_protocol::event::{CatervasEvent, event_from_value, fixtures::an_event_wire};
    use serde_json::{Value, json};

    use super::{Project, catervafication_of, catervafication_roles, epic_contract, task_contract};
    use crate::tools::fixtures::with_the_suggested_six;

    fn a_team(agents: &[(&str, &str)]) -> Team {
        let mut wire = a_team_wire();
        wire["agents"] = Value::Array(
            agents
                .iter()
                .map(|(id, role)| an_agent_wire(id, role))
                .collect(),
        );
        validate_team(&wire).expect("the fixture is a team")
    }

    fn the_suggested_six() -> Team {
        let mut wire = a_team_wire();
        with_the_suggested_six(&mut wire);
        validate_team(&wire).expect("the fixture is a team")
    }

    fn roles(team: &Team, project: Project, designer_ready: bool) -> Vec<(Role, Role)> {
        catervafication_roles(team, project, designer_ready)
    }

    #[test]
    fn lists_the_roles_that_get_a_task_with_their_reviewers() {
        use Role::{
            Architect, MarketingSpecialist, ProductManager, ScrumMaster, SoftwareDeveloper,
            UiUxDesigner,
        };
        let six = the_suggested_six();
        assert_eq!(
            roles(&six, Project::Existing, true),
            [
                (ProductManager, Architect),
                (Architect, ProductManager),
                (SoftwareDeveloper, Architect),
                (UiUxDesigner, Architect),
                (MarketingSpecialist, ProductManager),
            ]
        );
        assert!(
            roles(&six, Project::Existing, true)
                .iter()
                .all(|(role, _)| *role != ScrumMaster)
        );
        assert_eq!(
            roles(&six, Project::New, true),
            [
                (Architect, ProductManager),
                (SoftwareDeveloper, Architect),
                (MarketingSpecialist, ProductManager),
            ]
        );

        // The Architect paused: one Developer cannot review itself, the Product Manager's work goes
        // to the Scrum Master and the Designer's to the Developer.
        let paused = a_team(&[
            ("pm", "product_manager"),
            ("sm", "scrum_master"),
            ("dev", "software_developer"),
            ("iris", "ui_ux_designer"),
            ("kai", "marketing_specialist"),
        ]);
        assert_eq!(
            roles(&paused, Project::Existing, true),
            [
                (ProductManager, ScrumMaster),
                (UiUxDesigner, SoftwareDeveloper),
                (MarketingSpecialist, ProductManager),
            ]
        );

        // A Product Manager and one Developer: nobody to check anyone's notes.
        let two = a_team(&[("pm", "product_manager"), ("dev", "software_developer")]);
        assert!(roles(&two, Project::Existing, true).is_empty());

        // A Designer whose browser is not ready gets no task; no one else's row changes.
        assert_eq!(
            roles(&six, Project::Existing, false)
                .iter()
                .map(|(role, _)| *role)
                .collect::<Vec<_>>(),
            [
                ProductManager,
                Architect,
                SoftwareDeveloper,
                MarketingSpecialist
            ]
        );

        // With no Product Manager row (no Architect, no Scrum Master, two Developers), the
        // Marketing Specialist's goes too.
        let no_writers = a_team(&[
            ("pm", "product_manager"),
            ("dev-a", "software_developer"),
            ("dev-b", "software_developer"),
            ("kai", "marketing_specialist"),
        ]);
        let rows = roles(&no_writers, Project::Existing, true);
        assert_eq!(rows, [(SoftwareDeveloper, SoftwareDeveloper)]);
        assert!(rows.iter().all(|(role, _)| *role != MarketingSpecialist));
    }

    /// A template as the store files it: with an id and the status its place in the test asks for.
    fn filed(mut wire: Value, id: &str, status: &str, parent: Option<&str>) -> Value {
        wire["id"] = json!(id);
        wire["status"] = json!(status);
        if let Some(parent) = parent {
            wire["kind"] = json!("task");
            wire["parent"] = json!(parent);
        }
        wire
    }

    #[test]
    fn every_template_passes_the_definition_of_ready() {
        let epic_id = TaskId::try_from("CTV-1".to_string()).expect("an id");
        for (project, count) in [(Project::Existing, 5.0), (Project::New, 3.0)] {
            let mut context = a_ready_context();
            context.active_agents_by_role.insert(Role::UiUxDesigner, 1);
            let wire = epic_contract(project, 20.0);
            assert!(
                (wire["budget"]["max_cost_usd"].as_f64().expect("a budget") - 20.0 * count).abs()
                    < 1e-9
            );
            let mut as_epic = filed(wire.clone(), "CTV-1", "draft", None);
            as_epic["kind"] = json!("epic");
            let epic = validate_contract(&as_epic).expect("the epic is a contract");
            context.human_approves = true;
            assert_eq!(
                evaluate_readiness(&epic, &context),
                Ok(()),
                "{project:?} epic"
            );
            context.human_approves = false;

            context.parent = Some(ParentState {
                status: catervas_core::contract::TaskStatus::InProgress,
                allowed_paths: epic.allowed_paths.clone(),
                remaining_budget_usd: epic.budget.max_cost_usd,
                lanes: Vec::new(),
            });
            let six = the_suggested_six();
            for (role, reviewer) in catervafication_roles(&six, project, true) {
                let wire = task_contract(project, role, reviewer, &epic_id, 20.0);
                let task = validate_contract(&filed(wire, "CTV-2", "draft", Some("CTV-1")))
                    .unwrap_or_else(|errors| panic!("{project:?} {role:?}: {errors:?}"));
                let folder = catervas_core::folders::role_folder(role).expect("a folder");
                assert_eq!(task.allowed_paths, [format!("{folder}/**")]);
                assert_eq!(
                    evaluate_readiness(&task, &context),
                    Ok(()),
                    "{project:?} {role:?}"
                );
            }
        }
    }

    type TableRow<'a> = (Project, Role, Role, &'a str, &'a [&'a str], Option<&'a str>);

    fn criterion(wire: &Value, index: usize) -> &Value {
        &wire["exit_criteria"][index]["verification"]
    }

    #[test]
    fn writes_each_template_as_the_table_says() {
        use Role::{
            Architect, MarketingSpecialist, ProductManager, SoftwareDeveloper, UiUxDesigner,
        };
        let epic = TaskId::try_from("CTV-1".to_string()).expect("an id");
        let rubric_two = [
            "Does each document say only what the project, its history or its owner's words show, and name where it read it?",
            "Does any document contradict another, or the code?",
        ];
        let developer_question = "Does workflow.md say at its top whether it records the project's own process or adopts Catervas's recommended workflow, and does the evidence it names support that choice?";
        let marketing_question = "Was a marketing plan proposed from docs/catervas/product/ alone?";
        let rows: [TableRow<'_>; 7] = [
            (
                Project::Existing,
                ProductManager,
                Architect,
                "Write the product plan from the project",
                &["spec.md", "spec.agent.md", "roadmap.md", "roadmap.agent.md"],
                None,
            ),
            (
                Project::Existing,
                Architect,
                ProductManager,
                "Document how the project is put together",
                &["overview.md", "features.md", "jobs.md", "structure.md"],
                None,
            ),
            (
                Project::Existing,
                SoftwareDeveloper,
                Architect,
                "Document how the project is built and worked on",
                &["conventions.md", "workflow.md"],
                Some(developer_question),
            ),
            (
                Project::Existing,
                UiUxDesigner,
                Architect,
                "Write the project's UI inventory",
                &["ui-inventory.md"],
                None,
            ),
            (
                Project::Existing,
                MarketingSpecialist,
                ProductManager,
                "Propose the first marketing plan from the product plan",
                &[],
                Some(marketing_question),
            ),
            (
                Project::New,
                Architect,
                ProductManager,
                "Write the first architecture from the product spec",
                &["overview.md", "structure.md"],
                None,
            ),
            (
                Project::New,
                SoftwareDeveloper,
                Architect,
                "Set the workflow and the stack's conventions",
                &["workflow.md", "conventions.md"],
                Some(developer_question),
            ),
        ];
        for (project, role, reviewer, title, documents, extra) in rows {
            let wire = task_contract(project, role, reviewer, &epic, 20.0);
            let folder = catervas_core::folders::role_folder(role).expect("a folder");
            assert_eq!(wire["title"], title, "{project:?} {role:?}");
            assert!(
                wire["intent"]
                    .as_str()
                    .expect("an intent")
                    .ends_with(" Follow your skill catervafying-the-repository."),
                "{project:?} {role:?}"
            );
            assert!(wire["summary"].as_str().expect("a summary").len() >= 20);
            assert_eq!(wire["assignee_role"], role.to_string());
            assert_eq!(wire["reviewer_role"], reviewer.to_string());
            assert_eq!(wire["risk"], "low");
            assert_eq!(wire["budget"]["max_cost_usd"], 20.0);
            assert_eq!(wire["allowed_paths"], json!([format!("{folder}/**")]));
            assert_eq!(
                wire["scope"]["out_of_scope"],
                json!([format!(
                    "Anything outside {folder}/: the project's code and every other folder stay as they are."
                )])
            );
            let paths: Vec<String> = documents
                .iter()
                .map(|name| format!("{folder}/{name}"))
                .collect();
            if paths.is_empty() {
                assert_eq!(
                    wire["scope"]["in_scope"],
                    json!([
                        "A first marketing plan, proposed with catervas_propose_marketing_plan from docs/catervas/product/."
                    ])
                );
            } else {
                assert_eq!(wire["scope"]["in_scope"], json!(paths));
            }
            let criteria = wire["exit_criteria"].as_array().expect("criteria");
            assert_eq!(criteria.len(), documents.len() + 1, "{project:?} {role:?}");
            for (index, path) in paths.iter().enumerate() {
                let verification = criterion(&wire, index);
                assert_eq!(verification["method"], "artifact");
                assert_eq!(verification["path"], json!(path));
                let mut must = vec!["# "];
                if path.ends_with("architecture/overview.md") {
                    must.push("```mermaid");
                }
                assert_eq!(verification["must_contain"], json!(must), "{path}");
                assert_eq!(wire["exit_criteria"][index]["satisfies"], json!(["R1"]));
            }
            let review = criterion(&wire, documents.len());
            assert_eq!(review["method"], "review");
            let mut rubric: Vec<&str> = rubric_two.to_vec();
            rubric.extend(extra);
            assert_eq!(review["rubric"], json!(rubric), "{project:?} {role:?}");
            if !documents.is_empty() {
                assert_eq!(
                    wire["requirements"][0]["text"],
                    format!(
                        "{} exist under {folder}/, each written from what the project holds, as the skill says.",
                        documents.join(", ")
                    )
                );
            }
        }
        // The new project's Architect writes the first architecture and no plan.
        let wire = task_contract(Project::New, Architect, ProductManager, &epic, 20.0);
        assert!(!wire.to_string().contains("plans/"));
    }

    #[test]
    fn knows_a_catervafication_from_its_history() {
        let history = |project: Option<&str>| -> Vec<CatervasEvent> {
            let mut events =
                vec![event_from_value(&an_event_wire(EventKind::TaskCreated)).expect("valid")];
            if let Some(project) = project {
                let mut wire = an_event_wire(EventKind::RequestTriaged);
                wire["kind"] = json!("catervafication.started");
                wire["body"] = json!({ "project": project });
                events.push(event_from_value(&wire).expect("valid"));
            }
            events
        };
        assert_eq!(
            catervafication_of(&history(Some("new"))),
            Some(Project::New)
        );
        assert_eq!(
            catervafication_of(&history(Some("existing"))),
            Some(Project::Existing)
        );
        assert_eq!(catervafication_of(&history(None)), None);
    }
}
