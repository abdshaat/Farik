# Phase 1, step 07: The command line and the first live run

Status: draft
Branch: `phase/1-the-loop`
Spec: `docs/SPEC.md` 4.7, 11 (M0); F3, F5, F7, F8, F21
Depends on: phase 0 (merged); steps 01 to 06 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

An operator runs Praeco from the command line before Slack and the web app exist: creates a workspace for a product, syncs the agent, sets budgets, asks for work, pauses and resumes, and reads what Praeco did (cycles, findings, events). A live test runs one real research cycle on Claude Managed Agents against a real public site and records its events as the fake's fixture. Its success is M0.

Out of scope: members and roles (phase 2), the web app (phase 3).

## Decisions

- **Commands** (all print plain text; `--json` on the three reading commands prints one JSON value, `snake_case`):
  - `praeco workspace create --name <name> --site <url> --month-budget <usd> [--time-zone <IANA>]`: the time zone defaults to `UTC` and must be in `Intl.supportedValuesOf('timeZone')`; creates the workspace (`createWorkspace`), its profile with `site_url` and `name`, and its limits (`deriveLimits` from the month); prints `created workspace <name> (<id>)` and its limits. A month budget is required here, so the first cycle never waits on it from the command line.
  - `praeco workspace list`: one line per workspace: name, id, paused or not, time zone.
  - `praeco agent sync`: `loadAgentDefinition` (model `claude-opus-5-5`), `readAgentRef`; the same hash: prints `agent up to date (<id>, version <n>)`; else `syncAgent`, `saveAgentRef`, prints `agent synced (<id>, version <n>)`.
  - `praeco ask [--workspace <w>] <intent…>`: queues a `request` cycle: the intent as given (20 to 600 characters), expects at least 1 finding, tools `PROGRAM_TOOLS.request`, the cycle limit, 20 minutes, 150 tool calls; prints `queued J-<n>`.
  - `praeco cycles [--workspace <w>] [--limit <n>]` (default 20): `J-<n>  <program>  <status>[ (<reason>)]  $<spent>  <started, local time>  <summary's first 80 characters>`.
  - `praeco findings [--workspace <w>] [--topic <t>] [--limit <n>]` (default 50): `<recorded, local date>  [<topic>]  <statement>  <source_url>`, `(guess)` when it is one.
  - `praeco events [--workspace <w>] [--kind <k>] [--limit <n>]` (default 50): `<seq>  <recorded, local time>  <kind>  <actor>  <payload as compact JSON>`.
  - `praeco pause [--workspace <w>]`, `praeco resume [--workspace <w>]`: `setPaused` with actor `operator`; prints `paused` or `resumed`.
  - `praeco budget show [--workspace <w>]`; `praeco budget set [--workspace <w>] --month <usd> [--day <usd>] [--cycle <usd>]`: `deriveLimits`, `setLimits`; a refusal prints its reason in words and exits 1.
- **`--workspace`** takes an id or an exact name; without it, the only workspace; with none or several and no flag: exit 2 with "Name the workspace with --workspace; there are <n>."
- **Dollars** are parsed by `parseUsd`: digits, an optional `.` and one or two digits, at most 1 000 000; anything else is refused "Write an amount in dollars, like 30 or 12.50."; the result is micro-dollars. Printed as `$<dollars>.<cents>`.
- **Local times** are printed in the workspace's time zone as `YYYY-MM-DD HH:mm`.
- **The live test** (`apps/praeco/src/first-run.live.test.ts`, run only with `PRAECO_LIVE_TESTS=1` and `ANTHROPIC_API_KEY` set, in the `live` project): a Postgres container; `migrate`; `workspace create --name Live --site $PRAECO_LIVE_SITE --month-budget 5` (`PRAECO_LIVE_SITE` defaults to `https://www.postgresql.org`); `agent sync`; then the worker ticks until the first research cycle ends or 25 minutes pass. It asserts the cycle `finished`, at least 3 findings each with a `source_url` on the site's host, `spent_usd_micros` at most the cycle's limit, and that every `tool.allowed` has a matching `tool.completed` or `tool.failed`. It writes the session's raw events (from `client.beta.sessions.events.list`) to `packages/agent/src/fixtures/recorded/research-<YYYY-MM-DD>.json` and the test's summary to stdout. The fixture is committed by hand with the run's date.
- **The fake replays the recording:** `FakeManagedAgents.fromRecording(file)` replays a recorded session's events, answering each custom tool use with whatever the executor returns, so a later SDK upgrade is checked against real event shapes (ADR 0004). One test uses it.
- **M0** is declared in the phase's pull request with the live test's output pasted, the README's status line updated ("Praeco researches a product from the command line; no Slack yet"), and SPEC 11 unchanged.

## File map

