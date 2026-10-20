# Customizable governor, plan 3: the web app

Status: ready
Branch: `feat/customizable-governor-web` in its own worktree, from `feat/customizable-governor` after plan 1's Task 1; merged into `feat/customizable-governor` before plan 4
Spec and decisions: see `README.md`; ADR 0080 decisions 1 to 10

Component and test names, and what each test asserts (ADR 0008). Paths are under `apps/web/src/`. Every new string goes in `strings/en.ts`; TypeScript names are camelCase, mapped at the existing edge where the team is read and saved (hard rule 6).

## Decisions

- A new settings section, Rules, at `/settings/rules`, listed in `pages/settings/SettingsLayout.tsx` after Allowed actions: name "Rules", summary "What agents may touch and run". Its page, `pages/settings/Rules.tsx`, is built like `AllowedActions.tsx` from parts in a new `pages/GovernorRules.tsx`, which imports `Part`, `useChange`, `PutBack` and `useRuleParts` from `TeamRules.tsx` (exported there if not already). Chose a new file over growing `TeamRules.tsx`: it is already the largest page module.
- Parts of the Rules page, in order; each saves through `team.save` with its own foot, as `UiPaths` does:
  1. `ForbiddenFiles` ("Files agents may not touch"): each shipped glob is a row with its plain name ("Secrets in .env files" for `.env` and `.env.*`, "Certificates and keys" for `**/*.pem` and `**/*.key`, "Catervas's own files" for `.catervas/local/**`) and a three-way choice, "Off limits" / "Read only" / "Allowed". Off limits removes it from `lifted_paths` and `read_only_paths`; Read only adds it to both; Allowed adds it to `lifted_paths` alone. Choosing anything but Off limits for a shipped glob shows its warning beneath, in `--warning` text: what it kept out of reach. For `.catervas/local/**` the choice is "Off limits" / "Read only" alone, and the warning names the private books and the procurement mail. Below, the owner's own globs from `protected_paths` and `read_only_paths`, each with Off limits / Read only and Remove, and an add field (glob, defaulting to Off limits).
  2. `Commands` ("Commands agents may not run"): the `forbidden_commands` list. The add field takes plain words and writes `^` + the words with regex metacharacters escaped; "advanced" (`ShowAdvanced`) shows a field that adds a raw pattern. A row shows the plain words when the pattern is exactly that shape, else the pattern in `<code>`.
  3. `Git` ("Git in agents' commands"): the read-only eight shown as always allowed (not removable), the owner's `git_subcommands` with Remove, an add field that accepts `^[a-z][a-z0-9-]{0,39}$` only. When `push` is listed, a note: "A push from a command is not held to the remotes below."
  4. `Remotes` ("Where agents may push"): `push_remotes`, `origin` when absent, Remove disabled on the last one, an add field matching the schema's pattern. Shown only when the team's `policy.permissions.push` is true or any agent grants `git_remote`; otherwise one line pointing to Allowed actions.
  5. `Limits` ("When work stops and asks you"): number fields for `readiness_attempts` (1 to 100), `max_iterations` (1 to 100), `blocked_limit_hours` (1 to 720), `escalation_age_hours` (1 to 720), `wip_limit_per_agent` (0 to 100), each with the default as its placeholder and `PutBack`.
  6. `SessionBudget` ("How long one session may run"): `budgets.session`'s four fields, minutes for `max_wall_clock_seconds`, tokens in thousands, empty meaning the role's default.
