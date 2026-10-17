//! The Catervafication (`docs/SPEC.md` 5.16, ADR 0051): the epic that has the team document the
//! project, the fixed tasks it is filed with, and when it starts.

use std::fmt;

use catervas_core::contract::{Role, TaskId, TaskKind, TaskStatus};
use catervas_core::folders::{DocumentFormat, artifact_marks, role_folder, with_format};
use catervas_core::governor::gates::DesignerBrowser;
use catervas_core::governor::transition::TransitionRequest;
use catervas_core::governor::transition_table::TransitionActor;
use catervas_core::team::{Team, changes_code};
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind};
use catervas_store::files::FilesError;
use catervas_store::product_plan::product_plan_tasks;
use catervas_store::requests::{
    RequestError, file_catervafication_epic, file_request, placeholder_budget_usd,
};
use catervas_store::{EventLog, EventQuery, StoreError, TaskProjection};
use serde_json::{Value, json};

use crate::tools::ToolDeps;
use crate::transitions::{TransitionAsk, TransitionError};

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
/// `{x}` stands for the extension of the team's format.
const ASKS_ABOUT_THE_WORKFLOW: &str = "Does workflow.{x} say at its top whether it records the project's own process or adopts Catervas's recommended workflow, and does the evidence it names support that choice?";

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
    /// The documents' names without their extension, `.agent` stems included.
    documents: &'static [&'static str],
    /// A question the review adds to the two every row asks; `{x}` stands for the extension.
    question: Option<&'static str>,
}

