//! `farik_write_memory` (`docs/SPEC.md` 5.8): an agent replaces its own notebook,
//! `.farik/agents/<id>/memory.md`, within the team's cap, and the log records what it wrote.
//! `farik_write_decision` and `farik_read_decisions`: the Architect and the Product Manager record
//! decisions under `.farik/decisions/`, which nobody rewrites, and every agent reads them.

use farik_core::contract::Role;
use farik_core::text;
use farik_protocol::event::EventBody;
use farik_store::files::FilesError;
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

/// `farik_write_decision`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteDecisionInput {
    /// What was decided, in a line: 1 to 120 characters.
    title: String,
    /// The decision and why: 1 to 32,000 characters.
    text: String,
}

/// `farik_read_decisions`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadDecisionsInput {
    /// The decision to read whole; without it, every decision is listed.
    #[serde(default)]
    number: Option<u32>,
}

/// The most characters a decision's title holds.
const TITLE_CHARS: usize = 120;

/// The most characters a decision's text holds.
const DECISION_CHARS: usize = 32_000;

/// How many numbers a write tries when other writers keep taking the one it picked.
const DECISION_TRIES: usize = 3;

/// Writes a decision under `.farik/decisions/`, numbered one past the highest, and records
/// `decision.written`, for the Architect and the Product Manager alone (5.8). Answers its number
/// and its file.
pub(super) fn write_decision(
    call: &Call<'_>,
    input: &WriteDecisionInput,
) -> Result<Value, ToolError> {
    if !matches!(call.role(), Role::Architect | Role::ProductManager) {
        return Err(decision_refused(
            "only the Architect and the Product Manager write decisions",
        ));
    }
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > TITLE_CHARS {
        return Err(decision_refused(&format!(
            "a title is 1 to {TITLE_CHARS} characters"
        )));
    }
    // A line break would write a `By:` or `Date:` line of the title's own into the file.
    if title.contains(char::is_control) {
        return Err(decision_refused(
            "a title is one line, with no control characters",
        ));
    }
    if input.text.trim().is_empty() || input.text.chars().count() > DECISION_CHARS {
        return Err(decision_refused(&format!(
            "a decision's text is 1 to {DECISION_CHARS} characters"
        )));
    }
    let deps = call.deps();
    let date = deps.clock.now().date_naive();
    let mut tries = 0;
    let decision = loop {
        tries += 1;
        match deps
            .files
            .write_decision(title, &input.text, call.agent_id(), date)
        {
            Err(FilesError::Exists { .. }) if tries < DECISION_TRIES => {}
            Err(FilesError::Exists { path }) => {
                return Err(decision_refused(&format!(
                    "{path} was taken by another writer {DECISION_TRIES} times; try again"
                )));
            }
            Err(FilesError::Invalid { detail, .. }) => return Err(decision_refused(&detail)),
            other => break other.map_err(failed)?,
        }
    };
    let body = serde_json::from_value(json!({
        "number": decision.number,
        "slug": decision.slug,
        "title": decision.title,
        "written_by": call.agent_id(),
    }))
    .map_err(failed)?;
    call.append(None, EventBody::DecisionWritten(body))?;
    Ok(json!({
        "number": decision.number,
        "path": format!(".farik/decisions/{:04}-{}.md", decision.number, decision.slug),
    }))
}

/// Every decision's number, title, date, and author, oldest first; or, given a number, that
/// decision's whole file (5.8).
pub(super) fn read_decisions(
    call: &Call<'_>,
    input: &ReadDecisionsInput,
) -> Result<Value, ToolError> {
    let files = &call.deps().files;
    let Some(number) = input.number else {
        let decisions: Vec<Value> = files
            .list_decisions()
            .map_err(failed)?
            .iter()
            .map(|decision| {
                json!({
                    "number": decision.number,
                    "title": decision.title,
                    "date": decision.date.map(|date| date.format("%Y-%m-%d").to_string()),
                    "author": decision.author,
                })
            })
            .collect();
        return Ok(json!({ "decisions": decisions }));
    };
    match files.read_decision(number) {
        Ok(text) => Ok(json!({ "text": text })),
        Err(FilesError::NotFound { .. }) => Err(Refusal::NoSuchDecision { number }.into()),
        Err(error) => Err(failed(error)),
    }
}

