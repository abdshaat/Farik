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

/// The tools that read or write one file, named by `input.file_path`.
pub const FILE_TOOLS: [&str; 4] = ["Read", "Write", "Edit", "MultiEdit"];

/// The extensions that make a file a spreadsheet, compared without case.
pub const SPREADSHEET_EXTENSIONS: [&str; 3] = ["xlsx", "xls", "csv"];

/// One tool call of a reply: its id and name, and of its input only the fields named here (a
/// schedule, whether a file tool named a spreadsheet); the input is never kept whole and no text of
/// it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUse {
    /// The tool use's id.
    pub id: String,
    /// The tool's name.
    pub name: String,
    /// A `CronCreate` call's schedule fields; `None` for every other tool.
    pub cron: Option<CronCall>,
    /// Whether a file tool named a file with a spreadsheet extension. The path is never kept.
    pub touches_spreadsheet: bool,
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

fn is_spreadsheet_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        SPREADSHEET_EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
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
    let touches_spreadsheet = FILE_TOOLS.contains(&name.as_str())
        && block
            .get("input")
            .and_then(|input| text(input, "file_path"))
            .is_some_and(is_spreadsheet_path);
    Some(ToolUse {
        id,
        name,
        cron,
        touches_spreadsheet,
    })
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
/// name, `CronCreate`'s schedule fields and a file tool's extension; no text field is ever read.
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

/// Where a project, or an item of one, was found. A project's sources are derived from its rows, never
/// stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// A scan of this computer found it.
    Local,
    /// One of its sessions is a copy of another, so it was teleported here.
    Teleported,
    /// Claude.ai's export holds it.
    Export,
    /// The owner added it by hand.
    ByHand,
}

impl Source {
    /// The wire value of this source.
    #[must_use]
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Teleported => "teleported",
            Self::Export => "export",
            Self::ByHand => "by_hand",
        }
    }
}

fn remote_key(host: &str, path: &str) -> Option<String> {
    let host = host.trim().to_lowercase();
    let mut segments: Vec<String> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_lowercase)
        .collect();
    if let Some(last) = segments.last_mut()
        && let Some(stripped) = last.strip_suffix(".git")
    {
        *last = stripped.to_string();
    }
    segments.retain(|segment| !segment.is_empty());
    (!host.is_empty() && segments.len() >= 2).then(|| format!("{host}/{}", segments.join("/")))
}

/// A repository's address as one key: host then path, lower case, with a final `.git` and a trailing
/// `/` dropped, and the user, password and port dropped, so that SSH and HTTPS forms of one
/// repository are one and a token in a remote is never kept. `None` for a local path, a `file://`
/// address, a Windows drive path, and anything with fewer than two path segments.
#[must_use]
pub fn normalise_remote(address: &str) -> Option<String> {
    let address = address.trim();
    if address.is_empty() {
        return None;
    }
    if address.contains("://") {
        let url = url::Url::parse(address).ok()?;
        if url.scheme() == "file" {
            return None;
        }
        return remote_key(url.host_str()?, url.path());
    }
    // The scp form, `[user@]host:path`, where no `/` comes before the `:`.
    let (before, path) = address.split_once(':')?;
    if before.contains('/') {
        return None;
    }
    let host = before.rsplit('@').next().unwrap_or(before);
    if host.chars().count() < 2 {
        return None;
    }
    remote_key(host, path)
}

/// Why a project cannot be added by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandProjectRefusal {
    /// The name is empty.
    EmptyName,
    /// Neither a folder nor a repository address was given.
    NoPlace,
    /// The address is not one Catervas can read.
    NotARepositoryAddress(String),
    /// The folder is home, the root or a temporary folder.
    NotAProjectFolder(String),
}

impl std::fmt::Display for HandProjectRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("a project needs a name"),
            Self::NoPlace => {
                formatter.write_str("say where the project is: --folder <path> or --repo <address>")
            }
            Self::NotARepositoryAddress(address) => {
                write!(
                    formatter,
                    "{address} isn't a repository address Catervas can read"
                )
            }
            Self::NotAProjectFolder(path) => write!(
                formatter,
                "{path} can't be a project: it is your home, the root or a temporary folder"
            ),
        }
    }
}

impl std::error::Error for HandProjectRefusal {}

