# 0003. TypeScript end to end

Date: 2026-10-24
Status: proposed

## Context

Catervas is Rust behind a TypeScript web app (Catervas ADR 0005). Praeco is a server that calls web APIs (Anthropic, Slack, Notion, Google, GitHub, Exa) and a web app. Every one of those services ships an official TypeScript SDK; Slack's Bolt and Anthropic's SDK are first-class there. Rust would mean writing or wrapping clients for most of them, and two languages for one person to keep in their head.

## Decision

Praeco is TypeScript on Node.js 24.14.0 with pnpm 12.3.4, as Catervas's front end is (`.node-version`, `packageManager`). The toolchain is Catervas's front end's: TypeScript 7.0.2, Biome 2.5.14 for format and lint, Vitest 5.0.2 for tests, React 19.3.0, Vite 8.3.1 and React Router 8.4.0 for the web app, Playwright 1.64.1 for its browser tests. The server uses Fastify 5.12.7; the database Kysely 0.29.7 over `pg` 8.23.1; ids `uuid` 14.0.3 (version 7); validation Ajv 8.20.0 over JSON Schemas with types from `json-schema-to-typescript` 16.0.0; the agent `@anthropic-ai/sdk` 1.3.0; Slack `@slack/bolt` 5.1.0; Notion `@notionhq/client` 5.28.0. Every version is exact, in every `package.json`, and an upgrade is its own `chore` commit. Phase 0 step 01 installs the first of them together and records the day the check ran green; one that fails is replaced in that step's plan by the newest that passes.

`pnpm check` is the full check (format, lint, types, unit tests, generated files fresh, bare TODOs, `@praeco/core` imports no I/O), and `pnpm check --integration` adds the tests that need Docker (Postgres). `@praeco/core` imports nothing from `node:fs`, `node:net`, `node:http`, `node:child_process`, `pg`, `kysely`, `@anthropic-ai/sdk` or any other package that does I/O, and reads no clock and generates no id on its own: time and ids are passed in.

## Consequences

One language, one test runner, one lockfile; the SDKs do the protocol work.

What cuts against it: TypeScript's types vanish at run time, so every value that crosses a process boundary is validated against its schema before it is used (ADR 0005's mapping layer does it); and the `@anthropic-ai/sdk` 1.x line is days old on this date, so phase 0 step 01 may pin the newest 0.x instead if 1.3.0 fails its check.
