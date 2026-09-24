//! The team's channel (`docs/SPEC.md` 5.9): what somebody said, kept in the log as
//! `message.posted` and nowhere else.

use farik_core::contract::TaskId;
use farik_core::team::Team;
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    EventBody, EventIds, MessageKind, MessagePostedBody, Thread, new_event,
};
use farik_store::{EventLog, StoreError};

/// The most a message holds, in characters (5.9).
pub const TEXT_LIMIT: usize = 2_000;

/// The agents `text` names as `@<id>`, in the order first named, each once: only agents of the
/// team, active or not, and never the author. `@human` names nobody, since the human reads the
/// channel.
#[must_use]
pub fn mentions_in(text: &str, team: &Team, author: &str) -> Vec<String> {
    let mut mentions: Vec<String> = Vec::new();
    let mut before = ' ';
    for (at, character) in text.char_indices() {
        // An `@` inside a word is an address, not a mention.
        if character == '@' && !before.is_alphanumeric() {
            let rest = &text[at + 1..];
            let end = rest
                .find(|next: char| {
                    !(next.is_ascii_lowercase() || next.is_ascii_digit() || next == '-')
                })
                .unwrap_or(rest.len());
            let id = rest[..end].trim_end_matches('-');
            if id != author
                && !mentions.iter().any(|mentioned| mentioned == id)
                && team.agents.iter().any(|agent| agent.id.as_str() == id)
            {
                mentions.push(id.to_string());
            }
        }
        before = character;
    }
    mentions
}

/// A message to post, as whoever posts it knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMessage {
    /// An agent id, `human`, or `farik`.
    pub author: String,
    /// The agent on the envelope: the author when an agent wrote it, nobody otherwise.
    pub agent_id: Option<String>,
    /// What kind of message it is, decided by Farik rather than the author.
    pub kind: MessageKind,
    /// What was said.
    pub text: String,
    /// The agents it names, from `mentions_in`.
    pub mentions: Vec<String>,
    /// The task it is about, when it is about one.
    pub task_id: Option<TaskId>,
    /// The ceremony's thread it belongs to, when it belongs to one.
    pub thread: Option<Thread>,
    /// The seq of the message it answers, for a reply.
    pub in_reply_to: Option<u64>,
    /// The session it was said from, when it was said from one.
    pub session_id: Option<String>,
}

/// Why a message was not posted.
#[derive(Debug)]
pub enum ChannelError {
    /// The log could not be read or written.
    Store(StoreError),
    /// The message breaks a rule of the channel; `reason` says which.
    Refused {
        /// The rule and what broke it.
        reason: String,
    },
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "the channel's log failed: {error}"),
            Self::Refused { reason } => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for ChannelError {}

impl From<StoreError> for ChannelError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Posts `message` in the channel and answers its seq. Each line break in its text becomes a
/// space, so that a message is one line.
///
/// # Errors
///
/// `Refused` when the text is blank or longer than the channel takes; `Store` when the log fails.
pub fn post(
    log: &EventLog,
    clock: &dyn Clock,
    ids: &EventIds,
    message: NewMessage,
) -> Result<u64, ChannelError> {
    let text = message.text.replace("\r\n", " ").replace(['\n', '\r'], " ");
    if text.trim().is_empty() {
        return Err(ChannelError::Refused {
            reason: "blank_message: a message says something".to_string(),
        });
    }
    let length = text.chars().count();
    if length > TEXT_LIMIT {
        return Err(ChannelError::Refused {
            reason: format!(
                "message_too_long: a message is at most {TEXT_LIMIT} characters, and this one is \
                 {length}"
            ),
        });
    }
    let ids = EventIds {
        task_id: message.task_id,
        agent_id: message.agent_id,
        session_id: message.session_id,
        ..ids.clone()
    };
    let body = EventBody::MessagePosted(MessagePostedBody {
        author: message.author,
        kind: message.kind,
        text,
        mentions: message.mentions,
        thread: message.thread,
        in_reply_to: message.in_reply_to,
    });
    let event = new_event(body, clock.now(), ids).map_err(|error| ChannelError::Refused {
        reason: format!("the message cannot be recorded: {error:?}"),
    })?;
    Ok(log.append(&event)?.envelope.seq)
}

#[cfg(test)]
mod tests {
    use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
    use farik_core::team::validate_team;
    use serde_json::json;

    use std::path::Path;

    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventBody, EventIds, MessageKind};
    use farik_store::{EventQuery, IN_MEMORY, open_event_log};

    use super::{NewMessage, mentions_in, post};
    use crate::tools::fixtures::at;

    #[test]
    fn finds_the_mentions_in_a_message() {
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("pm", "product_manager"),
            an_agent_wire("dev-a", "software_developer"),
            an_agent_wire("dev-b", "software_developer"),
        ]);
        let team = validate_team(&wire).expect("the fixture is a team");

        let found = mentions_in(
            "@dev-a and @dev-b, and @dev-a again, @nobody, @human",
            &team,
            "dev-b",
        );

        assert_eq!(found, ["dev-a"]);
    }

    #[test]
    fn posts_a_message_as_one_line() {
        let log = open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens");
        let ids = EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            ..EventIds::default()
        };

        let seq = post(
            &log,
            &FixedClock::new(at()),
            &ids,
            NewMessage {
                author: "human".to_string(),
                agent_id: None,
                kind: MessageKind::Human,
                text: "one\r\ntwo\nthree\rfour".to_string(),
                mentions: Vec::new(),
                task_id: None,
                thread: None,
                in_reply_to: None,
                session_id: None,
            },
        )
        .expect("posted");

        let posted = log.read(&EventQuery::default()).expect("the log reads");
        assert_eq!(posted[0].envelope.seq, seq);
        let EventBody::MessagePosted(body) = &posted[0].body else {
            panic!("a message");
        };
        assert_eq!(body.text, "one two three four");
    }
}
