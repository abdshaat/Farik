//! The team's channel (`docs/SPEC.md` 5.9): what somebody said, kept in the log as
//! `message.posted` and nowhere else.

use std::num::NonZeroU64;

use farik_core::contract::TaskId;
use farik_core::team::Team;
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    EventBody, EventIds, EventKind, FarikEvent, MessageKind, MessagePostedBody,
    SessionStartedBodyPurpose, Thread, new_event,
};
use farik_store::files::{FilesError, ProjectFiles};
use farik_store::{EventLog, EventQuery, StoreError};

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
    /// The channel's summary could not be written.
    Files(FilesError),
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
            Self::Files(error) => write!(formatter, "the channel's summary failed: {error}"),
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

impl From<FilesError> for ChannelError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

/// The messages that mention `agent_id` and that it has not been given a conversation about:
/// those after the latest message its last `conversation` session was shown (that session's
/// `in_reply_to`, or its start in a log recorded before it carried one), oldest first. A message
/// posted while that session was starting is after it, so it stays pending. A reply's mentions and
/// a system line's start nothing, so that replies are one deep.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn pending_mentions(log: &EventLog, agent_id: &str) -> Result<Vec<FarikEvent>, StoreError> {
    let since = log
        .read(&EventQuery {
            agent_id: Some(agent_id.to_string()),
            kinds: vec![EventKind::SessionStarted],
            ..EventQuery::default()
        })?
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            EventBody::SessionStarted(body)
                if body.purpose == SessionStartedBodyPurpose::Conversation =>
            {
                Some(body.in_reply_to.map_or(event.envelope.seq, NonZeroU64::get))
            }
            _ => None,
        });
    Ok(log
        .read(&EventQuery {
            after_seq: since,
            kinds: vec![EventKind::MessagePosted],
            ..EventQuery::default()
        })?
        .into_iter()
        .filter(|event| {
            matches!(&event.body, EventBody::MessagePosted(body)
                if !matches!(body.kind, MessageKind::Reply | MessageKind::System)
                    && body.mentions.iter().any(|mentioned| mentioned == agent_id))
        })
        .collect())
}

/// The most tokens the channel's summary holds.
const SUMMARY_TOKENS: usize = 2_000;

/// The tokens of a text of `characters` characters, as the channel counts them: a quarter of
/// them, rounded up.
fn tokens(characters: usize) -> usize {
    characters.div_ceil(4)
}

/// The channel as an agent is shown it, derived with no model: the latest messages that fit
/// `SUMMARY_TOKENS`, oldest first, one line each (`<author> [<thread>]: <text>`). It is written to
/// `.farik/local/channel-summary.md` each time, so that the human can read what the agents saw.
///
/// # Errors
///
/// `Store` when the log cannot be read, `Files` when the summary cannot be written.
pub fn channel_summary(log: &EventLog, files: &ProjectFiles) -> Result<String, ChannelError> {
    let messages = log.read(&EventQuery {
        kinds: vec![EventKind::MessagePosted],
        ..EventQuery::default()
    })?;
    let mut lines: Vec<String> = Vec::new();
    let mut characters = 0;
    for event in messages.iter().rev() {
        let EventBody::MessagePosted(body) = &event.body else {
            continue;
        };
        let thread = body
            .thread
            .map_or_else(String::new, |thread| format!(" [{thread}]"));
        let line = format!("{}{thread}: {}", body.author, body.text);
        // The line, and the line break that joins it to those already held.
        let with_it = characters + line.chars().count() + usize::from(!lines.is_empty());
        if tokens(with_it) > SUMMARY_TOKENS {
            break;
        }
        characters = with_it;
        lines.push(line);
    }
    lines.reverse();
    let summary = lines.join("\n");
    files.write_channel_summary(&summary)?;
    Ok(summary)
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
        in_reply_to: message.in_reply_to.and_then(NonZeroU64::new),
    });
    let event = new_event(body, clock.now(), ids).map_err(|error| ChannelError::Refused {
        reason: format!("the message cannot be recorded: {error:?}"),
    })?;
    Ok(log.append(&event)?.envelope.seq)
}

/// Posts one of Farik's own lines (`author: farik, kind: system`) about `task_id`, cut to the
/// channel's limit ending with `…`, so that a line never fails what it reports. It names nobody,
/// and no agent is on its envelope.
///
/// # Errors
///
/// What `post` answers, for a blank `text` or a failing log.
pub fn post_system(
    log: &EventLog,
    clock: &dyn Clock,
    ids: &EventIds,
    task_id: Option<TaskId>,
    text: &str,
) -> Result<u64, ChannelError> {
    let text = if text.chars().count() > TEXT_LIMIT {
        let mut cut: String = text.chars().take(TEXT_LIMIT - 1).collect();
        cut.push('…');
        cut
    } else {
        text.to_string()
    };
    post(
        log,
        clock,
        ids,
        NewMessage {
            author: "farik".to_string(),
            agent_id: None,
            kind: MessageKind::System,
            text,
            mentions: Vec::new(),
            task_id,
            thread: None,
            in_reply_to: None,
            session_id: ids.session_id.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
    use farik_core::team::validate_team;
    use serde_json::json;

    use std::path::Path;

    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventBody, EventIds, MessageKind};
    use farik_store::{EventLog, EventQuery, IN_MEMORY, open_event_log};

    use super::{NewMessage, mentions_in, pending_mentions, post};
    use crate::recorded::fixtures::a_session_spec;
    use crate::session::{SessionPurpose, SessionSpec};
    use crate::sessions::record_session_started;
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

    fn farik_ids() -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            ..EventIds::default()
        }
    }

    /// The human's message mentioning dev-a, and its seq.
    fn mention(log: &EventLog, text: &str) -> u64 {
        post(
            log,
            &FixedClock::new(at()),
            &farik_ids(),
            NewMessage {
                author: "human".to_string(),
                agent_id: None,
                kind: MessageKind::Human,
                text: text.to_string(),
                mentions: vec!["dev-a".to_string()],
                task_id: None,
                thread: None,
                in_reply_to: None,
                session_id: None,
            },
        )
        .expect("posted")
    }

    #[test]
    fn keeps_a_mention_posted_while_its_conversation_starts() {
        // The channel rule reads the pending mentions, and the human's second message lands
        // before the conversation's `session.started` does.
        let log = open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens");
        let shown = mention(&log, "@dev-a status?");
        let pending = pending_mentions(&log, "dev-a").expect("the log reads");
        assert_eq!(pending.len(), 1);
        let late = mention(&log, "@dev-a and the tests?");
        let spec = SessionSpec {
            agent_id: "dev-a".to_string(),
            purpose: SessionPurpose::Conversation,
            ..a_session_spec()
        };
        let ids = EventIds {
            agent_id: Some("dev-a".to_string()),
            ..farik_ids()
        };
        record_session_started(&log, &spec, Some(shown), &ids, &FixedClock::new(at()))
            .expect("recorded");

        let seqs: Vec<u64> = pending_mentions(&log, "dev-a")
            .expect("the log reads")
            .iter()
            .map(|event| event.envelope.seq)
            .collect();

        assert_eq!(seqs, [late], "the conversation was shown {shown} alone");
    }
}