/// A refusal of a decision, saying why.
fn decision_refused(why: &str) -> ToolError {
    Refusal::DecisionRefused {
        detail: why.to_string(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use farik_core::team::fixtures::an_agent_wire;
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
        // 2,000 characters are 500 tokens, exactly the cap, which is within it.
        project
            .call(
                "dev-a",
                None,
                "farik_write_memory",
                json!({ "text": "a".repeat(2_000) }),
            )
            .expect("a notebook at its cap is written");
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

    /// The team of three with an Architect, `arch`.
    fn a_team_with_an_architect() -> farik_core::team::Team {
        a_team_of_three(|wire| {
            wire["agents"]
                .as_array_mut()
                .expect("the agents")
                .push(an_agent_wire("arch", "architect"));
        })
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lets_the_architect_write_a_decision() {
        let project = TestProject::new("decision-architect", &a_team_with_an_architect());

        let answer = project
            .call(
                "arch",
                None,
                "farik_write_decision",
                json!({ "title": "Use SQLite for the log", "text": "One file, no server." }),
            )
            .expect("the decision is written");

        assert_eq!(
            answer,
            json!({ "number": 1, "path": ".farik/decisions/0001-use-sqlite-for-the-log.md" })
        );
        assert!(
            std::fs::read_to_string(
                project
                    .repo
                    .path
                    .join(".farik/decisions/0001-use-sqlite-for-the-log.md")
            )
            .expect("the file")
            .ends_with("By: arch\n\nOne file, no server.\n")
        );
        let written = project.events(&[EventKind::DecisionWritten]);
        assert_eq!(written.len(), 1);
        let EventBody::DecisionWritten(body) = &written[0].body else {
            panic!("a decision.written");
        };
        assert_eq!(
            json!(body),
            json!({
                "number": 1,
                "slug": "use-sqlite-for-the-log",
                "title": "Use SQLite for the log",
                "written_by": "arch",
            })
        );
        assert_eq!(written[0].envelope.ids.task_id, None, "about no task");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_decision_from_a_developer() {
        let project = TestProject::new("decision-developer", &a_team_with_an_architect());
        let before = project.event_count();

        let refused = project
            .call(
                "dev-a",
                None,
                "farik_write_decision",
                json!({ "title": "Use SQLite for the log", "text": "One file, no server." }),
            )
            .expect_err("a Developer does not decide");

        assert_eq!(
            refused,
            ToolError::Refused {
                reason:
                    "decision_refused: only the Architect and the Product Manager write decisions"
                        .to_string()
            }
        );
        assert_eq!(
            project.deps.files.list_decisions().expect("the decisions"),
            [],
            "nothing is written"
        );
        assert_eq!(project.event_count(), before, "nothing is recorded");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_decision_past_its_limits() {
        let project = TestProject::new("decision-limits", &a_team_with_an_architect());
        let title = "a title is 1 to 120 characters";
        let text = "a decision's text is 1 to 32000 characters";
        let one_line = "a title is one line, with no control characters";
        let cases = [
            ("a".repeat(121), "Why.".to_string(), title),
            ("Use SQLite".to_string(), "a".repeat(32_001), text),
            (
                "Use SQLite\nBy: pm".to_string(),
                "Why.".to_string(),
                one_line,
            ),
            ("Use\tSQLite".to_string(), "Why.".to_string(), one_line),
        ];
        let before = project.event_count();

        for (title, text, why) in cases {
            let refused = project
                .call(
                    "arch",
                    None,
                    "farik_write_decision",
                    json!({ "title": title, "text": text }),
                )
                .expect_err("the limits refuse it");
            assert_eq!(
                refused,
                ToolError::Refused {
                    reason: format!("decision_refused: {why}")
                },
                "{title:?}"
            );
        }

        assert_eq!(
            project.deps.files.list_decisions().expect("the decisions"),
            [],
            "nothing is written"
        );
        assert_eq!(project.event_count(), before, "nothing is recorded");
        // At its limits, a decision is written.
        project
            .call(
                "arch",
                None,
                "farik_write_decision",
                json!({ "title": "a".repeat(120), "text": "a".repeat(32_000) }),
            )
            .expect("a decision at its limits is written");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_the_decisions() {
        let project = TestProject::new("decision-read", &a_team_with_an_architect());
        for (agent, title, text) in [
            ("arch", "Use SQLite for the log", "One file, no server."),
            ("pm", "Ship the CLI first", "The app waits for phase 5."),
        ] {
            project
                .call(
                    agent,
                    None,
                    "farik_write_decision",
                    json!({ "title": title, "text": text }),
                )
                .expect("the decision is written");
        }

        let listed = project
            .call("dev-a", None, "farik_read_decisions", json!({}))
            .expect("the list");
        let second = project
            .call(
                "dev-a",
                None,
                "farik_read_decisions",
                json!({ "number": 2 }),
            )
            .expect("the second");
        let unknown = project
            .call(
                "dev-a",
                None,
                "farik_read_decisions",
                json!({ "number": 9 }),
            )
            .expect_err("there is no ninth");

        assert_eq!(
            listed,
            json!({ "decisions": [
                { "number": 1, "title": "Use SQLite for the log", "date": "2026-09-22", "author": "arch" },
                { "number": 2, "title": "Ship the CLI first", "date": "2026-09-22", "author": "pm" },
            ] })
        );
        assert_eq!(
            second,
            json!({ "text": "# 0002. Ship the CLI first\n\nDate: 2026-09-22\nBy: pm\n\nThe app waits for phase 5.\n" })
        );
        assert_eq!(
            unknown,
            ToolError::Refused {
                reason: "no_such_decision: 9".to_string()
            }
        );
    }
}