/// Checks a project added by hand: its trimmed name and its repository's key.
///
/// # Errors
///
/// `EmptyName` for a blank name, `NoPlace` with neither a folder nor an address,
/// `NotARepositoryAddress` for an address `normalise_remote` cannot read.
pub fn check_hand_project(
    name: &str,
    has_folder: bool,
    repository: Option<&str>,
) -> Result<(String, Option<String>), HandProjectRefusal> {
    let name: String = name.trim().chars().take(TITLE_LIMIT).collect();
    if name.is_empty() {
        return Err(HandProjectRefusal::EmptyName);
    }
    if !has_folder && repository.is_none() {
        return Err(HandProjectRefusal::NoPlace);
    }
    let remote = repository
        .map(|address| {
            normalise_remote(address)
                .ok_or_else(|| HandProjectRefusal::NotARepositoryAddress(address.to_string()))
        })
        .transpose()?;
    Ok((name, remote))
}

/// A found project's root and remote, as far as matching a project by hand goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectKey<'a> {
    /// The root folder.
    pub root: &'a str,
    /// The normalised remote, when it has one.
    pub remote: Option<&'a str>,
}

/// The found project a project by hand is the same as, as an index into `projects` (newest first): the
/// one with the longest root containing the hand project's root, else the first with its remote.
#[must_use]
pub fn match_hand_project(
    root: Option<&str>,
    remote: Option<&str>,
    projects: &[ProjectKey<'_>],
) -> Option<usize> {
    let by_root = root.and_then(|root| {
        projects
            .iter()
            .enumerate()
            .filter(|(_, project)| is_under(root, project.root))
            .max_by_key(|(index, project)| (project.root.len(), std::cmp::Reverse(*index)))
            .map(|(index, _)| index)
    });
    by_root.or_else(|| {
        let remote = remote?;
        projects
            .iter()
            .position(|project| project.remote == Some(remote))
    })
}

/// How many fingerprints two sessions must share, under different reply keys, to be linked.
pub const MIN_SHARED_REPLIES: usize = 3;

/// What identifies a reply across copies of its session: its time to the second, its model and its
/// four token counters. No text is hashed, and none is read.
#[must_use]
pub fn reply_fingerprint(at: DateTime<Utc>, model: &str, usage: Usage) -> String {
    format!(
        "{}|{model}|{}|{}|{}|{}",
        at.timestamp(),
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens,
        usage.cache_write_tokens
    )
}

/// One session's replies, for finding copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReplies {
    /// The session.
    pub session_id: String,
    /// When it started.
    pub first_at: DateTime<Utc>,
    /// Its row id in the index: the order sessions were indexed in.
    pub indexed: i64,
    /// `(reply key, fingerprint)` in time order.
    pub replies: Vec<(String, String)>,
}

/// How two sessions are linked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyLink {
    /// `copy` is a copy of `of`, which is kept.
    CopyOf {
        /// The copy, counted nowhere.
        copy: String,
        /// The session kept.
        of: String,
    },
    /// Linked, but one of them is in another link too, so neither is dropped.
    MaybeSame {
        /// The smaller session id.
        first: String,
        /// The other.
        second: String,
    },
}

/// What `copies_among` found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Copies {
    /// The links, `CopyOf` by copy then `MaybeSame` by ids.
    pub links: Vec<CopyLink>,
    /// `(copied reply key, kept reply key)`, by copied key.
    pub reply_copies: Vec<(String, String)>,
}

fn is_prefix_covered(replies: &[(String, String)], shared: &BTreeSet<usize>) -> bool {
    shared.iter().next_back().is_some_and(|last| {
        (0..=*last).all(|index| shared.contains(&index)) && *last < replies.len()
    })
}

/// The positions of `a`'s replies whose fingerprint `b` holds under another key.
fn shared_positions(a: &SessionReplies, b: &SessionReplies) -> BTreeSet<usize> {
    a.replies
        .iter()
        .enumerate()
        .filter(|(_, (key, print))| {
            b.replies
                .iter()
                .any(|(other_key, other)| other == print && other_key != key)
        })
        .map(|(index, _)| index)
        .collect()
}

