//! What Claude Code's history says: one transcript line, and where each session belongs.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::pricing::Usage;

/// The Claude Code versions with a fixture and a test of their own.
pub const CHECKED_VERSIONS: [&str; 1] = ["2.1.296"];

/// How many characters of a session's title are kept.
pub const TITLE_LIMIT: usize = 200;

/// One line of a transcript, as far as Catervas reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptLine {
    /// A `user`, `attachment` or `assistant` line.
    Turn(Turn),
    /// An `ai-title` line.
    Title {
        /// The session it names.
        session_id: String,
        /// The title, trimmed and cut to `TITLE_LIMIT` characters.
        title: String,
    },
    /// Any other kind of line, including kinds a later version writes.
    Other,
    /// A line that is not JSON, or a turn or title missing what it needs.
    Unreadable,
}

/// One turn: where and when, never what was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// The session the line belongs to.
    pub session_id: String,
    /// The folder Claude Code was in.
    pub cwd: String,
    /// When the line was written.
    pub at: DateTime<Utc>,
    /// The git branch, when the line names one.
    pub git_branch: Option<String>,
    /// The Claude Code version, when the line names one.
    pub version: Option<String>,
    /// How Claude Code was started, when the line says.
    pub entrypoint: Option<String>,
    /// The model reply this line carries, when it carries one.
    pub reply: Option<Reply>,
}

/// One model reply's usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Its message id, else its request id.
    pub key: String,
    /// The model that answered.
    pub model: String,
    /// What the line reports it used.
    pub usage: Usage,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn owned(value: &Value, key: &str) -> Option<String> {
    text(value, key).map(str::to_string)
}

fn count(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn reply_of(line: &Value) -> Option<Reply> {
    let message = line.get("message")?;
    let model = text(message, "model").filter(|model| *model != "<synthetic>")?;
    let key = owned(message, "id").or_else(|| owned(line, "requestId"))?;
    let usage = message.get("usage").cloned().unwrap_or(Value::Null);
    Some(Reply {
        key,
        model: model.to_string(),
        usage: Usage {
            input_tokens: count(&usage, "input_tokens"),
            output_tokens: count(&usage, "output_tokens"),
            cache_read_tokens: count(&usage, "cache_read_input_tokens"),
            cache_write_tokens: count(&usage, "cache_creation_input_tokens"),
        },
    })
}

fn turn_of(line: &Value, is_assistant: bool) -> Option<Turn> {
    let at = DateTime::parse_from_rfc3339(text(line, "timestamp")?)
        .ok()?
        .with_timezone(&Utc);
    Some(Turn {
        session_id: owned(line, "sessionId")?,
        cwd: owned(line, "cwd")?,
        at,
        git_branch: owned(line, "gitBranch"),
        version: owned(line, "version"),
        entrypoint: owned(line, "entrypoint"),
        reply: if is_assistant { reply_of(line) } else { None },
    })
}

/// Reads one transcript line, keeping only the fields the index needs. Nothing a conversation
/// said can come out of it: `message.content` and every other field are never read.
#[must_use]
pub fn read_line(line: &str) -> TranscriptLine {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return TranscriptLine::Unreadable;
    };
    let Some(kind) = text(&value, "type") else {
        return TranscriptLine::Unreadable;
    };
    match kind {
        "user" | "attachment" | "assistant" => turn_of(&value, kind == "assistant")
            .map_or(TranscriptLine::Unreadable, TranscriptLine::Turn),
        "ai-title" => {
            let (Some(session_id), Some(title)) =
                (owned(&value, "sessionId"), text(&value, "aiTitle"))
            else {
                return TranscriptLine::Unreadable;
            };
            let title: String = title.trim().chars().take(TITLE_LIMIT).collect();
            if title.is_empty() {
                TranscriptLine::Other
            } else {
                TranscriptLine::Title { session_id, title }
            }
        }
        _ => TranscriptLine::Other,
    }
}

/// A reply's usage when a line reports it again: each counter's largest, whatever order the lines
/// arrive in, because Claude Code writes one line per content block of one reply.
#[must_use]
pub fn once_per_reply(seen: Usage, again: Usage) -> Usage {
    Usage {
        input_tokens: seen.input_tokens.max(again.input_tokens),
        output_tokens: seen.output_tokens.max(again.output_tokens),
        cache_read_tokens: seen.cache_read_tokens.max(again.cache_read_tokens),
        cache_write_tokens: seen.cache_write_tokens.max(again.cache_write_tokens),
    }
}

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

#[cfg(test)]
mod tests {
    use super::fixtures::{CLAUDE_CODE_2_1_296, a_reply_line, a_title_line, a_turn_line};
    use super::*;
    use crate::pricing::Usage;

    const CHECKED: [(&str, &str); 1] = [("2.1.296", CLAUDE_CODE_2_1_296)];

