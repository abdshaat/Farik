# Phase 1, step 02: Job contracts and the cycle lifecycle

Status: draft
Branch: `phase/1-the-loop`
Spec: `docs/SPEC.md` 5.2, 5.3; F4, F5
Depends on: phase 0 (merged); step 01 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

A job contract is checked before anything is queued (its shape by the schema, its meaning by `validateJob`), and a cycle moves only along the transitions of SPEC 5.2, with `canFinish` holding a cycle to its expectations. The store keeps cycles in the `cycles` table, numbers them `J-1`, `J-2`, … per workspace, allows one running cycle per workspace, and logs every move as an event.

Out of scope: who moves a cycle and when (the runner, step 04; the scheduler, step 06), counting records (step 05), budgets (step 03).

## Decisions

- **One job, one cycle.** A cycle row is the job's contract plus its run; the contract's `job_id` is the cycle's id, and `J-<number>` names both. Rejected: separate `jobs` and `cycles` tables, which are one-to-one.
- **`validateJob(input, limits)`**, after `parse('job_contract', input)`, refuses, in this order:
  - a tool in `allowed_tools` that is not in `PROGRAM_TOOLS[program]`: `tool_not_in_program`;
  - `budget.usd_micros` above `limits.cycleUsdMicros`: `over_cycle_limit`;
  - an `expects` kind that no allowed tool records (`RECORD_TOOLS`: `finding` is recorded by `record_finding`): `nothing_records`;
  - a schema failure: `invalid`, with the schema's path and message.
  `finish_cycle` need not be listed: a `control` tool is always allowed (step 01).
