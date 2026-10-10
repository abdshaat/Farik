//! The UI/UX Designer's plan gate (ADR 0026): `catervas_propose_design_plan`, which ends the
//! Designer's explore session with its plan, and `catervas_decide_design_plan`, the Product Manager's
//! approval or return of it. Both only write the log. Where a task's plan stands is read back from
//! the log by the governor's checks, the orchestrator, and the browser.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas_core::contract::{Role, TaskId, TaskStatus};
use catervas_core::governor::gates::DesignerBrowser;
use catervas_core::governor::permissions::{PermissionTier, check_design_plan};
use catervas_core::team::Team;
use catervas_protocol::event::{
    CatervasEvent, DesignPlanProposedBody, DesignReviewCheck, DesignReviewRecordedBody, EventBody,
    EventKind, PageCheckedBody, ReasonBody,
};
use catervas_roles::builtin_connector;
use catervas_store::waiting::last_move_into;
use catervas_store::{EventLog, EventQuery, StoreError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::work::opens_with_a_summary;
use super::{Call, ToolError};
use crate::preview::{CheckTheme, CheckWidth, check_page};
use crate::prompt::untrusted_block;
use crate::session::SessionPurpose;

/// How long a plan may be, in characters.
const PLAN_CHARS: std::ops::RangeInclusive<usize> = 200..=8_000;

/// `catervas_propose_design_plan`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposeDesignPlanInput {
    /// The plan, 200 to 8,000 characters: a summary for the user of 20 to 600 characters, a
    /// blank line, then what you saw, what you will change, which screens and sizes, and what you
    /// will leave alone.
    plan: String,
}