fn row(project: Project, role: Role) -> Row {
    match (project, role) {
        (Project::Existing, Role::ProductManager) => Row {
            title: "Write the product plan from the project",
            intent: "Read the project and write its product plan from what it holds: what the product is, who it is for and what comes next.",
            summary: "The Product Manager writes what your product is and what comes next, as your product plan to approve.",
            documents: &["spec", "spec.agent", "roadmap", "roadmap.agent"],
            question: None,
        },
        (Project::Existing, Role::Architect) => Row {
            title: "Document how the project is put together",
            intent: "Read the code and write down how the project is put together: an overview with a diagram, its features, its scheduled jobs and its structure.",
            summary: "The Architect writes how your project is put together, with diagrams.",
            documents: &["overview", "features", "jobs", "structure"],
            question: None,
        },
        (Project::Existing, Role::SoftwareDeveloper) => Row {
            title: "Document how the project is built and worked on",
            intent: "Read the project's configuration, tests and history and write down how it is built and worked on: its conventions and its workflow.",
            summary: "The Software Developer writes how your project is built and worked on: its habits and rules.",
            documents: &["conventions", "workflow"],
            question: Some(ASKS_ABOUT_THE_WORKFLOW),
        },
        (Project::Existing, Role::UiUxDesigner) => Row {
            title: "Write the project's UI inventory",
            intent: "Read the project's screens and styles and write an inventory of its user interface.",
            summary: "The UI/UX Designer writes an inventory of your project's screens and how they look.",
            documents: &["ui-inventory"],
            question: None,
        },
        (_, Role::MarketingSpecialist) => Row {
            title: "Propose the first marketing plan from the product plan",
            intent: "Read the approved product plan under docs/catervas/product/ and propose the project's first marketing plan from it.",
            summary: "The Marketing Specialist writes your marketing plan, after you approve the product plan.",
            documents: &[],
            question: None,
        },
        (Project::New, Role::Architect) => Row {
            title: "Write the first architecture from the product spec",
            intent: "Read the approved product plan and write this new project's first architecture: an overview with a diagram and its structure.",
            summary: "The Architect writes how your new project will be put together, from your approved product plan, with diagrams.",
            documents: &["overview", "structure"],
            question: None,
        },
        (Project::New, Role::SoftwareDeveloper) => Row {
            title: "Set the workflow and the stack's conventions",
            intent: "Read the approved product plan and set this new project's workflow and the conventions its stack calls for, and write them down.",
            summary: "The Software Developer sets how your new project will be built and worked on: its habits and rules.",
            documents: &["workflow", "conventions"],
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
/// its tasks, times the tasks it is filed with (five for an existing repository, else three). Its
/// one criterion is the product plan's spec in the team's `format`, which Catervas reads itself on
/// the integration branch (ADR 0061).
#[must_use]
pub(crate) fn epic_contract(project: Project, max_cost_usd: f64, format: DocumentFormat) -> Value {
    let (title, intent, summary, tasks) = match project {
        Project::Existing => (
            "Catervafy the repository",
            "The team reads the project and writes down what it knows, each role in its own folder under docs/catervas/, before building anything.",
            "Your team reads your project and writes down what it learns before building anything. You can read all of it in Files.",
            5.0,
        ),
        Project::New => (
            "Catervafy the new project",
            "The team reads the approved product plan and writes down what it knows, each role in its own folder under docs/catervas/, before building anything.",
            "Your team reads your approved product plan and writes down how it will build the project, before building anything. You can read all of it in Files.",
            3.0,
        ),
    };
    let spec = format!("docs/catervas/product/spec.{}", format.extension());
    json!({
        "title": title,
        "intent": intent,
        "summary": summary,
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
            "text": "Every Catervafication task is accepted or cancelled, and each accepted one's work is in your project.",
            "satisfies": ["R1"],
            "verification": {
                "method": "artifact",
                "path": spec,
                "must_contain": artifact_marks(&spec, format)
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
    format: DocumentFormat,
) -> Value {
    let folder = role_folder(role).unwrap_or("docs/catervas");
    let row = row(project, role);
    let extension = format.extension();
    let names: Vec<String> = row
        .documents
        .iter()
        .map(|stem| with_format(stem, format))
        .collect();
    let paths: Vec<String> = names
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
                names.join(", ")
            ),
        )
    };
    let mut criteria: Vec<Value> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            // A heading, and for the architecture's overview a diagram.
            let kind = match format {
                DocumentFormat::Markdown => "a Markdown heading",
                DocumentFormat::Html => "an HTML heading",
            };
            let must = artifact_marks(path, format);
            json!({
                "id": format!("C{}", index + 1),
                "text": format!("{path} exists and holds {kind}."),
                "satisfies": ["R1"],
                "verification": { "method": "artifact", "path": path, "must_contain": must }
            })
        })
        .collect();
    if changes_code(role) {
        let question = row
            .question
            .map(|question| question.replace("{x}", extension));
        let mut rubric = vec![ASKS_WHERE_IT_READ, ASKS_FOR_CONTRADICTIONS];
        rubric.extend(question.as_deref());
        criteria.push(json!({
            "id": format!("C{}", paths.len() + 1),
            "text": "Everything written says only what the project, its history or its owner's words show.",
            "satisfies": ["R1"],
            "verification": { "method": "review", "rubric": rubric }
        }));
    } else if paths.is_empty() {
        // The Marketing Specialist's task has no document: a plan file is the evidence. The glob
        // expands in `sh -c` at the root of the task's worktree, where the plan is committed.
        criteria.push(json!({
            "id": "C1",
            "text": "A marketing plan is written in docs/catervas/marketing/plans/",
            "satisfies": ["R1"],
            "verification": {
                "method": "command",
                "command": "ls docs/catervas/marketing/plans/MP-* >/dev/null 2>&1",
                "expect": { "exit_code": 0 }
            }
        }));
    }
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

/// What `start` did: the epic it filed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Started {
    /// The epic of the Catervafication.
    pub epic: TaskId,
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
/// new project's): files its epic and asks `draft -> refining` as the first active Product Manager. No epic waits for the
/// owner's approval (ADR 0061), so none is recorded.
///
/// # Errors
///
/// `NoProductManager` before anything is recorded; otherwise whichever part failed.
pub(crate) fn start(
    deps: &ToolDeps,
    team: &Team,
    project: Project,
    by: &str,
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
        epic_contract(
            project,
            placeholder_budget_usd(&team.rules()),
            team.document_format(),
        ),
        by,
        project,
        (deps.clock.now(), &deps.ids),
    )?;
    deps.projections.catch_up()?;
    deps.transitions.request(
        &TransitionRequest {
            task_id: epic.id.clone(),
            to: TaskStatus::Refining,
            actor: TransitionActor::ProductManager,
            agent_id: Some(manager.id.to_string()),
        },
        &TransitionAsk::default(),
        team,
    )?;
    Ok(Started { epic: epic.id })
}

/// What `file_due` found to do for an epic.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileDue {
    /// These tasks were filed.
    Filed(Vec<TaskId>),
    /// Nothing is due.
    Nothing,
}

