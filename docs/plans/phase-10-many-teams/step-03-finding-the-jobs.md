# Phase 10, step 03: Finding the jobs

Status: ready
Branch: `phase/10-many-teams` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 8.4, 8.6; no functional requirement of section 7 (ADR 0059)
Depends on: step 02 of this phase (committed, last as `e5d37e5c`; where this plan cites step 02, its code wins): `machine.db` and its migration list, the transcript reader, the found projects, `catervas discover`. Nothing of phase 9 or of steps 03b on.
Mockups: none; this step has no screen (step 06 draws them).
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, one round (with 03b): not ready on B1 (two sources shared an origin), B2 (tool calls split across lines), B3 (a token in a Vercel path); folded with N1–N9.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`catervas discover` also finds the owner's recurring jobs and prints each under its project, or under "this computer". It looks in the schedules committed in the owner's repositories (GitHub Actions, `vercel.json`, Kubernetes `CronJob`, `render.yaml`, Terraform), in the computer's schedulers (crontab, systemd timers, launchd agents, Windows Task Scheduler under WSL), and in Claude Code (session crons from `CronCreate` calls and `.claude/scheduled_tasks.json`, each with its 7-day expiry, and background jobs in `~/.claude/jobs`).

Jobs are kept in `machine.db`, one row per key. A job keeps its schedule, its name or program name and its project. A command line, an environment value, a prompt or a key is never kept.

The transcript reader also learns each reply's tool calls, kept as id and name only, an interface step 03b extends.

Out of scope: run status, disabled workflows and deployments through GitHub (07); routines by hand (06); connectors (03b); keys by remote and across sources (04, 06b); what a schedule means (07); `/etc/crontab`, `/etc/cron.d`, `/Library/LaunchAgents`; serve, the daemon and every screen.

## Decisions

### Where each part lives

- **Core reads, store gathers and stores, the CLI prints**, as in step 02. Core reads text from `&str` (crontab, systemd, CSV, Terraform, Claude Code's and plutil's JSON). It reads repository YAML and JSON as a `&serde_json::Value`. Store parses that YAML in `files`, the one place a project's YAML is parsed (`files.rs:805`). Its strict booleans keep an unquoted `on:` as a key. Rejected: `serde-saphyr` in core, which would be a second YAML dialect.
- **No new dependency.** Kubernetes files hold several documents, read with `serde-saphyr`'s `from_multiple_with_options`, already locked. `sha2` and `serde_json` are already in core. Rejected: a CSV crate (20 lines save it), a plist crate (`plutil`, below) and an HCL parser (below).
- **Phase 9 executes beside this step**, so shared files get additions only: `pub mod` lines; `yaml_documents` after `yaml_value`; `program_output` appended to `crates/runtime/src/computer.rs`. Nothing touches `serve.rs`, `crates/runtime/src/daemon/**` or `apps/web/**`.
- **Doc comments.** Core `jobs`: `/// The owner's recurring jobs, read from files, schedulers' answers and Claude Code's records.` Store `jobs`: `/// Finding the owner's jobs in their repositories, on this computer and in Claude Code's records.`

### Tool calls in the transcript reader (owned here; 03b extends it)

