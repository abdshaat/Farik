//! What Claude Code's history says: one transcript line, and where each session belongs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
    /// The tool calls the line carries, one per `tool_use` block.
    pub tools: Vec<ToolUse>,
}

/// One tool call of a reply: its id and name, and of its input only the fields named here. A later
/// step adds one field for what it needs (03b: whether it touched a spreadsheet file); the input is
/// never kept whole and no text of it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUse {
    /// The tool use's id.
    pub id: String,
    /// The tool's name.
    pub name: String,
    /// A `CronCreate` call's schedule fields; `None` for every other tool.
    pub cron: Option<CronCall>,
}

/// The schedule fields of a `CronCreate` call. Its prompt is never read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronCall {
    /// The schedule, as written.
    pub cron: String,
    /// Whether it repeats; true when the call does not say.
    pub is_recurring: bool,
    /// Whether it is saved to `.claude/scheduled_tasks.json`; false when the call does not say.
    pub is_durable: bool,
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

fn tool_use_of(block: &Value) -> Option<ToolUse> {
    if text(block, "type")? != "tool_use" {
        return None;
    }
    let (id, name) = (owned(block, "id")?, owned(block, "name")?);
    let cron = if name == "CronCreate" {
        block.get("input").and_then(|input| {
            Some(CronCall {
                cron: owned(input, "cron")?,
                is_recurring: input
                    .get("recurring")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                is_durable: input
                    .get("durable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
    } else {
        None
    };
    Some(ToolUse { id, name, cron })
}

fn reply_of(line: &Value) -> Option<Reply> {
    let message = line.get("message")?;
    let model = text(message, "model").filter(|model| *model != "<synthetic>")?;
    let key = owned(message, "id").or_else(|| owned(line, "requestId"))?;
    let usage = message.get("usage").cloned().unwrap_or(Value::Null);
    Some(Reply {
        key,
        tools: message
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| blocks.iter().filter_map(tool_use_of).collect())
            .unwrap_or_default(),
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
/// said can come out of it: `message.content` is read only for its `tool_use` blocks' type, id and
/// name, and `CronCreate`'s schedule fields; no text field is ever read.
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

/// Folders that are never a project, whatever the owner's home is.
pub const TEMPORARY_FOLDERS: [&str; 3] = ["/", "/tmp", "/var/tmp"];

/// A folder a session worked in, as the store found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The folder as the transcript names it.
    pub path: String,
    /// Its canonical path; `None` when the folder is gone.
    pub real: Option<String>,
    /// The main worktree of the repository it is in; `None` when it is in none.
    pub git_root: Option<String>,
}

/// A project root an earlier scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRoot {
    /// The root folder.
    pub root: String,
    /// Whether it was a git repository when last checked.
    pub is_git: bool,
}

/// Where a folder belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// In the project with this root.
    Project {
        /// The project's root folder.
        root: String,
    },
    /// Not on this computer any more, and under no known root.
    Gone,
    /// Home or a temporary folder.
    NotAProject,
}

fn is_under(path: &str, root: &str) -> bool {
    Path::new(path).starts_with(root)
}

/// Places each folder, in order: a not-project folder is `NotAProject`; else its git root; else the
/// longest known root containing it (only roots with git for a folder that still exists); else
/// `Gone` for a gone folder; else the outermost folder with no git among `folders` that contains it.
///
/// `not_projects` (home, the temporary folder) and `TEMPORARY_FOLDERS` are never a root, and a git
/// root that holds one is ignored as if there were no git.
#[must_use]
pub fn place_folders(
    folders: &[Folder],
    known: &[KnownRoot],
    not_projects: &[String],
) -> Vec<Place> {
    let blocked: Vec<&str> = not_projects
        .iter()
        .map(String::as_str)
        .chain(TEMPORARY_FOLDERS)
        .collect();
    let location = |folder: &'_ Folder| -> String {
        folder.real.clone().unwrap_or_else(|| folder.path.clone())
    };
    let git_of = |folder: &Folder| -> Option<String> {
        folder
            .git_root
            .clone()
            .filter(|root| !blocked.iter().any(|blocked| is_under(blocked, root)))
    };
    let scan_roots: Vec<String> = folders.iter().filter_map(git_of).collect();
    folders
        .iter()
        .map(|folder| {
            let here = location(folder);
            if blocked.contains(&here.as_str()) {
                return Place::NotAProject;
            }
            if let Some(root) = git_of(folder) {
                return Place::Project { root };
            }
            let exists = folder.real.is_some();
            let longest = known
                .iter()
                .filter(|root| root.is_git || !exists)
                .map(|root| root.root.as_str())
                .chain(scan_roots.iter().map(String::as_str))
                .filter(|root| !blocked.contains(root) && is_under(&here, root))
                .max_by_key(|root| root.len());
            if let Some(root) = longest {
                return Place::Project {
                    root: root.to_string(),
                };
            }
            if !exists {
                return Place::Gone;
            }
            let outermost = folders
                .iter()
                .filter(|other| other.real.is_some() && git_of(other).is_none())
                .map(location)
                .filter(|other| !blocked.contains(&other.as_str()) && is_under(&here, other))
                .min_by_key(String::len);
            Place::Project {
                root: outermost.unwrap_or(here),
            }
        })
        .collect()
}

/// The project root a Catervas worktree belongs to: the path before its `.catervas/local/`.
#[must_use]
pub fn catervas_root_of(folder: &str) -> Option<String> {
    Path::new(folder).ancestors().find_map(|ancestor| {
        let in_local = ancestor.file_name()? == "local";
        let marker = ancestor.parent()?;
        if !in_local || marker.file_name()? != ".catervas" {
            return None;
        }
        Some(marker.parent()?.to_string_lossy().into_owned())
    })
}

/// Where a session belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPlace {
    /// The owner's own session in a project.
    Owner {
        /// The project's root folder.
        root: String,
    },
    /// A session of Catervas's own agents in a project.
    Catervas {
        /// The project's root folder.
        root: String,
    },
    /// A session in a folder no longer on this computer.
    Gone {
        /// The folder the session worked in.
        folder: String,
    },
    /// A session in home or a temporary folder.
    NotAProject,
}

impl SessionPlace {
    /// The value of `sessions.place` for this place.
    #[must_use]
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Owner { .. } => "owner",
            Self::Catervas { .. } => "catervas",
            Self::Gone { .. } => "gone",
            Self::NotAProject => "not_a_project",
        }
    }
}

