# Phase 10, step 04: claude.ai's export, and one record across sources

Status: ready
Branch: `phase/10-many-teams` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 8.4, 8.6; no functional requirement of section 7 (ADR 0059)
Depends on: steps 02, 03 and 03b of this phase (planned; their commits on this branch when this step starts). Nothing of phase 9 or of steps 05 on.
Mockups: none; this step has no screen (step 06 draws the export drop and projects by hand).
Readiness confirmed by: round 1, 2026-10-12: not ready on B1 (one shared reply linked sessions), B2 (migration numbers, 03/03b), B3 (scan order), B4 (consumed signatures); folded with N1–N11. Round 2, 2026-10-12, a fresh-session Opus reviewer: every fold correct; not ready only on B1 (two tests counted two `maybe_same` pairs where the rule gives three), folded with N1–N5 (formed projects, copies in 03/03b as a ceiling, a re-imported export ignored, `discover`'s visibility, Windows paths); no third round needed.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`catervas discover --claude-export <file>` reads the claude.ai data export the owner downloaded (the `.zip` claude.ai emails) on this computer, keeps each conversation's uuid, title, dates and message count and each claude.ai Project's uuid, name, description and file names, and prints a "claude.ai" section: conversations per month and the Projects. A later export updates the earlier one by uuid; what an export with later content lacks is marked "not in the latest export", never deleted. Across sources each thing is kept once (design 2c): a teleported copy of a session with new ids is recognised by its replies and counted once, a repository is keyed by its normalised remote, a folder by its real path, and `catervas discover --add-project <name> --folder <path> | --repo <address>` adds a project by hand that joins the found project it names. Each project says which sources it came from. No message text, no document content and nothing of `users.json` but the account's uuid is ever kept.

Out of scope: routines by hand and the job key (03, 06); GitHub as a source and folding two folders by one remote (06b, which owns "These are different"); removing a project added by hand except by `--forget` (06); every screen, serve, the daemon (05, 06); analytics over conversations (08).

## Decisions

### Reading the export

- **Only the `.zip`.** Anything else: `NotAZip`. Rejected: also a folder or a lone `conversations.json`, a second reading path for one source.
- **The files, all inferred** (Anthropic documents none; the founder's own export becomes the second fixture): `conversations.json` (array: `uuid`, `name`, `created_at`, `updated_at`, `chat_messages`), `projects.json` (array: `uuid`, `name`, `description`, `created_at`, `updated_at`, `docs[].filename`), `users.json` (array: the first element's `uuid` only). Other files and fields are ignored. Rejected: reading `users.json` for a name or email, which nothing needs.
- **Entries are matched by `enclosed_name()` only**, at the root, else under exactly one top-level folder (a re-zipped folder). A directory, a symlink entry, or a name `enclosed_name()` rejects (`../`, absolute) never matches. Rejected: searching every depth, which lets a stray copy deep inside win.
- **Never extracted.** Each matched entry is streamed into `serde_json::from_reader` through a counting reader; nothing is written to disk but `machine.db`. Rejected: the `unzip` program (a process, and it extracts); one buffer per entry (an export can be hundreds of megabytes).
- **Limits:** `ExportLimits::DEFAULT` is 2 GiB for the zip and 1 GiB unpacked per matched entry, checked on the declared size and on the bytes read (a header can lie); injected so a test uses kilobytes. Rejected: a compression-ratio rule, which the byte count covers.
- **Typed, lenient metadata.** Core declares deserialise-only structs naming just those fields, each lenient (missing, `null` or another type reads as absent); `chat_messages` is counted with `IgnoredAny`; `docs` yields file names, `content` never declared, so no text is materialised even in memory. Rejected: per-element recovery through `serde_json::Value`, which loads every message's text to drop it.
- **Refusals**, each `ExportError` with its sentence after "catervas: ", nothing written (one `IMMEDIATE` transaction per import):
  - `NotAZip`: "this isn't claude.ai's export: it isn't a zip file; give Catervas the .zip claude.ai emailed you"
  - `NotAnExport(reason)`: "this isn't claude.ai's export: " + "it has neither conversations.json nor projects.json" or "it has more than one conversations.json" (or `projects.json`)
  - `NotAList { entry }`: "this isn't claude.ai's export as Catervas knows it: <entry> isn't a list"
  - `Unreadable { entry }` (invalid or truncated JSON, an element that is not an object): "this isn't claude.ai's export as Catervas knows it: <entry> couldn't be read"
  - `Damaged { entry }` (checksum mismatch): "the export is damaged: <entry> doesn't match its checksum; download it again"
  - `Unsupported { entry }` (a compression method other than stored or deflate): "this export uses a kind of zip Catervas can't read (<entry>)"
  - `Encrypted { entry }`: "<entry> in this export is password-protected; give Catervas the export as claude.ai sent it"
  - `TooLarge { limit_bytes }`: "this export is larger than Catervas reads (over <limit>)", the limit in the largest of GiB, MiB, KiB or bytes that gives a whole number
  - `NothingDated`: "this export has no conversations or Projects Catervas can read"
  - `Io`, `Store`: the inner sentence.
- **Kept:** uuid trimmed, 1 to 64 characters, else the item is skipped and counted; title and name trimmed, cut to step 02's `TITLE_LIMIT`, empty as none; description cut to `DESCRIPTION_LIMIT = 2000` (06b reads it for a remote, design 4); doc names cut to `TITLE_LIMIT`; dates RFC 3339 into UTC, unreadable as none. Rejected: dropping descriptions, which design 4 names as proof.
- **One account per export:** `users.json`'s first `uuid`, else `""`; exports are compared within one account. Rejected: one bucket for all, where a second account's export marks the first's items missing.
- **An export's time is its content's:** the latest `updated_at` (else `created_at`) of its kept items; none kept or none dated: `NothingDated`. Rejected: zip entry times (DOS, no zone); the import time (an older export imported later would hide newer items).
- **Re-import by uuid:** fields are replaced when the incoming `updated_at` is at least the stored one or the stored one is none; `seen_in_export_at` keeps the latest export time that held the item. An item is not in the latest export when its `seen_in_export_at` is before its account's latest export time, computed on read, so import order never matters. A later export whose content time equals the latest (it only lost items) is not later and marks nothing. Rows go only by `--forget`. Rejected: a stored "missing" flag, which a reversed import order leaves wrong.
- **Per month:** every known conversation, those not in the latest export included, by the month of `created_at` (UTC), over the twelve calendar months ending with the month of the newest `created_at` across all imports, newest first, a month with none printing 0; older ones summed as "earlier", undated ones as "undated", each left out at 0. Rejected: `updated_at`, which moves a reopened chat between months. They are activity, with no tokens. `latest_export_at` is per account; the section prints the latest across accounts.
- **Dependency:** `zip = { version = "=8.6.0", default-features = false, features = ["deflate"] }` in the workspace, direct in `catervas-store`, commented "Already locked through calamine and rust_xlsxwriter". MIT, maintained, already in `Cargo.lock` with these features, so no crate is added; within the founder's pre-approval. Rejected: a hand-written zip reader.

### One record across sources

- **Keys** (design 2c): session id and reply key as step 02; claude.ai items by uuid; repository by `normalise_remote`; folder by canonical path; jobs: step 03's, untouched. Rejected: keying claude.ai items by title, which renames break.
- **`normalise_remote`** (core, `url` for URL forms, by hand for scp form `[user@]host:path` where no `/` precedes the `:`): host then path segments, lower case, a final `.git` (any case) and trailing `/` dropped; user, password and port dropped, so a token in a remote never reaches the index; two or more segments, all kept; `file://`, local paths (a Windows drive path such as `C:/repo` included), anything else: none. Rejected: keeping the URL as written, which splits SSH and HTTPS and stores tokens.
- **A project's remote** is `origin`'s, else the first remote by name, from `git remote get-url` (`Git::remote_url`), written to `projects.remote` by `place_sessions` each scan. Rejected: `remote.origin.url` raw, which ignores `insteadOf`.
- **Sources are derived, not stored:** a project is `local` when the scan found it, `teleported` when one of its kept sessions has a `copy_of` copy (never for `maybe_same`), `by_hand` when a hand project matches it; a claude.ai item is `export`. When each saw it: its rows' times. Rejected: a stored set (step 02's note), a second copy of facts that a stopped scan leaves wrong.
- **Fields:** a found project's name is the matching by-hand name, else the root's last segment; the rest is the scan's. A project step 03b formed from connector work has no root: it is never matched by hand, keeps 03b's name, and its sources are `local` (or `teleported`). Steps 03 and 03b do not consult `session_links`, so a copied session's tool calls and session crons may count twice there; accepted as a ceiling, since a teleported session lands in a repository folder. Rejected: the folder name over the owner's own word. A claude.ai Project and a found project are never merged (design 2c).