- **`Reply` (step 02's) gains `tools: Vec<ToolUse>`.** It holds one `ToolUse { id, name, cron }` per `tool_use` block in `message.content`. Only a block's `type`, `id` and `name` are read. For `CronCreate` only, `cron` is `Some(CronCall { cron, is_recurring, is_durable })`, from `input.cron`, `input.recurring` (absent is true) and `input.durable` (absent is false). `is_durable` is kept because a durable call's record is `scheduled_tasks.json` (below). `input.prompt`, every other `input` field and every text block are never read.
- **One block per line.** Claude Code writes each content block of a reply on its own line under the same `message.id`. `read_chunk`'s merge of a reply's lines (`crates/store/src/claude_history.rs:175`) unions `tools` by tool-use id into `ReplyBatch.tools` (`machine.rs:83` gains the field). Session crons are taken per line, keyed by tool-use id, with the session, cwd and timestamp of the line that carries the block. Rejected: the first line's tools only, which misses a `CronCreate` in a later block.
- **`read_line`'s doc comment** becomes: "…`message.content` is read only for its `tool_use` blocks' type, id and name, and `CronCreate`'s schedule fields; no text field is ever read."
- **A block missing `id` or `name` is left out.** Tools ride on a reply, so a line with no reply key or a `<synthetic>` model yields none.
- **The doc comment states the rule for later steps:** "One tool call of a reply: its id and name, and of its input only the fields named here. A later step adds one field for what it needs (03b: whether it touched a spreadsheet file); the input is never kept whole and no text of it is read."
- **No `tools` column in `replies`.** Step 03 stores only `CronCreate` calls, as session crons. 03b decides how to store the rest.
- **Machine.db's migration 2, `migrations/machine/0002_jobs.sql`, makes `jobs` and reads every transcript again once.** It sets every `transcript_files` row's `read_to`, `turns` and `unreadable_lines` to 0, so the next scan reads every file from byte 0. Tool calls in lines step 02 already read are then found. Step 02's upserts give the same result when repeated (earliest and latest times, largest counters, latest title), so nothing counts twice. Two limits: a transcript Claude Code has deleted since keeps its sessions and replies, but its `turns` and `unreadable_lines` stay 0; and the latest title of a session spanning several files may come from a different file than before. Rejected: scanning every transcript for tool calls on every scan.

### Running the schedulers

- **The runner is a function value defined in store:** `RunProgram<'a> = &'a dyn Fn(&str, &[&str]) -> Option<ProgramAnswer>`, where `ProgramAnswer { succeeded, stdout }`. `None` means the program could not be run or passed the time limit. The crates depend `catervas-cli` → `catervas-runtime` → `catervas-store` → `catervas-core` (each crate's `Cargo.toml`; cli also depends on store directly). So store cannot call runtime. Runtime implements the runner as `program_output`, appended after the private `run` (`computer.rs:206`), and returns store's `ProgramAnswer`. It reuses `run`'s lookup on `io.env`'s `PATH`, its cleared environment, its null stdin, its 10 s `CHECK_TIMEOUT` and its process-group kill. The CLI passes `|program, args| program_output(&io.env, program, args)`. Store tests pass closures that answer recorded fixtures. Rejected: a one-method trait, which a closure already is; a second runner in store, copying the timeout and the kill.
- **Every unix computer is asked everything**, one command after another: `crontab -l`; `systemctl [--user] list-timers --all --no-legend --no-pager`, then `systemctl [--user] cat <timer> <unit>` per timer; `plutil -convert json -o - <file>` per regular `~/Library/LaunchAgents/*.plist`; `schtasks.exe /query /fo CSV /v`. A program that ran but failed (`no crontab for …`, WSL with no systemd) answers "nothing". One that is missing answers `None`. Neither prints a line. Rejected: branching by platform, which would miss WSL's `schtasks.exe`.
- **A scheduler answered** when every command it needed ran (`Some`). For Task Scheduler, a successful run must also have had its header recognized. launchd answered when its folder could be listed (a missing folder is an empty answer) and every `plutil` ran. A plist that `plutil` could not convert (a `<date>` or `<data>` value) counts as unreadable; that is the limit.
- **systemd's list is read from the end of each line.** The last two words are the timer and its unit; the date columns before them contain spaces. Rejected: `--output=json`, which older systemd lacks.

### What a job is, and what it keeps

- **Kinds** (`JobKind`; wire value "label"), in this order: `github_actions` "GitHub Actions", `vercel` "Vercel", `kubernetes` "Kubernetes", `render` "Render", `terraform` "Terraform"; `crontab` "crontab", `systemd_user` "systemd (user)", `systemd_system` "systemd", `launchd` "launchd", `task_scheduler` "Task Scheduler"; `session_cron` "session cron", `saved_session_cron` "saved session cron", `background_job` "background job". Rejected: one `scheduler` kind with a free-text source, which step 07's rules could not match on.
- **The key** (design 2c) is `(origin, file, name)`:

  | Job | origin | file | name |
  |---|---|---|---|
  | a repository's file | its root | path relative to the root | the job's name |
  | a scheduler | the kind's wire value | `''` | the unit; for crontab, the first 16 hex characters of the SHA-256 of the trimmed line |
  | `scheduled_tasks.json` | `claude_tasks:<folder>` | `.claude/scheduled_tasks.json` | the task's `id` |
  | `CronCreate` | `claude_session` | the session id | the tool use's `id` |
  | a background job | `claude_jobs` | `''` | its folder's name |

  Saved crons have their own origin, so a repository and its `.claude/` are replaced independently. The root stands in for the repository until 06b keys repositories by remote. The same key twice in one scan is one row, with its schedules joined in order and duplicates dropped. Rejected: a crontab line number, which moves; the line itself, which may hold a token.
- **What is kept:** for every job: its schedules as written, never interpreted; the name shown (the job's name, unit, label or task name, and for crontab the program's file name); its project root; for a session or saved cron, also its expiry, `folder` (the recorded cwd, or the file's folder) and `state_at` (the call's time, or `createdAt`), and its name shown is `Session cron <cron>`, never prompt text; for a background job, also its `state`, `updatedAt` as `state_at`, and `folder`; for every row, `last_seen`: when the row was last confirmed. **Never kept:** command lines and arguments, the paths used for tying, environment lines and `Environment=`, other unit keys, `EnvironmentVariables`, other Task Scheduler columns, prompts, `providerEnv` and every other `state.json` field. Core returns tie candidates in `ComputerJob::paths`, and store drops them after tying. Rejected: keeping the command line to show, since it may hold a token.
- **Crontab:** Blank, `#` and `NAME=value` lines are skipped whole. A job is five schedule fields plus a command, or one `@word` plus a command; any other line is not a job. Text after the first unescaped `%` is dropped. The command is split at `&&`, `||`, `;` and `|`. The program is the file name of the first word, after `NAME=value` words, of the first part that is not `cd`. The tie candidates are the words starting `/`, `~/`, `$HOME/` or `${HOME}/`, with home expanded.
- **systemd:** `systemctl cat`'s blocks are split at their `# <path>` headers. A timer whose unit file is under `/usr/lib/` or `/lib/` came with the system, so it is skipped (logrotate, apt-daily). The schedules are each `OnCalendar=` value, then each other `On…Sec=` line as written. The program and the tie candidates come from `ExecStart=`, without its `-@:+!` prefixes, and from `WorkingDirectory=`, with `%h` expanded to home. A timer with no schedule is not a job.
- **launchd** (plutil's JSON): The name is `Label`, else the file's stem. `StartInterval` *n* is `every n seconds`. Each `StartCalendarInterval` dict, alone or in an array, is the cron `Minute Hour Day Month Weekday`, `*` for a missing field. The program and the tie candidates come from `Program` or `ProgramArguments`, and from `WorkingDirectory`. An agent with no interval is not a job.
- **Task Scheduler:** The CSV is quote-aware. Columns are found by English header and a repeated header row is skipped. With no `TaskName` header the answer is unreadable; that is the limit on Windows in another language. A task's rows are one job. Each row's schedule is `Schedule Type`, `Start Time`, `Days` and `Repeat: Every`, trimmed and joined with `, `, leaving out `N/A`, `Disabled` and empty values. The tie candidates are `Task To Run`'s words, split respecting double quotes, and `Start In`. `X:\…` becomes `/mnt/x/…`; `\\wsl.localhost\<distro>\…` and `\\wsl$\<distro>\…` become `/…`. **Only tasks tied to a project are kept.** The founder's 110 tasks are all Microsoft's or vendors' updaters. Rejected: listing them under "this computer".
- **Tying:** Store keeps only the candidates that exist, canonicalized; a missing one ties to nothing. `tie_to_project` picks the longest present root (`projects.present = 1`) containing any candidate, compared component by component. With none, the job is "this computer". A repository's job takes its root. A session cron takes its session's `project_root` when the session's place is `owner` or `catervas`. Saved crons and background jobs are tied by their folder. A worktree outside its repository's folder ties to nothing; that is the limit.

### Where jobs are looked for

- **A Vercel job's name** is its `path` up to the first `?` or `#`, since a query may hold a key.
- **Repositories:** roots with `present = 1`, `is_git = 1`, `ownership = 'yours'` and sessions placed under them. **Explored repositories are not scanned.** Rejected: scanning them, which would list a cloned upstream's schedules as the owner's jobs. Files come from `Git::tracked_paths` (`git.rs:660`). `.gitignore` keeps out `node_modules/`, and an uncommitted file is not seen. The files read: `.github/workflows/*.yml|yaml` (direct children); any `vercel.json`, `render.yaml` or `*.tf`; any other `*.yml|yaml` whose text holds `CronJob`. A file is opened only if it is regular, at most `JOB_FILE_LIMIT` (1 MiB), and its canonical path is inside the canonical root. A file that does not parse (a Helm template) is unreadable. **Terraform is a line scan, not a parser:** `resource "<type>" "<name>"` or `module "<name>"` opens a block, naming the job `<type>.<name>` or `module.<name>` (`schedule_expression` before any block), and a literal `schedule_expression = "…"` inside is its schedule. Values with `${`, variables, heredocs, commented lines and Google's `schedule =` are not read; that is the limit.
- **Claude Code:** Session crons come from `Turn.reply.tools` as `read_transcripts` reads them, in the file's transaction. A durable call is skipped. A session-only cron expires 7 days after its call (`SESSION_CRON_DAYS`, Claude Code's `recurringMaxAgeMs`), at most; whether its session still runs cannot be known. `.claude/scheduled_tasks.json` is read in each present root and each existing session folder. Only `tasks[].id`, `cron`, `createdAt` (ms), `recurring` and `permanent` are read. A task expires `createdAt` + 7 days when recurring and not permanent, and never otherwise. The file is opened only when its canonical path is the folder's own `.claude/scheduled_tasks.json`. `<claude>/jobs/<id>/state.json` is undocumented, so it is read fail-soft. Only `name` (cut to `TITLE_LIMIT`), `state` (cut to `STATE_LIMIT` = 40), `cwd` and `updatedAt` are read. It is opened only if regular, inside the canonical `jobs/`, and within `JOB_FILE_LIMIT`. Nothing else in a job's folder is opened.

### Each scan

- **A job source's rows are replaced only when it answered**, in one `IMMEDIATE` transaction. Every row written or confirmed gets `last_seen = now`. A source that did not answer keeps its last-known rows and their `last_seen`, so reminders do not flicker. Each source is one origin: a scheduler answered as above; a repository whose `tracked_paths` succeeded (a git refusal there is one unreadable source and the scan goes on; git that cannot be run refuses the scan, as step 02's does); each `claude_tasks:<folder>` whose file is absent or read and parsed (a refused or unparsable file does not answer and is unreadable); `claude_jobs`, always. The stored repository and saved-cron origins that are no longer searched (a gone or explored root, a folder no longer searched) count as answered with nothing. Session crons are never replaced. Their `project_root` is set again from `sessions` on every replace.
- **Order in `discover`:** `read_transcripts`, `place_sessions`, `find_jobs`, `found`, `found_jobs`. Rejected: finding jobs before placement, which would tie them to the previous scan's roots.

### What `discover` prints

- After step 02's "skipped …" line come `found <n> jobs` and `couldn't read jobs from <n> places`. Each project row in "projects" and "explored" gets `    jobs: <n>`, with one line per job at six spaces. After every step 02 section comes `this computer: <n> jobs`, with its jobs at two spaces.
- **A job line** is `<name>  <label>`, then a repository job's file, then its schedules joined with `; `. A session or saved cron prints no name, and ends with `until <date>` or `expired <date>` when it has an expiry. A background job prints `<state>`, then the date of its `state_at`. Step 02's rules hold: UTC `YYYY-MM-DD` dates, "1 job" and "2 jobs", empty sections and lines left out, and everything printed through `printable`.
- **Every job is printed**, except a session cron that expired more than 7 days ago (design 6). Jobs are ordered by kind, then name, then file. A job whose root is not a printed project row goes under "this computer".
- **`--json`:** each project gains `jobs: [job]`; the top level gains `jobs_found`, `unreadable_job_sources` and `this_computer: [job]`. A job is `{ kind, name, file, schedules, expires_at, state, state_at, last_seen }`. `file` is null except for a repository's job; `name` is the name shown.
- **Spec:** the project plan's row names 3 and 6.7, but no role kit changes, so this step changes 3, 8.4 and 8.6. Task 8 changes the row's Spec column to "3, 8.4, 8.6".

## File map

```
crates/core/src/claude_history.rs, .../claude_history/fixtures.rs   modifies: ToolUse, CronCall, Reply.tools; a_cron_create_line (Task 1)
crates/core/src/jobs.rs, crates/core/src/lib.rs                 creates: kinds, tying, saved crons, background jobs (Task 1); pub mod
crates/core/src/jobs/repository.rs, jobs/computer.rs            creates (Tasks 2, 3)
crates/core/src/claude_history/fixtures/cron_create_2_1_296.jsonl     creates: CRON_CREATE_LINES (Task 1)
crates/core/src/jobs/fixtures.rs                                creates; Tasks 1-3 each add the constants of their files below
crates/core/src/jobs/fixtures/{scheduled_tasks.json, background_job_state.json}                     creates (Task 1)
crates/core/src/jobs/fixtures/{github_workflow.yml, github_workflow_unscheduled.yml, vercel.json,
    kubernetes_manifest.yaml, render.yaml, main.tf}                                                 creates (Task 2)
crates/core/src/jobs/fixtures/{crontab.txt, systemctl_timers_system.txt, systemctl_timers_user.txt,
    systemctl_cat_logrotate.txt, systemctl_cat_db_backup.txt, systemctl_cat_shop_sync.txt,
    launchd_shop_sync.json, launchd_keep_alive.json, schtasks.csv}                                  creates (Task 3)
crates/store/src/migrations.rs, .../migrations/machine/0002_jobs.sql   modifies, creates (Task 4)
crates/store/src/machine.rs, crates/store/src/claude_history.rs modifies: job rows, ReplyBatch.tools; tools merged and session crons (Task 4)
crates/store/src/jobs.rs, crates/store/src/lib.rs               creates: computer and Claude Code (Task 5), repositories (Task 6)
crates/store/src/files.rs, crates/store/tests/jobs.rs           modifies: yaml_documents; creates (Task 6)
crates/runtime/src/computer.rs                                  modifies: program_output appended (Task 7)
crates/cli/src/discover.rs, crates/cli/tests/discover.rs        modifies (Task 7)
docs/SPEC.md, docs/design/many-teams.md, docs/plans/project-plan.md   modifies (Task 8)
```

## Interfaces

Consumes:
- From step 02 of this phase (committed): `Turn`, `Reply`, `read_line`, `read_chunk` and `ReplyBatch` (`claude_history.rs:155`, `machine.rs:83`), the test `opens_the_machine_index_and_records_its_migration` (`machine.rs:573`), `TITLE_LIMIT` and the `claude_history::fixtures` builders; `MachineIndex`, `open_machine_index`, `apply_machine`, `apply_list` and `MACHINE_MIGRATIONS`; the `projects`, `sessions` and `transcript_files` tables; `read_transcripts`, `place_sessions` and `HistoryError`; `discover` and `claude_dir`.
- On `main`: `Git::tracked_paths` (`git.rs:660`); `files::yaml_value` and its options (`files.rs:805`, `:828`); `git_in` (`git/fixtures.rs:143`); `run` and the test helper `programs` (`computer.rs:206`, its tests); `printable`.

Produces:

```rust
// catervas_core::claude_history (added to step 02's)
pub struct Reply { /* step 02's fields */ pub tools: Vec<ToolUse> }
pub struct ToolUse { pub id: String, pub name: String, pub cron: Option<CronCall> }   // 03b adds one field
pub struct CronCall { pub cron: String, pub is_recurring: bool, pub is_durable: bool }
// catervas_core::claude_history::fixtures
pub const CRON_CREATE_LINES: &str;   // three lines of one reply, one block each
pub struct CronCreateLine<'a> { pub session_id: &'a str, pub cwd: &'a str, pub at: &'a str, pub message_id: &'a str,
                               pub tool_use_id: &'a str, pub cron: &'a str, pub recurring: Option<bool>, pub durable: Option<bool> }
