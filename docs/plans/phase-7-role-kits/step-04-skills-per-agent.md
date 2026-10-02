# Phase 7, step 04: Skills per agent

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6, 6.7, 8.2, 8.3, 8.5, 8.6; F9
Depends on: step 01 of this phase (committed; `canonical_json`, `here_or_sent`'s use for a team-file change, `local_project_id`), step 03 (planned; it lands first in plan order and nothing of it is consumed, but it touches the same files, `team.schema.json`, `core/team.rs`, `daemon/team.rs` and the event and command schemas, so rebase onto it knowingly), phase 6 (merged in #19)
Readiness confirmed by: fresh-session Opus reviewer, 2026-10-02: ready with findings, 4 Blocking, all folded; no second round (ADR 0032)

## Goal

Today each role's skills are written whole into every session's prompt (ADR 0011), and the user cannot add one. When this step is done, the user can give one agent, or the whole team, a skill in the Agent Skills format, a folder with a `SKILL.md` and the files it refers to, with `farik skill add` and through the daemon's commands and reads. They read it before it is added, and edit or remove it later. A session sees each such skill's name and description; it loads the body when it uses the skill, and a referenced file when it reads it, through Claude Code's own skills mechanism. A skill that arrived in the project by a clone, a pull or a hand edit is used only after the user has read and confirmed it on this computer. Out of scope: the agent page's Skills section and its dialogs, which step 04b builds on this step's commands and reads; a kit's skills, which step 05 loads through this step's folder; a saved team template carrying skills; skills that run scripts.

## Decisions

Research (2026-10-02; code.claude.com/docs/en/skills and /plugins, undated, read that day; agentskills.io/specification; the readiness review's credential-free probes of Claude Code 2.1.287, run with `--setting-sources ""`, `--disallowedTools Bash` and a scripted local Messages API):
- **`--setting-sources ""` turns off the user's and the project's `.claude/skills`.** A plugin folder given with `--plugin-dir` still loads, in `-p` mode, with no marketplace; it needs only `.claude-plugin/plugin.json` `{"name":"farik"}`. Its skills are named `farik:<skill>`. A `SKILL.md` nested inside a skill's folder is not discovered.
- **Skills are called through the built-in tool `Skill`**, whose input schema is `{ skill: string, args?: string }` with no other field; a `PreToolUse` hook sees it. A bare name equal to one of Claude Code's own skills loads Claude Code's, not the plugin's. A referenced file is read with `Read`, by its path in the skill's folder.
- **Frontmatter fields that change behaviour:** `when_to_use`, `argument-hint`, `arguments`, `disable-model-invocation`, `user-invocable`, `allowed-tools`, `disallowed-tools`, `model`, `effort`, `context`, `agent`, `background`, `hooks`, `paths`, `shell`, `metadata`, `license`, `compatibility`. Each is read from the frontmatter alone, so a copy whose frontmatter holds only `name` and `description` drops them all.
- **A body may run commands at load**, inline `` !`cmd` `` and a block opened by ```` ```! ````. `--disallowedTools Bash` stops both; `"disableSkillShellExecution": true` in `--settings` stops both even with Bash allowed.
- **`@<path>` in a loaded skill is attached by Claude Code's own read**, which no hook sees and only a `Read` deny rule stops (`@~/secret.txt` and `@../outside.txt` attached).
- **Claude Code ships skills of its own**, `deep-research`, `update-config`, `debug` and others, listed even with `--setting-sources ""`.
- **The specification's limits:** a name of at most 64 characters, lower-case letters, digits and single hyphens, equal to its folder's; a description of at most 1024 characters; a body of under 500 lines recommended.

Where skills live:
- **Three levels, as the project plan decided.**
  - The role's, embedded in the binary: `crates/roles/roles/<role>/skills/<name>/SKILL.md`.
  - The team's: `.farik/skills/<name>/`.
  - One agent's: `.farik/agents/<agent_id>/skills/<name>/`.

  All are read from the project's root, never a task's worktree, as `team.yaml` is. Agents cannot write `.farik/` (5.3).
- **The role's `role.yaml` skills stay in the prompt** (ADR 0011), in every session of the role, unless replaced (below). The sessions given one Farik tool alone (triage, the judgment, the design-plan decision) have no `Skill` tool, and the Product Manager's contract writing is its job in triage. Rejected: moving them to load on demand, which would take them from those sessions. Step 05's kit skills, and the team's and an agent's, load on demand.
- **One name, the most specific wins: agent, then team, then role.**
  - An agent's pin replaces the team's skill of that name whatever the agent's skill's state, so a skill waiting for review never falls back silently to the team's.
  - A team or agent skill whose name is in `core_skill_names()` replaces that shipped skill only when the person said so explicitly: `skill_save` and `skill_confirm` with such a name are refused `skill_name_taken` unless they carry `replace_shipped: true`. Every load needs one of those commands on this computer (Trust), so a loaded replacement was always confirmed as one. When it is `in_use`, the sessions that load skills leave the shipped skill out of the prompt and load the replacement on demand; one-tool sessions keep the shipped skill in their prompt, since they load no skills.
- **The states**, as `skill_rows` computes them for one agent, checked in this order:
  - `replaced`: a team row when the agent pins a skill of that name; a role row when a team or agent skill of that name is `in_use`.
  - `missing`: a pinned folder that does not exist.
  - `review`: the folder exists, but its `skill_sha256` differs from the pin, or the newest `skill.added`, `skill.changed` or `skill.confirmed` for it in this machine's log does not carry the pin's hash, or a `skill.removed` follows that event, or there is none, or `read_skill_folder` or `check_skill` refuses it.
  - `in_use`: otherwise. A role row is `in_use` unless `replaced`. `bytes` is the folder's total, 0 when missing, and the `SKILL.md`'s size for a role row.
- **Whole-team writes keep the pins.** `team.save` and `agent.replace` keep the top-level `skills` and each kept agent's `skills` as the team file holds them, whatever the browser sends. `template.apply` keeps the top-level `skills` and the `skills` of each agent it keeps; an agent it removes loses its pins as removing an agent does. Only `skill_save`, `skill_remove` and `skill_confirm` change a pin.

Loading into a session:
- **A plugin folder per session, outside the project.** The Claude adapter writes it at `<state>/skills/<local project id>/<session id>/`, where `<state>` is the user's Farik state folder and `<local project id>` is `local_project_id(state, root)` (ADR 0030's place for a connector's folder). It is 0700, written fresh at each start and resume, removed when the Claude Code process exits whatever the outcome, and `<state>/skills/<local project id>/` is removed whole when the daemon starts. It holds `.claude-plugin/plugin.json` `{"name":"farik"}` and `skills/<name>/` with a copy of each skill whose `SKILL.md` frontmatter is rewritten to `name` and `description` alone. The adapter passes `--plugin-dir <that folder>`, and adds `Skill` to `--tools`, only when the session has at least one skill. Every session's `--settings` sets `"disableSkillShellExecution": true`. Rejected: under `.farik/local/sessions/<id>/`, because the shipped protected path `.farik/local/**` is a `Read` deny rule Claude Code applies to sessions run in the project's root (`conversation`, `ceremony`, `chat`), which a hook's `allow` cannot lift (probed). Rejected: `.claude/skills` in the worktree, which `--setting-sources ""` turns off and which an agent could commit. Rejected: every skill in the prompt, the cost ADR 0011 names.
- **Which sessions.** Every session not given one Farik tool alone: the task sessions, `conversation`, `ceremony` and `chat`.
- **The hook.** A `Skill` call is allowed only when its input is exactly `{ "skill": "farik:<name>" }` or `{ "skill": "farik:<name>", "args": <string> }`, with `<name>` one of the session's skills. Anything else (a bare name, Claude Code's own skills, another field, a non-string) is denied `skill_not_in_session`. It is judged in `judge` after the `tool_call_limit` check and before the connector check, asks no tier, meets no plan gate, and an allowed call counts towards `max_tool_calls`.
- **Reads in the skill folder.** A `Read`, `Glob` or `Grep` whose `file_path` or `path`, resolved through links (`resolve`), lies under the registration's `skills_root` is allowed at tier `read` with no paths, so `allowed_paths` and the protected paths are not asked about. A `Glob` pattern stays relative. Every other tool's path there, and any write, stays `path_outside_workspace`.
- **The prompt** gains nothing. Claude Code lists the skills itself.

Trust:
- **A skill is instructions an agent follows**, so the user reads all of it before it is used. Adding or editing one shows the whole `SKILL.md` and each file's name and size first, and the add is the confirmation.
- **Pinned in the team file.** `team.yaml` gains `skills: [{ name, sha256 }]` at the top level for the team, and on each agent for that agent. `sha256` is `skill_sha256`: the sha256 of `canonical_json` of `{ <relative path>: <sha256 hex of the file's bytes> }` over every file in the folder.
- **Confirmed on this computer**, as ADR 0030 does for a connector: a skill loads only when it is `in_use` (the states above), that is when its folder hashes to its pin, the newest `skill.added`, `skill.changed` or `skill.confirmed` for it in this machine's log (8.4, never committed) carries that hash, and no `skill.removed` follows. Why: `.farik/` travels with the repository, so a pull could change a skill's instructions and its pin together.
  - `skill_confirm`, with the hash the person saw, confirms one. The daemon reads the folder, refuses `skill_hash_mismatch` unless its `skill_sha256` equals the hash sent, refuses whatever `check_skill` refuses with that refusal's code, writes that hash as the pin when the pin differs, then records `skill.confirmed`. So a skill whose folder or pin changed outside Farik is used again only after the person has read the folder as it is now.
- **No commands and no attached files.** A `SKILL.md` holding `` !` `` anywhere, or a line whose first characters, after spaces, are three or more backticks or tildes followed by `!`, is refused `skill_runs_commands`. A `SKILL.md` with an `@` at the start of a line or after whitespace, followed by a character that is not whitespace (regex `(^|\s)@\S`, multi-line), is refused `skill_attaches_files`; an `@` inside a word, as in an email address, is allowed. To point at a bundled file, a skill names it as a path or a Markdown link (`references/a.md`), which the agent reads with `Read` and the hook judges. Other frontmatter fields are accepted, dropped from the session's copy, and listed to the person: "Farik ignores: allowed-tools, hooks".
- **Shipped skills are Farik's own** and are neither pinned nor confirmed.
- **What stays.** A confirmed skill can steer its agent within its own tiers and connectors, in every later session, and its description is listed in each; the reading before adding is the defence against the instructions themselves. Claude Code's own skills stay listed and their calls are denied, which costs turns, not `max_tool_calls`. In no-sandbox mode a command an agent runs can read `daemon.json`'s token and send `skill_save`, whose `skill.added` counts as confirmation (Task 7 adds it to 8.3's warning).

Limits:
- **A skill folder.** `SKILL.md` at most 32 KiB; at most 16 files, `SKILL.md` included, each at most 64 KiB and UTF-8 text with no NUL; 256 KiB in all; a path has at most 3 parts, the file's name included (`a/b/c.md` passes, `a/b/c/d.md` is refused).
  - Each path part matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$`. No part may start with a dot, and no link may appear.
  - The refusals: `skill_too_large`, `skill_too_many_files`, `skill_file_not_text`, `skill_path_invalid`.
- **The frontmatter.** `SKILL.md` must open with a `---` line and close its frontmatter with another; CRLF line ends are read as LF. The frontmatter is a YAML mapping read under ADR 0007's options into a `serde_json::Value`. `name` and `description` must be strings. A missing `SKILL.md`, missing frontmatter, a frontmatter that is not a mapping, or a missing `name` gives `skill_frontmatter_invalid`; a missing `description` gives `skill_description_invalid`. Lengths are counted in Unicode scalar values. The session copy's frontmatter is exactly `---\nname: <name>\ndescription: <description as a JSON string>\n---\n` (JSON strings are valid YAML), followed by the original body byte for byte. `ignored_fields` lists the other keys in file order. Shipped skills keep `parse_skill`.
  - `name` matches `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 64 characters, and equals its folder's (`skill_name_invalid`, `skill_name_mismatch`). `description` has 1 to 1024 characters (`skill_description_invalid`).
- **Counts.** At most 20 team skills and 20 for each agent (the schema's `maxItems`, and `skill_limit_reached` at a save). Names are unique within a list (`skill_name_twice`).

Commands, events and reads:
- **Commands**, named as `command.schema.json` names them, sent through `here_or_sent` from the command line and through the RPC `command` from the browser:
  - `skill_save { level: team | agent, agent?, files, replace_shipped? }` adds a skill, or replaces one of the same name. `files` maps each relative path to its text, at most 256 KiB in all.
  - `skill_remove { level, agent?, name }`. For a `missing` skill it unpins and records `skill.removed`, and does not fail.
  - `skill_confirm { level, agent?, name, sha256, replace_shipped? }`; it writes the pin before the event.

  `save_skill` writes the new folder as `.<name>.new-<random>` beside its place. It renames any old folder to `.<name>.old-<random>`, renames the new one into place, then deletes the old one (`rename(2)` will not replace a non-empty folder). It then writes the pin, then the event. The daemon and the command line's own process both call `save_skill`, `remove_skill` and `confirm_skill`. They refuse `skill_unknown` for a name not pinned there, `skill_hash_mismatch`, `skill_name_taken`, `skill_limit_reached`, `agent_unknown`, and each `SkillRefusal`'s code.
- **Events:**
  - `skill.added { level, agent?, name, sha256 }` and `skill.changed { level, agent?, name, sha256 }`, from a save of a new or an existing name.
  - `skill.removed { level, agent?, name }`.
  - `skill.confirmed { level, agent?, name, sha256 }`.
- **RPCs:**
  - `skills.list { agent }` answers `{ skills: [SkillRow] }`, the rows of `skill_rows`: `{ level: role | team | agent, name, description, state: in_use | replaced | review | missing, bytes }`. Role rows are read-only.
  - `skill.get { level, agent?, name }` answers `{ files, sha256, ignored_fields }`, the files as the folder holds them. A folder that `read_skill_folder` or `check_skill` refuses answers that refusal's code.
- **The command line:**
  - `farik skill list [--agent <id>]`; without `--agent` it lists the team's skills.
  - `farik skill show <name> (--team | --agent <id>)`: every file, escaped by `printable`, and the hash.
  - `farik skill add <folder> (--team | --agent <id>) [--yes] [--replace]` reads and checks the folder in its own process and prints what `show` prints. On a terminal it asks "Add <name> for <whom>? [y/N]"; without a terminal it needs `--yes`. A name in `core_skill_names()` needs `--replace`, refused otherwise with a sentence naming it.
  - `farik skill remove <name> (--team | --agent <id>)`.
  - `farik skill confirm <name> (--team | --agent <id>) <sha256 or its first 12 hex digits> [--replace]`.
  - Removing an agent leaves its folder, which is the user's file, unpinned and unused.

ADR 0034 records the three levels and their order, a shipped skill replaced only on an explicit confirmation, role skills staying in the prompt, the per-session plugin folder outside the project, pinning and confirming on this computer, and the refusal of commands and attached files. It amends ADR 0011's last consequence. It is written in Task 1's commit, because step 05 loads kit skills through it.

For the founder, made by this plan and open to the founder's reversal:
- **O1, role skills stay in the prompt**, and only the team's, an agent's and (step 05) a kit's load on demand. This narrows the project plan's "loaded through Claude Code's skills directories" for the `role.yaml` skills; the reason is above. Recommendation: keep it.

## File map

```
docs/decisions/0034-skills-load-on-demand-and-are-confirmed.md   creates: the ADR (Task 1)
docs/schemas/team.schema.json                                    modifies: skillPin, top-level and per-agent skills (Task 1)
crates/core/src/team.rs, crates/core/src/skill.rs, lib.rs        modifies/creates: SkillPin, skill_sha256 (Task 1)
crates/roles/src/lib.rs, crates/roles/src/skill_check.rs         modifies/creates: check_skill, the limits, core skill names (Task 2)
crates/runtime/src/skills.rs, lib.rs                             creates: reading a folder, confirmations, assembly, the plugin folder (Task 3)
crates/runtime/src/session.rs                                    modifies: SessionSpec.skills (Task 4)
crates/runtime/src/claude.rs                                     modifies: ClaudeConfig.skills_dir, the plugin folder's write and removal, --plugin-dir, Skill, disableSkillShellExecution (Task 4)
crates/runtime/src/orchestrator/session.rs                       modifies: skills into sessions, replaced role skills out of the prompt (Task 4)
crates/runtime/src/daemon.rs, daemon/hooks.rs                    modifies: SessionRegistration.skills, the start's wipe, the Skill check, reads in skills_root (Task 4)
crates/runtime/src/skills.rs                                     modifies: save_skill, remove_skill, confirm_skill, skill_rows (Task 5)
crates/runtime/src/daemon/team.rs                                modifies: the three commands, the two RPCs, team.save and agent.replace keep pins (Task 5)
crates/runtime/src/daemon/templates.rs                           modifies: apply keeps pins (Task 5)
crates/runtime/src/daemon/web.rs                                 modifies: routes the two RPCs (Task 5)
docs/schemas/{event,command,rpc}.schema.json                     modifies: four events, three commands, two RPCs (Task 5)
crates/protocol/src/event.rs, command.rs                         modifies: EventKind, EventBody, Command (Task 5)
crates/cli/src/skill.rs, lib.rs                                  creates/modifies: farik skill (Task 6)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies (Task 7)
```

## Interfaces

Consumes: `canonical_json` (`farik-core`, main), `sha2` (already a `farik-core` dependency; `sha256_hex` is `pub(crate)` and hashes text, not bytes); `load_role`, `RoleDefinition.skills` (`farik-roles`, main); `local_project_id`, `write_private` (`farik-runtime`, step 01 and main); `EventLog` and its query by kind (`farik-store`, main); `SessionSpec`, `ClaudeConfig`, `claude_args`, `settings_json`, `write_session_files`, `judge`, `resolve`, `workspace_paths` (`farik-runtime`, main); `here_or_sent`, `printable` (`farik` cli, main).

Produces:

```rust
// farik-core
pub struct SkillPin { pub name: String, pub sha256: String }
// Team::skills() -> &[SkillPin]; each validated agent's skills: Vec<SkillPin>
pub fn skill_sha256(files: &BTreeMap<String, Vec<u8>>) -> String;
// farik-roles
pub struct CheckedSkill { pub name: String, pub description: String,
    pub ignored_fields: Vec<String>, pub session_files: BTreeMap<String, String> }
    // session_files: SKILL.md with its frontmatter rewritten, the rest unchanged
pub enum SkillRefusal { FrontmatterInvalid, NameInvalid, NameMismatch, DescriptionInvalid,
    RunsCommands, AttachesFiles, TooLarge, TooManyFiles, FileNotText(String), PathInvalid(String) }
impl SkillRefusal { pub fn code(&self) -> &'static str; }  // skill_frontmatter_invalid, …
pub fn check_skill(name: &str, files: &BTreeMap<String, Vec<u8>>) -> Result<CheckedSkill, SkillRefusal>;
pub fn core_skill_names() -> BTreeSet<&'static str>;     // every shipped role.yaml skill
// farik-runtime, skills.rs
pub enum SkillLevel { Team, Agent(String) }
pub fn skill_folder(root: &Path, level: &SkillLevel, name: &str) -> PathBuf;
pub fn read_skill_folder(folder: &Path) -> Result<BTreeMap<String, Vec<u8>>, SkillRefusal>;
pub fn confirmed_skills(events: &[FarikEvent]) -> BTreeMap<(SkillLevel, String), String>;
pub struct SessionSkill { pub name: String, pub files: BTreeMap<String, String> }
pub struct SessionSkills { pub skills: Vec<SessionSkill>, pub replaced_role_skills: BTreeSet<String> }
pub fn session_skills(root: &Path, team: &Team, agent_id: &str,
    confirmed: &BTreeMap<(SkillLevel, String), String>) -> SessionSkills;
pub fn write_plugin(plugin_dir: &Path, skills: &[SessionSkill]) -> io::Result<()>;
pub enum SkillState { InUse, Replaced, Review, Missing }
pub enum SkillRowLevel { Role, Team, Agent }
pub struct SkillRow { pub level: SkillRowLevel, pub name: String,
    pub description: String, pub state: SkillState, pub bytes: u64 }
pub fn skill_rows(root: &Path, team: &Team, agent_id: Option<&str>,
    confirmed: &BTreeMap<(SkillLevel, String), String>) -> Vec<SkillRow>;
pub fn save_skill(root: &Path, log: &EventLog, level: &SkillLevel,
    files: &BTreeMap<String, Vec<u8>>, replace_shipped: bool) -> Result<SkillPin, SkillCommandError>;
pub fn remove_skill(root: &Path, log: &EventLog, level: &SkillLevel, name: &str) -> Result<(), SkillCommandError>;
pub fn confirm_skill(root: &Path, log: &EventLog, level: &SkillLevel, name: &str, sha256: &str,
    replace_shipped: bool) -> Result<(), SkillCommandError>;
pub enum SkillCommandError { Unknown, HashMismatch, NameTaken, LimitReached, AgentUnknown,
    Refused(SkillRefusal), Io(io::Error) }
impl SkillCommandError { pub fn code(&self) -> &'static str; }  // skill_unknown, …, the refusal's code
```

`SessionSpec` gains `skills: Vec<SessionSkill>`. `ClaudeConfig` gains `skills_dir: PathBuf` (`<state>/skills/<local project id>`). `write_session_files` writes `<skills_dir>/<session id>/` when `spec.skills` is not empty. `claude_args` adds `--plugin-dir <that folder>` and `Skill` to `--tools` when it is not empty. The adapter owns the folder, as it owns `system-prompt.md` and `mcp.json`. `SessionRegistration` gains `skills: Vec<String>` and `skills_root: Option<PathBuf>` (the folder's `skills/`).

Wire (`snake_case`): the team file's `skills: [{ name, sha256 }]`, top level and per agent; events `skill.added`, `skill.changed`, `skill.removed`, `skill.confirmed`; commands `skill_save`, `skill_remove`, `skill_confirm`; RPCs `skills.list`, `skill.get`.

## Tasks

### Task 1: Pins in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, `crates/core/src/skill.rs`, `lib.rs`, ADR 0034. Produces `SkillPin`, `skill_sha256`.

- `accepts_team_and_agent_skill_pins`: a top-level `skills` and an agent's `skills` validate and read back equal.
- `refuses_a_bad_pin`: a name `Bad_Name` and a `sha256` of 63 hex digits are each a schema error at their field; one name twice in a list is `skill_name_twice` at the second; a 21st item is a schema error.
- `the_skill_hash_sees_every_file_and_ignores_order`: the same files inserted in two orders give one hash; one changed byte, one renamed file, or one added file each changes it.

- [x] `feat(core): pin a team's and an agent's skills in the team file`

### Task 2: Checking a skill

Files: `crates/roles/src/skill_check.rs`, `crates/roles/src/lib.rs`. Produces `check_skill`, `CheckedSkill`, `SkillRefusal`, `core_skill_names`.

- `accepts_a_skill_with_a_reference`: `SKILL.md` and `references/a.md` give `name`, `description`, and both files in `session_files`.
- `rewrites_the_frontmatter_to_name_and_description`: a `SKILL.md` with `allowed-tools`, `hooks` and `model` gives a session copy whose frontmatter is exactly `name` and `description`, a body unchanged byte for byte, and `ignored_fields` `["allowed-tools", "hooks", "model"]`; a description holding `: ` and a newline reads back equal from the session copy.
- `refuses_a_skill_without_a_frontmatter`: no `SKILL.md`, no opening `---`, and a frontmatter `[1, 2]` each give `FrontmatterInvalid`; a CRLF file passes.
- `refuses_a_skill_that_runs_commands`: `` !`ls` `` in the body, and a block opened by ```` ```! ````, each give `RunsCommands`.
- `refuses_a_skill_that_attaches_files`: `See @~/.ssh/id_rsa` and a line starting `@references/a.md` each give `AttachesFiles`; `mail ana@example.com` passes.
- `refuses_names_and_descriptions_out_of_bounds`: `Has_Upper` gives `NameInvalid`; a frontmatter naming another folder gives `NameMismatch`; empty and 1025 characters each give `DescriptionInvalid`.
- `refuses_folders_out_of_bounds`: a 33 KiB `SKILL.md`, a 65 KiB file, 257 KiB in all, and 17 files each give `TooLarge` or `TooManyFiles`; a file holding NUL gives `FileNotText`; `.hidden`, `a/b/c/d.md` and `../x` each give `PathInvalid`; `a/b/c.md` passes.
- `every_shipped_role_skill_passes`: each `role.yaml` skill passes `check_skill`, and `core_skill_names` equals the set of skill names gathered from `load_role` over every shipped role (not a literal count).

- [x] `feat(roles): check a skill against the Agent Skills format and Farik's limits`

### Task 3: Reading, confirming and assembling skills

Files: `crates/runtime/src/skills.rs`, `lib.rs`. Produces `SkillLevel`, `skill_folder`, `read_skill_folder`, `confirmed_skills`, `SessionSkill`, `SessionSkills`, `session_skills`, `write_plugin`. `session_skills` hashes and copies the same bytes, read once, so a folder changed between the check and the copy cannot slip in.

- `reads_a_folder_and_refuses_a_link`: a folder with a symlink gives `PathInvalid`; a 70 KiB file is refused before it is read whole (the read stops at the limit).
- `the_newest_confirmation_wins_and_removal_clears`: `skill.added` then `skill.changed` with another hash gives the second; a later `skill.removed` gives none.
- `a_skill_loads_only_when_pin_folder_and_log_agree`: three cases each leave the skill out. A folder edited after confirming. A pin and folder changed together with no event, as a pull would. A pin whose folder is gone.
- `an_agents_skill_replaces_the_teams`: both named `api-style` give one session skill, the agent's; when the agent's is in review, none.
- `a_confirmed_replacement_names_the_role_skill`: an `in_use` team skill `implementing-a-contract` is in `skills` and in `replaced_role_skills`; in review, neither.
- `writes_the_plugin_folder`: writes `<dir>/.claude-plugin/plugin.json` as `{"name":"farik"}` and `<dir>/skills/<name>/SKILL.md` as the session copy; the folder is mode 0700; writing twice replaces the first copy whole.

- [x] `feat(runtime): assemble an agent's confirmed skills for a session`

### Task 4: Skills in sessions

Files: `session.rs`, `claude.rs`, `orchestrator/session.rs`, `daemon.rs`, `daemon/hooks.rs`.

- `a_session_with_skills_gets_the_plugin_and_skill`: `claude_args` holds `--plugin-dir <skills_dir>/<session id>` and `Skill` in `--tools`, and the folder is gone after the session ends. Without skills, neither appears and no folder is written.
- `the_daemon_start_wipes_old_plugin_folders`: a folder left under `<skills_dir>` is gone after the daemon starts.
- `settings_turn_off_skill_commands`: `settings_json` holds `"disableSkillShellExecution": true` for every session.
- `one_tool_sessions_get_no_skills`: triage, the judgment and the design-plan decision have no `--plugin-dir`; a task session, a conversation, a ceremony and a chat have it.
- `the_prompt_still_carries_the_role_skills_alone`: `system-prompt.md` holds `### Skill: implementing-a-contract` and not the agent skill's body.
- `a_replaced_role_skill_leaves_the_prompt`: with an `in_use` team `writing-task-contracts`, the Product Manager's conversation prompt lacks `### Skill: writing-task-contracts` and its plugin holds it; its triage prompt keeps it.
- `the_hook_allows_only_the_sessions_skills`: `{ skill: "farik:api-style" }` and `{ skill: "farik:api-style", args: "x" }` are allowed and recorded `tool.called`. `{ skill: "api-style" }`, `{ skill: "deep-research" }`, `{ skill: "farik:other" }`, `{ skill: 3 }`, `{}` and `{ skill: "farik:api-style", extra: 1 }` are each denied `skill_not_in_session`. At `max_tool_calls`, an allowed name is denied `tool_call_limit`.
- `reads_reach_the_skill_folder_and_writes_do_not`: `Read` of `<skills_root>/api-style/references/a.md` is allowed; `Write` there, and `Read` of `<session dir>/mcp.json`, are denied `path_outside_workspace`. The same holds for a `chat` session registered with cwd = the project root.
- `a_live_session_loads_a_skill_on_use` (integration, `--integration`): see Verification.

- [ ] `feat(runtime): load an agent's skills into its sessions on demand`

### Task 5: Saving, removing and confirming

Files: `skills.rs` (`save_skill`, `remove_skill`, `confirm_skill`, `skill_rows`, `SkillCommandError`), `daemon/team.rs`, `daemon/templates.rs`, `daemon/web.rs`, the event, command and RPC schemas, `protocol/src/event.rs`, `command.rs`. `skillSaveBody`, `skillRemoveBody` and `skillConfirmBody` each set `additionalProperties: false`, so each matches exactly one branch of `commandBodyWire`'s `oneOf`.

- `save_writes_folder_pin_and_event`: `skill_save` for an agent writes `.farik/agents/theo/skills/api-style/`, pins it in `team.yaml` with `skill_sha256`, and records `skill.added`; no `.api-style.new-*` or `.old-*` folder remains. Saving it again changed records `skill.changed` with the new hash.
- `save_refuses_what_check_skill_refuses`: `skill_runs_commands`, with nothing written; the 21st team skill is refused `skill_limit_reached`; `writing-task-contracts` is refused `skill_name_taken` without `replace_shipped` and saved with it.
- `remove_deletes_folder_pin_and_records`: `skill.removed`; the team's skill of the same name is untouched. Removing a `missing` skill unpins it and records `skill.removed`.
- `confirm_needs_the_folders_hash`: after the folder is changed outside Farik, `skill_confirm` with the old hash is `skill_hash_mismatch`; with the new hash, it rewrites the pin to the new hash, records `skill.confirmed`, and `skills.list` gives `in_use`. A confirm of a folder that holds `` !` `` is refused `skill_runs_commands` with nothing written.
- `skills_list_gives_every_level_and_state`: one row each of `role`, role `replaced`, `team` `replaced` (by an agent skill in review), `agent`, `review` and `missing`, with `bytes`.
- `skill_get_answers_the_files_and_ignored_fields`: for a skill with `references/a.md` and `allowed-tools` in its frontmatter, it answers both files' text as saved, `sha256` equal to `skill_sha256`, and `ignored_fields` `["allowed-tools"]`; for a folder holding `` !` `` it answers `skill_runs_commands`.
- `whole_team_saves_keep_pins`: a `team.save` whose team carries no `skills`, or different ones, leaves both levels' pins as they were; so does `agent.replace`; `template.apply` keeps the team's pins and a kept agent's.

- [ ] `feat(runtime): save, remove and confirm a skill`

### Task 6: The command line

Files: `cli/src/skill.rs`, `cli/src/lib.rs`. Each change goes through `here_or_sent`, whose `here` calls `save_skill`, `remove_skill` or `confirm_skill`.

- `farik_skill_add_shows_then_asks`: on a terminal answering `n`, nothing is sent; answering `y` sends `skill_save`. The text printed before the question holds the whole `SKILL.md` and the hash.
- `farik_skill_add_needs_yes_without_a_terminal`: refused with a sentence naming `--yes`; a shipped name without `--replace` is refused with a sentence naming `--replace`.
- `farik_skill_confirm_takes_a_prefix`: the first 12 hex digits of the folder's hash confirm; 11 digits, or a prefix of another hash, are refused.
- `farik_skill_list_shows_levels_and_states`: with `--agent theo` it prints one line per row in `skill_rows` order, `<level>  <name>  <state>`, the states as `in use`, `replaced`, `review`, `missing`; without `--agent` it prints the team's rows alone.

- [ ] `feat(cli): add, show, remove and confirm a skill`

### Task 7: Spec and plan

`docs/SPEC.md`: 6 (the three levels and their order, role skills in the prompt, a shipped skill replaced only on confirmation), 6.7 (adding, confirming, limits), 8.2 (the plugin folder outside the project, `Skill` in `--tools`, `disableSkillShellExecution`, the hook's two checks), 8.3 (add "add skills" to the no-sandbox warning's list, "approve, accept, answer, and integrate", since a command can read `daemon.json`'s token and send `skill_save`), 8.5 (the four events), 8.6 (skills as instructions, confirmed on this computer, no commands and no `@` file attachments, and the no-sandbox residual), F9 (the commands and RPCs). `docs/plans/project-plan.md`: phase 7's row 04 and its skills decision bullet, corrected if execution changed them. `docs/design/role-kits.md`: its steps table.

- [ ] `docs(spec): record skills per agent`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with a_live_session_loads_a_skill_on_use passed
```

`a_live_session_loads_a_skill_on_use` (Task 4) runs a real Claude Code session as a `chat` session (cwd the project root, the harder case) with an agent skill `fixture-skill` and its file `references/note.md`. The human's message tells it to use the skill, read the note, and end.
- The stream's `system/init` line lists `farik:fixture-skill` among its skills.
- The log has `tool.called` for `Skill` and for `Read` of the note.
- `system-prompt.md` lacks the skill's body.

The input shape is pinned in Decisions; the live test confirms it once more. Optional: the readiness review's fake Messages API (`ANTHROPIC_BASE_URL` at a local server streaming scripted `tool_use` blocks) would make this test deterministic and free of a credential.