### Copies of a session

- **A reply's fingerprint** (`reply_fingerprint`) is its `at` to the second, model and four token counters, from its `replies` row. **No text is hashed.** Rejected: the design's text hash, which needs the reader to open `message.content`, the field step 02's guard keeps unread, and keeps a guessable digest of short replies. If teleporting rewrites times or usage, nothing merges and both count: the safe failure.
- **A pair is linked only on strong evidence:** two sessions share at least `MIN_SHARED_REPLIES = 3` fingerprints under different reply keys, and the shared ones cover every reply of one of the two, in time order, up to its last shared one. A shared key is a resume or fork, counted once by step 02, and not evidence; replies within one session are never compared. Rejected: one shared fingerprint, which links two agents started the same second with one identical first reply.
- **Among linked pairs**, a session in exactly one, whose partner is in exactly that one, merges: the later `first_at` is `copy_of` the other; on a tie, the session indexed first (lowest `sessions` row id) is kept. Every other linked pair is `maybe_same`, both kept. The kept session's project shows the count. Rejected: smallest id on a tie, which depends on random ids.
- **Replies counted once only within linked pairs:** among a fingerprint's replies in linked sessions, the kept one is the copy's partner's, else the earliest `first_at`, then lowest row id, then smallest key; the others are `reply_copies`. A `copy_of` session is not counted. Both tables are rebuilt in one transaction each scan, so a third copy later turns a merge into `maybe_same`. Rejected: incremental links, which never revisit a merge.

### Projects by hand

