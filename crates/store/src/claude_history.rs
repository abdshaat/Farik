//! Reading Claude Code's transcripts into the machine index, and placing each session in its project.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use catervas_core::claude_history::{
    Folder, HandProjectRefusal, KnownRoot, Place, ProjectKey, SessionPlace, TranscriptLine,
    check_hand_project, copies_among, match_hand_project, normalise_remote, once_per_reply,
    place_folders, place_session, read_line,
};
use catervas_core::claude_services::server_segment;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};

use crate::error::StoreError;
use crate::git::{Git, GitError};
use crate::machine::{
    FileRead, FileState, HandProject, MachineIndex, Ownership, Placement, ProjectRow, ReplyBatch,
    SessionBatch, SessionCronBatch, ToolCallBatch,
};

/// Where a Catervas project keeps its log, relative to its root.
pub const PROJECT_LOG: &str = ".catervas/local/catervas.db";

/// Why reading or placing the history did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// The index or the file system refused.
    Store(StoreError),
    /// The `git` program could not be run.
    Git(GitError),
    /// A project cannot be added by hand, for this reason.
    HandProject(HandProjectRefusal),
    /// There is no folder at this path.
    NoSuchFolder(PathBuf),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => error.fmt(formatter),
            Self::Git(error) => error.fmt(formatter),
            Self::HandProject(refusal) => refusal.fmt(formatter),
            Self::NoSuchFolder(path) => {
                write!(formatter, "there is no folder at {}", path.display())
            }
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<StoreError> for HistoryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<GitError> for HistoryError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

impl From<std::io::Error> for HistoryError {
    fn from(error: std::io::Error) -> Self {
        Self::Store(error.into())
    }
}

/// What one scan of the transcripts did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReadSummary {
    /// Transcript files listed and opened.
    pub files: u64,
    /// Sessions in the index after the scan.
    pub sessions: u64,
    /// Sessions the scan added.
    pub new_sessions: u64,
    /// Files refused for being outside Claude Code's transcripts.
    pub refused: u64,
}

fn sorted_entries(folder: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut entries = std::fs::read_dir(folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries)
}

fn is_transcript_name(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "jsonl")
}

