# Phase 7, step 04: Skills per agent

Status: draft
Branch: `phase/7-role-kits`
Spec: `docs/SPEC.md` 6, 6.7, 8.2, 8.5, 8.6; F9
Depends on: step 01 of this phase (committed; `canonical_json`, `here_or_sent`'s use for a team-file change, the agent page's sections), phase 6 (merged in #19)
Readiness: pending: a readiness review by another Opus session
Mockups approved by: pending (Task 1's gate)

## Goal

Today each role's skills are written whole into every session's prompt (ADR 0011), and the user cannot add one. When this step is done, the user can give one agent, or the whole team, a skill in the Agent Skills format, a folder with a `SKILL.md` and the files it refers to, from the agent page or with `farik skill add`. They read it before it is added, and edit or remove it later. A session sees each such skill's name and description; it loads the body when it uses the skill, and a referenced file when it reads it, through Claude Code's own skills mechanism. A skill that arrived in the project by a clone, a pull or a hand edit is used only after the user has read and confirmed it on this computer. Out of scope: a kit's skills, which step 05 loads through this step's folder; a saved team template carrying skills; skills that run scripts.

## Decisions

Research (2026-10-02; code.claude.com/docs/en/skills and /plugins, undated, read that day; agentskills.io/specification; a probe recorded in this repository, `crates/runtime/src/recorded/transcripts/credential_refused.jsonl`, Claude Code 2.1.285):
- **`--setting-sources ""` turns off the user's and the project's `.claude/skills`.** A plugin folder given with `--plugin-dir` still loads, in `-p` mode, with no marketplace; it needs only `.claude-plugin/plugin.json`. Its skills are named `<plugin>:<skill>`.
- **Skills are called through the built-in tool `Skill`**, which a `PreToolUse` hook sees. A referenced file is read with `Read`, by its path in the skill's folder.
- **Claude Code adds frontmatter fields that change behaviour:** `allowed-tools`, `hooks`, `context`, `agent`, `model`, `effort`, `paths`, `disable-model-invocation`. A body may also hold `` !`command` ``, which runs a shell command when the skill loads.
- **Claude Code ships skills of its own**, `deep-research`, `update-config` and others, listed even with `--setting-sources ""` (the probe's `skills`).
- **The specification's limits:** a name of at most 64 characters, lower-case letters, digits and single hyphens, equal to its folder's; a description of at most 1024 characters; a body of under 500 lines recommended.

Where skills live:
- **Three levels, as the project plan decided.**
  - The role's, embedded in the binary: `crates/roles/roles/<role>/skills/<name>/SKILL.md`.
  - The team's: `.farik/skills/<name>/`.
  - One agent's: `.farik/agents/<agent_id>/skills/<name>/`.

  All are read from the project's root, never a task's worktree, as `team.yaml` is. Agents cannot write `.farik/` (5.3).
- **The role's `role.yaml` skills stay in the prompt** (ADR 0011), in every session of the role. The sessions given one Farik tool alone (triage, the judgment, the design-plan decision) have no `Skill` tool, and the Product Manager's contract writing is its job in triage. Rejected: moving them to load on demand, which would take them from those sessions. Step 05's kit skills, and the team's and an agent's, load on demand.
- **One name, the most specific wins.** An agent's skill replaces the team's of the same name, and the page says so. A team or agent skill may not take the name of any shipped `role.yaml` skill (`skill_name_taken`), which is already in the prompt.

Loading into a session:
- **A plugin folder per session.** `run_session` writes `.farik/local/sessions/<id>/plugin/`, with `.claude-plugin/plugin.json` `{"name":"farik"}` and `skills/<name>/` holding a copy of each skill. It passes `--plugin-dir <that folder>`, and adds `Skill` to `--tools`, only when the session has at least one skill. Each copied `SKILL.md`'s frontmatter is rewritten to `name` and `description` alone, so no Claude Code field reaches the session. Rejected: `.claude/skills` in the worktree, which `--setting-sources ""` turns off and which an agent could commit. Rejected: writing every skill into the prompt, which ADR 0011 already names as the cost to revisit.
- **Which sessions.** Every session not given one Farik tool alone. That is the task sessions, `conversation`, `ceremony` and `chat`.
- **The hook.** A `Skill` call is allowed only when its input's `skill` is a string naming one of the session's skills, as `farik:<name>` or `<name>`. Anything else, Claude Code's own skills among it, is denied `skill_not_in_session`. It counts towards `max_tool_calls`. `Read`, `Glob` and `Grep` may reach the session's `plugin/skills/` folder as well as the worktree; every write there stays `path_outside_workspace`.
- **The prompt** gains nothing. Claude Code lists the skills itself.

Trust:
- **A skill is instructions an agent follows**, so the user reads all of it before it is used. Adding or editing one shows the whole `SKILL.md` and each file's name and size first, and "Add skill" is the confirmation.
- **Pinned in the team file.** `team.yaml` gains `skills: [{ name, sha256 }]` at the top level for the team, and on each agent for that agent. `sha256` is `skill_sha256`: the sha256 of `canonical_json` of `{ <relative path>: <sha256 hex of the file's bytes> }` over every file in the folder.
- **Confirmed on this computer**, as ADR 0030 does for a connector: a skill loads only when three things agree. Its folder hashes to its pin. The newest `skill.added`, `skill.changed` or `skill.confirmed` for it in this machine's log (8.4, never committed) carries that hash. No `skill.removed` follows that event.
  - Otherwise it loads nothing. The page shows "Review before <agent> uses it", or "Missing" when the pinned folder is gone.
  - `skill_confirm`, with the hash the person saw, confirms one. Why: `.farik/` travels with the repository, so a pull could change a skill's instructions and its pin together.
- **No commands.** A `SKILL.md` holding `` !` `` anywhere is refused `skill_runs_commands`. Other frontmatter fields are accepted, dropped from the session's copy, and listed to the person: "Farik ignores: allowed-tools, hooks".
- **Shipped skills are Farik's own** and are neither pinned nor confirmed.

Limits:
- **A skill folder.** `SKILL.md` at most 32 KiB; at most 16 files, each at most 64 KiB and UTF-8 text with no NUL; 256 KiB in all; at most 3 folders deep.
  - Each path part matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$`. No part may start with a dot, and no link may appear.
  - The refusals: `skill_too_large`, `skill_too_many_files`, `skill_file_not_text`, `skill_path_invalid`.
- **The frontmatter.** `name` matches `^[a-z0-9]+(-[a-z0-9]+)*$`, at most 64 characters, and equals its folder's (`skill_name_invalid`, `skill_name_mismatch`). `description` has 1 to 1024 characters (`skill_description_invalid`).
- **Counts.** At most 20 team skills and 20 for each agent (the schema's `maxItems`, and `skill_limit_reached` at a save). Names are unique within a list (`skill_name_twice`).

Commands, events and reads:
- **Commands**, named as `command.schema.json` names them, sent through `here_or_sent` from the command line and through the RPC `command` from the browser:
  - `skill_save { level: team | agent, agent?, files }` adds a skill, or replaces one of the same name. `files` maps each relative path to its text, at most 256 KiB in all.
  - `skill_remove { level, agent?, name }`.
  - `skill_confirm { level, agent?, name, sha256 }`.

  The daemon writes a folder beside its place and renames it over, then the pin, then the event. It refuses `skill_unknown` for a name not pinned there, `skill_hash_mismatch` when `skill_confirm`'s hash is not the folder's, and `agent_unknown`.
- **Events:**
  - `skill.added { level, agent?, name, sha256 }` and `skill.changed { level, agent?, name, sha256 }`, from a save of a new or an existing name.
  - `skill.removed { level, agent?, name }`.
  - `skill.confirmed { level, agent?, name, sha256 }`.
- **RPCs:**
  - `skills.list { agent }` answers `{ skills: [{ level: role | team | agent, name, description, state: in_use | replaced | review | missing, bytes }] }`. The role's own skills are `level: role`, `in_use`, and read-only. `replaced` is a team skill the agent's own of the same name replaces.
  - `skill.get { level, agent?, name }` answers `{ files, sha256, ignored_fields }`, the files as the folder holds them, for editing and reviewing.
- **The command line:**
  - `farik skill list [--agent <id>]`.
  - `farik skill show <name> (--team | --agent <id>)`: every file, escaped by `printable`, and the hash.
  - `farik skill add <folder> (--team | --agent <id>) [--yes]` reads and checks the folder in its own process and prints what `show` prints. On a terminal it asks "Add <name> for <whom>? [y/N]"; without a terminal it needs `--yes`.
  - `farik skill remove <name> (--team | --agent <id>)`.
  - `farik skill confirm <name> (--team | --agent <id>) <sha256 or its first 12 hex digits>`.
  - Removing an agent leaves its folder, which is the user's file, unpinned and unused.

What a non-technical user sees:
- **The agent page's new Skills section** lists three groups: "Comes with <role>" (read-only, "Read"), "For the whole team" and "Just for <name>", each row with its description, "Edit" and "Remove". A row to review says "Changed in the project. Review before <name> uses it", with "Review".
- **"Add a skill"** opens `SkillEdit`:
  - who it is for ("Just <name>" or "Everyone on the team");
  - a name;
  - "When should <name> use it?" for the description;
  - the instructions, or "Upload a SKILL.md".

  Other files of a skill being edited are listed by name and size and kept as they are. A folder with several files is added with `farik skill add`. Save shows the whole text with "Farik will follow these instructions. Read them before adding." and "Add skill".
- **`SkillReview`** shows a skill from the project: its whole text in an `untrusted` frame, its files, "Use this skill" (`skill_confirm`), and "Remove".

ADR 0034 records the three levels and their order, role skills staying in the prompt, the per-session plugin folder, pinning and confirming on this computer, and the refusal of commands. It amends ADR 0011's last consequence. It is written in Task 2's commit, because step 05 loads kit skills through it.

For the founder, made by this plan and open to the founder's reversal:
- **O1, role skills stay in the prompt**, and only the team's, an agent's and (step 05) a kit's load on demand. This narrows the project plan's "loaded through Claude Code's skills directories" for the `role.yaml` skills; the reason is above. Recommendation: keep it.
- **O2, the mockups.** The founder approves Task 1's boards, or says to approve them automatically. Task 8 does not start until then.

## File map

```
docs/design/mockups/{AgentEdit,SkillEdit,SkillReview}.dc.html, canvas.json   Task 1
docs/decisions/0034-skills-load-on-demand-and-are-confirmed.md   creates: the ADR (Task 2)
docs/schemas/team.schema.json                                    modifies: skillPin, top-level and per-agent skills (Task 2)
crates/core/src/team.rs, crates/core/src/skill.rs, lib.rs        modifies/creates: SkillPin, skill_sha256 (Task 2)
crates/roles/src/lib.rs, crates/roles/src/skill_check.rs         modifies/creates: check_skill, the limits, core skill names (Task 3)
crates/runtime/src/skills.rs, lib.rs                             creates: reading a folder, confirmations, the plugin folder (Task 4)
crates/runtime/src/session.rs, claude.rs                         modifies: SessionSpec.skills, --plugin-dir, Skill (Task 5)
crates/runtime/src/orchestrator/session.rs                       modifies: skills into sessions (Task 5)
crates/runtime/src/daemon.rs, daemon/hooks.rs                    modifies: SessionRegistration.skills, the Skill check, reads in plugin/skills (Task 5)
crates/runtime/src/daemon/team.rs                                modifies: the three commands, the two RPCs (Task 6)
crates/runtime/src/daemon/web.rs                                 modifies: routes the two RPCs (Task 6)
docs/schemas/{event,command,rpc}.schema.json                     modifies: four events, three commands, two RPCs (Task 6)
crates/protocol/src/event.rs, command.rs                         modifies: EventKind, EventBody, Command (Task 6)
crates/cli/src/skill.rs, lib.rs                                  creates/modifies: farik skill (Task 7)
packages/protocol-client/src/mapping.ts                          modifies (Task 8)
apps/web/src/pages/AgentEdit.tsx, dialogs/{SkillEdit,SkillReview}.tsx, skills.test.tsx, strings/en.ts   (Task 8)
docs/SPEC.md, docs/plans/project-plan.md, docs/design/role-kits.md   modifies (Task 9)
```

## Interfaces

Consumes: `canonical_json`, `sha256_hex` (`farik-core`, step 01 and main); `parse_skill`'s frontmatter reading, `load_role`, `RoleDefinition.skills` (`farik-roles`, main); `local_project_id`, `write_private` (`farik-runtime`, step 01 and main); the log's query by kind (`farik-store`, main); `SessionSpec`, `claude_args`, `write_session_files`, `decide_pre_tool_use`, `workspace_paths` (`farik-runtime`, main); `here_or_sent`, `printable` (`farik` cli, main).

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
pub enum SkillRefusal { NameInvalid, NameMismatch, NameTaken, DescriptionInvalid, RunsCommands,
    TooLarge, TooManyFiles, FileNotText(String), PathInvalid(String) }
impl SkillRefusal { pub fn code(&self) -> &'static str; }  // skill_name_invalid, …
pub fn check_skill(name: &str, files: &BTreeMap<String, Vec<u8>>) -> Result<CheckedSkill, SkillRefusal>;
pub fn core_skill_names() -> BTreeSet<&'static str>;     // every shipped role.yaml skill
// farik-runtime, skills.rs
pub enum SkillLevel { Team, Agent(String) }
pub fn skill_folder(root: &Path, level: &SkillLevel, name: &str) -> PathBuf;
pub fn read_skill_folder(folder: &Path) -> Result<BTreeMap<String, Vec<u8>>, SkillRefusal>;
pub fn confirmed_skills(events: &[FarikEvent]) -> BTreeMap<(SkillLevel, String), String>;
pub struct SessionSkill { pub name: String, pub files: BTreeMap<String, String> }
pub fn session_skills(root: &Path, team: &Team, agent_id: &str,
    confirmed: &BTreeMap<(SkillLevel, String), String>) -> Vec<SessionSkill>;
pub fn write_plugin(session_dir: &Path, skills: &[SessionSkill]) -> io::Result<Option<PathBuf>>;
    // None when there are no skills
// SessionSpec gains `skills_plugin: Option<PathBuf>`, `skills: Vec<String>`;
// SessionRegistration gains `skills: Vec<String>`, `skills_root: Option<PathBuf>`
```

Wire (`snake_case`): the team file's `skills: [{ name, sha256 }]`, top level and per agent; events `skill.added`, `skill.changed`, `skill.removed`, `skill.confirmed`; commands `skill_save`, `skill_remove`, `skill_confirm`; RPCs `skills.list`, `skill.get`.

## Tasks

### Task 1: The skill screens, mocked up

A Sonnet agent draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, page "Team and settings"), each at desktop and phone width, in the canvas's tokens, muted and light, one colour per job. They are copied into `docs/design/mockups/`.

- **`AgentEdit`, the Skills section**, below Connectors, for Theo, a Software Developer:
  - "Comes with Software Developer": `implementing-a-contract`, its description, "Read".
  - "For the whole team": `release-notes`, "Write release notes in our voice", with "Edit" and "Remove"; and `api-style`, with the muted note "Theo's own api-style replaces this one for Theo".
  - "Just for Theo": `api-style`, with "Edit" and "Remove".
  - A row to review: `deploy-checklist`, "Changed in the project. Review before Theo uses it", with "Review".
  - A missing row: "`old-skill` is in the team file but its folder is gone", with "Remove".
  - The button "Add a skill".
  - Remove's confirmation: "Remove api-style? Theo stops using it, and its folder is deleted from the project."
- **`SkillEdit`**, a dialog in three states:
  - **Adding.** Who it is for, two choices: "Just Theo" and "Everyone on the team". Then "Name", with the hint "lower-case words joined by hyphens". Then "When should Theo use it?", with "1024 characters at most". Then "Instructions", a tall monospace textarea, with the link "Upload a SKILL.md instead". Buttons: "Next" and "Cancel".
  - **Editing a skill with more files.** As above, filled in, with "Also in this skill: references/checklist.md, 2 KB; templates/note.md, 1 KB. Farik keeps them as they are."
  - **Reading before adding.** "Farik will follow these instructions. Read them before adding." Then the whole SKILL.md in a scrolling frame. Then, when there are ignored fields, the muted line "Farik ignores: allowed-tools, hooks". Then "Add skill" and "Back".
  - **Refusals, under their field:** "This skill runs commands when it loads, which Farik doesn't allow." and "Instructions are limited to 32 KB."
- **`SkillReview`**, a dialog: the title "Review deploy-checklist", and "It came with the project, by a clone, a pull or an edit, and Theo won't use it until you've read it." Below that, the whole text in the `untrusted` frame the `ToolApproval` dialog uses, then the file list with sizes, then "Use this skill" and "Remove".

Gate (O2): the founder approves the boards, or says to approve them automatically, and the approval is written into this plan's header with its date. Task 8 does not start until then; Tasks 2 to 7 do not depend on the boards.

- [ ] `docs(design): mock up skills per agent`

### Task 2: Pins in the team file

Files: `team.schema.json`, `crates/core/src/team.rs`, `crates/core/src/skill.rs`, `lib.rs`, ADR 0034. Produces `SkillPin`, `skill_sha256`.

- `accepts_team_and_agent_skill_pins`: a top-level `skills` and an agent's `skills` validate and read back equal.
- `refuses_a_bad_pin`: a name `Bad_Name` and a `sha256` of 63 hex digits are each a schema error at their field; one name twice in a list is `skill_name_twice` at the second; a 21st item is a schema error.
- `the_skill_hash_sees_every_file_and_ignores_order`: the same files inserted in two orders give one hash; one changed byte, one renamed file, or one added file each changes it.

- [ ] `feat(core): pin a team's and an agent's skills in the team file`

### Task 3: Checking a skill

Files: `crates/roles/src/skill_check.rs`, `crates/roles/src/lib.rs`. Produces `check_skill`, `CheckedSkill`, `SkillRefusal`, `core_skill_names`.

- `accepts_a_skill_with_a_reference`: `SKILL.md` and `references/a.md` give `name`, `description`, and both files in `session_files`.
- `rewrites_the_frontmatter_to_name_and_description`: a `SKILL.md` with `allowed-tools`, `hooks` and `model` gives a session copy whose frontmatter is exactly `name` and `description`, a body unchanged, and `ignored_fields` `["allowed-tools", "hooks", "model"]`.
- `refuses_a_skill_that_runs_commands`: `` !`ls` `` in the body gives `RunsCommands`.
- `refuses_names_and_descriptions_out_of_bounds`: `Has_Upper` gives `NameInvalid`; a frontmatter naming another folder gives `NameMismatch`; `writing-task-contracts` gives `NameTaken`; empty and 1025 characters each give `DescriptionInvalid`.
- `refuses_folders_out_of_bounds`: a 33 KiB `SKILL.md`, a 65 KiB file, 257 KiB in all, and 17 files each give `TooLarge` or `TooManyFiles`; a file holding NUL gives `FileNotText`; `.hidden`, `a/b/c/d.md` and `../x` each give `PathInvalid`.
- `every_shipped_role_skill_passes`: each `role.yaml` skill passes `check_skill`, and `core_skill_names` holds all eleven.

- [ ] `feat(roles): check a skill against the Agent Skills format and Farik's limits`

### Task 4: Reading, confirming and assembling skills

Files: `crates/runtime/src/skills.rs`, `lib.rs`. Produces `SkillLevel`, `skill_folder`, `read_skill_folder`, `confirmed_skills`, `SessionSkill`, `session_skills`, `write_plugin`.

- `reads_a_folder_and_refuses_a_link`: a folder with a symlink gives `PathInvalid`; a 70 KiB file is refused before it is read whole (the read stops at the limit).
- `the_newest_confirmation_wins_and_removal_clears`: `skill.added` then `skill.changed` with another hash gives the second; a later `skill.removed` gives none.
- `a_skill_loads_only_when_pin_folder_and_log_agree`: three cases each leave the skill out. A folder edited after confirming. A pin and folder changed together with no event, as a pull would. A pin whose folder is gone.
- `an_agents_skill_replaces_the_teams`: both named `api-style` give one session skill, the agent's.
- `writes_the_plugin_folder`: `plugin/.claude-plugin/plugin.json` is `{"name":"farik"}`, and `plugin/skills/<name>/SKILL.md` is the session copy. No skills gives `None` and no folder.

- [ ] `feat(runtime): assemble an agent's confirmed skills for a session`

### Task 5: Skills in sessions

Files: `session.rs`, `claude.rs`, `orchestrator/session.rs`, `daemon.rs`, `daemon/hooks.rs`.

- `a_session_with_skills_gets_the_plugin_and_skill`: `claude_args` holds `--plugin-dir <session dir>/plugin` and `Skill` in `--tools`. Without skills, neither appears.
- `one_tool_sessions_get_no_skills`: triage, the judgment and the design-plan decision have no `--plugin-dir`; a task session, a conversation, a ceremony and a chat have it.
- `the_prompt_still_carries_the_role_skills_alone`: `system-prompt.md` holds `### Skill: implementing-a-contract` and not the agent skill's body.
- `the_hook_allows_only_the_sessions_skills`: `{ skill: "farik:api-style" }` and `{ skill: "api-style" }` are allowed and recorded `tool.called`. `{ skill: "deep-research" }`, `{ skill: 3 }` and `{}` are each denied `skill_not_in_session`.
- `reads_reach_the_skill_folder_and_writes_do_not`: `Read` of `<plugin>/skills/api-style/references/a.md` is allowed; `Write` there, and `Read` of `<session dir>/mcp.json`, are denied `path_outside_workspace`.
- `a_live_session_loads_a_skill_on_use` (integration, `--integration`): see Verification.

- [ ] `feat(runtime): load an agent's skills into its sessions on demand`

### Task 6: Saving, removing and confirming through the daemon

Files: `daemon/team.rs`, `daemon/web.rs`, the event, command and RPC schemas, `protocol/src/event.rs`, `command.rs`.

- `save_writes_folder_pin_and_event`: `skill_save` for an agent writes `.farik/agents/theo/skills/api-style/`, pins it in `team.yaml` with `skill_sha256`, and records `skill.added`. Saving it again changed records `skill.changed` with the new hash.
- `save_refuses_what_check_skill_refuses`: `skill_runs_commands`, with nothing written; and the 21st team skill is refused `skill_limit_reached`.
- `remove_deletes_folder_pin_and_records`: `skill.removed`; the team's skill of the same name is untouched.
- `confirm_needs_the_folders_hash`: after the folder is changed outside Farik, `skill_confirm` with the old hash is `skill_hash_mismatch`; with the new hash, it records `skill.confirmed`, and the skill is `in_use`.
- `skills_list_gives_every_level_and_state`: one row each of `role`, `team` `replaced`, `agent`, `review` and `missing`.
- `skill_get_answers_the_files_and_ignored_fields`.

- [ ] `feat(runtime): save, remove and confirm a skill`

### Task 7: The command line

Files: `cli/src/skill.rs`, `cli/src/lib.rs`.

- `farik_skill_add_shows_then_asks`: on a terminal answering `n`, nothing is sent; answering `y` sends `skill_save`. The text printed before the question holds the whole `SKILL.md` and the hash.
- `farik_skill_add_needs_yes_without_a_terminal`: refused with a sentence naming `--yes`.
- `farik_skill_confirm_takes_a_prefix`: the first 12 hex digits of the folder's hash confirm; 11 digits, or a prefix of another hash, are refused.
- `farik_skill_list_shows_levels_and_states`.

- [ ] `feat(cli): add, show, remove and confirm a skill`

### Task 8: The screens

Files: `AgentEdit.tsx`, `dialogs/SkillEdit.tsx`, `dialogs/SkillReview.tsx`, `skills.test.tsx`, `strings/en.ts`, `mapping.ts`. Built from Task 1's approved boards.

- `agent_edit_lists_skills_by_level`: the three group headings, the replaced note, and the review and missing rows.
- `skill_edit_shows_the_whole_text_before_adding`: "Add skill" appears only on the reading step, and sends `skill_save` with every file, the untouched ones included.
- `skill_edit_says_refusals_at_their_field`: `skill_runs_commands` and `skill_too_large`.
- `skill_review_confirms_with_the_hash_it_showed`: "Use this skill" sends `skill_confirm` with `skill.get`'s `sha256`.
- `skill_review_renders_markup_as_text`.

- [ ] `feat(web): skills on the agent page`

### Task 9: Spec and plan

`docs/SPEC.md`: 6 (the three levels, role skills in the prompt), 6.7 (adding, confirming, limits), 8.2 (the plugin folder, `Skill` in `--tools`, the hook's two checks), 8.5 (the four events), 8.6 (skills as instructions, confirmed on this computer, no commands), F9 (the commands and RPCs). `docs/plans/project-plan.md`: phase 7's row 04 and its skills decision bullet, corrected if execution changed them. `docs/design/role-kits.md`: its steps table.

- [ ] `docs(spec): record skills per agent`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with a_live_session_loads_a_skill_on_use passed
```

`a_live_session_loads_a_skill_on_use` (Task 5) runs a real Claude Code session with an agent skill `fixture-skill` and its file `references/note.md`. The human's message tells it to use the skill, read the note, and end.
- The stream's `system/init` line lists `farik:fixture-skill` among its skills.
- The log has `tool.called` for `Skill` and for `Read` of the note.
- `system-prompt.md` lacks the skill's body.

The test also pins the `Skill` input's field name, and the hook refuses any other shape.