- **`catervas discover --add-project <name> [--folder <path>] [--repo <address>]`** now, so the by-hand merge is checked end to end; step 06's screen calls the same store function. Routines by hand wait for 06, a routine being a job keyed by step 03. Rejected: deferring all to 06, leaving "by hand" untested here.
- **Refusals:** core `HandProjectRefusal`: `EmptyName` "a project needs a name"; `NoPlace` "say where the project is: --folder <path> or --repo <address>"; `NotARepositoryAddress(a)` "<a> isn't a repository address Catervas can read"; `NotAProjectFolder(p)` "<p> can't be a project: it is your home, the root or a temporary folder"; store `HistoryError::NoSuchFolder(p)` "there is no folder at <p>". Rejected: accepting home, which step 02 never makes a project.
- **Its root** is step 02's `place_folders` of the canonical folder with its `main_worktree`, no known roots, and step 02's not-project folders (home, `TMPDIR`, `TEMPORARY_FOLDERS`); `NotAProject` refuses. **Key:** the root, else the remote; the same key again renames. **Match**, on every `found`: the found project with the longest root containing its root, else the first found project, newest first, with its remote. `found.by_hand` lists only unmatched ones. Rejected: matching once at add time, which a later scan could not fix.

### Where it lives, scan order and migrations

- Core `claude_export` (types, records); additions to core `claude_history` (keys, copies, by hand, `Source`); store `claude_export`; additions to store `machine`, `claude_history`, `git`; cli `discover`. Nothing in `serve.rs`, `crates/runtime/src/daemon/**` or `apps/web/**`; shared files get additions only.
- **`discover`'s scan:** `read_transcripts → place_sessions →` steps 03's and 03b's passes, unchanged `→ find_copies → found` (and 03/03b's `found_*`). `--claude-export` imports before the scan. With no Claude Code history, `machine.db` is opened only when it exists or an export is given, keeping step 02's "writes nothing". `--forget`, `--claude-export` and `--add-project` conflict; `--folder` and `--repo` require `--add-project`; `--add-project` prints only its line. Rejected: copies before placement, whose project would be stale.
- **Three migrations, each the next free four-digit number when its task runs** (after steps 03's and 03b's; `MACHINE_MIGRATIONS` grows by one per task): `<n>_claude_export.sql`, `<n>_copies.sql`, `<n>_by_hand.sql` under `migrations/machine/`, every table `STRICT`, times RFC 3339 UTC:
  - claude_export: `claude_exports (account TEXT NOT NULL, content_at TEXT NOT NULL, imported_at TEXT NOT NULL, PRIMARY KEY (account, content_at))`, an export already imported is ignored (`INSERT OR IGNORE`, `imported_at` unchanged); `claude_conversations (uuid TEXT PRIMARY KEY, account TEXT NOT NULL, title TEXT, created_at TEXT, updated_at TEXT, messages INTEGER NOT NULL, seen_in_export_at TEXT NOT NULL, first_imported_at TEXT NOT NULL)`; `claude_projects (uuid TEXT PRIMARY KEY, account TEXT NOT NULL, name TEXT, description TEXT, doc_names TEXT NOT NULL, created_at TEXT, updated_at TEXT, seen_in_export_at TEXT NOT NULL, first_imported_at TEXT NOT NULL)`, `doc_names` a JSON array.
  - copies: `session_links (session_id TEXT NOT NULL, other_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('copy_of','maybe_same')), PRIMARY KEY (session_id, other_id))` (`copy_of`: copy, kept; `maybe_same`: smaller id first); `reply_copies (reply_key TEXT PRIMARY KEY, same_as TEXT NOT NULL)`.
  - by_hand: `ALTER TABLE projects ADD COLUMN remote TEXT`; `hand_projects (key TEXT PRIMARY KEY, name TEXT NOT NULL, root TEXT, remote TEXT, added_at TEXT NOT NULL, CHECK (root IS NOT NULL OR remote IS NOT NULL))`.
  Rejected: one migration for the step, which would create tables before the task that tests them.

### What `discover` prints

- Names through `printable`, paths and counts as step 02 ("1 conversation", "1 Project", "1 file", "no files"); a line or section with nothing is left out. Import lines come first; then steps 02, 03 and 03b's, a matched project under its by-hand name; then "3 sessions may be copies of others; their usage is counted once"; "added by hand: 1" with `  Billing  github.com/me/billing`; and the claude.ai section as in Verification, a Project not in the latest export ending `  not in the latest export`, and a last line `  not in the latest export: 1 conversation`. `--add-project`: "added Shop by hand: the same project as ~/shop" or "added Billing by hand".
- **`--json`** adds: `projects[].repository`, `projects[].sources` (sorted), `projects[].name` the shown name; `maybe_same_sessions`; `by_hand: [{ name, root, repository }]`; `claude_ai: { latest_export_at, conversations, undated, by_month: [{ month, conversations }], projects: [{ uuid, name, description, doc_names, updated_at, in_latest_export }], not_in_latest_export: { conversations, projects } }`; with `--claude-export`, `export: { conversations, new_conversations, projects, new_projects, skipped }`. `--add-project --json`: `{ name, root, repository, same_as }`.

## File map

