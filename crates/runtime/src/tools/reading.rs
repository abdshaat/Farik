//! The reading tools: a task, the board, the rules, and the criterion library, as the command
//! line's `--json` prints them.

use farik_core::contract::TaskId;
use farik_store::requests::{board_json, board_row_json, criteria_json, rules_json};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Call, ToolError, failed};

/// `farik_read_task`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadTaskInput {
    /// The task to read, `FRK-<n>`.
    task_id: String,
}

/// The contract on disk and the board's row, whose status is the one that counts (8.4).
pub(super) fn read_task(call: &Call<'_>, input: &ReadTaskInput) -> Result<Value, ToolError> {
    let task: TaskId = input
        .task_id
        .parse()
        .map_err(|error| ToolError::InvalidInput {
            detail: format!("{} is not a task id: {error}", input.task_id),
        })?;
    let row = call.row(&task)?;
    let contract = call.deps().files.read_contract(&task).map_err(failed)?;
    Ok(json!({
        "contract": serde_json::to_value(&contract).map_err(failed)?,
        "board": board_row_json(&row),
    }))
}

pub(super) fn read_board(call: &Call<'_>) -> Result<Value, ToolError> {
    Ok(board_json(
        &call.deps().projections.board().map_err(failed)?,
    ))
}

pub(super) fn read_rules(call: &Call<'_>) -> Value {
    rules_json(&call.team.rules())
}

pub(super) fn read_criteria(call: &Call<'_>) -> Result<Value, ToolError> {
    Ok(criteria_json(
        &call.deps().files.read_criteria().map_err(failed)?,
    ))
}

#[cfg(test)]
mod tests {
    use farik_store::requests::{board_json, criteria_json, rules_json};
    use serde_json::json;

    use crate::tools::fixtures::{TestProject, a_team_of_three};

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_a_task_with_its_board_row() {
        let project = TestProject::new("tools-read-task", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "draft", "task", None);
        project.moved("FRK-1", "draft", "refining", &json!({}));
        let read = project
            .call(
                "dev-a",
                None,
                "farik_read_task",
                json!({ "task_id": "FRK-1" }),
            )
            .expect("the task reads");
        assert_eq!(read["contract"]["id"], "FRK-1");
        assert_eq!(read["contract"]["status"], "draft", "the file as it is");
        assert_eq!(read["board"]["status"], "refining", "the board's status");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_board_the_rules_and_the_criteria() {
        let project = TestProject::new(
            "tools-read-all",
            &a_team_of_three(|wire| wire["rules"] = json!({ "forbidden_commands": ["curl"] })),
        );
        project.filed("FRK-1", "draft", "task", None);
        let deps = &project.deps;
        assert_eq!(
            project.call("pm", None, "farik_read_board", json!({})),
            Ok(board_json(&deps.projections.board().expect("a board")))
        );
        let team = deps.files.read_team().expect("a team");
        let rules = project
            .call("pm", None, "farik_read_rules", json!({}))
            .expect("the rules read");
        assert_eq!(rules, rules_json(&team.rules()));
        assert_eq!(rules["forbidden_commands"], json!(["curl"]));
        assert_eq!(
            project.call("pm", None, "farik_read_criteria", json!({})),
            Ok(criteria_json(
                &deps.files.read_criteria().expect("a library")
            ))
        );
        assert_eq!(
            project
                .call("pm", None, "farik_read_board", json!({}))
                .expect("a board")["tasks"][0]["task_id"],
            "FRK-1"
        );
    }
}