- The agent's page (`pages/AgentEdit.tsx`) gains a section, Rules for <name>, after "may", from a new `pages/AgentRules.tsx`: the team's effective forbidden and read-only files with, per row, "Lift for <name>" (writes the glob to the agent's `rules.lifted_paths`); an add field for the agent's own off-limits or read-only glob; the agent's added commands and git, as on the Rules page; the agent's `session`, as `SessionBudget`. It saves through the page's existing draft and `agent.replace`.
- Connector tools: in the agent page's connectors section, each tool of a connector gets `ToolChoice` (in `AgentRules.tsx`): "Runs" / "Asks first" / "Never", writing the tool's label `network` or `external_effect` (the label it had when added; a tool added as `ask` or `denied` becomes `external_effect`), `ask`, or `denied`. A tool labelled `paid` offers "Asks first" (keeps `paid`) and "Never". A tool the connector marks as the plan's shows no choice.
- `Team` (`pages/setup/TeamSetup.tsx`) gains the camelCase fields: `rules.liftedPaths`, `rules.readOnlyPaths`, `rules.gitSubcommands`, `rules.protectedPaths`, `rules.forbiddenCommands`, `policy.pushRemotes`, `policy.readinessAttempts`, `policy.maxIterations`, `policy.blockedLimitHours`, `policy.escalationAgeHours`, `policy.wipLimitPerAgent`, `budgets.session`; `Agent` gains `rules` and `session`. The labels' union gains `"ask"`, and `labelsSaid` (`ConnectorAdd.tsx`) says it "asks first".

## File map

```
pages/settings/SettingsLayout.tsx, app/App.tsx   modifies: the Rules section and route (T13)
pages/settings/Rules.tsx                         creates: the page (T13)
pages/GovernorRules.tsx                          creates: ForbiddenFiles (T13), Commands, Git, Remotes (T14), Limits, SessionBudget (T15)
pages/TeamRules.tsx                              modifies: exports the shared helpers (T13)
pages/setup/TeamSetup.tsx                        modifies: the Team and Agent types (T13)
pages/AgentRules.tsx                             creates: the agent's rules (T16), ToolChoice (T17)
pages/AgentEdit.tsx                              modifies: renders them (T16, T17)
pages/ConnectorAdd.tsx                           modifies: labelsSaid says ask (T17)
strings/en.ts                                    modifies: every new string (T13 to T17)
pages/settings/settings.test.tsx                 tests (T13 to T15)
pages/team.test.tsx, pages/connectors.test.tsx   tests (T16, T17)
```

## Tasks

### Task 13: the owner chooses which files agents may touch

Tests (`settings.test.tsx`):
- `lifts a shipped path, makes it read only, and adds the owner's own` — on `/settings/rules`, choosing Read only for "Secrets in .env files" (`.env`) and adding `infra/**` as Off limits saves a team whose `rules` are `{ lifted_paths: [".env"], read_only_paths: [".env"], protected_paths: ["infra/**"] }` (in wire form), and the `.env` warning is shown.
- `keeps Catervas's own files unwritable` — the `.catervas/local/**` row offers Off limits and Read only and no Allowed.

- [ ] `feat(web): the owner chooses which files agents may touch`

### Task 14: commands, git and remotes on the Rules page

Tests (`settings.test.tsx`):
- `adds a forbidden command in plain words and git the owner allows` — adding "rm -rf" saves `forbidden_commands: ["^rm -rf"]`; adding `fetch` saves `git_subcommands: ["fetch"]`; `Fetch` cannot be added; the eight read-only subcommands show without Remove.
- `lists the remotes agents may push to` — with `push: true`, the page shows `origin`, adding `fork` saves `push_remotes: ["origin", "fork"]`, and the last remote cannot be removed; with no agent able to push, the part is one line linking to Allowed actions.

- [ ] `feat(web): commands, git and remotes on the Rules page`

### Task 15: limits and session budgets on the Rules page

Tests (`settings.test.tsx`):
- `sets when work stops and how long a session runs` — setting readiness attempts to 5 and session minutes to 10 saves `policy.readiness_attempts: 5` and `budgets.session.max_wall_clock_seconds: 600`; Put back clears both from the saved team.

- [ ] `feat(web): limits and session budgets on the Rules page`

### Task 16: an agent's own rules

Tests (`team.test.tsx`):
- `lifts a team path for one agent and adds its own` — on `/team/ada`, "Lift for Ada" on `infra/**` and adding `notes/**` as Off limits and `max_tool_calls` 50 sends `agent.replace` with `rules: { lifted_paths: ["infra/**"], protected_paths: ["notes/**"] }` and `session: { max_tool_calls: 50 }`, and the team's own rules are unchanged.

- [ ] `feat(web): an agent's own rules`

### Task 17: what each connector tool does

Tests (`connectors.test.tsx`):
- `sets a tool to run, ask first, or never` — an `external_effect` tool set to Asks first saves `ask`, then Never saves `denied`, then Runs saves `external_effect`; a `paid` tool offers Asks first and Never only; a plan-marked tool shows no choice.

- [ ] `feat(web): the owner sets what each connector tool does`

## Verification

```
cd apps/web && pnpm check
# expected: exit 0, every test passing
```