```
Cargo.toml, crates/store/Cargo.toml                                      modifies: zip (Task 3)
crates/core/src/claude_export.rs, crates/core/src/lib.rs                 creates, tests; modifies: pub mod (Task 1)
crates/core/src/claude_export/fixtures.rs, fixtures/2026_10/{conversations,projects,users}.json   creates (Task 1)
crates/core/src/claude_history.rs                                        modifies, tests: keys, copies, by hand, Source (Task 2)
crates/store/src/migrations.rs, crates/store/src/migrations/machine/<n>_*.sql   modifies; creates one (Tasks 3, 5, 6)
crates/store/src/machine.rs                                              modifies: upserts, found's additions, step 02's migration test (Tasks 3 to 6)
crates/store/src/claude_export.rs, claude_export/fixtures.rs, lib.rs     creates, tests; modifies: pub mod (Tasks 3, 4)
crates/store/src/claude_history.rs                                       modifies, tests: find_copies (5), remote, add_project_by_hand (6)
crates/store/src/git.rs, crates/store/tests/git.rs                       modifies, tests: remote_url (Task 6)
crates/store/tests/claude_history.rs                                     modifies: copies (5); remote, by hand, linked folder (6)
crates/cli/src/discover.rs, crates/cli/src/lib.rs, crates/cli/tests/discover.rs   modifies (Task 7)
docs/SPEC.md, docs/design/many-teams.md                                  modifies (Task 8)
```

## Interfaces

Consumes, from step 02 (`step-02-reading-the-claude-code-history.md`, Decisions and Tasks 3 and 6; if its committed code differs, the code wins: check the working tree before Task 1): `pub(crate) fn apply_machine(connection: &mut Connection, now: DateTime<Utc>) -> Result<(), StoreError>`; `MACHINE_MIGRATIONS: [Migration; N]` (N = 1 in step 02, 3 after 03b; this step makes it N + 3); `TITLE_LIMIT`, `TEMPORARY_FOLDERS`, `Folder`, `Place`, `place_folders`, the fixtures `CLAUDE_CODE_2_1_296` and `a_reply_line(session_id, cwd, at, message_id, model, usage) -> String`; `MachineIndex`, `open_machine_index`, `Found`, `FoundProject`, `found`; `read_transcripts`, `place_sessions(index, not_projects: &[PathBuf], now)`, `HistoryError`; `Git::main_worktree`; cli `#[cfg(unix)] Discover { forget: bool }` and `pub fn discover(io: &CliIo<'_>, forget: bool) -> Result<Report, String>`, both replaced below; `claude_dir`. From steps 03 and 03b: their passes in `discover` and their migrations, by position only. On `main`: `Git::open`, `run_git`, `git_in`, `printable`, `Report`, `StoreError`, `pricing::Usage`.

Produces:

