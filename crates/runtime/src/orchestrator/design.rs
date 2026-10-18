//! Rule 6 for a UI/UX Designer's task (ADR 0026): explore, plan, Catervas's approval (ADR 0061),
//! then implement. The task's latest `design_plan.*` event decides which session runs, read afresh
//! at every tick, so that a session which ends without its one answer is simply started again.

use catervas_core::contract::{TaskContract, TaskStatus};
use catervas_core::team::{Agent, Team};
use catervas_protocol::event::{EventBody, EventKind, ReasonBody};
use catervas_store::{EventQuery, TaskProjection};

use super::messages::explore_message;
use super::rules::{Waiting, acted, asleep, governor_moves, refused_since_entering, spent};
use super::session::{SessionAsk, run_session};
use super::verify::append;
use super::{Orchestrator, OrchestratorError, TickReport, worktree};
use crate::cost::extra_tries;
use crate::session::SessionPurpose;
use crate::tools::design::{DesignPlan, PlanState, design_plan, plan_history, returns};
use crate::transitions::TransitionOutcome;

/// The Catervas tools an explore session is offered: the five reading tools, the plan, and the page
/// check when the preview is open for it (step 12).
pub(super) const EXPLORE_TOOLS: &[&str] = &[
    "catervas_read_task",
    "catervas_read_board",
    "catervas_read_rules",
    "catervas_read_criteria",
    "catervas_read_decisions",
    "catervas_propose_design_plan",
    "catervas_check_page",
];

/// The Catervas tools the Designer's design review of a Developer's UI change is offered (D9): the
/// five reading tools, the page check, and its one answer.
pub(super) const DESIGN_REVIEW_TOOLS: &[&str] = &[
    "catervas_read_task",
    "catervas_read_board",
    "catervas_read_rules",
    "catervas_read_criteria",
    "catervas_read_decisions",
    "catervas_check_page",
    RECORD_DESIGN_REVIEW_TOOL,
];

/// The design review's one answer, offered to that session alone.
pub(super) const RECORD_DESIGN_REVIEW_TOOL: &str = "catervas_record_design_review";

/// Why Catervas approves every proposed plan.
const APPROVED_REASON: &str = "Catervas approved it: no agent approves a design plan, and the \
     finished change is reviewed (ADR 0061).";

/// Where a Designer's task stands before its implement session.
pub(super) enum Stage {
    /// The plan is approved: the implement session runs, given it.
    Approved(String),
    /// Something else was done, or nothing could be.
    Handled(Option<TickReport>),
}

/// The Designer's task by its latest plan: none or returned, an explore session, unless the
/// returns reached the task's tries, when the governor escalates it (`iterations`); proposed, approved by Catervas
/// at once; approved, the plan for the implement session.
pub(super) async fn stage(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    designer: &Agent,
    contract: &TaskContract,
    waiting: &mut Waiting,
) -> Result<Stage, OrchestratorError> {
    let deps = &orchestrator.deps;
    let history = plan_history(&deps.tools.log, &row.task_id)?;
    let handled = match design_plan(&history) {
        Some(DesignPlan {
            state: PlanState::Approved,
            plan,
            ..
        }) => return Ok(Stage::Approved(plan)),
        Some(DesignPlan {
            state: PlanState::Proposed,
            plan,
            ..
        }) => {
            // Nobody approves a plan (ADR 0061): Catervas records the approval, and the implement
            // session follows in this tick.
            append(
                &deps.tools,
                &row.task_id,
                None,
                None,
                EventBody::DesignPlanApproved(ReasonBody {
                    reason: APPROVED_REASON.to_string(),
                }),
            )?;
            return Ok(Stage::Approved(plan));
        }
        latest => {
            let resolved = deps.tools.log.read(&EventQuery {
                task_id: Some(row.task_id.clone()),
                kinds: vec![EventKind::EscalationResolved],
                ..EventQuery::default()
            })?;
            let tries = u32::try_from(contract.budget.max_iterations.get())
                .unwrap_or(u32::MAX)
                .saturating_add(extra_tries(&resolved));
            if returns(&history) >= tries {
                escalate(orchestrator, team, row)?
            } else {
                let reason = latest.and_then(|plan| plan.reason);
                explore(orchestrator, team, row, designer, contract, reason, waiting).await?
            }
        }
    };
    Ok(Stage::Handled(handled))
}

