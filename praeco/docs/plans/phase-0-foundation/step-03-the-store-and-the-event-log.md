# Phase 0, step 03: The store and the event log

Status: draft
Branch: `phase/0-foundation`
Spec: `docs/SPEC.md` 8.3, 8.4, 8.5
Depends on: steps 01 and 02 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

`@praeco/store` opens Postgres, applies migrations, creates workspaces, and appends and reads events. Every query runs either inside `inWorkspace(id, …)`, as the role `praeco_app` restricted by row-level security to that workspace, or inside `asInstallation(…)`, which is named for what it bypasses; the store offers no third way. The event log refuses updates and deletes. Three integration tests prove migrations, tenancy and append-only.

Out of scope: the program and `praeco migrate` as a command (step 04), cycles, budgets and everything else (phase 1).

## Decisions

- **Dependencies:** `kysely` 0.29.7, `pg` 8.23.1, `@types/pg` 8.23.1; tests `@testcontainers/postgresql` 12.2.1 with the image `postgres:18.0`.
- **Connection:** `openStore(url)` makes one `pg.Pool` (`max` 10, `idleTimeoutMillis` 30000) under Kysely's `PostgresDialect` with `CamelCasePlugin`, and checks it with `select 1`; a failure is `StoreError` `connection`.
- **Transactions and roles** (ADR 0005): `inWorkspace(id, work)` opens a transaction, runs `set local role praeco_app` and `select set_config('praeco.workspace_id', $1, true)`, then calls `work` with a `WorkspaceTx`; `asInstallation(work)` opens a transaction as the connecting user and calls `work` with an `InstallationTx`. Each is a distinct type carrying the Kysely transaction (`db`), so a function states which it needs. A `work` that throws rolls back and the error is returned as `StoreError` `query`; a `work` that returns `err(…)` also rolls back (the store checks the `Result`).
- **Migrations** are SQL files in `packages/store/migrations/` named `<nnnn>_<subject>.sql`, read by a `SqlFileMigrationProvider` and applied by Kysely's `Migrator` (its `kysely_migration` tables), each file in its own transaction as the connecting user. `migrate(url)` returns the names applied.
- **`0001_installation.sql`:**
  - The role: `do $$ begin if not exists (select from pg_roles where rolname = 'praeco_app') then create role praeco_app nologin; end if; end $$;` then `grant praeco_app to current_user;` (so the connecting user may `set role`) and `grant usage on schema public to praeco_app;`.
  - `workspaces (id uuid primary key, name text not null check (char_length(name) between 1 and 100), time_zone text not null default 'UTC', paused_at timestamptz, created_at timestamptz not null)`; row-level security on, policy `workspaces_in_workspace` for `praeco_app`: `using (id = current_setting('praeco.workspace_id')::uuid)`; grants `select, update (name, time_zone, paused_at)` to `praeco_app`.
  - `events (id uuid primary key, workspace_id uuid not null references workspaces (id) on delete cascade, seq bigint generated always as identity, kind text not null, actor_kind text not null check (actor_kind in ('praeco', 'agent', 'operator')), actor_id text, subject_id uuid, payload jsonb not null, recorded_at timestamptz not null)`; index `events_by_workspace (workspace_id, seq)`; row-level security on, policy `events_in_workspace` for `praeco_app` with the same `using` and `with check`; grants `select, insert` to `praeco_app`.
  - Trigger `events_refuse_change` (before update or delete, for each row) calling `events_refuse_change()`: an update raises `events are append-only`; a delete raises the same unless `current_setting('praeco.deleting_workspace', true) = old.workspace_id::text`.
  - Every later migration that adds a workspace table enables row-level security, adds the `<table>_in_workspace` policy and grants `praeco_app` only what it uses.