```rust
// catervas_core::claude_export
pub const EXPORT_SHAPES: [&str; 1] = ["2026-10"];
pub const DESCRIPTION_LIMIT: usize = 2000;
#[derive(Deserialize)] pub struct ExportConversation { /* private, lenient */ }   // likewise ExportProject, ExportUser
pub struct ConversationRecord { pub uuid: String, pub title: Option<String>, pub created_at: Option<DateTime<Utc>>, pub updated_at: Option<DateTime<Utc>>, pub messages: u64 }
pub struct ClaudeProjectRecord { pub uuid: String, pub name: Option<String>, pub description: Option<String>,
                                 pub doc_names: Vec<String>, pub created_at: Option<DateTime<Utc>>, pub updated_at: Option<DateTime<Utc>> }
pub fn conversation_record(raw: ExportConversation) -> Option<ConversationRecord>;   // None: no usable uuid
pub fn project_record(raw: ExportProject) -> Option<ClaudeProjectRecord>;
pub fn account_of(users: &[ExportUser]) -> String;
pub fn export_content_at(conversations: &[ConversationRecord], projects: &[ClaudeProjectRecord]) -> Option<DateTime<Utc>>;
// catervas_core::claude_export::fixtures
pub const EXPORT_2026_10: [(&str, &str); 3];   // (entry name, include_str!)
pub fn a_conversation_json(uuid: &str, name: &str, created_at: &str, updated_at: &str, messages: usize) -> String;
pub fn a_project_json(uuid: &str, name: &str, updated_at: &str, doc_names: &[&str]) -> String;
pub fn a_users_json(uuid: &str) -> String;
// catervas_core::claude_history (additions)
pub fn normalise_remote(address: &str) -> Option<String>;
pub enum Source { Local, Teleported, Export, ByHand }
impl Source { pub fn as_wire(&self) -> &'static str; }
pub enum HandProjectRefusal { EmptyName, NoPlace, NotARepositoryAddress(String), NotAProjectFolder(String) }
pub fn check_hand_project(name: &str, has_folder: bool, repository: Option<&str>) -> Result<(String, Option<String>), HandProjectRefusal>;
pub struct ProjectKey<'a> { pub root: &'a str, pub remote: Option<&'a str> }
pub fn match_hand_project(root: Option<&str>, remote: Option<&str>, projects: &[ProjectKey<'_>]) -> Option<usize>;
pub const MIN_SHARED_REPLIES: usize = 3;
pub fn reply_fingerprint(at: DateTime<Utc>, model: &str, usage: Usage) -> String;
pub struct SessionReplies { pub session_id: String, pub first_at: DateTime<Utc>, pub indexed: i64,
                            pub replies: Vec<(String, String)> }   // (reply key, fingerprint) in time order; indexed: row id
pub enum CopyLink { CopyOf { copy: String, of: String }, MaybeSame { first: String, second: String } }
pub struct Copies { pub links: Vec<CopyLink>, pub reply_copies: Vec<(String, String)> }   // (copied key, kept key)
pub fn copies_among(sessions: &[SessionReplies]) -> Copies;
// catervas_store::claude_export
pub struct ExportLimits { pub file_bytes: u64, pub entry_bytes: u64 }
impl ExportLimits { pub const DEFAULT: ExportLimits; }
pub struct ExportSummary { pub conversations: u64, pub new_conversations: u64, pub projects: u64, pub new_projects: u64, pub skipped: u64, pub content_at: DateTime<Utc> }
pub enum ExportError { NotAZip, NotAnExport(&'static str), NotAList { entry: &'static str }, Unreadable { entry: &'static str },
                       Damaged { entry: &'static str }, Unsupported { entry: &'static str }, Encrypted { entry: &'static str },
                       TooLarge { limit_bytes: u64 }, NothingDated, Io(std::io::Error), Store(StoreError) }
pub fn import_claude_export(index: &mut MachineIndex, file: &Path, limits: ExportLimits, now: DateTime<Utc>) -> Result<ExportSummary, ExportError>;
// catervas_store::claude_export::fixtures
pub enum ZipPatch { DeclaredSize(u32), Encrypted, Method(u16), BadChecksum }   // patched in both headers' bytes
pub fn an_export_zip(entries: &[(&str, &[u8])]) -> Vec<u8>;
pub fn the_2026_10_export() -> Vec<u8>;
pub fn a_patched_zip(name: &str, contents: &[u8], patch: ZipPatch) -> Vec<u8>;
pub fn a_zip_with_a_symlink(name: &str, target: &str) -> Vec<u8>;
// catervas_store::machine (additions)
pub struct MonthCount { pub month: String, pub conversations: u64 }
pub struct ClaudeAiProject { pub uuid: String, pub name: Option<String>, pub description: Option<String>,
                             pub doc_names: Vec<String>, pub updated_at: Option<DateTime<Utc>>, pub in_latest_export: bool }
pub struct ClaudeAi { pub latest_export_at: Option<DateTime<Utc>>, pub conversations: u64, pub undated: u64,
                      pub by_month: Vec<MonthCount>, pub earlier: u64, pub projects: Vec<ClaudeAiProject>,
                      pub conversations_not_in_latest: u64, pub projects_not_in_latest: u64 }
pub struct HandProject { pub name: String, pub root: Option<String>, pub remote: Option<String> }
// FoundProject gains: remote: Option<String>, name_by_hand: Option<String>, sources: BTreeSet<Source>
// Found gains: maybe_same_sessions: u64, by_hand: Vec<HandProject>, claude_ai: ClaudeAi
// catervas_store::claude_history (additions); HistoryError gains HandProject(HandProjectRefusal), NoSuchFolder(PathBuf)
pub fn find_copies(index: &mut MachineIndex) -> Result<(), HistoryError>;
pub struct AddedProject { pub project: HandProject, pub same_as: Option<String> }
pub fn add_project_by_hand(index: &mut MachineIndex, name: &str, folder: Option<&Path>, repository: Option<&str>,
                           not_projects: &[PathBuf], now: DateTime<Utc>) -> Result<AddedProject, HistoryError>;
// catervas_store::git::Git
pub fn remote_url(&self) -> Result<Option<String>, GitError>;
// catervas (cli): Discover gains claude_export: Option<PathBuf>, add_project: Option<String>, folder: Option<PathBuf>, repo: Option<String>
pub struct DiscoverArgs<'a> { pub forget: bool, pub claude_export: Option<&'a Path>, pub add_project: Option<&'a str>, pub folder: Option<&'a Path>, pub repository: Option<&'a str> }
pub(crate) fn discover(io: &CliIo<'_>, args: &DiscoverArgs<'_>) -> Result<Report, String>; // visibility as step 02's code
```

## Tasks

A test marked *guard* holds a property the first green code already has; it is not watched to fail, and the landing review checks it by mutation.

### Task 1: The export's records in core

Files: `claude_export.rs`, `fixtures.rs`, the three `.json`, written by hand, every text field `NEVER-KEPT-TEXT`, user `ffffffff-ffff-4fff-8fff-ffffffffffff` with email `never-kept@example.invalid`. Conversations: C1 `aaaaaaaa-…` "Plan the shop launch", created `2026-09-03T10:00:00.000000Z`, updated `2026-10-02T08:00:00.000000Z`, three messages with `text`, `content` and `attachments[].extracted_content`; C2 `bbbbbbbb-…` name `""`, created and updated `2026-10-05T12:00:00Z`, no messages, an unknown `a_field_from_later` object; C3 no `uuid`; C4 `cccccccc-…` name `42`, created `"not a date"`, one message. Projects: P1 `dddddddd-…` "Shop launch", description "Launch github.com/me/shop", a `prompt_template`, updated `2026-10-01T09:00:00Z`, docs `brief.md` and `prices.csv` with `content`; P2 `eeeeeeee-…` "Taxes", updated `2026-09-12T09:00:00Z`, no `docs`.