pub fn a_cron_create_line(line: &CronCreateLine<'_>) -> String;   // one assistant line, one CronCreate block
// catervas_core::jobs
pub const SESSION_CRON_DAYS: i64 = 7;
pub const STATE_LIMIT: usize = 40;
pub enum JobKind { GithubActions, Vercel, Kubernetes, Render, Terraform, Crontab, SystemdUser, SystemdSystem,
                   Launchd, TaskScheduler, SessionCron, SavedSessionCron, BackgroundJob }
impl JobKind { pub fn as_wire(self) -> &'static str; pub fn from_wire(wire: &str) -> Option<Self>; pub fn label(self) -> &'static str; }
pub struct SavedCron { pub id: String, pub cron: String, pub created_at: DateTime<Utc>, pub is_recurring: bool, pub is_permanent: bool }
pub fn read_scheduled_tasks(text: &str) -> Option<Vec<SavedCron>>;      // None: not a JSON object
pub fn cron_expires_at(created_at: DateTime<Utc>, is_recurring: bool, is_permanent: bool) -> Option<DateTime<Utc>>;
pub struct BackgroundJob { pub name: String, pub state: String, pub cwd: Option<String>, pub updated_at: Option<DateTime<Utc>> }
pub fn read_background_job(text: &str) -> Option<BackgroundJob>;
pub fn tie_to_project(paths: &[String], roots: &[String]) -> Option<String>;
// catervas_core::jobs::repository
pub struct FileJob { pub kind: JobKind, pub name: String, pub schedules: Vec<String> }
pub fn schedule_file_kind(path: &str) -> Option<JobKind>;
pub fn jobs_in_documents(kind: JobKind, path: &str, documents: &[Value]) -> Vec<FileJob>;   // all but Terraform
pub fn jobs_in_terraform(text: &str) -> Vec<FileJob>;
// catervas_core::jobs::computer
pub struct ComputerJob { pub kind: JobKind, pub unit: String, pub name: String, pub schedules: Vec<String>, pub paths: Vec<String> }
pub fn read_crontab(text: &str, home: Option<&str>) -> Vec<ComputerJob>;
pub fn timer_units(list: &str) -> Vec<(String, String)>;                 // (timer, unit it starts)
pub fn read_systemd_timer(kind: JobKind, timer: &str, cat: &str, home: Option<&str>) -> Option<ComputerJob>;
pub fn read_launchd_agent(file_stem: &str, plutil_json: &str) -> Option<ComputerJob>;
pub fn read_task_scheduler(csv: &str) -> Option<Vec<ComputerJob>>;      // None: no TaskName header
// catervas_store::machine
pub struct JobRow { pub origin: String, pub file: String, pub name: String, pub kind: JobKind, pub display_name: String,
                    pub schedules: Vec<String>, pub folder: Option<String>, pub project_root: Option<String>,
                    pub expires_at: Option<DateTime<Utc>>, pub state: Option<String>, pub state_at: Option<DateTime<Utc>>,
                    pub last_seen: DateTime<Utc> }
pub struct JobSearch { pub repositories: Vec<String>, pub roots: Vec<String>, pub folders: Vec<String> }
impl MachineIndex {
    pub fn job_search(&self) -> Result<JobSearch, StoreError>;
    pub fn replace_found_jobs(&mut self, answered: &[String], jobs: &[JobRow], now: DateTime<Utc>) -> Result<(), StoreError>;
    pub fn found_jobs(&self, now: DateTime<Utc>) -> Result<Vec<JobRow>, StoreError>;
}
// catervas_store::jobs
pub const JOB_FILE_LIMIT: u64 = 1_048_576;
pub struct ProgramAnswer { pub succeeded: bool, pub stdout: String }
pub type RunProgram<'a> = &'a dyn Fn(&str, &[&str]) -> Option<ProgramAnswer>;
pub struct JobPlaces { pub home: Option<PathBuf>, pub claude_dir: PathBuf }
pub struct JobsSummary { pub repositories: u64, pub jobs: u64, pub unreadable: u64 }
pub fn find_jobs(index: &mut MachineIndex, places: &JobPlaces, run: RunProgram<'_>, now: DateTime<Utc>)
    -> Result<JobsSummary, HistoryError>;
// catervas_store::files
pub fn yaml_documents(text: &str, named: &str) -> Result<Vec<Value>, FilesError>;
// catervas_runtime::computer
pub fn program_output(env: &BTreeMap<String, String>, program: &str, args: &[&str]) -> Option<ProgramAnswer>;
```

`0002_jobs.sql` creates `jobs` and the index `jobs_by_project_root (project_root)`, then resets `transcript_files`:

```sql
jobs (
  origin TEXT NOT NULL, file TEXT NOT NULL, name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (/* the 13 wire values */)),
  display_name TEXT NOT NULL,
  schedules TEXT NOT NULL,   -- a JSON array of strings
  folder TEXT, project_root TEXT, expires_at TEXT, state TEXT, state_at TEXT,
  last_seen TEXT NOT NULL,
  PRIMARY KEY (origin, file, name)
) STRICT
```

## Tasks

**Guard tests.** A test marked *guard* holds a property the first green code already has, so it is not watched to fail. The landing review checks it by mutation (store the line, keep a prompt, open the link), and the test must then fail.

**Fixtures.** Written by hand in the shapes recorded on 2026-10-12: Claude Code 2.1.296; systemd 255 on Ubuntu 24.04 under WSL; the Task Scheduler header from Windows 11's `schtasks.exe`.

Secrets are spelled `SECRET-<where>` and prompts `NEVER-KEPT-TEXT`. Home is `/home/me`, written `\home\me` in Windows paths, and store tests substitute their temporary home.

### Task 1: Tool calls, kinds, tying and Claude Code's files in core

Files: `claude_history.rs`; step 02's tests that build a `Reply` gain `tools: vec![]`. `claude_history/fixtures.rs` gets `CRON_CREATE_LINES`: three `assistant` lines of one reply, as Claude Code writes them, each with session `1111…`, cwd `/home/me/shop`, message `msg_01C`, model `claude-opus-5-5`, usage 1/2/0/0 and one content block: at `09:02:00Z` a text block `NEVER-KEPT-TEXT`; at `09:02:01Z` a `Bash` tool use `toolu_01B` whose `command` is `NEVER-KEPT-TEXT`; at `09:02:02Z` `CronCreate` `toolu_01C` with `cron` `*/10 * * * *`, `prompt` `NEVER-KEPT-TEXT`, and no `recurring` or `durable` (all on 2026-10-12).

`jobs.rs`, with `jobs/fixtures.rs`: `SCHEDULED_TASKS`, each task with `createdAt` 1791795600000: `a1b2c3d4` `0 9 * * 1-5` recurring; `e5f6` `30 14 28 2 *` once; `p1` recurring and permanent; and `{"id":"bad"}`; and `BACKGROUND_JOB`: `shop-cleanup`, `blocked`, cwd `/home/me/shop`, `updatedAt` `2026-10-11T08:00:00.000Z`, `providerEnv` `{"ANTHROPIC_API_KEY":"SECRET-PROVIDER"}`, `needs` `NEVER-KEPT-TEXT`.

Tests:
- `reads_the_tool_calls_of_a_reply`: `CRON_CREATE_LINES` read line by line give replies keyed `msg_01C` with tools `[]`, `[ToolUse { toolu_01B, "Bash", None }]` and `[ToolUse { toolu_01C, "CronCreate", Some(CronCall { "*/10 * * * *", true, false }) }]`. `a_cron_create_line` with `Some(false)` and `Some(true)` reads those flags. A `CronCreate` with no `cron` has `cron: None`. A block with no `id` is left out. Step 02's fixture lines have `tools` `[]`. RED: no field.
- `names_each_kind`: `from_wire(as_wire(k)) == Some(k)` for all 13 kinds; `from_wire("cron")` is `None`; each `label` is as in Decisions. RED.
- `reads_saved_session_crons`: `SCHEDULED_TASKS` gives `a1b2c3d4` (created `2026-10-12T09:00:00Z`, recurring), `e5f6` (once) and `p1` (permanent), in order. `{"tasks":3}` is `Some([])`; `not json` is `None`. `cron_expires_at(t, true, false)` is `t` + 7 days, and `(t, false, false)` and `(t, true, true)` are `None`. RED.
- `reads_a_background_job`: `BACKGROUND_JOB` gives the four fields. With no `cwd` or `updatedAt`, those are `None`. With no `name`, no `state`, or `not json`, the result is `None`. A 300-character name is cut to 200 and a 100-character state to 40. RED.
- `reads_no_prompt_or_key` (guard): the `Debug` of everything read above holds no `NEVER-KEPT-TEXT` and no `SECRET`.
- `ties_a_job_to_the_deepest_project`: with roots `/home/me/a` and `/home/me/a/b`: `["/x", "/home/me/a/b/c.sh"]` gives `/home/me/a/b`; `["/home/me/a"]` gives `/home/me/a`; `["/home/me/ab/x"]` and `[]` give `None`. RED.

- [x] `feat(core): read tool calls, saved session crons and background jobs`

### Task 2: Schedule files in core

Files: `repository.rs`. These fixtures are for store's tests; core's tests build documents with `json!`: `GITHUB_WORKFLOW`: `name: nightly-sync`, an unquoted `on:` with `schedule` `0 3 * * *` and `30 4 * * 1`, and `workflow_dispatch`; `GITHUB_WORKFLOW_UNSCHEDULED`: `on: [push]`; `VERCEL_JSON`: `/api/cron/digest` `0 5 * * *`, `/api/cron/cleanup` `0 0 * * 0` and `/api/cron/x?key=SECRET-VERCEL` `0 * * * *`; `KUBERNETES_MANIFEST`: a Deployment, CronJob `report` `15 2 * * *`, and a CronJob with no schedule; `RENDER_YAML`: a `web` service, and cron `daily-report` `0 6 * * *`; `TERRAFORM_MAIN_TF`: `aws_cloudwatch_event_rule.nightly` `cron(0 3 * * ? *)`, `aws_scheduler_schedule.hourly` `rate(1 hour)`, a `"${var.schedule}"` and a commented line.

Tests:
- `classifies_schedule_files`: `.github/workflows/nightly.yml` is `GithubActions`; `.github/workflows/sub/x.yml` and `k8s/report.yaml` are `Kubernetes`; `apps/web/vercel.json` is `Vercel`, `render.yaml` is `Render` and `infra/main.tf` is `Terraform`; `README.md` and `vercel.json.bak` are `None`. RED: no module.
- `finds_a_workflows_schedules`: the workflow gives one job `nightly-sync` with both crons, and is named `nightly` when it has no `name`. `on` as `"push"`, `["push"]` or `{push: null}`, or an empty `schedule`, gives none. A `schedule` entry with no `cron` is left out. RED.
- `finds_vercel_render_and_kubernetes_jobs`: Vercel gives three jobs named by path, the third `/api/cron/x` (cut at `?`; `/a#b` is `/a`), leaves out an entry with no `schedule`, and `{}` gives none. Render gives `daily-report` alone. The three Kubernetes documents give `report` alone. RED.
- `finds_terraform_schedule_expressions`: `TERRAFORM_MAIN_TF` gives exactly the two named jobs with their values. An expression before any block is named `schedule_expression`. RED.

- [x] `feat(core): find the schedules in a repository's files`

### Task 3: The computer's schedulers in core

Files: `computer.rs`. Fixtures: `CRONTAB`, one line each: a comment; `SHELL=/bin/bash`; `GITHUB_TOKEN=ghp_SECRET-CRON-ENV`; `0 2 * * * API_KEY=SECRET-CRON-INLINE /home/me/shop/bin/sync.sh --token SECRET-CRON-ARG`; `*/15 * * * * cd ~/blog && ./publish.sh > /dev/null 2>&1`; `@daily $HOME/notes/backup.sh`; `0 1 * * * /usr/bin/backup-all % SECRET-CRON-STDIN`; `not a cron line`. `SYSTEMCTL_TIMERS_SYSTEM`: the recorded `logrotate` row and an `n/a` row for `db-backup`. `SYSTEMCTL_TIMERS_USER`: `shop-sync`. `SYSTEMCTL_CAT_LOGROTATE`: as recorded, under `/usr/lib/systemd/system/`. `SYSTEMCTL_CAT_DB_BACKUP`: in `/etc/systemd/system/`, with `OnBootSec=15min`, `OnUnitActiveSec=1d` and `ExecStart=-/usr/local/bin/db-backup`. `SYSTEMCTL_CAT_SHOP_SYNC`: in `~/.config/systemd/user/`, with `OnCalendar=*-*-* 02:30:00`, `ExecStart=%h/shop/bin/sync --key SECRET-UNIT`, `Environment=TOKEN=SECRET-ENV` and `WorkingDirectory=%h/shop`. `LAUNCHD_SHOP_SYNC` (hand-written in plutil's JSON shape; no Mac was recorded): `Label` `com.me.shop-sync`; `ProgramArguments` `["/home/me/shop/bin/sync","--token","SECRET-LAUNCHD"]`; `StartCalendarInterval` `[{Hour:2,Minute:30},{Weekday:1,Hour:9,Minute:0}]`; `EnvironmentVariables` holding `SECRET-LAUNCHD-ENV`; `WorkingDirectory` `/home/me/shop`. `LAUNCHD_KEEP_ALIVE`: no interval. `SCHTASKS_CSV`, with the header twice: `\Microsoft\Windows\Defrag\ScheduledDefrag`; `\Adobe Acrobat Update Task`, in two rows: `At logon time`; and `Daily`, `11:00:00 AM`, `Every 1 day(s)`; `\ShopSync`: `Task To Run` `wsl.exe -d Ubuntu /home/me/shop/bin/sync --key SECRET-TASK`, `Start In` `\\wsl.localhost\Ubuntu\home\me\shop`, `Comment` `SECRET-COMMENT`, `Daily`, `2:30:00 AM`; `\ShopReport`: `"C:\Users\me\shop\report, final.exe"`, `Weekly`, `MON`, `9:00:00 AM`.

Tests:
- `reads_a_crontab`: with home `/home/me`, four jobs in order (schedule, program, paths): `0 2 * * *`, `sync.sh`, `[/home/me/shop/bin/sync.sh]`; `*/15 * * * *`, `publish.sh`, paths containing `/home/me/blog`; `@daily`, `backup.sh`, `[/home/me/notes/backup.sh]`; `0 1 * * *`, `backup-all`, `[/usr/bin/backup-all]`. Units are 16 lowercase hex characters, distinct, and the same when read again. RED: no module.
- `never_reads_a_token_from_a_crontab_line` (guard): the `Debug` of those jobs holds no `SECRET`.
- `reads_systemd_timers`: `timer_units` gives `[(logrotate.timer, logrotate.service), (db-backup.timer, db-backup.service)]` for the system list and `shop-sync` for the user list; logrotate is `None`; db-backup gives schedules `[OnBootSec=15min, OnUnitActiveSec=1d]`, program `db-backup`, paths `[/usr/local/bin/db-backup]`; shop-sync, given home, gives `[*-*-* 02:30:00]`, program `sync`, paths `[/home/me/shop/bin/sync, /home/me/shop]`; nothing read holds `SECRET`. RED.
- `reads_a_launchd_agent`: `LAUNCHD_SHOP_SYNC` gives unit and name `com.me.shop-sync`, schedules `[30 2 * * *, 0 9 * * 1]`, program `sync`, paths `[/home/me/shop/bin/sync, /home/me/shop]`. `StartInterval` 3600 gives `[every 3600 seconds]`. `LAUNCHD_KEEP_ALIVE` and `not json` give `None`. Nothing read holds `SECRET`. RED.
- `reads_windows_task_scheduler`: four jobs named by `TaskName`: Acrobat's schedules are the two strings `At logon time` and `Daily, 11:00:00 AM, Every 1 day(s)`; ShopSync's paths include `/home/me/shop/bin/sync` and `/home/me/shop`; ShopReport's paths are `[/mnt/c/Users/me/shop/report, final.exe]`. With no `TaskName` header the result is `None`. Nothing read holds `SECRET`. RED.

- [x] `feat(core): read the computer's schedulers`

### Task 4: Jobs in the machine index

Files: `0002_jobs.sql`; `migrations.rs` (`MACHINE_MIGRATIONS: [Migration; 2]`); `machine.rs` (`ReplyBatch.tools`, the job rows; step 02's `opens_the_machine_index_and_records_its_migration` now expects `[1, 2]`); `claude_history.rs` (`read_chunk` unions a reply's tools by id; session crons per line, in the file's transaction). Tests seed `projects` and `sessions` with `rusqlite`, in 8.4's format.

- `adds_the_jobs_table_and_reads_every_transcript_again` (`migrations.rs`): a database at version 1 (via `apply_list` with the first migration) holds a `transcript_files` row (500, 6, 2). After `apply_machine` the versions are `[1, 2]`, `jobs` exists and the row is (0, 0, 0). RED.
- `replaces_only_the_sources_that_answered` (`machine.rs`): At t1, replace crontab row `A` and background row `B`, both answered. At t2, with `crontab` and `claude_jobs` answered, rows `[A']`: `B` is gone and `A` holds `A'`'s fields, `last_seen` t2. At t3, with only `claude_jobs` answered and no rows: `A'` is kept with `last_seen` t2. Throughout, a seeded `session_cron` survives. Its `project_root` is its session's root when the session is `owner`, NULL when `not_a_project`. Key `K` given twice, with `["x"]` and `["y", "x"]`, is one row `["x", "y"]`. RED.
- `lists_jobs_by_kind_and_name` (`machine.rs`): `found_jobs` at `2026-10-12T12:00:00Z` orders by kind, name, then file, and round-trips `schedules`. It leaves out a session cron that expired `2026-10-01` and keeps one that expired `2026-10-08`. RED.
- `merges_a_replys_tools_from_every_line` (`claude_history.rs`): `read_chunk` over `CRON_CREATE_LINES` gives one `ReplyBatch` `msg_01C` whose tools are `toolu_01B` and `toolu_01C`; the same lines twice still give those two. RED.
- `reads_session_crons_from_transcripts` (`claude_history.rs`): the step 02 fixture, then `CRON_CREATE_LINES`, then a durable `a_cron_create_line`, give one `jobs` row: origin `claude_session`, file `1111…`, name `toolu_01C`, kind `session_cron`, display name `Session cron */10 * * * *`, `["*/10 * * * *"]`, folder `/home/me/shop`, expiring `2026-10-19T09:02:02Z`, `state_at` `09:02:02Z`. A second read changes nothing. RED.
- `reads_a_transcript_again_without_counting_twice`: after that read, `transcript_files` is set to (0, 0, 0), as migration 2 does, and the transcript is read again. `new_sessions` is 0. `sessions`, `replies` and `jobs` match row for row, and the file's `turns` and `unreadable_lines` equal their first values. RED.
- `keeps_no_prompt_or_tool_input` (guard): `machine.db`'s bytes hold no `NEVER-KEPT-TEXT`.

- [x] `feat(store): keep jobs in the machine index`

### Task 5: The computer's and Claude Code's jobs

Files: `jobs.rs` (`find_jobs` without repositories). The tests use a temporary home with `shop` seeded as a root with `is_git` 0, holding the files `bin/sync.sh` and `bin/sync` so that the fixtures' paths exist (a missing path ties to nothing). The runner closure answers each exact command with `Some(ProgramAnswer { succeeded: true, stdout: <fixture> })` and every other command with `None`.

- `finds_the_computers_jobs`: from crontab, rows `sync.sh` tied to `shop` (with `bin/sync.sh` removed, untied), and `publish.sh`, `backup.sh` and `backup-all` untied; from systemd, `db-backup` (`systemd_system`, untied) and `shop-sync` (`systemd_user`, tied), and no `logrotate`; from Task Scheduler, `\ShopSync` tied, and nothing for `\ShopReport`, Acrobat or Defrag. `jobs` is 7 and `unreadable` 0. RED: no module.
- `never_keeps_a_token_from_a_crontab_line` (guard): after that scan `machine.db`'s bytes hold none of `SECRET-CRON-ENV`, `-INLINE`, `-ARG` or `-STDIN`, nor any `SECRET`.
- `keeps_a_schedulers_jobs_when_it_does_not_answer`: scan 2 runs one hour later. The runner answers `None` for `crontab` and for `systemctl --user cat shop-sync.timer shop-sync.service`. All four crontab rows and the `shop-sync` row are unchanged, `last_seen` at scan 1. The system timers answered, so `db-backup` has `last_seen` at scan 2. In scan 3, `crontab -l` answers `Some { succeeded: false, stdout: "" }` (no crontab). No `crontab` rows remain. RED.
- `reads_launchd_agents_through_plutil`: with `Library/LaunchAgents/com.me.shop-sync.plist` and `broken.plist`, where plutil answers `succeeded: false` for the second, the scan gives one `launchd` row tied to `shop`, and `unreadable` is 1. RED.
- `finds_claude_codes_saved_crons_and_background_jobs`: `shop/.claude/scheduled_tasks.json` holds `SCHEDULED_TASKS`, `<claude>/jobs/abc12345/state.json` holds `BACKGROUND_JOB`, and `jobs/def/state.json` holds `not json`. The result: three `saved_session_cron` rows, origin `claude_tasks:<home>/shop`, display names `Session cron <cron>`, tied to `shop`; `a1b2c3d4` expires `2026-10-19T09:00:00Z` and the other two never; one `background_job` row, `abc12345`, shown as `shop-cleanup`, `blocked`, tied to `shop`; `unreadable` 1; no byte of `NEVER-KEPT-TEXT` or `SECRET`. RED.
- `refuses_whatever_is_not_a_job_file`: three files exist, and none gives a row: `jobs/evil/state.json`, a link to `<claude>/.credentials.json` whose `name` is `SECRET-MARKER`; `jobs/x/timeline.jsonl`, holding the marker; `shop/.claude/scheduled_tasks.json`, a link out of `shop`. `machine.db` holds no marker. RED.

Tests reduced to one per major functionality (ADR 0060, founder 2026-10-12).

- [x] `feat(store): find the jobs on this computer and in claude code`

### Task 6: The schedules in the owner's repositories

Files: `jobs.rs` (repositories); `files.rs` (`yaml_documents`); `tests/jobs.rs`. The tests are `#[ignore = "needs the git program: cargo xtask check --integration"]`, with repositories made by `git_in` and seeded with `is_git` 1.

- `reads_every_document_of_a_yaml_file` (`files.rs`): `a: 1\n---\nb: 2\n` gives two values; `on: x` keeps the key `on`; a duplicate key in the second document is `Invalid`. RED.
- `finds_the_schedules_in_a_repository`: `shop` (yours) commits: `.github/workflows/nightly.yml` (`GITHUB_WORKFLOW`); `.github/workflows/ci.yml` (unscheduled); `vercel.json`; `deploy/k8s.yaml`; `deploy/chart/templates/cron.yaml`, holding `kind: CronJob` and `{{ .Values.schedule }}`; `render.yaml`; `infra/main.tf`. It also has an ignored `node_modules/x/vercel.json` and an untracked `extra/vercel.json`. The scan gives exactly eight rows with origin `<shop>`, each tied to `shop` and carrying its relative file: `nightly-sync`, three Vercel, `report`, `daily-report` and two Terraform. `repositories` is 1 and `unreadable` 1. No byte of `SECRET-VERCEL` reaches `machine.db`. RED.
- `keeps_each_origin_when_another_fails`: scan 1 finds `shop`'s repository rows and its `.claude/scheduled_tasks.json` crons. In scan 2, `shop/.git/HEAD` holds `garbage`, so git refuses there, while `.claude/` answers: the repository rows keep scan 1's `last_seen` and the saved crons get scan 2's. In scan 3, `HEAD` is restored and `scheduled_tasks.json` holds `not json`: the repository rows get scan 3's `last_seen` and the saved crons keep scan 2's. RED.
- `scans_only_your_repositories`: an `explored` repository, a gone root and an `is_git` 0 root, each with a committed `vercel.json`, give no rows. A stored row of a root that has since become explored is removed. RED.
- `refuses_a_tracked_link_out_of_the_repository`: a committed `vercel.json` that links to `<tmp>/outside.json`, with a cron at `/SECRET-LINK`, gives no row and no marker in `machine.db`. RED.

Tests reduced to one per major functionality (ADR 0060, founder 2026-10-12).

- [x] `feat(store): find the schedules in the owner's repositories`

### Task 7: `catervas discover` prints the jobs

Files: `computer.rs` (`program_output`, after `on_path`); `discover.rs` (`find_jobs` after `place_sessions`, with `JobPlaces { home, claude_dir }` and the runner over `io.env`; the lines and the JSON); `tests/discover.rs`. Step 02's tests set no `PATH`, so their output is unchanged.

- `answers_a_programs_output` (`computer.rs`, via `programs`): a script printing `hi` gives `Some { succeeded: true, stdout: "hi\n" }`; one exiting 1 gives `Some { succeeded: false, … }`; a program not on `PATH` gives `None`. RED.
- `discovers_the_jobs_of_a_project` (ignored, needs git). Step 02's fixture home, plus: `shop` commits `GITHUB_WORKFLOW` as `.github/workflows/nightly.yml`; `io.env`'s `PATH` is only a folder holding a `crontab` script (git is found through the process's own environment, as `git.rs` runs it) printing `0 2 * * * <home>/shop/bin/sync.sh --token SECRET-CLI` and `0 1 * * * /usr/bin/backup-all`; `<home>/shop/bin/sync.sh` exists; `<home>/.claude/jobs/abc12345/state.json` holds `BACKGROUND_JOB`, with home substituted. Expected: exit 0, and stdout exactly the Verification's lines; with `--json`, `projects[0].jobs[0]` is `{ kind: "github_actions", name: "nightly-sync", file: ".github/workflows/nightly.yml", schedules: ["0 3 * * *", "30 4 * * 1"], expires_at: null, state: null, state_at: null, last_seen: <the clock's time> }`; `this_computer[0]` has `kind` `crontab`, `name` `backup-all` and `file` null; `jobs_found` is 4; `machine.db` holds no `SECRET`. RED: no jobs printed.
- `prints_jobs_as_text` (guard, ignored): a crontab program `a\u{1b}[2Jb` is printed `a\u001b[2Jb`.

Tests reduced to one per major functionality (ADR 0060, founder 2026-10-12).

- [x] `feat(cli): print the jobs catervas found`

### Task 8: Spec

`docs/SPEC.md`, at the next free revision number when the task runs:
- **3**, in step 02's concept "Claude Code history": jobs. Cover the kinds and where each is looked for, the key, tying and "this computer", tied Task Scheduler tasks only, system timers skipped, explored repositories not scanned, session crons' 7 days, and tool calls kept as id and name.
- **8.4:** machine.db's migration 2 (`jobs`, its columns and key, `last_seen`), replacement only for a source that answered, session crons kept, and the one re-read of every transcript.
- **8.6:** the programs run, each read-only, on `PATH`, within 10 s; the fields kept and never kept; no tool input read but `CronCreate`'s schedule fields; files opened only by canonical path inside their repository, `jobs/` or `.claude/`. Remove `jobs/` from the sentence "It never opens `jobs/`, …", and replace the sentence saying none of a line's text fields (`message.content`, …) is read with: `message.content` is read only for its `tool_use` blocks' type, id and name and `CronCreate`'s schedule fields, and no text field is read.
- **3, line 67:** drop `jobs` from "Out of this step: …".

`docs/design/many-teams.md` section 3: Decisions' limits (Task Scheduler, system timers, worktrees outside the folder, Terraform's literal values, explored repositories). `docs/plans/project-plan.md`: step 03's Spec column becomes "3, 8.4, 8.6".

- [ ] `docs(spec): record finding the jobs`

## Verification

Phase 9 builds concurrently in another worktree, so every cargo command uses this worktree's build folder:

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration   # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```

By hand, on step 02's fixture home `$H`. `$H/shop` commits `.github/workflows/nightly.yml` from `GITHUB_WORKFLOW`, and `$H/shop/bin/sync.sh` exists, and `$H/.claude/jobs/abc12345/state.json` holds `BACKGROUND_JOB` (`/home/me` replaced by `$H`). `$P` holds the `crontab` script above and a link to `git`, so that no real scheduler answers.

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo build -q -p catervas
env -i HOME=$H XDG_CONFIG_HOME=$H/.config PATH=$P ./target/debug/catervas discover
# expected:
reading your Claude Code history on this computer; nothing leaves it
read 1 transcript file: 1 session, 1 new
skipped 2 lines it couldn't read
found 4 jobs
projects: 1
  shop  ~/shop  last worked 2026-10-12  1 session  1.2k in  47 out
    jobs: 3
      nightly-sync  GitHub Actions  .github/workflows/nightly.yml  0 3 * * *; 30 4 * * 1
      sync.sh  crontab  0 2 * * *
      shop-cleanup  background job  blocked  2026-10-11
this computer: 1 job
  backup-all  crontab  0 1 * * *
```
