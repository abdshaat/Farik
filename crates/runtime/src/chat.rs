//! One-to-one chats (`docs/SPEC.md` 4.3, 5.9; ADR 0026): what the user and one agent said to each
//! other, kept in the log as `chat_message.posted` and never in the team's channel.

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use chrono::{DateTime, Utc};
use farik_core::contract::Role;
use farik_core::team::{AgentStatus, Team};

use farik_protocol::clock::Clock;
use farik_protocol::event::{
    ChatMessagePostedBody, EventBody, EventIds, EventKind, FarikEvent, SessionStartedBodyPurpose,
    new_event,
};
use farik_store::{EventLog, EventQuery, Projections, StoreError};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::cost::CostError;

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
/// unless a chat session of that agent answers it or a later one, whoever wrote last. A message
/// sent while a session ran is past that session's, so it is answered next; a failed session is
/// not retried, since its `in_reply_to` answers the message it was started for.
///
/// Every tick asks this of every agent, so it reads back from the newest message alone, by the
/// log's index on the agent and the kind: a session that answers the newest message started after
/// it, so only the sessions since are read.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn pending_chat(log: &EventLog, agent_id: &str) -> Result<Option<u64>, StoreError> {
    let Some(newest) = newest_from_the_user(log, agent_id)? else {
        return Ok(None);
    };
    let answered = log
        .read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            kinds: vec![EventKind::SessionStarted],
            after_seq: Some(newest),
            ..EventQuery::default()
        })?
        .iter()
        .any(|event| {
            matches!(&event.body, EventBody::SessionStarted(body)
                if body.purpose == SessionStartedBodyPurpose::Chat
                    && body.in_reply_to.is_some_and(|seq| seq.get() >= newest))
        });
    Ok((!answered).then_some(newest))
}

/// How many of a chat's messages one read takes, newest first, looking for the user's newest.
const CHAT_PAGE: usize = 8;

/// The seq of the user's newest message in `agent_id`'s chat, read a page at a time from the
/// newest back: the agent replies once to each, so it is on the first page but for a long run of
/// replies.
fn newest_from_the_user(log: &EventLog, agent_id: &str) -> Result<Option<u64>, StoreError> {
    let mut before_seq = None;
    loop {
        let page = log.read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            kinds: vec![EventKind::ChatMessagePosted],
            newest_first: true,
            before_seq,
            limit: Some(CHAT_PAGE),
            ..EventQuery::default()
        })?;
        if let Some(found) = page.iter().find(|event| {
            matches!(&event.body, EventBody::ChatMessagePosted(body) if body.author == HUMAN)
        }) {
            return Ok(Some(found.envelope.seq));
        }
        match page.last() {
            Some(oldest) if page.len() == CHAT_PAGE => before_seq = Some(oldest.envelope.seq),
            _ => return Ok(None),
        }
    }
}

/// Why the user's newest message in a chat has no answer yet (ADR 0026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatWaiting {
    /// The agent has retired; its chat is read-only.
    Retired,
    /// The model provider refused the AI account's key, and the team is paused for it.
    KeyRefused,
    /// The team's daily budget is spent: the message is kept, and answered another day.
    DaySpent,
    /// The agent sleeps until its model provider's limit resets.
    Asleep {
        /// When it wakes.
        until: DateTime<Utc>,
    },
    /// A session is answering, or is about to.
    Answering,
    /// A session ran and ended without a reply: the user asks again.
    NoAnswer,
}

impl ChatWaiting {
    /// The reason as the wire words it.
    #[must_use]
    pub fn because(&self) -> &'static str {
        match self {
            Self::Retired => "retired",
            Self::KeyRefused => "key_refused",
            Self::DaySpent => "day_spent",
            Self::Asleep { .. } => "asleep",
            Self::Answering => "answering",
            Self::NoAnswer => "no_answer",
        }
    }
}

/// Why `agent_id`'s chat waits, when the user wrote last; the first reason that applies, in the
/// order of `ChatWaiting`. `None` when the agent answered, or nobody wrote.
///
/// # Errors
///
/// `CostError` when the log or the costs cannot be read.
pub fn chat_waiting(
    log: &EventLog,
    projections: &Projections,
    team: &Team,
    agent_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<ChatWaiting>, CostError> {
    let newest = log
        .read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            kinds: vec![EventKind::ChatMessagePosted],
            newest_first: true,
            limit: Some(1),
            ..EventQuery::default()
        })?
        .into_iter()
        .next();
    let user_wrote_last = newest.is_some_and(
        |event| matches!(&event.body, EventBody::ChatMessagePosted(body) if body.author == HUMAN),
    );
    let Some(agent) = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == agent_id)
    else {
        return Ok(None);
    };
    if !user_wrote_last {
        return Ok(None);
    }
    if agent.status == AgentStatus::Retired {
        return Ok(Some(ChatWaiting::Retired));
    }
    if crate::pause::key_refused(log)? {
        return Ok(Some(ChatWaiting::KeyRefused));
    }
    if crate::cost::day_spent(projections, team, Role::from(agent.role), now)? {
        return Ok(Some(ChatWaiting::DaySpent));
    }
    if let Some(until) = crate::sleep::asleep_until(log, agent_id, now)? {
        return Ok(Some(ChatWaiting::Asleep { until }));
    }
    if pending_chat(log, agent_id)?.is_some() || chat_session_runs(log, agent_id)? {
        return Ok(Some(ChatWaiting::Answering));
    }
    Ok(Some(ChatWaiting::NoAnswer))
}

