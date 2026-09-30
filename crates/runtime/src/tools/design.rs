//! The UI/UX Designer's plan gate (ADR 0026): `farik_propose_design_plan`, which ends the
//! Designer's explore session with its plan, and `farik_decide_design_plan`, the Product Manager's
//! approval or return of it. Both only write the log. Where a task's plan stands is read back from
//! the log by the governor's checks, the orchestrator, and the browser.

use farik_core::contract::{Role, TaskId};
use farik_core::governor::permissions::{PermissionTier, check_design_plan};
use farik_protocol::event::{
    DesignPlanDecidedBody, DesignPlanProposedBody, EventBody, EventKind, FarikEvent,
};
use farik_store::{EventLog, EventQuery, StoreError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::work::opens_with_a_summary;
use super::{Call, ToolError};
use crate::session::SessionPurpose;

/// How long a plan may be, in characters.
const PLAN_CHARS: std::ops::RangeInclusive<usize> = 200..=8_000;

/// `farik_propose_design_plan`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposeDesignPlanInput {
    /// The plan, 200 to 8,000 characters: a summary for the user of 20 to 600 characters, a
    /// blank line, then what you saw, what you will change, which screens and sizes, and what you
    /// will leave alone.
    plan: String,
}

/// `farik_decide_design_plan`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecideDesignPlanInput {
    /// True to approve the plan, false to return it to the Designer.
    approve: bool,
    /// Why, which a returned plan's next exploration is given.
    reason: String,
}

/// Where a task's latest design plan stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlanState {
    /// Waiting for the Product Manager.
    Proposed,
    /// Approved: the Designer implements it.
    Approved,
    /// Returned: the Designer explores again.
    Returned,
}

/// A task's latest design plan, and the Product Manager's decision on it once there is one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DesignPlan {
    pub(crate) plan: String,
    pub(crate) state: PlanState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

/// The latest plan among a task's events, oldest first, with the decision that followed it.
pub(crate) fn design_plan(history: &[FarikEvent]) -> Option<DesignPlan> {
    let mut latest: Option<DesignPlan> = None;
    for event in history {
        let (state, reason) = match &event.body {
            EventBody::DesignPlanProposed(body) => {
                latest = Some(DesignPlan {
                    plan: body.plan.clone(),
                    state: PlanState::Proposed,
                    reason: None,
                });
                continue;
            }
            EventBody::DesignPlanApproved(body) => (PlanState::Approved, &body.reason),
            EventBody::DesignPlanReturned(body) => (PlanState::Returned, &body.reason),
            _ => continue,
        };
        if let Some(plan) = latest.as_mut() {
            plan.state = state;
            plan.reason = Some(reason.clone());
        }
    }
    latest
}

