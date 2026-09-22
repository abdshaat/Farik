//! Governed transitions (`docs/SPEC.md` sections 5.2, 5.3, 5.4, 5.7, and 8.4): a request to move a
//! contract is judged by `farik-core`'s governor on facts read from Farik's own store, never on
//! evidence the requester supplies, and the answer is recorded either way.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use farik_core::budget::SessionLedger;
use farik_core::contract::{Role, TaskContract, TaskId, TaskStatus};
use farik_core::governor::done::DoneEvidence;
use farik_core::governor::gates::{
    AssignmentInput, AssignmentRequester, Blocker, ChildState, DependencyState, Rejection,
    WorkState,
};
use farik_core::governor::readiness::{ParentState, ReadinessContext};
use farik_core::governor::transition::{ContractAcceptance, TransitionContext, TransitionRequest};
use farik_core::team::{HumanAcceptsContracts, Team};
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    ContractEvaluatedBodyGate, EventBody, EventIds, EventKind, FarikEvent, TaskStatusWire,
};
use farik_store::files::{FilesError, ProjectFiles};
use farik_store::{EventLog, EventQuery, Git, GitError, Projections, StoreError, TaskProjection};

use crate::cost::{CostError, budget_state};

/// The governor's door: everything a transition is judged on and recorded in.
pub struct Transitions {
    log: Arc<EventLog>,
    projections: Arc<Projections>,
    files: Arc<ProjectFiles>,
    git: Git,
    clock: Arc<dyn Clock + Send + Sync>,
    #[expect(dead_code, reason = "the events `request` records are stamped with it")]
    ids: EventIds,
}

/// What the requester brings to a transition, and nothing else: every other fact is read from the
/// store, because a requester that supplies its own evidence is not governed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransitionAsk {
    /// The agent an assignment would give the task to.
    pub assignee_id: Option<String>,
    /// The agent an assignment would have review it.
    pub reviewer_id: Option<String>,
    /// What is in the way, for a move into `blocked`.
    pub blocker: Option<Blocker>,
    /// What cleared the blocker, for a move out of `blocked`.
    pub blocker_resolution: Option<String>,
    /// Why the reviewer rejected the work, for a move into `rejected`.
    pub rejection: Option<Rejection>,
    /// Whether a permission was denied on an action the task requires.
    pub permission_denied: bool,
    /// The session the request came from, when it came from one.
    pub session_id: Option<String>,
}

/// Why a transition could not be judged or recorded. A refusal is not one of these: it is an
/// answer, and is recorded like a move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The log or its projections refused, or the log knows nothing of the task.
    Store {
        /// What the store said.
        detail: String,
    },
    /// A contract file could not be read or written.
    Files {
        /// What the files said.
        detail: String,
    },
    /// Git could not say what the task's branch holds.
    Git {
        /// What git said.
        detail: String,
    },
    /// The budgets could not be read.
    Cost(CostError),
    /// An event could not be stamped.
    Event {
        /// Why.
        detail: String,
    },
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store { detail } => write!(formatter, "the store refused: {detail}"),
            Self::Files { detail } => write!(formatter, "the files refused: {detail}"),
            Self::Git { detail } => write!(formatter, "git refused: {detail}"),
            Self::Cost(error) => write!(formatter, "{error}"),
            Self::Event { detail } => write!(formatter, "the event cannot be recorded: {detail}"),
        }
    }
}

impl std::error::Error for TransitionError {}

impl From<StoreError> for TransitionError {
    fn from(error: StoreError) -> Self {
        Self::Store {
            detail: error.to_string(),
        }
    }
}

impl From<FilesError> for TransitionError {
    fn from(error: FilesError) -> Self {
        Self::Files {
            detail: error.to_string(),
        }
    }
}

impl From<GitError> for TransitionError {
    fn from(error: GitError) -> Self {
        Self::Git {
            detail: error.to_string(),
        }
    }
}