fn are_linked(a: &SessionReplies, b: &SessionReplies) -> bool {
    let in_a = shared_positions(a, b);
    let prints: BTreeSet<&String> = in_a.iter().map(|index| &a.replies[*index].1).collect();
    if prints.len() < MIN_SHARED_REPLIES {
        return false;
    }
    let in_b = shared_positions(b, a);
    is_prefix_covered(&a.replies, &in_a) || is_prefix_covered(&b.replies, &in_b)
}

fn component_of(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

/// The replies of linked sessions to count once, within each connected group of linked sessions, as
/// `(copied key, kept key)` by copied key.
fn replies_counted_once(
    sessions: &[SessionReplies],
    pairs: &[(usize, usize)],
    degree: &[usize],
    kept_of: &BTreeMap<usize, usize>,
) -> Vec<(String, String)> {
    let mut parent: Vec<usize> = (0..sessions.len()).collect();
    for (first, second) in pairs {
        let (x, y) = (
            component_of(&mut parent, *first),
            component_of(&mut parent, *second),
        );
        parent[x] = y;
    }
    let mut groups: BTreeMap<(usize, &String), Vec<(usize, &String)>> = BTreeMap::new();
    for (index, session) in sessions.iter().enumerate() {
        if degree[index] == 0 {
            continue;
        }
        let component = component_of(&mut parent, index);
        for (key, print) in &session.replies {
            groups
                .entry((component, print))
                .or_default()
                .push((index, key));
        }
    }
    let mut reply_copies = Vec::new();
    for replies in groups.values() {
        let holders: BTreeSet<usize> = replies.iter().map(|(index, _)| *index).collect();
        if holders.len() < 2 {
            continue;
        }
        let partner = holders.iter().find(|index| kept_of.contains_key(index));
        let kept_session = partner.copied().unwrap_or_else(|| {
            holders
                .iter()
                .copied()
                .min_by_key(|index| (sessions[*index].first_at, sessions[*index].indexed))
                .unwrap_or_default()
        });
        let kept_key = replies
            .iter()
            .filter(|(index, _)| *index == kept_session)
            .map(|(_, key)| *key)
            .min();
        let Some(kept_key) = kept_key else { continue };
        for (index, key) in replies {
            if *index != kept_session {
                reply_copies.push(((*key).clone(), kept_key.clone()));
            }
        }
    }
    reply_copies.sort();
    reply_copies
}

/// Finds the sessions that are copies of one another and the replies to count once.
///
/// Two sessions are linked only when they share at least `MIN_SHARED_REPLIES` fingerprints under
/// different reply keys and the shared ones cover every reply of one of them up to its last shared
/// one. A pair in which each is in no other link merges: the later start is the copy of the other
/// (on equal starts, the session indexed first is kept). Every other linked pair is `MaybeSame`. A
/// fingerprint's replies in linked sessions are counted once: the kept one is the copy's partner's,
/// else the earliest session's, then the lowest row id, then the smallest key.
#[must_use]
pub fn copies_among(sessions: &[SessionReplies]) -> Copies {
    // Only sessions that hold a fingerprint in common can be linked, so only they are compared.
    let mut holders: BTreeMap<&String, BTreeSet<usize>> = BTreeMap::new();
    for (index, session) in sessions.iter().enumerate() {
        for (_, print) in &session.replies {
            holders.entry(print).or_default().insert(index);
        }
    }
    let mut candidates: BTreeSet<(usize, usize)> = BTreeSet::new();
    for group in holders.values().filter(|group| group.len() > 1) {
        for first in group {
            candidates.extend(group.range(first + 1..).map(|second| (*first, *second)));
        }
    }
    let pairs: Vec<(usize, usize)> = candidates
        .into_iter()
        .filter(|(first, second)| are_linked(&sessions[*first], &sessions[*second]))
        .collect();
    let mut degree = vec![0_usize; sessions.len()];
    for (first, second) in &pairs {
        degree[*first] += 1;
        degree[*second] += 1;
    }
    let mut links = Vec::new();
    let mut kept_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (first, second) in &pairs {
        let (a, b) = (&sessions[*first], &sessions[*second]);
        if degree[*first] == 1 && degree[*second] == 1 {
            let a_is_kept = (a.first_at, a.indexed) <= (b.first_at, b.indexed);
            let (kept, copy) = if a_is_kept {
                (*first, *second)
            } else {
                (*second, *first)
            };
            kept_of.insert(kept, copy);
            links.push((
                sessions[copy].session_id.clone(),
                sessions[kept].session_id.clone(),
                true,
            ));
        } else {
            let (low, high) = if a.session_id <= b.session_id {
                (&a.session_id, &b.session_id)
            } else {
                (&b.session_id, &a.session_id)
            };
            links.push((low.clone(), high.clone(), false));
        }
    }
    links.sort();
    let links = links
        .into_iter()
        .map(|(one, other, is_copy)| {
            if is_copy {
                CopyLink::CopyOf {
                    copy: one,
                    of: other,
                }
            } else {
                CopyLink::MaybeSame {
                    first: one,
                    second: other,
                }
            }
        })
        .collect();

    let reply_copies = replies_counted_once(sessions, &pairs, &degree, &kept_of);
    Copies {
        links,
        reply_copies,
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
                cron: None,
                touches_spreadsheet: false
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
                }),
                touches_spreadsheet: false
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

    fn connector_tools() -> Vec<(String, Vec<ToolUse>)> {
        use super::fixtures::CONNECTOR_FIXTURES;
        let versions: Vec<&str> = CONNECTOR_FIXTURES.iter().map(|(v, _)| *v).collect();
        assert_eq!(versions, CHECKED_VERSIONS);
        CONNECTOR_FIXTURES[0]
            .1
            .lines()
            .filter_map(|line| match read_line(line) {
                TranscriptLine::Turn(Turn {
                    reply: Some(reply), ..
                }) => Some((reply.key, reply.tools)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn tells_a_tool_call_that_touches_a_spreadsheet() {
        let replies = connector_tools();
        let flat: Vec<(&str, &str, bool)> = replies
            .iter()
            .flat_map(|(_, tools)| tools)
            .map(|tool| {
                (
                    tool.id.as_str(),
                    tool.name.as_str(),
                    tool.touches_spreadsheet,
                )
            })
            .collect();
        let touching: Vec<&str> = flat.iter().filter(|t| t.2).map(|t| t.0).collect();
        assert_eq!(touching, ["toolu_4", "toolu_9"]);
        let of = |key: &str| -> Vec<&str> {
            replies
                .iter()
                .filter(|(k, _)| k == key)
                .flat_map(|(_, tools)| tools.iter().map(|t| t.id.as_str()))
                .collect()
        };
        assert_eq!(of("msg_T3"), ["toolu_3", "toolu_4"]);
        assert_eq!(of("msg_T8"), ["toolu_8", "toolu_9"]);
        assert!(of("msg_S2").is_empty(), "T7 is no reply");
        for id in [
            "toolu_1", "toolu_2", "toolu_3", "toolu_5", "toolu_6", "toolu_8",
        ] {
            assert!(flat.iter().any(|t| t.0 == id && !t.2), "{id}");
        }
    }

    /// Guard: checked by mutation (keep the file path), not watched to fail.
    #[test]
    fn keeps_no_file_path() {
        use super::fixtures::CONNECTOR_FIXTURES;
        for line in CONNECTOR_FIXTURES[0].1.lines().map(read_line) {
            assert!(!format!("{line:?}").contains("NEVER-KEPT-TEXT"), "{line:?}");
        }
    }

    #[test]
    fn reads_a_file_path_tolerantly() {
        let touches = |name: &str, input: &str| -> bool {
            let line = format!(
                r#"{{"type":"assistant","sessionId":"s","cwd":"/c","timestamp":"2026-10-12T09:00:00Z","message":{{"id":"m1","model":"m","content":[{{"type":"tool_use","id":"t1","name":"{name}","input":{input}}}]}}}}"#
            );
            tools_of(&line)[0].touches_spreadsheet
        };
        assert!(!touches("Read", r#"{"file_path":7}"#));
        assert!(!touches("Read", "{}"));
        assert!(touches("Read", r#"{"file_path":"C:\\b\\x.CSV"}"#));
        assert!(touches("MultiEdit", r#"{"file_path":"/a/b.XlS"}"#));
        assert!(!touches("Read", r#"{"file_path":"/a/xlsx"}"#));
        assert!(!touches("Read", r#"{"file_path":"/a/b.xlsx.bak"}"#));
        assert!(!touches("Glob", r#"{"pattern":"*.csv"}"#));
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

    #[test]
    fn normalises_ssh_and_https_remotes_alike() {
        for address in [
            "https://github.com/Me/Shop.git",
            "https://github.com/me/shop/",
            "http://github.com/me/shop",
            "git@github.com:me/shop.git",
            "github.com:me/shop.git",
            "ssh://git@github.com/me/shop.GIT",
            "ssh://git@github.com:22/me/shop",
            "git://github.com/me/shop.git",
            "https://x-access-token:SECRET@github.com/me/shop.git",
        ] {
            assert_eq!(
                normalise_remote(address).as_deref(),
                Some("github.com/me/shop"),
                "{address}"
            );
        }
        assert_eq!(
            normalise_remote("https://gitlab.com/Group/Sub/Proj.git").as_deref(),
            Some("gitlab.com/group/sub/proj")
        );
        for address in [
            "/home/me/shop",
            "file:///home/me/shop",
            "../shop",
            "https://github.com/me",
            "git@github.com:",
            "C:/repo",
            "",
        ] {
            assert_eq!(normalise_remote(address), None, "{address}");
        }
    }

    #[test]
    fn checks_a_project_by_hand() {
        assert_eq!(
            check_hand_project("  Shop ", true, None),
            Ok(("Shop".to_string(), None))
        );
        assert_eq!(
            check_hand_project("Shop", false, Some("git@github.com:me/shop.git")),
            Ok(("Shop".to_string(), Some("github.com/me/shop".to_string())))
        );
        assert_eq!(
            check_hand_project(" ", true, None),
            Err(HandProjectRefusal::EmptyName)
        );
        assert_eq!(
            check_hand_project("Shop", false, None),
            Err(HandProjectRefusal::NoPlace)
        );
        assert_eq!(
            check_hand_project("Shop", false, Some("/tmp/x")),
            Err(HandProjectRefusal::NotARepositoryAddress(
                "/tmp/x".to_string()
            ))
        );
        let sentences = [
            (HandProjectRefusal::EmptyName, "a project needs a name"),
            (
                HandProjectRefusal::NoPlace,
                "say where the project is: --folder <path> or --repo <address>",
            ),
            (
                HandProjectRefusal::NotARepositoryAddress("/tmp/x".to_string()),
                "/tmp/x isn't a repository address Catervas can read",
            ),
            (
                HandProjectRefusal::NotAProjectFolder("/".to_string()),
                "/ can't be a project: it is your home, the root or a temporary folder",
            ),
        ];
        for (refusal, sentence) in sentences {
            assert_eq!(refusal.to_string(), sentence);
        }
    }

    #[test]
    fn matches_a_project_by_hand() {
        let projects = [
            ProjectKey {
                root: "/h/shop",
                remote: Some("github.com/me/shop"),
            },
            ProjectKey {
                root: "/h/shop/apps",
                remote: None,
            },
            ProjectKey {
                root: "/h/notes",
                remote: None,
            },
        ];
        assert_eq!(
            match_hand_project(Some("/h/shop/apps/web"), None, &projects),
            Some(1)
        );
        assert_eq!(
            match_hand_project(Some("/h/shopping"), None, &projects),
            None
        );
        assert_eq!(
            match_hand_project(None, Some("github.com/me/shop"), &projects),
            Some(0)
        );
        assert_eq!(
            match_hand_project(Some("/h/other"), Some("github.com/me/shop"), &projects),
            Some(0)
        );
        assert_eq!(
            match_hand_project(None, Some("github.com/me/x"), &projects),
            None
        );
    }

    fn at_second(seconds: u32) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!(
            "2026-10-12T09:{:02}:{:02}Z",
            seconds / 60,
            seconds % 60
        ))
        .expect("a time")
        .with_timezone(&Utc)
    }

    /// A reply made at second `n`, with usage that only it has.
    fn print(n: u32) -> String {
        reply_fingerprint(
            at_second(n),
            "claude-opus-5-5",
            usage(u64::from(n), 2, 3, 4),
        )
    }

    fn session(id: &str, indexed: i64, first: u32, replies: &[(&str, u32)]) -> SessionReplies {
        SessionReplies {
            session_id: id.to_string(),
            first_at: at_second(first),
            indexed,
            replies: replies
                .iter()
                .map(|(key, n)| ((*key).to_string(), print(*n)))
                .collect(),
        }
    }

    fn pair(copy: &str, of: &str) -> CopyLink {
        CopyLink::CopyOf {
            copy: copy.to_string(),
            of: of.to_string(),
        }
    }

    fn maybe(first: &str, second: &str) -> CopyLink {
        CopyLink::MaybeSame {
            first: first.to_string(),
            second: second.to_string(),
        }
    }

    fn copied(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(copy, kept)| ((*copy).to_string(), (*kept).to_string()))
            .collect()
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    fn links_copies_on_strong_evidence_only() {
        let a = session(
            "a",
            1,
            5,
            &[("a1", 5), ("a2", 60), ("a3", 120), ("a4", 180)],
        );
        let b = session(
            "b",
            2,
            300,
            &[("b1", 5), ("b2", 60), ("b3", 120), ("b4", 180), ("b5", 400)],
        );
        let found = copies_among(&[a.clone(), b.clone()]);
        assert_eq!(found.links, vec![pair("b", "a")]);
        assert_eq!(
            found.reply_copies,
            copied(&[("b1", "a1"), ("b2", "a2"), ("b3", "a3"), ("b4", "a4")])
        );
        // On equal start times the session indexed first is kept.
        let tied = SessionReplies {
            first_at: a.first_at,
            ..b.clone()
        };
        assert_eq!(
            copies_among(&[tied.clone(), a.clone()]).links,
            vec![pair("b", "a")]
        );
        let earlier = SessionReplies { indexed: 0, ..tied };
        assert_eq!(
            copies_among(&[earlier, a.clone()]).links,
            vec![pair("a", "b")]
        );
        // Two shared replies are not enough.
        let two = session("b", 2, 300, &[("b1", 5), ("b2", 60)]);
        assert_eq!(copies_among(&[a.clone(), two]), Copies::default());
        // Three shared, but each holds a reply between them that the other lacks.
        let gap_a = session(
            "a",
            1,
            5,
            &[("a1", 5), ("a2", 60), ("a3", 120), ("a4", 180)],
        );
        let gap_b = session(
            "b",
            2,
            300,
            &[("b1", 5), ("b9", 90), ("b3", 120), ("b4", 180)],
        );
        assert_eq!(copies_among(&[gap_a, gap_b]), Copies::default());
        // A shared key is a resume, not a copy.
        let resumed = session(
            "b",
            2,
            300,
            &[("a1", 5), ("a2", 60), ("a3", 120), ("a4", 180)],
        );
        assert_eq!(copies_among(&[a.clone(), resumed]), Copies::default());
        // Two copies of one session cannot be told apart.
        let c = session(
            "c",
            3,
            310,
            &[("c1", 5), ("c2", 60), ("c3", 120), ("c4", 180)],
        );
        let found = copies_among(&[a.clone(), b, c]);
        assert_eq!(
            found.links,
            vec![maybe("a", "b"), maybe("a", "c"), maybe("b", "c")]
        );
        assert_eq!(found.reply_copies.len(), 8);
        for (copy, kept) in [("b1", "a1"), ("c4", "a4"), ("b3", "a3"), ("c2", "a2")] {
            assert!(
                found
                    .reply_copies
                    .contains(&(copy.to_string(), kept.to_string()))
            );
        }
        // Two separate pairs.
        let x = session("x", 4, 1000, &[("x1", 1000), ("x2", 1001), ("x3", 1002)]);
        let y = session("y", 5, 1100, &[("y1", 1000), ("y2", 1001), ("y3", 1002)]);
        let two_pairs = copies_among(&[
            a.clone(),
            x,
            y,
            session(
                "z",
                6,
                400,
                &[("z1", 5), ("z2", 60), ("z3", 120), ("z4", 180)],
            ),
        ]);
        assert_eq!(two_pairs.links, vec![pair("y", "x"), pair("z", "a")]);
        // A second only changes the second of the time; seconds decide.
        let subsecond = reply_fingerprint(
            at_second(5) + chrono::Duration::milliseconds(700),
            "claude-opus-5-5",
            usage(5, 2, 3, 4),
        );
        assert_eq!(subsecond, print(5));
        let wires = [
            (Source::Local, "local"),
            (Source::Teleported, "teleported"),
            (Source::Export, "export"),
            (Source::ByHand, "by_hand"),
        ];
        for (source, wire) in wires {
            assert_eq!(source.as_wire(), wire);
        }
    }
}