    fn usage(input: u64, output: u64, read: u64, write: u64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: read,
            cache_write_tokens: write,
        }
    }

    fn turn_of(line: &TranscriptLine) -> &Turn {
        match line {
            TranscriptLine::Turn(turn) => turn,
            other => panic!("expected a turn, got {other:?}"),
        }
    }

    #[test]
    fn reads_each_line_of_a_checked_version() {
        for version in CHECKED_VERSIONS {
            let text = CHECKED
                .iter()
                .find(|(name, _)| *name == version)
                .expect("every checked version has a fixture")
                .1;
            let lines: Vec<TranscriptLine> = text.lines().map(read_line).collect();
            assert_eq!(lines.len(), 13);
            for index in [0, 1, 5] {
                assert!(turn_of(&lines[index]).reply.is_none(), "line {}", index + 1);
            }
            let first = turn_of(&lines[0]);
            assert_eq!(first.session_id, "11111111-1111-4111-8111-111111111111");
            assert_eq!(first.cwd, "/home/me/shop");
            assert_eq!(first.at.to_rfc3339(), "2026-10-12T09:00:00+00:00");
            assert_eq!(first.git_branch.as_deref(), Some("main"));
            assert_eq!(first.version.as_deref(), Some("2.1.296"));
            assert_eq!(first.entrypoint.as_deref(), Some("cli"));
            let reply = turn_of(&lines[2]).reply.as_ref().expect("L3 is a reply");
            assert_eq!(reply.key, "msg_01A");
            assert_eq!(reply.model, "claude-opus-5-5");
            assert_eq!(reply.usage, usage(10, 1, 1000, 200));
            let again = turn_of(&lines[3]).reply.as_ref().expect("L4 is a reply");
            assert_eq!(again.key, "msg_01A");
            assert_eq!(again.usage, usage(10, 40, 1000, 200));
            let keyed = turn_of(&lines[4]).reply.as_ref().expect("L5 is a reply");
            assert_eq!(keyed.key, "req_01B");
            assert_eq!(keyed.usage, usage(5, 7, 0, 0));
            assert_eq!(
                lines[6],
                TranscriptLine::Title {
                    session_id: "11111111-1111-4111-8111-111111111111".to_string(),
                    title: "Fix the checkout total".to_string(),
                }
            );
            for line in &lines[7..=10] {
                assert_eq!(*line, TranscriptLine::Other);
            }
            assert_eq!(lines[11], TranscriptLine::Unreadable);
            assert_eq!(lines[12], TranscriptLine::Unreadable);
        }
    }

    /// Guard: checked by mutation (keep a text field), not watched to fail.
    #[test]
    fn reads_no_conversation_text() {
        for line in CLAUDE_CODE_2_1_296.lines().map(read_line) {
            assert!(!format!("{line:?}").contains("NEVER-KEPT-TEXT"), "{line:?}");
        }
    }

    #[test]
    fn reads_a_line_tolerantly() {
        let extra =
            a_turn_line("s", "/c", "2026-10-12T09:00:00Z").replace("{\"", "{\"extra\":1,\"");
        assert!(matches!(read_line(&extra), TranscriptLine::Turn(_)));
        let yesterday = a_turn_line("s", "/c", "yesterday");
        assert_eq!(read_line(&yesterday), TranscriptLine::Unreadable);
        let keyless = r#"{"type":"assistant","sessionId":"s","cwd":"/c","timestamp":"2026-10-12T09:00:00Z","message":{"model":"m","usage":{"input_tokens":1}}}"#;
        assert!(turn_of(&read_line(keyless)).reply.is_none());
        let no_read = r#"{"type":"assistant","sessionId":"s","cwd":"/c","timestamp":"2026-10-12T09:00:00Z","message":{"id":"m1","model":"m","usage":{"input_tokens":4,"output_tokens":2}}}"#;
        let reply = turn_of(&read_line(no_read)).reply.clone().expect("a reply");
        assert_eq!(reply.usage, usage(4, 2, 0, 0));
        let long = "x".repeat(300);
        assert_eq!(
            read_line(&a_title_line("s", &long)),
            TranscriptLine::Title {
                session_id: "s".to_string(),
                title: "x".repeat(TITLE_LIMIT)
            }
        );
        assert_eq!(read_line(&a_title_line("s", "  ")), TranscriptLine::Other);
        assert_eq!(read_line(""), TranscriptLine::Unreadable);
        assert_eq!(read_line("[]"), TranscriptLine::Unreadable);
        // The builders produce lines the reader reads back.
        let built = a_reply_line(
            "s",
            "/c",
            "2026-10-12T09:00:00Z",
            "m1",
            "m",
            usage(1, 2, 3, 4),
        );
        assert_eq!(
            turn_of(&read_line(&built)).reply.as_ref().map(|r| r.usage),
            Some(usage(1, 2, 3, 4))
        );
    }

    #[test]
    fn counts_a_reply_once() {
        let a = usage(10, 1, 1000, 200);
        let b = usage(10, 40, 1000, 200);
        assert_eq!(once_per_reply(a, b), b);
        assert_eq!(once_per_reply(b, a), b);
        assert_eq!(once_per_reply(a, a), a);
    }
}
