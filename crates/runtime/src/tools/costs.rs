//! `farik_read_costs` (`docs/SPEC.md` 6.6): the Finance Specialist reads what the team has spent
//! on AI, summed by task, agent, sprint, day or purpose, from the costs the log already keeps.

use chrono::NaiveDate;
use farik_core::contract::Role;
use farik_store::{CostScope, CostWindow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};

/// `farik_read_costs`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadCostsInput {
    /// What to sum by: `task`, `agent`, `sprint`, `day` or `purpose`.
    by: CostsBy,
    /// The first day to count, as YYYY-MM-DD in UTC. Give it with `to`, or neither.
    #[serde(default)]
    from: Option<String>,
    /// The last day to count, as YYYY-MM-DD in UTC, at most 366 days after `from`.
    #[serde(default)]
    to: Option<String>,
}

/// What a sum of the costs groups them by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CostsBy {
    /// By task id.
    Task,
    /// By agent id.
    Agent,
    /// By the sprint the task was in when the cost was recorded.
    Sprint,
    /// By the UTC day the cost was recorded on.
    Day,
    /// By what the session was for, such as `implement` or `chat`.
    Purpose,
}

/// The most days a range of costs spans, counted from its first day to its last.
const MOST_DAYS_APART: i64 = 366;

fn range_invalid(detail: impl Into<String>) -> ToolError {
    Refusal::Finance {
        code: "cost_range_invalid",
        detail: detail.into(),
    }
    .into()
}

/// An ISO date, or `cost_range_invalid` saying which end it was.
fn day(end: &str, text: &str) -> Result<NaiveDate, ToolError> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| range_invalid(format!("`{end}` is {text:?}, and a day is YYYY-MM-DD")))
}

/// The window the input asks for: every cost, or the days from `from` to `to`.
fn window_of(input: &ReadCostsInput) -> Result<CostWindow, ToolError> {
    let (from, to) = match (&input.from, &input.to) {
        (None, None) => return Ok(CostWindow::All),
        (Some(from), Some(to)) => (day("from", from)?, day("to", to)?),
        _ => return Err(range_invalid("give `from` and `to` together, or neither")),
    };
    if from > to {
        return Err(range_invalid(format!("`from` {from} is after `to` {to}")));
    }
    if (to - from).num_days() > MOST_DAYS_APART {
        return Err(range_invalid(format!(
            "`from` and `to` are at most {MOST_DAYS_APART} days apart"
        )));
    }
    Ok(CostWindow::Between(from, to))
}

