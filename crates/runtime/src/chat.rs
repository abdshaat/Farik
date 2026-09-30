//! One-to-one chats (`docs/SPEC.md` 4.3, 5.9; ADR 0026): what the user and one agent said to each
//! other, kept in the log as `chat_message.posted` and never in the team's channel.

use std::num::NonZeroU64;

use farik_protocol::clock::Clock;
use farik_protocol::event::{
    ChatMessagePostedBody, EventBody, EventIds, EventKind, FarikEvent, SessionStartedBodyPurpose,
    new_event,
};
use farik_store::{EventLog, EventQuery, StoreError};
use schemars::JsonSchema;
use serde::Deserialize;

/// The most a chat message's text or a proposed request's text holds, in code points.
pub const CHAT_TEXT_MAX: usize = 4_000;
/// The most a proposed request's title holds, in code points.
const TITLE_MAX: usize = 120;
/// The least a proposed request's text holds, in code points: `request.file`'s floor.
const REQUEST_TEXT_MIN: usize = 20;

/// The request an agent proposes in its reply, for the user to edit and send.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposedRequest {
    /// The request's title: one line, at most 120 characters.
    pub title: String,
    /// What the request asks for: 20 to 4,000 characters.
    pub text: String,
}

/// A chat message to record, as whoever says it knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChatMessage {
    /// The agent whose chat it is, which the envelope names whoever wrote it.
    pub chat: String,
    /// `human`, or the chat's agent.
    pub author: String,
    /// What was said, its line breaks kept.
    pub text: String,
    /// The user's message a reply answers.
    pub in_reply_to: Option<u64>,
    /// The request a reply proposes.
    pub request: Option<ProposedRequest>,
    /// The session a reply was written in.
    pub session_id: Option<String>,
}

/// Why a chat message was not recorded.
#[derive(Debug)]
pub enum ChatError {
    /// The message breaks a rule of the chat; `reason` says which.
    Refused {
        /// The rule and what broke it.
        reason: String,
    },
    /// The log could not be read or written.
    Store(StoreError),
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused { reason } => formatter.write_str(reason),
            Self::Store(error) => write!(formatter, "the chat's log failed: {error}"),
        }
    }
}

impl std::error::Error for ChatError {}

impl From<StoreError> for ChatError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

fn refused(reason: String) -> ChatError {
    ChatError::Refused { reason }
}

/// `text`'s length in code points when it is `min` to `max` of them and not blank; otherwise a
/// refusal naming it as `what`.
fn within(text: &str, what: &str, min: usize, max: usize) -> Result<(), ChatError> {
    if text.trim().is_empty() {
        return Err(refused(format!(
            "blank_{what}: a chat's {what} says something"
        )));
    }
    let length = text.chars().count();
    if length < min || length > max {
        return Err(refused(format!(
            "{what}_out_of_bounds: a chat's {what} is {min} to {max} characters, and this one is \
             {length}"
        )));
    }
    Ok(())
}

/// Records `message` in its agent's chat and answers its seq. The text keeps its line breaks.
///
/// # Errors
///
/// `Refused` when the text or a proposed request is out of its limits; `Store` when the log fails.
pub fn post_chat(
    log: &EventLog,
    clock: &dyn Clock,
    ids: &EventIds,
    message: NewChatMessage,
) -> Result<u64, ChatError> {
    within(&message.text, "message", 1, CHAT_TEXT_MAX)?;
    if let Some(request) = &message.request {
        within(&request.title, "title", 1, TITLE_MAX)?;
        if request.title.contains(['\n', '\r']) {
            return Err(refused(
                "title_out_of_bounds: a proposed request's title is one line".to_string(),
            ));
        }
        within(&request.text, "request", REQUEST_TEXT_MIN, CHAT_TEXT_MAX)?;
    }
    let ids = EventIds {
        agent_id: Some(message.chat.clone()),
        session_id: message.session_id,
        task_id: None,
        ..ids.clone()
    };
    let body = EventBody::ChatMessagePosted(ChatMessagePostedBody {
        chat: message.chat,
        author: message.author,
        text: message.text,
        in_reply_to: message.in_reply_to.and_then(NonZeroU64::new),
        request: message
            .request
            .map(|request| farik_protocol::event::ProposedRequest {
                title: request.title,
                text: request.text,
            }),
    });
    let event = new_event(body, clock.now(), ids)
        .map_err(|error| refused(format!("the chat message cannot be recorded: {error:?}")))?;
    Ok(log.append(&event)?.envelope.seq)
}

/// The newest `limit` messages of `agent_id`'s chat before `before_seq`, oldest first.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn chat_page(
    log: &EventLog,
    agent_id: &str,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<FarikEvent>, StoreError> {
    let mut page = log.read(&EventQuery {
        agent_id: Some(agent_id.to_string()),
        kinds: vec![EventKind::ChatMessagePosted],
        before_seq,
        newest_first: true,
        limit: Some(limit),
        ..EventQuery::default()
    })?;
    page.reverse();
    Ok(page)
}