```
apps/praeco/src/money.ts, money.test.ts                          creates, tests: parseUsd, formatUsd (Task 1)
apps/praeco/src/workspace-option.ts                              creates: resolveWorkspace (Task 1)
apps/praeco/src/commands/workspace.ts, budget.ts, pause.ts       creates (Task 1)
apps/praeco/src/commands/agent.ts                                creates: agent sync (Task 2)
apps/praeco/src/commands/ask.ts, cycles.ts, findings.ts, events.ts   creates (Task 3)
apps/praeco/src/commands.ts                                      modifies: the dispatch table (Tasks 1 to 3)
apps/praeco/src/commands.integration.test.ts                     tests (Tasks 1 to 3)
packages/agent/src/fixtures/fake-managed-agents.ts               modifies: fromRecording (Task 4)
packages/agent/src/fixtures/recorded/research-<date>.json        creates by the live run (Task 4)
packages/agent/src/replay.integration.test.ts                    tests (Task 4)
apps/praeco/src/first-run.live.test.ts                           creates (Task 4)
README.md, CLAUDE.md                                             modifies: status, commands (Task 4)
```

## Interfaces

Consumes: everything steps 01 to 06 produce.

Produces:

```ts
// praeco (apps/praeco)
export function parseUsd(text: string): Result<number, { message: string }>;
export function formatUsd(usdMicros: number): string;
export function resolveWorkspace(tx: InstallationTx, flag: string | undefined): Promise<Result<Workspace, { message: string; exitCode: 2 }>>;
// the commands: workspace create|list, agent sync, ask, cycles, findings, events, pause, resume, budget show|set
// @praeco/agent/fixtures
//   FakeManagedAgents gains: static fromRecording(path: string): Promise<FakeManagedAgents>;
```

## Tasks

### Task 1: Workspaces, budgets and pausing

Files: created `apps/praeco/src/money.ts`, `money.test.ts`, `workspace-option.ts`, `commands/workspace.ts`, `commands/budget.ts`, `commands/pause.ts`, `commands.integration.test.ts`; modified `commands.ts`.
Produces: `parseUsd`, `formatUsd`, `resolveWorkspace`; `workspace create|list`, `budget show|set`, `pause`, `resume`.

- `parseUsd` — asserts that `30`, `12.5` and `12.50` give 30 000 000, 12 500 000 and 12 500 000 micro-dollars, and that `12.505`, `-1`, `$5` and `1e3` are refused with the message above.
- workspace and budget commands — asserts that `workspace create --name Acme --site https://acme.example --month-budget 30 --time-zone Europe/Berlin` creates the workspace with a $1.00 day and cycle and prints them; that `--time-zone Mars/Base` exits 2 naming the option; that `budget set --month 30 --day 40` exits 1 with the day-over-month reason in words; that `pause` then `workspace list` shows it paused; and that a second workspace makes `pause` without `--workspace` exit 2 with "there are 2".

- [ ] `feat(praeco): create workspaces, set budgets and pause from the command line`

### Task 2: `agent sync`

Files: created `apps/praeco/src/commands/agent.ts`; modified `commands.ts`, `commands.integration.test.ts`.
Consumes: `loadAgentDefinition`, `readAgentRef`, `saveAgentRef`, `FakeManagedAgents`. A `ManagedAgents` factory is injected into `runCommand`'s `Io` as `agents(): ManagedAgents`, built in `main.ts` from `new Anthropic()` and `anthropicManagedAgents`; the `work` command (step 06) is changed to take it from there too, so tests drive both commands with the fake.

- `agent sync` — asserts, with the fake, that the first run syncs and saves the ref, a second run prints "agent up to date" without syncing again, and a changed system text (in a temporary copy of `agents/praeco`) syncs again with the new hash.

- [ ] `feat(praeco): sync the agent's definition`

### Task 3: Asking and reading

Files: created `apps/praeco/src/commands/ask.ts`, `cycles.ts`, `findings.ts`, `events.ts`; modified `commands.ts`, `commands.integration.test.ts`.

- reading commands — asserts that `ask "Find what the pricing page says about seats"` prints `queued J-1` and `cycles` then lists it as `queued`; that after a fake cycle records two findings and finishes, `cycles --json` gives its status, spend and summary, `findings` prints both with their sources, and `events --kind cycle.finished` prints exactly one line.

- [ ] `feat(praeco): ask for work and read cycles, findings and events`

### Task 4: The live run and the replay

Files: created `apps/praeco/src/first-run.live.test.ts`, `packages/agent/src/replay.integration.test.ts`, the recorded fixture (by running the live test); modified `packages/agent/src/fixtures/fake-managed-agents.ts`, `README.md`, `CLAUDE.md`.

- `fromRecording` replay — asserts that replaying the committed recording through `runCycle` with a fake executor ends the cycle `finished` and makes as many executor calls as the recording has custom tool uses.
- The live test, as decided above; run by hand, its output pasted in the commit body and the pull request.

- [ ] `test(praeco): run the first live research cycle and record it`

## Verification

```
pnpm check --integration
# expected, last line: check: ok
PRAECO_LIVE_TESTS=1 ANTHROPIC_API_KEY=… pnpm vitest run --project live
# expected: 1 passed; the cycle finished with at least 3 findings and its spend under its limit
cp .env.example .env    # set POSTGRES_PASSWORD and ANTHROPIC_API_KEY
docker compose up -d --build
docker compose run --rm serve workspace create --name Acme --site https://www.postgresql.org --month-budget 5
docker compose run --rm serve agent sync
# within the hour:
docker compose run --rm serve findings
# expected: findings with sources on www.postgresql.org
docker compose run --rm serve pause
docker compose down
```