/// The governor's escalation of a task whose plans were returned as often as it may be; passed
/// over once refused, until the task moves.
fn escalate(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    if refused_since_entering(deps, &row.task_id, row.status, TaskStatus::Escalated)? {
        return Ok(None);
    }
    Ok(
        match governor_moves(deps, team, row, TaskStatus::Escalated)? {
            TransitionOutcome::Moved(_) => Some(TickReport::Acted {
                task_id: row.task_id.clone(),
                what: "escalated it: its design plan was returned as often as it may be"
                    .to_string(),
            }),
            TransitionOutcome::Refused(_) => None,
        },
    )
}

/// The Designer's `explore` session, in the task's worktree with the read tier's built-ins and
/// `EXPLORE_TOOLS`, told the reason its last plan was returned when it was.
async fn explore(
    orchestrator: &Orchestrator,
    team: &Team,
    row: &TaskProjection,
    designer: &Agent,
    contract: &TaskContract,
    returned: Option<String>,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    let deps = &orchestrator.deps;
    if spent(deps, team, contract, &mut waiting.day_spent)?
        | asleep(deps, designer, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: designer,
            contract: Some(contract),
            purpose: SessionPurpose::Explore,
            cwd: worktree(deps, &row.task_id),
            executor: None,
            read_only: true,
            only_tool: None,
            tools: Some(EXPLORE_TOOLS),
            in_reply_to: None,
            thread: None,
            initial_prompt: explore_message(contract, returned.as_deref()),
        },
    )
    .await?;
    Ok(Some(acted(row, designer, "explore", &end)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use catervas_core::contract::TaskStatus;
    use catervas_core::governor::permissions::PermissionTier;
    use catervas_protocol::command::Command;
    use catervas_protocol::event::{EscalationRaisedBodyReason, EventBody, EventKind};
    use serde_json::{Value, json};

    use crate::claude::allowed_builtins;
    use crate::orchestrator::fixtures::{ExecutorWitness, Harness};
    use crate::preview::fixtures::FakePreviews;
    use crate::recorded::Transcript;
    use crate::recorded::fixtures::{
        explore_plans_ctv_1, implement_by_iris_ctv_1, review_writes_note,
    };
    use crate::session::{SessionPurpose, SessionSpec};
    use crate::tools::fixtures::{browsing, with_the_designer};

    /// What the explore session's plan says, which the implement session is given.
    const PLAN_WORDS: &str = "the label goes above its field";
    /// Why the Product Manager returns the plan.
    const RETURNED: &str = "Say what the page looks like in the dark theme too.";

    /// CTV-1 in progress, held by the UI/UX Designer `iris` and reviewed by the Architect `ada`,
    /// with its worktree made, and `change` applied to its contract.
    fn a_designers_task(name: &str, change: impl FnOnce(&mut Value)) -> Harness {
        a_designers_task_in(name, with_the_designer, change)
    }

    /// `a_designers_task`, with `team` making the team's wire.
    fn a_designers_task_in(
        name: &str,
        team: impl FnOnce(&mut Value),
        change: impl FnOnce(&mut Value),
    ) -> Harness {
        let harness = Harness::new(name, team);
        harness.file("CTV-1", "ready", |wire| {
            wire["assignee_role"] = json!("ui_ux_designer");
            wire["reviewer_role"] = json!("architect");
            change(wire);
        });
        let people = json!({ "assignee": "iris", "reviewer": "ada" });
        harness.project.moved("CTV-1", "ready", "assigned", &people);
        harness
            .project
            .moved("CTV-1", "assigned", "in_progress", &people);
        harness
            .project
            .deps
            .git
            .create_worktree(&harness.worktree("CTV-1"), &harness.branch("CTV-1"), "main")
            .expect("the task's worktree is made");
        harness
    }

    /// Ticks `count` times with `transcripts`, and answers the sessions started.
    async fn ticked(
        harness: &Harness,
        transcripts: Vec<Transcript>,
        count: usize,
    ) -> Vec<SessionSpec> {
        let adapter = harness.recorded(transcripts);
        let orchestrator = harness.orchestrator(adapter.clone());
        for _ in 0..count {
            orchestrator.tick().await.expect("the tick runs");
        }
        adapter.started()
    }

    fn who(started: &[SessionSpec]) -> Vec<(&str, SessionPurpose)> {
        started
            .iter()
            .map(|spec| (spec.agent_id.as_str(), spec.purpose))
            .collect()
    }

    fn escalation_reasons(harness: &Harness) -> Vec<EscalationRaisedBodyReason> {
        harness
            .events(&[EventKind::EscalationRaised])
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn catervas_approves_a_proposed_plan_and_the_designer_implements() {
        let harness = a_designers_task("design-flow-order", |_| {});
        let started = ticked(
            &harness,
            vec![explore_plans_ctv_1(), implement_by_iris_ctv_1()],
            2,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("iris", SessionPurpose::Explore),
                ("iris", SessionPurpose::Implement)
            ]
        );
        let explore = &started[0];
        assert_eq!(
            explore.catervas_tools,
            [
                "catervas_read_task",
                "catervas_read_board",
                "catervas_read_rules",
                "catervas_read_criteria",
                "catervas_read_decisions",
                "catervas_propose_design_plan",
            ]
        );
        assert_eq!(
            explore.builtin_tools,
            allowed_builtins(&BTreeSet::from([
                PermissionTier::Read,
                PermissionTier::Network
            ]))
        );
        assert_eq!(explore.cwd, harness.worktree("CTV-1"));
        assert!(
            started[1].initial_prompt.contains(PLAN_WORDS),
            "{}",
            started[1].initial_prompt
        );
        assert!(!explore.initial_prompt.contains(PLAN_WORDS));
        assert_eq!(harness.events(&[EventKind::DesignPlanProposed]).len(), 1);
        let approved = harness.events(&[EventKind::DesignPlanApproved]);
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].envelope.ids.agent_id, None);
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Verifying);

        // An older log's returned plan still reads as returned: the Designer explores again.
        let again = a_designers_task("design-flow-returned", |_| {});
        again.project.record(
            "CTV-1",
            "design_plan.proposed",
            &json!({ "plan": "A plan." }),
        );
        again.project.record(
            "CTV-1",
            "design_plan.returned",
            &json!({ "reason": RETURNED }),
        );
        let started = ticked(&again, vec![explore_plans_ctv_1()], 1).await;
        assert_eq!(who(&started), [("iris", SessionPurpose::Explore)]);
        assert!(started[0].initial_prompt.contains(RETURNED));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn explores_with_the_browser() {
        let mut harness = a_designers_task_in("design-flow-browser", browsing, |_| {});
        harness.previews = std::sync::Arc::new(FakePreviews::ready());
        let started = ticked(&harness, vec![explore_plans_ctv_1()], 1).await;

        assert_eq!(who(&started), [("iris", SessionPurpose::Explore)]);
        let explore = &started[0];
        assert_eq!(
            explore.catervas_tools,
            [
                "catervas_read_task",
                "catervas_read_board",
                "catervas_read_rules",
                "catervas_read_criteria",
                "catervas_read_decisions",
                "catervas_propose_design_plan",
                "catervas_check_page",
            ]
        );
        let servers: Vec<&str> = explore
            .mcp_servers
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(servers, ["playwright"]);
        assert!(
            !explore.initial_prompt.contains("no browser yet"),
            "{}",
            explore.initial_prompt
        );
        assert!(
            explore.initial_prompt.contains("`catervas_check_page`"),
            "{}",
            explore.initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn holds_the_explore_session_to_the_read_tier() {
        // The hook judges every call by the registration's tiers, so an explore session is held
        // to reading and researching there too, not by its offered tools alone, whatever the
        // Designer's grants.
        let harness = a_designers_task_in(
            "design-flow-explore-tiers",
            |wire| {
                with_the_designer(wire);
                wire["agents"][3]["grants"] = json!(["network", "git_remote"]);
            },
            |_| {},
        );
        let adapter = harness.recorded(vec![explore_plans_ctv_1()]);
        let witness = std::sync::Arc::new(ExecutorWitness::new(
            adapter.clone(),
            std::sync::Arc::clone(&harness.daemon),
        ));
        let orchestrator = harness.orchestrator(witness.clone());
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(who(&adapter.started()), [("iris", SessionPurpose::Explore)]);
        assert_eq!(
            witness.given_tiers(),
            [vec![PermissionTier::Read, PermissionTier::Network]]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_after_too_many_returns() {
        let harness = a_designers_task("design-flow-returns", |wire| {
            wire["budget"]["max_iterations"] = json!(3);
        });
        let plan = json!({ "plan": "A plan." });
        let returned = json!({ "reason": "Not yet." });
        for _ in 0..2 {
            harness
                .project
                .record("CTV-1", "design_plan.proposed", &plan);
            harness
                .project
                .record("CTV-1", "design_plan.returned", &returned);
        }
        let adapter = harness.recorded(vec![explore_plans_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        // Two returns of three: the Designer explores again.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(who(&adapter.started()), [("iris", SessionPurpose::Explore)]);
        assert!(escalation_reasons(&harness).is_empty());

        harness
            .project
            .record("CTV-1", "design_plan.returned", &returned);
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(harness.row("CTV-1").status, TaskStatus::Escalated);
        assert_eq!(
            escalation_reasons(&harness),
            [EscalationRaisedBodyReason::Iterations]
        );
        assert_eq!(
            adapter.started().len(),
            1,
            "no session after the third return"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn explores_again_once_the_human_grants_more_tries() {
        let harness = a_designers_task("design-flow-extra-tries", |wire| {
            wire["budget"]["max_iterations"] = json!(3);
        });
        let plan = json!({ "plan": "A plan." });
        let returned = json!({ "reason": "Not yet." });
        for _ in 0..3 {
            harness
                .project
                .record("CTV-1", "design_plan.proposed", &plan);
            harness
                .project
                .record("CTV-1", "design_plan.returned", &returned);
        }
        let adapter = harness.recorded(vec![explore_plans_ctv_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("CTV-1").status, TaskStatus::Escalated);
        assert!(adapter.started().is_empty());

        orchestrator
            .handle(Command::EscalationResolve {
                task_id: "CTV-1".parse().expect("a task id"),
                to: TaskStatus::InProgress,
                message: "Two more plans, then.".to_string(),
                extra_tries: Some(2),
                extra_room: false,
            })
            .await
            .expect("the escalation is resolved with more tries");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(who(&adapter.started()), [("iris", SessionPurpose::Explore)]);
        assert_eq!(
            escalation_reasons(&harness),
            [EscalationRaisedBodyReason::Iterations]
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn sends_the_designers_work_to_the_architect() {
        let harness = a_designers_task("design-flow-review", |_| {});
        let started = ticked(
            &harness,
            vec![
                explore_plans_ctv_1(),
                implement_by_iris_ctv_1(),
                review_writes_note(),
            ],
            3,
        )
        .await;

        assert_eq!(
            who(&started),
            [
                ("iris", SessionPurpose::Explore),
                ("iris", SessionPurpose::Implement),
                ("ada", SessionPurpose::Verify),
            ]
        );
        assert_eq!(harness.events(&[EventKind::ReviewRecorded]).len(), 1);
    }
}