- `reads_each_record_of_a_known_export_shape`: for each of `EXPORT_SHAPES`: C1 with title, both dates, 3 messages; C2 title none, 0; C3 none; C4 title none, `created_at` none, 1; P1 with name, description, `doc_names` `["brief.md","prices.csv"]`; P2 no names; `account_of` the user's uuid; `export_content_at` `2026-10-05T12:00:00Z`. RED: no module.
- `keeps_no_export_text` (guard): no record's `Debug` holds `NEVER-KEPT-TEXT` or `never-kept@example.invalid`.
- `reads_an_export_leniently`: `chat_messages` `"x"` is 0 messages; `docs` `{}` no names; a uuid of 65 characters or `"  "` is none; a 300-character name is cut to `TITLE_LIMIT`, a 3000-character description to `DESCRIPTION_LIMIT`; `account_of(&[])` is `""`; `export_content_at` of undated records is none and uses `created_at` where `updated_at` is none. RED.

- [x] `feat(core): read claude.ai's export for metadata`

### Task 2: Keys, copies and projects by hand in core

Files: `claude_history.rs`.

- `normalises_ssh_and_https_remotes_alike`: `https://github.com/Me/Shop.git`, `https://github.com/me/shop/`, `http://github.com/me/shop`, `git@github.com:me/shop.git`, `github.com:me/shop.git`, `ssh://git@github.com/me/shop.GIT`, `ssh://git@github.com:22/me/shop`, `git://github.com/me/shop.git`, `https://x-access-token:SECRET@github.com/me/shop.git` each give `github.com/me/shop`; `https://gitlab.com/Group/Sub/Proj.git` gives `gitlab.com/group/sub/proj`; `/home/me/shop`, `file:///home/me/shop`, `../shop`, `https://github.com/me`, `git@github.com:`, `""` give none. RED: no function.
- `checks_a_project_by_hand`: `("  Shop ", true, None)` is `("Shop", None)`; `("Shop", false, Some("git@github.com:me/shop.git"))` is `("Shop", Some("github.com/me/shop"))`; `(" ", true, None)` `EmptyName`; `("Shop", false, None)` `NoPlace`; `("Shop", false, Some("/tmp/x"))` `NotARepositoryAddress("/tmp/x")`. RED.
- `matches_a_project_by_hand`: projects `/h/shop` (remote `github.com/me/shop`), `/h/shop/apps`, `/h/notes`: root `/h/shop/apps/web` matches `/h/shop/apps`; `/h/shopping` none; no root with that remote matches `/h/shop`, as does root `/h/other` with it. RED.
- `links_copies_on_strong_evidence_only`: replies R1–R4 at 09:00:05, 09:01, 09:02, 09:03; `a` holds them, `b` (later `first_at`) the same fingerprints under new keys plus R5: `CopyOf { copy: b, of: a }` and `reply_copies` maps each of `b`'s four to `a`'s; on equal `first_at`, the lower `indexed` is kept. `b` sharing only R1–R2: nothing. `b` sharing R1, R3, R4 while each holds an unshared reply between them: nothing. `b` and `c` each copying `a`: three `MaybeSame` (a–b, a–c, b–c), no `CopyOf`, and `reply_copies` maps each of `b`'s and `c`'s four replies to `a`'s (8 rows). Two separate copy pairs: two `CopyOf`. `reply_fingerprint` ignores sub-second time. Each `Source::as_wire` is its wire value. RED.

- [x] `feat(core): key repositories and match records across sources`

### Task 3: Importing the export

Files: `Cargo.toml`, `crates/store/Cargo.toml`; `migrations.rs`, `<n>_claude_export.sql`; `claude_export.rs`, its `fixtures.rs`; `machine.rs` (upserts; step 02's `opens_the_machine_index_and_records_its_migration` updated so the latest applied version is this task's, as Tasks 5 and 6 update it again). Tests in `claude_export.rs` on a temporary folder named after the test; rows read with `rusqlite`.

- `imports_a_known_export_shape`: `the_2026_10_export()` gives 3 conversations, 3 new, 2 Projects, 2 new, 1 skipped, content `2026-10-05T12:00:00Z`; rows hold Task 1's values, `account` the user's uuid; one `claude_exports` row; the latest applied migration is this task's. RED: no function.
- `keeps_no_export_text` (guard): `machine.db`'s bytes hold no `NEVER-KEPT-TEXT` and no `never-kept@example.invalid`.
- `finds_the_export_inside_one_folder`: the entries under `claude-export/` import the same; under both `a/` and `b/`, `NotAnExport("it has more than one conversations.json")`. RED.
- `refuses_what_is_not_a_claude_export`: a text file `NotAZip`; only `notes.txt` `NotAnExport`; `conversations.json` `{}`, and `projects.json` `{}`, `NotAList`; `users.json` `{}` `NotAList`; `conversations.json` cut mid-array `Unreadable`; only C3 `NothingDated`; `a_patched_zip` with `BadChecksum` `Damaged`, `Method(99)` `Unsupported`, `Encrypted` `Encrypted`. Each `Display` is its sentence above; after each, the claude.ai tables are empty. RED.
- `reads_a_malicious_zip_safely`: `NotAnExport` when the only `conversations.json` is `../conversations.json`, `/conversations.json`, or `a_zip_with_a_symlink("conversations.json", "/etc/passwd")`; with 4 KiB per entry, `TooLarge { limit_bytes: 4096 }` reading "over 4 KiB" for an honest 8 KiB entry and for `DeclaredSize(100)` over 8 KiB; with a 1 KiB file limit, `TooLarge` for a 2 KiB zip. After each, nothing in the claude.ai tables, and the folder holds only the zips and `machine.db`. RED.