/// The files that may be transcripts: `projects/<folder>/<session>.jsonl` and
/// `projects/<folder>/<session>/subagents/<agent>.jsonl`, in path order. Nothing else under
/// `projects/`.
fn candidates(projects: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut found = Vec::new();
    for folder in sorted_entries(projects)? {
        if !folder.is_dir() {
            continue;
        }
        for entry in sorted_entries(&folder)? {
            if entry.is_dir() {
                let subagents = entry.join("subagents");
                if subagents.is_dir() {
                    found.extend(
                        sorted_entries(&subagents)?
                            .into_iter()
                            .filter(|file| is_transcript_name(file)),
                    );
                }
            } else if is_transcript_name(&entry) {
                found.push(entry);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// What a candidate is, once its link is followed.
#[derive(Debug, PartialEq, Eq)]
enum Opened {
    /// A regular file inside `projects/`, as its canonical path: the one to open, so that a link
    /// swapped after the check cannot redirect the read.
    Transcript(PathBuf),
    /// Pointing outside `projects/`, or Claude Code's credentials: never opened.
    Refused,
    /// Gone or not a regular file; tried again next scan.
    Skipped,
}

fn classify(path: &Path, projects: &Path) -> Opened {
    let Ok(real) = path.canonicalize() else {
        return Opened::Skipped;
    };
    if !real.starts_with(projects)
        || real
            .file_name()
            .is_some_and(|name| name == ".credentials.json")
    {
        return Opened::Refused;
    }
    if real.is_file() {
        Opened::Transcript(real)
    } else {
        Opened::Skipped
    }
}

/// What reading the new part of one file found.
#[derive(Default)]
struct Chunk {
    read_to: u64,
    turns: u64,
    unreadable_lines: u64,
    sessions: BTreeMap<String, SessionBatch>,
    titles: BTreeMap<String, String>,
    replies: BTreeMap<String, ReplyBatch>,
    crons: BTreeMap<(String, String), SessionCronBatch>,
    tool_calls: BTreeMap<String, ToolCallBatch>,
}

fn read_chunk(bytes: &[u8], mut chunk: Chunk) -> Chunk {
    // A line still being written has no newline yet; it is read next time.
    let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return chunk;
    };
    chunk.read_to += (end + 1) as u64;
    for raw in bytes[..end].split(|byte| *byte == b'\n') {
        match read_line(&String::from_utf8_lossy(raw)) {
            TranscriptLine::Turn(turn) => {
                chunk.turns += 1;
                if let Some(reply) = turn.reply {
                    for tool in &reply.tools {
                        if let Some(call) = tool.cron.as_ref().filter(|call| !call.is_durable) {
                            chunk
                                .crons
                                .entry((turn.session_id.clone(), tool.id.clone()))
                                .or_insert_with(|| SessionCronBatch {
                                    session_id: turn.session_id.clone(),
                                    tool_use_id: tool.id.clone(),
                                    cwd: turn.cwd.clone(),
                                    cron: call.cron.clone(),
                                    at: turn.at,
                                });
                        }
                    }
                    for tool in &reply.tools {
                        if server_segment(&tool.name).is_some() || tool.touches_spreadsheet {
                            chunk.tool_calls.entry(tool.id.clone()).or_insert_with(|| {
                                ToolCallBatch {
                                    tool_use_id: tool.id.clone(),
                                    reply_key: reply.key.clone(),
                                    tool_name: tool.name.clone(),
                                    touches_spreadsheet: tool.touches_spreadsheet,
                                    at: turn.at,
                                }
                            });
                        }
                    }
                    let batch = ReplyBatch {
                        key: reply.key.clone(),
                        session_id: turn.session_id.clone(),
                        model: reply.model,
                        at: turn.at,
                        usage: reply.usage,
                        tools: reply.tools,
                    };
                    chunk
                        .replies
                        .entry(reply.key)
                        .and_modify(|seen| {
                            seen.usage = once_per_reply(seen.usage, batch.usage);
                            seen.at = seen.at.min(batch.at);
                            for tool in &batch.tools {
                                if !seen.tools.iter().any(|known| known.id == tool.id) {
                                    seen.tools.push(tool.clone());
                                }
                            }
                        })
                        .or_insert(batch);
                }
                let session = chunk
                    .sessions
                    .entry(turn.session_id.clone())
                    .or_insert_with(|| SessionBatch {
                        session_id: turn.session_id.clone(),
                        cwd: turn.cwd.clone(),
                        git_branch: turn.git_branch.clone(),
                        version: turn.version.clone(),
                        entrypoint: turn.entrypoint.clone(),
                        first_at: turn.at,
                        last_at: turn.at,
                    });
                if turn.at < session.first_at {
                    session.cwd = turn.cwd;
                    session.git_branch = turn.git_branch;
                    session.version = turn.version;
                    session.entrypoint = turn.entrypoint;
                    session.first_at = turn.at;
                }
                session.last_at = session.last_at.max(turn.at);
            }
            TranscriptLine::Title { session_id, title } => {
                chunk.titles.insert(session_id, title);
            }
            TranscriptLine::Other => {}
            TranscriptLine::Unreadable => chunk.unreadable_lines += 1,
        }
    }
    chunk
}

/// Reads the transcripts under `claude_dir/projects` into the index, each file from where the last
/// scan stopped. A file Claude Code replaced with a shorter one is read again from its start.
///
/// Only `.jsonl` files whose canonical path is inside `projects/` are opened; a link pointing out of
/// it, and `.credentials.json`, are counted in `refused` and never opened. A folder with no
/// `projects/` reads nothing.
///
/// # Errors
///
/// `Store` when the index refuses or the projects folder cannot be listed.
pub fn read_transcripts(
    index: &mut MachineIndex,
    claude_dir: &Path,
    now: DateTime<Utc>,
) -> Result<ReadSummary, HistoryError> {
    let mut summary = ReadSummary::default();
    let Ok(projects) = claude_dir.join("projects").canonicalize() else {
        summary.sessions = index.session_count()?;
        return Ok(summary);
    };
    let before = index.session_count()?;
    for path in candidates(&projects)? {
        let real = match classify(&path, &projects) {
            Opened::Skipped => continue,
            Opened::Refused => {
                summary.refused += 1;
                continue;
            }
            Opened::Transcript(real) => real,
        };
        let key = path.to_string_lossy().into_owned();
        let state = index.file_state(&key)?;
        // A file that cannot be opened or read now keeps its offset and is tried next scan.
        let Ok(mut file) = std::fs::File::open(&real) else {
            continue;
        };
        let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
            continue;
        };
        let mut chunk = Chunk::default();
        let mut start = 0;
        if let Some(state) = state
            && length >= state.read_to
        {
            start = state.read_to;
            chunk.read_to = state.read_to;
            chunk.turns = state.turns;
            chunk.unreadable_lines = state.unreadable_lines;
        }
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
            continue;
        }
        let chunk = read_chunk(&bytes, chunk);
        summary.files += 1;
        index.record_file(
            &FileRead {
                path: &key,
                state: FileState {
                    read_to: chunk.read_to,
                    turns: chunk.turns,
                    unreadable_lines: chunk.unreadable_lines,
                },
                sessions: chunk.sessions.into_values().collect(),
                titles: chunk.titles.into_iter().collect(),
                replies: chunk.replies.into_values().collect(),
                crons: chunk.crons.into_values().collect(),
                tool_calls: chunk.tool_calls.into_values().collect(),
            },
            now,
        )?;
    }
    summary.sessions = index.session_count()?;
    summary.new_sessions = summary.sessions - before;
    Ok(summary)
}

/// The ids of the sessions a Catervas project's log names, read without changing anything: the file
/// is opened read-only and immutable, so SQLite makes no `-wal`, `-shm` or `-journal` file and
/// migrates nothing. A log that is not there, or cannot be read, names none. `immutable` does not
/// see frames of a live log not yet checkpointed, so a session recorded since counts as the owner's
/// until a later scan.
fn log_session_ids(root: &str) -> BTreeSet<String> {
    let path = Path::new(root).join(PROJECT_LOG);
    if !path.is_file() {
        return BTreeSet::new();
    }
    let escaped = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23");
    let uri = format!("file:{escaped}?mode=ro&immutable=1");
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let ids = Connection::open_with_flags(uri, flags).and_then(|connection| {
        let mut statement = connection
            .prepare("SELECT DISTINCT session_id FROM events WHERE session_id IS NOT NULL")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<BTreeSet<_>, _>>()
    });
    ids.unwrap_or_default()
}

/// What git could not say is nothing, except that it could not be run at all.
fn tolerate<T>(answer: Result<Option<T>, GitError>) -> Result<Option<T>, HistoryError> {
    match answer {
        Ok(value) => Ok(value),
        Err(error @ GitError::NotInstalled { .. }) => Err(error.into()),
        Err(_) => Ok(None),
    }
}

/// Whether a repository is somebody else's: it has a commit, git knows the owner's email, and no
/// commit on any ref was authored by it. With no commit, or no email, Catervas cannot tell and says
/// it is the owner's.
fn ownership_of(git: &Git) -> Result<Ownership, HistoryError> {
    if tolerate(git.head_summary().map(Some))?.flatten().is_none() {
        return Ok(Ownership::Yours);
    }
    let Some(email) = tolerate(git.user_email())? else {
        return Ok(Ownership::Yours);
    };
    match git.has_commit_by(&email) {
        Ok(false) => Ok(Ownership::Explored),
        Ok(true) | Err(GitError::NotARepository | GitError::CommandFailed { .. }) => {
            Ok(Ownership::Yours)
        }
        Err(error) => Err(error.into()),
    }
}

/// Places every session in its project, recomputed over all of them: folders are deleted and
/// repositories made between scans. `not_projects` are folders that are never a project (home, the
/// temporary folder), on top of `TEMPORARY_FOLDERS`.
///
/// Writes each session's place and root, and what every root is: a git repository or not, the
/// owner's or only explored, still on this computer or not.
///
/// # Errors
///
/// `Git` when the `git` program cannot be run; `Store` when the index refuses.
pub fn place_sessions(
    index: &mut MachineIndex,
    not_projects: &[PathBuf],
    now: DateTime<Utc>,
) -> Result<(), HistoryError> {
    let sessions = index.session_folders()?;
    let cwds: BTreeSet<&str> = sessions.iter().map(|(_, cwd)| cwd.as_str()).collect();
    let mut folders = Vec::new();
    for cwd in &cwds {
        let real = Path::new(cwd)
            .canonicalize()
            .ok()
            .filter(|real| real.is_dir());
        let git_root = match &real {
            Some(real) => tolerate(Git::open(real.clone()).main_worktree())?,
            None => None,
        };
        folders.push(Folder {
            path: (*cwd).to_string(),
            real: real.map(|real| real.to_string_lossy().into_owned()),
            git_root,
        });
    }
    let stored = index.stored_roots()?;
    let known: Vec<KnownRoot> = stored
        .iter()
        .map(|root| KnownRoot {
            root: root.root.clone(),
            is_git: root.is_git,
        })
        .collect();
    let blocked: Vec<String> = not_projects
        .iter()
        .map(|path| {
            path.canonicalize()
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let places = place_folders(&folders, &known, &blocked);
    let place_of: BTreeMap<&str, &Place> = cwds.iter().copied().zip(places.iter()).collect();

    let mut logs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for place in &places {
        if let Place::Project { root } = place
            && !logs.contains_key(root)
        {
            logs.insert(root.clone(), log_session_ids(root));
        }
    }
    let mut placements = Vec::new();
    let mut roots = BTreeSet::new();
    for (id, cwd) in &sessions {
        let located = place_session(id, cwd, place_of[cwd.as_str()], &logs, &blocked);
        let root = match &located {
            SessionPlace::Owner { root } | SessionPlace::Catervas { root } => {
                roots.insert(root.clone());
                Some(root.clone())
            }
            SessionPlace::Gone { folder } => Some(folder.clone()),
            SessionPlace::NotAProject => None,
        };
        placements.push(Placement {
            session_id: id.clone(),
            place: located.as_wire(),
            root,
        });
    }

    let mut projects = Vec::new();
    for root in roots {
        let present = Path::new(&root).is_dir();
        let before = stored.iter().find(|stored| stored.root == root);
        let (is_git, ownership) = if present {
            let git = Git::open(PathBuf::from(&root));
            let is_git = tolerate(git.main_worktree())?.as_deref() == Some(root.as_str());
            let ownership = if is_git {
                ownership_of(&git)?
            } else {
                Ownership::Yours
            };
            (is_git, ownership)
        } else {
            before.map_or((false, Ownership::Yours), |stored| {
                (stored.is_git, stored.ownership)
            })
        };
        let remote = if present && is_git {
            tolerate(Git::open(PathBuf::from(&root)).remote_url())?
                .as_deref()
                .and_then(normalise_remote)
        } else {
            None
        };
        projects.push(ProjectRow {
            root,
            is_git,
            ownership,
            present,
            remote,
        });
    }
    index.write_placements(&placements, &projects, now)?;
    Ok(())
}

/// A project the owner added by hand, and the found project it is the same as, when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedProject {
    /// What was kept.
    pub project: HandProject,
    /// The root of the found project it is the same as.
    pub same_as: Option<String>,
}

/// Where a folder given by hand is a project: its repository's main worktree, else itself, as
/// `place_folders` places a folder.
fn hand_root(folder: &Path, not_projects: &[PathBuf]) -> Result<String, HistoryError> {
    let real = folder
        .canonicalize()
        .ok()
        .filter(|real| real.is_dir())
        .ok_or_else(|| HistoryError::NoSuchFolder(folder.to_path_buf()))?;
    let git_root = tolerate(Git::open(real.clone()).main_worktree())?;
    let blocked: Vec<String> = not_projects
        .iter()
        .map(|path| {
            path.canonicalize()
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let named = Folder {
        path: real.to_string_lossy().into_owned(),
        real: Some(real.to_string_lossy().into_owned()),
        git_root,
    };
    match place_folders(&[named], &[], &blocked).pop() {
        Some(Place::Project { root }) => Ok(root),
        _ => Err(HistoryError::HandProject(
            HandProjectRefusal::NotAProjectFolder(folder.display().to_string()),
        )),
    }
}

/// Adds a project by hand, in a folder, a repository, or both, or renames the one with the same key
/// (its root, else its remote). Whether it is the same as a project a scan found is worked out when
/// `found` is asked, so that a later scan can still join them; `same_as` says which one it is now.
///
/// # Errors
///
/// `HandProject` for a blank name, no place or an address that is not a repository's, or a folder
/// that is home, the root or a temporary folder; `NoSuchFolder`; `Git` when the `git` program cannot
/// be run; `Store` when the index refuses.
pub fn add_project_by_hand(
    index: &mut MachineIndex,
    name: &str,
    folder: Option<&Path>,
    repository: Option<&str>,
    not_projects: &[PathBuf],
    now: DateTime<Utc>,
) -> Result<AddedProject, HistoryError> {
    let (name, remote) = check_hand_project(name, folder.is_some(), repository)
        .map_err(HistoryError::HandProject)?;
    let root = folder
        .map(|folder| hand_root(folder, not_projects))
        .transpose()?;
    let key = root.clone().or_else(|| remote.clone()).unwrap_or_default();
    let project = HandProject { name, root, remote };
    index.put_hand_project(&key, &project, now)?;
    let found = index.found()?;
    let keys: Vec<ProjectKey<'_>> = found
        .projects
        .iter()
        .map(|shown| ProjectKey {
            root: &shown.root,
            remote: shown.remote.as_deref(),
        })
        .collect();
    let same_as = match_hand_project(project.root.as_deref(), project.remote.as_deref(), &keys)
        .map(|at| found.projects[at].root.clone());
    Ok(AddedProject { project, same_as })
}

/// Finds the sessions that are copies of one another (a session teleported from another computer
/// keeps its replies, with new ids) and the replies to count once, and writes them, replacing what an
/// earlier scan wrote: a third copy turns a merge into a maybe. Run after `place_sessions`.
///
/// # Errors
///
/// `Store` when the index refuses.
pub fn find_copies(index: &mut MachineIndex) -> Result<(), HistoryError> {
    let sessions = index.session_replies()?;
    index.replace_copies(&copies_among(&sessions))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::{DateTime, TimeZone, Utc};
    use rusqlite::Connection;

    use super::*;
    use crate::files::fixtures::TempProject;
    use crate::machine::{MACHINE_DB, open_machine_index};
    use catervas_core::claude_history::fixtures::{
        CLAUDE_CODE_2_1_296, CLAUDE_CODE_2_1_296_CONNECTORS, CRON_CREATE_LINES, CronCreateLine,
        a_cron_create_line, a_reply_line, a_turn_line,
    };
    use catervas_core::pricing::Usage;

    const SESSION: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER: &str = "00000000-0000-4000-8000-000000000000";
    const CWD: &str = "/home/me/shop";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
            .single()
            .expect("a valid time")
    }

    fn usage(input: u64, output: u64, read: u64, write: u64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: read,
            cache_write_tokens: write,
        }
    }

    /// A temporary Claude folder with `projects/-home-me-shop/`, and the index beside it.
    struct Setup {
        temp: TempProject,
    }

    impl Setup {
        fn new(name: &str) -> Self {
            let temp = TempProject::new(name);
            std::fs::create_dir_all(temp.root.join("claude/projects/-home-me-shop"))
                .expect("a projects folder");
            Self { temp }
        }

        fn claude(&self) -> PathBuf {
            self.temp.root.join("claude")
        }

        fn db(&self) -> PathBuf {
            self.temp.root.join("state").join(MACHINE_DB)
        }

        fn transcript(&self, name: &str) -> PathBuf {
            self.claude()
                .join("projects/-home-me-shop")
                .join(format!("{name}.jsonl"))
        }

        fn read(&self) -> ReadSummary {
            let mut index = open_machine_index(&self.db(), now()).expect("the index opens");
            read_transcripts(&mut index, &self.claude(), now()).expect("the scan reads")
        }

        fn rows(&self, sql: &str) -> Vec<String> {
            query(&self.db(), sql)
        }
    }

    fn query(db: &Path, sql: &str) -> Vec<String> {
        let connection = Connection::open(db).expect("the index file opens");
        let mut statement = connection.prepare(sql).expect("a statement");
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("rows")
    }

    fn dump(setup: &Setup) -> Vec<String> {
        let mut all = Vec::new();
        for sql in [
            "SELECT printf('%s|%d|%d|%d', path, read_to, turns, unreadable_lines) FROM transcript_files ORDER BY path",
            "SELECT printf('%s|%s|%s|%s|%s|%s|%s', session_id, cwd, git_branch, version, entrypoint, title, last_at) FROM sessions ORDER BY session_id",
            "SELECT printf('%s|%s|%d|%d|%d|%d', reply_key, session_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens) FROM replies ORDER BY reply_key",
        ] {
            all.extend(setup.rows(sql));
        }
        all
    }

    const REPLIES: &str = "SELECT printf('%s|%s|%d|%d|%d|%d', reply_key, session_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens) FROM replies ORDER BY reply_key";

    #[test]
    fn reads_a_checked_versions_transcript() {
        let setup = Setup::new("reads-checked-transcript");
        std::fs::write(setup.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("a transcript");
        let summary = setup.read();
        assert_eq!(
            summary,
            ReadSummary {
                files: 1,
                sessions: 1,
                new_sessions: 1,
                refused: 0
            }
        );
        assert_eq!(
            setup.rows("SELECT printf('%s|%s|%s|%s|%s|%s|%s', cwd, git_branch, version, entrypoint, title, first_at, last_at) FROM sessions"),
            vec![
                "/home/me/shop|main|2.1.296|cli|Fix the checkout total|2026-10-12T09:00:00.000Z|2026-10-12T09:01:10.000Z"
            ]
        );
        assert_eq!(
            setup.rows(REPLIES),
            vec![
                format!("msg_01A|{SESSION}|10|40|1000|200"),
                format!("req_01B|{SESSION}|5|7|0|0")
            ]
        );
        assert_eq!(
            setup.rows("SELECT printf('%d|%d', turns, unreadable_lines) FROM transcript_files"),
            vec!["6|2"]
        );
    }

    /// Guard: checked by mutation (store a text field), not watched to fail.
    #[test]
    fn keeps_no_conversation_text() {
        let setup = Setup::new("keeps-no-text");
        std::fs::write(setup.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("a transcript");
        setup.read();
        let bytes = std::fs::read(setup.db()).expect("the index file");
        let marker = b"NEVER-KEPT-TEXT";
        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
    }

    #[test]
    fn reads_only_what_was_appended() {
        let setup = Setup::new("reads-appended");
        let path = setup.transcript(SESSION);
        std::fs::write(&path, CLAUDE_CODE_2_1_296).expect("a transcript");
        setup.read();
        let before = dump(&setup);
        let again = setup.read();
        assert_eq!((again.new_sessions, again.sessions), (0, 1));
        assert_eq!(dump(&setup), before);

        let reply = a_reply_line(
            SESSION,
            CWD,
            "2026-10-12T09:02:00Z",
            "msg_01C",
            "claude-opus-5-5",
            usage(1, 2, 0, 0),
        );
        let half = r#"{"type":"user","sessionId":"#;
        let whole = format!("{CLAUDE_CODE_2_1_296}{reply}\n");
        std::fs::write(&path, format!("{whole}{half}")).expect("a longer transcript");
        setup.read();
        assert_eq!(setup.rows(REPLIES).len(), 3);
        assert!(setup.rows(REPLIES)[0].starts_with("msg_01A|"));
        assert_eq!(setup.rows(REPLIES)[1], format!("msg_01C|{SESSION}|1|2|0|0"));
        assert_eq!(
            setup.rows("SELECT CAST(read_to AS TEXT) FROM transcript_files"),
            vec![whole.len().to_string()]
        );

        // A later turn in a subfolder, on another branch, does not move where the session started.
        let finished = a_turn_line(SESSION, "/home/me/shop/sub", "2026-10-12T09:03:00Z")
            .replace("\"main\"", "\"later\"")
            .replace("2.1.296", "2.1.9")
            .replace("\"cli\"", "\"vscode\"");
        std::fs::write(&path, format!("{whole}{finished}\n")).expect("a finished line");
        setup.read();
        assert_eq!(
            setup.rows("SELECT printf('%s|%s|%s|%s|%s', cwd, git_branch, version, entrypoint, first_at) FROM sessions"),
            vec!["/home/me/shop|main|2.1.296|cli|2026-10-12T09:00:00.000Z"]
        );
        assert_eq!(
            setup.rows("SELECT CAST(turns AS TEXT) FROM transcript_files"),
            vec!["8"]
        );
        assert_eq!(
            setup.rows("SELECT last_at FROM sessions"),
            vec!["2026-10-12T09:03:00.000Z"]
        );

        let first = CLAUDE_CODE_2_1_296.lines().next().expect("a first line");
        std::fs::write(&path, format!("{first}\n")).expect("a replaced transcript");
        setup.read();
        assert_eq!(
            setup.rows("SELECT printf('%d|%d', turns, unreadable_lines) FROM transcript_files"),
            vec!["1|0"]
        );
        assert_eq!(setup.rows(REPLIES).len(), 3);
        assert_eq!(
            setup.rows(REPLIES)[0],
            format!("msg_01A|{SESSION}|10|40|1000|200")
        );
    }

    fn resumed(session: &str, shift: bool) -> String {
        let mut text = a_turn_line(
            session,
            CWD,
            if shift {
                "2026-10-12T10:00:00Z"
            } else {
                "2026-10-12T09:00:00Z"
            },
        );
        text.push('\n');
        for line in CLAUDE_CODE_2_1_296.lines().skip(2).take(3) {
            let line = line.replace(SESSION, session);
            let line = if shift {
                line.replace("T09:0", "T10:0")
            } else {
                line
            };
            text.push_str(&line);
            text.push('\n');
        }
        let fresh = a_reply_line(
            session,
            CWD,
            "2026-10-12T10:02:00Z",
            "msg_02A",
            "claude-opus-5-5",
            usage(3, 4, 0, 0),
        );
        text.push_str(&fresh);
        text.push('\n');
        text
    }

    #[test]
    fn counts_a_resumed_session_once() {
        let setup = Setup::new("counts-resumed-once");
        std::fs::write(setup.transcript(OTHER), resumed(OTHER, true)).expect("the resumed file");
        std::fs::write(setup.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("the original");
        setup.read();
        assert_eq!(
            setup.rows(REPLIES),
            vec![
                format!("msg_01A|{SESSION}|10|40|1000|200"),
                format!("msg_02A|{OTHER}|3|4|0|0"),
                format!("req_01B|{SESSION}|5|7|0|0"),
            ]
        );

        // The resumed file read after the original: the reply keeps its earliest time and session.
        let late = Setup::new("counts-resumed-late");
        std::fs::write(late.transcript("zzz"), resumed(OTHER, true)).expect("the resumed file");
        std::fs::write(late.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("the original");
        late.read();
        assert_eq!(
            late.rows(
                "SELECT printf('%s|%s', session_id, at) FROM replies WHERE reply_key = 'msg_01A'"
            ),
            vec![format!("{SESSION}|2026-10-12T09:00:05.000Z")]
        );

        // Started together, the smaller id owns the reply, even when its file is read last.
        let tied = Setup::new("counts-resumed-tied");
        std::fs::write(tied.transcript("zzz"), resumed(OTHER, false)).expect("the resumed file");
        std::fs::write(tied.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("the original");
        tied.read();
        assert_eq!(
            tied.rows(REPLIES),
            vec![
                format!("msg_01A|{OTHER}|10|40|1000|200"),
                format!("msg_02A|{OTHER}|3|4|0|0"),
                format!("req_01B|{OTHER}|5|7|0|0"),
            ]
        );
        // The same, read the other way round: the smaller id's file first.
        let tied_first = Setup::new("counts-resumed-tied-first");
        std::fs::write(tied_first.transcript(OTHER), resumed(OTHER, false)).expect("resumed");
        std::fs::write(tied_first.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("original");
        tied_first.read();
        assert_eq!(tied_first.rows(REPLIES), tied.rows(REPLIES));

        let subagents = setup
            .claude()
            .join(format!("projects/-home-me-shop/{SESSION}/subagents"));
        std::fs::create_dir_all(&subagents).expect("a subagents folder");
        let sub = a_reply_line(
            SESSION,
            CWD,
            "2026-10-12T09:05:00Z",
            "msg_SUB",
            "claude-opus-5-5",
            usage(2, 2, 0, 0),
        );
        std::fs::write(subagents.join("agent-a.jsonl"), format!("{sub}\n"))
            .expect("a subagent file");
        let summary = setup.read();
        assert_eq!(
            (summary.files, summary.sessions, summary.new_sessions),
            (3, 2, 0)
        );
        assert!(
            setup
                .rows(REPLIES)
                .contains(&format!("msg_SUB|{SESSION}|2|2|0|0"))
        );
    }

    #[test]
    fn keeps_the_largest_counters_whatever_order_a_files_lines_come_in() {
        let setup = Setup::new("keeps-largest-in-a-file");
        let reply = |at: &str, key: &str, usage| {
            a_reply_line(SESSION, CWD, at, key, "claude-opus-5-5", usage)
        };
        let lines = [
            reply(
                "2026-10-12T09:00:00Z",
                "big_first",
                usage(10, 40, 1000, 200),
            ),
            reply("2026-10-12T09:00:01Z", "big_first", usage(5, 1, 900, 0)),
            reply("2026-10-12T09:00:02Z", "big_last", usage(5, 1, 900, 0)),
            reply("2026-10-12T09:00:03Z", "big_last", usage(10, 40, 1000, 200)),
        ];
        std::fs::write(setup.transcript(SESSION), lines.join("\n") + "\n").expect("a file");
        setup.read();
        assert_eq!(
            setup.rows("SELECT printf('%s|%d|%d|%d|%d|%s', reply_key, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, at) FROM replies ORDER BY reply_key"),
            vec![
                "big_first|10|40|1000|200|2026-10-12T09:00:00.000Z",
                "big_last|10|40|1000|200|2026-10-12T09:00:02.000Z"
            ]
        );
    }

    #[test]
    fn moves_a_sessions_start_to_an_earlier_turn_in_a_later_file() {
        let setup = Setup::new("moves-start-earlier");
        let late = a_turn_line(SESSION, CWD, "2026-10-12T09:05:00Z");
        std::fs::write(setup.transcript("a"), format!("{late}\n")).expect("a file");
        setup.read();
        let early = a_turn_line(SESSION, "/home/me/other", "2026-10-12T09:00:00Z")
            .replace("\"main\"", "\"dev\"")
            .replace("2.1.296", "2.1.0")
            .replace("\"cli\"", "\"vscode\"");
        std::fs::write(setup.transcript("b"), format!("{early}\n")).expect("a file");
        setup.read();
        assert_eq!(
            setup.rows("SELECT printf('%s|%s|%s|%s|%s|%s', cwd, git_branch, version, entrypoint, first_at, last_at) FROM sessions"),
            vec!["/home/me/other|dev|2.1.0|vscode|2026-10-12T09:00:00.000Z|2026-10-12T09:05:00.000Z"]
        );
    }

    #[test]
    fn keeps_the_largest_counters_when_a_later_file_repeats_a_reply() {
        let setup = Setup::new("keeps-largest-counters");
        std::fs::write(setup.transcript(SESSION), CLAUDE_CODE_2_1_296).expect("the original");
        setup.read();
        // The fork repeats the reply with every counter smaller, then another file with every one larger.
        let repeat =
            |at: &str, usage| a_reply_line(OTHER, CWD, at, "msg_01A", "claude-opus-5-5", usage);
        let smaller = repeat("2026-10-12T09:00:30Z", usage(1, 1, 1, 1));
        std::fs::write(setup.transcript("zzz"), format!("{smaller}\n")).expect("a fork");
        setup.read();
        assert_eq!(
            setup.rows(REPLIES)[0],
            format!("msg_01A|{SESSION}|10|40|1000|200")
        );
        let larger = repeat("2026-10-12T09:00:40Z", usage(11, 41, 1001, 201));
        std::fs::write(setup.transcript("zzy"), format!("{larger}\n")).expect("a fork");
        setup.read();
        assert_eq!(
            setup.rows(REPLIES)[0],
            format!("msg_01A|{SESSION}|11|41|1001|201")
        );
    }

    #[cfg(unix)]
    #[test]
    fn opens_the_canonical_path_that_was_checked() {
        use std::os::unix::fs::symlink;

        let setup = Setup::new("opens-canonical");
        let projects = setup
            .claude()
            .join("projects")
            .canonicalize()
            .expect("projects");
        std::fs::write(setup.transcript("real"), "x\n").expect("a file");
        symlink(setup.transcript("real"), setup.transcript("link")).expect("a link");
        let real = projects.join("-home-me-shop/real.jsonl");
        assert_eq!(
            classify(&setup.transcript("link"), &projects),
            Opened::Transcript(real)
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_whatever_is_not_a_transcript() {
        use std::os::unix::fs::symlink;

        let setup = Setup::new("refuses-not-transcripts");
        let claude = setup.claude();
        let marker = a_turn_line("s", "/SECRET-MARKER", "2026-10-12T09:00:00Z");
        std::fs::write(claude.join(".credentials.json"), "SECRET-MARKER").expect("credentials");
        let outside = setup.temp.root.join("outside.jsonl");
        std::fs::write(&outside, format!("{marker}\n")).expect("an outside transcript");
        let folder = claude.join("projects/-x");
        std::fs::create_dir_all(folder.join("s/tool-results")).expect("folders");
        symlink(claude.join(".credentials.json"), folder.join("evil.jsonl")).expect("a link");
        symlink(&outside, folder.join("out.jsonl")).expect("a link");
        std::fs::create_dir_all(claude.join("jobs/j")).expect("a jobs folder");
        std::fs::write(claude.join("jobs/j/state.json"), format!("{marker}\n")).expect("a job");
        std::fs::write(claude.join("history.jsonl"), format!("{marker}\n")).expect("a history");
        std::fs::write(folder.join("s/tool-results/r.jsonl"), format!("{marker}\n"))
            .expect("a result");

        // Inside `projects/`: the credentials, a link to them, and files that are not transcripts.
        std::fs::write(folder.join(".credentials.json"), format!("{marker}\n")).expect("a copy");
        symlink(folder.join(".credentials.json"), folder.join("c.jsonl")).expect("a link");
        std::fs::write(folder.join("notes.json"), format!("{marker}\n")).expect("notes");
        std::fs::create_dir_all(folder.join("memory")).expect("a memory folder");
        std::fs::write(folder.join("memory/m.md"), format!("{marker}\n")).expect("a memory");

        let summary = setup.read();
        assert_eq!(summary.refused, 3);
        assert_eq!(summary.files, 0);
        assert!(setup.rows("SELECT path FROM transcript_files").is_empty());
        let bytes = std::fs::read(setup.db()).expect("the index file");
        let secret = b"SECRET-MARKER";
        assert!(!bytes.windows(secret.len()).any(|window| window == secret));
    }

    #[test]
    fn merges_a_replys_tools_from_every_line() {
        let tools = |text: &str| -> Vec<Vec<String>> {
            read_chunk(text.as_bytes(), Chunk::default())
                .replies
                .values()
                .map(|reply| reply.tools.iter().map(|tool| tool.id.clone()).collect())
                .collect()
        };
        let expected = vec![vec!["toolu_01B".to_string(), "toolu_01C".to_string()]];
        assert_eq!(tools(CRON_CREATE_LINES), expected);
        assert_eq!(
            tools(&format!("{CRON_CREATE_LINES}{CRON_CREATE_LINES}")),
            expected
        );
    }

    const JOBS: &str = "SELECT printf('%s|%s|%s|%s|%s|%s|%s|%s|%s|%s', origin, file, name, kind, display_name, schedules, folder, expires_at, state_at, last_seen) FROM jobs ORDER BY origin, file, name";

    fn durable_cron_line() -> String {
        a_cron_create_line(&CronCreateLine {
            session_id: SESSION,
            cwd: CWD,
            at: "2026-10-12T09:03:00Z",
            message_id: "msg_01D",
            tool_use_id: "toolu_01D",
            cron: "0 0 * * *",
            recurring: None,
            durable: Some(true),
        })
    }

    #[test]
    fn reads_session_crons_from_transcripts() {
        let setup = Setup::new("reads-session-crons");
        let text = format!(
            "{CLAUDE_CODE_2_1_296}{CRON_CREATE_LINES}{}\n",
            durable_cron_line()
        );
        std::fs::write(setup.transcript(SESSION), text).expect("a transcript");
        setup.read();
        let expected = vec![format!(
            "claude_session|{SESSION}|toolu_01C|session_cron|Session cron */10 * * * *|[\"*/10 * * * *\"]|/home/me/shop|2026-10-19T09:02:02.000Z|2026-10-12T09:02:02.000Z|2026-10-12T12:00:00.000Z"
        )];
        assert_eq!(setup.rows(JOBS), expected);
        setup.read();
        assert_eq!(setup.rows(JOBS), expected);
    }

    #[test]
    fn reads_a_transcript_again_without_counting_twice() {
        let setup = Setup::new("reads-again-without-counting-twice");
        let text = format!(
            "{CLAUDE_CODE_2_1_296}{CRON_CREATE_LINES}{}\n",
            durable_cron_line()
        );
        std::fs::write(setup.transcript(SESSION), text).expect("a transcript");
        setup.read();
        let everything = |setup: &Setup| {
            let mut all = dump(setup);
            all.extend(setup.rows(JOBS));
            all
        };
        let before = everything(&setup);
        let connection = Connection::open(setup.db()).expect("the index file opens");
        connection
            .execute(
                "UPDATE transcript_files SET read_to = 0, turns = 0, unreadable_lines = 0",
                [],
            )
            .expect("migration 2's reset");
        drop(connection);
        let again = setup.read();
        assert_eq!((again.new_sessions, again.sessions), (0, 1));
        assert_eq!(everything(&setup), before);
    }

    /// Guard: checked by mutation (store a prompt or a tool's input), not watched to fail.
    #[test]
    fn keeps_no_prompt_or_tool_input() {
        let setup = Setup::new("keeps-no-prompt");
        let text = format!(
            "{CLAUDE_CODE_2_1_296}{CRON_CREATE_LINES}{}\n",
            durable_cron_line()
        );
        std::fs::write(setup.transcript(SESSION), text).expect("a transcript");
        setup.read();
        let bytes = std::fs::read(setup.db()).expect("the index file");
        let marker = b"NEVER-KEPT-TEXT";
        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
    }

    const CONNECTORS_SESSION: &str = "33333333-3333-4333-8333-333333333333";
    const TOOL_CALLS: &str = "SELECT printf('%s|%s|%s|%d|%s', tool_use_id, reply_key, tool_name, touches_spreadsheet, at) FROM tool_calls ORDER BY tool_use_id";

    #[test]
    fn keeps_each_tool_call_once() {
        let setup = Setup::new("keeps-each-tool-call-once");
        std::fs::write(
            setup.transcript(CONNECTORS_SESSION),
            CLAUDE_CODE_2_1_296_CONNECTORS,
        )
        .expect("a transcript");
        setup.read();
        let expected = vec![
            "toolu_1|msg_T1|mcp__claude_ai_Zoho_Books__list_invoices|0|2026-10-12T09:10:00.000Z",
            "toolu_2|msg_T2|mcp__claude_ai_Zoho_Books__get_invoice|0|2026-10-12T09:10:10.000Z",
            "toolu_3|msg_T3|mcp__claude_ai_Zoho_Books__create_expense|0|2026-10-12T09:10:20.000Z",
            "toolu_4|msg_T3|Read|1|2026-10-12T09:10:21.000Z",
            "toolu_8|msg_T8|mcp__plugin_product-management_slack__send_message|0|2026-10-12T09:11:20.000Z",
            "toolu_9|msg_T8|Edit|1|2026-10-12T09:11:21.000Z",
        ];
        assert_eq!(setup.rows(TOOL_CALLS), expected);

        // Another session's file repeating a reply adds no row and moves none.
        let other = CLAUDE_CODE_2_1_296_CONNECTORS
            .lines()
            .take(1)
            .map(|line| line.replace(CONNECTORS_SESSION, OTHER))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(setup.transcript(OTHER), format!("{other}\n")).expect("a second file");
        setup.read();
        assert_eq!(setup.rows(TOOL_CALLS), expected);

        // Migration 3 reads every file again; nothing changes.
        let everything = |setup: &Setup| {
            let mut all = dump(setup);
            all.extend(setup.rows(TOOL_CALLS));
            all
        };
        let before = everything(&setup);
        let connection = Connection::open(setup.db()).expect("the index file opens");
        connection
            .execute(
                "UPDATE transcript_files SET read_to = 0, turns = 0, unreadable_lines = 0",
                [],
            )
            .expect("migration 3's reset");
        drop(connection);
        setup.read();
        assert_eq!(everything(&setup), before);
    }

    /// Guard: checked by mutation (store a tool's input or path), not watched to fail.
    #[test]
    fn keeps_no_tool_input() {
        let setup = Setup::new("keeps-no-tool-input");
        std::fs::write(
            setup.transcript(CONNECTORS_SESSION),
            CLAUDE_CODE_2_1_296_CONNECTORS,
        )
        .expect("a transcript");
        setup.read();
        let bytes = std::fs::read(setup.db()).expect("the index file");
        let marker = b"NEVER-KEPT-TEXT";
        assert!(!bytes.windows(marker.len()).any(|window| window == marker));
        assert!(!setup.rows(TOOL_CALLS).is_empty());
    }

    #[test]
    fn counts_what_it_could_not_read() {
        let setup = Setup::new("counts-unreadable");
        std::fs::write(setup.transcript(SESSION), "not json\nnot json\nnot json\n")
            .expect("a file");
        setup.read();
        assert_eq!(
            setup.rows("SELECT printf('%d|%d', turns, unreadable_lines) FROM transcript_files"),
            vec!["0|3"]
        );
        assert!(setup.rows("SELECT session_id FROM sessions").is_empty());
    }
}
