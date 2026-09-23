//! The order of work, and what each state on the board asks of the orchestrator. A tick takes the
//! rules in order, one per state, and each rule the tasks in its state by their number; the first
//! that acts ends the tick.

use farik_core::contract::{Role, TaskContract, TaskKind, TaskStatus};
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{Agent, Team};
use farik_store::TaskProjection;

use super::messages::plan_message;
use super::session::{SessionAsk, SessionEnd, run_session};
use super::{OrchestratorDeps, OrchestratorError, TickReport};
use crate::session::{EndReason, SessionPurpose};
use crate::transitions::TransitionAsk;

/// What a tick says when no rule matched.
const NOTHING_TO_DO: &str = "nothing on the board needs doing";

/// One tick: the first rule that acts, or `Idle`.
pub(super) async fn tick(deps: &OrchestratorDeps) -> Result<TickReport, OrchestratorError> {
    let team = deps.tools.files.read_team()?;
    let mut board = deps.tools.projections.board()?;
    board.sort_by_key(|row| task_number(row.task_id.as_str()));
    for row in board.iter().filter(|row| row.status == TaskStatus::Ready) {
        if let Some(report) = ready(deps, &team, &board, row).await? {
            return Ok(report);
        }
    }
    Ok(TickReport::Idle {
        why: NOTHING_TO_DO.to_string(),
    })
}

/// Rule 8: a standalone task that is `ready` gets a plan session of its assigner (the active Scrum
/// Master, else the Product Manager), when an agent of its assignee role has room for it and every
/// dependency is accepted and integrated.
async fn ready(
    deps: &OrchestratorDeps,
    team: &Team,
    board: &[TaskProjection],
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    if row.kind != TaskKind::Task || row.parent.is_some() {
        return Ok(None);
    }
    let Some(assigner) = assigner(team) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let assignees: Vec<String> = team
        .active_agents()
        .filter(|agent| Role::from(agent.role) == contract.assignee_role)
        .filter(|agent| has_room(team, board, agent))
        .map(|agent| agent.id.to_string())
        .collect();
    let Some(first) = assignees.first() else {
        return Ok(None);
    };
    if !dependencies_integrated(deps, team, &contract, assigner, first)? {
        return Ok(None);
    }
    let reviewers: Vec<String> = team
        .active_agents()
        .filter(|agent| Role::from(agent.role) == contract.reviewer_role)
        .map(|agent| agent.id.to_string())
        .collect();
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: assigner,
            contract: &contract,
            purpose: SessionPurpose::Plan,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            initial_prompt: plan_message(&contract, &assignees, &reviewers),
        },
    )
    .await?;
    Ok(Some(acted(row, assigner, "plan", &end)))
}

/// Who assigns: the active Scrum Master, else the active Product Manager (D6).
fn assigner(team: &Team) -> Option<&Agent> {
    let holding = |role: Role| {
        team.active_agents()
            .find(move |agent| Role::from(agent.role) == role)
    };
    holding(Role::ScrumMaster).or_else(|| holding(Role::ProductManager))
}

/// Whether `agent` holds fewer open tasks, neither accepted nor cancelled, than the WIP limit.
fn has_room(team: &Team, board: &[TaskProjection], agent: &Agent) -> bool {
    let held = board
        .iter()
        .filter(|row| row.assignee_id.as_deref() == Some(agent.id.as_str()))
        .filter(|row| !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled))
        .count();
    u64::try_from(held).unwrap_or(u64::MAX)
        < u64::try_from(team.policy.wip_limit_per_agent).unwrap_or(0)
}