impl From<CostError> for TransitionError {
    fn from(error: CostError) -> Self {
        Self::Cost(error)
    }
}

impl Transitions {
    /// The door over one project's log, board, files, and repository. `ids` gives the team and the
    /// project every event is stamped with; its other ids are ignored, since each request names its
    /// own.
    #[must_use]
    pub fn new(
        log: Arc<EventLog>,
        projections: Arc<Projections>,
        files: Arc<ProjectFiles>,
        git: Git,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: EventIds,
    ) -> Transitions {
        Transitions {
            log,
            projections,
            files,
            git,
            clock,
            ids,
        }
    }

    /// Everything the governor asks about this request, read from the store: the contract with its
    /// status, people, and iteration taken from the board (8.4 makes the log the source of truth for
    /// what happened), and only the ask's own words taken from the requester.
    ///
    /// # Errors
    ///
    /// `Store` when the log or the board cannot be read or the log knows nothing of the task or its
    /// epic; `Files` when a contract file cannot be read; `Git` when the task has a worktree git
    /// cannot read; `Cost` when the budgets cannot be read.
    pub fn context(
        &self,
        request: &TransitionRequest,
        ask: &TransitionAsk,
        team: &Team,
    ) -> Result<TransitionContext, TransitionError> {
        let id = &request.task_id;
        let board = self.projections.board()?;
        let row = row_of(&board, id)?;
        let mut contract = self.files.read_contract(id)?;
        contract.status = row.status;
        contract.assignee.clone_from(&row.assignee_id);
        contract.reviewer.clone_from(&row.reviewer_id);
        contract.iteration = row.iteration.into();
        let history = self.log.read(&EventQuery {
            task_id: Some(id.clone()),
            ..EventQuery::default()
        })?;
        let now = self.clock.now();

        let budget = budget_state(
            &self.projections,
            team,
            contract.assignee_role,
            Some(&contract),
            &SessionLedger::default(),
            now,
        )?;
        let day_left = (budget.day_max_usd - budget.day_spent_usd).max(0.0);

        let readiness = ReadinessContext {
            remaining_sprint_budget_usd: day_left,
            dependency_statuses: contract
                .dependencies
                .iter()
                .filter_map(|dependency| {
                    status_on(&board, dependency.as_str())
                        .map(|status| (dependency.to_string(), status))
                })
                .collect(),
            active_agents_by_role: active_agents_by_role(team),
            parent: self.parent_state(&board, &contract)?,
            rules: team.rules(),
            requires_judgment_review: team.has_active(Role::ScrumMaster),
            judgment_review: None,
        };

        let (work, changed_paths) = self.work(id, team)?;

        let dependencies = contract
            .dependencies
            .iter()
            .filter_map(|dependency| {
                status_on(&board, dependency.as_str()).map(|status| DependencyState {
                    task_id: dependency.to_string(),
                    status,
                    integrated: false,
                })
            })
            .collect();
        let assignment = assignment(ask, team, &board, day_left, dependencies);

        let hours = team.policy.blocked_limit_hours.get();
        Ok(TransitionContext {
            triaged: row.triaged,
            children: board
                .iter()
                .filter(|child| child.parent.as_ref() == Some(id))
                .map(|child| ChildState {
                    task_id: child.task_id.to_string(),
                    status: child.status,
                })
                .collect(),
            readiness,
            readiness_failed_attempts: readiness_failed_attempts(&history),
            acceptance: ContractAcceptance {
                required_by_policy: team.policy.human_accepts_contracts
                    == HumanAcceptsContracts::All,
                given: false,
            },
            assignment,
            assignee_results: Vec::new(),
            work,
            blocker: ask.blocker.clone(),
            blocker_resolution: ask.blocker_resolution.clone(),
            blocked_at: last_move_into(&history, TaskStatus::Blocked)
                .map(|event| event.envelope.recorded_at),
            now,
            blocked_limit: Duration::from_secs(hours.saturating_mul(3600)),
            done: DoneEvidence {
                results: Vec::new(),
                changed_paths,
                completion_note: None,
                review_note: None,
                human_accepted: false,
            },
            rejection: ask.rejection.clone(),
            budget,
            permission_denied: ask.permission_denied,
            contract,
        })
    }