/// Files, as Catervas, the task of each role `catervafication_roles` lists that has none under
/// `epic` in any status (a task the owner cancelled is not filed again), each into the epic's
/// sprint. The Marketing Specialist's task is filed with the others and depends on the Product
/// Manager's task under the epic (on an existing repository) or on the new project's product plan
/// task, so that it starts from that task's branch once it is accepted (ADR 0061); it is not filed
/// when that task is cancelled.
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
    let mut filed_by: Vec<(Role, TaskId)> = Vec::new();
    for (role, reviewer) in catervafication_roles(team, project, designer_ready) {
        if held.iter().any(|(held, _)| *held == role) {
            continue;
        }
        let mut dependencies: Vec<TaskId> = Vec::new();
        if role == Role::MarketingSpecialist {
            match project {
                Project::Existing => {
                    let held_plan = held
                        .iter()
                        .find(|(held, _)| *held == Role::ProductManager)
                        .map(|(_, row)| *row);
                    match held_plan {
                        // The owner cancelled it: nothing to read.
                        Some(plan) if plan.status == TaskStatus::Cancelled => continue,
                        Some(plan) => dependencies.push(plan.task_id.clone()),
                        None => match filed_by
                            .iter()
                            .find(|(filed, _)| *filed == Role::ProductManager)
                        {
                            Some((_, id)) => dependencies.push(id.clone()),
                            None => continue,
                        },
                    }
                }
                Project::New => {
                    if let Some((plan, _)) = product_plan_tasks(&deps.log)?.pop() {
                        dependencies.push(plan);
                    }
                }
            }
        }
        let mut wire = task_contract(
            project,
            role,
            reviewer,
            &epic.task_id,
            max_cost_usd,
            team.document_format(),
        );
        if !dependencies.is_empty() {
            wire["dependencies"] = json!(
                dependencies
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
            );
        }
        let contract = file_request(
            &deps.files,
            &deps.log,
            wire,
            "catervas",
            Some(&epic.task_id),
            deps.clock.now(),
            &deps.ids,
            None,
        )?;
        deps.projections.catch_up()?;
        filed_by.push((role, contract.id.clone()));
        filed.push(contract.id);
    }
    Ok(if filed.is_empty() {
        FileDue::Nothing
    } else {
        FileDue::Filed(filed)
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

        // A Product Manager and one Developer: the Product Manager's task alone, which it reviews
        // itself in a fresh session; one Developer cannot review its own.
        let two = a_team(&[("pm", "product_manager"), ("dev", "software_developer")]);
        assert_eq!(
            roles(&two, Project::Existing, true),
            [(ProductManager, ProductManager)]
        );

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

        // With no Architect and no Scrum Master the Product Manager reviews its own task, so the
        // Marketing Specialist's, which it reviews, is filed too, beside the two Developers'.
        let no_writers = a_team(&[
            ("pm", "product_manager"),
            ("dev-a", "software_developer"),
            ("dev-b", "software_developer"),
            ("kai", "marketing_specialist"),
        ]);
        let rows = roles(&no_writers, Project::Existing, true);
        assert_eq!(
            rows,
            [
                (ProductManager, ProductManager),
                (SoftwareDeveloper, SoftwareDeveloper),
                (MarketingSpecialist, ProductManager)
            ]
        );
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
            let wire = epic_contract(
                project,
                20.0,
                catervas_core::folders::DocumentFormat::Markdown,
            );
            assert!(
                (wire["budget"]["max_cost_usd"].as_f64().expect("a budget") - 20.0 * count).abs()
                    < 1e-9
            );
            let mut as_epic = filed(wire.clone(), "CTV-1", "draft", None);
            as_epic["kind"] = json!("epic");
            let epic = validate_contract(&as_epic).expect("the epic is a contract");
            assert_eq!(
                evaluate_readiness(&epic, &context),
                Ok(()),
                "{project:?} epic"
            );
            // In a project folder too: its one criterion is an artifact, not a command.
            let mut in_a_folder = context.clone();
            in_a_folder.repos = Some(vec!["api".to_string()]);
            assert_eq!(
                evaluate_readiness(&epic, &in_a_folder),
                Ok(()),
                "{project:?} epic in a project folder"
            );

            context.parent = Some(ParentState {
                status: catervas_core::contract::TaskStatus::InProgress,
                allowed_paths: epic.allowed_paths.clone(),
                remaining_budget_usd: epic.budget.max_cost_usd,
                lanes: Vec::new(),
            });
            let six = the_suggested_six();
            for (role, reviewer) in catervafication_roles(&six, project, true) {
                let wire = task_contract(
                    project,
                    role,
                    reviewer,
                    &epic_id,
                    20.0,
                    catervas_core::folders::DocumentFormat::Markdown,
                );
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

    fn the_epic_is_worded_for_its_owner() {
        // The epic is worded for its owner, as the Offer card is.
        for (project, summary) in [
            (
                Project::Existing,
                "Your team reads your project and writes down what it learns before building anything. You can read all of it in Files.",
            ),
            (
                Project::New,
                "Your team reads your approved product plan and writes down how it will build the project, before building anything. You can read all of it in Files.",
            ),
        ] {
            let wire = epic_contract(
                project,
                20.0,
                catervas_core::folders::DocumentFormat::Markdown,
            );
            assert_eq!(wire["summary"], summary, "{project:?}");
            // Catervas runs the epic's one criterion itself (ADR 0061): the spec is there.
            for (format, spec, heading) in [
                (
                    catervas_core::folders::DocumentFormat::Markdown,
                    "docs/catervas/product/spec.md",
                    "# ",
                ),
                (
                    catervas_core::folders::DocumentFormat::Html,
                    "docs/catervas/product/spec.html",
                    "<h1",
                ),
            ] {
                let wire = epic_contract(project, 20.0, format);
                assert_eq!(
                    wire["exit_criteria"][0]["verification"],
                    json!({ "method": "artifact", "path": spec, "must_contain": [heading] }),
                    "{project:?}"
                );
            }
        }
    }

    #[test]
    fn writes_each_template_as_the_table_says() {
        use Role::{
            Architect, MarketingSpecialist, ProductManager, SoftwareDeveloper, UiUxDesigner,
        };
        let epic = TaskId::try_from("CTV-1".to_string()).expect("an id");
        the_epic_is_worded_for_its_owner();
        let rubric_two = [
            "Does each document say only what the project, its history or its owner's words show, and name where it read it?",
            "Does any document contradict another, or the code?",
        ];
        let developer_question = "Does workflow.md say at its top whether it records the project's own process or adopts Catervas's recommended workflow, and does the evidence it names support that choice?";
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
                None,
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
            let wire = task_contract(
                project,
                role,
                reviewer,
                &epic,
                20.0,
                catervas_core::folders::DocumentFormat::Markdown,
            );
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
            // Only a code role's task is reviewed (ADR 0061); Catervas runs every other criterion.
            let reviewed = catervas_core::team::changes_code(role);
            assert_eq!(
                criteria.len(),
                if documents.is_empty() {
                    1
                } else {
                    documents.len() + usize::from(reviewed)
                },
                "{project:?} {role:?}"
            );
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
            if reviewed {
                let review = criterion(&wire, documents.len());
                assert_eq!(review["method"], "review");
                let mut rubric: Vec<&str> = rubric_two.to_vec();
                rubric.extend(extra);
                assert_eq!(review["rubric"], json!(rubric), "{project:?} {role:?}");
            } else if documents.is_empty() {
                // The Marketing Specialist's task has no document: a plan file is the evidence.
                assert_eq!(
                    wire["exit_criteria"][0],
                    json!({
                        "id": "C1",
                        "text": "A marketing plan is written in docs/catervas/marketing/plans/",
                        "satisfies": ["R1"],
                        "verification": {
                            "method": "command",
                            "command": "ls docs/catervas/marketing/plans/MP-* >/dev/null 2>&1",
                            "expect": { "exit_code": 0 }
                        }
                    })
                );
            }
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
        let wire = task_contract(
            Project::New,
            Architect,
            ProductManager,
            &epic,
            20.0,
            catervas_core::folders::DocumentFormat::Markdown,
        );
        assert!(!wire.to_string().contains("plans/"));
    }

    #[test]
    fn catervafication_files_html_documents() {
        use catervas_core::folders::DocumentFormat;
        let epic = TaskId::try_from("CTV-1".to_string()).expect("an id");
        let wire = task_contract(
            Project::Existing,
            Role::Architect,
            Role::ProductManager,
            &epic,
            20.0,
            DocumentFormat::Html,
        );
        let folder = catervas_core::folders::role_folder(Role::Architect).expect("a folder");
        let paths: Vec<String> = ["overview", "features", "jobs", "structure"]
            .iter()
            .map(|stem| format!("{folder}/{stem}.html"))
            .collect();
        assert_eq!(wire["scope"]["in_scope"], json!(paths));
        for (index, path) in paths.iter().enumerate() {
            let verification = criterion(&wire, index);
            assert_eq!(verification["path"], json!(path));
            let must = if index == 0 {
                json!(["<h1", "<svg"])
            } else {
                json!(["<h1"])
            };
            assert_eq!(verification["must_contain"], must, "{path}");
        }
        assert_eq!(
            wire["requirements"][0]["text"],
            "overview.html, features.html, jobs.html, structure.html exist under docs/catervas/architecture/, each written from what the project holds, as the skill says."
        );
        // The Developer's question names the workflow document as the team writes it.
        let developer = task_contract(
            Project::Existing,
            Role::SoftwareDeveloper,
            Role::Architect,
            &epic,
            20.0,
            DocumentFormat::Html,
        );
        assert!(
            developer
                .to_string()
                .contains("Does workflow.html say at its top"),
            "{developer}"
        );
        assert!(!developer.to_string().contains(".md"), "{developer}");
        // A Markdown team keeps the heading and the Mermaid block.
        let markdown = task_contract(
            Project::Existing,
            Role::Architect,
            Role::ProductManager,
            &epic,
            20.0,
            DocumentFormat::Markdown,
        );
        assert_eq!(
            criterion(&markdown, 0)["must_contain"],
            json!(["# ", "```mermaid"])
        );
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