- **`RecordKind`** is `'finding'` in phase 1; later steps add kinds with the tools that record them.
- **Transitions** (`CYCLE_TRANSITIONS`, SPEC 5.2): `queued → running` (scheduler), `queued → cancelled` (operator, or scheduler for `expired`), `running → finished` (governor), `running → stopped` (governor). `transitionCycle` refuses any other pair with `not_allowed`, and sets `startedAt` and `heartbeatAt` on `running`, `endedAt` on `finished`, `stopped` and `cancelled`.
- **`StopReason`**: `budget`, `time`, `tool_calls`, `refused`, `unfinished`, `error`, `paused` (SPEC 5.2). **Cancel reasons**: `by_operator`, `expired`.
- **`attachSession(cycle, sessionId)`** sets the session id on a running cycle that has none, and refuses otherwise (`session_already_attached` or `not_running`); it records no event (the session id is on `cycle.started`'s cycle row and in later events).
- **`canFinish(contract, counts, gaveUp)`**: every expectation's `atLeast` met by `counts[kind]`, or `gaveUp` at least 20 characters after trimming. Refusals: `expectation_unmet` (the first unmet one, with its numbers), `give_up_too_short`.
- **Priority**: `request` cycles before `program` cycles, then oldest `queued_at` first (step 06 claims in this order).
- **`0002_cycles.sql`**:
  - `alter table workspaces add column next_cycle_number integer not null default 1;` and grant `update (next_cycle_number)` to `praeco_app`.
  - `cycles (id uuid primary key, workspace_id uuid not null references workspaces (id) on delete cascade, number integer not null, program text not null, contract jsonb not null, priority text not null check (priority in ('request', 'program')), status text not null check (status in ('queued', 'running', 'finished', 'stopped', 'cancelled')), stop_reason text check (stop_reason in ('budget', 'time', 'tool_calls', 'refused', 'unfinished', 'error', 'paused')), cancel_reason text check (cancel_reason in ('by_operator', 'expired')), session_id text, queued_at timestamptz not null, started_at timestamptz, ended_at timestamptz, heartbeat_at timestamptz, spent_usd_micros bigint not null default 0, summary text, gave_up text, unique (workspace_id, number))`.
  - `create unique index cycles_one_running on cycles (workspace_id) where status = 'running';` and `create index cycles_by_queue on cycles (status, priority, queued_at);`.
  - Row-level security with `cycles_in_workspace`; grants `select, insert, update` to `praeco_app`.
- **Numbering:** `queueCycle` takes the number with `update workspaces set next_cycle_number = next_cycle_number + 1 where id = $1 returning next_cycle_number - 1`, whose row lock orders concurrent queuers.
- **The contract is stored as its wire form** (`toWire`) and parsed on load; a stored contract that no longer parses is `StoreError` `query` with the schema's message, since only a migration could have made it so.
- **Events** (each a schema under `docs/schemas/events/`, `subject_id` the cycle's id): `cycle.queued` `{ number, program, intent, priority }`; `cycle.started` `{ number }`; `cycle.finished` `{ number, summary, gave_up }`; `cycle.stopped` `{ number, reason }`; `cycle.cancelled` `{ number, reason }`.
- **`saveCycle(tx, cycle, event)`** writes the cycle's mutable columns and appends the event in the caller's transaction; the caller builds the event from the transition.

## File map

```
packages/core/src/job-contract.ts, job-contract.test.ts        creates, tests: validateJob, RECORD_TOOLS (Task 1)
packages/core/src/cycle.ts, cycle.test.ts                      creates, tests: Cycle, transitions, attachSession, canFinish (Task 2)
packages/core/src/fixtures/cycle.ts, fixtures/index.ts         creates: aResearchContract, aQueuedCycle (Task 2)
packages/core/src/index.ts                                     modifies (Tasks 1, 2)
packages/store/migrations/0002_cycles.sql                      creates (Task 3)
docs/schemas/events/cycle.*.schema.json (five files)           creates (Task 3)
packages/schemas/src/generated/**                              generated (Task 3)
packages/store/src/database.ts, cycles.ts, index.ts            modifies; creates (Task 3)
packages/store/src/cycles.integration.test.ts                  tests (Task 3)
```

## Interfaces

Consumes: `Program`, `PROGRAM_TOOLS`, `ToolName` (step 01); `parse`, `toWire`, `JobContractDomain` (`@praeco/schemas`); `Store`, `WorkspaceTx`, `appendEvent`, `NewEvent`, `StoreError` (`@praeco/store`).

Produces:

```ts
// @praeco/core
export type RecordKind = 'finding';
export const RECORD_TOOLS: Readonly<Record<RecordKind, readonly ToolName[]>>;
export type JobContract = Omit<JobContractDomain, 'program' | 'expects'> & {
  program: Program; expects: readonly { kind: RecordKind; atLeast: number }[] };
export interface JobLimits { cycleUsdMicros: number }
export type JobRefusal =
  | { reason: 'invalid'; path: string; message: string }
  | { reason: 'tool_not_in_program'; tool: string; program: Program }
  | { reason: 'over_cycle_limit'; usdMicros: number; limitUsdMicros: number }
  | { reason: 'nothing_records'; kind: RecordKind };
export function validateJob(input: unknown, limits: JobLimits): Result<JobContract, JobRefusal>;
export type CycleStatus = 'queued' | 'running' | 'finished' | 'stopped' | 'cancelled';
export type StopReason = 'budget' | 'time' | 'tool_calls' | 'refused' | 'unfinished' | 'error' | 'paused';
export type CancelReason = 'by_operator' | 'expired';
export type Priority = 'request' | 'program';
export interface Cycle {
  id: string; workspaceId: string; number: number; contract: JobContract; priority: Priority;
  status: CycleStatus; stopReason: StopReason | null; cancelReason: CancelReason | null; sessionId: string | null;
  queuedAt: Date; startedAt: Date | null; endedAt: Date | null; heartbeatAt: Date | null;
  spentUsdMicros: number; summary: string | null; gaveUp: string | null };
export type CycleChange =
  | { to: 'running' }
  | { to: 'finished'; summary: string; gaveUp: string | null }
  | { to: 'stopped'; reason: StopReason }
  | { to: 'cancelled'; reason: CancelReason };
export interface TransitionRow { from: CycleStatus; to: CycleStatus; actor: 'scheduler' | 'governor' | 'operator' }
export const CYCLE_TRANSITIONS: readonly TransitionRow[];
export type TransitionRefusal = { reason: 'not_allowed'; from: CycleStatus; to: CycleStatus };
export function transitionCycle(cycle: Cycle, change: CycleChange, now: Date): Result<Cycle, TransitionRefusal>;
export function attachSession(cycle: Cycle, sessionId: string): Result<Cycle, { reason: 'not_running' | 'session_already_attached' }>;
export type RecordCounts = Readonly<Record<RecordKind, number>>;
export type FinishRefusal =
  | { reason: 'expectation_unmet'; kind: RecordKind; atLeast: number; recorded: number }
  | { reason: 'give_up_too_short' };
export function canFinish(contract: JobContract, counts: RecordCounts, gaveUp: string | null): Result<void, FinishRefusal>;
// @praeco/core/fixtures
export function aResearchContract(overrides?: Partial<JobContract>): JobContract;
export function aQueuedCycle(overrides?: Partial<Cycle>): Cycle;
// @praeco/store
export function queueCycle(tx: WorkspaceTx, contract: JobContract, input: { priority: Priority; eventId: string; now: Date }): Promise<Result<Cycle, StoreError>>;
export function loadCycle(tx: WorkspaceTx, id: string): Promise<Result<Cycle, StoreError>>;
export function saveCycle(tx: WorkspaceTx, cycle: Cycle, event: NewEvent): Promise<Result<void, StoreError>>;
export function listCycles(tx: WorkspaceTx, filter: { statuses?: readonly CycleStatus[]; limit?: number }): Promise<Result<Cycle[], StoreError>>;
export function cycleEvent(cycle: Cycle, eventId: string, actor: Actor, now: Date): NewEvent;   // the event for the cycle's current status
```

## Tasks

### Task 1: The job contract

Files: created `packages/core/src/job-contract.ts`, `job-contract.test.ts`; modified `packages/core/src/index.ts`.
Produces: `RecordKind`, `RECORD_TOOLS`, `JobContract`, `JobLimits`, `JobRefusal`, `validateJob`.

- `validateJob` — asserts that `aJobContractWire()` with `program: 'research'` and a limit of $1.50 is accepted; that adding `post_to_channel` to `allowed_tools` is refused `tool_not_in_program`; that `usd_micros: 2_000_000` against a $1.50 limit is refused `over_cycle_limit`; that removing `record_finding` while expecting findings is refused `nothing_records`; and that `intent: 'short'` is refused `invalid` at `/intent`.

- [ ] `feat(core): validate job contracts against their program and limits`

### Task 2: The cycle lifecycle

Files: created `packages/core/src/cycle.ts`, `cycle.test.ts`, `src/fixtures/cycle.ts`; modified `src/fixtures/index.ts`, `src/index.ts`.
Produces: the cycle types, `CYCLE_TRANSITIONS`, `transitionCycle`, `attachSession`, `canFinish`, `aResearchContract`, `aQueuedCycle`.

- `transitionCycle` follows SPEC 5.2 — asserts, over every pair of statuses, that exactly the four rows of `CYCLE_TRANSITIONS` succeed (setting `startedAt` and `heartbeatAt` to `now` for `running`, `endedAt` for the three endings) and every other pair is refused `not_allowed` with its `from` and `to`.
- `canFinish` — asserts that a contract expecting three findings finishes with three, is refused `expectation_unmet` with `recorded: 2` at two, finishes at two when given up with "The site has only two pages." and is refused `give_up_too_short` with "no".

- [ ] `feat(core): move cycles only along the lifecycle`

### Task 3: Cycles in the store

Files: created `packages/store/migrations/0002_cycles.sql`, the five `docs/schemas/events/cycle.*.schema.json`, `packages/store/src/cycles.ts`, `src/cycles.integration.test.ts`; modified `packages/store/src/database.ts`, `src/index.ts`; generated `packages/schemas/src/generated/**`.
Produces: `queueCycle`, `loadCycle`, `saveCycle`, `listCycles`, `cycleEvent`.

- `cycles` — asserts that three cycles queued in one workspace get numbers 1, 2, 3 and a `cycle.queued` event each; that a cycle saved as `running` reloads equal to what was saved; that saving a second `running` cycle in the same workspace fails with Postgres error `23505`, while a running cycle in another workspace succeeds; and that `listCycles` with `statuses: ['queued']` returns only the queued ones, oldest first.

- [ ] `feat(store): keep cycles and number them per workspace`

## Verification

```
pnpm check --integration
# expected, last line: check: ok
```