- **Events** (SPEC 8.4): `docs/schemas/event.schema.json` is the envelope (`id` uuid, `workspace_id` uuid, `kind` `pattern: ^[a-z_]+\.[a-z_]+$`, `actor` `{ kind: enum [praeco, agent, operator], id: string or null }`, `subject_id` uuid or null, `payload` object, `recorded_at` date-time). Each kind's payload is its own schema, `docs/schemas/events/<kind>.schema.json`; the first is `events/workspace.created.schema.json`: `{ name: string, time_zone: string }`. `appendEvent` refuses a kind with no payload schema (`StoreError` `unknown_event_kind`) and a payload its schema refuses (`invalid_event`).
- **The generator learns `events/`** (`scripts/generate.ts`): each `docs/schemas/events/<kind>.schema.json` becomes `generated/events/<kind with . as ->.ts` with `<Pascal of kind>PayloadWire` (for `workspace.created`, `WorkspaceCreatedPayloadWire`), and `generated/index.ts` gains the map type `EventPayloadWire` keyed by kind and the table `EVENT_PAYLOAD_SCHEMAS`; `@praeco/schemas` gains `parseEventPayload(kind, value)`. Rejected: one schema with a `oneOf` per kind, which repeats the envelope in every branch.
- **`createWorkspace`** inserts the row and appends `workspace.created` (actor `operator`) in the same installation transaction.
- **`readEvents`** filters by `kinds`, `subjectId` and `afterSeq`, orders by `seq`, and returns at most `limit` (default 100, at most 1000).
- **Test database:** `withTestStore` starts one container per test file (in `beforeAll`), runs `migrate`, and gives each test the store; tests make their own workspaces with fresh ids, so they never share rows.

## File map

```
packages/store/package.json, tsconfig.json                               creates (Task 1)
packages/store/migrations/0001_installation.sql                          creates (Task 1)
packages/store/src/database.ts                                           creates: Kysely table types (Tasks 1, 3)
packages/store/src/store.ts, migrate.ts, errors.ts, index.ts             creates: openStore, migrate, StoreError (Task 1)
packages/store/src/fixtures/index.ts, test-store.ts                      creates: withTestStore (Task 1)
packages/store/src/migrate.integration.test.ts                           tests (Task 1)
packages/store/src/tenancy.integration.test.ts                           tests (Task 2)
docs/schemas/event.schema.json, docs/schemas/events/workspace.created.schema.json   creates (Task 3)
scripts/generate.ts, scripts/generate.test.ts                            modifies, tests: events/ (Task 3)
packages/schemas/src/registry.ts, parse.ts, index.ts                     modifies: parseEventPayload (Task 3)
packages/schemas/src/generated/**                                        generated (Task 3)
packages/store/src/workspaces.ts, events.ts                              creates (Tasks 2, 3)
packages/store/src/events.integration.test.ts                            tests (Task 3)
```

## Interfaces

Consumes: `Result`, `ok`, `err`, `Ids` (`@praeco/core`, step 01); `parse`, `toWire` (`@praeco/schemas`, step 02).

Produces:

```ts
// @praeco/store
export type StoreError =
  | { kind: 'connection'; message: string }
  | { kind: 'query'; message: string; code: string | null }
  | { kind: 'not_found'; what: string; id: string }
  | { kind: 'unknown_event_kind'; eventKind: string }
  | { kind: 'invalid_event'; eventKind: string; path: string; message: string };
export interface WorkspaceTx { readonly kind: 'workspace'; readonly workspaceId: string; readonly db: Transaction<Database> }
export interface InstallationTx { readonly kind: 'installation'; readonly db: Transaction<Database> }
export interface Store {
  inWorkspace<T>(workspaceId: string, work: (tx: WorkspaceTx) => Promise<Result<T, StoreError>>): Promise<Result<T, StoreError>>;
  asInstallation<T>(work: (tx: InstallationTx) => Promise<Result<T, StoreError>>): Promise<Result<T, StoreError>>;
  close(): Promise<void>;
}
export function openStore(url: string): Promise<Result<Store, StoreError>>;
export function migrate(url: string): Promise<Result<{ applied: string[] }, StoreError>>;
export interface Workspace { id: string; name: string; timeZone: string; pausedAt: Date | null; createdAt: Date }
export function createWorkspace(tx: InstallationTx, input: { id: string; name: string; timeZone: string }, eventId: string, now: Date): Promise<Result<Workspace, StoreError>>;
export function listWorkspaces(tx: InstallationTx): Promise<Result<Workspace[], StoreError>>;
export type Actor = { kind: 'praeco' | 'agent' | 'operator'; id: string | null };
export interface NewEvent { id: string; kind: string; actor: Actor; subjectId: string | null; payload: unknown; recordedAt: Date }
export interface StoredEvent extends NewEvent { workspaceId: string; seq: number }
export interface EventFilter { kinds?: readonly string[]; subjectId?: string; afterSeq?: number; limit?: number }
export function appendEvent(tx: WorkspaceTx | InstallationTx & { workspaceId: string }, event: NewEvent): Promise<Result<StoredEvent, StoreError>>;
export function readEvents(tx: WorkspaceTx, filter: EventFilter): Promise<Result<StoredEvent[], StoreError>>;
// @praeco/store/fixtures
export function withTestStore(): { store(): Store; url(): string };   // registers beforeAll/afterAll
// @praeco/schemas
export function parseEventPayload(kind: string, value: unknown): Result<unknown, SchemaError | { kind: 'unknown_event_kind'; eventKind: string }>;
```