/// Whether every dependency of the task is accepted and integrated, as the governor's context for
/// the assignment reads them.
fn dependencies_integrated(
    deps: &OrchestratorDeps,
    team: &Team,
    contract: &TaskContract,
    assigner: &Agent,
    candidate: &str,
) -> Result<bool, OrchestratorError> {
    if contract.dependencies.is_empty() {
        return Ok(true);
    }
    let request = TransitionRequest {
        task_id: contract.id.clone(),
        to: TaskStatus::Assigned,
        actor: if Role::from(assigner.role) == Role::ScrumMaster {
            TransitionActor::ScrumMaster
        } else {
            TransitionActor::ProductManager
        },
        agent_id: Some(assigner.id.to_string()),
    };
    let ask = TransitionAsk {
        assignee_id: Some(candidate.to_string()),
        ..TransitionAsk::default()
    };
    let context = deps.tools.transitions.context(&request, &ask, team)?;
    let states = context
        .assignment
        .map(|assignment| assignment.dependencies)
        .unwrap_or_default();
    Ok(states.len() == contract.dependencies.len()
        && states
            .iter()
            .all(|state| state.status == TaskStatus::Accepted && state.integrated))
}

/// What a tick that ran a session says.
fn acted(row: &TaskProjection, agent: &Agent, purpose: &str, end: &SessionEnd) -> TickReport {
    let how = match end.reason {
        EndReason::Completed => "completed",
        EndReason::Aborted => "was aborted",
        EndReason::Limit => "reached a limit",
        EndReason::Error => "failed",
    };
    TickReport::Acted {
        task_id: row.task_id.clone(),
        what: format!(
            "ran {}'s {purpose} session, which {how}: {}",
            agent.id.as_str(),
            end.detail
        ),
    }
}

