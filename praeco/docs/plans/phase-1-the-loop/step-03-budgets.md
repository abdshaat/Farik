# Phase 1, step 03: Budgets

Status: draft
Branch: `phase/1-the-loop`
Spec: `docs/SPEC.md` 5.5; F7
Depends on: phase 0 (merged); steps 01 and 02 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0013, ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

A workspace has a month's limit, a day's limit and a cycle's limit, nested and in whole cents. Code decides whether a cycle may start (the day and the month have room for its whole limit, the workspace is not paused, a month's limit exists) and whether a running cycle must stop (it reached its limit). The store keeps the limits, sums what cycles started in a period spent, and keeps each cycle's spend as the session last reported it.

Out of scope: reading the session's cost (step 04), refusing to claim a cycle (step 06), paid allowances (phase 9), the digest's notice (phase 2).

## Decisions

- **Money** is integer micro-dollars throughout; the session budget and `usage.list_cost` are cents (ADR 0013), converted by `centsToMicros` (× 10 000) and `microsToCents` (exact division; a value not a whole number of cents is refused where it is set, so it never needs rounding).
- **`deriveLimits(input)`**:
  - `monthUsdMicros` null: refused `no_month_limit`.
  - Any value not positive: `not_positive` naming the field. Any value not a multiple of 10 000: `not_whole_cents` naming it.
  - `dayUsdMicros` null: the month's thirtieth, rounded down to whole cents, at least one cent.
  - `cycleUsdMicros` null: `DEFAULT_CYCLE_USD_MICROS` (1 500 000) or the day's limit, whichever is smaller.
  - Day above month: `day_over_month`; cycle above day: `cycle_over_day`.
- **`canStartCycle(limits, spent, isPaused)`**, in this order: no limits (`null`): `no_month_limit`; paused: `paused`; `spent.dayUsdMicros + limits.cycleUsdMicros > limits.dayUsdMicros`: `day_reached`; the same for the month: `month_reached`. Each spend refusal carries `spentUsdMicros` and `limitUsdMicros`.
- **`cycleSpendDecision(limit, spent)`**: `'stop'` when `spent >= limit`, else `'continue'`.
- **Periods** (`periodStarts(now, timeZone)`): the start of the day and of the month in the workspace's IANA time zone, as instants, computed with `Intl.DateTimeFormat` (`formatToParts`, then the offset found by comparing the zone's wall time with UTC), so that a day across a daylight-saving change has 23 or 25 hours. An unknown zone is a programmer error here: `workspaces.time_zone` is checked when it is set (phase 1 step 07, `Intl.supportedValuesOf('timeZone')`).
- **What counts in a period:** the spend of every cycle whose `started_at` is at or after the period's start. A cycle that runs across midnight counts in the day it started. Rejected: a spend ledger with a row per usage event, which costs a write per model request to move a cent across midnight.
- **`0003_budgets.sql`**: `budgets (workspace_id uuid primary key references workspaces (id) on delete cascade, month_usd_micros bigint not null, day_usd_micros bigint not null, cycle_usd_micros bigint not null, updated_at timestamptz not null)`; row-level security `budgets_in_workspace`; grants `select, insert, update` to `praeco_app`. A missing row means no limits.
- **`recordCycleSpend(tx, cycleId, totalUsdMicros)`** sets `spent_usd_micros` to `greatest(spent_usd_micros, $2)`, since the session reports a running total that only grows.
- **`noteBudgetReached(tx, refusal, eventId, now, timeZone)`** appends `budget.reached` `{ reason, spent_usd_micros, limit_usd_micros }` unless one with the same reason was appended since the period's start (the day's for `day_reached`, the month's for `month_reached`), and says whether it did. `paused` and `no_month_limit` are not budget events and are refused by the type.
- **Events:** `budget.set` `{ month_usd_micros, day_usd_micros, cycle_usd_micros }` (actor as given), `budget.reached` as above.

## File map

```
packages/core/src/budget.ts, budget.test.ts               creates, tests: limits, canStartCycle, cycleSpendDecision, cents (Task 1)
packages/core/src/periods.ts, periods.test.ts             creates, tests: periodStarts (Task 1)
packages/core/src/index.ts                                modifies (Task 1)
packages/store/migrations/0003_budgets.sql                creates (Task 2)
docs/schemas/events/budget.set.schema.json, budget.reached.schema.json   creates (Task 2)
packages/schemas/src/generated/**                         generated (Task 2)
packages/store/src/budgets.ts, database.ts, index.ts       creates; modifies (Task 2)
packages/store/src/budgets.integration.test.ts            tests (Task 2)
```

## Interfaces

