# 0034. Skills load on demand and are confirmed on this computer

Date: 2026-10-02
Status: accepted (phase 7 step 04, readiness-reviewed 2026-10-02)

Amends ADR 0011's last consequence: the skills folder with `--plugin-dir` is now used, for the skills a user or a kit adds, and not for a role's own.

## Context

ADR 0011 writes each role's skills whole into every session's prompt and leaves "the skills folder with `--plugin-dir`" as the alternative to revisit. Phase 7 step 04 lets a user give one agent, or the team, a skill of their own in the Agent Skills format (a folder with a `SKILL.md`), and step 05's kits ship skills too. Five questions bind the steps after this one.

Where skills live and which one wins. A skill can belong to a role (embedded in the binary), to the team, or to one agent. The project plan decided the three levels; the order of precedence and what a shipped skill does when a user's has its name were open.

Prompt or on demand. Written into the prompt, a skill costs space in every session, used or not, and a user's twenty skills would swamp it. Claude Code lists a skill's name and description and loads the body when the skill is used. The sessions given one Farik tool alone (triage, the judgment, the design-plan decision) have no `Skill` tool, so skills cannot load there.

Where the session's copy is. Claude Code reads `--plugin-dir <folder>` with `.claude-plugin/plugin.json` and `skills/<name>/SKILL.md` (probed on 2.1.287, the plugin's skills named `farik:<name>`). The options: under `.farik/local/sessions/<id>/`, which the shipped protected path `.farik/local/**` turns into a `Read` deny rule for sessions run in the project's root, and which a hook's `allow` cannot lift (probed); `.claude/skills` in the worktree, which `--setting-sources ""` turns off and which an agent could commit; or a folder in the user's state folder.

What a skill may do. A skill is instructions an agent follows. Claude Code reads frontmatter fields that change behaviour (`allowed-tools`, `hooks`, `model`, `context`, and more), runs a body's `` !`cmd` `` and ```` ```! ```` blocks at load, and attaches `@<path>` by its own read, which no hook sees. `.farik/` travels with the repository, so a pull could change a skill's text and its pin together.

## Decision

**Three levels, the most specific wins.** The role's skills (`crates/roles/roles/<role>/skills/`, embedded), the team's (`.farik/skills/<name>/`) and one agent's (`.farik/agents/<agent_id>/skills/<name>/`). One name resolves agent, then team, then role. An agent's pin replaces the team's skill of that name whatever the agent's skill's state, so one waiting for review never falls back silently. A team or agent skill with a shipped name replaces the shipped skill only when the person said so (`replace_shipped`), and only while it is `in_use`.

**Role skills stay in the prompt**; the team's and the agent's (and, in step 05, a kit's) load on demand. Moving a role's own skills to load on demand would take them from the one-tool sessions. A replaced shipped skill leaves the prompt only of the sessions that load skills.

**A plugin folder per session, in the state folder.** `<state>/skills/<local project id>/<session id>/`, 0700, written fresh at each start and resume, removed when the Claude Code process exits, the project's folder wiped whole when the daemon starts. It holds the plugin manifest and a copy of each skill whose frontmatter is rewritten to `name` and `description` alone, which drops every behaviour-changing field. The adapter adds `--plugin-dir` and `Skill` to `--tools` only when the session has a skill, and every session's settings set `disableSkillShellExecution`. A `Skill` call is allowed only as `farik:<name>` for one of the session's skills, with an optional string `args` holding no `@` (Claude Code attaches `@<path>` in a skill's arguments as it does in its body) and nothing else; `Read`, `Glob` and `Grep` under the folder are allowed at tier `read` and every write there is refused.

**Pinned and confirmed on this computer, as ADR 0030 does for a connector.** `team.yaml` pins `{ name, sha256 }` per skill, where `sha256` hashes the canonical JSON of every file's path and byte hash. A skill loads only when its folder hashes to its pin, the newest `skill.added`, `skill.changed` or `skill.confirmed` for it in this machine's log carries that hash, and no `skill.removed` follows. The user reads the whole `SKILL.md` before adding, and `skill_confirm` with the hash the person saw is how a skill that arrived by a clone, a pull or a hand edit is used.

**No commands and no attached files.** A `SKILL.md` holding `` !` `` or ```` ```! ```` anywhere, or a line opening a fence of backticks or tildes then `!`, is refused. An `@` is refused unless the character before it is an ASCII letter or digit or one of `._%+-`, as in an email address: Claude Code attaches `@<path>` after any whitespace, a byte order mark included, and after 。、？！. A skill points at a bundled file by its path, which the agent reads with `Read`, which the hook judges.

## Consequences

A confirmed skill can steer its agent within the agent's own tiers and connectors in every later session, and its description is listed in each; the reading before adding is the defence against the instructions themselves. In no-sandbox mode a command an agent runs can read `daemon.json`'s token and send `skill_save`, whose `skill.added` counts as confirmation; spec 8.3's warning names it. Claude Code's own skills stay listed and calls to them are denied, which costs turns, not `max_tool_calls`.

Step 05 loads a kit's skills through the team level's folder and this ADR's checks. A saved team template does not carry skills. A skill that runs scripts is out of scope.