/// The number in a task id, `FRK-<n>`, by which ties are broken; an id without one goes last.
fn task_number(task: &str) -> u64 {
    task.rsplit('-')
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use farik_core::contract::{Role, TaskStatus};
    use farik_protocol::event::{
        EventBody, EventKind, SessionEndedBodyReason, SessionStartedBodyPurpose,
    };
    use farik_roles::RoleError;
    use serde_json::json;

    use crate::claude::allowed_builtins;
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{OrchestratorError, TickReport};
    use crate::recorded::fixtures::{plan_assigns_frk_1, reads_a_file};
    use crate::session::SessionPurpose;

    const NOTHING_TO_DO: &str = "nothing on the board needs doing";

    fn acted_on(report: &TickReport) -> Option<&str> {
        match report {
            TickReport::Acted { task_id, .. } => Some(task_id.as_str()),
            TickReport::Idle { .. } => None,
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_the_assigners_plan_session_for_a_ready_task() {
        let harness = Harness::new("orch-plan-starts", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(acted_on(&report), Some("FRK-1"), "{report:?}");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(spec.purpose, SessionPurpose::Plan);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(
            spec.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert_eq!(spec.cwd, harness.project.repo.path);
        assert!(spec.mcp_servers.is_empty());
        let team = harness
            .project
            .deps
            .files
            .read_team()
            .expect("the team reads");
        let pm = team
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "pm")
            .expect("pm is on the team");
        assert_eq!(
            spec.builtin_tools,
            allowed_builtins(&pm.tiers().into_iter().collect::<BTreeSet<_>>())
        );
        assert!(
            spec.initial_prompt.contains("FRK-1"),
            "{}",
            spec.initial_prompt
        );
        let row = harness.row("FRK-1");
        assert_eq!(row.status, TaskStatus::Assigned);
        assert_eq!(row.assignee_id.as_deref(), Some("dev-a"));
        assert_eq!(row.reviewer_id.as_deref(), Some("dev-b"));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn records_the_session_its_cost_and_its_end() {
        let harness = Harness::new("orch-plan-records", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let session_id = adapter.started()[0].session_id.clone();
        let events = harness.events(&[
            EventKind::SessionStarted,
            EventKind::ToolCalled,
            EventKind::TaskTransitioned,
            EventKind::CostRecorded,
            EventKind::SessionEnded,
        ]);
        let kinds: Vec<EventKind> = events.iter().map(|event| event.body.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::SessionStarted,
                EventKind::ToolCalled,
                EventKind::TaskTransitioned,
                EventKind::CostRecorded,
                EventKind::SessionEnded,
            ]
        );
        assert!(matches!(
            &events[0].body,
            EventBody::SessionStarted(body) if body.purpose == SessionStartedBodyPurpose::Plan
        ));
        assert!(matches!(
            &events[1].body,
            EventBody::ToolCalled(body) if body.tool == "mcp__farik__farik_assign_task"
        ));
        assert!(matches!(
            &events[2].body,
            EventBody::TaskTransitioned(body) if body.to.to_string() == "assigned"
        ));
        let cost = &events[3].envelope.ids;
        assert_eq!(cost.session_id.as_deref(), Some(session_id.as_str()));
        assert_eq!(cost.agent_id.as_deref(), Some("pm"));
        assert_eq!(
            cost.task_id.as_ref().map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert!(matches!(
            &events[4].body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Completed
        ));
        assert!(harness.daemon.tool_context(&session_id).is_none());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_free_assignee() {
        let harness = Harness::new("orch-plan-no-room", |_| {});
        harness.ready("FRK-1");
        harness.blocked("FRK-2", "dev-a", "dev-b");
        harness.blocked("FRK-3", "dev-b", "dev-a");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let report = orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            report,
            TickReport::Idle {
                why: NOTHING_TO_DO.to_string()
            }
        );
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_a_dependency_to_be_integrated() {
        // FRK-1's assignee is paused, so that no rule works FRK-1 and the tick reaches FRK-2.
        let harness = Harness::new("orch-plan-dependency", |wire| {
            wire["agents"][1]["status"] = json!("paused");
        });
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.file("FRK-2", "ready", |wire| {
            wire["dependencies"] = json!(["FRK-1"]);
        });
        let adapter = harness.recorded(vec![reads_a_file(), plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert!(
            !adapter
                .started()
                .iter()
                .any(|spec| spec.task_id.as_ref().map(|task| task.as_str()) == Some("FRK-2")),
            "{:?}",
            adapter.started()
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn fails_the_tick_when_a_session_cannot_start() {
        let harness = Harness::new("orch-plan-no-start", |_| {});
        harness.ready("FRK-1");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter);

        let ticked = orchestrator.tick().await;

        assert!(
            matches!(ticked, Err(OrchestratorError::Runtime(_))),
            "{ticked:?}"
        );
        let events = harness.events(&[
            EventKind::SessionStarted,
            EventKind::CostRecorded,
            EventKind::SessionEnded,
        ]);
        assert_eq!(events.len(), 3, "{events:?}");
        assert_eq!(events[0].body.kind(), EventKind::SessionStarted);
        match &events[1].body {
            EventBody::CostRecorded(body) => {
                assert_eq!(body.usage.input_tokens, 0);
                assert_eq!(body.usage.output_tokens, 0);
                assert_eq!(
                    events[1]
                        .envelope
                        .ids
                        .task_id
                        .as_ref()
                        .map(|task| task.as_str()),
                    Some("FRK-1")
                );
            }
            other => panic!("expected a cost, got {other:?}"),
        }
        assert!(matches!(
            &events[2].body,
            EventBody::SessionEnded(body) if body.reason == SessionEndedBodyReason::Error
        ));
        let session_id = events[0]
            .envelope
            .ids
            .session_id
            .clone()
            .expect("a session's event names it");
        assert!(harness.daemon.tool_context(&session_id).is_none());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn takes_the_scrum_master_as_assigner_when_there_is_one() {
        // Farik ships no Scrum Master until phase 4, so its role does not load: the tick fails
        // naming the role it chose, before any session starts.
        let harness = Harness::new("orch-plan-scrum-master", |wire| {
            let agents = wire["agents"].as_array_mut().expect("a list of agents");
            agents.push(json!({
                "id": "sam",
                "display_name": "sam",
                "role": "scrum_master",
                "status": "active"
            }));
        });
        harness.ready("FRK-1");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        let ticked = orchestrator.tick().await;

        assert_eq!(
            ticked,
            Err(OrchestratorError::Role(RoleError::NotFound {
                role_id: Role::ScrumMaster.to_string()
            }))
        );
        assert!(adapter.started().is_empty());
    }
}