- [x] `feat(store): import claude.ai's export into the machine index`

### Task 4: Re-importing, and what `found` says about claude.ai

Files: `machine.rs` (`ClaudeAi`); `claude_export.rs` tests. Exports from Task 1's builders: A (account `u1`): C1 "Old title" updated 2026-10-02, C2 updated 2026-10-05, P1 docs `[brief.md]`; B (`u1`): C1 "New title" updated 2026-10-15, C5 updated 2026-10-20, P1 docs `[brief.md, prices.csv]` updated 2026-10-16.

- `updates_an_earlier_export_by_uuid`: A then B: C1 "New title", C2 kept and not in the latest, C5 new; `claude_ai` 3 conversations, 1 not in the latest, latest `2026-10-20`, P1 both names; B again: no row changes, 0 new. RED.
- `reads_overlapping_exports_the_same_in_either_order`: B then A on a fresh index: same rows, same `ClaudeAi`. RED.
- `marks_nothing_for_an_export_that_only_lost_items`: A, then A without C1 (same content time): nothing is not in the latest. RED.
- `keeps_each_accounts_exports_apart`: after A, an export of `u2` holding only C9 marks nothing of A's. RED.
- `counts_conversations_by_month`: conversations created in 2026-10, 2026-07 (two, one not in the latest), 2025-11, 2025-10, and one undated: `by_month` is twelve entries 2026-10 to 2025-11 with 1, 0, 0, 2, 0 … 0, 1; `earlier` 1; `undated` 1. RED.

- [x] `feat(store): update claude.ai's export by uuid on re-import`

### Task 5: A copied session counted once

