# Standalone: the owner sets the governor's rules

Status: ready
Branch: `feat/customizable-governor` (its own pull request to `main`); the web plan may run on `feat/customizable-governor-web` in its own worktree, after plan 1's Task 1 (the schema) is committed, and is merged into this branch before the record task
Decision: ADR 0080
Spec: `docs/SPEC.md` 5.5, 5.6, 5.7, 5.12, 5.14, 8.6 (revision 0.135)
Depends on: `origin/main` at `d581bb6b` (pull requests #83 and #84 merged)
Readiness confirmed by: the planner (Opus 5.5), 2026-10-20, against the three readiness rules (ADR 0065)

## Goal

Every rule the governor applies can be changed by the owner in the web app, for the team and per agent (ADR 0080). When done: the five shipped forbidden paths can each be lifted, made read only, or kept, and the owner adds their own, as off limits or read only; an agent's page adds or lifts paths for that agent alone; the owner lists the remotes an agent may push its task branch to and the git subcommands agents' commands may run beyond the read-only eight; forbidden commands, readiness attempts, rejections before escalation, hours blocked, work per agent and session budgets (team and agent) are on screens; and each connector tool of an agent runs, asks first, or never runs. Catervas's own state stays unwritable and its daemon token unreadable whatever the owner sets. Out of scope: choosing a direct push over a pull request for Catervas's integration (ADR 0061 stands), a branch allow-list, rules per role, `preauthorized_external_tools` on a screen.

## Plans, in order

| File | Tasks | Where |
|---|---|---|
| `01-core.md` | 1 to 6 | this worktree |
| `02-runtime.md` | 7 to 12 | this worktree, after 01 |
| `03-web.md` | 13 to 17 | worktree `feat/customizable-governor-web`, after Task 1, in parallel with the rest of 01 and with 02 |
| `04-record.md` | 18 | this worktree, after 02 and after 03 is merged in |

## Decisions shared by the plans

- Every product decision is in ADR 0080; "decision n" below is its number.
- New team-file fields, all optional (`docs/schemas/team.schema.json`), wire names final:
  - `rules.lifted_paths`: array, unique, items `enum` of the five `DEFAULT_PROTECTED_PATHS`.
  - `rules.read_only_paths`: array of glob strings, `maxItems` 100.
  - `rules.git_subcommands`: array, unique, `maxItems` 50, items `pattern` `^[a-z][a-z0-9-]{0,39}$`.
  - `policy.push_remotes`: array, unique, `minItems` 1, `maxItems` 20, items `pattern` `^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$`; absent means `["origin"]`.
  - `policy.readiness_attempts`: integer 1 to 100; absent means 3.
  - `agents[].rules`: object, `additionalProperties: false`, with `protected_paths`, `read_only_paths`, `lifted_paths` (glob strings, `maxItems` 100), `forbidden_commands` (`maxItems` 100), `git_subcommands` (as the team's).
  - `agents[].session`: `$ref` `#/$defs/sessionLimits`.
  - `$defs/connectorTag` gains `"ask"` (plan 1 Task 6, with its enum variant; every other field is Task 1's).
- Catervas's floor (decision 4), constants in `crates/core/src/governor/team_rules.rs`, never read from the team file:
  - `CATERVAS_STATE: &str = ".catervas/local/**"`: no `write_workspace` call's path matches it.
  - `CATERVAS_KEYS: [&str; 2] = [".catervas/local/daemon.json", ".catervas/local/sessions/**"]`: no call's path matches them, whatever its tier.
- How an agent's rules are made (`Team::rules_for`): start from `Team::rules()` (shipped protected paths minus `rules.lifted_paths`, plus `rules.protected_paths`, in that order, deduplicated); add the agent's `protected_paths`, `read_only_paths`, `forbidden_commands`, `git_subcommands`; remove every glob the agent's `lifted_paths` names, exactly as written, from both `protected_paths` and `read_only_paths`. An unknown agent id gets `Team::rules()`.
- A lifted path is not made read only by lifting it; the screen offers "Read only" as a separate answer that writes the glob into `read_only_paths` (decision 3). The one exception is the floor: `.catervas/local/**` lifted is still unwritable because of `CATERVAS_STATE`.
- New refusal wire names: `path_read_only` (a write to a read-only or floor path) and `remote_not_allowed` (`catervas_git_push` to an unlisted remote). Both are added wherever `path_protected` is listed (refusal display, `event.schema.json` if it enumerates refusals).
- Every cargo command in these plans runs with `CARGO_BUILD_BUILD_DIR=$PWD/target`.

## Verification of the whole change

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs the same under its workarounds (CLAUDE.md).

Then the founder's live look in the web app, on a throwaway project:
- Settings → Rules: lift `.env`, set it read only; a developer's task reads `.env` and its `Edit` of `.env` is refused with "read only".
- On one agent's page, lift `docs/**` from a team path added as off limits; that agent reads it, another is refused.
- Add a remote `fork` to the remotes; a push to `fork` passes the governor, a push to `upstream` is refused with `remote_not_allowed`.
- Add `fetch` to git; `git fetch` runs in a task's command.
- Set a connector tool to "Asks first"; its call waits on Help needed.