/// How many times the task's plans were returned, which counts against its `max_iterations`.
pub(crate) fn returns(history: &[FarikEvent]) -> u32 {
    let count = history
        .iter()
        .filter(|event| matches!(event.body, EventBody::DesignPlanReturned(_)))
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The task's design-plan events, oldest first.
pub(crate) fn plan_history(log: &EventLog, task: &TaskId) -> Result<Vec<FarikEvent>, StoreError> {
    log.read(&EventQuery {
        task_id: Some(task.clone()),
        kinds: vec![
            EventKind::DesignPlanProposed,
            EventKind::DesignPlanApproved,
            EventKind::DesignPlanReturned,
        ],
        ..EventQuery::default()
    })
}

/// The task's latest design plan, read from the log now.
pub(crate) fn read_design_plan(
    log: &EventLog,
    task: &TaskId,
) -> Result<Option<DesignPlan>, StoreError> {
    Ok(design_plan(&plan_history(log, task)?))
}

/// The plan gate of a call of `role`'s needing `tier` in a session of `task`, read from the log at
/// the call (ADR 0026): a UI/UX Designer's writes wait for its task's plan to be approved, and one
/// with no task has no plan to wait for. Every other role passes without the log being read.
///
/// # Errors
///
/// The gate's refusal, in its words; or the log's failure, as `failed`.
pub(crate) fn design_plan_gate(
    log: &EventLog,
    role: Role,
    tier: PermissionTier,
    task: Option<&TaskId>,
) -> Result<(), ToolError> {
    let approved = match task {
        Some(task) if role == Role::UiUxDesigner => read_design_plan(log, task)
            .map_err(super::failed)?
            .is_some_and(|plan| plan.state == PlanState::Approved),
        _ => false,
    };
    check_design_plan(role, tier, approved).map_err(|refusal| Refusal::Tool(refusal).into())
}

fn refused(detail: impl Into<String>) -> ToolError {
    Refusal::DesignPlanRefused {
        detail: detail.into(),
    }
    .into()
}

/// Records `design_plan.proposed`, from the UI/UX Designer's explore session of its task, for a
/// plan within its bounds that opens with a summary.
pub(super) fn propose(call: &Call<'_>, input: ProposeDesignPlanInput) -> Result<Value, ToolError> {
    let task = match &call.context.task_id {
        Some(task)
            if call.role() == Role::UiUxDesigner
                && call.context.purpose == SessionPurpose::Explore =>
        {
            task
        }
        _ => {
            return Err(refused(
                "only the UI/UX Designer proposes a design plan, in its explore session of the task",
            ));
        }
    };
    let length = input.plan.chars().count();
    if !PLAN_CHARS.contains(&length) {
        return Err(refused(format!(
            "a plan is 200 to 8,000 characters, and this one is {length}"
        )));
    }
    if !opens_with_a_summary(&input.plan) {
        return Err(refused(
            "open the plan with a summary for the user, 20 to 600 characters, then a blank line",
        ));
    }
    let event = call.append(
        Some(task),
        EventBody::DesignPlanProposed(DesignPlanProposedBody { plan: input.plan }),
    )?;
    Ok(json!({ "seq": event.envelope.seq }))
}

/// Records `design_plan.approved` or `design_plan.returned`, from the Product Manager's verify
/// session of the task, with a reason, while the task's latest plan waits for a decision.
pub(super) fn decide(call: &Call<'_>, input: DecideDesignPlanInput) -> Result<Value, ToolError> {
    let task = match &call.context.task_id {
        Some(task)
            if call.role() == Role::ProductManager
                && call.context.purpose == SessionPurpose::Verify =>
        {
            task
        }
        _ => {
            return Err(refused(
                "only the Product Manager decides a design plan, in its verify session of the task",
            ));
        }
    };
    if input.reason.trim().is_empty() {
        return Err(Refusal::BlankReason.into());
    }
    let waiting = read_design_plan(&call.deps().log, task)
        .map_err(super::failed)?
        .is_some_and(|plan| plan.state == PlanState::Proposed);
    if !waiting {
        return Err(refused("the task has no plan waiting for a decision"));
    }
    let body = DesignPlanDecidedBody {
        reason: input.reason,
    };
    let event = call.append(
        Some(task),
        if input.approve {
            EventBody::DesignPlanApproved(body)
        } else {
            EventBody::DesignPlanReturned(body)
        },
    )?;
    Ok(json!({ "seq": event.envelope.seq }))
}

#[cfg(test)]
mod tests {
    use farik_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use super::super::ToolError;
    use super::super::fixtures::{TestProject, a_team_of_three, run, with_the_designer};
    use crate::session::SessionPurpose;

    /// A plan of `length` characters that opens with a summary of `summary` characters.
    pub(crate) fn a_plan(summary: usize, length: usize) -> String {
        let opening = format!("{}\n\n", "s".repeat(summary));
        let rest = length.saturating_sub(opening.chars().count());
        format!("{opening}{}", "p".repeat(rest))
    }

    fn a_project(name: &str) -> TestProject {
        let project = TestProject::new(name, &a_team_of_three(with_the_designer));
        project.filed("FRK-1", "in_progress", "task", None);
        project
    }

    fn call(
        project: &TestProject,
        agent: &str,
        task: Option<&str>,
        purpose: SessionPurpose,
        name: &str,
        input: Value,
    ) -> Result<Value, ToolError> {
        let mut context = project.context(agent, task);
        context.purpose = purpose;
        run(&context, name, input)
    }

    fn propose(
        project: &TestProject,
        agent: &str,
        purpose: SessionPurpose,
        plan: &str,
    ) -> Result<Value, ToolError> {
        call(
            project,
            agent,
            Some("FRK-1"),
            purpose,
            "farik_propose_design_plan",
            json!({ "plan": plan }),
        )
    }

    fn decide(
        project: &TestProject,
        agent: &str,
        task: Option<&str>,
        purpose: SessionPurpose,
    ) -> Result<Value, ToolError> {
        call(
            project,
            agent,
            task,
            purpose,
            "farik_decide_design_plan",
            json!({ "approve": true, "reason": "It keeps to the task's screens." }),
        )
    }

    fn refused(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_the_plan_outside_explore() {
        let project = a_project("tools-design-plan");
        let plan = a_plan(40, 400);
        let outside = "design_plan_refused: only the UI/UX Designer proposes a design plan, in \
                       its explore session of the task";
        assert_eq!(
            refused(propose(&project, "iris", SessionPurpose::Implement, &plan)),
            outside
        );
        assert_eq!(
            refused(propose(&project, "dev-a", SessionPurpose::Explore, &plan)),
            outside
        );
        for length in [199, 8_001] {
            assert_eq!(
                refused(propose(
                    &project,
                    "iris",
                    SessionPurpose::Explore,
                    &a_plan(40, length)
                )),
                format!(
                    "design_plan_refused: a plan is 200 to 8,000 characters, and this one is \
                     {length}"
                )
            );
        }
        let summary = "design_plan_refused: open the plan with a summary for the user, 20 to 600 \
                       characters, then a blank line";
        for opening in [19, 601] {
            assert_eq!(
                refused(propose(
                    &project,
                    "iris",
                    SessionPurpose::Explore,
                    &a_plan(opening, 2_000)
                )),
                summary
            );
        }
        assert!(project.events(&[EventKind::DesignPlanProposed]).is_empty());

        for (opening, length) in [(20, 200), (600, 8_000)] {
            propose(
                &project,
                "iris",
                SessionPurpose::Explore,
                &a_plan(opening, length),
            )
            .expect("a plan within its bounds, in the Designer's explore session");
        }
        let proposed = project.events(&[EventKind::DesignPlanProposed]);
        assert_eq!(proposed.len(), 2);
        let EventBody::DesignPlanProposed(body) = &proposed[1].body else {
            panic!("a design_plan.proposed");
        };
        assert_eq!(body.plan, a_plan(600, 8_000));
        let ids = &proposed[1].envelope.ids;
        assert_eq!(ids.task_id.as_ref().map(|id| id.as_str()), Some("FRK-1"));
        assert_eq!(ids.agent_id.as_deref(), Some("iris"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_the_decision_but_from_the_product_manager() {
        let project = a_project("tools-design-decide");
        project.record(
            "FRK-1",
            "design_plan.proposed",
            &json!({ "plan": a_plan(40, 400) }),
        );
        let outside = "design_plan_refused: only the Product Manager decides a design plan, in \
                       its verify session of the task";
        assert_eq!(
            refused(decide(
                &project,
                "ada",
                Some("FRK-1"),
                SessionPurpose::Verify
            )),
            outside
        );
        // The Designer never decides its own plan, in whatever session.
        for purpose in [SessionPurpose::Explore, SessionPurpose::Verify] {
            assert_eq!(
                refused(decide(&project, "iris", Some("FRK-1"), purpose)),
                outside
            );
        }
        assert_eq!(
            refused(decide(
                &project,
                "pm",
                Some("FRK-1"),
                SessionPurpose::Implement
            )),
            outside
        );
        assert_eq!(
            refused(decide(&project, "pm", None, SessionPurpose::Verify)),
            outside
        );
        assert!(project.events(&[EventKind::DesignPlanApproved]).is_empty());

        decide(&project, "pm", Some("FRK-1"), SessionPurpose::Verify)
            .expect("the Product Manager decides, in its verify session of the task");
        let approved = project.events(&[EventKind::DesignPlanApproved]);
        assert_eq!(approved.len(), 1);
        let EventBody::DesignPlanApproved(body) = &approved[0].body else {
            panic!("a design_plan.approved");
        };
        assert_eq!(body.reason, "It keeps to the task's screens.");
        assert_eq!(approved[0].envelope.ids.agent_id.as_deref(), Some("pm"));

        // A plan is decided once: the next decision waits for the next plan.
        assert_eq!(
            refused(decide(
                &project,
                "pm",
                Some("FRK-1"),
                SessionPurpose::Verify
            )),
            "design_plan_refused: the task has no plan waiting for a decision"
        );
        // With the next plan waiting, a blank reason is still refused.
        project.record(
            "FRK-1",
            "design_plan.proposed",
            &json!({ "plan": a_plan(40, 400) }),
        );
        assert_eq!(
            refused(call(
                &project,
                "pm",
                Some("FRK-1"),
                SessionPurpose::Verify,
                "farik_decide_design_plan",
                json!({ "approve": false, "reason": " " }),
            )),
            "blank_reason: a reason is recorded, and the log is where somebody reads it back"
        );
        assert!(project.events(&[EventKind::DesignPlanReturned]).is_empty());
    }
}
