//! Governed transitions (`docs/SPEC.md` sections 5.2, 5.3, 5.4, 5.7, and 8.4): a request to move a
//! contract is judged by `farik-core`'s governor on facts read from Farik's own store, never on
//! evidence the requester supplies, and the answer is recorded either way.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use farik_core::budget::SessionLedger;
use farik_core::contract::{Role, TaskContract, TaskId, TaskKind, TaskStatus};
use farik_core::governor::done::{CriterionResult, DoneEvidence, RunBy};
use farik_core::governor::escalation::EscalationReason;
use farik_core::governor::gates::{
    AssignmentInput, AssignmentRequester, Blocker, ChildState, DependencyState, Rejection,
    WorkState,
};
use farik_core::governor::readiness::{ParentState, ReadinessContext};
use farik_core::governor::transition::{
    ContractAcceptance, GateFailure, TransitionContext, TransitionDecision, TransitionEffect,
    TransitionRefusal, TransitionRequest, evaluate_transition,
};
use farik_core::governor::transition_table::{GateId, TransitionActor};
use farik_core::team::{AgentStatus, HumanAcceptsContracts, Team};
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    BlockerWire, ContractEvaluatedBody, ContractEvaluatedBodyGate, CriterionRecordedBodyRunBy,
    EscalationRaisedBody, EscalationRaisedBodyReason, EventBody, EventIds, EventKind, FarikEvent,
    GateWire, NoteWrittenBodyKind, RejectionWire, TaskStatusWire, TaskTransitionedBody,
    TaskTransitionedBodyEffectsItem, TransitionActorWire, TransitionRefusedBody,
    TransitionRefusedBodyRefusal, new_event,
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
    ids: EventIds,
    /// Taken for the whole of `request`: reading the context, writing the file, and appending the
    /// event is a read-check-write, and the tools and the orchestrator ask at once. It covers one
    /// process, which is what phase 3 runs.
    requests: Mutex<()>,
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
    /// Why Farik could not run one of the task's criteria for its reviewer, for a reason that is
    /// not the work's: the words of the governor's escalation (5.4).
    pub criterion_unrunnable: Option<String>,
}