/// `catervas_decide_design_plan`'s input.
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
pub(crate) fn design_plan(history: &[CatervasEvent]) -> Option<DesignPlan> {
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
pub(crate) fn returns(history: &[CatervasEvent]) -> u32 {
    let count = history
        .iter()
        .filter(|event| matches!(event.body, EventBody::DesignPlanReturned(_)))
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The task's design-plan events, oldest first.
pub(crate) fn plan_history(
    log: &EventLog,
    task: &TaskId,
) -> Result<Vec<CatervasEvent>, StoreError> {
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
    let body = ReasonBody {
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

/// `catervas_check_page`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckPageInput {
    /// The page's path on the preview, starting with `/`.
    path: String,
    /// phone (360 px wide) or desktop (1280 px).
    width: CheckWidth,
    /// light or dark: the colour scheme the page is told the user prefers.
    theme: CheckTheme,
}

/// How much of a check's violations an agent is shown.
const VIOLATIONS_CAP_BYTES: usize = 16 * 1024;

/// Where the screenshots of `task` are kept, under the project at `root`.
pub(crate) fn screenshots(root: &Path, task: &TaskId) -> PathBuf {
    root.join(".catervas/local/screenshots").join(task.as_str())
}

/// Checks a page of the task's preview, from the UI/UX Designer's session that has it open, and
/// records `page.checked`. The page's words reach the agent inside an `untrusted` block.
pub(super) async fn check(call: &Call<'_>, input: CheckPageInput) -> Result<Value, ToolError> {
    let (task, preview) = match (&call.context.task_id, &call.context.preview) {
        (Some(task), Some(preview)) if call.role() == Role::UiUxDesigner => {
            (task, Arc::clone(preview))
        }
        _ => {
            return Err(Refusal::CheckPageRefused {
                detail: "only the UI/UX Designer checks a page, in a session of its task with \
                         the preview open"
                    .to_string(),
            }
            .into());
        }
    };
    if !input.path.starts_with('/') {
        return Err(Refusal::CheckPageRefused {
            detail: format!("a page's path starts with /, and {:?} does not", input.path),
        }
        .into());
    }
    let definition = builtin_connector("playwright").ok_or_else(|| ToolError::Failed {
        detail: "Catervas ships no playwright connector".to_string(),
    })?;
    let folder = screenshots(call.deps().files.root(), task);
    std::fs::create_dir_all(&folder).map_err(super::failed)?;
    let file = format!(
        "{}-{}-{}.png",
        call.context.session_id,
        input.width.as_str(),
        input.theme.as_str()
    );
    let out = folder.join(&file);
    let (path, width, theme) = (input.path.clone(), input.width, input.theme);
    let checked = tokio::task::spawn_blocking(move || {
        check_page(&definition, preview.as_ref(), &path, width, theme, &out)
    })
    .await
    .map_err(super::failed)?
    .map_err(super::failed)?;
    let body: PageCheckedBody = serde_json::from_value(json!({
        "width": width.as_str(),
        "theme": theme.as_str(),
        "path": checked.path,
        "violations": checked.violations,
        "screenshot": file,
    }))
    .map_err(super::failed)?;
    call.append(Some(task), EventBody::PageChecked(body))?;
    let listed = serde_json::to_string_pretty(&checked.violations).map_err(super::failed)?;
    Ok(json!({
        "width": width.as_str(),
        "theme": theme.as_str(),
        "path": checked.path,
        "screenshot": file,
        "violation_count": checked.violations.len(),
        "violations": untrusted_block("page", &listed, VIOLATIONS_CAP_BYTES),
    }))
}

/// `catervas_record_design_review`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordDesignReviewInput {
    /// True when the change reads well at both widths in both themes, false to send it back to
    /// the Developer.
    pass: bool,
    /// Why: what you saw, and for a fail, what the Developer is to change.
    reasons: String,
}

/// The four checks a design review needs, in the order it records them.
const REVIEW_CHECKS: [(&str, &str); 4] = [
    ("phone", "light"),
    ("phone", "dark"),
    ("desktop", "light"),
    ("desktop", "dark"),
];

/// Records `design_review.recorded`, from the UI/UX Designer's design review of the task (its
/// `verify` session), once the session has checked a page at each width in each theme: the
/// session's latest check of each is copied into the review, so that what the review says it saw
/// is Catervas's own measurement.
pub(super) fn record_review(
    call: &Call<'_>,
    input: RecordDesignReviewInput,
) -> Result<Value, ToolError> {
    let task = match &call.context.task_id {
        Some(task)
            if call.role() == Role::UiUxDesigner
                && call.context.purpose == SessionPurpose::Verify =>
        {
            task
        }
        _ => {
            return Err(Refusal::DesignReviewRefused {
                detail: "only the UI/UX Designer records a design review, in its review of the \
                         task"
                    .to_string(),
            }
            .into());
        }
    };
    if input.reasons.trim().is_empty() {
        return Err(Refusal::BlankReason.into());
    }
    let checked = call
        .deps()
        .log
        .read(&EventQuery {
            task_id: Some(task.clone()),
            kinds: vec![EventKind::PageChecked],
            ..EventQuery::default()
        })
        .map_err(super::failed)?;
    let mut checks = Vec::new();
    let mut missing = Vec::new();
    for (width, theme) in REVIEW_CHECKS {
        let latest = checked.iter().rev().find_map(|event| match &event.body {
            EventBody::PageChecked(body)
                if event.envelope.ids.session_id.as_deref()
                    == Some(call.context.session_id.as_str())
                    && body.width.to_string() == width
                    && body.theme.to_string() == theme =>
            {
                Some(body)
            }
            _ => None,
        });
        match latest {
            Some(body) => checks.push(DesignReviewCheck {
                width: body.width,
                theme: body.theme,
                violations: body.violations.clone(),
            }),
            None => missing.push(format!("{width} {theme}")),
        }
    }
    if !missing.is_empty() {
        return Err(Refusal::DesignReviewIncomplete {
            missing: missing.join(", "),
        }
        .into());
    }
    let event = call.append(
        Some(task),
        EventBody::DesignReviewRecorded(DesignReviewRecordedBody {
            pass: input.pass,
            reasons: input.reasons,
            checks,
        }),
    )?;
    Ok(json!({ "seq": event.envelope.seq }))
}

/// Where a UI change's design review stands (step 12), as `task.get` words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewState {
    /// Not a UI change, or no Designer on the team.
    NotNeeded,
    /// The Designer's review is due.
    Waiting,
    /// The team's only Designer is paused.
    WaitingOnDesigner,
    /// The team has not said how to open its app.
    PreviewMissing,
    /// The Designer cannot have its browser: no Docker sandbox.
    DesignerNeedsSandbox,
    /// The active Designer has its Playwright connector off, so it gets no work.
    DesignerNeedsBrowser,
    /// The Designer passed the change since the task last entered `verifying`.
    Passed,
    /// The Designer failed it.
    Failed,
}

/// A task's design review: where it stands, and the latest one recorded since the task last
/// entered `verifying`, when there is one.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DesignReview {
    pub(crate) state: ReviewState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reasons: Option<String>,
    pub(crate) checks: Vec<DesignReviewCheck>,
    /// The Designer that recorded it, and its session.
    #[serde(skip)]
    pub(crate) recorded_by: Option<(String, Option<String>)>,
}