    /// What the task's branch holds, read from git only when its worktree
    /// `.farik/local/worktrees/<id>` exists; otherwise no commits, not clean, and no paths, which
    /// refuses `verifying` truthfully. The integration branch is resolved only then, so that no git
    /// error can arise for a task with no worktree.
    fn work(&self, id: &TaskId, team: &Team) -> Result<(WorkState, Vec<String>), TransitionError> {
        let worktree = self
            .files
            .root()
            .join(".farik/local/worktrees")
            .join(id.as_str());
        if !worktree.is_dir() {
            return Ok((WorkState::default(), Vec::new()));
        }
        let base = match &team.policy.integration_branch {
            Some(branch) => branch.to_string(),
            None => self.git.default_branch()?,
        };
        let branch = format!("farik/{}", id.as_str());
        let work = WorkState {
            commits: self.git.commit_count(&base, &branch)?,
            worktree_clean: self.git.is_clean(&worktree)?,
        };
        Ok((work, self.git.changed_paths(&base, &branch)?))
    }

    /// The epic above a task: its status, its paths, and what is left of its budget once its own
    /// spend and the budgets of its other live children are taken out (5.16: "within the epic's
    /// remaining budget"), so that children readied one at a time cannot add up past it.
    fn parent_state(
        &self,
        board: &[TaskProjection],
        child: &TaskContract,
    ) -> Result<Option<ParentState>, TransitionError> {
        let Some(parent) = &child.parent else {
            return Ok(None);
        };
        let parent = TaskId::from_str(parent.as_str()).map_err(|error| TransitionError::Files {
            detail: format!(
                "{} names an epic that is no task id: {error}",
                child.id.as_str()
            ),
        })?;
        let parent = &parent;
        let child = &child.id;
        let row = row_of(board, parent)?;
        let epic = self.files.read_contract(parent)?;
        let mut remaining = epic.budget.max_cost_usd - row.cost_usd;
        for sibling in board.iter().filter(|other| {
            other.parent.as_ref() == Some(parent)
                && other.task_id != *child
                && other.status != TaskStatus::Cancelled
        }) {
            remaining -= self
                .files
                .read_contract(&sibling.task_id)?
                .budget
                .max_cost_usd;
        }
        Ok(Some(ParentState {
            status: row.status,
            allowed_paths: epic.allowed_paths.iter().map(ToString::to_string).collect(),
            remaining_budget_usd: remaining,
        }))
    }
}

fn row_of<'a>(
    board: &'a [TaskProjection],
    id: &TaskId,
) -> Result<&'a TaskProjection, TransitionError> {
    board
        .iter()
        .find(|row| row.task_id == *id)
        .ok_or_else(|| TransitionError::Store {
            detail: format!("the log has no event about {}", id.as_str()),
        })
}

/// The pair an assignment would name, from the ask's ids and the team's roles, when the ask names
/// an assignee. `requested_by` is the governor's to set from the request's actor.
fn assignment(
    ask: &TransitionAsk,
    team: &Team,
    board: &[TaskProjection],
    remaining_sprint_budget_usd: f64,
    dependencies: Vec<DependencyState>,
) -> Option<AssignmentInput> {
    let assignee_id = ask.assignee_id.as_deref()?;
    let (reviewer_id, reviewer_role) = match ask.reviewer_id.as_deref() {
        Some(reviewer) => (reviewer.to_string(), role_in(team, reviewer)),
        // The human reviews an epic the Product Manager broke down (5.16 item 4), and has no agent
        // id.
        None => (String::new(), Role::Human),
    };
    Some(AssignmentInput {
        requested_by: AssignmentRequester::ScrumMaster,
        has_active_scrum_master: team.has_active(Role::ScrumMaster),
        assignee_id: assignee_id.to_string(),
        assignee_role: role_in(team, assignee_id),
        reviewer_id,
        reviewer_role,
        assignee_open_tasks: open_tasks(board, assignee_id),
        wip_limit: u32::try_from(team.policy.wip_limit_per_agent).unwrap_or(u32::MAX),
        remaining_sprint_budget_usd,
        dependencies,
    })
}

