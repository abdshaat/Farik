//! `farik_chat_reply` (ADR 0026): an agent's one answer in its one-to-one chat, the one thing a
//! chat session writes.

use farik_protocol::event::EventKind;
use farik_store::EventQuery;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::refusal::Refusal;
use super::{Call, ToolError, failed};
use crate::chat::{ChatError, NewChatMessage, ProposedRequest, post_chat};
use crate::session::SessionPurpose;

/// `farik_chat_reply`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatReplyInput {
    /// Your answer: 1 to 4,000 characters; line breaks are kept.
    pub text: String,
    /// The request you propose, when work is needed; the user sends it, or not.
    pub request: Option<ProposedRequest>,
}

fn refused(detail: impl Into<String>) -> ToolError {
    Refusal::ChatReplyRefused {
        detail: detail.into(),
    }
    .into()
}

/// Records the agent's reply in its chat, answering the user's message the session was started
/// for, and tells the agent its turn is over. Refused outside a `chat` session and after the
/// session's one reply; a reply refused for its limits does not use that one up.
pub(super) fn chat_reply(call: &Call<'_>, input: ChatReplyInput) -> Result<Value, ToolError> {
    let context = call.context;
    if context.purpose != SessionPurpose::Chat {
        return Err(refused(
            "farik_chat_reply answers the user in a chat session, and this is not one",
        ));
    }
    let deps = call.deps();
    let session = Some(context.session_id.as_str());
    let replied = deps
        .log
        .read(&EventQuery {
            agent_id: Some(call.agent_id().to_string()),
            kinds: vec![EventKind::ChatMessagePosted],
            after_seq: context.in_reply_to,
            ..EventQuery::default()
        })
        .map_err(failed)?
        .iter()
        .any(|event| event.envelope.ids.session_id.as_deref() == session);
    if replied {
        return Err(refused(
            "a chat session sends its one reply, and this one has",
        ));
    }
    let seq = post_chat(
        &deps.log,
        deps.clock.as_ref(),
        &deps.ids,
        NewChatMessage {
            chat: call.agent_id().to_string(),
            author: call.agent_id().to_string(),
            text: input.text,
            in_reply_to: context.in_reply_to,
            request: input.request,
            session_id: Some(context.session_id.clone()),
        },
    )
    .map_err(|error| match error {
        ChatError::Refused { reason } => refused(reason),
        ChatError::Store(error) => failed(error),
    })?;
    Ok(json!({ "seq": seq, "said": "Sent. Your turn is over." }))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use farik_protocol::event::{EventBody, EventKind};
    use serde_json::json;

    use crate::session::SessionPurpose;
    use crate::tools::ToolError;
    use crate::tools::fixtures::{TestProject, a_team_of_three, run};

    fn refused_as_chat_reply(refused: &ToolError) -> bool {
        matches!(refused, ToolError::Refused { reason } if reason.starts_with("chat_reply_refused: "))
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_the_reply() {
        let project = TestProject::new("chat-reply", &a_team_of_three(|_| {}));
        let asked = crate::chat::post_chat(
            &project.deps.log,
            project.deps.clock.as_ref(),
            &project.deps.ids,
            crate::chat::NewChatMessage {
                chat: "pm".to_string(),
                author: "human".to_string(),
                text: "Could customers also pay with Apple Pay?".to_string(),
                in_reply_to: None,
                request: None,
                session_id: None,
            },
        )
        .expect("the question is recorded");
        let mut context = project.context("pm", None);
        context.purpose = SessionPurpose::Chat;
        context.in_reply_to = Some(asked);
        let reply = |input| run(&context, "farik_chat_reply", input);

        // A request outside its limits is refused, and does not use up the one reply.
        let too_short = reply(json!({
            "text": "Not yet.",
            "request": { "title": "Apple Pay", "text": "Too short." }
        }))
        .expect_err("a request's text is 20 characters at least");
        assert!(refused_as_chat_reply(&too_short), "{too_short:?}");
        let answered = reply(json!({
            "text": "Not yet.\n\nI can put it on the board.",
            "request": {
                "title": "Let customers pay with Apple Pay",
                "text": "Offer Apple Pay at checkout beside the card form."
            }
        }))
        .expect("the reply is recorded");
        assert_eq!(answered["said"], "Sent. Your turn is over.", "{answered}");
        let before = project.event_count();
        let second = reply(json!({ "text": "And another thing." }))
            .expect_err("a chat session replies once");
        assert!(refused_as_chat_reply(&second), "{second:?}");
        // Not from any other session.
        let mut conversation = project.context("pm", None);
        conversation.purpose = SessionPurpose::Conversation;
        conversation.session_id = "session-2".to_string();
        let elsewhere = run(
            &conversation,
            "farik_chat_reply",
            json!({ "text": "Hello." }),
        )
        .expect_err("only a chat session replies in a chat");
        assert!(refused_as_chat_reply(&elsewhere), "{elsewhere:?}");
        assert_eq!(project.event_count(), before, "nothing more is recorded");

        let replies = project.events(&[EventKind::ChatMessagePosted]);
        assert_eq!(replies.len(), 2, "the question and one reply: {replies:?}");
        let EventBody::ChatMessagePosted(body) = &replies[1].body else {
            panic!("a chat message");
        };
        assert_eq!((body.chat.as_str(), body.author.as_str()), ("pm", "pm"));
        assert_eq!(body.text, "Not yet.\n\nI can put it on the board.");
        assert_eq!(body.in_reply_to.map(NonZeroU64::get), Some(asked));
        assert_eq!(
            body.request.as_ref().map(|request| request.title.as_str()),
            Some("Let customers pay with Apple Pay")
        );
        assert_eq!(
            replies[1].envelope.ids.session_id.as_deref(),
            Some("session-1")
        );
        assert_eq!(replies[1].envelope.ids.agent_id.as_deref(), Some("pm"));
    }
}