/// The governor's answer, which is recorded either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionOutcome {
    /// The task moved, and this is the row it took and what was recorded with it.
    Moved(TransitionDecision),
    /// The task stayed where it was, for these reasons.
    Refused(TransitionRefusal),
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
            requests: Mutex::new(()),
        }
    }

    /// Judges one request on the store's facts and records the answer: a move writes the contract
    /// file and appends `task.transitioned`, then `escalation.raised` when the move escalates; a
    /// refusal appends `transition.refused` with every reason; either is preceded by
    /// `contract.evaluated` when a Definition of Ready or Done was asked. Before the governor is
    /// asked, a Product Manager or Scrum Master must be an active agent of that role, and an
    /// assignment must name active agents of the team.
    ///
    /// # Errors
    ///
    /// Only when the store, the files, git, or the budgets fail, or an event cannot be stamped; a
    /// refusal is an `Ok`.
    pub fn request(
        &self,
        request: &TransitionRequest,
        ask: &TransitionAsk,
        team: &Team,
    ) -> Result<TransitionOutcome, TransitionError> {
        // A poisoned lock is a request that panicked part-way; what it left is in the store, which
        // is read afresh here, so there is nothing to distrust in the lock itself.
        let _held = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        let ask = &tidied(ask);
        let context = self.context(request, ask, team)?;
        let decided = match refused_before_the_governor(request, ask, team, &context.contract) {
            Some(refusal) => Err(refusal),
            None => evaluate_transition(request, &context),
        };
        for (gate, passed, failures) in evaluations(&decided) {
            self.append(
                request,
                ask,
                EventBody::ContractEvaluated(ContractEvaluatedBody {
                    gate,
                    passed,
                    failures,
                }),
            )?;
        }
        match decided {
            Ok(decision) => {
                self.record_move(request, ask, context.contract, &decision)?;
                Ok(TransitionOutcome::Moved(decision))
            }
            Err(refusal) => {
                let body = TransitionRefusedBody {
                    from: status_wire(context.contract.status)?,
                    to: status_wire(request.to)?,
                    actor: actor_wire(request.actor),
                    requested_by: requested_by(request),
                    refusal: refusal_wire(&refusal),
                    details: refusal_details(&refusal),
                };
                self.append(request, ask, EventBody::TransitionRefused(body))?;
                Ok(TransitionOutcome::Refused(refusal))
            }
        }
    }

    /// Writes the moved contract, then records the move and any escalation it raises.
    fn record_move(
        &self,
        request: &TransitionRequest,
        ask: &TransitionAsk,
        mut contract: TaskContract,
        decision: &TransitionDecision,
    ) -> Result<(), TransitionError> {
        contract.status = decision.to;
        if decision
            .effects
            .contains(&TransitionEffect::IncrementIteration)
        {
            contract.iteration = contract.iteration.saturating_add(1);
        }
        if decision.from == TaskStatus::Ready && decision.to == TaskStatus::Assigned {
            contract.assignee.clone_from(&ask.assignee_id);
            contract.reviewer.clone_from(&ask.reviewer_id);
        }
        contract.updated_at = Some(self.clock.now());
        self.files.write_contract(&contract)?;
        let body = TaskTransitionedBody {
            from: status_wire(decision.from)?,
            to: status_wire(decision.to)?,
            actor: actor_wire(request.actor),
            requested_by: requested_by(request),
            gate: gate_wire(decision.row.gate),
            effects: decision.effects.iter().copied().map(effect_wire).collect(),
            assignee: contract.assignee.clone(),
            reviewer: contract.reviewer.clone(),
            iteration: u32::try_from(contract.iteration).unwrap_or(u32::MAX),
            blocker: ask.blocker.as_ref().map(|blocker| BlockerWire {
                description: blocker.description.clone(),
                needed: blocker.needed.clone(),
            }),
            blocker_resolution: ask.blocker_resolution.clone(),
            rejection: ask.rejection.as_ref().map(|rejection| RejectionWire {
                failed_criterion_ids: rejection.failed_criterion_ids.clone(),
                reasons: rejection.reasons.clone(),
            }),
            reason: None,
        };
        self.append(request, ask, EventBody::TaskTransitioned(body))?;
        for effect in &decision.effects {
            if let TransitionEffect::RaiseEscalation(reason) = effect {
                let detail = self.escalation_detail(request, ask, decision)?;
                self.append(
                    request,
                    ask,
                    EventBody::EscalationRaised(EscalationRaisedBody {
                        reason: reason_wire(*reason),
                        detail,
                    }),
                )?;
            }
        }
        Ok(())
    }

    /// The gate that opened an escalation, and the words it was about when there are any: the
    /// rejection's reasons, the blocker's description, which for `blocked -> escalated` (asked by
    /// the governor with an empty ask) is read from the task's last move into `blocked`, or why
    /// Farik could not run a criterion.
    fn escalation_detail(
        &self,
        request: &TransitionRequest,
        ask: &TransitionAsk,
        decision: &TransitionDecision,
    ) -> Result<String, TransitionError> {
        let gate = gate_wire(decision.row.gate).to_string();
        let mut words = ask
            .rejection
            .as_ref()
            .map(|rejection| rejection.reasons.clone())
            .or_else(|| {
                ask.blocker
                    .as_ref()
                    .map(|blocker| blocker.description.clone())
            })
            .or_else(|| ask.criterion_unrunnable.clone());
        if words.is_none() && decision.from == TaskStatus::Blocked {
            let history = self.log.read(&EventQuery {
                task_id: Some(request.task_id.clone()),
                kinds: vec![EventKind::TaskTransitioned],
                ..EventQuery::default()
            })?;
            words =
                last_move_into(&history, TaskStatus::Blocked).and_then(|event| match &event.body {
                    EventBody::TaskTransitioned(body) => body
                        .blocker
                        .as_ref()
                        .map(|blocker| blocker.description.clone()),
                    _ => None,
                });
        }
        Ok(match words.as_deref().map(str::trim) {
            Some(words) if !words.is_empty() => format!("{gate}: {words}"),
            _ => gate,
        })
    }

    /// Appends one event about the request's task, stamped with who asked and the session they
    /// asked from, and projects it.
    fn append(
        &self,
        request: &TransitionRequest,
        ask: &TransitionAsk,
        body: EventBody,
    ) -> Result<(), TransitionError> {
        let ids = EventIds {
            task_id: Some(request.task_id.clone()),
            // An agent's id when an agent asked; the human and the governor are not agents.
            agent_id: match request.actor {
                TransitionActor::Human | TransitionActor::Governor => None,
                _ => request.agent_id.clone(),
            },
            session_id: ask.session_id.clone(),
            ..self.ids.clone()
        };
        let event =
            new_event(body, self.clock.now(), ids).map_err(|error| TransitionError::Event {
                detail: format!("{error:?}"),
            })?;
        let appended = self.log.append(&event)?;
        self.projections.apply(&appended)?;
        Ok(())
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
        let ask = &tidied(ask);
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

        // A dependency is integrated once it is accepted and no longer awaiting integration
        // (5.14): an accepted epic never awaits, its children carrying the branches.
        let dependencies = contract
            .dependencies
            .iter()
            .filter_map(|dependency| {
                board
                    .iter()
                    .find(|row| row.task_id.as_str() == dependency.as_str())
                    .map(|row| DependencyState {
                        task_id: dependency.to_string(),
                        status: row.status,
                        integrated: row.status == TaskStatus::Accepted && !row.awaiting_integration,
                    })
            })
            .collect();
        let assignment = assignment(ask, team, &board, day_left, dependencies);

        let (results, completion_note, review_note) = evidence_since_work_began(&history);
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
            assignee_results: results.clone(),
            work,
            blocker: ask.blocker.clone(),
            blocker_resolution: ask.blocker_resolution.clone(),
            blocked_at: last_move_into(&history, TaskStatus::Blocked)
                .map(|event| event.envelope.recorded_at),
            now,
            blocked_limit: Duration::from_secs(hours.saturating_mul(3600)),
            done: DoneEvidence {
                results,
                changed_paths,
                completion_note,
                review_note,
                human_accepted: false,
            },
            rejection: ask.rejection.clone(),
            budget,
            permission_denied: ask.permission_denied,
            criterion_unrunnable: ask.criterion_unrunnable.is_some(),
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
        let base = integration_branch(team, &self.git)?;
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

/// The ask with its agent ids trimmed and a blank one taken as none, so that the check before the
/// governor, the governor, the file, and the log all read the same ids.
fn tidied(ask: &TransitionAsk) -> TransitionAsk {
    let tidy = |id: &Option<String>| {
        id.as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
    };
    TransitionAsk {
        assignee_id: tidy(&ask.assignee_id),
        reviewer_id: tidy(&ask.reviewer_id),
        ..ask.clone()
    }
}

/// The refusals decided here rather than by the governor. A Product Manager or Scrum Master must be
/// an active agent of that role, since the governor cannot tell who holds a team role; and an
/// assignment must name active agents of the team, which `check_assignment` does not ask. The
/// claimed role is checked first.
fn refused_before_the_governor(
    request: &TransitionRequest,
    ask: &TransitionAsk,
    team: &Team,
    contract: &TaskContract,
) -> Option<TransitionRefusal> {
    let role = match request.actor {
        TransitionActor::ProductManager => Role::ProductManager,
        TransitionActor::ScrumMaster => Role::ScrumMaster,
        TransitionActor::Assignee
        | TransitionActor::Reviewer
        | TransitionActor::Governor
        | TransitionActor::Human => return None,
    };
    let holds = request
        .agent_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|id| {
            team.active_agents()
                .any(|agent| agent.id.as_str() == id && Role::from(agent.role) == role)
        });
    if !holds {
        return Some(TransitionRefusal::NotTheNamedAgent {
            actor: request.actor,
            named: None,
            asked: request.agent_id.clone(),
        });
    }
    if contract.status != TaskStatus::Ready || request.to != TaskStatus::Assigned {
        return None;
    }
    // An epic on a team with no active Scrum Master is reviewed by the human (5.16 item 4), who
    // has no agent id.
    let human_reviews = contract.kind == TaskKind::Epic && !team.has_active(Role::ScrumMaster);
    let mut details = Vec::new();
    for (what, id) in [
        ("assignee", &ask.assignee_id),
        ("reviewer", &ask.reviewer_id),
    ] {
        match id.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
            None if what == "reviewer" && human_reviews => {}
            None => details.push(format!(
                "the request names no {what}, and an assignment names both agents"
            )),
            Some(id) => match team.agents.iter().find(|agent| agent.id.as_str() == id) {
                None => details.push(format!(
                    "{id} is not an agent of this team, and the {what} must be one"
                )),
                Some(agent) if agent.status != AgentStatus::Active => details.push(format!(
                    "{id} is {}, and the {what} must be an active agent",
                    agent.status
                )),
                Some(_) => {}
            },
        }
    }
    (!details.is_empty()).then(|| TransitionRefusal::GateFailed {
        failures: vec![GateFailure {
            gate: GateId::Assignment,
            details,
        }],
    })
}

