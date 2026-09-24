//! `farik_post_message` (`docs/SPEC.md` 5.9): an agent says something in the team's channel from
//! its session, and Farik decides what kind of message it is from the session's purpose.

use farik_protocol::event::{EventBody, EventKind, FarikEvent, MessageKind};
use farik_store::EventQuery;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::channel::{ChannelError, NewMessage, mentions_in, post};
use crate::session::SessionPurpose;

/// `farik_post_message`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PostMessageInput {
    /// What to say: one or two sentences.
    text: String,
}

/// Posts the agent's message, about the session's task when it has one, and answers its seq and
/// kind.
pub(super) fn post_message(call: &Call<'_>, input: PostMessageInput) -> Result<Value, ToolError> {
    let deps = call.deps();
    let kind = kind_of(call)?;
    let seq = post(
        &deps.log,
        deps.clock.as_ref(),
        &deps.ids,
        NewMessage {
            author: call.agent_id().to_string(),
            agent_id: Some(call.agent_id().to_string()),
            kind,
            mentions: mentions_in(&input.text, &call.team, call.agent_id()),
            text: input.text,
            task_id: call.context.task_id.clone(),
            thread: None,
            // A conversation's reply names the latest message it was started to answer.
            in_reply_to: (kind == MessageKind::Reply)
                .then_some(call.context.in_reply_to)
                .flatten(),
            session_id: Some(call.context.session_id.clone()),
        },
    )
    .map_err(|error| match error {
        ChannelError::Refused { reason } => ToolError::Refused { reason },
        other => failed(other),
    })?;
    Ok(json!({ "seq": seq, "kind": kind }))
}