/// Places a session: Catervas's when its folder is under `.catervas/local/` or its project's log
/// names it, else the owner's under its folder's project. Home and the other `not_projects` are
/// never a project's root, so a `.catervas/local/` straight under one does not make it Catervas's.
#[must_use]
pub fn place_session(
    session_id: &str,
    cwd: &str,
    folder: &Place,
    catervas_logs: &BTreeMap<String, BTreeSet<String>>,
    not_projects: &[String],
) -> SessionPlace {
    if let Some(root) = catervas_root_of(cwd)
        && !not_projects.contains(&root)
        && !TEMPORARY_FOLDERS.contains(&root.as_str())
    {
        return SessionPlace::Catervas { root };
    }
    match folder {
        Place::Project { root } => {
            if catervas_logs
                .get(root)
                .is_some_and(|ids| ids.contains(session_id))
            {
                SessionPlace::Catervas { root: root.clone() }
            } else {
                SessionPlace::Owner { root: root.clone() }
            }
        }
        Place::Gone => SessionPlace::Gone {
            folder: cwd.to_string(),
        },
        Place::NotAProject => SessionPlace::NotAProject,
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

    fn tools_of(line: &str) -> Vec<ToolUse> {
        turn_of(&read_line(line))
            .reply
            .as_ref()
            .expect("a reply")
            .tools
            .clone()
    }

    #[test]
    fn reads_the_tool_calls_of_a_reply() {
        use super::fixtures::{CRON_CREATE_LINES, CronCreateLine, a_cron_create_line};
        let lines: Vec<&str> = CRON_CREATE_LINES.lines().collect();
        let keys: Vec<String> = lines
            .iter()
            .map(|line| {
                turn_of(&read_line(line))
                    .reply
                    .as_ref()
                    .expect("a reply")
                    .key
                    .clone()
            })
            .collect();
        assert_eq!(keys, ["msg_01C"; 3]);
        assert_eq!(tools_of(lines[0]), vec![]);
        assert_eq!(
            tools_of(lines[1]),
            vec![ToolUse {
                id: "toolu_01B".to_string(),
                name: "Bash".to_string(),
                cron: None
            }]
        );
        assert_eq!(
            tools_of(lines[2]),
            vec![ToolUse {
                id: "toolu_01C".to_string(),
                name: "CronCreate".to_string(),
                cron: Some(CronCall {
                    cron: "*/10 * * * *".to_string(),
                    is_recurring: true,
                    is_durable: false
                })
            }]
        );
        let call = |recurring, durable| CronCreateLine {
            session_id: "s",
            cwd: "/c",
            at: "2026-10-12T09:00:00Z",
            message_id: "m1",
            tool_use_id: "t1",
            cron: "0 * * * *",
            recurring,
            durable,
        };
        let flags = |recurring, durable| {
            tools_of(&a_cron_create_line(&call(recurring, durable)))[0]
                .cron
                .clone()
                .map(|cron| (cron.is_recurring, cron.is_durable))
        };
        assert_eq!(flags(Some(false), Some(true)), Some((false, true)));
        assert_eq!(flags(Some(true), Some(false)), Some((true, false)));
        let no_cron = r#"{"type":"assistant","sessionId":"s","cwd":"/c","timestamp":"2026-10-12T09:00:00Z","message":{"id":"m1","model":"m","content":[{"type":"tool_use","id":"t1","name":"CronCreate","input":{}}]}}"#;
        assert_eq!(tools_of(no_cron)[0].cron, None);
        let no_id = r#"{"type":"assistant","sessionId":"s","cwd":"/c","timestamp":"2026-10-12T09:00:00Z","message":{"id":"m1","model":"m","content":[{"type":"tool_use","name":"Bash"},{"type":"tool_use","id":"t2"}]}}"#;
        assert_eq!(tools_of(no_id), vec![]);
        for line in CLAUDE_CODE_2_1_296.lines().map(read_line) {
            if let TranscriptLine::Turn(Turn {
                reply: Some(reply), ..
            }) = line
            {
                assert_eq!(reply.tools, vec![]);
            }
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
        // Each counter is larger in one of the two, in both directions across the four.
        let a = usage(10, 1, 1000, 0);
        let b = usage(5, 40, 900, 200);
        let largest = usage(10, 40, 1000, 200);
        assert_eq!(once_per_reply(a, b), largest);
        assert_eq!(once_per_reply(b, a), largest);
        assert_eq!(once_per_reply(a, a), a);
    }

    fn folder(path: &str, git_root: Option<&str>) -> Folder {
        Folder {
            path: path.to_string(),
            real: Some(path.to_string()),
            git_root: git_root.map(str::to_string),
        }
    }

    fn gone(path: &str) -> Folder {
        Folder {
            path: path.to_string(),
            real: None,
            git_root: None,
        }
    }

    fn known(root: &str, is_git: bool) -> KnownRoot {
        KnownRoot {
            root: root.to_string(),
            is_git,
        }
    }

    fn project(root: &str) -> Place {
        Place::Project {
            root: root.to_string(),
        }
    }

    fn home() -> Vec<String> {
        vec!["/home/me".to_string()]
    }

    #[test]
    fn places_a_worktree_in_its_repository() {
        let folders = [
            folder("/home/me/shop", Some("/home/me/shop")),
            folder("/home/me/shop/.claude/worktrees/x", Some("/home/me/shop")),
            folder("/home/me/shop-wt", Some("/home/me/shop")),
            // Git could not say (a build folder it ignores), but it is below a repository's root.
            folder("/home/me/shop/build", None),
        ];
        let places = place_folders(&folders, &[], &home());
        assert_eq!(places, vec![project("/home/me/shop"); 4]);
    }

    #[test]
    fn places_a_deleted_worktree_under_its_known_root() {
        let old = gone("/home/me/shop/.claude/worktrees/old");
        let places = place_folders(&[old], &[known("/home/me/shop", true)], &home());
        assert_eq!(places, vec![project("/home/me/shop")]);
        let places = place_folders(
            &[gone("/home/me/shopping")],
            &[known("/home/me/shop", true)],
            &home(),
        );
        assert_eq!(places, vec![Place::Gone]);
        let places = place_folders(
            &[gone("/home/me/a/b/c")],
            &[known("/home/me/a", true), known("/home/me/a/b", true)],
            &home(),
        );
        assert_eq!(places, vec![project("/home/me/a/b")]);
        let places = place_folders(
            &[gone("/home/me/n/x")],
            &[known("/home/me/n", false)],
            &home(),
        );
        assert_eq!(places, vec![project("/home/me/n")]);
    }

    #[test]
    fn never_takes_home_as_a_known_root() {
        let folders = [folder("/home/me/scratch", None)];
        let places = place_folders(&folders, &[known("/home/me", true)], &home());
        assert_eq!(places, vec![project("/home/me/scratch")]);
    }

    #[test]
    fn folds_a_folder_with_no_git_into_its_outermost_session_folder() {
        let folders = [
            folder("/home/me/notes/2026/q4", None),
            folder("/home/me/notes", None),
            folder("/home/me/notes/2026", None),
            folder("/home/me/solo", None),
        ];
        let expected = vec![
            project("/home/me/notes"),
            project("/home/me/notes"),
            project("/home/me/notes"),
            project("/home/me/solo"),
        ];
        assert_eq!(place_folders(&folders, &[], &home()), expected);
        let with_known = [known("/home/me/notes/2026", false)];
        assert_eq!(place_folders(&folders, &with_known, &home()), expected);
    }

    #[test]
    fn never_makes_home_or_a_temporary_folder_a_project() {
        let folders = [
            folder("/home/me", None),
            folder("/tmp", None),
            folder("/tmp/scratch", None),
            folder("/home/me/scratch", None),
        ];
        let places = place_folders(&folders, &[], &home());
        assert_eq!(
            places,
            vec![
                Place::NotAProject,
                Place::NotAProject,
                project("/tmp/scratch"),
                project("/home/me/scratch"),
            ]
        );
        // A home folder kept in git, and a git root of `/`, are as if there were no git.
        let kept = [folder("/home/me/docs", Some("/home/me"))];
        assert_eq!(
            place_folders(&kept, &[], &home()),
            vec![project("/home/me/docs")]
        );
        let rooted = [folder("/data/x", Some("/"))];
        assert_eq!(
            place_folders(&rooted, &[], &home()),
            vec![project("/data/x")]
        );
        let no_home = [folder("/home/me", None)];
        assert_eq!(place_folders(&no_home, &[], &[]), vec![project("/home/me")]);
    }

    #[test]
    fn places_catervas_sessions_under_their_project() {
        assert_eq!(
            catervas_root_of("/home/me/shop/.catervas/local/worktrees/CTV-3"),
            Some("/home/me/shop".to_string())
        );
        assert_eq!(catervas_root_of("/home/me/shop"), None);
        assert_eq!(catervas_root_of("/home/me/shop/local/x"), None);
        assert_eq!(catervas_root_of("/home/me/shop/.catervas/x/local"), None);
        let none = BTreeMap::new();
        let catervas = SessionPlace::Catervas {
            root: "/home/me/shop".to_string(),
        };
        let cwd = "/home/me/shop/.catervas/local/worktrees/CTV-3";
        for folder in [project("/home/me/other"), Place::Gone, Place::NotAProject] {
            assert_eq!(place_session("s", cwd, &folder, &none, &home()), catervas);
        }
        let logs = BTreeMap::from([(
            "/home/me/shop".to_string(),
            BTreeSet::from(["cat-1".to_string()]),
        )]);
        let here = project("/home/me/shop");
        assert_eq!(
            place_session("cat-1", "/home/me/shop", &here, &logs, &home()),
            catervas
        );
        let owner = place_session("own-1", "/home/me/shop", &here, &logs, &home());
        assert_eq!(
            owner,
            SessionPlace::Owner {
                root: "/home/me/shop".to_string()
            }
        );
        let lost = place_session("s", "/home/me/old", &Place::Gone, &logs, &home());
        assert_eq!(
            lost,
            SessionPlace::Gone {
                folder: "/home/me/old".to_string()
            }
        );
        assert_eq!(
            place_session("s", "/home/me", &Place::NotAProject, &logs, &home()),
            SessionPlace::NotAProject
        );
        // Home is never a project root, so a `.catervas/local` folder straight under it is not one.
        assert_eq!(
            place_session(
                "s",
                "/home/me/.catervas/local/x",
                &Place::NotAProject,
                &none,
                &home()
            ),
            SessionPlace::NotAProject
        );
        assert_eq!(
            place_session(
                "s",
                "/tmp/.catervas/local/x",
                &project("/tmp/p"),
                &none,
                &home()
            ),
            SessionPlace::Owner {
                root: "/tmp/p".to_string()
            }
        );
        let wires = [
            (catervas, "catervas"),
            (owner, "owner"),
            (lost, "gone"),
            (SessionPlace::NotAProject, "not_a_project"),
        ];
        for (place, wire) in wires {
            assert_eq!(place.as_wire(), wire);
        }
    }
}
