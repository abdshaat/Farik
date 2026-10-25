# 0005. Postgres is the only store, one schema per format, and one mapping at each edge

Date: 2026-10-24
Status: proposed

## Context

Catervas keeps its records in files committed to a repository and its event log in a SQLite file on one computer (Catervas SPEC 8.4). Praeco serves many people and, hosted, many companies, from more than one process. The options: Postgres; SQLite per workspace on a shared volume; a document store. Postgres gives transactions across the queue (ADR 0006) and the records, row-level security for tenancy, and runs everywhere a self-hoster would run Praeco.

## Decision

- **Postgres 18** is the only store, self-hosted and hosted. Queries go through Kysely; schema changes are numbered SQL files in `packages/store/migrations/` applied by Kysely's migrator from `praeco migrate`, never edited after they land.
- **Tenancy:** every table but the installation's own has `workspace_id uuid not null`; row-level security policies compare it with `current_setting('praeco.workspace_id')`. Praeco connects with one database URL whose user owns the schema (the installation role). Every workspace transaction (`Store.inWorkspace`) begins with `SET LOCAL ROLE praeco_app` (a `NOLOGIN` role the first migration creates, which owns nothing, so the policies bind it) and `SET LOCAL praeco.workspace_id`. Work that is the installation's (migrations, creating a workspace, the scheduler's list of workspaces, deleting one) goes through `Store.asInstallation`, which is named for what it bypasses. The store exports no other way to run a query.
- **The event log** is the table `events`, append-only: a trigger refuses every `UPDATE`, and every `DELETE` but those of a workspace's rows inside the one transaction that deletes the workspace, which sets `praeco.deleting_workspace` to its id.
- **Formats:** every format that leaves a process or is stored as JSON has a JSON Schema (2020-12) in `docs/schemas/`. `pnpm generate` turns each into TypeScript types and an Ajv validator in `packages/schemas/src/generated/`, committed; `pnpm check` fails when they are stale.
- **Naming:** wire, files and columns are `snake_case`; TypeScript is `camelCase`. Two mapping layers and no others: `@praeco/schemas` (`parse<T>` validates a wire value, then maps its keys to camelCase; `toWire` maps back) and `@praeco/store` (Kysely's `CamelCasePlugin`).
- **Money** is integer micro-dollars (`usd_micros`); **time** is `timestamptz`, ISO 8601 UTC on the wire; **ids** are UUID v7 from an injected `Ids`; a job's human-readable number is `J-<n>`, counted per workspace.

## Consequences

One store to back up, one transaction around a tool's effect and its event, and tenancy enforced below the code. A workspace's data is exportable and deletable by `workspace_id`.

What cuts against it: integration tests need Postgres in Docker (`pnpm check --integration`, via Testcontainers), so the store's tests do not run in the default check; and row-level security is one more thing to get right, which is why it has its own test from phase 0 step 03.
