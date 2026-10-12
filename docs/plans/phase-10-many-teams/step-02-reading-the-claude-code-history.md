# Phase 10, step 02: Reading the Claude Code history

Status: ready
Branch: `phase/10-many-teams` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 8.4, 8.6; no functional requirement of section 7 (ADR 0059)
Depends on: none beyond `main`, with ADR 0059 and `docs/design/many-teams.md` on this branch. No code of phase 9 is consumed, though this branch carries it; nothing of step 01 (no code) or of steps 03 on.
Mockups: none; this step has no screen (step 06 draws them).
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, one round: not ready on B1 (placement by scan order), B2 (path printing), B3 (build dir), B4 (phase 9 dependency); folded with N1–N11.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at `main` (28600da).

## Goal

`catervas discover` reads the owner's Claude Code history on this computer into a machine-level index, `machine.db` in Catervas's state folder, and prints what it found: the projects they worked on, newest first, each with when it was last worked on, its sessions and its tokens, Catervas's own agents' sessions counted under their team's project, then the explored repositories, the folders no longer on the computer, and the sessions in no project. A second run reads only what was appended since. `catervas discover --forget` deletes the index and reads nothing. Only metadata is kept, never a conversation's text; Claude Code's credentials and anything outside its transcript store are never opened.

Out of scope, each a later step's: jobs, session crons and `~/.claude/jobs` (03); connectors and tool calls (03b); claude.ai's export, the `sources` of a record and teleported copies with a new id (04); serve, the daemon, a daily rescan and every screen (05 to 06); `cleanupPeriodDays`, the ignore list and Pause finding (06); same-project by remote and links (06b); the reminder's signals (07); prices (08).

## Decisions

### Where each part lives

- **`catervas-core` decides; `catervas-store` reads and writes; `catervas-cli` asks and prints.** Core (`claude_history`): reading one transcript line, counting a reply once, placing folders and sessions. Store (`machine`, `claude_history`): `machine.db`, walking and reading the transcript files, git, a Catervas project's log. CLI (`discover`): the paths from the environment, the store calls, the report. Rejected: a crate of its own, which would hold one module.
- **Phase 9 executes beside this step**, so nothing here touches `serve.rs`, `crates/runtime/src/daemon/**` or `apps/web/**`. Shared files get additions only: one `pub mod` line in each crate's `lib.rs`, three methods appended after `Git::check_branch_name` (`git.rs:445`), a second migration list in `migrations.rs` below `MIGRATIONS` (`:17`, left as it is), one variant and one arm in `crates/cli/src/lib.rs`.
- **New modules' doc comments:** core `/// What Claude Code's history says: one transcript line, and where each session belongs.`; store `machine` `/// The machine-level index of the owner's Claude Code history, apart from every project's log.` and `claude_history` `/// Reading Claude Code's transcripts into the machine index, and placing each session in its project.`; cli `discover` `/// \`catervas discover\`: what Catervas found in the Claude Code history.`

### The index

- **`<state>/machine.db`**, `state_dir` (`crates/cli/src/state.rs:10`) joined with `MACHINE_DB = "machine.db"`; the folder made by `make_state_dir` (`:24`). With no state folder, `discover` is refused: "Catervas has no state folder; set HOME".
- **It reuses the store's migration runner.** `apply_through`'s loop (`migrations.rs:102`) moves into a private `apply_list(connection: &mut Connection, list: &[Migration], last_version: i64, now: DateTime<Utc>) -> Result<(), StoreError>`; `apply` and `apply_through` call it with `MIGRATIONS` unchanged, and `pub(crate) fn apply_machine(connection: &mut Connection, now: DateTime<Utc>) -> Result<(), StoreError>` with `MACHINE_MIGRATIONS: [Migration; 1]`, version 1 `include_str!("migrations/machine/0001_history.sql")`. The file goes under `migrations/machine/` so that a project log's numbering and the index's never meet in one folder. `machine.db` keeps its own `schema_migrations`. Rejected: a second runner, the same 30 lines.
- **SQLite's default rollback journal, `busy_timeout` 30 s, `foreign_keys` on.** Rejected: write-ahead logging as the event log has, which needs `write_ahead` made public and leaves `-wal` and `-shm` files for `--forget` to delete; the index has one writer at a time.
- **Tables** (`0001_history.sql`, every table `STRICT`, times RFC 3339 UTC as the log stamps them):
  - `transcript_files (path TEXT PRIMARY KEY, read_to INTEGER NOT NULL, turns INTEGER NOT NULL, unreadable_lines INTEGER NOT NULL, read_at TEXT NOT NULL)`: the byte offset read up to, and counts of lines read as turns and lines that could not be read.
  - `sessions (session_id TEXT PRIMARY KEY, cwd TEXT, git_branch TEXT, version TEXT, entrypoint TEXT, title TEXT, first_at TEXT, last_at TEXT, place TEXT CHECK (place IN ('owner','catervas','gone','not_a_project')), project_root TEXT)`.
  - `replies (reply_key TEXT PRIMARY KEY, session_id TEXT NOT NULL, model TEXT NOT NULL, at TEXT NOT NULL, input_tokens INTEGER NOT NULL, output_tokens INTEGER NOT NULL, cache_read_tokens INTEGER NOT NULL, cache_write_tokens INTEGER NOT NULL)`, index `replies_by_session (session_id)`. The token columns are `pricing::Usage`'s fields (`crates/core/src/pricing.rs:85`), so step 08 prices a row as it is.
  - `projects (root TEXT PRIMARY KEY, is_git INTEGER NOT NULL, ownership TEXT NOT NULL CHECK (ownership IN ('yours','explored')), present INTEGER NOT NULL, first_seen_at TEXT NOT NULL, checked_at TEXT NOT NULL)`. Rows are never deleted, so a root seen once stays known (below); `found` lists only roots with sessions placed under them.
  - No `sources` column: step 02 has one source, the local store; step 04 adds the column with its migration.
