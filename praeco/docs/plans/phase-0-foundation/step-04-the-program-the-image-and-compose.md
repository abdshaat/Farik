# Phase 0, step 04: The program, the image and compose

Status: draft
Branch: `phase/0-foundation`
Spec: `docs/SPEC.md` 8.1, 8.6; F23
Depends on: steps 01 to 03 of this phase
Readiness confirmed by: its author, 2026-10-24, against the three rules of `docs/standards/workflow.md` stage 2 (Catervas ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (Catervas ADR 0008).

## Goal

`praeco` is a program with three commands: `praeco version` prints its version, `praeco migrate` applies the store's migrations, and `praeco serve` starts the HTTP server, whose `GET /health` answers `{"status":"ok","version":"0.0.0"}` while Postgres answers and `503 {"status":"unavailable"}` otherwise. One image builds from the root `Dockerfile`, and `docker compose up` starts Postgres, runs `migrate` once and then `serve` on port 8080.

Out of scope: `praeco work` and every other command (phase 1), the web app (phase 3).

## Decisions

- **Entry:** `apps/praeco/src/main.ts`, run as `node apps/praeco/src/main.ts <command>` (no build, step 01); `apps/praeco/package.json` has `"bin": { "praeco": "./src/main.ts" }` and version `0.0.0`, read with `import pkg from '../package.json' with { type: 'json' }`.
- **Commands** are parsed with `node:util` `parseArgs` (`strict: true`, `allowPositionals: true`); the first positional names the command. Unknown or missing: the usage text on stderr, exit 2. `runCommand(argv, io)` takes an `Io` (`stdout`, `stderr`, `env`, `clock`, `ids`, `signal`) so tests pass their own; `main.ts` builds the real one (`process.stdout`, `process.stderr`, `process.env`, a clock from `new Date()`, ids from `uuid` 14.0.3 `v7`, an `AbortSignal` aborted on `SIGTERM` and `SIGINT`) and exits with the returned code.
- **Config** (`loadConfig(env)`): `PRAECO_DATABASE_URL` required, a `postgres:` or `postgresql:` URL; `PRAECO_HOST` default `0.0.0.0`; `PRAECO_PORT` default `8080`, an integer 1 to 65535; `PRAECO_LOG_LEVEL` one of `fatal`, `error`, `warn`, `info`, `debug`, default `info`. Every problem is reported at once (`ConfigError` lists them), one per line on stderr, exit 2. Later phases add variables here, each with its default or its requirement.
- **`serve`**: Fastify 5.12.7 with its built-in logger at the config's level; `GET /health` runs `ping(store)` with a 2-second timeout. On the abort signal it closes Fastify, then the store, and returns 0.
- **`ping(store)`** is added to `@praeco/store`: `select 1` through `asInstallation`.
- **`migrate`** prints `applied <name>` per migration and `migrations: up to date` when none; exit 1 with the store's message on failure.
- **Image** (`Dockerfile`): `FROM node:24.14.0-bookworm-slim`; `corepack enable`; `WORKDIR /app`; copy the workspace's manifests (`package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, every `packages/*/package.json` and `apps/*/package.json`) and run `pnpm install --frozen-lockfile --prod`; then copy the rest; `USER node`; `ENTRYPOINT ["node", "apps/praeco/src/main.ts"]`; `CMD ["serve"]`; `EXPOSE 8080`. `.dockerignore`: `.git`, `node_modules`, `docs`, `**/*.test.ts`, `.env`. The workspace's packages stay symlinked into `node_modules` from their source folders, so Node strips their types (step 01).
- **Compose** (`docker-compose.yml`): `db` (`postgres:18.0`, `POSTGRES_DB=praeco`, `POSTGRES_PASSWORD` from `.env`, volume `praeco-db`, healthcheck `pg_isready -U postgres` every 5 s); `migrate` (`build: .`, `command: migrate`, `depends_on: db: condition: service_healthy`, `restart: "no"`); `serve` (`build: .`, `command: serve`, `depends_on: migrate: condition: service_completed_successfully`, `ports: ["8080:8080"]`). Both app services take `PRAECO_DATABASE_URL=postgres://postgres:${POSTGRES_PASSWORD}@db:5432/praeco`. `.env.example` holds `POSTGRES_PASSWORD=change-me`.
- **The image in the check:** `scripts/check.ts --integration` adds a last stage, `docker build -t praeco:check .`, so CI proves the image builds.

## File map

```
apps/praeco/package.json, tsconfig.json                       creates (Task 1)
apps/praeco/src/config.ts, config.test.ts                     creates, tests (Task 1)
apps/praeco/src/commands.ts, commands.test.ts                 creates, tests: runCommand, version, migrate (Task 1)
apps/praeco/src/main.ts                                       creates (Task 1)
packages/store/src/ping.ts, src/index.ts                      creates; modifies (Task 2)
apps/praeco/src/server.ts, server.integration.test.ts         creates, tests: buildServer, /health (Task 2)
apps/praeco/src/commands.ts                                   modifies: serve (Task 2)
Dockerfile, .dockerignore, docker-compose.yml, .env.example   creates (Task 3)
scripts/check.ts                                              modifies: docker build under --integration (Task 3)
```

## Interfaces

Consumes: `Clock`, `Ids`, `Result` (`@praeco/core`); `openStore`, `migrate`, `Store` (`@praeco/store`, step 03).

Produces:

```ts
// praeco (apps/praeco)
export interface Config { databaseUrl: string; host: string; port: number; logLevel: 'fatal' | 'error' | 'warn' | 'info' | 'debug' }
export interface ConfigError { problems: string[] }
export function loadConfig(env: Readonly<Record<string, string | undefined>>): Result<Config, ConfigError>;
export interface Io { stdout: NodeJS.WritableStream; stderr: NodeJS.WritableStream; env: Readonly<Record<string, string | undefined>>; clock: Clock; ids: Ids; signal: AbortSignal }
export function runCommand(argv: readonly string[], io: Io): Promise<number>;
export function buildServer(deps: { store: Store; version: string; logLevel: Config['logLevel'] }): FastifyInstance;
// @praeco/store
export function ping(store: Store): Promise<Result<void, StoreError>>;
```

## Tasks

### Task 1: The program and its config

Files: created `apps/praeco/package.json`, `tsconfig.json`, `src/config.ts`, `src/config.test.ts`, `src/commands.ts`, `src/commands.test.ts`, `src/main.ts` (dependencies `fastify` 5.12.7, `uuid` 14.0.3, `@praeco/core`, `@praeco/store`).
Produces: `loadConfig`, `Config`, `ConfigError`, `Io`, `runCommand` with `version` and `migrate`.

- `loadConfig` — asserts that an empty environment yields one problem naming `PRAECO_DATABASE_URL`, that `PRAECO_PORT=0` together with `PRAECO_LOG_LEVEL=loud` yields two problems in one `ConfigError`, and that a full environment yields the config with defaults applied.
- `runCommand` — asserts that `[]` and `['nope']` write the usage to stderr and return 2, and that `['version']` writes `0.0.0` and a newline to stdout and returns 0.

- [ ] `feat(praeco): add the program with version and migrate`

### Task 2: `serve` and its health

Files: created `packages/store/src/ping.ts`, `apps/praeco/src/server.ts`, `apps/praeco/src/server.integration.test.ts`; modified `packages/store/src/index.ts`, `apps/praeco/src/commands.ts`.
Consumes: `withTestStore` (`@praeco/store/fixtures`).
Produces: `ping`, `buildServer`, the `serve` command.

- `/health` — asserts, through Fastify's `inject`, that against a migrated test database it answers 200 with `{ status: 'ok', version: '0.0.0' }`, and after the store is closed it answers 503 with `{ status: 'unavailable' }`.

- [ ] `feat(praeco): serve http with a health check`

### Task 3: The image and compose

Files: created `Dockerfile`, `.dockerignore`, `docker-compose.yml`, `.env.example`; modified `scripts/check.ts`.

No test: the check's `docker build` stage and the verification below are its proof.

- [ ] `build(repo): build one image and run it with docker compose`

## Verification

```
pnpm check --integration
# expected, last lines: the docker build succeeds; check: ok
cp .env.example .env && docker compose up -d --build
docker compose logs migrate
# expected: applied 0001_installation
curl -s localhost:8080/health
# expected: {"status":"ok","version":"0.0.0"}
docker compose down -v
```