/// The design review of a task whose change is `ui_change` or not, read from its `history`
/// (oldest first) and the team as it is now. A review recorded since the task last entered
/// `verifying` answers first, whatever has changed since; without one, the team's preview, the
/// Designer's browser, and a Designer that is not paused are each waited for, in that order.
/// `browser` says whether the Designer can have its browser, and is called only when the answer
/// waits for that: it may ask Docker, which a page's request must not wait for when the answer
/// does not depend on it.
pub(crate) fn design_review(
    team: &Team,
    ui_change: bool,
    history: &[CatervasEvent],
    browser: impl FnOnce() -> DesignerBrowser,
) -> DesignReview {
    let waiting = |state| DesignReview {
        state,
        reasons: None,
        checks: Vec::new(),
        recorded_by: None,
    };
    if !ui_change || !team.has_designer() {
        return waiting(ReviewState::NotNeeded);
    }
    let since =
        last_move_into(history, TaskStatus::Verifying).map_or(0, |event| event.envelope.seq);
    let recorded = history
        .iter()
        .rev()
        .take_while(|event| event.envelope.seq > since)
        .find_map(|event| match &event.body {
            EventBody::DesignReviewRecorded(body) => Some((event, body)),
            _ => None,
        });
    if let Some((event, body)) = recorded {
        return DesignReview {
            state: if body.pass {
                ReviewState::Passed
            } else {
                ReviewState::Failed
            },
            reasons: Some(body.reasons.clone()),
            checks: body.checks.clone(),
            recorded_by: event
                .envelope
                .ids
                .agent_id
                .clone()
                .map(|agent| (agent, event.envelope.ids.session_id.clone())),
        };
    }
    if team.preview().is_none() {
        return waiting(ReviewState::PreviewMissing);
    }
    waiting(match browser() {
        DesignerBrowser::NoSandbox | DesignerBrowser::NoPreview => {
            ReviewState::DesignerNeedsSandbox
        }
        DesignerBrowser::NoConnector => ReviewState::DesignerNeedsBrowser,
        DesignerBrowser::Ready if team.designer().is_none() => ReviewState::WaitingOnDesigner,
        DesignerBrowser::Ready => ReviewState::Waiting,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use crate::preview::fixtures::{CheckedPreview, SCREENSHOT, after};

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
        project.filed("CTV-1", "in_progress", "task", None);
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
            Some("CTV-1"),
            purpose,
            "catervas_propose_design_plan",
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
            "catervas_decide_design_plan",
            json!({ "approve": true, "reason": "It keeps to the task's screens." }),
        )
    }

    fn refused(result: Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The check's own arguments, after asserting docker's: run once, as the user, in the
    /// preview's namespace, labelled, by `node` in the pinned image.
    fn beside_the_preview(args: &[String]) -> &[String] {
        assert_eq!(&args[..4], ["run", "--rm", "-i", "--init"], "{args:?}");
        assert_eq!(after(args, "--user"), ["1000:1000"]);
        assert_eq!(
            after(args, "--network"),
            ["container:catervas-preview-p-ctv-1"]
        );
        assert_eq!(
            after(args, "--label"),
            ["catervas.project=p", "catervas.task=CTV-1"]
        );
        // A missing image fails the check at once rather than pulling gigabytes unseen.
        assert_eq!(after(args, "--pull"), ["never"]);
        // Named, so that a check Catervas gives up on is removed by name, not left to its watchdog.
        let named = after(args, "--name");
        assert!(
            named.len() == 1 && named[0].starts_with("catervas-check-catervas-preview-p-ctv-1-"),
            "{args:?}"
        );
        assert_eq!(after(args, "--entrypoint"), ["node"]);
        let definition = catervas_roles::builtin_connector("playwright").expect("shipped");
        let image = args
            .iter()
            .position(|arg| *arg == definition.image)
            .expect("the pinned image");
        &args[image + 1..]
    }

    const A_VIOLATION: &str = r#"{"violations":[{"rule":"button-name","impact":"critical","target":"button.menu","help":"Buttons must have discernible text"}]}"#;

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn checks_a_page_through_the_runner() {
        let project = a_project("tools-check-page");
        let preview = Arc::new(CheckedPreview::printing(A_VIOLATION));
        let mut context = project.context("iris", Some("CTV-1"));
        context.preview = Some(preview.clone());
        let phone = run(
            &context,
            "catervas_check_page",
            json!({ "path": "/settings", "width": "phone", "theme": "dark" }),
        )
        .expect("the Designer checks a page of its task's preview");
        run(
            &context,
            "catervas_check_page",
            json!({ "path": "/", "width": "desktop", "theme": "light" }),
        )
        .expect("and another");

        let runs = preview.runs();
        assert_eq!(runs.len(), 2);
        let per_check = [
            ("/settings", "360", "dark", "session-1-phone-dark.png"),
            ("/", "1280", "light", "session-1-desktop-light.png"),
        ];
        for (args, (path, width, theme, file)) in runs.iter().zip(per_check) {
            let script = beside_the_preview(args);
            assert_eq!(script[..2], ["--input-type=module", "-"]);
            assert_eq!(
                after(script, "--url"),
                [format!("http://localhost:4400{path}").as_str()]
            );
            assert_eq!(after(script, "--width"), [width]);
            assert_eq!(after(script, "--theme"), [theme]);
            assert_eq!(
                after(script, "--screenshot"),
                [format!("/output/{file}").as_str()]
            );
            assert_eq!(after(script, "--module-root"), ["/app/node_modules"]);
            assert_eq!(after(script, "--proxy-server"), ["http://127.0.0.1:9"]);
            assert_eq!(after(script, "--proxy-bypass"), ["localhost"]);
            assert_eq!(
                after(script, "--tags"),
                ["wcag2a,wcag2aa,wcag21a,wcag21aa,wcag22aa"]
            );
            assert_eq!(
                std::fs::read(
                    project
                        .repo
                        .path
                        .join(".catervas/local/screenshots/CTV-1")
                        .join(file)
                )
                .expect("the screenshot is kept"),
                SCREENSHOT
            );
        }

        let checked = project.events(&[EventKind::PageChecked]);
        assert_eq!(checked.len(), 2);
        assert_eq!(
            serde_json::to_value(&checked[0].body).expect("a body")["body"],
            json!({
                "width": "phone",
                "theme": "dark",
                "path": "/settings",
                "violations": [{
                    "rule": "button-name",
                    "impact": "critical",
                    "target": "button.menu",
                    "help": "Buttons must have discernible text"
                }],
                "screenshot": "session-1-phone-dark.png"
            })
        );
        let ids = &checked[0].envelope.ids;
        assert_eq!(ids.task_id.as_ref().map(|id| id.as_str()), Some("CTV-1"));
        assert_eq!(ids.agent_id.as_deref(), Some("iris"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));

        // The page's words reach the agent inside an untrusted block, Catervas's own beside it.
        assert_eq!(phone["width"], "phone");
        assert_eq!(phone["theme"], "dark");
        assert_eq!(phone["path"], "/settings");
        assert_eq!(phone["screenshot"], "session-1-phone-dark.png");
        assert_eq!(phone["violation_count"], 1);
        let violations = phone["violations"].as_str().expect("text");
        assert!(
            violations.starts_with("<untrusted source=\"page\">\n"),
            "{violations}"
        );
        assert!(violations.ends_with("\n</untrusted>"), "{violations}");
        assert!(violations.contains("button-name"), "{violations}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_outside_a_designer_session() {
        let project = a_project("tools-check-page-refused");
        let preview = Arc::new(CheckedPreview::printing(A_VIOLATION));
        let page = json!({ "path": "/", "width": "phone", "theme": "light" });
        let outside = "check_page_refused: only the UI/UX Designer checks a page, in a session \
                       of its task with the preview open";

        let mut architect = project.context("ada", Some("CTV-1"));
        architect.purpose = SessionPurpose::Verify;
        architect.preview = Some(preview.clone());
        assert_eq!(
            refused(run(&architect, "catervas_check_page", page.clone())),
            outside
        );
        let without_preview = project.context("iris", Some("CTV-1"));
        assert_eq!(
            refused(run(&without_preview, "catervas_check_page", page.clone())),
            outside
        );
        let mut without_task = project.context("iris", None);
        without_task.preview = Some(preview.clone());
        assert_eq!(
            refused(run(&without_task, "catervas_check_page", page)),
            outside
        );

        // The path is the page's on the preview: anything else could name another host.
        let mut designer = project.context("iris", Some("CTV-1"));
        designer.preview = Some(preview.clone());
        for path in ["@evil.test", "x"] {
            assert_eq!(
                refused(run(
                    &designer,
                    "catervas_check_page",
                    json!({ "path": path, "width": "phone", "theme": "light" })
                )),
                format!("check_page_refused: a page's path starts with /, and {path:?} does not")
            );
        }

        assert!(preview.runs().is_empty(), "nothing was checked");
        assert!(project.events(&[EventKind::PageChecked]).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn copies_the_latest_check_of_each_into_the_review() {
        let project = a_project("tools-design-review-latest");
        let mut context = project.context("iris", Some("CTV-1"));
        context.purpose = SessionPurpose::Verify;
        let check = |context: &crate::tools::ToolContext, width: &str, theme: &str| {
            run(
                context,
                "catervas_check_page",
                json!({ "path": "/", "width": width, "theme": theme }),
            )
            .expect("the Designer checks a page");
        };
        // Phone light first with a problem, then again once it is fixed.
        context.preview = Some(Arc::new(CheckedPreview::printing(A_VIOLATION)));
        check(&context, "phone", "light");
        context.preview = Some(Arc::new(CheckedPreview::printing(r#"{"violations":[]}"#)));
        for (width, theme) in [
            ("phone", "light"),
            ("phone", "dark"),
            ("desktop", "light"),
            ("desktop", "dark"),
        ] {
            check(&context, width, theme);
        }
        run(
            &context,
            "catervas_record_design_review",
            json!({ "pass": true, "reasons": "Reads well now." }),
        )
        .expect("the review records");

        let recorded = project.events(&[EventKind::DesignReviewRecorded]);
        let body = serde_json::to_value(&recorded[0].body).expect("a body");
        assert_eq!(
            body["body"]["checks"][0],
            json!({ "width": "phone", "theme": "light", "violations": [] })
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_incomplete_design_review() {
        let project = a_project("tools-design-review");
        let preview = Arc::new(CheckedPreview::printing(A_VIOLATION));
        let mut context = project.context("iris", Some("CTV-1"));
        context.purpose = SessionPurpose::Verify;
        context.preview = Some(preview);
        let review = json!({ "pass": false, "reasons": "The menu button has no name." });
        let check = |width: &str, theme: &str| {
            run(
                &context,
                "catervas_check_page",
                json!({ "path": "/", "width": width, "theme": theme }),
            )
            .expect("the Designer checks a page");
        };
        // A check of another session is not this review's.
        project.record(
            "CTV-1",
            "page.checked",
            &json!({
                "width": "desktop", "theme": "dark", "path": "/", "violations": [],
                "screenshot": "earlier-desktop-dark.png"
            }),
        );

        check("phone", "light");
        check("phone", "dark");
        check("desktop", "light");
        check("phone", "light");
        assert_eq!(
            refused(run(
                &context,
                "catervas_record_design_review",
                review.clone()
            )),
            "design_review_incomplete: check each page at both widths in both themes first; \
             missing: desktop dark"
        );
        assert!(
            project
                .events(&[EventKind::DesignReviewRecorded])
                .is_empty()
        );

        check("desktop", "dark");
        run(&context, "catervas_record_design_review", review)
            .expect("all four checks are in: the review records");

        let recorded = project.events(&[EventKind::DesignReviewRecorded]);
        assert_eq!(recorded.len(), 1);
        let violations = json!([{
            "rule": "button-name",
            "impact": "critical",
            "target": "button.menu",
            "help": "Buttons must have discernible text"
        }]);
        let check_of = |width: &str, theme: &str| json!({ "width": width, "theme": theme, "violations": violations });
        assert_eq!(
            serde_json::to_value(&recorded[0].body).expect("a body")["body"],
            json!({
                "pass": false,
                "reasons": "The menu button has no name.",
                "checks": [
                    check_of("phone", "light"),
                    check_of("phone", "dark"),
                    check_of("desktop", "light"),
                    check_of("desktop", "dark"),
                ]
            })
        );
        let ids = &recorded[0].envelope.ids;
        assert_eq!(ids.agent_id.as_deref(), Some("iris"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));

        // Only the Designer records one, in its design review of the task.
        let mut implementing = project.context("iris", Some("CTV-1"));
        implementing.purpose = SessionPurpose::Implement;
        let mut architect = project.context("ada", Some("CTV-1"));
        architect.purpose = SessionPurpose::Verify;
        for outside in [implementing, architect] {
            assert!(
                refused(run(
                    &outside,
                    "catervas_record_design_review",
                    json!({ "pass": true, "reasons": "Fine." })
                ))
                .starts_with("design_review_refused: "),
            );
        }
        assert_eq!(
            refused(run(
                &context,
                "catervas_record_design_review",
                json!({ "pass": true, "reasons": " " })
            )),
            "blank_reason: a reason is recorded, and the log is where somebody reads it back"
        );
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
        assert_eq!(ids.task_id.as_ref().map(|id| id.as_str()), Some("CTV-1"));
        assert_eq!(ids.agent_id.as_deref(), Some("iris"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_the_decision_but_from_the_product_manager() {
        let project = a_project("tools-design-decide");
        project.record(
            "CTV-1",
            "design_plan.proposed",
            &json!({ "plan": a_plan(40, 400) }),
        );
        let outside = "design_plan_refused: only the Product Manager decides a design plan, in \
                       its verify session of the task";
        assert_eq!(
            refused(decide(
                &project,
                "ada",
                Some("CTV-1"),
                SessionPurpose::Verify
            )),
            outside
        );
        // The Designer never decides its own plan, in whatever session.
        for purpose in [SessionPurpose::Explore, SessionPurpose::Verify] {
            assert_eq!(
                refused(decide(&project, "iris", Some("CTV-1"), purpose)),
                outside
            );
        }
        assert_eq!(
            refused(decide(
                &project,
                "pm",
                Some("CTV-1"),
                SessionPurpose::Implement
            )),
            outside
        );
        assert_eq!(
            refused(decide(&project, "pm", None, SessionPurpose::Verify)),
            outside
        );
        assert!(project.events(&[EventKind::DesignPlanApproved]).is_empty());

        decide(&project, "pm", Some("CTV-1"), SessionPurpose::Verify)
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
                Some("CTV-1"),
                SessionPurpose::Verify
            )),
            "design_plan_refused: the task has no plan waiting for a decision"
        );
        // With the next plan waiting, a blank reason is still refused.
        project.record(
            "CTV-1",
            "design_plan.proposed",
            &json!({ "plan": a_plan(40, 400) }),
        );
        assert_eq!(
            refused(call(
                &project,
                "pm",
                Some("CTV-1"),
                SessionPurpose::Verify,
                "catervas_decide_design_plan",
                json!({ "approve": false, "reason": " " }),
            )),
            "blank_reason: a reason is recorded, and the log is where somebody reads it back"
        );
        assert!(project.events(&[EventKind::DesignPlanReturned]).is_empty());
    }
}
