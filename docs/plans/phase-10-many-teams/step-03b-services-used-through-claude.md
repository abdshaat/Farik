# Phase 10, step 03b: Services used through Claude

Status: ready
Branch: `phase/10-many-teams` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 6.7, 8.4, 8.6; no functional requirement of section 7 (ADR 0059)
Depends on: steps 02 and 03 of this phase (planned; their commits on this branch when this step starts). Nothing of phase 9 or of steps 04 on.
Mockups: none; this step has no screen (step 06 draws the services found and the web-only picker).
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, one round (with 03): not ready on B1 (verification not isolated), B2 (links followed), B3 (output order), B4 (tool calls split across lines); folded with N1–N7.

Step 02's code has landed (e5d37e5c); where this plan cites it, the code wins.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`catervas discover` also says which services the owner reaches through Claude's connectors: from Claude Code's MCP settings (the user's, each project's, a found project's `.mcp.json`, enabled plugins'), claude.ai's connectors as Claude Code knows them, Claude Desktop's settings and extensions, and above all the tool calls in transcripts. Each service is sorted by a catalog in `catervas-core` (bookkeeping, spreadsheets, payments, selling, marketing, code, other), kept once however many sources name it, and listed under every project whose sessions called it, with its calls and when last used. Connector work done from the home folder forms a project of its own, such as "Bookkeeping (Zoho Books)", and a folder with no git whose sessions read or write spreadsheets is marked as bookkeeping. No key is read into the index or printed: from a settings file only a server's name, kind, host and program name are kept.

Out of scope: the screens and the web-only picker (06); suggested connections and making a team for a formed project (06b); the sign-in reminder and `mcp-needs-auth-cache.json` (07); usage per team (08); serve, the daemon, `apps/web/**` (phase 9 executes beside this step); services for someone with no Claude Code history (06's picker).

## Decisions

### Where each part lives

- **Core decides from strings; store reads files and the index; CLI finds paths and prints**, as step 02. Core: `claude_history`'s `ToolUse` (step 03's) gains a spreadsheet flag; a new `claude_services` (settings parsing, tool names, merging, forming projects) with `claude_services/catalog.rs` (the catalog) and `claude_services/fixtures.rs`. Store: a new `claude_services` (reading settings files), `machine` (two tables, `found_services`). CLI: `state.rs` (paths), `discover.rs` (printing). Rejected: putting services in `claude_history`, already ~300 lines and about transcripts.
- **Shared files get additions only:** one `pub mod` line in core's and store's `lib.rs`; migration 3 appended to `MACHINE_MIGRATIONS`; one field on step 03's `ToolUse`; tool calls written beside step 03's in step 02's `record_file`. Nothing in `serve.rs`, `crates/runtime/src/daemon/**` or `apps/web/**`.
- **Doc comments:** core `/// Which services the owner reaches through Claude's connectors: names from settings, tool calls, the catalog.`; store `/// Reading the connectors Claude's settings name into the machine index, never a key.`

### Tool calls in the transcript reader

- **Step 03 owns the reader's tool calls** (`Reply.tools: Vec<ToolUse>`, `ToolUse { id, name, cron }`, migration 2 and its re-read). This step consumes them and adds one field, `touches_spreadsheet: bool`: true when `name` is in `FILE_TOOLS = ["Read", "Write", "Edit", "MultiEdit"]` and `input.file_path`'s extension, case-insensitive, is in `SPREADSHEET_EXTENSIONS = ["xlsx", "xls", "csv"]`. Only a boolean is kept, never the path, and no other `input` field is read. Rejected: `Bash` commands and `Glob`/`Grep` patterns, which are free text that may hold a key and do not name one file. If step 03's committed `ToolUse` differs from the shape above, its file wins and the field is added to that.
- **The server of a call** is `server_segment(name)`: for `mcp__<server>__<tool>`, the non-empty segment up to the next `__`. The full tool name is stored raw (`mcp__claude_ai_Zoho_Books__list_invoices`), so a catalog update re-sorts old calls without a rescan, and the Microsoft 365 rule below can read the tool's name.
- **Kept in `tool_calls`** (migration 3): a tool use with a server or `touches_spreadsheet`, keyed by its `id`, pointing at its `reply_key`. Claude Code writes one line per content block, so one reply's tool uses arrive over several lines; step 03 merges them by tool-use id, and so does `INSERT OR IGNORE` here. So a resumed session's repeated line adds nothing, and a call's session is its reply's, decided once by step 02. Rejected: a session id on the call, which would need step 02's earliest-session rule again.
- **Migration 3 reads every transcript again once**, as step 03's migration 2 does (every `transcript_files` row's `read_to`, `turns` and `unreadable_lines` set to 0), since files read under migration 2 kept no `tool_calls`. Step 02's upserts, step 03's session crons and `INSERT OR IGNORE` by `id` are idempotent. Rejected: a separate backfill pass, the same reading twice.

### Reading settings, never a key