Consumes: `Result` (`@praeco/core`); `Cycle` (step 02); `WorkspaceTx`, `appendEvent`, `Actor`, `StoreError` (`@praeco/store`).

Produces:

```ts
// @praeco/core
export interface Limits { monthUsdMicros: number; dayUsdMicros: number; cycleUsdMicros: number }
export interface LimitsInput { monthUsdMicros: number | null; dayUsdMicros: number | null; cycleUsdMicros: number | null }
export const DEFAULT_CYCLE_USD_MICROS: 1_500_000;
export type LimitsRefusal =
  | { reason: 'no_month_limit' } | { reason: 'not_positive'; field: keyof LimitsInput }
  | { reason: 'not_whole_cents'; field: keyof LimitsInput } | { reason: 'day_over_month' } | { reason: 'cycle_over_day' };
export function deriveLimits(input: LimitsInput): Result<Limits, LimitsRefusal>;
export interface Spent { dayUsdMicros: number; monthUsdMicros: number }
export type SpendRefusal = { reason: 'day_reached' | 'month_reached'; spentUsdMicros: number; limitUsdMicros: number };
export type BudgetRefusal = { reason: 'no_month_limit' } | { reason: 'paused' } | SpendRefusal;
export function canStartCycle(limits: Limits | null, spent: Spent, isPaused: boolean): Result<void, BudgetRefusal>;
export function cycleSpendDecision(cycleLimitUsdMicros: number, spentUsdMicros: number): 'continue' | 'stop';
export function centsToMicros(cents: number): number;
export function microsToCents(micros: number): number;
export function periodStarts(now: Date, timeZone: string): { day: Date; month: Date };
// @praeco/store
export function setLimits(tx: WorkspaceTx, limits: Limits, input: { eventId: string; actor: Actor; now: Date }): Promise<Result<void, StoreError>>;
export function readLimits(tx: WorkspaceTx): Promise<Result<Limits | null, StoreError>>;
export function spentSince(tx: WorkspaceTx, since: Date): Promise<Result<number, StoreError>>;
export function recordCycleSpend(tx: WorkspaceTx, cycleId: string, totalUsdMicros: number): Promise<Result<void, StoreError>>;
export function noteBudgetReached(tx: WorkspaceTx, refusal: SpendRefusal, input: { eventId: string; now: Date; timeZone: string }): Promise<Result<boolean, StoreError>>;
```

## Tasks

### Task 1: Limits and decisions

Files: created `packages/core/src/budget.ts`, `budget.test.ts`, `periods.ts`, `periods.test.ts`; modified `src/index.ts`.
Produces: everything in `@praeco/core` above.

- `deriveLimits` — asserts that a $30 month gives a $1 day and a $1 cycle; that a $300 month gives a $10 day and a $1.50 cycle; that a day of $40 against a $30 month is `day_over_month`; that a cycle of $1.505 is `not_whole_cents` naming `cycleUsdMicros`; and that no month is `no_month_limit`.
- `canStartCycle` — asserts that with a $10 day, $1.50 cycle and $8.40 spent today a cycle may start, with $8.60 spent it is `day_reached` carrying both amounts, that a month with less room than one cycle is `month_reached`, that a paused workspace is `paused` before any spend check, and that no limits is `no_month_limit`.
- `periodStarts` — asserts that for `2026-03-08T12:00:00Z` in `America/New_York` the day starts at `2026-03-08T05:00:00Z` and the month at `2026-03-01T05:00:00Z`, and that for `2026-10-24T23:30:00Z` in `Asia/Tokyo` the day starts at `2026-10-24T15:00:00Z`.

- [ ] `feat(core): decide budgets as hard stops`

### Task 2: Budgets in the store

Files: created `packages/store/migrations/0003_budgets.sql`, the two event schemas, `packages/store/src/budgets.ts`, `src/budgets.integration.test.ts`; modified `src/database.ts`, `src/index.ts`; generated `packages/schemas/src/generated/**`.
Consumes: `queueCycle`, `saveCycle`, `transitionCycle` (step 02), the core functions (Task 1).
Produces: `setLimits`, `readLimits`, `spentSince`, `recordCycleSpend`, `noteBudgetReached`.

- `budgets` — asserts that `readLimits` is null before `setLimits` and equal to what was set after, with one `budget.set` event; that with cycles started yesterday ($2) and today ($1, then reported again as $3 and then $2.50) `spentSince(today's start)` is $3 and `spentSince(the month's start)` is $5; and that `noteBudgetReached` for `day_reached` returns true once and false the second time the same day.

- [ ] `feat(store): keep limits and sum spend per period`

## Verification

```
pnpm check --integration
# expected, last line: check: ok
```
