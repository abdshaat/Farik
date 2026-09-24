//! `farik_append_retro` (`docs/SPEC.md` 5.9): the retro ceremony records what the next planning
//! should know, in `.farik/team/retro.md` and the log.

use farik_protocol::event::{EventBody, EventKind, Thread};
use farik_store::EventQuery;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::ceremonies::ended_sprint;

/// `farik_append_retro`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppendRetroInput {
    /// What the next planning should know: at most 4,000 characters.
    text: String,
}

/// The most a retro says, in characters.
const RETRO_CHARS: usize = 4_000;

/// Appends the retro of the latest ended sprint to `team/retro.md` and records `retro.appended`,
/// once per session, in a retro ceremony session alone. Answers the sprint.
pub(super) fn append_retro(call: &Call<'_>, input: &AppendRetroInput) -> Result<Value, ToolError> {
    if call.context.thread != Some(Thread::Retro) {
        return Err(refused("only the retro ceremony writes team/retro.md"));
    }
    if input.text.trim().is_empty() {
        return Err(refused("the retro says nothing"));
    }
    if input.text.chars().count() > RETRO_CHARS {
        return Err(refused(&format!(
            "a retro is at most {RETRO_CHARS} characters"
        )));
    }
    let deps = call.deps();
    let session = Some(call.context.session_id.as_str());
    let appended = deps
        .log
        .read(&EventQuery {
            agent_id: Some(call.agent_id().to_string()),
            kinds: vec![EventKind::RetroAppended],
            ..EventQuery::default()
        })
        .map_err(failed)?;
    if appended
        .iter()
        .any(|event| event.envelope.ids.session_id.as_deref() == session)
    {
        return Err(refused(
            "this session has appended its retro, and appends once",
        ));
    }
    let sprint = ended_sprint(&deps.log)
        .map_err(failed)?
        .ok_or_else(|| refused("no sprint has ended since the last one started"))?;
    deps.files
        .append_retro(
            &sprint.sprint_id,
            deps.clock.now().date_naive(),
            &input.text,
        )
        .map_err(failed)?;
    let body = serde_json::from_value(json!({
        "sprint_id": sprint.sprint_id,
        "text": input.text,
        "appended_by": call.agent_id(),
    }))
    .map_err(failed)?;
    call.append(None, EventBody::RetroAppended(body))?;
    Ok(json!({ "sprint_id": sprint.sprint_id }))
}

/// A refusal of an append, saying why.
fn refused(why: &str) -> ToolError {
    Refusal::RetroRefused {
        detail: why.to_string(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use farik_protocol::event::Thread;

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three, run};

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_retro_outside_a_retro_ceremony() {
        let project = TestProject::new("retro-outside", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "in_progress", "task", None);
        let before = project.event_count();

        let refused = project
            .call(
                "dev-a",
                Some("FRK-1"),
                "farik_append_retro",
                json!({ "text": "Keep the tasks small." }),
            )
            .expect_err("an implement session writes no retro");

        assert_eq!(
            refused,
            ToolError::Refused {
                reason: "retro_refused: only the retro ceremony writes team/retro.md".to_string()
            }
        );
        assert_eq!(project.event_count(), before, "nothing is recorded");
        assert_eq!(
            project.deps.files.read_retro().expect("the file reads"),
            None
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_blank_or_long_retro() {
        let project = TestProject::new("retro-text", &a_team_of_three(|_| {}));
        project.open_sprint("S1", None, &[]);
        project.record(
            "",
            "sprint.ended",
            &json!({ "sprint_id": "S1", "ended_by": "human", "left": [] }),
        );
        let mut context = project.context("pm", None);
        context.purpose = SessionPurpose::Ceremony;
        context.thread = Some(Thread::Retro);
        let before = project.event_count();

        for text in [" \n ".to_string(), "x".repeat(4_001)] {
            let refused = run(&context, "farik_append_retro", json!({ "text": text }))
                .expect_err("not a retro");
            assert!(
                matches!(&refused, ToolError::Refused { reason } if reason.starts_with("retro_refused: ")),
                "{refused:?}"
            );
        }

        assert_eq!(project.event_count(), before, "nothing is recorded");
        assert_eq!(
            project.deps.files.read_retro().expect("the file reads"),
            None
        );
        run(
            &context,
            "farik_append_retro",
            json!({ "text": "é".repeat(4_000) }),
        )
        .expect("4,000 characters are a retro");
    }
}