/// The status the board gives a task, if the board has it.
fn status_on(board: &[TaskProjection], task: &str) -> Option<TaskStatus> {
    board
        .iter()
        .find(|row| row.task_id.as_str() == task)
        .map(|row| row.status)
}

/// How many agents of each role are active.
fn active_agents_by_role(team: &Team) -> BTreeMap<Role, u32> {
    let mut counts = BTreeMap::new();
    for agent in team.active_agents() {
        *counts.entry(Role::from(agent.role)).or_insert(0) += 1;
    }
    counts
}

/// The role the team gives an agent. An id the team does not know has none of the roles a contract
/// names; the human stands in, and the assignment gate refuses it by role.
fn role_in(team: &Team, agent_id: &str) -> Role {
    team.agents
        .iter()
        .find(|agent| agent.id.as_str() == agent_id)
        .map_or(Role::Human, |agent| Role::from(agent.role))
}

/// The tasks an agent holds that are neither accepted nor cancelled (5.2).
fn open_tasks(board: &[TaskProjection], agent_id: &str) -> u32 {
    let held = board
        .iter()
        .filter(|row| {
            row.assignee_id.as_deref() == Some(agent_id)
                && !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled)
        })
        .count();
    u32::try_from(held).unwrap_or(u32::MAX)
}

/// The failed Definition of Ready evaluations since refining last started over: the later of the
/// task's last move into `refining` and its last triage (a re-triage from `small` to `large` starts
/// refining over), or its first event when there is neither.
fn readiness_failed_attempts(history: &[FarikEvent]) -> u32 {
    let since = history
        .iter()
        .filter(|event| {
            event.body.kind() == EventKind::RequestTriaged
                || is_move_into(event, TaskStatus::Refining)
        })
        .map(|event| event.envelope.seq)
        .max()
        .unwrap_or(0);
    let failed = history
        .iter()
        .filter(|event| event.envelope.seq > since)
        .filter(|event| {
            matches!(
                &event.body,
                EventBody::ContractEvaluated(body)
                    if body.gate == ContractEvaluatedBodyGate::DefinitionOfReady && !body.passed
            )
        })
        .count();
    u32::try_from(failed).unwrap_or(u32::MAX)
}

fn is_move_into(event: &FarikEvent, status: TaskStatus) -> bool {
    matches!(&event.body, EventBody::TaskTransitioned(body) if wire_status(body.to) == Some(status))
}

/// The task's last `task.transitioned` into `status`.
fn last_move_into(history: &[FarikEvent], status: TaskStatus) -> Option<&FarikEvent> {
    history
        .iter()
        .rev()
        .find(|event| is_move_into(event, status))
}