/// The Definition of Ready or Done evaluations a decision holds: the decided row's gate when it
/// was one, passed, and each tried row's that was one, failed. No move has two rows of those gates
/// for one actor, so a row that failed before another opened is never one of them.
fn evaluations(
    decided: &Result<TransitionDecision, TransitionRefusal>,
) -> Vec<(ContractEvaluatedBodyGate, bool, Vec<String>)> {
    match decided {
        Ok(decision) => evaluated_gate(decision.row.gate)
            .map(|gate| (gate, true, Vec::new()))
            .into_iter()
            .collect(),
        Err(TransitionRefusal::GateFailed { failures }) => failures
            .iter()
            .filter_map(|failure| {
                evaluated_gate(failure.gate).map(|gate| (gate, false, failure.details.clone()))
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn evaluated_gate(gate: GateId) -> Option<ContractEvaluatedBodyGate> {
    match gate {
        GateId::DefinitionOfReady => Some(ContractEvaluatedBodyGate::DefinitionOfReady),
        GateId::DefinitionOfDone => Some(ContractEvaluatedBodyGate::DefinitionOfDone),
        _ => None,
    }
}

/// Who asked, as the log names them: the agent's id, or the actor's own name when no agent asked.
fn requested_by(request: &TransitionRequest) -> String {
    if matches!(
        request.actor,
        TransitionActor::Human | TransitionActor::Governor
    ) {
        return actor_wire(request.actor).to_string();
    }
    request
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map_or_else(|| actor_wire(request.actor).to_string(), str::to_string)
}

/// Every reason a refusal gives: each gate's words, or for the refusals that are not a gate's, one
/// sentence of the refusal's own.
pub(crate) fn refusal_details(refusal: &TransitionRefusal) -> Vec<String> {
    match refusal {
        TransitionRefusal::GateFailed { failures } => failures
            .iter()
            .flat_map(|failure| failure.details.iter().cloned())
            .collect(),
        TransitionRefusal::WrongTask { asked, contract } => vec![format!(
            "wrong task: asked {}, and the contract is {}",
            asked.as_str(),
            contract.as_str()
        )],
        TransitionRefusal::NoSuchTransition { from, to } => {
            vec![format!("no such transition: from {from} to {to}")]
        }
        TransitionRefusal::ActorNotAllowed { actor, allowed } => vec![format!(
            "actor not allowed: {} asked, and only {} may",
            actor_wire(*actor),
            allowed
                .iter()
                .map(|actor| actor_wire(*actor).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )],
        TransitionRefusal::NotTheNamedAgent { named, asked, .. } => {
            let asked = asked.as_deref().unwrap_or("nobody");
            vec![match named {
                Some(named) => {
                    format!("not the named agent: asked {asked}, and the contract names {named}")
                }
                None => format!("not the named agent: asked {asked}"),
            }]
        }
    }
}

pub(crate) fn refusal_wire(refusal: &TransitionRefusal) -> TransitionRefusedBodyRefusal {
    match refusal {
        TransitionRefusal::WrongTask { .. } => TransitionRefusedBodyRefusal::WrongTask,
        TransitionRefusal::NoSuchTransition { .. } => {
            TransitionRefusedBodyRefusal::NoSuchTransition
        }
        TransitionRefusal::ActorNotAllowed { .. } => TransitionRefusedBodyRefusal::ActorNotAllowed,
        TransitionRefusal::NotTheNamedAgent { .. } => {
            TransitionRefusedBodyRefusal::NotTheNamedAgent
        }
        TransitionRefusal::GateFailed { .. } => TransitionRefusedBodyRefusal::GateFailed,
    }
}

/// A status as the wire spells it. The two lists are one, which a test in `farik-protocol` pins.
fn status_wire(status: TaskStatus) -> Result<TaskStatusWire, TransitionError> {
    TaskStatusWire::from_str(&status.to_string()).map_err(|_| TransitionError::Event {
        detail: format!("the event vocabulary has no status {status}"),
    })
}

pub(crate) fn actor_wire(actor: TransitionActor) -> TransitionActorWire {
    match actor {
        TransitionActor::ProductManager => TransitionActorWire::ProductManager,
        TransitionActor::ScrumMaster => TransitionActorWire::ScrumMaster,
        TransitionActor::Assignee => TransitionActorWire::Assignee,
        TransitionActor::Reviewer => TransitionActorWire::Reviewer,
        TransitionActor::Governor => TransitionActorWire::Governor,
        TransitionActor::Human => TransitionActorWire::Human,
    }
}

fn gate_wire(gate: GateId) -> GateWire {
    match gate {
        GateId::None => GateWire::None,
        GateId::Triaged => GateWire::Triaged,
        GateId::DefinitionOfReady => GateWire::DefinitionOfReady,
        GateId::ReadinessExhausted => GateWire::ReadinessExhausted,
        GateId::ContractRequiresHuman => GateWire::ContractRequiresHuman,
        GateId::Assignment => GateWire::Assignment,
        GateId::CriteriaRecorded => GateWire::CriteriaRecorded,
        GateId::BlockerWritten => GateWire::BlockerWritten,
        GateId::BlockerResolved => GateWire::BlockerResolved,
        GateId::BlockedAge => GateWire::BlockedAge,
        GateId::DefinitionOfDone => GateWire::DefinitionOfDone,
        GateId::RejectionReasons => GateWire::RejectionReasons,
        GateId::IterationBelowLimit => GateWire::IterationBelowLimit,
        GateId::IterationLimitReached => GateWire::IterationLimitReached,
        GateId::GovernorEscalation => GateWire::GovernorEscalation,
    }
}

/// An effect's name; an escalation's reason is on the `escalation.raised` that follows.
fn effect_wire(effect: TransitionEffect) -> TaskTransitionedBodyEffectsItem {
    match effect {
        TransitionEffect::IncrementIteration => TaskTransitionedBodyEffectsItem::IncrementIteration,
        TransitionEffect::RaiseEscalation(_) => TaskTransitionedBodyEffectsItem::RaiseEscalation,
        TransitionEffect::ResetBlocker => TaskTransitionedBodyEffectsItem::ResetBlocker,
        TransitionEffect::StampBlockedAt => TaskTransitionedBodyEffectsItem::StampBlockedAt,
    }
}

fn reason_wire(reason: EscalationReason) -> EscalationRaisedBodyReason {
    match reason {
        EscalationReason::Budget => EscalationRaisedBodyReason::Budget,
        EscalationReason::Sessions => EscalationRaisedBodyReason::Sessions,
        EscalationReason::Iterations => EscalationRaisedBodyReason::Iterations,
        EscalationReason::BlockerAge => EscalationRaisedBodyReason::BlockerAge,
        EscalationReason::Permission => EscalationRaisedBodyReason::Permission,
        EscalationReason::RiskGate => EscalationRaisedBodyReason::RiskGate,
        EscalationReason::Approval => EscalationRaisedBodyReason::Approval,
        EscalationReason::ReadinessFailures => EscalationRaisedBodyReason::ReadinessFailures,
        EscalationReason::Integration => EscalationRaisedBodyReason::Integration,
        EscalationReason::ExplicitRequest => EscalationRaisedBodyReason::ExplicitRequest,
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

/// The criterion results and notes recorded since the task last entered `in_progress`, so that a
/// rejected iteration's evidence does not pass the next: the latest result per criterion and
/// runner, and the latest completion and review notes.
fn evidence_since_work_began(
    history: &[FarikEvent],
) -> (Vec<CriterionResult>, Option<String>, Option<String>) {
    let since =
        last_move_into(history, TaskStatus::InProgress).map_or(0, |event| event.envelope.seq);
    let mut results: Vec<CriterionResult> = Vec::new();
    let mut completion_note = None;
    let mut review_note = None;
    for event in history.iter().filter(|event| event.envelope.seq > since) {
        match &event.body {
            EventBody::CriterionRecorded(body) => {
                let run_by = match body.run_by {
                    CriterionRecordedBodyRunBy::Assignee => RunBy::Assignee,
                    CriterionRecordedBodyRunBy::Reviewer => RunBy::Reviewer,
                };
                results.retain(|result| {
                    result.criterion_id != body.criterion_id || result.run_by != run_by
                });
                results.push(CriterionResult {
                    criterion_id: body.criterion_id.clone(),
                    passed: body.passed,
                    evidence: body.evidence.clone(),
                    run_by,
                });
            }
            EventBody::NoteWritten(body) => match body.kind {
                NoteWrittenBodyKind::Completion => completion_note = Some(body.text.clone()),
                NoteWrittenBodyKind::Review => review_note = Some(body.text.clone()),
                NoteWrittenBodyKind::Progress => {}
            },
            _ => {}
        }
    }
    (results, completion_note, review_note)
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

/// The branch a task's work is measured against and merges into: the team's
/// `policy.integration_branch`, or the repository's default branch when the team names none.
///
/// # Errors
///
/// `CommandFailed` naming the team's branch when git would not take it for a branch name
/// (`Git::check_branch_name`); what `Git::default_branch` refuses, asked only when the team names
/// no branch.
pub(crate) fn integration_branch(team: &Team, git: &Git) -> Result<String, GitError> {
    match &team.policy.integration_branch {
        Some(branch) => {
            // The team file's word reaches refspecs and options, so git judges it first.
            git.check_branch_name(branch.as_str())?;
            Ok(branch.to_string())
        }
        None => git.default_branch(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::{DateTime, TimeZone, Utc};
    use farik_core::budget::default_session_limits;
    use farik_core::contract::fixtures::a_contract_wire;
    use farik_core::contract::{Role, TaskStatus, validate_contract};
    use farik_core::governor::gates::{Blocker, DependencyState, Rejection};
    use farik_core::governor::transition::TransitionRefusal;
    use farik_core::governor::transition::TransitionRequest;
    use farik_core::governor::transition_table::GateId;
    use farik_core::governor::transition_table::TransitionActor;
    use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
    use farik_core::team::{Team, validate_team};
    use farik_protocol::clock::Clock;
    use farik_protocol::event::{
        ContractEvaluatedBodyGate, EscalationRaisedBodyReason, EventBody, EventIds, EventKind,
        FarikEvent, GateWire, NewEvent, TaskStatusWire, TaskTransitionedBodyEffectsItem,
        TransitionRefusedBodyRefusal, event_from_value,
    };
    use farik_store::EventQuery;
    use farik_store::files::ProjectFiles;
    use farik_store::git::fixtures::{TempRepo, git_in};
    use farik_store::{EventLog, IN_MEMORY, Projections, open_event_log, open_projections};
    use serde_json::{Value, json};

    use super::{TransitionAsk, TransitionOutcome, Transitions};

    /// A clock a test moves by hand, for a block that has to age.
    struct MovableClock(std::sync::Mutex<DateTime<Utc>>);

    impl Clock for MovableClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().expect("no test panics holding the clock")
        }
    }

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
        clock: Arc<MovableClock>,
    }

    impl Project {
        fn new(name: &str, team: Team, now: DateTime<Utc>) -> Self {
            let repo = TempRepo::new(name);
            let files = Arc::new(ProjectFiles::open(repo.path.clone()));
            files.init(&team).expect(".farik/ is made");
            let log = Arc::new(open_event_log(Path::new(IN_MEMORY), now).expect("the log opens"));
            let projections =
                Arc::new(open_projections(Arc::clone(&log)).expect("the projections open"));
            let clock = Arc::new(MovableClock(std::sync::Mutex::new(now)));
            let transitions = Transitions::new(
                Arc::clone(&log),
                Arc::clone(&projections),
                Arc::clone(&files),
                repo.adapter(),
                Arc::clone(&clock) as Arc<dyn Clock + Send + Sync>,
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
                clock,
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
            self.append_wire(&json!({
                "seq": 1,
                "recorded_at": recorded_at.to_rfc3339(),
                "team_id": "farik",
                "project_id": "farik",
                "task_id": task,
                "kind": kind,
                "body": body,
            }))
        }

        /// A `cost.recorded` of `usd` dollars against `task`, today.
        fn spent(&self, task: &str, usd: f64) {
            self.append_wire(&json!({
                "seq": 1,
                "recorded_at": at(10).to_rfc3339(),
                "team_id": "farik",
                "project_id": "farik",
                "task_id": task,
                "agent_id": "dev-a",
                "session_id": "s-1",
                "kind": "cost.recorded",
                "body": {
                    "purpose": "implement",
                    "model_id": "claude-sonnet-4-5",
                    "usage": {
                        "input_tokens": 1000,
                        "output_tokens": 100,
                        "cache_read_tokens": 0,
                        "cache_write_tokens": 0
                    },
                    "cost_usd": usd
                },
            }));
        }

        fn append_wire(&self, wire: &Value) -> FarikEvent {
            let event = event_from_value(wire).expect("the fixture is schema-valid");
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

        /// Asks for a move as this project's team.
        fn ask(&self, request: &TransitionRequest, ask: &TransitionAsk) -> TransitionOutcome {
            self.transitions
                .request(request, ask, &self.team)
                .expect("the request is judged")
        }

        /// Every event the log holds about `task`, of these kinds, oldest first.
        fn events(&self, task: &str, kinds: &[EventKind]) -> Vec<FarikEvent> {
            self.log
                .read(&EventQuery {
                    task_id: Some(task.parse().expect("a task id")),
                    kinds: kinds.to_vec(),
                    ..EventQuery::default()
                })
                .expect("the log reads")
        }

        /// The contract file as a value, to compare before and after.
        fn file_value(&self, task: &str) -> Value {
            serde_json::to_value(
                self.files
                    .read_contract(&task.parse().expect("a task id"))
                    .expect("the file reads"),
            )
            .expect("a contract serialises")
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

        // A failed Definition of Done is not a readiness failure.
        project.record(
            "FRK-1",
            "contract.evaluated",
            &json!({ "gate": "definition_of_done", "passed": false, "failures": [] }),
            at(10),
        );
        let request = a_request("FRK-1", TaskStatus::Ready, TransitionActor::Governor, None);
        assert_eq!(
            project
                .context(&request, &TransitionAsk::default())
                .readiness_failed_attempts,
            1
        );

        // A re-triage starts refining over, as a move into refining does.
        project.record(
            "FRK-1",
            "request.triaged",
            &json!({ "size": "large", "reason": "Two deliverables.", "triaged_by": "maya" }),
            at(11),
        );
        project.evaluated("FRK-1", false);
        assert_eq!(
            project
                .context(&request, &TransitionAsk::default())
                .readiness_failed_attempts,
            1
        );
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

        std::fs::write(worktree.join("scratch.txt"), "not committed\n").expect("written");
        let dirty = project.context(
            &a_request(
                "FRK-1",
                TaskStatus::Verifying,
                TransitionActor::Assignee,
                Some("dev-a"),
            ),
            &TransitionAsk::default(),
        );
        assert!(!dirty.work.worktree_clean);

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

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_epics_remaining_budget_and_the_days_remainder() {
        let project = Project::new("epic-budget", a_team(|_| {}), at(12));
        project.file("FRK-1", |wire| {
            wire["kind"] = json!("epic");
            wire["budget"]["max_cost_usd"] = json!(12);
            wire["allowed_paths"] = json!(["src/**", "docs/**"]);
        });
        project.created_under("FRK-1", "in_progress", "epic", None);
        project.spent("FRK-1", 2.5);
        for (task, status, max) in [
            ("FRK-2", "refining", 1),
            ("FRK-3", "ready", 3),
            ("FRK-4", "cancelled", 4),
        ] {
            project.file(task, |wire| {
                wire["parent"] = json!("FRK-1");
                wire["budget"]["max_cost_usd"] = json!(max);
            });
            project.created_under(task, status, "task", Some("FRK-1"));
        }
        let request = a_request("FRK-2", TaskStatus::Ready, TransitionActor::Governor, None);
        let context = project.context(&request, &TransitionAsk::default());
        let parent = context.readiness.parent.expect("the epic");
        // Twelve, less the epic's own 2.50, less the live sibling's 3; not the cancelled one's
        // 4, nor the child's own 1.
        assert!(
            (parent.remaining_budget_usd - 6.5).abs() < 1e-9,
            "{}",
            parent.remaining_budget_usd
        );
        assert_eq!(parent.status, TaskStatus::InProgress);
        assert_eq!(
            parent.allowed_paths,
            vec!["src/**".to_string(), "docs/**".to_string()]
        );
        assert!((context.readiness.remaining_sprint_budget_usd - 17.5).abs() < 1e-9);

        // A day spent past its budget leaves nothing, not less than nothing.
        let poor = a_team(|wire| wire["budgets"]["daily_usd"] = json!(1));
        let context = project
            .transitions
            .context(&request, &TransitionAsk::default(), &poor)
            .expect("the context reads");
        assert!(context.readiness.remaining_sprint_budget_usd.abs() < 1e-12);
        assert!(context.readiness.remaining_sprint_budget_usd >= 0.0);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_rest_of_the_context_from_the_store_and_the_team() {
        let team = a_team(|wire| {
            let agents = wire["agents"].as_array_mut().expect("a list of agents");
            agents.push(an_agent_wire("sam", "scrum_master"));
            let mut paused = an_agent_wire("dev-c", "software_developer");
            paused["status"] = json!("paused");
            agents.push(paused);
        });
        let project = Project::new("context-sources", team, at(12));
        project.file("FRK-1", |wire| {
            wire["assignee_role"] = json!("scrum_master");
            wire["dependencies"] = json!(["FRK-2", "FRK-9"]);
        });
        project.created("FRK-1", "ready");
        project.spent("FRK-1", 1.5);
        project.created("FRK-2", "in_progress");
        project.created("FRK-3", "ready");
        project.moved(
            "FRK-3",
            "ready",
            "assigned",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
            at(10),
        );
        project.created("FRK-4", "ready");
        project.moved(
            "FRK-4",
            "assigned",
            "cancelled",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
            at(10),
        );
        let request = a_request(
            "FRK-1",
            TaskStatus::Assigned,
            TransitionActor::ScrumMaster,
            Some("sam"),
        );
        let context = project.context(&request, &assigning("dev-a", "dev-b"));

        assert!(!context.triaged);
        assert_eq!(
            context.readiness.dependency_statuses,
            [("FRK-2".to_string(), TaskStatus::InProgress)]
                .into_iter()
                .collect()
        );
        assert_eq!(
            context.readiness.active_agents_by_role,
            [
                (Role::ProductManager, 1),
                (Role::ScrumMaster, 1),
                (Role::SoftwareDeveloper, 2),
            ]
            .into_iter()
            .collect()
        );
        assert!(context.readiness.requires_judgment_review);
        assert!(!context.acceptance.given);
        assert_eq!(context.blocked_limit, Duration::from_hours(24));
        assert_eq!(
            context.budget.session_limits,
            default_session_limits(Role::ScrumMaster)
        );
        assert!((context.budget.task_max_usd - 5.0).abs() < 1e-9);
        assert!((context.budget.task_spent_usd - 1.5).abs() < 1e-9);
        let assignment = context.assignment.expect("an assignment");
        assert!(assignment.has_active_scrum_master);
        assert_eq!(assignment.assignee_open_tasks, 1);
        assert_eq!(
            assignment.dependencies,
            vec![DependencyState {
                task_id: "FRK-2".to_string(),
                status: TaskStatus::InProgress,
                integrated: false,
            }]
        );

        project.record(
            "FRK-1",
            "request.triaged",
            &json!({ "size": "small", "reason": "One deliverable.", "triaged_by": "maya" }),
            at(11),
        );
        assert!(project.context(&request, &TransitionAsk::default()).triaged);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_integration_branch_git_would_not_name() {
        let project = Project::new("integration-branch-name", a_team(|_| {}), at(12));
        for name in ["main:other", "-f"] {
            let team = a_team(|wire| wire["policy"]["integration_branch"] = json!(name));
            match super::integration_branch(&team, &project.repo.adapter()) {
                Err(farik_store::GitError::CommandFailed { stderr, .. }) => {
                    assert!(stderr.contains(name), "{name}: {stderr}");
                }
                other => panic!("{name}: expected a refusal, got {other:?}"),
            }
        }
        assert_eq!(
            super::integration_branch(
                &a_team(|wire| wire["policy"]["integration_branch"] = json!("release/1.0")),
                &project.repo.adapter()
            ),
            Ok("release/1.0".to_string())
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_a_dependency_as_integrated_once_merged() {
        let project = Project::new("dependency-integrated", a_team(|_| {}), at(12));
        project.file("FRK-2", |wire| wire["dependencies"] = json!(["FRK-1"]));
        project.created("FRK-1", "verifying");
        project.created("FRK-2", "ready");
        project.moved("FRK-1", "verifying", "accepted", &json!({}), at(10));
        let request = a_request(
            "FRK-2",
            TaskStatus::Assigned,
            TransitionActor::ProductManager,
            Some("maya"),
        );
        let dependencies = |project: &Project| {
            project
                .context(&request, &assigning("dev-a", "dev-b"))
                .assignment
                .expect("an assignment")
                .dependencies
        };
        assert_eq!(
            dependencies(&project),
            vec![DependencyState {
                task_id: "FRK-1".to_string(),
                status: TaskStatus::Accepted,
                integrated: false,
            }]
        );

        project.record(
            "FRK-1",
            "task.integrated",
            &json!({ "sha": "abc", "into": "main", "integrated_by": "governor" }),
            at(11),
        );
        assert_eq!(
            dependencies(&project),
            vec![DependencyState {
                task_id: "FRK-1".to_string(),
                status: TaskStatus::Accepted,
                integrated: true,
            }]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_integration_branch_from_the_policy_and_only_for_a_worktree() {
        let project = Project::new("integration-branch", a_team(|_| {}), at(12));
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
        std::fs::write(worktree.join("form.rs"), "fn form() {}\n").expect("written");
        git_in(&worktree, &["add", "-A"]);
        git_in(&worktree, &["commit", "-m", "the form"]);
        git_in(&project.repo.path, &["branch", "develop", "farik/FRK-1"]);
        // With no remote and a detached head, git can name no default branch.
        git_in(&project.repo.path, &["checkout", "--detach"]);
        let verifying = |task| {
            a_request(
                task,
                TaskStatus::Verifying,
                TransitionActor::Assignee,
                Some("dev-a"),
            )
        };

        let develop = a_team(|wire| wire["policy"]["integration_branch"] = json!("develop"));
        let context = project
            .transitions
            .context(&verifying("FRK-1"), &TransitionAsk::default(), &develop)
            .expect("the policy's branch needs no default");
        assert_eq!(context.work.commits, 0);
        assert!(context.done.changed_paths.is_empty());

        // A task with no worktree asks git nothing, so the missing default is no error.
        project.context(&verifying("FRK-2"), &TransitionAsk::default());
        assert!(
            project
                .transitions
                .context(
                    &verifying("FRK-1"),
                    &TransitionAsk::default(),
                    &project.team
                )
                .is_err()
        );
    }

    fn moved_body(event: &FarikEvent) -> &farik_protocol::event::TaskTransitionedBody {
        match &event.body {
            EventBody::TaskTransitioned(body) => body,
            other => panic!("expected a task.transitioned, got {other:?}"),
        }
    }

    fn refused_body(event: &FarikEvent) -> &farik_protocol::event::TransitionRefusedBody {
        match &event.body {
            EventBody::TransitionRefused(body) => body,
            other => panic!("expected a transition.refused, got {other:?}"),
        }
    }

    fn escalation_body(event: &FarikEvent) -> &farik_protocol::event::EscalationRaisedBody {
        match &event.body {
            EventBody::EscalationRaised(body) => body,
            other => panic!("expected an escalation.raised, got {other:?}"),
        }
    }

    fn assignment_failures(outcome: &TransitionOutcome) -> Vec<String> {
        match outcome {
            TransitionOutcome::Refused(TransitionRefusal::GateFailed { failures }) => failures
                .iter()
                .filter(|failure| failure.gate == GateId::Assignment)
                .flat_map(|failure| failure.details.clone())
                .collect(),
            other => panic!("expected an assignment refused at its gate, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn moves_a_ready_task_to_assigned_and_records_it() {
        let project = Project::new("assign", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "ready");
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("maya"),
            ),
            &TransitionAsk {
                session_id: Some("s-7".to_string()),
                ..assigning("dev-a", "dev-b")
            },
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let moves = project.events("FRK-1", &[EventKind::TaskTransitioned]);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].envelope.ids.session_id.as_deref(), Some("s-7"));
        let body = moved_body(&moves[0]);
        assert_eq!(body.requested_by, "maya");
        assert_eq!(body.assignee.as_deref(), Some("dev-a"));
        assert_eq!(body.reviewer.as_deref(), Some("dev-b"));
        assert_eq!(moves[0].envelope.ids.agent_id.as_deref(), Some("maya"));
        let file = project
            .files
            .read_contract(&"FRK-1".parse().expect("a task id"))
            .expect("the file reads");
        assert_eq!(file.status, TaskStatus::Assigned);
        assert_eq!(file.assignee.as_deref(), Some("dev-a"));
        assert_eq!(file.updated_at, Some(at(12)));
        let row = project
            .projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the board reads")
            .expect("on the board");
        assert_eq!(row.status, TaskStatus::Assigned);
        assert_eq!(row.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(row.reviewer_id.as_deref(), Some("dev-b"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_assignment_to_someone_not_on_the_team() {
        let team = a_team(|wire| {
            let mut paused = an_agent_wire("dev-c", "software_developer");
            paused["status"] = json!("paused");
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(paused);
        });
        let project = Project::new("assign-unknown", team, at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "ready");
        let before = project.file_value("FRK-1");
        let request = a_request(
            "FRK-1",
            TaskStatus::Assigned,
            TransitionActor::ProductManager,
            Some("maya"),
        );
        let only_an_assignee = TransitionAsk {
            assignee_id: Some("dev-a".to_string()),
            ..TransitionAsk::default()
        };
        for (ask, named) in [
            (assigning("ghost", "dev-b"), "ghost"),
            (assigning("dev-c", "dev-b"), "dev-c"),
            (only_an_assignee, "reviewer"),
        ] {
            let failures = assignment_failures(&project.ask(&request, &ask));
            assert!(
                failures.iter().any(|failure| failure.contains(named)),
                "{named}: {failures:?}"
            );
        }
        assert_eq!(
            project
                .events("FRK-1", &[EventKind::TransitionRefused])
                .len(),
            3
        );
        assert!(
            project
                .events("FRK-1", &[EventKind::TaskTransitioned])
                .is_empty()
        );
        assert_eq!(project.file_value("FRK-1"), before);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lets_the_human_review_an_epic_only_when_no_scrum_master_is_active() {
        let project = Project::new("assign-epic", a_team(|_| {}), at(12));
        // An epic on a team with no Scrum Master is the Product Manager's, reviewed by the human,
        // who has no agent id (5.16 item 4).
        for task in ["FRK-1", "FRK-2"] {
            project.file(task, |wire| wire["kind"] = json!("epic"));
            project.created_under(task, "ready", "epic", None);
        }
        let epic = TransitionAsk {
            assignee_id: Some("maya".to_string()),
            ..TransitionAsk::default()
        };
        let assigning_epic = |task| {
            a_request(
                task,
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("maya"),
            )
        };
        let outcome = project.ask(&assigning_epic("FRK-1"), &epic);
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let moves = project.events("FRK-1", &[EventKind::TaskTransitioned]);
        assert_eq!(moved_body(&moves[0]).reviewer, None);

        // With a Scrum Master active, the epic's reviewer is an agent, and the ask must name it.
        let with_sam = a_team(|wire| {
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(an_agent_wire("sam", "scrum_master"));
        });
        let outcome = project
            .transitions
            .request(&assigning_epic("FRK-2"), &epic, &with_sam)
            .expect("the request is judged");
        assert_eq!(
            assignment_failures(&outcome),
            vec!["the request names no reviewer, and an assignment names both agents".to_string()]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_blank_reviewer_as_none() {
        let project = Project::new("assign-blank", a_team(|_| {}), at(12));
        project.file("FRK-1", |wire| wire["kind"] = json!("epic"));
        project.created_under("FRK-1", "ready", "epic", None);
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("maya"),
            ),
            &TransitionAsk {
                assignee_id: Some(" maya ".to_string()),
                reviewer_id: Some("  ".to_string()),
                ..TransitionAsk::default()
            },
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let moves = project.events("FRK-1", &[EventKind::TaskTransitioned]);
        assert_eq!(moved_body(&moves[0]).assignee.as_deref(), Some("maya"));
        assert_eq!(moved_body(&moves[0]).reviewer, None);
        let id = "FRK-1".parse().expect("a task id");
        let file = project.files.read_contract(&id).expect("the file reads");
        assert_eq!(file.assignee.as_deref(), Some("maya"));
        assert_eq!(file.reviewer, None);
        let row = project
            .projections
            .task(&id)
            .expect("the board reads")
            .expect("on the board");
        assert_eq!(row.reviewer_id, None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn serialises_two_requests_that_race_for_one_place() {
        let team = a_team(|wire| wire["policy"]["wip_limit_per_agent"] = json!(1));
        let project = Project::new("race", team, at(12));
        for task in ["FRK-1", "FRK-2"] {
            project.file(task, |_| {});
            project.created(task, "ready");
        }
        let outcomes: Vec<TransitionOutcome> = std::thread::scope(|scope| {
            let racers: Vec<_> = ["FRK-1", "FRK-2"]
                .into_iter()
                .map(|task| {
                    let project = &project;
                    scope.spawn(move || {
                        project.ask(
                            &a_request(
                                task,
                                TaskStatus::Assigned,
                                TransitionActor::ProductManager,
                                Some("maya"),
                            ),
                            &assigning("dev-a", "dev-b"),
                        )
                    })
                })
                .collect();
            racers
                .into_iter()
                .map(|racer| racer.join().expect("a racer finishes"))
                .collect()
        });
        let moved = outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TransitionOutcome::Moved(_)))
            .count();
        assert_eq!(moved, 1, "{outcomes:?}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_the_blocker_the_escalation_was_about() {
        let project = Project::new("blocker-escalation", a_team(|_| {}), at(9));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "assigned");
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
            at(9),
        );
        let blocking = TransitionAsk {
            blocker: Some(Blocker {
                description: "no key".to_string(),
                needed: "a key for the payment sandbox".to_string(),
            }),
            ..TransitionAsk::default()
        };
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::Blocked,
                TransitionActor::Assignee,
                Some("dev-a"),
            ),
            &blocking,
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let escalating = a_request(
            "FRK-1",
            TaskStatus::Escalated,
            TransitionActor::Governor,
            None,
        );
        *project.clock.0.lock().expect("the clock") = at(9) + chrono::Duration::hours(23);
        let outcome = project.ask(&escalating, &TransitionAsk::default());
        assert!(
            matches!(outcome, TransitionOutcome::Refused(_)),
            "a day's limit is not reached in 23 hours: {outcome:?}"
        );
        *project.clock.0.lock().expect("the clock") = at(9) + chrono::Duration::hours(25);
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::Escalated,
                TransitionActor::Governor,
                None,
            ),
            &TransitionAsk::default(),
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );

        let moves = project.events("FRK-1", &[EventKind::TaskTransitioned]);
        let into_blocked = moved_body(&moves[1]);
        assert_eq!(
            into_blocked
                .blocker
                .as_ref()
                .map(|blocker| blocker.description.as_str()),
            Some("no key")
        );
        let escalations = project.events("FRK-1", &[EventKind::EscalationRaised]);
        let EventBody::EscalationRaised(escalation) = &escalations[0].body else {
            panic!("an escalation.raised");
        };
        assert_eq!(escalation.reason, EscalationRaisedBodyReason::BlockerAge);
        assert_eq!(escalation.detail, "blocked_age: no key");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn escalates_a_task_whose_criterion_farik_could_not_run() {
        let project = Project::new("unrunnable-escalation", a_team(|_| {}), at(9));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "assigned");
        project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b" }),
            at(9),
        );
        let escalating = a_request(
            "FRK-1",
            TaskStatus::Escalated,
            TransitionActor::Governor,
            None,
        );
        let outcome = project.ask(&escalating, &TransitionAsk::default());
        assert!(
            matches!(outcome, TransitionOutcome::Refused(_)),
            "nothing to escalate: {outcome:?}"
        );
        let outcome = project.ask(
            &escalating,
            &TransitionAsk {
                criterion_unrunnable: Some("C1: git could not read the diff".to_string()),
                ..TransitionAsk::default()
            },
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );

        let escalations = project.events("FRK-1", &[EventKind::EscalationRaised]);
        let escalation = escalation_body(&escalations[0]);
        assert_eq!(
            escalation.reason,
            EscalationRaisedBodyReason::ExplicitRequest
        );
        assert_eq!(
            escalation.detail,
            "governor_escalation: C1: git could not read the diff"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_claimed_role_the_agent_does_not_hold() {
        let project = Project::new("claimed-role", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "ready");
        let before = project.file_value("FRK-1");
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("dev-a"),
            ),
            &assigning("dev-a", "dev-b"),
        );
        assert!(
            matches!(
                outcome,
                TransitionOutcome::Refused(TransitionRefusal::NotTheNamedAgent { .. })
            ),
            "{outcome:?}"
        );
        let refusals = project.events("FRK-1", &[EventKind::TransitionRefused]);
        assert_eq!(refusals.len(), 1);
        let body = refused_body(&refusals[0]);
        assert_eq!(body.from, TaskStatusWire::Ready);
        assert_eq!(body.refusal, TransitionRefusedBodyRefusal::NotTheNamedAgent);
        assert_eq!(
            body.details,
            vec!["not the named agent: asked dev-a".to_string()]
        );
        assert_eq!(project.file_value("FRK-1"), before);

        // A Product Manager who is paused holds the role no longer.
        let paused = a_team(|wire| {
            wire["agents"][0]["status"] = json!("paused");
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(an_agent_wire("ada", "product_manager"));
        });
        let outcome = project
            .transitions
            .request(
                &a_request(
                    "FRK-1",
                    TaskStatus::Assigned,
                    TransitionActor::ProductManager,
                    Some("maya"),
                ),
                &assigning("dev-a", "dev-b"),
                &paused,
            )
            .expect("the request is judged");
        assert!(
            matches!(
                outcome,
                TransitionOutcome::Refused(TransitionRefusal::NotTheNamedAgent { .. })
            ),
            "{outcome:?}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_refusal_with_every_reason_its_gate_gave() {
        let project = Project::new("refusal-reasons", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "ready");
        // The Product Manager is neither the contract's assignee role nor its reviewer role, and
        // would review its own work: one gate, several reasons.
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::Assigned,
                TransitionActor::ProductManager,
                Some("maya"),
            ),
            &assigning("maya", "maya"),
        );
        let reasons = assignment_failures(&outcome);
        assert!(reasons.len() >= 2, "{reasons:?}");
        let refusals = project.events("FRK-1", &[EventKind::TransitionRefused]);
        let body = refused_body(&refusals[0]);
        assert_eq!(body.from, TaskStatusWire::Ready);
        assert_eq!(body.to, TaskStatusWire::Assigned);
        assert_eq!(body.refusal, TransitionRefusedBodyRefusal::GateFailed);
        assert_eq!(body.details, reasons);
        assert!(
            body.details
                .iter()
                .any(|detail| detail.contains("cannot review its own work")),
            "{:?}",
            body.details
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_the_readiness_evaluation_before_the_move() {
        let project = Project::new("readiness-recorded", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "refining");
        let outcome = project.ask(
            &a_request("FRK-1", TaskStatus::Ready, TransitionActor::Governor, None),
            &TransitionAsk::default(),
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let recorded = project.events(
            "FRK-1",
            &[EventKind::ContractEvaluated, EventKind::TaskTransitioned],
        );
        assert_eq!(recorded.len(), 2);
        let EventBody::ContractEvaluated(evaluated) = &recorded[0].body else {
            panic!("the evaluation first, got {:?}", recorded[0].body);
        };
        assert_eq!(evaluated.gate, ContractEvaluatedBodyGate::DefinitionOfReady);
        assert!(evaluated.passed);
        assert_eq!(recorded[1].body.kind(), EventKind::TaskTransitioned);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn escalates_on_the_third_readiness_failure() {
        let project = Project::new("readiness-escalation", a_team(|_| {}), at(12));
        // Fifty dollars is past the team's five-dollar task maximum, so the contract fails.
        project.file("FRK-1", |wire| wire["budget"]["max_cost_usd"] = json!(50));
        project.created("FRK-1", "refining");
        project.evaluated("FRK-1", false);
        project.evaluated("FRK-1", false);
        let governor = |to| a_request("FRK-1", to, TransitionActor::Governor, None);
        let outcome = project.ask(&governor(TaskStatus::Ready), &TransitionAsk::default());
        assert!(
            matches!(outcome, TransitionOutcome::Refused(_)),
            "{outcome:?}"
        );
        let outcome = project.ask(&governor(TaskStatus::Escalated), &TransitionAsk::default());
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let recorded = project.events(
            "FRK-1",
            &[EventKind::TaskTransitioned, EventKind::EscalationRaised],
        );
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].body.kind(), EventKind::TaskTransitioned);
        let EventBody::EscalationRaised(escalation) = &recorded[1].body else {
            panic!("the escalation after the move, got {:?}", recorded[1].body);
        };
        assert_eq!(
            escalation.reason,
            EscalationRaisedBodyReason::ReadinessFailures
        );
        assert_eq!(escalation.detail, "readiness_exhausted");
        let row = project
            .projections
            .task(&"FRK-1".parse().expect("a task id"))
            .expect("the board reads")
            .expect("on the board");
        assert_eq!(row.status, TaskStatus::Escalated);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn increments_the_iteration_when_a_rejected_task_returns() {
        let project = Project::new("iteration", a_team(|_| {}), at(12));
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "verifying");
        project.moved(
            "FRK-1",
            "verifying",
            "rejected",
            &json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": 1 }),
            at(10),
        );
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::InProgress,
                TransitionActor::Governor,
                None,
            ),
            &TransitionAsk::default(),
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let moves = project.events("FRK-1", &[EventKind::TaskTransitioned]);
        let body = moved_body(&moves[1]);
        assert_eq!(body.iteration, 2);
        assert_eq!(body.gate, GateWire::IterationBelowLimit);
        assert_eq!(
            body.effects,
            vec![
                TaskTransitionedBodyEffectsItem::IncrementIteration,
                TaskTransitionedBodyEffectsItem::ResetBlocker
            ]
        );
        // A move that is not an assignment keeps the people the task has.
        assert_eq!(body.assignee.as_deref(), Some("dev-a"));
        assert_eq!(body.reviewer.as_deref(), Some("dev-b"));
        assert_eq!(moves[1].envelope.ids.agent_id, None);
        let file = project
            .files
            .read_contract(&"FRK-1".parse().expect("a task id"))
            .expect("the file reads");
        assert_eq!(file.iteration, 2);
        assert_eq!(file.assignee.as_deref(), Some("dev-a"));
        assert_eq!(file.reviewer.as_deref(), Some("dev-b"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_the_resolution_and_the_rejection_a_move_was_judged_on() {
        let project = Project::new("evidence", a_team(|_| {}), at(12));
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": 1 });
        project.file("FRK-1", |_| {});
        project.created("FRK-1", "in_progress");
        project.moved("FRK-1", "in_progress", "blocked", &people, at(10));
        // The human is no agent, whatever id the request carries.
        let outcome = project.ask(
            &a_request(
                "FRK-1",
                TaskStatus::InProgress,
                TransitionActor::Human,
                Some("maya"),
            ),
            &TransitionAsk {
                blocker_resolution: Some("the key arrived".to_string()),
                ..TransitionAsk::default()
            },
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let moves = project.events("FRK-1", &[EventKind::TaskTransitioned]);
        let body = moved_body(&moves[1]);
        assert_eq!(body.blocker_resolution.as_deref(), Some("the key arrived"));
        assert_eq!(body.gate, GateWire::BlockerResolved);
        assert_eq!(body.requested_by, "human");
        assert_eq!(moves[1].envelope.ids.agent_id, None);

        project.file("FRK-2", |wire| wire["budget"]["max_iterations"] = json!(1));
        project.created("FRK-2", "in_progress");
        project.moved("FRK-2", "in_progress", "verifying", &people, at(10));
        let rejection = TransitionAsk {
            rejection: Some(Rejection {
                failed_criterion_ids: vec!["C1".to_string()],
                reasons: "the form has no labels".to_string(),
            }),
            ..TransitionAsk::default()
        };
        let outcome = project.ask(
            &a_request(
                "FRK-2",
                TaskStatus::Rejected,
                TransitionActor::Reviewer,
                Some("dev-b"),
            ),
            &rejection,
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let outcome = project.ask(
            &a_request(
                "FRK-2",
                TaskStatus::Escalated,
                TransitionActor::Governor,
                None,
            ),
            &rejection,
        );
        assert!(
            matches!(outcome, TransitionOutcome::Moved(_)),
            "{outcome:?}"
        );
        let moves = project.events("FRK-2", &[EventKind::TaskTransitioned]);
        let recorded = moved_body(&moves[1])
            .rejection
            .as_ref()
            .expect("a rejection");
        assert_eq!(recorded.failed_criterion_ids, vec!["C1".to_string()]);
        assert_eq!(recorded.reasons, "the form has no labels");
        let escalations = project.events("FRK-2", &[EventKind::EscalationRaised]);
        let escalation = escalation_body(&escalations[0]);
        assert_eq!(escalation.reason, EscalationRaisedBodyReason::Iterations);
        assert_eq!(
            escalation.detail,
            "iteration_limit_reached: the form has no labels"
        );
    }
}