/// The kind the session's next post is: a conversation's one `reply`; any other session's first a
/// `reaction`, and every further one `ambient` while the agent's allowance lasts. The allowance
/// counts the agent's ambient messages since the open sprint started, or since the UTC day began
/// when no sprint is open.
fn kind_of(call: &Call<'_>) -> Result<MessageKind, ToolError> {
    let deps = call.deps();
    let posted = deps
        .log
        .read(&EventQuery {
            agent_id: Some(call.agent_id().to_string()),
            kinds: vec![EventKind::MessagePosted],
            ..EventQuery::default()
        })
        .map_err(failed)?;
    let session = Some(call.context.session_id.as_str());
    let first = !posted
        .iter()
        .any(|event| event.envelope.ids.session_id.as_deref() == session);
    if call.context.purpose == SessionPurpose::Conversation {
        return if first {
            Ok(MessageKind::Reply)
        } else {
            Err(Refusal::ChannelLimit {
                detail: "a conversation session posts its one reply".to_string(),
            }
            .into())
        };
    }
    if first {
        return Ok(MessageKind::Reaction);
    }
    let started = match deps.projections.open_sprint().map_err(failed)? {
        Some(sprint) => deps
            .log
            .read(&EventQuery {
                kinds: vec![EventKind::SprintStarted],
                ..EventQuery::default()
            })
            .map_err(failed)?
            .iter()
            .rev()
            .find(|event| {
                matches!(&event.body, EventBody::SprintStarted(body) if body.sprint_id.as_str() == sprint.sprint_id)
            })
            .map(|event| event.envelope.seq),
        None => None,
    };
    let day = deps
        .clock
        .now()
        .date_naive()
        .and_time(chrono::NaiveTime::MIN)
        .and_utc();
    let in_window = |event: &FarikEvent| match started {
        Some(seq) => event.envelope.seq > seq,
        None => event.envelope.recorded_at >= day,
    };
    let spent = posted
        .iter()
        .filter(|event| {
            in_window(event)
                && matches!(&event.body, EventBody::MessagePosted(body) if body.kind == MessageKind::Ambient)
        })
        .count();
    let allowance = usize::try_from(call.team.policy.ambient_messages_per_sprint).unwrap_or(0);
    if spent >= allowance {
        let window = if started.is_some() {
            "this sprint"
        } else {
            "today, with no sprint open"
        };
        return Err(Refusal::ChannelLimit {
            detail: format!(
                "{} has said its {allowance} unprompted message(s) {window}",
                call.agent_id()
            ),
        }
        .into());
    }
    Ok(MessageKind::Ambient)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventBody, EventKind, FarikEvent, MessageKind};
    use serde_json::json;

    use crate::channel::{NewMessage, post};
    use crate::orchestrator::fixtures::Harness;
    use crate::recorded::fixtures::implement_reacts_frk_1;
    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three, at, run};

    fn kinds(messages: &[FarikEvent]) -> Vec<MessageKind> {
        messages
            .iter()
            .filter_map(|event| match &event.body {
                EventBody::MessagePosted(body) => Some(body.kind),
                _ => None,
            })
            .collect()
    }

    fn say(project: &TestProject, text: &str) -> Result<serde_json::Value, ToolError> {
        project.call(
            "dev-a",
            Some("FRK-1"),
            "farik_post_message",
            json!({ "text": text }),
        )
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn posts_a_reaction_from_a_session() {
        let harness = Harness::new("channel-reaction", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        let adapter = harness.recorded(vec![implement_reacts_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the session runs");

        let messages = harness.events(&[EventKind::MessagePosted]);
        assert_eq!(messages.len(), 1, "{messages:?}");
        let EventBody::MessagePosted(body) = &messages[0].body else {
            panic!("a message");
        };
        assert_eq!(body.kind, MessageKind::Reaction);
        assert_eq!(body.author, "dev-a");
        assert_eq!(
            messages[0]
                .envelope
                .ids
                .task_id
                .as_ref()
                .map(|task| task.as_str()),
            Some("FRK-1")
        );
        assert_eq!(messages[0].envelope.ids.agent_id.as_deref(), Some("dev-a"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_a_second_post_as_ambient() {
        let project = TestProject::new("channel-ambient", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "in_progress", "task", None);
        project.open_sprint("S1", None, &[]);

        say(&project, "FRK-1 is in review.").expect("the reaction");
        say(&project, "The login page is lovely.").expect("the ambient message");
        let before = project.event_count();
        let refused = say(&project, "And another thing.").expect_err("past the allowance");

        assert!(
            matches!(&refused, ToolError::Refused { reason } if reason.starts_with("channel_limit: ")),
            "{refused:?}"
        );
        assert_eq!(project.event_count(), before, "nothing is recorded");
        assert_eq!(
            kinds(&project.events(&[EventKind::MessagePosted])),
            [MessageKind::Reaction, MessageKind::Ambient]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_the_allowance_per_day_without_a_sprint() {
        let project = TestProject::new("channel-day", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "in_progress", "task", None);
        post(
            &project.deps.log,
            &FixedClock::new(at() - Duration::days(1)),
            &project.deps.ids,
            NewMessage {
                author: "dev-a".to_string(),
                agent_id: Some("dev-a".to_string()),
                kind: MessageKind::Ambient,
                text: "Yesterday's.".to_string(),
                mentions: Vec::new(),
                task_id: None,
                thread: None,
                in_reply_to: None,
                session_id: Some("session-0".to_string()),
            },
        )
        .expect("posted");

        say(&project, "FRK-1 is in review.").expect("the reaction");
        say(&project, "Today's.").expect("today's allowance is whole");

        assert_eq!(
            kinds(&project.events(&[EventKind::MessagePosted])),
            [
                MessageKind::Ambient,
                MessageKind::Reaction,
                MessageKind::Ambient
            ]
        );
    }

    /// dev-a's ambient message `text`, posted at `when` from another session.
    fn ambient_at(project: &TestProject, when: chrono::DateTime<chrono::Utc>, text: &str) {
        post(
            &project.deps.log,
            &FixedClock::new(when),
            &project.deps.ids,
            NewMessage {
                author: "dev-a".to_string(),
                agent_id: Some("dev-a".to_string()),
                kind: MessageKind::Ambient,
                text: text.to_string(),
                mentions: Vec::new(),
                task_id: None,
                thread: None,
                in_reply_to: None,
                session_id: Some("session-0".to_string()),
            },
        )
        .expect("posted");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_the_allowance_per_sprint() {
        // Yesterday's message, inside the open sprint, spends today's allowance too.
        let project = TestProject::new("channel-sprint", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "in_progress", "task", None);
        project.open_sprint("S1", None, &[]);
        ambient_at(
            &project,
            at() - Duration::days(1),
            "Yesterday's, in the sprint.",
        );

        say(&project, "FRK-1 is in review.").expect("the reaction");
        let refused = say(&project, "Today's.").expect_err("the sprint's allowance is spent");

        assert!(
            matches!(&refused, ToolError::Refused { reason } if reason.starts_with("channel_limit: ")),
            "{refused:?}"
        );

        // A message from before the sprint started, even today, spends nothing of it.
        let project = TestProject::new("channel-sprint-before", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "in_progress", "task", None);
        ambient_at(&project, at(), "Before the sprint.");
        project.open_sprint("S1", None, &[]);

        say(&project, "FRK-1 is in review.").expect("the reaction");
        say(&project, "Today's.").expect("the sprint's allowance is whole");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn posts_one_reply_from_a_conversation() {
        let project = TestProject::new("channel-reply", &a_team_of_three(|_| {}));
        let mut context = project.context("dev-a", None);
        context.purpose = SessionPurpose::Conversation;
        let say = |text: &str| run(&context, "farik_post_message", json!({ "text": text }));

        say("I can take it.").expect("the reply");
        let refused = say("And more.").expect_err("a conversation replies once");

        assert!(
            matches!(&refused, ToolError::Refused { reason } if reason.starts_with("channel_limit: ")),
            "{refused:?}"
        );
        assert_eq!(
            kinds(&project.events(&[EventKind::MessagePosted])),
            [MessageKind::Reply]
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn limits_a_conversation_to_one_post() {
        let project = TestProject::new("channel-one-reply", &a_team_of_three(|_| {}));
        let mut context = project.context("dev-a", None);
        context.purpose = SessionPurpose::Conversation;
        context.in_reply_to = Some(7);
        let say = |text: &str| run(&context, "farik_post_message", json!({ "text": text }));

        say("On it.").expect("the reply");
        let before = project.event_count();
        let refused = say("And another.").expect_err("one post");

        assert!(
            matches!(&refused, ToolError::Refused { reason } if reason.starts_with("channel_limit: ")),
            "{refused:?}"
        );
        assert_eq!(project.event_count(), before, "nothing is recorded");
        let posted = project.events(&[EventKind::MessagePosted]);
        let EventBody::MessagePosted(reply) = &posted[0].body else {
            panic!("a message");
        };
        assert_eq!(reply.in_reply_to, Some(7));
    }
}