/// `farik_read_costs`: what the team spent, by the key the input names, for a Finance Specialist
/// in any session. Reads the projections and records nothing.
pub(super) fn read_costs(call: &Call<'_>, input: &ReadCostsInput) -> Result<Value, ToolError> {
    if call.role() != Role::FinanceSpecialist {
        return Err(Refusal::Finance {
            code: "sheet_refused",
            detail: "only the Finance Specialist reads the team's costs".to_string(),
        }
        .into());
    }
    let window = window_of(input)?;
    let scope = match input.by {
        CostsBy::Task => CostScope::Task,
        CostsBy::Agent => CostScope::Agent,
        CostsBy::Sprint => CostScope::Sprint,
        CostsBy::Day => CostScope::Day,
        CostsBy::Purpose => CostScope::Purpose,
    };
    let rows: Vec<Value> = call
        .deps()
        .projections
        .costs_for(scope, window)
        .map_err(failed)?
        .into_iter()
        .map(|cost| {
            json!({
                "key": cost.key,
                "usd": cost.usd,
                "input_tokens": cost.input_tokens,
                "output_tokens": cost.output_tokens,
                "sessions": cost.sessions,
            })
        })
        .collect();
    Ok(json!({ "by": input.by, "rows": rows }))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::tools::ToolError;
    use crate::tools::fixtures::{
        TestProject, a_team_of_three, with_the_finance_specialist, with_the_procurement_specialist,
    };

    /// A project whose finance agent `fin` can read the costs of `FRK-1`, spent by `dev-a` and
    /// `dev-b` over two days, and of a sprint that holds it.
    fn a_project_with_costs(name: &str) -> TestProject {
        let project = TestProject::new(name, &a_team_of_three(with_the_finance_specialist));
        project.filed("FRK-1", "in_progress", "task", None);
        project.open_sprint("S1", None, &["FRK-1"]);
        project.spent("dev-a", Some("FRK-1"), "s1", "2026-10-01", (1.0, 100));
        project.spent("dev-a", Some("FRK-1"), "s2", "2026-10-02", (2.0, 200));
        project.spent("dev-b", Some("FRK-1"), "s3", "2026-10-02", (4.0, 400));
        project
    }

    fn rows(answer: &Value) -> Vec<Value> {
        answer["rows"].as_array().expect("rows").clone()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_costs_by_each_scope() {
        let project = a_project_with_costs("costs-scopes");
        let read = |input: Value| {
            project
                .call("fin", None, "farik_read_costs", input)
                .expect("the costs are read")
        };

        let agents = read(json!({ "by": "agent" }));
        assert_eq!(agents["by"], "agent");
        assert_eq!(
            rows(&agents),
            vec![
                json!({ "key": "dev-a", "usd": 3.0, "input_tokens": 300, "output_tokens": 30, "sessions": 2 }),
                json!({ "key": "dev-b", "usd": 4.0, "input_tokens": 400, "output_tokens": 40, "sessions": 1 }),
            ]
        );
        assert_eq!(
            rows(&read(json!({ "by": "day" }))),
            vec![
                json!({ "key": "2026-10-01", "usd": 1.0, "input_tokens": 100, "output_tokens": 10, "sessions": 1 }),
                json!({ "key": "2026-10-02", "usd": 6.0, "input_tokens": 600, "output_tokens": 60, "sessions": 2 }),
            ]
        );
        for (by, key) in [
            ("task", "FRK-1"),
            ("sprint", "S1"),
            ("purpose", "implement"),
        ] {
            let answer = read(json!({ "by": by }));
            let rows = rows(&answer);
            assert_eq!(rows.len(), 1, "{by}: {rows:?}");
            assert_eq!(rows[0]["key"], key, "{by}");
            assert_eq!(rows[0]["usd"], 7.0, "{by}");
            assert_eq!(rows[0]["sessions"], 3, "{by}");
        }
        // A range keeps the days between its ends, both included.
        assert_eq!(
            rows(&read(
                json!({ "by": "agent", "from": "2026-10-02", "to": "2026-10-02" })
            )),
            vec![
                json!({ "key": "dev-a", "usd": 2.0, "input_tokens": 200, "output_tokens": 20, "sessions": 1 }),
                json!({ "key": "dev-b", "usd": 4.0, "input_tokens": 400, "output_tokens": 40, "sessions": 1 }),
            ]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn procurement_is_not_given_the_costs() {
        // The team's AI costs stay the Finance Specialist's (6.6; the founder's answer of
        // 2026-10-07 to step 10b's review), though the Procurement Specialist has a folder too.
        let project = TestProject::new(
            "costs-not-procurement",
            &a_team_of_three(|wire| {
                with_the_finance_specialist(wire);
                with_the_procurement_specialist(wire);
            }),
        );
        project.filed("FRK-1", "in_progress", "task", None);
        let before = project.event_count();
        for task in [None, Some("FRK-1")] {
            let refused = project
                .call("proc", task, "farik_read_costs", json!({ "by": "agent" }))
                .expect_err("only the Finance Specialist reads the costs");
            assert!(
                matches!(&refused, ToolError::Refused { reason } if reason.starts_with("sheet_refused: ")),
                "{task:?}: {refused:?}"
            );
        }
        assert_eq!(project.event_count(), before, "a read records nothing");
        project
            .call("fin", None, "farik_read_costs", json!({ "by": "agent" }))
            .expect("the Finance Specialist still reads them");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_range_and_another_role() {
        let project = a_project_with_costs("costs-refused");
        let before = project.event_count();
        let invalid = |input: Value| {
            let refused = project
                .call("fin", None, "farik_read_costs", input.clone())
                .expect_err("the range is refused");
            assert!(
                matches!(&refused, ToolError::Refused { reason } if reason.starts_with("cost_range_invalid: ")),
                "{input}: {refused:?}"
            );
        };
        invalid(json!({ "by": "day", "from": "2026-10-01" }));
        invalid(json!({ "by": "day", "to": "2026-10-01" }));
        invalid(json!({ "by": "day", "from": "2026-10-02", "to": "2026-10-01" }));
        invalid(json!({ "by": "day", "from": "2026-01-01", "to": "2027-01-03" }));
        invalid(json!({ "by": "day", "from": "2026-13-01", "to": "2026-13-02" }));
        invalid(json!({ "by": "day", "from": "yesterday", "to": "today" }));
        // 366 days apart is the most a range holds.
        project
            .call(
                "fin",
                None,
                "farik_read_costs",
                json!({ "by": "day", "from": "2026-01-01", "to": "2027-01-02" }),
            )
            .expect("366 days apart is within the range");

        let refused = project
            .call("pm", None, "farik_read_costs", json!({ "by": "agent" }))
            .expect_err("only the Finance Specialist reads the costs");
        assert!(
            matches!(&refused, ToolError::Refused { reason } if reason.starts_with("sheet_refused: ")),
            "{refused:?}"
        );
        assert_eq!(project.event_count(), before, "a read records nothing");
    }
}
