//! `catervas_file_bug` (ADR 0063): in a bug hunt, its assignee files each bug it finds as a request
//! for the Product Manager to refine, with its evidence in fixed words.

use std::fmt::Write as _;

use catervas_core::contract::{TaskId, TaskStatus};
use catervas_core::folders::is_hunt;
use catervas_store::requests::{
    RequestError, bugs_found, file_bug as file_a_bug, placeholder_budget_usd, request_from_brief,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::session::SessionPurpose;

/// The most bugs one hunt files.
pub const MAX_BUGS_PER_HUNT: usize = 10;

/// How bad a bug is.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BugSeverity {
    /// A nuisance.
    Low,
    /// Gets in the way.
    Medium,
    /// Breaks the product or loses the user's work.
    High,
}

impl BugSeverity {
    fn word(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// `catervas_file_bug`'s input. Each text is counted trimmed.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileBugInput {
    /// A short title: 3 to 120 characters.
    title: String,
    /// Where it is: a page, a screen or a file, 1 to 300 characters.
    #[serde(rename = "where")]
    place: String,
    /// The steps that show it: 1 to 10, each 1 to 300 characters.
    steps: Vec<String>,
    /// What should happen: 1 to 1,000 characters.
    expected: String,
    /// What happens: 1 to 1,000 characters.
    actual: String,
    /// `low`, `medium` or `high`.
    severity: BugSeverity,
    /// Text evidence: a URL, a console line, a command and its output; 1 to 5 items, each 1 to
    /// 1,000 characters.
    evidence: Vec<String>,
}

fn refused(code: &str, detail: impl std::fmt::Display) -> ToolError {
    ToolError::Refused {
        reason: format!("{code}: {detail}"),
    }
}

fn out_of_bounds(field: &str, text: &str, least: usize, most: usize) -> Result<(), ToolError> {
    let length = text.trim().chars().count();
    if (least..=most).contains(&length) {
        Ok(())
    } else {
        Err(refused(
            "bug_out_of_bounds",
            format!("{field} is {length} characters; it must be {least} to {most}"),
        ))
    }
}

fn many(field: &str, items: &[String], most: usize, longest: usize) -> Result<(), ToolError> {
    if !(1..=most).contains(&items.len()) {
        return Err(refused(
            "bug_out_of_bounds",
            format!(
                "{field} has {} items; it must have 1 to {most}",
                items.len()
            ),
        ));
    }
    items
        .iter()
        .try_for_each(|item| out_of_bounds(field, item, 1, longest))
}

/// The title as duplicates compare it: trimmed, lower-cased, each run of whitespace one space.
fn comparable(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The request's intent, in fixed words.
fn bug_intent(input: &FileBugInput, hunter: &str, hunt: &TaskId) -> String {
    let mut intent = format!(
        "Fix this bug, found by {hunter} while hunting in {}: {}\n\nWhere: {}\nSteps:\n",
        hunt.as_str(),
        input.actual.trim(),
        input.place.trim()
    );
    for (at, step) in input.steps.iter().enumerate() {
        let _ = writeln!(intent, "{}. {}", at + 1, step.trim());
    }
    let _ = write!(
        intent,
        "Expected: {}\nActual: {}\nSeverity: {}\nEvidence:",
        input.expected.trim(),
        input.actual.trim(),
        input.severity.word()
    );
    for item in &input.evidence {
        let _ = write!(intent, "\n- {}", item.trim());
    }
    intent
}

/// Files the bug as a draft request linked to the hunt.
pub(super) fn file_bug(call: &Call<'_>, input: &FileBugInput) -> Result<Value, ToolError> {
    let deps = call.deps();
    let hunt = call
        .context
        .task_id
        .clone()
        .filter(|_| call.context.purpose == SessionPurpose::Implement)
        .filter(|hunt| {
            deps.files
                .read_contract(hunt)
                .is_ok_and(|contract| is_hunt(&contract))
                && call
                    .row(hunt)
                    .is_ok_and(|row| row.assignee_id.as_deref() == Some(call.agent_id()))
        })
        .ok_or_else(|| {
            refused(
                "bug_refused",
                "only a hunt's assignee files a bug, in the hunt's implement session",
            )
        })?;

    out_of_bounds("title", &input.title, 3, 120)?;
    out_of_bounds("where", &input.place, 1, 300)?;
    many("steps", &input.steps, 10, 300)?;
    out_of_bounds("expected", &input.expected, 1, 1_000)?;
    out_of_bounds("actual", &input.actual, 1, 1_000)?;
    many("evidence", &input.evidence, 5, 1_000)?;

    let found = bugs_found(&deps.log).map_err(failed)?;
    let filed = found.iter().filter(|(_, by)| *by == hunt).count();
    if filed >= MAX_BUGS_PER_HUNT {
        return Err(refused(
            "hunt_bug_limit",
            format!(
                "{} has filed {MAX_BUGS_PER_HUNT} bugs, the most a hunt files; write your \
                 report and finish",
                hunt.as_str()
            ),
        ));
    }
    let title = comparable(&input.title);
    for (bug, _) in &found {
        let row = call.row(bug)?;
        if !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled)
            && comparable(&row.title) == title
        {
            return Err(refused(
                "bug_already_filed",
                format!("{} is open with this title", bug.as_str()),
            ));
        }
    }

    let intent = bug_intent(input, &call.agent.display_name, &hunt);
    let mut wire = request_from_brief(
        input.title.trim(),
        &intent,
        placeholder_budget_usd(&call.team.rules()),
    )
    .map_err(|reason| ToolError::InvalidInput { detail: reason })?;
    // A bug found by a hunt is a fix (ADR 0066); the Product Manager may still change it.
    wire["change"] = json!("fix");
    let bug = file_a_bug(
        &deps.files,
        &deps.log,
        wire,
        call.agent_id(),
        &hunt,
        deps.clock.now(),
        &call.ids(None),
    )
    .map_err(|error| match error {
        RequestError::Refused { reason } => Refusal::RequestRefused { reason }.into(),
        other => failed(other),
    })?;
    deps.projections.catch_up().map_err(failed)?;
    Ok(json!({
        "task_id": bug.id.as_str(),
        "status": "draft",
        "filed": filed + 1,
        "left": MAX_BUGS_PER_HUNT - filed - 1,
    }))
}

#[cfg(test)]
mod tests {
    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    fn a_hunt_project(name: &str) -> TestProject {
        let project = TestProject::new(name, &a_team_of_three(|_| {}));
        project.filed_with("CTV-1", "assigned", "task", None, |wire| {
            wire["assignee_role"] = json!("software_developer");
            wire["allowed_paths"] = json!(["docs/catervas/engineering/bug-hunt.html"]);
        });
        project.moved(
            "CTV-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "pm" }),
        );
        project
    }

