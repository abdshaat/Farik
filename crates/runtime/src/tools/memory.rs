//! `farik_write_memory` (`docs/SPEC.md` 5.8): an agent replaces its own notebook,
//! `.farik/agents/<id>/memory.md`, within the team's cap, and the log records what it wrote.

use farik_core::text;
use farik_protocol::event::EventBody;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};

/// `farik_write_memory`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteMemoryInput {
    /// The whole notebook: it replaces what was there. Empty empties it.
    text: String,
}

/// Replaces the calling agent's notebook with `text` and records `memory.written`, unless the text
/// is past the team's `memory_cap_tokens`, when nothing is written or recorded.
pub(super) fn write_memory(call: &Call<'_>, input: &WriteMemoryInput) -> Result<Value, ToolError> {
    let cap = call.team.policy.memory_cap_tokens;
    let tokens = text::tokens(&input.text);
    if i64::try_from(tokens).unwrap_or(i64::MAX) > cap {
        return Err(Refusal::MemoryRefused {
            detail: format!("{tokens} tokens is past your cap of {cap}; prune it"),
        }
        .into());
    }
    let deps = call.deps();
    deps.files
        .write_memory(&call.agent.id, &input.text)
        .map_err(failed)?;
    let body = serde_json::from_value(json!({
        "text": input.text,
        "written_by": call.agent_id(),
    }))
    .map_err(failed)?;
    call.append(None, EventBody::MemoryWritten(body))?;
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use farik_protocol::event::{EventBody, EventKind};

    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three};

    /// The notebook of `agent`, as the file holds it.
    fn memory(project: &TestProject, agent: &str) -> String {
        project
            .deps
            .files
            .read_memory(&agent.parse().expect("an agent id"))
            .expect("the notebook reads")
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn writes_an_agents_memory() {
        let project = TestProject::new("memory-write", &a_team_of_three(|_| {}));

        project
            .call(
                "dev-a",
                None,
                "farik_write_memory",
                json!({ "text": "use pnpm" }),
            )
            .expect("the notebook is written");

        assert_eq!(memory(&project, "dev-a"), "use pnpm");
        let written = project.events(&[EventKind::MemoryWritten]);
        assert_eq!(written.len(), 1);
        let EventBody::MemoryWritten(body) = &written[0].body else {
            panic!("a memory.written");
        };
        assert_eq!(
            (body.text.as_str(), body.written_by.as_str()),
            ("use pnpm", "dev-a")
        );
        assert_eq!(written[0].envelope.ids.task_id, None, "about no task");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_memory_past_its_cap() {
        let project = TestProject::new(
            "memory-cap",
            &a_team_of_three(|wire| wire["policy"]["memory_cap_tokens"] = json!(500)),
        );
        project
            .call(
                "dev-a",
                None,
                "farik_write_memory",
                json!({ "text": "use pnpm" }),
            )
            .expect("the notebook is written");
        let before = project.event_count();

        // 2,001 characters are 501 tokens, one past the cap.
        let refused = project
            .call(
                "dev-a",
                None,
                "farik_write_memory",
                json!({ "text": "a".repeat(2_001) }),
            )
            .expect_err("the cap refuses it");

        assert_eq!(
            refused,
            ToolError::Refused {
                reason: "memory_refused: 501 tokens is past your cap of 500; prune it".to_string()
            }
        );
        assert_eq!(
            memory(&project, "dev-a"),
            "use pnpm",
            "the file is unchanged"
        );
        assert_eq!(project.event_count(), before, "nothing is recorded");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn empties_a_memory() {
        let project = TestProject::new("memory-empty", &a_team_of_three(|_| {}));
        project
            .call(
                "dev-a",
                None,
                "farik_write_memory",
                json!({ "text": "use pnpm" }),
            )
            .expect("the notebook is written");

        project
            .call("dev-a", None, "farik_write_memory", json!({ "text": "" }))
            .expect("an empty notebook is allowed");

        assert_eq!(memory(&project, "dev-a"), "");
        let written = project.events(&[EventKind::MemoryWritten]);
        assert_eq!(written.len(), 2);
        let EventBody::MemoryWritten(body) = &written[1].body else {
            panic!("a memory.written");
        };
        assert_eq!(body.text, "");
    }
}