`appendEvent`'s first parameter accepts an installation transaction only together with the workspace it writes for, which `createWorkspace` uses; every other caller passes a `WorkspaceTx`.

## Tasks

### Task 1: Opening and migrating

Files: created `packages/store/package.json`, `tsconfig.json`, `migrations/0001_installation.sql`, `src/database.ts`, `src/store.ts`, `src/migrate.ts`, `src/errors.ts`, `src/index.ts`, `src/fixtures/index.ts`, `src/fixtures/test-store.ts`, `src/migrate.integration.test.ts`.
Produces: `openStore`, `migrate`, `Store`, `StoreError`, `WorkspaceTx`, `InstallationTx`, `withTestStore`.

- `migrate` — asserts that on an empty database it applies `0001_installation`, that a second run applies nothing, and that `praeco_app` exists and cannot log in.

- [ ] `feat(store): open postgres and apply migrations`

### Task 2: Workspaces and tenancy

Files: created `src/workspaces.ts`, `src/tenancy.integration.test.ts`; modified `src/index.ts`.
Produces: `createWorkspace` (its event is written by Task 3; until then it inserts the row only), `listWorkspaces`, `Workspace`.

- `tenancy` — asserts that, with workspaces A and B and an event row inserted for A through `asInstallation`, `inWorkspace(B)` reads no events and only workspace B from `workspaces`, and that an `insert into events` naming A's id run through `inWorkspace(B)`'s `db` is refused by the policy (Postgres error `42501`). This is the security guarantee of SPEC 8.5.

- [ ] `feat(store): create workspaces and confine queries to one`

### Task 3: The event log

Files: created `docs/schemas/event.schema.json`, `docs/schemas/events/workspace.created.schema.json`, `packages/store/src/events.ts`, `src/events.integration.test.ts`; modified `scripts/generate.ts`, `scripts/generate.test.ts`, `packages/schemas/src/registry.ts`, `parse.ts`, `index.ts`, `packages/store/src/workspaces.ts` (appends `workspace.created`), `src/index.ts`; generated `packages/schemas/src/generated/**`.
Consumes: `parse` (step 02), `createWorkspace` (Task 2).
Produces: `appendEvent`, `readEvents`, `NewEvent`, `StoredEvent`, `EventFilter`, `Actor`, `parseEventPayload`.

- `renderAll` (extended) — asserts that a folder with `events/thing.happened.schema.json` yields `generated/events/thing-happened.ts` with `ThingHappenedPayloadWire` and an `EventPayloadWire` entry `'thing.happened'`.
- `events` — asserts that `createWorkspace` leaves one `workspace.created` event whose payload holds the name and time zone, that `appendEvent` of an unknown kind returns `unknown_event_kind`, and that an `update events …` and a `delete from events …` through `inWorkspace`'s `db` both fail with `events are append-only`, while deleting the workspace through `asInstallation` with `praeco.deleting_workspace` set to its id removes it and its events.

- [ ] `feat(store): append and read workspace events`

## Verification

```
pnpm check
# expected, last line: check: ok
pnpm check --integration
# expected: the three integration test files pass; last line: check: ok
```