- **Files opened, and only these** (missing, unreadable or not JSON: adds nothing, no notice; step 06 shows sources): `<claude.json>` (`$CLAUDE_CONFIG_DIR/.claude.json` when that is set and not empty, else `$HOME/.claude.json`); `<claude_dir>/settings.json`; `<claude_dir>/plugins/installed_plugins.json`; for each enabled plugin, `<installPath>/.mcp.json` and `<installPath>/.claude-plugin/plugin.json`, only when the canonical `installPath` is inside the canonical `<claude_dir>/plugins/`; `<root>/.mcp.json` for each present root in step 02's `projects`; for each Claude Desktop folder, `claude_desktop_config.json` and `Claude Extensions/<dir>/manifest.json`. The root's `.mcp.json`, the desktop config and each manifest are opened only when they are regular files and their canonical path is exactly that file inside its canonical folder (the root, the desktop folder, the extension's folder), as step 03 opens `scheduled_tasks.json`, so a link pointing elsewhere is not followed. Never `.credentials.json`, `Claude Extensions Settings/`, `mcp-needs-auth-cache.json` or anything else.
- **From a server entry** (an object under `mcpServers`, read as `serde_json::Value`): `type` (`stdio`, `http`, `sse`; absent: `stdio` with `command`, else `http` with `url`, else skipped; any other value skipped), the host of `url` (`url::Url::host_str`, lower case, so user, password, port, path and query are dropped; unparsable: none), and the program: `command`'s last `/` or `\` segment, none when it holds whitespace or `=`. Never `env`, `headers`, `args`, `oauth` or any other field. Rejected: typed `Deserialize` structs, which fail a whole map on one odd entry.
- **Where servers come from:** `~/.claude.json`'s top `mcpServers` and `projects.<path>.mcpServers`; a root's `.mcp.json` minus the names in `projects.<root>.disabledMcpjsonServers`; `claudeAiMcpEverConnected` strings, `claude.ai ` dropped from the front (kind `http`, no host, no program); plugins whose id is `true` in `settings.json`'s `enabledPlugins` and that have a `"scope": "user"` install in `installed_plugins.json` (its `.mcp.json`'s `mcpServers`, else that file's top-level object as the map of servers; an inline `mcpServers` object in `plugin.json`; a path string there is not followed); a desktop config's `mcpServers`; an extension manifest's `name` (kind `stdio`). Rejected: project-scoped plugin installs and project `settings.json` files, since a plugin actually used is still found by its `mcp__plugin_<plugin>_<server>__` calls; marketplace folders, never read, so a listed but not enabled plugin adds nothing.
- **Claude Desktop's folders** (`claude_desktop_dirs`): Linux `$XDG_CONFIG_HOME/Claude`, else `$HOME/.config/Claude`; macOS `$HOME/Library/Application Support/Claude`; Windows `%APPDATA%\Claude`. On Linux with `WSL_DISTRO_NAME` set and not empty, also Windows' folders: for each folder in `/mnt/c/Users` except `Public`, `Default`, `Default User`, `All Users`, its Claude folders are `AppData/Roaming/Claude` and every `AppData/Local/Packages/Claude_*/LocalCache/Roaming/Claude` (the Microsoft Store install, found on the founder's computer 2026-10-12) that hold `claude_desktop_config.json` or `Claude Extensions/`. **Only when exactly one Windows user has such a folder** are that user's read (ruling, 2026-10-12); with several, none is read and `--json` says `desktop: "several_windows_users"`, since another person's connectors would be counted as the owner's. `// ponytail: C: at /mnt/c only; a changed automount root is not followed.` Rejected: reading `/mnt/c` without `WSL_DISTRO_NAME`, which tests on a WSL machine would do to the real disk; reading every user's.
- **Settings are a snapshot:** each scan replaces every `configured_servers` row in one transaction. Tool calls are the history.

### Which servers are services, and which service

- **Not services:** `NOT_SERVICES = ["catervas", "farik", "ccd_session", "claude-code-remote", "ide", "Claude_Browser", "Claude_Preview", "remote-devices", "computer-use", "claude-in-chrome", "Claude_Docs"]`, compared normalised against a call's server name (so `claude_ai_Claude_Docs` is `Claude Docs`, `claudedocs`): Catervas's own (before and after the rename), Claude Code's session and remote-control servers, its IDE bridge, the Claude app's browser, preview, device and Chrome surfaces, and Claude Docs, Anthropic's own documents connector (the names seen in the founder's transcripts and tool lists, 2026-10-12; no other Anthropic product was seen as a `claude_ai_` server). They count nowhere.
- **A tool call's server name:** `claude_ai_<Name>` is `<Name>` with `_` as spaces, origin claude.ai; `plugin_<plugin>_<server>` is `<server>` (split at the first `_` after `plugin_`), origin plugin; anything else is itself, origin settings.
- **Normalised:** lower case, ASCII letters and digits only, then one leading `mcp` and one trailing `mcpserver`, `mcp` or `server` removed (`zoho-books-mcp`, `Zoho Books`, `zoho_books` are all `zohobooks`).
- **The catalog is a `const` table in core**, rejected: an embedded JSON or YAML, which code.md's wire rule would give a schema and a parse-error path for data the compiler can check. A service matches by host first (equal to, or ending in `.` plus, a listed host), then by normalised name, then by the program's normalised file stem, each against an entry's `names`:

  | id | name | kind | names | hosts |
  |---|---|---|---|---|
  | `zoho_books` | Zoho Books | bookkeeping | zohobooks | books.zoho.com, books.zoho.eu, books.zoho.in, books.zoho.com.au |
  | `quickbooks` | QuickBooks | bookkeeping | quickbooks, qbo, intuitquickbooks | quickbooks.api.intuit.com |
  | `xero` | Xero | bookkeeping | xero | xero.com |
  | `freshbooks` | FreshBooks | bookkeeping | freshbooks | freshbooks.com |
  | `wave` | Wave | bookkeeping | wave, waveapps, waveaccounting | waveapps.com |
  | `sage` | Sage | bookkeeping | sage, sageaccounting, sageintacct | sage.com, intacct.com |
  | `microsoft_365` | Microsoft 365 and Excel | spreadsheets | excel, microsoft365, m365, office365, microsoftgraph, msgraph | graph.microsoft.com, office.com, office365.com |
  | `google_sheets` | Google Sheets | spreadsheets | googlesheets, gsheets, sheets | sheets.googleapis.com |
  | `stripe` | Stripe | payments | stripe | stripe.com |
  | `paypal` | PayPal | payments | paypal | paypal.com |
  | `shopify` | Shopify | selling | shopify | shopify.com, myshopify.com |
  | `amazon_seller_central` | Amazon Seller Central | selling | amazonsellercentral, sellercentral, amazonsellingpartner, spapi | sellercentral.amazon.com, sellingpartnerapi-na.amazon.com, sellingpartnerapi-eu.amazon.com, sellingpartnerapi-fe.amazon.com |
  | `buffer` | Buffer | marketing | buffer | buffer.com |
  | `google_ads` | Google Ads | marketing | googleads, adwords | googleads.googleapis.com |
  | `github` | GitHub | code | github | github.com, githubcopilot.com |
  | `vercel` | Vercel | code | vercel | vercel.com |
  | `sentry` | Sentry | code | sentry | sentry.io |

  `ServiceKind` is ordered bookkeeping, spreadsheets, payments, selling, marketing, code, other; an unknown server is `other`. **Its name** is that of its first configured server, in source order (`claude_code`, `claude_ai`, `claude_desktop`) then `found_in` then name; with none, that of its earliest call (ties by tool-use id).
- **Microsoft 365 is not only Excel** (ruling, 2026-10-12): its hosts and names also serve Outlook and Teams. A call to `microsoft_365` counts as spreadsheets work only when its tool name, case-insensitive, contains `excel`, `workbook`, `worksheet` or `sheet`; otherwise that call counts as other (`call_kind`). The service record keeps kind spreadsheets.
- **One record per service** (design 2c): the key is the catalog id; else `<normalised name>@<host>`, the host empty when none. A tool call takes the host of the configured servers with its normalised name when they all have one host, else none. Its sources are the set of `claude_code` (Claude Code's settings and plugins), `claude_ai` (`claudeAiMcpEverConnected`, and calls of origin claude.ai), `claude_desktop` and `transcripts` (any call). Its host is the first configured host in source order. No `services` table: services are derived on each read from `configured_servers` and `tool_calls`, so the catalog applies to old rows.

### Where a service belongs, and projects formed from connector work

- **Only the owner's sessions count** (place `owner`, `gone`, `not_a_project`); Catervas's agents' sessions do not, since their tools are Catervas's own connectors.
- **A service is attached to a project root** for each call by a session placed under that root: calls and last use, most calls first, ties by name. Configured servers attach to no project (only calls say where a service is used). Rejected: attaching project-scoped settings, which needs placing folders no session used.
- **Home work forms a project.** Only `not_a_project` sessions (home, `/`, `/tmp`, `/var/tmp`, `TMPDIR`); a scratch folder such as `/tmp/x` is already its own project (step 02). These three rules were decided by the phase's coordinator on 2026-10-12, from the founder's "bookkeeping services such as Excel or zoho books":
  - **Spreadsheet connectors are bookkeeping work.** Works: bookkeeping (bookkeeping and spreadsheets services), payments, selling, marketing. Rejected: a separate "Spreadsheets (Excel)" project, which splits the founder's one kind of work in two.
  - **Code and unknown services form nothing.** Rejected: "Code (GitHub)", since code work belongs in a repository its team needs; "Other (<name>)", since an unknown kind cannot propose a team.
  - **"Mostly", exactly:** a session is W-work when at least `MIN_WORK_CALLS = 3` of its calls are of W's kinds (by `call_kind`) and those are more than half of all its calls to services. Rejected: summing all home sessions before deciding, which lets one long session drown many short ones; no minimum, which makes one stray call a project.

  All W-work sessions form one project, `"<Work> (<service>)"`, the service with the most W calls among them, ties by name; its last worked is its sessions' latest; its sessions leave "not in a project".
- **A spreadsheet folder is bookkeeping:** a present root with `is_git` 0 whose owner sessions made at least `MIN_WORK_CALLS` spreadsheet calls. It stays the same project, marked `bookkeeping`.

### `catervas discover`

- **Order:** `read_transcripts`, `place_sessions`, step 03's `find_jobs`, then `read_service_settings`, then `found`, `found_jobs`, `found_services`. With no transcript store, unchanged (services are not read; the web-only picker is step 06's).
- **Printed** (names through `printable`; "1 call", "2 calls"; dates as step 02). A formed project is a row among the projects, newest first, with no path, no `Catervas's agents` line and no jobs; a bookkeeping folder has `bookkeeping` after its path. Under each project row: its Catervas line (step 02), its `jobs:` block (step 03), then one `services:` line when it has services. After every section, step 03's `this computer:` included, one line of every service, by kind then name, left out when there are none:
  ```
  projects: 2
    Bookkeeping (Zoho Books)  last worked 2026-10-12  1 session  6 in  6 out
      services: Zoho Books 3 calls, last 2026-10-12; slack 1 call, last 2026-10-12
    books  ~/books  bookkeeping  last worked 2026-10-11  2 sessions  3k in  1k out
      jobs: 1
        ...
      services: Microsoft 365 and Excel 4 calls, last 2026-10-11
  this computer: 1 job
    ...
  services you use with Claude: Zoho Books, Microsoft 365 and Excel, Stripe, slack
  ```
  Lower case first, as step 02's lines are.
- **`--json`:** every project gains `work` (`null`, `bookkeeping`, `payments`, `selling`, `marketing`) and `services: [{ id, name, kind, calls, last_used_at }]`; a formed project is `{ root: null, name, is_git: false, ownership: "yours", work, last_worked_at, own, catervas: { sessions: 0, tokens: {} }, jobs: [], services }`; `not_a_project` is what is left; the top level gains `services: [{ id, name, kind, host, sources }]`, `sources` sorted, and `desktop`: `"several_windows_users"` in that case, else `null`.

## File map

```
crates/core/src/claude_history.rs                                     modifies, tests: ToolUse.touches_spreadsheet (Task 1)
crates/core/src/claude_history/fixtures.rs                            modifies: CLAUDE_CODE_2_1_296_CONNECTORS (Task 1)
crates/core/src/claude_history/fixtures/claude_code_2_1_296_connectors.jsonl  creates (Task 1)
crates/core/src/claude_services.rs, crates/core/src/lib.rs            creates, tests; modifies: pub mod (Tasks 2, 4)
crates/core/src/claude_services/fixtures.rs                           creates: settings full of keys (Task 2)
crates/core/src/claude_services/catalog.rs                            creates, tests (Task 3)
crates/store/src/migrations.rs                                        modifies: MACHINE_MIGRATIONS: [Migration; 3] (Task 5)
crates/store/src/migrations/machine/0003_services.sql                 creates (Task 5)
crates/store/src/machine.rs                                           modifies, tests: tool calls (5), configured servers (6), found_services (7)
crates/store/src/claude_history.rs                                    modifies, tests: tool calls in each file's transaction (Task 5)
crates/store/src/claude_services.rs, crates/store/src/lib.rs          creates, tests; modifies: pub mod (Task 6)
crates/cli/src/state.rs, crates/cli/src/discover.rs                   modifies, tests: paths (Task 8), reading and printing (Task 8)
crates/cli/tests/discover.rs                                          modifies (Task 8)
docs/SPEC.md, docs/design/many-teams.md, docs/plans/project-plan.md   modifies (Task 9)
```

## Interfaces

Consumes, from step 03 on this branch: `Reply.tools`, `ToolUse { id, name, cron }`, migration 2 (`catervas_core::claude_history`, `catervas_store::migrations`); `find_jobs`, `found_jobs` and the `jobs:` and `this computer:` printing (`catervas_store`, cli). From step 02: `read_line`, `TranscriptLine`, `Turn`, `Reply`, `CHECKED_VERSIONS` (`catervas_core::claude_history`); `MachineIndex`, `open_machine_index`, `Activity`, `Found`, `found`, `record_file`, `FileRead`, `MACHINE_MIGRATIONS`, `apply_machine` (`catervas_store::machine`, `migrations`); `read_transcripts`, `place_sessions` (`catervas_store::claude_history`); `claude_dir`, `discover` and its token formatter (cli). On `main`: `url::Url` (a core dependency), `printable`, `StoreError`.

Produces:

```rust
// catervas_core::claude_history
pub const FILE_TOOLS: [&str; 4] = ["Read", "Write", "Edit", "MultiEdit"];
pub const SPREADSHEET_EXTENSIONS: [&str; 3] = ["xlsx", "xls", "csv"];
pub struct ToolUse { /* step 03's id, name, cron */ pub touches_spreadsheet: bool }
// catervas_core::claude_history::fixtures
pub const CLAUDE_CODE_2_1_296_CONNECTORS: &str;
pub const CONNECTOR_FIXTURES: [(&str, &str); 1] = [("2.1.296", CLAUDE_CODE_2_1_296_CONNECTORS)];  // one per CHECKED_VERSIONS entry
// catervas_core::claude_services
pub enum ServerKind { Stdio, Http, Sse }                       // as_wire, from_wire
pub struct ConfiguredServer { pub name: String, pub kind: ServerKind, pub host: Option<String>, pub program: Option<String> }
pub fn mcp_servers(json: &str) -> Vec<ConfiguredServer>;      // the object's `mcpServers`, sorted by name
pub struct ClaudeJson { pub user: Vec<ConfiguredServer>, pub projects: BTreeMap<String, Vec<ConfiguredServer>>,
                        pub disabled_mcp_json: BTreeMap<String, BTreeSet<String>>, pub claude_ai: Vec<String> }
pub fn read_claude_json(json: &str) -> ClaudeJson;
pub fn user_plugin_installs(settings_json: &str, installed_plugins_json: &str) -> Vec<(String, String)>;  // (plugin id, installPath)
pub fn extension_name(manifest_json: &str) -> Option<String>;
pub fn server_segment(tool_name: &str) -> Option<&str>;      // mcp__<server>__<tool> → <server>
pub enum ServerOrigin { Settings, ClaudeAi, Plugin }
pub fn called_server(segment: &str) -> Option<(String, ServerOrigin)>;
pub const NOT_SERVICES: [&str; 11];
pub fn is_a_service(name: &str) -> bool;
pub enum ServiceSource { ClaudeCode, ClaudeAi, ClaudeDesktop, Transcripts }   // as_wire, from_wire; Ord
pub struct Configured { pub source: ServiceSource, pub server: ConfiguredServer }
pub struct Service { pub key: String, pub name: String, pub kind: ServiceKind, pub host: Option<String>, pub sources: BTreeSet<ServiceSource> }
pub struct SessionUse { pub session_id: String, pub root: Option<String>, pub is_home: bool, pub calls: Vec<Call> }
pub struct Call { pub tool_use_id: String, pub tool_name: String, pub at: DateTime<Utc> }   // tool_name raw, mcp__…
pub fn call_kind(service: &CatalogEntry, tool_name: &str) -> ServiceKind;   // the Microsoft 365 rule; else the entry's kind
pub struct ServiceUse { pub key: String, pub name: String, pub calls: u64, pub last_used_at: DateTime<Utc> }
pub enum Work { Bookkeeping, Payments, Selling, Marketing }   // as_wire; label(): "Bookkeeping", ...
pub struct FormedProject { pub name: String, pub work: Work, pub session_ids: Vec<String>, pub services: Vec<ServiceUse> }  // services: all its sessions called
pub struct ServicesFound { pub services: Vec<Service>, pub by_root: BTreeMap<String, Vec<ServiceUse>>, pub formed: Vec<FormedProject> }
pub const MIN_WORK_CALLS: u64 = 3;
pub fn find_services(configured: &[Configured], sessions: &[SessionUse]) -> ServicesFound;
pub fn is_bookkeeping_folder(is_git: bool, spreadsheet_calls: u64) -> bool;
// catervas_core::claude_services::catalog
pub enum ServiceKind { Bookkeeping, Spreadsheets, Payments, Selling, Marketing, Code, Other }  // as_wire; Ord
pub struct CatalogEntry { pub id: &'static str, pub name: &'static str, pub kind: ServiceKind,
                          pub names: &'static [&'static str], pub hosts: &'static [&'static str] }
pub const CATALOG: [CatalogEntry; 17];
pub fn normalise(name: &str) -> String;
pub fn match_service(name: &str, host: Option<&str>, program: Option<&str>) -> Option<&'static CatalogEntry>;
// catervas_core::claude_services::fixtures
pub fn a_claude_json_full_of_keys() -> String;   // each key a distinct KEY-MARKER-<n>
pub fn a_desktop_config_full_of_keys() -> String;
pub fn a_plugin_mcp_json_full_of_keys() -> String;
// catervas_store::claude_services
pub struct SettingsPaths { pub claude_json: Option<PathBuf>, pub claude_dir: PathBuf, pub desktop_dirs: Vec<PathBuf> }
pub fn read_service_settings(index: &mut MachineIndex, paths: &SettingsPaths) -> Result<(), StoreError>;
pub enum WindowsClaude { None, One(Vec<PathBuf>), Several }     // One: that user's Claude folders, sorted
pub fn windows_claude_dirs(users_root: &Path) -> WindowsClaude;
// catervas_store::machine
pub struct FormedFound { pub name: String, pub work: Work, pub last_at: DateTime<Utc>, pub activity: Activity, pub services: Vec<ServiceUse> }
pub struct FoundServices { pub services: Vec<Service>, pub by_root: BTreeMap<String, Vec<ServiceUse>>, pub formed: Vec<FormedFound>,
                           pub bookkeeping_roots: BTreeSet<String>, pub not_a_project: Activity }
impl MachineIndex { pub fn found_services(&self) -> Result<FoundServices, StoreError>; }
// catervas (cli), state.rs
pub(crate) fn claude_json(env: &BTreeMap<String, String>) -> Option<PathBuf>;
pub(crate) fn claude_desktop_dirs(env: &BTreeMap<String, String>, os: &str) -> Vec<PathBuf>;   // os: std::env::consts::OS
```

Migration `0003_services.sql`: `tool_calls (tool_use_id TEXT PRIMARY KEY, reply_key TEXT NOT NULL, tool_name TEXT NOT NULL, touches_spreadsheet INTEGER NOT NULL CHECK (touches_spreadsheet IN (0,1)), at TEXT NOT NULL) STRICT` (a row only for a tool with a server or touching a spreadsheet), index `tool_calls_by_reply (reply_key)`; `configured_servers (source TEXT NOT NULL CHECK (source IN ('claude_code','claude_ai','claude_desktop')), found_in TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('stdio','http','sse')), host TEXT, program TEXT, PRIMARY KEY (source, found_in, name)) STRICT`, `found_in` being `''` (user level, claude.ai), a project path, `plugin:<id>`, a desktop folder, or `extension:<folder name>`; duplicates `INSERT OR IGNORE`; then the `transcript_files` reset, as migration 2's.

## Tasks

A test marked *guard* holds a property the first green code already has; it is not watched to fail, and the landing review checks it by mutation (keep `env`, keep the file path) and the test must then fail.

### Task 1: Whether a tool call touches a spreadsheet

Files: `claude_history.rs` (`touches_spreadsheet` set where step 03 builds each `ToolUse`), `fixtures.rs`, the new `.jsonl`; step 03's `ToolUse` literals gain `touches_spreadsheet: false`. Fixture, by hand, session `33333333-3333-4333-8333-333333333333`, `cwd` `/home/me`, version `2.1.296`, model `claude-opus-5-5`, usage 1/1/0/0 on each reply, every other text `NEVER-KEPT-TEXT`, all 2026-10-12 UTC: T1 09:10:00 `msg_T1`, a `text` block then `tool_use` `toolu_1` `mcp__claude_ai_Zoho_Books__list_invoices` with `input.organization_id` the marker; T2 09:10:10 `msg_T2` `toolu_2` `mcp__claude_ai_Zoho_Books__get_invoice`; T3a 09:10:20 `msg_T3` `toolu_3` `mcp__claude_ai_Zoho_Books__create_expense`; T3b 09:10:21 `msg_T3` again (one block per line, as Claude Code writes them) `toolu_4` `Read` of `/home/me/books/NEVER-KEPT-TEXT.XLSX`; T4 09:10:30 `msg_T4` `toolu_5` `Bash` `cat NEVER-KEPT-TEXT.csv`; T5 09:10:40 `msg_T5` `toolu_6` `Write` of `/home/me/notes.md`; T6 09:11:00 `user` with a `tool_result` block; T7 09:11:10 model `<synthetic>`, `msg_S2`, `toolu_7` `mcp__stripe__list`; T8a 09:11:20 `msg_T8` `toolu_8` `mcp__plugin_product-management_slack__send_message`; T8b 09:11:21 `msg_T8` `toolu_9` `Edit` of `/home/me/a.csv`.

- `tells_a_tool_call_that_touches_a_spreadsheet`: for each `(version, fixture)` of `CONNECTOR_FIXTURES`, whose versions are exactly `CHECKED_VERSIONS`, the fixture read line by line gives T3a the one tool `toolu_3` and T3b `toolu_4`, T8a `toolu_8` and T8b `toolu_9`, with `touches_spreadsheet` true on `toolu_4` and `toolu_9` alone, false on `toolu_1` to `toolu_3`, `toolu_5` (`Bash`), `toolu_6` (`.md`) and `toolu_8`; T7 is no reply. RED: no field.
- `keeps_no_file_path` (guard): no line of the fixture reads into anything whose `Debug` holds `NEVER-KEPT-TEXT`.
- `reads_a_file_path_tolerantly`: `file_path` a number or absent is false; `C:\b\x.CSV` and `/a/b.XlS` are true; `/a/xlsx` and `/a/b.xlsx.bak` false; a `Glob` with `pattern` `*.csv` false. RED.

- [x] `feat(core): tell a claude code tool call that touches a spreadsheet`

### Task 2: Server names from settings, never a key

Files: `claude_services.rs` (parsing, `called_server`), `claude_services/fixtures.rs`, `lib.rs`.

- `reads_a_servers_name_kind_host_and_program`: `mcp_servers` of entries `a` `{type: http, url: https://u:KEY-MARKER-1@mcp.zoho.example:8443/mcp?key=KEY-MARKER-2}`, `b` `{command: /usr/local/bin/stripe-mcp, args, env}`, `c` `{type: sse, url: "not a url"}`, `d` `{command: "API_KEY=KEY-MARKER-3 npx"}`, `e` `{}`, `f` `{url: https://h.example/x}`, `g` `{type: websocket, url: wss://w.example}` is `a` http `mcp.zoho.example` no program; `b` stdio no host `stripe-mcp`; `c` sse no host; `d` stdio no program; `f` http `h.example`; no `e`, no `g`; not JSON gives none. RED: no module.
- `keeps_no_key_from_settings` (guard): everything read from the three fixtures by `read_claude_json` and `mcp_servers` has a `Debug` holding no `KEY-MARKER`.
- `reads_claude_json`: from `a_claude_json_full_of_keys`: the user servers; `projects` only for paths with servers; `disabled_mcp_json` for `/home/me/shop` `{sentry}`; `claude_ai` `["Higgsfield", "Zoho Books", "plain"]` from `["claude.ai Higgsfield", "claude.ai Zoho Books", "plain", 7]`. Not JSON: all empty. RED.
- `finds_enabled_user_plugins`: `enabledPlugins` `{pm@kw: true, fin@s: false, proj@x: true}` with installs pm user `P1`, fin user `P2`, proj project `P3` gives `[("pm@kw", "P1")]`. RED.
- `reads_an_extensions_name`: `{name: "zoho-books", display_name, user_config: {key: KEY-MARKER-4}}` gives `zoho-books`; no `name`, `None`. RED.
- `names_the_server_a_tool_called`: `server_segment` of `mcp__claude_ai_Zoho_Books__list` is `claude_ai_Zoho_Books`, of `mcp__github__get_me` `github`, of `Read`, `mcp__x` and `mcp____t` `None`; `called_server` of `claude_ai_Zoho_Books` is (`Zoho Books`, ClaudeAi); `plugin_product-management_slack` (`slack`, Plugin); `github` (`github`, Settings); `plugin_x` (`plugin_x`, Settings); `claude_ai_` and `""` are `None`. RED.

- [x] `feat(core): read connector names from claude settings without keys`

### Task 3: The service catalog

Files: `claude_services/catalog.rs`.

- `matches_each_catalog_entry`: for every entry, each of its `names`, each host and `mcp.` plus each host match it; `Zoho Books`, `zoho_books`, `zoho-books-mcp`, `mcp-zoho-books` match `zoho_books`; `Unknown Thing` matches none; ids are unique. RED: no module.
- `matches_by_host_then_name_then_program`: name `github` with host `mcp.stripe.com` is `stripe`; name `my-books` with program `quickbooks-mcp.exe` is `quickbooks`; host `notstripe.com` alone matches none. RED.
- `leaves_out_catervas_and_claude_codes_own_servers`: `is_a_service` is false for each of `NOT_SERVICES`, for `Catervas`, `FARIK` and `farik-mcp`, true for `zoho-books`. RED.

- [ ] `feat(core): sort the services used through claude by a catalog`

### Task 4: One record per service, and projects from connector work

Files: `claude_services.rs` (`find_services`, `call_kind`, `is_bookkeeping_folder`). Times `d1 < d2`; "a call `x`" is a `Call` with tool name `mcp__x__do`.

- `keeps_one_service_across_sources`: Zoho from `claude_code` (`zoho-books`, host `books.zoho.com`), `claude_ai` (`Zoho Books`), `claude_desktop` (`zoho_books`) and a call `claude_ai_Zoho_Books` is one `Service` `zoho_books`, sources all four, host `books.zoho.com`. RED: no function.
- `keys_an_unknown_service_by_name_and_host`: `widget` configured with host `w.example` and a call `widget` are one `widget@w.example`, sources `claude_code`, `transcripts`; calls `claude_ai_Higgsfield` at d2 and `higgsfield` at d1 are one `higgsfield@`, kind other, named `higgsfield` (its earliest call); configured `Widget` in a project and `widget` at user level, the name is `widget` (`found_in` `''` first); `acme` configured with hosts `a.example` and `b.example` and a call `acme` are three, `acme@a.example`, `acme@b.example`, `acme@`. RED.
- `counts_no_call_to_what_is_not_a_service`: calls to `catervas`, `farik`, `claude-code-remote`, `claude_ai_Claude_Docs` add no service, no use, and do not count in a session's calls. RED.
- `attaches_services_to_projects_by_their_calls`: s1 under `/home/me/shop` calls `github` at d1, d2 and `stripe` at d1; s2 there `github` at d1: `by_root["/home/me/shop"]` is GitHub 3 last d2, then Stripe 1; services come ordered by kind then name. RED.
- `forms_a_project_from_connector_work_at_home`: home h1 Zoho ×3; h2 Zoho ×2, Xero ×2; h3 Stripe ×3, Zoho ×1; h4 Zoho ×2; h5 GitHub ×5: `formed` is `Bookkeeping (Zoho Books)` `[h1, h2]` then `Payments (Stripe)` `[h3]`. RED.
- `needs_more_than_half_of_a_sessions_calls`: home Zoho ×3 with GitHub ×3 forms nothing; with GitHub ×2 forms Bookkeeping; a session under a root with Zoho ×5 forms nothing. RED.
- `counts_microsoft_365_as_spreadsheets_only_for_excel_tools`: `call_kind(microsoft_365, …)` is spreadsheets for `mcp__microsoft365__excel_read_range`, `…__get_Workbook`, `…__list_worksheets` and `…__update_sheet`, and other for `mcp__microsoft365__send_mail` and `…__list_teams`; `call_kind(xero, "mcp__xero__x")` is bookkeeping. A home session of three `mcp__microsoft365__excel_read_range` calls forms `Bookkeeping (Microsoft 365 and Excel)`; one of three `mcp__microsoft365__send_mail` forms nothing, and its service is still `microsoft_365`, kind spreadsheets. RED.
- `tells_a_bookkeeping_folder`: `is_bookkeeping_folder(false, 3)` true, `(false, 2)` false, `(true, 10)` false. RED.

- [ ] `feat(core): form projects from work done through claude connectors`

### Task 5: Tool calls in the index

Files: `0003_services.sql`, `migrations.rs`, `machine.rs` (`FileRead` gains the file's tool calls, inserted `OR IGNORE`, written by `record_file` beside step 03's session crons), `claude_history.rs`. Step 03's migration test expects `[1, 2]` after migration 2 only; step 02's `opens_the_machine_index_and_records_its_migration` now expects `[1, 2, 3]` and the two tables. Tests on a temporary Claude folder as step 02's Task 4.

- `adds_the_services_tables_and_reads_every_transcript_again` (`migrations.rs`): a database brought to version 2 through `apply_list` with a `transcript_files` row (500, 6, 2); `apply_machine` makes it `[1, 2, 3]`, both tables exist, and the row is (0, 0, 0). RED.
- `keeps_each_tool_call_once`: the connectors fixture gives `tool_calls` rows `toolu_1`, `toolu_2`, `toolu_3`, `toolu_8` (their full tool names), `toolu_4` and `toolu_9` (`touches_spreadsheet` 1), and no others; both tools of `msg_T3` (`toolu_3`, `toolu_4`) and both of `msg_T8` (`toolu_8`, `toolu_9`), each from its own line, have a row with that `reply_key`; `toolu_1`'s `reply_key` is `msg_T1`; a second file of another session repeating T1 adds no row; after every `transcript_files` row is set to (0, 0, 0) and the file read again, `tool_calls`, `sessions` and `replies` are row for row as before. RED: no table.
- `keeps_no_tool_input` (guard): after reading the connectors fixture, the bytes of `machine.db` hold no `NEVER-KEPT-TEXT`.

- [ ] `feat(store): keep claude code tool calls in the machine index`

### Task 6: Reading the settings files

Files: `claude_services.rs` (store), `machine.rs` (replace `configured_servers`), `lib.rs`. A temporary home with `.claude.json`, `.claude/` and a desktop folder; `projects` rows inserted with `rusqlite`.

- `reads_every_settings_source`: `.claude.json` with user `stripe` (http, `https://mcp.stripe.com`), project `/p` server `acme`, `claudeAiMcpEverConnected` `["claude.ai Zoho Books"]`, `projects.<root>.disabledMcpjsonServers` `["sentry"]`; `pm@kw` enabled with a user install under `.claude/plugins/cache/kw/pm/1.0/` whose `.mcp.json` names `slack` (http `mcp.slack.com`) and whose `plugin.json` has inline `linear`; a present root whose `.mcp.json` names `vercel` and `sentry`; a desktop folder whose config names `xero` (`command` `/opt/xero-mcp`) and `Claude Extensions/ant.zoho/manifest.json` naming `zoho-books`. Rows are exactly: `claude_code`/`''`/stripe/http/`mcp.stripe.com`; `claude_code`/`/p`/acme; `claude_ai`/`''`/`Zoho Books`/http; `claude_code`/`plugin:pm@kw`/slack and linear; `claude_code`/root/vercel; `claude_desktop`/folder/xero/stdio/program `xero-mcp`; `claude_desktop`/`extension:ant.zoho`/`zoho-books`/stdio. With `stripe` then removed from `.claude.json`, a second read has no stripe row. A second plugin whose `.mcp.json` is a bare map `{ "notion": { "type": "http", "url": "https://mcp.notion.com/mcp" } }` gives row `notion`. RED: no function.
- `keeps_no_key_from_any_settings_file`: the same layout written from the three fixtures, plus `.claude/.credentials.json`, `Claude Extensions Settings/ant.zoho.json` and `.claude/mcp-needs-auth-cache.json` each holding `KEY-MARKER-9`: the bytes of `machine.db` hold no `KEY-MARKER`. RED: written before `read_service_settings` exists, like the test above.
- `refuses_plugin_installs_outside_claudes_plugins`: an enabled user install whose `installPath` is a folder outside `.claude/plugins/` with a `.mcp.json` naming `evil`, and one whose `installPath` is a link inside `plugins/` to it: no `evil` row. RED.
- `follows_no_link_to_a_settings_file`: a present root whose `.mcp.json` is a link to a file elsewhere holding `{ "mcpServers": { "evil": { "type": "http", "url": "https://e.example" } } }`, a desktop folder whose `claude_desktop_config.json` is such a link, and an extension whose `manifest.json` links to a manifest naming `evil`: no `evil` row. RED.
- `skips_settings_it_cannot_read`: `.claude.json` holding `not json` and a desktop folder that does not exist: `Ok`, and the plugin's rows are still read. RED.
- `finds_claude_desktop_under_windows_users`: a users folder where `Abd/AppData/Roaming/Claude/` and `Abd/AppData/Local/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/` each hold a `claude_desktop_config.json`, and the same under `Public`, `Default`, `Default User` and `All Users`, gives `One` of Abd's two, sorted; adding `Bee/AppData/Roaming/Claude/Claude Extensions/` gives `Several`; `Cy/AppData/Roaming/Claude/` with neither file does not count; a users folder that does not exist gives `None`. RED.

- [ ] `feat(store): read the connectors claude settings name, never a key`

### Task 7: Services and connector projects from the index

Files: `machine.rs` (`found_services`: owner, gone and not-a-project sessions with their calls through `replies`, `configured_servers`, then `find_services`, then activity sums; spreadsheet calls summed per present root, then `is_bookkeeping_folder`). Unit tests insert `projects`, `sessions`, `replies`, `tool_calls` and `configured_servers` rows with `rusqlite`; "call `x`" is a `tool_calls` row with `tool_name` `mcp__x__do`.

- `reports_services_per_project`: owner session s1 under present git root `/home/me/shop`, replies r1 (call `github`) and r2 (call `claude_ai_Zoho_Books`), configured `stripe`: `by_root` `/home/me/shop` is GitHub 1, Zoho Books 1; `services` is Zoho Books, Stripe, GitHub. RED: no method.
- `forms_a_project_and_leaves_the_rest_not_in_a_project`: `not_a_project` h1 with three Zoho calls on three replies of 1/1/0/0, and h2 with none and one reply 5/5/0/0: `formed` is one `Bookkeeping (Zoho Books)`, last at h1's, one session, tokens 3/3, services Zoho Books 3; `not_a_project` is one session, 5/5. RED.
- `leaves_catervas_sessions_out_of_services`: a `catervas` session under `shop` calling Zoho gives no service and no `by_root` entry. RED.
- `tells_a_folder_of_spreadsheets`: present root `/home/me/books`, `is_git` 0, owner session with three spreadsheet calls: `bookkeeping_roots` is `{books}`; with two, empty; a git root with five, empty. RED.

- [ ] `feat(store): report services and connector projects from the machine index`

### Task 8: `catervas discover` prints the services

Files: `state.rs` (`claude_json`, `claude_desktop_dirs`, unit tests beside `claude_dir_prefers_claude_config_dir`); `discover.rs` (`SettingsPaths` from them, plus `windows_claude_dirs(Path::new("/mnt/c/Users"))`'s `One` folders when `os` is `linux` and `WSL_DISTRO_NAME` is set and not empty, `Several` giving `desktop: "several_windows_users"`; the order, lines and JSON above); `tests/discover.rs`, home and fixtures as step 02's Task 6, ignored when they reach git.

- `claude_json_follows_claude_config_dir`: `CLAUDE_CONFIG_DIR=/c` gives `/c/.claude.json`; empty with `HOME=/h`, `/h/.claude.json`; neither, `None`. RED.
- `finds_claude_desktop_per_system`: `linux` with `XDG_CONFIG_HOME=/x` gives `[/x/Claude]`, with `HOME=/h` alone `[/h/.config/Claude]`; `macos` `[/h/Library/Application Support/Claude]`; `windows` with `APPDATA=C:\A` `[C:\A\Claude]`; no variable, `[]`. RED.
- `discovers_services_used_through_claude` (ignored): the Verification's home: stdout exactly the Verification's lines; with `--json`, `projects[0]` has `root: null`, `name: "Bookkeeping (Zoho Books)"`, `work: "bookkeeping"`, `services[0]` `{ id: "zoho_books", name: "Zoho Books", kind: "bookkeeping", calls: 3, last_used_at: "2026-10-12T09:10:20Z" }`; top `services` is `zoho_books` (sources `claude_ai`, `transcripts`, host null), `stripe` (`claude_code`, `mcp.stripe.com`), `slack@` (`transcripts`); the formed project's `jobs` is `[]`; `not_a_project` 0 sessions; `desktop` null. RED: no services printed.
- `prints_no_key` (guard, ignored): with the three fixtures as `.claude.json`, a desktop config and a plugin's `.mcp.json` in the home, neither stdout, the `--json` output nor `machine.db` holds `KEY-MARKER`.

- [ ] `feat(cli): print the services used through claude`

### Task 9: Spec

`docs/SPEC.md`, at the next free revision number when the task runs:
- 3: a concept "Services used through Claude (ADR 0059; phase 10 step 03b)": where they are found, the catalog and its kinds, one record per service, attached by calls, home work forming "<Work> (<service>)" by the "mostly" rule, a folder of spreadsheets as bookkeeping, what is not a service.
- 6.7: the owner's Claude connectors are found, never used by a team's agents; a team connects Catervas's own (suggested in step 06b).
- 8.4: `machine.db`'s `tool_calls` and `configured_servers`, the snapshot, the one-time re-read.
- 8.6: from settings only name, kind, host and program are kept; the files opened (by canonical path, no link followed) and those never opened; of a transcript's tool use only its id, name and a file tool's spreadsheet extension; the history reader's paragraph loses "`settings.json`, `.claude.json`" from what it never opens, since the services reader now opens them for names only.

`docs/design/many-teams.md` 2d: the "mostly" rule and `MIN_WORK_CALLS`; spreadsheets services as bookkeeping work, and the Microsoft 365 rule; code and other form nothing; the file tools and extensions; the servers that are not services; user-scope plugin installs only; the Microsoft Store desktop folder and the one-Windows-user rule.

`docs/plans/project-plan.md`, phase 10's table: step 03b's Spec becomes "3, 6.7, 8.4, 8.6".

- [ ] `docs(spec): record the services used through claude`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration   # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```

By hand, isolated as step 03's: step 02's fixture home (`$H`, `.claude/projects/-shop/` with `/home/me` replaced by `$H`, `$H/shop` a repository with one commit by its `user.email` and no schedule file), plus `.claude/projects/-home/` holding the connectors fixture with `/home/me` replaced by `$H`, and `$H/.claude.json` `{"mcpServers":{"stripe":{"type":"http","url":"https://mcp.stripe.com","headers":{"Authorization":"Bearer KEY-MARKER"}}}}`. `$P` holds only a link to `git`, so no scheduler answers and step 03 finds no job; `env -i` keeps out `WSL_DISTRO_NAME` and `CLAUDE_CONFIG_DIR`.

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo build -q -p catervas
env -i HOME=$H XDG_CONFIG_HOME=$H/.config PATH=$P ./target/debug/catervas discover
# expected:
reading your Claude Code history on this computer; nothing leaves it
read 2 transcript files: 2 sessions, 2 new
skipped 2 lines it couldn't read
projects: 2
  Bookkeeping (Zoho Books)  last worked 2026-10-12  1 session  6 in  6 out
    services: Zoho Books 3 calls, last 2026-10-12; slack 1 call, last 2026-10-12
  shop  ~/shop  last worked 2026-10-12  1 session  1.2k in  47 out
services you use with Claude: Zoho Books, Stripe, slack
grep -c KEY-MARKER $H/.config/catervas/machine.db
# expected: 0
```