- **`--forget` deletes `machine.db` and `machine.db-journal`** when present; it opens nothing and reads nothing. Rejected: also clearing transcripts or project logs, which are Claude Code's and the teams' own, not Catervas's index.

### Reading a transcript

- **The store is `$CLAUDE_CONFIG_DIR` when it is set and not empty, else `$HOME/.claude`** (`claude_dir` in `state.rs`); neither: refused, "Catervas can't tell where Claude Code keeps its history; set HOME". With no `projects/` folder in it, `discover` prints "no Claude Code history on this computer" and writes nothing. Rejected: `$HOME/.claude` first, which reads the wrong store for someone who moved Claude Code's, as Claude Code itself honours the variable.
- **Which files are read:** `projects/<folder>/<session>.jsonl` and `projects/<folder>/<session>/subagents/<agent>.jsonl` (a subagent's lines carry its parent's `sessionId`, so its replies count under the parent), in path order. Nothing else under `projects/` (`tool-results/`, `memory/`) and nothing outside it: not `.credentials.json`, `jobs/`, `history.jsonl`, `settings.json`, `.claude.json` or `stats-cache.json`.
- **Refused paths.** A file is opened only when it is a regular file whose canonical path is inside the canonical `projects/` and whose name is not `.credentials.json`. A link pointing out of `projects/` is refused before it is opened, counted in `refused`, and given no `transcript_files` row.
- **Incremental.** Reading starts at `read_to` and stops after the last `\n`, so a line being written is read next time. A file shorter than its `read_to` (replaced) is read again from 0, its `turns` and `unreadable_lines` reset to 0 before counting. Each file's records and its new `read_to` are written in one `IMMEDIATE` transaction, so a stopped scan never skips or half-counts lines. A file that cannot be opened between listing and reading keeps its offset and is tried next scan.
- **Fields read, and only these** (checked against Claude Code 2.1.296 on 2026-10-12, `claude --version`):
  - `user`, `attachment` and `assistant` lines are turns: `sessionId`, `cwd`, `timestamp` required; `gitBranch`, `version`, `entrypoint` when present.
  - An `assistant` line is also a reply when `message.model` is not `<synthetic>` (Claude Code's placeholder for its own error messages, with zero usage) and it has a key: `message.id`, else `requestId`. Its usage: `message.usage.input_tokens`, `output_tokens`, `cache_read_input_tokens` as `cache_read_tokens`, `cache_creation_input_tokens` as `cache_write_tokens`, each 0 when absent. No key, no reply.
  - `ai-title`: `sessionId` and `aiTitle`, trimmed, cut to `TITLE_LIMIT = 200` characters, an empty one ignored.
  - Any other `type` (`queue-operation`, `last-prompt`, `cost-state`, `atis-latch`, and every later one) is `Other`. `cost-state` totals are not read: they would count usage twice.
  - A line that is not JSON, or a turn or title missing a required field or with an unreadable `timestamp`, is `Unreadable`.
  - `message.content`, `lastPrompt`, `attachment` and every other field are never read, so no conversation text can reach the index.
- **A Claude Code version Catervas cannot read** degrades: a transcript file with unreadable lines and no turn is an unreadable session, and `discover` says "couldn't read <n> sessions from this version of Claude Code"; other unreadable lines are "skipped <n> lines it couldn't read". Neither fails the scan. `CHECKED_VERSIONS = ["2.1.296"]` names each version with a fixture; a version is added with its fixture and its test.

### One record per session and per reply

- **A session is keyed by `sessionId`.** Merged in SQL on upsert: `first_at` the earliest, `last_at` the latest, `cwd`, `git_branch`, `version` and `entrypoint` those of its earliest turn (the folder it started in), `title` the latest read. Rejected: the latest turn's, which would move a session to whatever folder it last moved into.
- **A reply is keyed by its message id, else its request id**, as the line gives it, unprefixed. Claude Code writes one line per content block of one reply, each with that reply's usage, and a resumed or forked session's file repeats earlier replies. So a reply is one row: each counter the largest seen (`once_per_reply`), whatever order the lines arrive in, and its `session_id` that of the session with the earliest `first_at` among those that recorded it, ties going to the lexically smallest id (compared at each upsert, with the file's own session already merged). Rejected: summing lines, which counts a three-block reply three times; the first session read, which depends on file names.

### Placing sessions in projects

- **Folders first, then sessions.** Store gathers, for each distinct session `cwd`, its canonical path (none when the folder is gone) and its git root; core places each folder, then each session. Paths are compared by `std::path::Path::starts_with`, component by component.
- **The git root is the repository's main worktree**, the first `worktree` line of `git worktree list --porcelain` run in the folder (`Git::main_worktree`), so every worktree, Claude Code's `.claude/worktrees/*` among them, folds into its repository; a bare repository's root is its own folder. Not a repository: `None`. git that cannot be run at all is `GitError::NotInstalled`, which refuses the scan with git's sentence. Rejected: the parent of `--git-common-dir`, wrong for `--separate-git-dir`.
- **Not projects:** home (`HOME` from `io.env`; when unset, no folder is excluded as home), `TMPDIR` when set, and `TEMPORARY_FOLDERS = ["/", "/tmp", "/var/tmp"]`, each canonical when it exists. None of them is ever a root, and a git root that holds one (a home folder kept in git) is ignored as if there were no git, so it does not swallow every folder below it. The temporary folders extend the design's "home is not a project", from its Testing list's `/tmp`. Rejected: treating `/tmp` as an ordinary folder, which would make it one project swallowing every scratch folder under it.
- **A folder's place, in this order** (the design's section 2):
  1. a not-project folder is `NotAProject`;
  2. else its git root, when it has one;
  3. else the longest known root that contains it: for a folder that still exists, only known roots with git (`projects.is_git = 1`) and this scan's git roots; for a gone folder (a deleted worktree), any known root. So placement never depends on the order of earlier scans;
  4. else, when it is gone, `Gone`;
  5. else (a folder with no git) the outermost session folder with no git that contains it and is not a not-project folder, which may be itself, so its subfolders fold into it.
- **A session's place:** `catervas` under root `R` when its `cwd` has a `.catervas/local/` segment (`R` is the path before `/.catervas/`), or when its folder's root is `R` and `R`'s log names its id; else `owner` under its folder's root; else `gone` or `not_a_project` as its folder is. A Catervas project's log is `<root>/.catervas/local/catervas.db` (`PROJECT_LOG`, the same path as `crates/cli/src/project.rs:18`), opened through SQLite's URI `file:<path>?mode=ro&immutable=1`, so no `-wal`, `-shm` or `-journal` is made and nothing is migrated; its ids `SELECT DISTINCT session_id FROM events WHERE session_id IS NOT NULL`; a root with no log has no ids. `immutable` does not see a live log's frames not yet checkpointed, so a session recorded since then counts as the owner's until a later scan (ruling N10). Catervas's session id is Claude Code's (project plan, phase 3 decisions), so the ids match.
- **Yours or explored**, for each root with git, checked every scan: explored when the repository has a commit (`head_summary` is `Some`), git knows an email in it (`Git::user_email`, `git config --get user.email`, the repository's own or the global one), and no commit on any ref was authored by it (`Git::has_commit_by`: `git log --all -1 --format=%H --fixed-strings --regexp-ignore-case --author=<email>`, the email in `<>`). Else yours: a repository with no commit is the owner's new one, and with no email Catervas cannot tell. A folder with no git is yours.
- **Placement is recomputed over every session each scan**, writing `sessions.place` and `project_root` and upserting `projects` (`present` from the root's existence; a root never seen present is `is_git` 0, `yours`), since folders are deleted and repositories made between scans.

### `catervas discover`

- **`catervas discover [--forget]`**, unix only as `state` is, needs no project and runs anywhere. It checks the state folder, then the Claude folder, then makes the state folder and opens the index, then `read_transcripts`, `place_sessions` (home and `TMPDIR` from `io.env`), `found`. Running it is the owner's consent (the screen's consent is step 06's); its first line says "reading your Claude Code history on this computer; nothing leaves it". Rejected: a yes-or-no prompt, which a script cannot answer and which the explicit command already is.
- **What it prints**, every path through `printable` (`crates/cli/src/printable.rs:15`), a path inside the canonical home shown as `~/…` (the `--forget` line too); tokens as `950`, `12.3k`, `1.2M`, one decimal with a trailing `.0` dropped (`310k`, `2k`); "in" being input, cache read and cache write together; dates `YYYY-MM-DD` in UTC; "1 session", "2 sessions":
  ```
  reading your Claude Code history on this computer; nothing leaves it
  read 37 transcript files: 312 sessions, 4 new
  couldn't read 2 sessions from this version of Claude Code
  skipped 14 lines it couldn't read
  projects: 5
    Farik  ~/Farik  last worked 2026-10-12  58 sessions  41.2M in  310k out
      Catervas's agents  12 sessions  3.1M in  80k out
  explored: 1
    ...
  no longer on this computer: 2
    ~/old-thing  last worked 2026-09-20  3 sessions  400k in  2k out
  not in a project: 5 sessions
  ```
  A section or line with nothing in it is left out, a link refused adds "refused <n> files outside Claude Code's transcripts", and the name is the root's last segment. Projects and gone folders are newest first; a project with Catervas sessions and none of the owner's still has its row, with "0 sessions". A gone root's sessions (owner's and Catervas's together) are one row among the gone folders.
- **`--json`** prints one object, every path absolute: `{ files, sessions, new_sessions, refused, unreadable_sessions, unreadable_lines, projects: [{ root, name, is_git, ownership, last_worked_at, own: { sessions, tokens }, catervas: { sessions, tokens } }], gone: [{ folder, last_worked_at, sessions, tokens }], not_a_project: { sessions, tokens } }`, `tokens` keyed by model, each `{ input_tokens, output_tokens, cache_read_tokens, cache_write_tokens }`. `--forget --json`: `{ forgotten: true|false, path }`. Built from the same values as the lines, as `metrics` does; no schema, as no command's `--json` has one.
- **`--forget`** prints "forgot everything Catervas found: ~/.config/catervas/machine.db deleted" (the path as above), or "nothing to forget".

## File map

The tasks run in sequence, and each only adds to a shared file (`lib.rs`, `git.rs`, `migrations.rs`, `tests/git.rs`).

```
crates/core/src/claude_history.rs, crates/core/src/lib.rs                creates, tests; modifies: pub mod (Tasks 1, 2)
crates/core/src/claude_history/fixtures.rs                                creates: CLAUDE_CODE_2_1_296, line builders (Task 1)
crates/core/src/claude_history/fixtures/claude_code_2_1_296.jsonl         creates: the synthetic transcript (Task 1)
crates/store/src/migrations.rs                                            modifies: apply_list, MACHINE_MIGRATIONS, apply_machine (Task 3)
crates/store/src/migrations/machine/0001_history.sql                      creates (Task 3)
crates/store/src/machine.rs, crates/store/src/lib.rs                      creates, tests; modifies: pub mod (Tasks 3, 4, 5)
crates/store/src/claude_history.rs, crates/store/src/lib.rs               creates, tests: read_transcripts (Task 4), place_sessions (Task 5)
crates/store/src/git.rs, crates/store/tests/git.rs                        modifies, tests: main_worktree, user_email, has_commit_by (Task 5)
crates/store/tests/claude_history.rs                                      creates: placement and ownership on real repositories (Task 5)
crates/cli/src/discover.rs, crates/cli/src/state.rs, crates/cli/src/lib.rs   creates; modifies: claude_dir, mod, Discover (Task 6)
crates/cli/tests/discover.rs                                              creates (Task 6)
docs/SPEC.md, docs/design/many-teams.md                                   modifies (Task 7)
```

## Interfaces

Consumes, all on `main`: `pricing::Usage` (`crates/core/src/pricing.rs:85`); `StoreError` (`crates/store/src/error.rs:7`); `migrations::apply_through` (`migrations.rs:102`); `Git::open`, `head_summary`, `run_git` (`git.rs:128`, `:163`, `:903`); `git_in`, `git_output_in` (`git/fixtures.rs:143`, `:154`); `open_event_log` (`event_log.rs:89`); `state_dir`, `make_state_dir` (`state.rs:10`, `:24`); `Report`, `report` (`lib.rs:291`, `:1927`); `printable`; `FixedClock` (`crates/protocol/src/clock.rs:24`).

Produces:

```rust
// catervas_core::claude_history
pub const CHECKED_VERSIONS: [&str; 1] = ["2.1.296"];
pub const TITLE_LIMIT: usize = 200;
pub const TEMPORARY_FOLDERS: [&str; 3] = ["/", "/tmp", "/var/tmp"];
pub enum TranscriptLine { Turn(Turn), Title { session_id: String, title: String }, Other, Unreadable }
pub struct Turn { pub session_id: String, pub cwd: String, pub at: DateTime<Utc>, pub git_branch: Option<String>,
                  pub version: Option<String>, pub entrypoint: Option<String>, pub reply: Option<Reply> }
pub struct Reply { pub key: String, pub model: String, pub usage: Usage }
pub fn read_line(line: &str) -> TranscriptLine;
pub fn once_per_reply(seen: Usage, again: Usage) -> Usage;          // each counter's largest
pub struct Folder { pub path: String, pub real: Option<String>, pub git_root: Option<String> }  // real: None when gone
pub struct KnownRoot { pub root: String, pub is_git: bool }
pub enum Place { Project { root: String }, Gone, NotAProject }
pub fn place_folders(folders: &[Folder], known: &[KnownRoot], not_projects: &[String]) -> Vec<Place>;  // one per folder, in order; adds TEMPORARY_FOLDERS
pub fn catervas_root_of(folder: &str) -> Option<String>;
pub enum SessionPlace { Owner { root: String }, Catervas { root: String }, Gone { folder: String }, NotAProject }
impl SessionPlace { pub fn as_wire(&self) -> &'static str; }       // owner, catervas, gone, not_a_project
pub fn place_session(session_id: &str, cwd: &str, folder: &Place, catervas_logs: &BTreeMap<String, BTreeSet<String>>) -> SessionPlace;
// catervas_core::claude_history::fixtures
pub const CLAUDE_CODE_2_1_296: &str;   // include_str! of the .jsonl
pub fn a_turn_line(session_id: &str, cwd: &str, at: &str) -> String;
pub fn a_reply_line(session_id: &str, cwd: &str, at: &str, message_id: &str, model: &str, usage: Usage) -> String;
pub fn a_title_line(session_id: &str, title: &str) -> String;
// catervas_store::machine
pub const MACHINE_DB: &str = "machine.db";
pub struct MachineIndex { /* connection */ }
pub fn open_machine_index(path: &Path, now: DateTime<Utc>) -> Result<MachineIndex, StoreError>;
pub fn forget_machine_index(path: &Path) -> Result<bool, StoreError>;   // true: there was one
pub enum Ownership { Yours, Explored }
pub struct Activity { pub sessions: u64, pub tokens: BTreeMap<String, Usage> }   // by model
pub struct FoundProject { pub root: String, pub is_git: bool, pub ownership: Ownership,
                          pub last_at: DateTime<Utc>, pub own: Activity, pub catervas: Activity }
pub struct GoneFolder { pub folder: String, pub last_at: DateTime<Utc>, pub activity: Activity }
pub struct Found { pub projects: Vec<FoundProject>, pub gone: Vec<GoneFolder>, pub not_a_project: Activity,
                   pub unreadable_sessions: u64, pub unreadable_lines: u64 }   // projects: present roots, newest first
impl MachineIndex { pub fn found(&self) -> Result<Found, StoreError>; pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError>; }
// catervas_store::claude_history
pub const PROJECT_LOG: &str = ".catervas/local/catervas.db";
pub enum HistoryError { Store(StoreError), Git(GitError) }   // Display: the inner sentence
pub struct ReadSummary { pub files: u64, pub sessions: u64, pub new_sessions: u64, pub refused: u64 }
pub fn read_transcripts(index: &mut MachineIndex, claude_dir: &Path, now: DateTime<Utc>) -> Result<ReadSummary, HistoryError>;
pub fn place_sessions(index: &mut MachineIndex, not_projects: &[PathBuf], now: DateTime<Utc>) -> Result<(), HistoryError>;
// catervas_store::git::Git
pub fn main_worktree(&self) -> Result<Option<String>, GitError>;
pub fn user_email(&self) -> Result<Option<String>, GitError>;
pub fn has_commit_by(&self, email: &str) -> Result<bool, GitError>;
// catervas (cli)
pub(crate) fn claude_dir(env: &BTreeMap<String, String>) -> Option<PathBuf>;   // state.rs
pub fn discover(io: &CliIo<'_>, forget: bool) -> Result<Report, String>;         // discover.rs
```

## Tasks

A test marked *guard* holds a property the first green code already has (no text kept, no escape obeyed). It is not watched to fail; the landing review checks it by mutation (copy a text field into the index, print a path raw) and the test must then fail.

### Task 1: One transcript line in core

Files: `claude_history.rs` (the line reader and `once_per_reply`); `fixtures.rs` and the `.jsonl`. The fixture is written by hand, holds no real transcript's text, session `11111111-1111-4111-8111-111111111111`, `cwd` `/home/me/shop`, `gitBranch` `main`, `version` `2.1.296`, `entrypoint` `cli`, model `claude-opus-5-5`, every text field `NEVER-KEPT-TEXT`, in this order: L1 `user` 09:00:00; L2 `attachment` 09:00:01; L3 `assistant` 09:00:05, `message.id` `msg_01A`, `requestId` `req_01A`, usage 10/1/1000/200 (input, output, cache read, cache creation); L4 the same reply 09:00:06, output 40; L5 `assistant` 09:01:00, no `message.id`, `requestId` `req_01B`, usage 5/7/0/0; L6 `assistant` 09:01:10, model `<synthetic>`, `message.id` `msg_S`, usage 0; L7 `ai-title` "Fix the checkout total"; L8 `last-prompt`; L9 `queue-operation`; L10 `cost-state`; L11 type `a-kind-from-a-later-version`; L12 `not json`; L13 `user` with no `cwd`. All on 2026-10-12, UTC.

- `reads_each_line_of_a_checked_version`: for each of `CHECKED_VERSIONS` its fixture reads as: L1, L2 and L6 `Turn` with no reply, L1's fields as above; L3 a reply `msg_01A` with usage 10/1/1000/200; L4 `msg_01A` with output 40; L5 key `req_01B`; L7 `Title` "Fix the checkout total"; L8 to L11 `Other`; L12 and L13 `Unreadable`. RED: no module.
- `reads_no_conversation_text` (guard): no `Turn`, `Reply` or `Title` read from the fixture holds `NEVER-KEPT-TEXT` in any field (checked through `Debug`).
- `reads_a_line_tolerantly`: a turn with an unknown extra field reads; one with `timestamp` `"yesterday"` is `Unreadable`; an `assistant` with no `message.id` and no `requestId` is a `Turn` with no reply; one with `usage` lacking `cache_read_input_tokens` reads it as 0; an `ai-title` of 300 characters is cut to 200, of `"  "` is `Other`; `""` and `[]` are `Unreadable`. RED.
- `counts_a_reply_once`: `once_per_reply` of 10/1/1000/200 and 10/40/1000/200 is 10/40/1000/200 either way round, and of a value with itself is that value. RED.

- [ ] `feat(core): read a claude code transcript line`

### Task 2: Placing folders and sessions in core

Files: `claude_history.rs` (placement). Home `/home/me` in every case, passed in `not_projects`.

- `places_a_worktree_in_its_repository`: folders `/home/me/shop`, `/home/me/shop/.claude/worktrees/x` and `/home/me/shop-wt`, each with git root `/home/me/shop`, are each `Project { root: "/home/me/shop" }`. RED: no function.
- `places_a_deleted_worktree_under_its_known_root`: gone `/home/me/shop/.claude/worktrees/old` with known root `/home/me/shop` is that project; gone `/home/me/shopping` is `Gone`; with known roots `/home/me/a` and `/home/me/a/b`, gone `/home/me/a/b/c` is under `/home/me/a/b`; gone `/home/me/n/x` with known root `/home/me/n` of `is_git` false is under it. RED.
- `folds_a_folder_with_no_git_into_its_outermost_session_folder`: `/home/me/notes`, `/home/me/notes/2026` and `/home/me/notes/2026/q4`, none with git, are each under `/home/me/notes`, also when `/home/me/notes/2026` is a known root with `is_git` false; `/home/me/solo` alone is under itself. RED.
- `never_makes_home_or_a_temporary_folder_a_project`: `/home/me` and `/tmp` are `NotAProject`; `/tmp/scratch` and `/home/me/scratch` with no git are each under themselves; a folder whose git root is `/home/me` (home kept in git) is placed as if it had no git; a folder whose git root is `/` likewise; with no home passed, `/home/me` with no git is under itself. RED.
- `places_catervas_sessions_under_their_project`: `catervas_root_of("/home/me/shop/.catervas/local/worktrees/CTV-3")` is `/home/me/shop`, of `/home/me/shop` is `None`; `place_session` gives `Catervas { root: "/home/me/shop" }` for a `cwd` under `.catervas/local/` whatever its folder's place (gone included), and for session `cat-1` in `/home/me/shop` when `catervas_logs` maps `/home/me/shop` to `{cat-1}`, while `own-1` there is `Owner`; a session in a `Gone` folder is `Gone { folder: <cwd> }`. Each `as_wire` is its table value. RED.

- [ ] `feat(core): fold claude code sessions into projects`

### Task 3: The machine index

Files: `migrations.rs`; `0001_history.sql`; `machine.rs` (open, forget, `applied_migrations`); tests in `machine.rs` on a temporary folder named after the test, as `TempProject` makes one (`files/fixtures.rs:20`).

- `opens_the_machine_index_and_records_its_migration`: `open_machine_index` on a path in a folder not yet made makes both; `applied_migrations` is `[1]`; opening again applies nothing and keeps `[1]`; the four tables exist. The event log's `known_versions()` is unchanged. RED: no module.
- `forgets_the_machine_index`: with `machine.db` and a `machine.db-journal` beside it, `forget_machine_index` answers `true` and both are gone, the folder and a `state.json` in it kept; again, `false`. RED.

- [ ] `feat(store): keep a machine-level index of claude code history`

### Task 4: Reading the transcripts into the index

Files: `claude_history.rs` (`read_transcripts`, `HistoryError`); `machine.rs` (the upserts). Placement and `found` are Task 5's, so these tests read the `sessions`, `replies` and `transcript_files` tables with `rusqlite` on the file, the format 8.4 documents. Tests in `claude_history.rs` on a temporary Claude folder with `projects/-home-me-shop/`.

- `reads_a_checked_versions_transcript`: the fixture as `<session>.jsonl`: `ReadSummary` is 1 file, 1 session, 1 new, 0 refused; one session row with `cwd` `/home/me/shop`, branch `main`, version `2.1.296`, entrypoint `cli`, title "Fix the checkout total", first 09:00:00, last 09:01:10; two reply rows, `msg_01A` 10/40/1000/200 and `req_01B` 5/7/0/0, none for `msg_S`; the file's row has `turns` 6 and `unreadable_lines` 2. RED: no function.
- `keeps_no_conversation_text` (guard): after reading the fixture, the bytes of `machine.db` hold no `NEVER-KEPT-TEXT`.
- `reads_only_what_was_appended`: a second read is 0 new sessions and changes no row; a line appended with `a_reply_line` (`msg_01C`, 1/2/0/0) and half a line after it (no `\n`) adds `msg_01C` alone and `read_to` stops before the half line; once the line is finished it is read. A file truncated to its first line is read again from 0, its row then `turns` 1 and `unreadable_lines` 0, and the replies' counters stay as they were. RED.
- `counts_a_resumed_session_once`: a second file, session `00000000-0000-4000-8000-000000000000` (its file read first), starting 10:00 and repeating L3 to L5 then a new reply `msg_02A`: three reply rows in all, `msg_01A` and `req_01B` under session `1111…` (started 09:00), `msg_02A` under `0000…`. With both sessions starting 09:00, the repeated replies go to `0000…`. A subagent file `<1111…>/subagents/agent-a.jsonl` whose lines name session `1111…` adds its replies to that session and no session row. RED.
- `refuses_whatever_is_not_a_transcript`: with `.credentials.json` beside `projects/` holding `SECRET-MARKER`, a link `projects/-x/evil.jsonl` to it, a link `projects/-x/out.jsonl` to a transcript outside the Claude folder, `jobs/j/state.json`, `history.jsonl` and `projects/-x/<s>/tool-results/r.jsonl`, each holding the marker in a turn: `refused` is 2, no `transcript_files` row names any of them, and `machine.db` holds no `SECRET-MARKER`. RED.
- `counts_what_it_could_not_read`: a file of three `not json` lines alone has a row with `turns` 0 and `unreadable_lines` 3, and no session row. RED.

- [ ] `feat(store): read claude code transcripts into the machine index`

### Task 5: Each session in its project

Files: `git.rs` (three methods after `check_branch_name`); `claude_history.rs` (`place_sessions`, the log read); `machine.rs` (`found`). Tests needing git are `#[ignore = "needs the git program: cargo xtask check --integration"]`, in `tests/git.rs` and `tests/claude_history.rs`, on a temporary home with repositories made by `git_in` (`init -b main`, a local `user.email` `me@example.invalid`, one commit), sessions written with Task 1's builders.

- `finds_a_repositorys_main_worktree` (`tests/git.rs`): in a repository, in its subfolder and in a linked worktree, `main_worktree` is the repository's canonical root; in a plain folder `None`. RED.
- `knows_who_committed` (`tests/git.rs`): `user_email` is `me@example.invalid`; `has_commit_by("me@example.invalid")` and `("ME@example.invalid")` are true, `("e@example.invalid")` false (the `<>` matters), and true for `other@example.invalid` once a commit is made with `-c user.email=other@example.invalid` on another branch. RED.
- `places_every_session` (`tests/claude_history.rs`): home holding repository `shop` with a worktree `shop/.claude/worktrees/w`, a plain folder `notes` with `notes/sub`, and sessions in `shop`, the worktree, `notes/sub`, `notes`, home, a gone `shop/.claude/worktrees/old`, a gone `gone-app`, and `shop/.catervas/local/worktrees/CTV-1`: `found` gives projects `shop` (owner 3 sessions, Catervas 1) and `notes` (2), gone `gone-app` (1), and `not_a_project` 1; replies' tokens are summed by model under each. RED: no function.
- `places_the_same_whatever_the_scans_saw_first` (`tests/claude_history.rs`): scan 1 sees only a session in `notes/sub`; scan 2 adds one in `notes`; after scan 2 both are under `notes`, and `found` has no `notes/sub` row. RED.
- `says_how_many_sessions_it_could_not_read` (`tests/claude_history.rs`): Task 4's three-line file and the fixture give `found.unreadable_sessions` 1 and `unreadable_lines` 5. RED.
- `counts_catervas_sessions_from_the_projects_log` (`tests/claude_history.rs`): with `shop/.catervas/local/catervas.db` made by `open_event_log` holding one event whose envelope names session `cat-1`, the log dropped before the scan, a session `cat-1` in `shop` counts under `shop`'s Catervas activity and not its own; after the scan the log's bytes are unchanged and no `catervas.db-wal`, `-shm` or `-journal` exists beside it. RED.
- `tells_explored_from_yours` (`tests/claude_history.rs`): `shop`, committed as `me@example.invalid`, is yours; `clone`, whose only commit is by `other@example.invalid` with local `user.email` `me@example.invalid`, is `Explored`; a repository with no commit is yours; a plain folder is yours with `is_git` false. RED.
- `remembers_a_root_after_its_folder_is_gone` (`tests/claude_history.rs`): after a scan finds `shop`, its folder removed and rescanned: `shop`'s sessions, and a gone worktree session under it, are one row `shop` in `found.gone`, none in `projects`. RED.

- [ ] `feat(store): place each claude code session in its project`

### Task 6: `catervas discover`

Files: `discover.rs` (`discover`, the lines and the JSON); `state.rs` (`claude_dir`, its unit test beside `state_dir_follows_xdg_then_home_then_appdata`); `lib.rs` (`#[cfg(unix)] mod discover;`, the `#[cfg(unix)] Discover { forget: bool }` variant after `Doctor` with the doc comment "Read your Claude Code history on this computer and say what Catervas found (ADR 0059).", and its arm in the `outcome` match, `lib.rs:1200`). Tests in `tests/discover.rs` through `run_cli` with `HOME` and `XDG_CONFIG_HOME` set in `io.env` to a temporary folder, the fixture's `/home/me` replaced by that home, and `shop` made there with `git_in` with a local `user.email` and one commit by it; those that reach git are `#[ignore]`d as in Task 5.

- `claude_dir_prefers_claude_config_dir` (`state.rs`): `CLAUDE_CONFIG_DIR=/c` gives `/c`; empty, `HOME=/h` gives `/h/.claude`; neither `None`. RED.
- `discovers_a_project_from_the_history` (ignored): exit 0 and stdout exactly the Verification's lines; `machine.db` is in `<XDG_CONFIG_HOME>/catervas/`; a second run says "read 1 transcript file: 1 session, 0 new". With `--json`, `projects[0]` is `{ root: <home>/shop, name: "shop", is_git: true, ownership: "yours", last_worked_at: "2026-10-12T09:01:10Z", own: { sessions: 1, tokens: { "claude-opus-5-5": { input_tokens: 15, output_tokens: 47, cache_read_tokens: 1000, cache_write_tokens: 200 } } }, catervas: { sessions: 0, tokens: {} } }`, `root` absolute. RED: unrecognised subcommand.
- `forgets_without_reading`: with a `machine.db` made by `open_machine_index` and the fixture in the Claude folder, `discover --forget` deletes `machine.db` and says "forgot everything Catervas found: ~/.config/catervas/machine.db deleted"; a second says "nothing to forget"; with `--json`, `{ forgotten: true, path: <absolute> }`; no transcript file's bytes or mtime change. RED.
- `says_when_there_is_no_history`: with no `.claude/projects`, stdout is the first line and "no Claude Code history on this computer", exit 0, and no `machine.db`; with no `HOME`, `XDG_CONFIG_HOME` or `CLAUDE_CONFIG_DIR`, exit 1 and stderr "catervas: Catervas has no state folder; set HOME"; with `XDG_CONFIG_HOME` alone, exit 1 and stderr "catervas: Catervas can't tell where Claude Code keeps its history; set HOME". RED.
- `prints_paths_as_text` (guard, ignored): a session whose `cwd` is a gone `<home>/a\u{1b}[2Jb` is printed under "no longer on this computer" as `~/a\u001b[2Jb`.
- `writes_tokens_short`: the token formatter gives `950`, `2k` for 2000, `12.3k`, `310k`, `1.2M` for 1 215 000, `41.2M`. RED.

- [ ] `feat(cli): say what catervas found in the claude code history`

### Task 7: Spec

`docs/SPEC.md`, at the next free revision number when the task runs:
- 3: a concept "Claude Code history (ADR 0059; phase 10 step 02)": `catervas discover` reads the transcripts on this computer only, keeping metadata and titles, never text; the folding order with not-project folders; Catervas's own sessions by `.catervas/local/` and by log ids; yours or explored; `--forget`
- 8.4: `machine.db` in the state folder, apart from every project's log, its four tables, its own migrations, the offset read per file, one row per session and per reply, counters kept once per reply and the reply's session the earliest-started
- 8.6: the reader opens only `.jsonl` transcripts under `projects/` by canonical path, refuses `.credentials.json` and links out, reads none of a line's text fields, opens a project's log read-only and immutable, and sends nothing anywhere

`docs/design/many-teams.md` section 2: the temporary folders are not projects; a folder with no git folds into its outermost session folder, and only roots with git are known roots for a folder that exists; a subagent's replies count under its parent session; a reply's counters are the largest seen.

- [ ] `docs(spec): record reading the claude code history`

## Verification

Phase 9 builds concurrently in another worktree, so every cargo command uses this worktree's own build folder:

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration   # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```

By hand, on a fixture home (`H=$(mktemp -d)`; `.claude/projects/-shop/` holding the fixture with `/home/me` replaced by `$H`; `$H/shop` a repository with a local `user.email` and one commit by it):

```
HOME=$H XDG_CONFIG_HOME=$H/.config CARGO_BUILD_BUILD_DIR=$PWD/target cargo run -q -p catervas -- discover
# expected:
reading your Claude Code history on this computer; nothing leaves it
read 1 transcript file: 1 session, 1 new
skipped 2 lines it couldn't read
projects: 1
  shop  ~/shop  last worked 2026-10-12  1 session  1.2k in  47 out
HOME=$H XDG_CONFIG_HOME=$H/.config CARGO_BUILD_BUILD_DIR=$PWD/target cargo run -q -p catervas -- discover --forget
# expected: forgot everything Catervas found: ~/.config/catervas/machine.db deleted
```