    fn a_bug(title: &str) -> Value {
        json!({
            "title": title,
            "where": "Settings, the Profile card",
            "steps": ["Open Settings", "Press Save"],
            "expected": "The name is saved",
            "actual": "Nothing happens and the console logs TypeError",
            "severity": "high",
            "evidence": ["console: TypeError: x is undefined at save.ts:12"]
        })
    }

    fn file(
        project: &TestProject,
        agent: &str,
        task: &str,
        input: Value,
    ) -> Result<Value, ToolError> {
        project.call(agent, Some(task), "catervas_file_bug", input)
    }

    fn reason(refused: &ToolError) -> &str {
        let ToolError::Refused { reason } = refused else {
            panic!("a refusal, not {refused:?}");
        };
        reason
    }

    const TITLE: &str = "The save button does nothing";

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn files_a_bug_with_its_evidence() {
        let project = a_hunt_project("tools-file-bug");
        let answered = file(&project, "dev-a", "CTV-1", a_bug(TITLE)).expect("the hunter files");
        assert_eq!(
            answered,
            json!({ "task_id": "CTV-2", "status": "draft", "filed": 1, "left": 9 })
        );
        let contract = project
            .deps
            .files
            .read_contract(&"CTV-2".parse().expect("an id"))
            .expect("the bug is a contract");
        assert_eq!(contract.title.as_str(), TITLE);
        assert_eq!(
            contract.change,
            Some(catervas_core::generated::task_contract::CatervasTaskContractChange::Fix)
        );
        let hunter = a_team_of_three(|_| {})
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "dev-a")
            .map(|agent| agent.display_name.to_string())
            .expect("dev-a");
        let expected = format!(
            "Fix this bug, found by {hunter} while hunting in CTV-1: Nothing happens and the console logs TypeError\n\n\
             Where: Settings, the Profile card\n\
             Steps:\n1. Open Settings\n2. Press Save\n\
             Expected: The name is saved\n\
             Actual: Nothing happens and the console logs TypeError\n\
             Severity: high\n\
             Evidence:\n- console: TypeError: x is undefined at save.ts:12"
        );
        assert_eq!(contract.intent.as_str(), expected);
        let created = project.events(&[EventKind::TaskCreated]);
        let EventBody::TaskCreated(body) = &created[created.len() - 1].body else {
            panic!("a task.created");
        };
        assert_eq!(body.created_by, "dev-a");
        assert_eq!(
            body.found_by.as_ref().map(|hunt| hunt.as_str()),
            Some("CTV-1")
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn holds_a_hunt_to_its_bugs() {
        let project = a_hunt_project("tools-hunt-bounds");
        file(&project, "dev-a", "CTV-1", a_bug(TITLE)).expect("the first bug");
        let created = || project.events(&[EventKind::TaskCreated]).len();

        let again = file(
            &project,
            "dev-a",
            "CTV-1",
            a_bug("  the SAVE button   does nothing "),
        )
        .expect_err("the same title");
        assert_eq!(
            reason(&again),
            "bug_already_filed: CTV-2 is open with this title"
        );
        project.moved("CTV-2", "draft", "cancelled", &json!({}));
        project
            .deps
            .projections
            .catch_up()
            .expect("the board is up to date");
        file(&project, "dev-a", "CTV-1", a_bug(TITLE)).expect("filed once the first is cancelled");
        for at in 0..8 {
            file(
                &project,
                "dev-a",
                "CTV-1",
                a_bug(&format!("Another bug number {at}")),
            )
            .expect("within the bound");
        }
        let limit =
            file(&project, "dev-a", "CTV-1", a_bug("One bug too many")).expect_err("the eleventh");
        assert!(
            reason(&limit).starts_with("hunt_bug_limit: CTV-1 has filed 10 bugs, "),
            "{}",
            reason(&limit)
        );

        // Only the hunt's assignee, in its implement session.
        let other = file(&project, "dev-b", "CTV-1", a_bug("Another title")).expect_err("dev-b");
        assert!(reason(&other).starts_with("bug_refused: "));
        let refining = project
            .call_in(
                SessionPurpose::Refine,
                "dev-a",
                Some("CTV-1"),
                "catervas_file_bug",
                a_bug("Another title"),
            )
            .expect_err("a refine session");
        assert!(reason(&refining).starts_with("bug_refused: "));
        project.filed_with("CTV-20", "assigned", "task", None, |wire| {
            wire["assignee_role"] = json!("software_developer");
            wire["allowed_paths"] = json!(["src/**"]);
        });
        project.moved(
            "CTV-20",
            "assigned",
            "in_progress",
            &json!({ "assignee": "dev-a", "reviewer": "pm" }),
        );
        let before = created();
        let not_a_hunt =
            file(&project, "dev-a", "CTV-20", a_bug("Another title")).expect_err("not a hunt");
        assert!(reason(&not_a_hunt).starts_with("bug_refused: "));

        let mut empty = a_bug("Yet another");
        empty["steps"] = json!([]);
        let bounds = file(&project, "dev-a", "CTV-1", empty).expect_err("no steps");
        assert!(reason(&bounds).starts_with("bug_out_of_bounds: steps"));
        assert_eq!(created(), before, "no refusal filed anything");
    }
}
