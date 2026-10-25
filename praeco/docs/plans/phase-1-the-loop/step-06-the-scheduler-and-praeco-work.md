# Phase 1, step 06: The scheduler and `praeco work`

Status: draft
Branch: `phase/1-the-loop`
Spec: `docs/SPEC.md` 5.2, 5.5, 6.1, 10; F8
Depends on: phase 0 (merged); steps 01 to 05 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0006, ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

`praeco work` keeps every workspace working: on each tick it stops cycles whose process died, cancels cycles queued too long, queues the programs that are due, and claims and runs queued cycles, requests first, one at a time per workspace, up to `PRAECO_WORKERS` at once, each only when the budget has room for it. A running cycle beats every 30 seconds. Pausing a workspace stops its running cycle and claims no more until it is resumed. Docker Compose gains a `work` service.

Out of scope: the commands that pause, queue requests and set budgets (step 07), schedules other than a program's interval (the digest's 09:00 is phase 2's).

## Decisions

- **Programs' defaults** (`DEFAULT_PROGRAMS`): `research` enabled, every 60 minutes. `request` is never scheduled; it is queued by a person (step 07, then phase 2).
- **The research job a due program queues** (`programContract('research', limits)`): intent "Learn what the product does, who it is for, what it costs and how it describes itself, from its own site and what the knowledge store does not yet hold; record each fact with its page."; expects at least 3 findings; tools `PROGRAM_TOOLS.research`; budget the workspace's cycle limit, 20 minutes, 150 tool calls.
- **`dueCycles(programs, limits, now)`** returns a contract draft for each program that is enabled, has no `queued` or `running` cycle, and whose last cycle was queued at least its interval ago (or never), and returns nothing when `limits` is null (no month limit: SPEC 4.1 step 2 says the first cycle waits for one).
- **`0006_scheduler.sql`**: `program_settings (workspace_id uuid not null references workspaces (id) on delete cascade, program text not null, is_enabled boolean not null, interval_minutes integer not null check (interval_minutes between 15 and 10080), primary key (workspace_id, program))`, row-level security, `select, insert, update` to `praeco_app`. A workspace with no row for a program uses `DEFAULT_PROGRAMS`.
- **A tick** (`tick(deps, now)`), every `PRAECO_TICK_SECONDS` (default 15, 5 to 300):
  1. **Reap** (`reapStale`, as the installation): every `running` cycle whose `heartbeat_at` is older than 2 minutes becomes `stopped` `error` with `cycle.stopped`; every `queued` cycle older than 24 hours becomes `cancelled` `expired` with `cycle.cancelled`.
  2. **Queue** (per workspace that is not paused, from `listWorkspaces`): `readProgramStates`, `readLimits`, `dueCycles`, then `validateJob` and `queueCycle` (priority `program`) for each draft, with fresh ids. A draft `validateJob` refuses is a bug: it is logged at `error` and skipped.
  3. **Claim** while this process has free slots: `listClaimable` (as the installation) gives workspaces that are not paused, have a queued cycle and no running one, oldest queued first; for each, in its own `inWorkspace` transaction: `readLimits`, `spentSince` for the day and the month (`periodStarts` in the workspace's time zone), `canStartCycle`; a spend refusal calls `noteBudgetReached` and skips the workspace this tick; allowed: `claimNextCycle` takes the workspace's next queued cycle (`request` before `program`, then oldest) with `for update skip locked`, transitions it to `running` and appends `cycle.started`; a unique-index conflict (another process claimed the workspace's slot first) is "nothing claimed".
  4. **Run** each claimed cycle with `runCycle` without awaiting it, holding a slot until it resolves.
- **Heartbeats:** while a cycle runs, the worker calls `beat` every 30 seconds; a failed beat is logged and retried at the next interval.
- **Pause:** `setPaused(tx, true)` sets `paused_at` and appends `workspace.paused`; `false` clears it and appends `workspace.resumed`. The runner already stops a running cycle at its next event (step 04); the tick queues and claims nothing for a paused workspace.
- **Shutdown:** on the abort signal the worker stops ticking, interrupts each running session, and waits up to 30 seconds for the runners to record their endings; a runner still going after that is left for the reaper, whose 2-minute rule then stops it as `error`.
- **`praeco work`** builds its dependencies: the store; `new Anthropic()` (it reads `ANTHROPIC_API_KEY`); `anthropicManagedAgents`; the agent's ref from `readAgentRef` (missing, or its `definitionHash` different from `loadAgentDefinition`'s: exit 1 with "The agent is not synced; run praeco agent sync."); `researchExecutor` with the global `fetch`; the program texts; `afterCycle` as `syncKnowledge`; `deadline(ms)` as `AbortSignal.timeout(ms)`.
- **Config** adds `ANTHROPIC_API_KEY` (required by `work` and, in step 07, `agent sync`; checked by those commands, not by `loadConfig`, so `serve` and `migrate` run without it), `PRAECO_WORKERS` (default 4, 1 to 32), `PRAECO_TICK_SECONDS`, and `PRAECO_BOT_INFO_URL` (step 05).
- **Compose** adds `work` (`command: work`, `depends_on: migrate: condition: service_completed_successfully`, `environment` adds `ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}`); `.env.example` gains `ANTHROPIC_API_KEY=`.
- **Events:** `workspace.paused` `{}`, `workspace.resumed` `{}`.

## File map

```
packages/core/src/programs.ts, programs.test.ts                   creates, tests: DEFAULT_PROGRAMS, programContract, dueCycles (Task 1)
packages/core/src/index.ts                                        modifies (Task 1)
packages/store/migrations/0006_scheduler.sql                      creates (Task 2)
docs/schemas/events/workspace.paused.schema.json, workspace.resumed.schema.json   creates (Task 2)
packages/schemas/src/generated/**                                 generated (Task 2)
packages/store/src/scheduler.ts, database.ts, index.ts            creates; modifies (Task 2)
packages/store/src/scheduler.integration.test.ts                  tests (Task 2)
apps/praeco/src/worker.ts, worker.integration.test.ts             creates, tests: tick, runWorker (Task 3)
apps/praeco/src/config.ts, config.test.ts, commands.ts            modifies: the variables, the work command (Task 3)
docker-compose.yml, .env.example                                  modifies (Task 3)
```

## Interfaces

Consumes: `PROGRAM_TOOLS`, `Program` (step 01); `validateJob`, `JobContract`, `transitionCycle`, `Cycle` (step 02); `Limits`, `canStartCycle`, `periodStarts`, `readLimits`, `spentSince`, `noteBudgetReached` (step 03); `runCycle`, `CycleDeps`, `loadAgentDefinition`, `loadProgramText`, `anthropicManagedAgents`, `readAgentRef` (step 04); `researchExecutor`, `syncKnowledge` (step 05).

Produces:

```ts
// @praeco/core
export interface ProgramSetting { program: Program; isEnabled: boolean; intervalMinutes: number }
export const DEFAULT_PROGRAMS: readonly ProgramSetting[];
export interface ProgramState extends ProgramSetting { lastQueuedAt: Date | null; hasOpenCycle: boolean }
export type ContractDraft = Omit<JobContract, 'jobId' | 'workspaceId' | 'requestedBy' | 'createdAt'>;
export function programContract(program: Program, limits: Limits): ContractDraft;
export function dueCycles(programs: readonly ProgramState[], limits: Limits | null, now: Date): readonly ContractDraft[];
// @praeco/store
export function readProgramStates(tx: WorkspaceTx): Promise<Result<ProgramState[], StoreError>>;
export function listClaimable(tx: InstallationTx, limit: number): Promise<Result<Workspace[], StoreError>>;
export function claimNextCycle(tx: WorkspaceTx, input: { eventId: string; now: Date }): Promise<Result<Cycle | null, StoreError>>;
export function beat(tx: WorkspaceTx, cycleId: string, now: Date): Promise<Result<void, StoreError>>;
export function reapStale(tx: InstallationTx, input: { ids: Ids; now: Date }): Promise<Result<{ stopped: number; cancelled: number }, StoreError>>;
export function setPaused(tx: WorkspaceTx, paused: boolean, input: { eventId: string; actor: Actor; now: Date }): Promise<Result<void, StoreError>>;
// praeco (apps/praeco)
export interface WorkerDeps { store: Store; cycleDeps(workspaceId: string): CycleDeps; clock: Clock; ids: Ids; workers: number; tickSeconds: number;
  log: { info(msg: string): void; error(msg: string): void } }
export function tick(deps: WorkerDeps, running: Map<string, Promise<unknown>>): Promise<void>;
export function runWorker(deps: WorkerDeps, signal: AbortSignal): Promise<void>;
```

## Tasks

### Task 1: Which programs are due

Files: created `packages/core/src/programs.ts`, `programs.test.ts`; modified `src/index.ts`.
Produces: `ProgramSetting`, `DEFAULT_PROGRAMS`, `ProgramState`, `ContractDraft`, `programContract`, `dueCycles`.

- `dueCycles` — asserts that research is due when enabled, never queued and a month limit is set; is not due 59 minutes after its last queueing and is due at 60; is not due while it has an open cycle, when disabled, or when the limits are null; and that the draft it returns passes `validateJob` with the same limits.

- [ ] `feat(core): decide which programs are due`

### Task 2: Claiming, heartbeats, reaping and pause in the store

Files: created `packages/store/migrations/0006_scheduler.sql`, the two event schemas, `packages/store/src/scheduler.ts`, `src/scheduler.integration.test.ts`; modified `src/database.ts`, `src/index.ts`; generated schemas.
Produces: `readProgramStates`, `listClaimable`, `claimNextCycle`, `beat`, `reapStale`, `setPaused`.

- `claimNextCycle` — asserts that with a `program` cycle queued before a `request` cycle, the request is claimed first, `running`, with `cycle.started`; that two claims racing in the same workspace (two `inWorkspace` transactions started together) yield one cycle and one `null`; and that `listClaimable` leaves out a paused workspace and one with a running cycle.
- `reapStale` — asserts that a running cycle beaten 119 seconds ago is left and one beaten 121 seconds ago becomes `stopped` `error`, and that a cycle queued 25 hours ago becomes `cancelled` `expired`, each with its event.

- [ ] `feat(store): claim one cycle per workspace and reap dead ones`

### Task 3: The worker and `praeco work`

Files: created `apps/praeco/src/worker.ts`, `worker.integration.test.ts`; modified `apps/praeco/src/config.ts`, `config.test.ts`, `commands.ts`, `docker-compose.yml`, `.env.example`.
Consumes: Tasks 1 and 2; `runCycle` with `FakeManagedAgents`.
Produces: `WorkerDeps`, `tick`, `runWorker`, the `work` command.

With a test store, two workspaces with month limits, and a fake script that reads one page, records three findings and finishes, costing 50 cents:

- the worker — asserts that two ticks run one research cycle per workspace to `finished`, never two at once in one workspace; that with the day's limit at $1 and the cycle's at $1 a second research cycle in the same day is not claimed and one `budget.reached` `day_reached` is appended; that a workspace paused before a tick gets no cycle; and that aborting the signal while a cycle runs interrupts its session and the cycle ends `stopped`.
- `loadConfig` (extended) — asserts that `PRAECO_WORKERS=0` is a problem and that defaults give 4 workers and 15-second ticks.

- [ ] `feat(praeco): keep every workspace working with praeco work`

## Verification

```
pnpm check --integration
# expected, last line: check: ok
```