Files: `migrations.rs`, `<n>_copies.sql`; `claude_history.rs` (`find_copies`: reads every reply once per scan, builds `SessionReplies`, writes `copies_among`'s answer); `machine.rs` (`found` leaves out `copy_of` sessions and `reply_copies`, adds `maybe_same_sessions` and `Teleported`; the migration test as Task 3). Tests in `tests/claude_history.rs`, `#[ignore]`d as step 02's (`found` needs `place_sessions`, so git), session folders under a temporary home with no repository; sessions written with `a_reply_line`, `a` holding four replies, a copy the same lines under a new session id and new message ids.

- `counts_a_teleported_copy_once`: `session_links` holds the copy `copy_of` `a`; the project counts 1 session and `a`'s tokens once; sources `local` and `teleported`. RED: no function.
- `does_not_link_sessions_sharing_one_reply_by_chance`: two sessions started the same second whose first reply is identical (time, model, usage) and the rest differ: no `session_links` or `reply_copies` row; 2 sessions and both replies counted. RED.
- `keeps_a_resumed_session_apart_from_a_copy`: step 02's resumed session (same message ids): no link rows; 2 sessions. RED.
- `marks_ambiguous_copies_maybe_the_same`: two copies of `a`: three `maybe_same` rows (a–b, a–c, b–c), 3 sessions, `maybe_same_sessions` 3, tokens once, sources without `teleported`. One copy, then a second in a later scan: the `copy_of` becomes `maybe_same`. RED.

- [x] `feat(store): count a copied claude code session once`

### Task 6: Remotes and projects by hand

Files: `git.rs` (`remote_url`, after step 02's methods); `migrations.rs`, `<n>_by_hand.sql`; `claude_history.rs` (`place_sessions` writes `remote`; `add_project_by_hand`); `machine.rs` (names, sources, `by_hand`; the migration test as Task 3). Git tests `#[ignore = "needs the git program: cargo xtask check --integration"]` on a temporary home with `shop` made by `git_in`.

- `reads_a_repositorys_remote` (`tests/git.rs`): with `origin`, its URL; only `upstream`, that one's; none, none. RED.
- `keys_a_project_by_its_remote` (`tests/claude_history.rs`): `shop` with `origin` `git@github.com:Me/Shop.git` has remote `github.com/me/shop`; a folder with no git none. RED.
- `keeps_no_token_from_a_remote` (guard, `tests/claude_history.rs`): `origin` `https://x-access-token:SECRET-REMOTE@github.com/me/shop.git`: `machine.db`'s bytes hold no `SECRET-REMOTE`.
- `adds_a_project_by_hand_to_its_found_project` (`tests/claude_history.rs`): `Shop` with `https://github.com/me/shop` is `same_as` `shop`'s root; `found` shows `name_by_hand` "Shop", sources `local` and `by_hand`, and no `by_hand` entry; the same with folder `<home>/link-to-shop` (a link to `shop`) and with `shop/sub`. RED.
- `keeps_an_unmatched_project_by_hand` (`tests/claude_history.rs`): `Billing` with `git@github.com:me/billing.git` is in `found.by_hand`; added again as "Billing 2", one row renamed. RED.
- `places_a_session_in_a_linked_folder_with_its_project` (guard, `tests/claude_history.rs`): a session in `link-to-shop` counts under `shop`.
- `refuses_a_project_by_hand_with_nowhere` (`tests/claude_history.rs`): no folder or repository `HandProject(NoPlace)`; a missing folder `NoSuchFolder`; folders home, `/` and `/tmp` `HandProject(NotAProjectFolder)`; no row written. RED.

- [x] `feat(store): key projects by remote and add projects by hand`

### Task 7: `catervas discover` with the export, copies and projects by hand

Files: `discover.rs` (`DiscoverArgs`, the lines and JSON); `lib.rs` (`Discover`'s fields and conflicts); `tests/discover.rs` as step 02's, with no `PATH` in `io.env` so steps 03 and 03b print nothing; zips from `the_2026_10_export()`.

- `reads_a_claude_export` (no Claude Code history): stdout exactly the Verification's lines, exit 0; again, "(0 new)" twice; `--json`: `claude_ai.by_month` twelve entries, the first two `{ month: "2026-10", conversations: 1 }` and `{ month: "2026-09", conversations: 1 }`, the rest 0; `undated` 1; `export.skipped` 1. A plain `discover` then prints the claude.ai section. RED: unexpected argument.
- `refuses_what_is_not_a_claude_export`: a text file: exit 1, stderr "catervas: " and `NotAZip`'s sentence. RED.
- `discovers_copies_and_projects_by_hand` (ignored): with `shop`'s `origin` `git@github.com:me/shop.git` and history in it, a plain `discover` runs first; then `--add-project Shop --repo https://github.com/Me/Shop` prints "added Shop by hand: the same project as ~/shop", and `discover` shows `shop`'s row named "Shop", JSON `sources` `["by_hand","local"]`, `repository` `github.com/me/shop`; `--add-project Billing --repo git@github.com:me/billing.git` prints "added Billing by hand" and `discover` an "added by hand: 1" section; `--add-project X` alone exits 1 with "catervas: " and `NoPlace`'s sentence. With two copies of one session in the history, stdout holds "3 sessions may be copies of others; their usage is counted once". RED.
- `prints_claude_ai_names_as_text` (guard): a Project named `a\u{1b}[2Jb` prints as `a\u001b[2Jb`.

- [ ] `feat(cli): read claude.ai's export and projects by hand in discover`

### Task 8: Spec

`docs/SPEC.md`, at the next free revision number when the task runs:
- 3: the Claude Code history concept gains claude.ai's export (metadata only, re-import by uuid, "not in the latest export" only for an export with later content, per account), the keys and derived sources, copies linked only on three shared replies covering one session's start with no text hashed, and projects by hand.
- 8.4: `machine.db`'s three new migrations and tables; replies counted once within linked sessions.
- 8.6: the export read only from a zip, entries matched by enclosed name, streamed, never extracted, its limits and refusals; no text, document content, or user's name or email kept; a remote's credentials dropped.

`docs/design/many-teams.md` 2c: the fingerprint is time, model and usage, no text hash; a link needs three shared replies covering one session's start; sources derived; folding by remote stays 06b's. 2b: the first export fixture's shape is inferred and the founder's own export becomes the second; projects by hand from `catervas discover --add-project`, routines in step 06.

- [ ] `docs(spec): record claude.ai's export and one record across sources`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration   # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```

By hand: `H=$(mktemp -d)`, `P=$(mktemp -d)` (empty, so no scheduler answers and step 03 prints nothing); no `$H/.claude/projects`, so steps 03 and 03b read nothing (03b: "with no transcript store, unchanged"); the zip made with `python3 -I -m zipfile -c $H/export.zip` and the three files of `crates/core/src/claude_export/fixtures/2026_10/` (stored at the root).

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo build -q -p catervas
env -i HOME=$H XDG_CONFIG_HOME=$H/.config PATH=$P ./target/debug/catervas discover --claude-export $H/export.zip
# expected:
reading your claude.ai export on this computer; nothing leaves it
read claude.ai's export: 3 conversations (3 new), 2 Projects (2 new)
skipped 1 conversation it couldn't read
reading your Claude Code history on this computer; nothing leaves it
no Claude Code history on this computer
claude.ai: 3 conversations, 2 Projects, latest export up to 2026-10-05
  2026-10  1 conversation
  2026-09  1 conversation
  2026-08  0 conversations
  2026-07  0 conversations
  2026-06  0 conversations
  2026-05  0 conversations
  2026-04  0 conversations
  2026-03  0 conversations
  2026-02  0 conversations
  2026-01  0 conversations
  2025-12  0 conversations
  2025-11  0 conversations
  undated  1 conversation
  Projects:
    Shop launch  2 files  last updated 2026-10-01
    Taxes  no files  last updated 2026-09-12
```