/// The seq of the user's message `agent_id`'s chat waits to have answered: the user's newest,
/// when it is past the `in_reply_to` of every chat session of that agent, whoever wrote last. A
/// message sent while a session ran is past that session's, so it is answered next; a failed
/// session is not retried, since its `in_reply_to` answers the message it was started for.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn pending_chat(log: &EventLog, agent_id: &str) -> Result<Option<u64>, StoreError> {
    // ponytail: reads the agent's chat and its sessions' starts whole; a projection of each
    // chat's newest user message and answered seq when chats grow long.
    let newest = log
        .read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            kinds: vec![EventKind::ChatMessagePosted],
            newest_first: true,
            ..EventQuery::default()
        })?
        .into_iter()
        .find(|event| {
            matches!(&event.body, EventBody::ChatMessagePosted(body) if body.author == HUMAN)
        })
        .map(|event| event.envelope.seq);
    let Some(newest) = newest else {
        return Ok(None);
    };
    let answered = log
        .read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            kinds: vec![EventKind::SessionStarted],
            ..EventQuery::default()
        })?
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::SessionStarted(body) if body.purpose == SessionStartedBodyPurpose::Chat => {
                body.in_reply_to.map(NonZeroU64::get)
            }
            _ => None,
        })
        .max();
    Ok(answered
        .is_none_or(|answered| newest > answered)
        .then_some(newest))
}

/// Who the user is in a chat.
const HUMAN: &str = "human";

/// The most of a chat a chat session's prompt is shown, in bytes.
const HISTORY_BYTES: usize = 16 * 1024;

/// `agent_id`'s chat as its chat session is shown it (ADR 0026): the newest messages that fit in
/// 16 KiB, oldest first, the newest always. The user's lines are the user's own; the agent's are
/// wrapped `untrusted`, as agent-written text is (ADR 0011). `None` for a chat with nothing in it.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn chat_history(log: &EventLog, agent_id: &str) -> Result<Option<String>, StoreError> {
    let mut lines = Vec::new();
    let mut used = 0;
    // ponytail: reads the whole chat newest first to keep its last 16 KiB; page with
    // `before_seq` when chats grow long.
    for event in log.read(&EventQuery {
        agent_id: Some(agent_id.to_string()),
        kinds: vec![EventKind::ChatMessagePosted],
        newest_first: true,
        ..EventQuery::default()
    })? {
        let EventBody::ChatMessagePosted(body) = event.body else {
            continue;
        };
        let line = if body.author == HUMAN {
            format!("The user:\n{}", body.text)
        } else {
            let said = match &body.request {
                Some(request) => format!(
                    "{}\n\nProposed request: {}\n{}",
                    body.text, request.title, request.text
                ),
                None => body.text,
            };
            format!(
                "You:\n{}",
                crate::prompt::untrusted_block("chat", &said, HISTORY_BYTES)
            )
        };
        used += line.len() + 2;
        if used > HISTORY_BYTES && !lines.is_empty() {
            break;
        }
        lines.push(line);
    }
    lines.reverse();
    Ok((!lines.is_empty()).then(|| lines.join("\n\n")))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventBody, EventIds, EventKind, event_from_value, event_to_value};
    use farik_store::{EventQuery, IN_MEMORY, open_event_log};
    use serde_json::json;

    use super::{NewChatMessage, ProposedRequest, post_chat};
    use crate::tools::fixtures::at;

    fn farik_ids() -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            ..EventIds::default()
        }
    }

    #[test]
    fn records_a_chat_message() {
        let log = open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens");
        let clock = FixedClock::new(at());
        let asked = post_chat(
            &log,
            &clock,
            &farik_ids(),
            NewChatMessage {
                chat: "mira".to_string(),
                author: "human".to_string(),
                text: "Could customers\npay with Apple Pay?".to_string(),
                in_reply_to: None,
                request: None,
                session_id: None,
            },
        )
        .expect("the user's message is recorded");
        let replied = post_chat(
            &log,
            &clock,
            &farik_ids(),
            NewChatMessage {
                chat: "mira".to_string(),
                author: "mira".to_string(),
                text: "Not yet.\n\nI can propose it.".to_string(),
                in_reply_to: Some(asked),
                request: Some(ProposedRequest {
                    title: "Let customers pay with Apple Pay".to_string(),
                    text: "Add Apple Pay at checkout, beside the card form.".to_string(),
                }),
                session_id: Some("session-1".to_string()),
            },
        )
        .expect("the reply is recorded");

        let chats = log
            .read(&EventQuery {
                kinds: vec![EventKind::ChatMessagePosted],
                ..EventQuery::default()
            })
            .expect("the log reads");
        assert_eq!(
            chats
                .iter()
                .map(|event| event.envelope.seq)
                .collect::<Vec<_>>(),
            [asked, replied]
        );
        for event in &chats {
            // The envelope names the chat's agent, on the user's message too (8.5).
            assert_eq!(event.envelope.ids.agent_id.as_deref(), Some("mira"));
            let wire = event_to_value(event);
            assert_eq!(
                &event_from_value(&wire).expect("the event validates"),
                event
            );
        }
        assert_eq!(
            chats[1].envelope.ids.session_id.as_deref(),
            Some("session-1")
        );
        let bodies: Vec<_> = chats
            .iter()
            .map(|event| match &event.body {
                EventBody::ChatMessagePosted(body) => json!(body),
                other => panic!("a chat message, not {other:?}"),
            })
            .collect();
        assert_eq!(
            bodies,
            [
                json!({ "chat": "mira", "author": "human", "text": "Could customers\npay with Apple Pay?" }),
                json!({
                    "chat": "mira",
                    "author": "mira",
                    "text": "Not yet.\n\nI can propose it.",
                    "in_reply_to": asked,
                    "request": {
                        "title": "Let customers pay with Apple Pay",
                        "text": "Add Apple Pay at checkout, beside the card form."
                    }
                }),
            ]
        );
    }
}