/// A wire status as the contract's own. The two lists are one, which a test in `farik-protocol`
/// pins, so `None` is a log no Farik wrote.
fn wire_status(status: TaskStatusWire) -> Option<TaskStatus> {
    TaskStatus::from_str(&status.to_string()).ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use chrono::{DateTime, TimeZone, Utc};
    use farik_core::contract::fixtures::a_contract_wire;
    use farik_core::contract::{Role, TaskStatus, validate_contract};
    use farik_core::governor::transition::TransitionRequest;
    use farik_core::governor::transition_table::TransitionActor;
    use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
    use farik_core::team::{Team, validate_team};
    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventIds, FarikEvent, NewEvent, event_from_value};
    use farik_store::files::ProjectFiles;
    use farik_store::git::fixtures::{TempRepo, git_in};
    use farik_store::{EventLog, IN_MEMORY, Projections, open_event_log, open_projections};
    use serde_json::{Value, json};

    use super::{TransitionAsk, Transitions};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    /// A team of a Product Manager, `maya`, and two Software Developers, `dev-a` and `dev-b`, with
    /// twenty dollars a day and no Scrum Master; `change` edits the wire before it is read.
    fn a_team(change: impl FnOnce(&mut Value)) -> Team {
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("maya", "product_manager"),
            an_agent_wire("dev-a", "software_developer"),
            an_agent_wire("dev-b", "software_developer"),
        ]);
        change(&mut wire);
        validate_team(&wire).expect("the fixture is a team")
    }

    /// A repository with `.farik/` initialised, a log in memory, and the door over both, its clock
    /// stopped at `now`.
    struct Project {
        repo: TempRepo,
        log: Arc<EventLog>,
        projections: Arc<Projections>,
        files: Arc<ProjectFiles>,
        transitions: Transitions,
        team: Team,
    }

    impl Project {
        fn new(name: &str, team: Team, now: DateTime<Utc>) -> Self {
            let repo = TempRepo::new(name);
            let files = Arc::new(ProjectFiles::open(repo.path.clone()));
            files.init(&team).expect(".farik/ is made");
            let log = Arc::new(open_event_log(Path::new(IN_MEMORY), now).expect("the log opens"));
            let projections =
                Arc::new(open_projections(Arc::clone(&log)).expect("the projections open"));
            let transitions = Transitions::new(
                Arc::clone(&log),
                Arc::clone(&projections),
                Arc::clone(&files),
                repo.adapter(),
                Arc::new(FixedClock::new(now)),
                EventIds {
                    team_id: "farik".to_string(),
                    project_id: "farik".to_string(),
                    ..EventIds::default()
                },
            );
            Self {
                repo,
                log,
                projections,
                files,
                transitions,
                team,
            }
        }

        /// Appends one event about `task` and projects it, as a command does.
        fn record(
            &self,
            task: &str,
            kind: &str,
            body: &Value,
            recorded_at: DateTime<Utc>,
        ) -> FarikEvent {
            let wire = json!({
                "seq": 1,
                "recorded_at": recorded_at.to_rfc3339(),
                "team_id": "farik",
                "project_id": "farik",
                "task_id": task,
                "kind": kind,
                "body": body,
            });
            let event = event_from_value(&wire).expect("the fixture is schema-valid");
            let appended = self
                .log
                .append(&NewEvent {
                    recorded_at: event.envelope.recorded_at,
                    ids: event.envelope.ids,
                    body: event.body,
                })
                .expect("appends");
            self.projections.apply(&appended).expect("projects");
            appended
        }

        /// A `task.created` putting `task` on the board as a task in `status`.
        fn created(&self, task: &str, status: &str) {
            self.created_under(task, status, "task", None);
        }

        fn created_under(&self, task: &str, status: &str, kind: &str, parent: Option<&str>) {
            let mut summary = json!({ "kind": kind, "title": "Add a login page", "status": status, "risk": "low" });
            if let Some(parent) = parent {
                summary["parent"] = json!(parent);
            }
            self.record(
                task,
                "task.created",
                &json!({ "summary": summary, "created_by": "human" }),
                at(9),
            );
        }

        /// A `task.transitioned` of `task` into `to`, with `body` merged over a plain move.
        fn moved(
            &self,
            task: &str,
            from: &str,
            to: &str,
            extra: &Value,
            recorded_at: DateTime<Utc>,
        ) -> FarikEvent {
            let mut body = json!({
                "from": from,
                "to": to,
                "actor": "governor",
                "requested_by": "governor",
                "gate": "none",
                "effects": [],
                "iteration": 0
            });
            for (key, value) in extra.as_object().expect("an object") {
                body[key] = value.clone();
            }
            self.record(task, "task.transitioned", &body, recorded_at)
        }

        /// A `contract.evaluated` of the Definition of Ready.
        fn evaluated(&self, task: &str, passed: bool) {
            self.record(
                task,
                "contract.evaluated",
                &json!({ "gate": "definition_of_ready", "passed": passed, "failures": [] }),
                at(9),
            );
        }

        /// The fixture contract as `task`, a Software Developer's reviewed by another, written to its
        /// file in `draft` with `change` applied.
        fn file(&self, task: &str, change: impl FnOnce(&mut Value)) {
            let mut wire = a_contract_wire();
            wire["id"] = json!(task);
            wire["reviewer_role"] = json!("software_developer");
            change(&mut wire);
            let contract = validate_contract(&wire).expect("the fixture is a contract");
            self.files
                .write_contract(&contract)
                .expect("the file is written");
        }

        fn context(
            &self,
            request: &TransitionRequest,
            ask: &TransitionAsk,
        ) -> farik_core::governor::transition::TransitionContext {
            self.transitions
                .context(request, ask, &self.team)
                .expect("the context reads")
        }
    }

    fn a_request(
        task: &str,
        to: TaskStatus,
        actor: TransitionActor,
        agent: Option<&str>,
    ) -> TransitionRequest {
        TransitionRequest {
            task_id: task.parse().expect("a task id"),
            to,
            actor,
            agent_id: agent.map(str::to_string),
        }
    }

    fn assigning(assignee: &str, reviewer: &str) -> TransitionAsk {
        TransitionAsk {
            assignee_id: Some(assignee.to_string()),
            reviewer_id: Some(reviewer.to_string()),
            ..TransitionAsk::default()
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_status_and_people_from_the_board_not_the_file() {
        let project = Project::new("board-not-file", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "draft");
        project.moved(
            "FRK-1",
            "refining",
            "ready",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": 1 }),
            at(10),
        );
        let context = project.context(
            &a_request(
                "FRK-1",
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("maya"),
            ),
            &TransitionAsk::default(),
        );
        assert_eq!(context.contract.status, TaskStatus::Ready);
        assert_eq!(context.contract.assignee.as_deref(), Some("dev-a"));
        assert_eq!(context.contract.reviewer.as_deref(), Some("dev-b"));
        assert_eq!(context.contract.iteration, 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_readiness_failures_since_the_task_last_entered_refining() {
        let project = Project::new("readiness-count", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "draft");
        project.evaluated("FRK-1", false);
        project.evaluated("FRK-1", false);
        project.moved(
            "FRK-1",
            "escalated",
            "refining",
            &json!({ "actor": "human", "requested_by": "human" }),
            at(10),
        );
        project.evaluated("FRK-1", true);
        project.evaluated("FRK-1", false);
        let context = project.context(
            &a_request("FRK-1", TaskStatus::Ready, TransitionActor::Governor, None),
            &TransitionAsk::default(),
        );
        assert_eq!(context.readiness_failed_attempts, 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_an_assignment_from_the_ask_and_the_team() {
        let project = Project::new("assignment", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "ready");
        project.created("FRK-2", "ready");
        project.moved(
            "FRK-2",
            "ready",
            "assigned",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
            at(10),
        );
        project.created("FRK-3", "ready");
        project.moved(
            "FRK-3",
            "verifying",
            "accepted",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
            at(10),
        );
        let context = project.context(
            &a_request(
                "FRK-1",
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("maya"),
            ),
            &assigning("dev-a", "dev-b"),
        );
        let assignment = context.assignment.expect("an assignment");
        assert_eq!(assignment.assignee_id, "dev-a");
        assert_eq!(assignment.assignee_role, Role::SoftwareDeveloper);
        assert_eq!(assignment.reviewer_id, "dev-b");
        assert_eq!(assignment.reviewer_role, Role::SoftwareDeveloper);
        assert_eq!(assignment.assignee_open_tasks, 1);
        assert_eq!(assignment.wip_limit, 2);
        assert!(!assignment.has_active_scrum_master);
        assert!((assignment.remaining_sprint_budget_usd - 20.0).abs() < 1e-9);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_policy_that_asks_the_human_for_every_contract() {
        let project = Project::new("policy", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "refining");
        let request = a_request("FRK-1", TaskStatus::Ready, TransitionActor::Governor, None);
        let all = a_team(|wire| wire["policy"]["human_accepts_contracts"] = json!("all"));
        let context = project
            .transitions
            .context(&request, &TransitionAsk::default(), &all)
            .expect("the context reads");
        assert!(context.acceptance.required_by_policy);
        assert!(
            !project
                .context(&request, &TransitionAsk::default())
                .acceptance
                .required_by_policy
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_work_from_the_tasks_worktree() {
        let project = Project::new("worktree-work", a_team(|_| {}), at(12));
        for task in ["FRK-1", "FRK-2"] {
            project.file(task, |_| {});
            project.created(task, "in_progress");
        }
        let worktree = project.repo.path.join(".farik/local/worktrees/FRK-1");
        project
            .repo
            .adapter()
            .create_worktree(&worktree, "farik/FRK-1", "main")
            .expect("the worktree is made");
        std::fs::create_dir_all(worktree.join("src/login")).expect("a directory");
        std::fs::write(worktree.join("src/login/form.rs"), "fn form() {}\n").expect("written");
        git_in(&worktree, &["add", "-A"]);
        git_in(&worktree, &["commit", "-m", "the form"]);

        let context = project.context(
            &a_request(
                "FRK-1",
                TaskStatus::Verifying,
                TransitionActor::Assignee,
                Some("dev-a"),
            ),
            &TransitionAsk::default(),
        );
        assert_eq!(context.work.commits, 1);
        assert!(context.work.worktree_clean);
        assert_eq!(
            context.done.changed_paths,
            vec!["src/login/form.rs".to_string()]
        );

        let context = project.context(
            &a_request(
                "FRK-2",
                TaskStatus::Verifying,
                TransitionActor::Assignee,
                Some("dev-a"),
            ),
            &TransitionAsk::default(),
        );
        assert_eq!(context.work.commits, 0);
        assert!(!context.work.worktree_clean);
        assert!(context.done.changed_paths.is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_blocked_time_from_the_last_move_into_blocked() {
        let project = Project::new("blocked-time", a_team(|_| {}), at(15));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "in_progress");
        project.moved("FRK-1", "in_progress", "blocked", &json!({}), at(10));
        project.moved("FRK-1", "blocked", "in_progress", &json!({}), at(11));
        let blocked = project.moved("FRK-1", "in_progress", "blocked", &json!({}), at(12));
        let context = project.context(
            &a_request(
                "FRK-1",
                TaskStatus::Escalated,
                TransitionActor::Governor,
                None,
            ),
            &TransitionAsk::default(),
        );
        assert_eq!(context.blocked_at, Some(blocked.envelope.recorded_at));
        assert_eq!(context.now, at(15));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_children_from_the_board() {
        let project = Project::new("children", a_team(|_| {}), at(12));
        project.file("FRK-1", |wire| wire["kind"] = json!("epic"));
        project.created_under("FRK-1", "in_progress", "epic", None);
        project.created_under("FRK-2", "accepted", "task", Some("FRK-1"));
        project.created_under("FRK-3", "ready", "task", Some("FRK-1"));
        project.created("FRK-4", "ready");
        let context = project.context(
            &a_request(
                "FRK-1",
                TaskStatus::Verifying,
                TransitionActor::Assignee,
                Some("maya"),
            ),
            &TransitionAsk::default(),
        );
        let children: Vec<(String, TaskStatus)> = context
            .children
            .iter()
            .map(|child| (child.task_id.clone(), child.status))
            .collect();
        assert_eq!(
            children,
            vec![
                ("FRK-2".to_string(), TaskStatus::Accepted),
                ("FRK-3".to_string(), TaskStatus::Ready),
            ]
        );
    }
}
