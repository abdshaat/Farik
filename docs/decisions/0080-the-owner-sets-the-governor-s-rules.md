# 0080. The owner sets the governor's rules

Date: 2026-10-20
Status: accepted (the founder, 2026-10-20, quoted below; the mechanisms decided by the planner the same day)

The founder: "I am looking to make the governer engine customizable. Things like forbidden paths, pushing things to remote repo, etc. it all must be customizable from the user's end."

Asked four questions, the founder answered: every shipped forbidden path is removable ("Everything removable"); of pushing, the remotes an agent may push to and the git an agent's commands may run ("Which remote, More git for agents"), not a choice between a pull request and a direct push, and not a branch list; the commands, the retry and blocking limits, the session budgets and the outward acts are set in the web app too; rules are set for the team and overridden per agent ("Team-wide + per agent"). Told that lifting `.catervas/local/**` would let an agent write the governor's own state (its event log, the approvals, the merge permission), the founder chose "Removable, but never writable".

Amends SPEC 5.5 (session limits per agent), 5.6 (protected paths, git in commands, connector labels), 5.7 (readiness attempts), 5.12 (team rules, per-agent rules), 5.14 (the remotes a task branch is pushed to) and 8.6. Amends ADR 0061's "Secrets stay out of reach: the protected paths" to "unless the owner lifts them", and ADR 0079 decision 9's fixed read-only git list to a default. Leaves ADR 0061's pull request (Catervas's integration is always one) and its "only payments ask" for Catervas's own decisions unchanged.

## Context

The governor's rules were three kinds: shipped and fixed in code (the five protected paths, the read-only git list, `origin`, three readiness attempts), in `.catervas/team.yaml` with no screen (`protected_paths`, `forbidden_commands`, the limits, the session budgets), or on a settings screen (`run_commands`, `push`, the daily limit, the UI paths, merging). Catervas is for non-technical owners (ADR 0016): a rule only the team file holds is a rule they cannot change, and a rule fixed in code is one nobody can.

## Decision

1. **One Rules page and a Rules part on each agent's page.** Settings gains Rules (`/settings/rules`): forbidden files, commands, git, remotes, limits and session budgets. An agent's page (`/team/:id`) gains its own rules: the files it may or may not touch, its own forbidden commands and git, its session budget, and what each of its connector tools does. What the team sets is the default; what the agent's page sets is added to it or lifts part of it for that agent alone. Every rule reads in plain words, with the glob or pattern under "advanced".
2. **The shipped protected paths are removable.** `rules.lifted_paths` names which of the five shipped globs no longer apply; `rules.protected_paths` keeps adding the team's own. Chose a list of lifted ones over making `protected_paths` the whole list: a team file written before this, which lists only its additions, keeps the shipped five, so no team is loosened by an upgrade. A lifted glob shows its warning: what it kept out of reach.
3. **Read only is a third answer.** `rules.read_only_paths`: globs an agent may read and no tool may write. A forbidden file is "Off limits" (protected), "Read only", or neither.
4. **Catervas's own state is never writable, and its keys never readable.** Not a rule, so not removable: no agent's tool call writes under `.catervas/local/`, and none reads `.catervas/local/daemon.json` or `.catervas/local/sessions/**`, which hold the daemon's token: reading it is writing everything. Lifting `.catervas/local/**` makes the rest of it read only. Catervas's own tools (the notebook, the Finance Specialist's books) still write their files, as before. The done check's refusal of any diff under `.catervas/` (SPEC 5.4) is unchanged.
5. **Per agent.** `agents[].rules` holds `protected_paths` and `read_only_paths` (added for this agent), `lifted_paths` (any glob of the team's that does not apply to this agent, the shipped five among them), `forbidden_commands` (added) and `git_subcommands` (added). An agent's rules are the team's with these applied; every check that reads the protected paths for a session reads its agent's.
6. **Git in commands.** `rules.git_subcommands` lists the git subcommands `catervas_exec` runs on top of the read-only eight, which stay the default and cannot be removed (they change nothing). A subcommand is a lower-case word. A pushing subcommand allowed here is the owner's choice and is not held to the remotes below; the page says so.
7. **Remotes.** `policy.push_remotes` names the remotes `catervas_git_push` may push to, `["origin"]` when left out. The tool takes an optional `remote`, the first listed when left out; it still pushes only the assignee's own task branch. Team-wide only: a remote is the repository's, not an agent's. Catervas's own integration pushes are unchanged.
8. **Limits.** `policy.readiness_attempts` (1 to 100, default 3) replaces the fixed three. It and `max_iterations`, `blocked_limit_hours`, `wip_limit_per_agent` and `escalation_age_hours` are on the Rules page. Team-wide only.
9. **Session budgets per agent.** `agents[].session` (the `sessionLimits` shape) is applied after `budgets.session`, field by field. Both are on screens.
10. **Outward acts.** A connector tool's label gains `ask`: it asks the human, or uses an allowance, as a `paid` one does, and costs nothing. On the agent's page each tool is "Runs", "Asks first" or "Never" (`network` or `external_effect` as shipped, `ask`, `denied`). A `paid` tool offers "Asks first" or "Never": no tool pays without the owner (ADR 0041). A tool the owner's plan marks (ADR 0042) is unchanged.
11. **Agents never change the rules.** The rules live in `.catervas/team.yaml`, under `.catervas/`, which no agent writes (5.4); only the owner's screens and the team file change them.

## Consequences

- An owner can now give an agent `.env`, a key file or the private books; the warning is the only thing between. That is the founder's call.
- Every check that reads the protected paths for a session or a task (the hook, the tools, the done check, the session's settings, the prompt, the folders' tools, `catervas_exec`) reads `Team::rules_for(agent)` instead of `Team::rules()`; the project scan, `catervas doctor` and `catervas team` show the team's.
- Claude Code's deny rules are built per session: `Read(...)` for off-limits globs and Catervas's keys, `Edit(...)` for read-only globs and `.catervas/local/**`.
- `connectorTag` gains `ask`; `team.schema.json` gains `rules.lifted_paths`, `rules.read_only_paths`, `rules.git_subcommands`, `policy.push_remotes`, `policy.readiness_attempts`, `agents[].rules` and `agents[].session`. Every addition is optional, so every team file stays valid.

## Alternatives rejected

- A locked floor of the shipped paths: the founder chose every one removable.
- `.catervas/local/**` fully removable: an agent that writes the event log approves its own calls, and then no other rule holds.
- Team, role and agent layers: two layers say everything a role layer would, with less to explain.
- A choice between a pull request and a direct push: not asked for; ADR 0061 stands.