/// Whether a chat session of `agent_id` started and has not ended.
fn chat_session_runs(log: &EventLog, agent_id: &str) -> Result<bool, StoreError> {
    let mut running = BTreeSet::new();
    for event in log.read(&EventQuery {
        agent_id: Some(agent_id.to_string()),
        kinds: vec![EventKind::SessionStarted, EventKind::SessionEnded],
        ..EventQuery::default()
    })? {
        match &event.body {
            EventBody::SessionStarted(body) if body.purpose == SessionStartedBodyPurpose::Chat => {
                running.insert(event.envelope.ids.session_id);
            }
            EventBody::SessionEnded(_) => {
                running.remove(&event.envelope.ids.session_id);
            }
            _ => {}
        }
    }
    Ok(!running.is_empty())
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

    use farik_store::event_log::fixtures::append_unreadable;

    use super::{NewChatMessage, ProposedRequest, pending_chat, post_chat};
    use crate::tools::fixtures::at;

    fn farik_ids() -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            ..EventIds::default()
        }
    }

    /// A chat `session.started` of `agent` answering `in_reply_to`.
    fn chat_started(log: &farik_store::EventLog, agent: &str, in_reply_to: u64) -> u64 {
        let event = event_from_value(&json!({
            "seq": 1, "recorded_at": at().to_rfc3339(), "team_id": "farik", "project_id": "farik",
            "agent_id": agent, "session_id": format!("chat-{in_reply_to}"),
            "kind": "session.started",
            "body": {
                "purpose": "chat", "model": "claude-opus-5", "effort": "low",
                "in_reply_to": in_reply_to,
            },
        }))
        .expect("the fixture is schema-valid");
        log.append(&farik_protocol::event::NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        })
        .expect("appends")
        .envelope
        .seq
    }

    /// The user's or `agent`'s message in `agent`'s chat.
    fn said(log: &farik_store::EventLog, agent: &str, author: &str) -> u64 {
        post_chat(
            log,
            &FixedClock::new(at()),
            &farik_ids(),
            NewChatMessage {
                chat: agent.to_string(),
                author: author.to_string(),
                text: "Status?".to_string(),
                in_reply_to: None,
                request: None,
                session_id: None,
            },
        )
        .expect("the message is recorded")
    }

    #[test]
    fn finds_the_pending_message_without_reading_the_chat_or_sessions_before_it() {
        // Every tick asks this of every agent, so it reads from the newest message back, not the
        // agent's whole history: a row before the newest message is never reached.
        let log = open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens");
        append_unreadable(&log, "mira", EventKind::SessionStarted);
        append_unreadable(&log, "mira", EventKind::ChatMessagePosted);
        assert_eq!(pending_chat(&log, "ari").expect("reads"), None);
        // An earlier exchange, a page's worth, answered.
        for _ in 0..super::CHAT_PAGE / 2 {
            let earlier = said(&log, "mira", "human");
            chat_started(&log, "mira", earlier);
            said(&log, "mira", "mira");
        }

        let asked = said(&log, "mira", "human");
        assert_eq!(pending_chat(&log, "mira").expect("reads"), Some(asked));
        chat_started(&log, "mira", asked);
        assert_eq!(pending_chat(&log, "mira").expect("reads"), None);
        said(&log, "mira", "mira");
        assert_eq!(pending_chat(&log, "mira").expect("reads"), None);

        // Asked again while the agent's other work ran: answered next.
        let again = said(&log, "mira", "human");
        for _ in 0..3 {
            said(&log, "mira", "mira");
        }
        assert_eq!(pending_chat(&log, "mira").expect("reads"), Some(again));
        chat_started(&log, "mira", again);
        assert_eq!(pending_chat(&log, "mira").expect("reads"), None);

        // A message behind more than a page of the agent's is still found.
        let behind = said(&log, "mira", "human");
        for _ in 0..=super::CHAT_PAGE {
            said(&log, "mira", "mira");
        }
        assert_eq!(pending_chat(&log, "mira").expect("reads"), Some(behind));

        // Behind exactly a page: the message is the first row of the next page.
        chat_started(&log, "mira", behind);
        let first_on_the_next_page = said(&log, "mira", "human");
        for _ in 0..super::CHAT_PAGE {
            said(&log, "mira", "mira");
        }
        assert_eq!(
            pending_chat(&log, "mira").expect("reads"),
            Some(first_on_the_next_page)
        );
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
