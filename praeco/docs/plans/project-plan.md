# Praeco project plan

Status: draft, revision 1 (2026-10-24). The founder's brief: a marketing agent "that lives inside the workspace they use", "easily configurable and learns by itself", in a public repository with "a fully hosted option", "long running and working constantly either on scraping the web about the product or the problem it's trying to solve, collect potential customers, and optimize SEO for the product". Its decisions are ADRs 0001 to 0013 and `docs/SPEC.md` 0.1, all proposed until the founder accepts them; no step starts before they are accepted.
Owner: project founder.
Decision changes: each pull request that edits this plan says which decision changed and why; the history is `git log -- docs/plans/project-plan.md`.

This document divides the project into phases and each phase into steps (Catervas ADR 0003). A phase ends in something a person can use or verify, and is one pull request from `phase/<n>-<name>`. A step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, written from `docs/plans/step-template.md`, and lands as a group of commits on the phase branch. Steps run in the order listed; a step depends only on steps above it in its phase and on phases already merged. The rules are in `docs/standards/workflow.md` stage 2.

Step plans exist for phases 0 and 1, the two this revision plans. A later phase's step plans are written when the phase before it merges, from that phase's rows below and the code then on `main`, so that their signatures are the code's and not a guess (Catervas's rule, "written for the phase being planned next, not for phases further out, where they would go stale before use").

Milestones are in `docs/SPEC.md` section 11: M0 at the end of phase 1, M1 at the end of phase 8, M2 at the end of phase 10.

## How to read a phase

"Ends with" is what a person can use or verify when the phase merges. "Decisions" lists what the phase rests on. "Steps" is the ordered table. "Interfaces this phase adds" lists, for phases 0 and 1, the public items each step produces, abbreviated (the step plan has them exact), so that the no-forward-dependency rule can be checked by reading. Where a later step changes what an earlier interface is passed, the later step adds the field; the earlier step never leaves a placeholder.

## Decisions that apply to every phase

- Workflow, planning, models, review: ADR 0001, `docs/standards/workflow.md`.
- Public repository, Apache 2.0 outside `ee/`, hosted operations in `praeco-ops`: ADR 0002.
- TypeScript, the toolchain, exact versions, `pnpm check`: ADR 0003, `docs/standards/code.md`.
- The agent on Claude Managed Agents; every effect outside the sandbox is a Praeco tool the governor decides: ADR 0004.
- Postgres, tenancy by row-level security, the append-only event log, schemas own their types, one mapping per edge: ADR 0005.
- Bounded cycles, the `cycles` table as the queue: ADR 0006.
- Slack the front door, the web app for settings and records, Notion a destination: ADR 0007. People and roles: ADR 0008.
- Sources within their terms: ADR 0009. Prospects without contact data, never contacted: ADR 0010. SEO by pull request: ADR 0011. The playbook: ADR 0012. Budgets as hard stops: ADR 0013.
- `@praeco/core` does no I/O, checked by `scripts/core-io.ts` from phase 0 step 01.
- Time and ids are injected: `Clock` (`now(): Date`) and `Ids` (`uuid(): string`, version 7) from `@praeco/core`; nothing below `apps/` reads the clock or makes an id on its own.
- Tests: unit (`*.test.ts`, in `pnpm check`), integration (`*.integration.test.ts`, Postgres in Docker through Testcontainers 12.2.1 and the `postgres:18.0` image, in `pnpm check --integration`, which CI runs as its one job), live (`*.live.test.ts`, Anthropic and other real services, run by hand with `PRAECO_LIVE_TESTS=1`, never in CI).
- Event kinds and schemas grow with the code: the step that first emits a kind or first writes a format adds it to its schema in `docs/schemas/`.
- Catervas material is copied, never linked at build time: a skill or design copied from `abdshaat/farik` names its source path and commit in its first lines.

| Schema | First generated in |
|---|---|
| `job-contract.schema.json` | phase 0 step 02 |
| `event.schema.json` | phase 0 step 03 |
| `tool-finish-cycle.schema.json` | phase 1 step 04 |
| `tool-read-page.schema.json`, `tool-record-finding.schema.json`, `tool-update-product-profile.schema.json` | phase 1 step 05 |
| `events/<kind>.schema.json`, one per event kind | the step that first emits the kind |

| Package | Directory | Created in |
|---|---|---|
| `@praeco/core` | `packages/core` | phase 0 step 01 |
| `@praeco/schemas` | `packages/schemas` | phase 0 step 02 |
| `@praeco/store` | `packages/store` | phase 0 step 03 |
| `praeco` (the program) | `apps/praeco` | phase 0 step 04 |
| `@praeco/agent` | `packages/agent` | phase 1 step 04 |
| `@praeco/tools` | `packages/tools` | phase 1 step 05 |
| `@praeco/slack` | `packages/slack` | phase 2 step 01 |
| `@praeco/web` | `apps/web` | phase 3 step 03 |
| `ee/billing` | `ee/billing` | phase 10 step 01 |

## Phase 0: Foundation

Ends with: a pnpm workspace where `pnpm check` runs generated-file freshness, Biome, the type check, the unit tests, the bare-TODO check and the core no-I/O check, locally and in CI; the job contract's schema produces types and a validator; Postgres holds workspaces and an append-only event log with tenancy proven by a test; and `docker compose up` starts `praeco serve`, whose `/health` answers.

Decisions for this phase, all in its step plans: the root files and scripts (step 01), the generator and the mapping (step 02), roles, policies and the trigger (step 03), the image and the compose file (step 04).

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | The workspace and the check | 8.1 | pnpm workspace, toolchain, `pnpm check`, hooks, CI, `@praeco/core` with `Result`, `Clock`, `Ids` |
| 02 | Schemas own their types | 5.3, 8.3 | `docs/schemas/`, `pnpm generate`, `@praeco/schemas` with `parse`, `toWire`, the job contract's schema |
| 03 | The store and the event log | 8.3, 8.4, 8.5 | Postgres, migrations, `inWorkspace` and `asInstallation`, workspaces, `events`, the tenancy test |
| 04 | The program, the image and compose | 8.1, 8.6, F23 | `apps/praeco` with `migrate`, `serve` (health), `version`; `Dockerfile`; `docker-compose.yml` |

### Interfaces this phase adds

- **Step 01** (`@praeco/core`): `type Result<T, E>`, `ok(value)`, `err(error)`, `interface Clock { now(): Date }`, `interface Ids { uuid(): string }`, fixtures `fixedClock(at)`, `sequentialIds()`. Scripts: `findBareTodos`, `findCoreIo`, `checkCommitMessage`.
- **Step 02** (`@praeco/schemas`): `CamelKeys`, `toCamel`, `toSnake`, `SchemaName = 'job_contract'`, `Domain`, `SchemaError`, `parse(name, value)`, `toWire(name, value)`, `JobContractWire`, `JobContractDomain`; fixture `aJobContractWire`. Scripts: `renderAll`, `findStale`.
- **Step 03** (`@praeco/store`): `openStore(url): Promise<Result<Store, StoreError>>`; `Store.inWorkspace(id, work)`, `Store.asInstallation(work)`, `Store.close()`; `migrate(url)`; `createWorkspace(tx, input, eventId, now)`, `listWorkspaces(tx)`; `appendEvent(tx, event)`, `readEvents(tx, filter)`; fixture `withTestStore(work)`. Schemas `event.schema.json` and `events/workspace.created.schema.json`; `parseEventPayload` in `@praeco/schemas`.
- **Step 04** (`praeco`): `loadConfig(env): Result<Config, ConfigError>`; `Io`; `runCommand(argv, io): Promise<number>`; `buildServer(deps): FastifyInstance` with `GET /health`. (`@praeco/store`): `ping(store)`.

## Phase 1: The loop (M0)

Ends with: on one computer with Docker and an Anthropic API key, `praeco workspace create --name Acme --site https://acme.example --month-budget 30` then `praeco agent sync` and `praeco work` make Praeco research the product continuously in bounded cycles, every tool call decided by the governor and logged, stopping at the day's budget; `praeco cycles`, `praeco findings` and `praeco events` show what it did; `praeco pause` stops it.

Decisions for this phase:

- Phase 1 has two programs, `research` (learning the product from its own site, every 60 minutes by default) and `request` (a cycle queued by hand with `praeco ask`). Phase 4 replaces `research`'s instructions with the foundation and adds continuous research.
- Phase 1's Praeco tools: `read_page` (read), `record_finding` and `update_product_profile` (record), `finish_cycle` (control). Built-ins: the file tools and `web_search`. No outward or paid tool exists yet; the governor's `ask` path is tested with a test table.
- The operator is the person at the command line, recorded as the actor `operator` until phase 2 adds members.
- `ANTHROPIC_API_KEY` is read by the Anthropic SDK as it documents; it is the one environment variable without the `PRAECO_` prefix.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | The tools and the governor | 5.1, 5.4, 5.8, F6 | `TOOL_RULES`, `PROGRAM_TOOLS`, `decideToolCall`, the blocked hosts, in core |
| 02 | Job contracts and the cycle lifecycle | 5.2, 5.3, F4, F5 | `validateJob`, the transition table, `canFinish`; the `cycles` table |
| 03 | Budgets | 5.5, F7 | limits, `canStartCycle`, `cycleSpendDecision`, periods in the workspace's time zone; the `budgets` table |
| 04 | Running a cycle on Managed Agents | 8.2, F5, F6 | `@praeco/agent`: the port, the SDK adapter, the fake, `syncAgent`, `runCycle` with `finish_cycle` |
| 05 | The research tools and the knowledge store | 5.8, 6.1, F3, F12 | `@praeco/tools`: `read_page`, `record_finding`, `update_product_profile`; `pages`, `findings`, `product_profiles`; the read-only `knowledge` store |
| 06 | The scheduler and `praeco work` | 5.2, F8 | `dueCycles`, claiming, heartbeats, the reaper, pause; `praeco work` |
| 07 | The command line and the first live run | 4.7, 11 (M0) | `workspace create`, `agent sync`, `ask`, `cycles`, `findings`, `events`, `pause`, `resume`, `budget set`; the live test |

### Interfaces this phase adds

- **Step 01** (`@praeco/core`): `Tier`, `ToolName`, `ToolRule`, `TOOL_RULES`, `Program = 'research' | 'request'`, `PROGRAM_TOOLS`, `ToolContext`, `ToolDecision`, `ToolRefusal`, `decideToolCall(call, context, rules?)`, `refusalMessage`, `REFUSALS_BEFORE_STOP`, `shouldStopForRefusals`, `BUILT_IN_BLOCKED_HOSTS`, `isBlockedHost(host, extra)`.
- **Step 02** (`@praeco/core`): `RecordKind = 'finding'`, `RECORD_TOOLS`, `JobContract`, `JobLimits`, `JobRefusal`, `validateJob(input, limits)`; `CycleStatus`, `StopReason`, `CancelReason`, `Priority`, `Cycle`, `CycleChange`, `CYCLE_TRANSITIONS`, `transitionCycle(cycle, change, now)`, `attachSession`, `RecordCounts`, `canFinish(contract, counts, gaveUp)`; fixtures `aResearchContract`, `aQueuedCycle`. (`@praeco/store`): `queueCycle`, `loadCycle`, `saveCycle`, `listCycles`, `cycleEvent`; events `cycle.queued`, `cycle.started`, `cycle.finished`, `cycle.stopped`, `cycle.cancelled`.
- **Step 03** (`@praeco/core`): `Limits`, `LimitsInput`, `DEFAULT_CYCLE_USD_MICROS`, `deriveLimits`, `Spent`, `BudgetRefusal`, `SpendRefusal`, `canStartCycle(limits, spent, isPaused)`, `cycleSpendDecision`, `centsToMicros`, `microsToCents`, `periodStarts(now, timeZone)`. (`@praeco/store`): `setLimits`, `readLimits`, `spentSince`, `recordCycleSpend`, `noteBudgetReached`; events `budget.set`, `budget.reached`.
- **Step 04** (`@praeco/agent`): `ManagedAgents` (`syncAgent`, `createSession`, `events`, `sendToolResult`, `sendMessage`, `interrupt`, `archive`), `SessionEvent`, `SessionInput`, `AgentError`, `AgentDefinition`, `anthropicManagedAgents(client)`, `FakeManagedAgents`, `loadAgentDefinition`, `loadProgramText`, `renderInstructions`, `ToolExecutor`, `ToolOutcome`, `ToolCallContext`, `CycleDeps`, `CycleOutcome`, `runCycle(deps, cycle)`. (`@praeco/store`): `AgentRef`, `readAgentRef`, `saveAgentRef`, `readWorkspace`. Schema `tool-finish-cycle`; events `tool.allowed`, `tool.refused`, `tool.asked`, `tool.completed`, `tool.failed`, `spend.recorded`.
- **Step 05** (`@praeco/tools`): `readPage`, `ReadPageDeps`, `researchExecutor(deps)`, `ResearchDeps`, `syncKnowledge(agents, store, workspaceId)`. (`@praeco/store`): `Page`, `recordPage`, `findPage`, `Finding`, `insertFinding`, `listFindings`, `ProductProfile`, `readProductProfile`, `writeProductProfile`, `setProfileField`, `recordCounts`, `setKnowledgeStoreId`, `Workspace.knowledgeStoreId`. (`@praeco/agent`, additions): `ManagedAgents.ensureMemoryStore`, `ManagedAgents.writeMemory`, `SessionInput.resources`, `CycleDeps.afterCycle`. Schemas `tool-read-page`, `tool-record-finding`, `tool-update-product-profile`; events `finding.recorded`, `product_profile.updated`.
- **Step 06** (`@praeco/core`): `ProgramSetting`, `DEFAULT_PROGRAMS`, `ProgramState`, `ContractDraft`, `programContract`, `dueCycles(programs, limits, now)`. (`@praeco/store`): `readProgramStates`, `listClaimable`, `claimNextCycle`, `beat`, `reapStale`, `setPaused`; events `workspace.paused`, `workspace.resumed`. (`praeco`): `WorkerDeps`, `tick`, `runWorker(deps, signal)`, the `work` command.
- **Step 07** (`praeco`): `parseUsd`, `formatUsd`, `resolveWorkspace`; the commands `workspace create|list`, `agent sync`, `ask`, `cycles`, `findings`, `events`, `pause`, `resume`, `budget show|set`; `Io.agents()`. (`@praeco/agent/fixtures`): `FakeManagedAgents.fromRecording`.

## Phase 2: Slack and people

Ends with: a company installs Praeco into its Slack workspace, signs in with Slack, and talks to Praeco: a mention or direct message becomes a request cycle answered in its thread; approvals arrive with buttons only approvers can press; the daily digest is posted in its channel; `@Praeco pause` and `resume` work.

Decisions: ADRs 0007 and 0008; SPEC open question 1 is answered (Slack's terms and Marketplace review read against the design) before step 03 starts; the Slack app manifest is `slack-manifest.yaml` at the root, with scopes `app_mentions:read`, `chat:write`, `im:history`, `im:read`, `im:write`, `channels:join`, `channels:read`, `channels:manage`, `users:read` and nothing that reads a channel's history.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | The Slack app and its install | 4.1, 8.5 | `@praeco/slack`; manifest; OAuth v2 install; tokens encrypted with `PRAECO_SECRET_KEY`; Socket Mode or HTTP by config |
| 02 | Members and sign-in | 3, F1, F2 | `members`, roles, Sign in with Slack (OIDC), sessions, the last-owner rule; the actor `member` |
| 03 | Requests from Slack | 4.3, F9 | mentions and direct messages become `request` cycles; the reply in thread; only the asking message's text is stored |
| 04 | Approvals | 4.4, 5.6, F10 | `approvals` table; the `ask` decision executed on approval; Block Kit buttons; approver-only; seven-day expiry; first outward tool `post_to_channel` |
| 05 | The digest and pausing from Slack | 4.2, 4.7, F11 | the 09:00 digest in the workspace's time zone; `pause`, `resume`, `status` |
| 06 | The live check in a real Slack | 11 | a runbook the founder runs in a test Slack workspace |

## Phase 3: The web app

Ends with: an owner sets Praeco up and runs it from the browser: the setup of SPEC 4.1, the connections page, members and roles, rules (programs on or off, cadences, the blocked list), budgets, the activity log, and the findings, on the self-hosted server and behind `praeco serve`.

Decisions: React, Vite and React Router as ADR 0003; the API is JSON over HTTP under `/api`, one schema per request and response in `docs/schemas/api/`; the session cookie of ADR 0008; Catervas's brand is not reused (Praeco gets its own in step 01's mockups).

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | Every page mocked up | 4 | Opus draws each page in HTML; the founder approves them before step 03 |
| 02 | The API | 8.5 | Fastify routes, schemas, the `Origin` check, role checks per route |
| 03 | The shell | 4.1 | `@praeco/web`: sign in, layout, navigation, served by `praeco serve` |
| 04 | Setup | 4.1 | product, site, channel, budget; queues the research |
| 05 | Connections | F22 | the sign-in framework and its first user: Notion (read pages, write under one parent) |
| 06 | Activity, findings and budget | F21 | the event log filtered; findings by topic; spend against limits |
| 07 | Rules and members | 5.4, F2 | programs, cadences, the blocked list, approval mode; roles |

## Phase 4: Research

Ends with: the research foundation runs at setup and every 92 days, continuous research runs daily (competitors' changed pages, mentions, new threads), the product's state follows its repository, and the documents are in the web app and Notion.

Decisions: the five stages and their documents as Catervas ADR 0072 and `docs/design/marketing-agent.md`; the Marketing Specialist's research skills copied from Catervas (`researching-the-market`, `researching-the-audience`, `researching-competitors`, `studying-competitor-outreach`, `understanding-the-product`, `finding-where-to-start`, `running-the-research-foundation`), each adapted to Praeco's tools; search results from blocked hosts recorded with `record_search_result`, never fetched.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | The skills and the foundation | 6.1, F12 | the copied skills; five stage cycles in order; one document per stage |
| 02 | More sources | 5.8, ADR 0009 | `exa_search` (Reddit and the web), `hn_search`, `stackexchange_search`, `record_search_result`; per-source rate limits |
| 03 | Continuous research | 6.1 | watched pages and their diffs; mentions; daily cadence; the foundation's refresh |
| 04 | Notion pages | 6.1, F14 | `write_notion_page` (outward, `auto` allowed); each document kept in sync |
| 05 | The product's state | 6.1, F13 | the GitHub App (read) on the product's repository; releases, tags, changelog, README into findings and the profile |

## Phase 5: Prospects

Ends with: Praeco keeps a profile of a good fit, scans its sources daily for signals and companies that match, scores them in code, and puts them in an inbox in the web app and the top five in the digest, each with a drafted reply a member posts themselves; unsaved prospects are deleted after 90 days and a deletion request suppresses a handle.

Decisions: ADR 0010; `scoreProspect` weights from the profile, criteria met with evidence only; drafts follow the playbook (phase 7) once it exists, and the profile's voice notes before.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | The profile of a good fit | 6.2, F15 | criteria and weights from research; owner-editable; `scoreProspect` |
| 02 | Scans | 6.2, 5.9 | `record_prospect` with redaction and suppression; dedupe by URL and handle |
| 03 | The inbox | 4.5, F15 | the web page; Save, Dismiss with reason, Open; the digest's top five |
| 04 | Reply drafts | 4.5, ADR 0010 | `write_draft` for saved prospects; Copy; "I contacted them" |
| 05 | Retention and deletion | 5.9, F16 | the 90-day purge; deletion requests; the suppression list; CSV export |

## Phase 6: SEO

Ends with: Praeco reads Search Console daily, audits the site weekly, keeps a keyword map with opportunities, writes briefs and drafts, opens pull requests for fixes to the allowed paths of the site's repository, and measures each merged change at two and four weeks.

Decisions: ADR 0011; Search Console scope `https://www.googleapis.com/auth/webmasters.readonly`; PageSpeed Insights with the installation's key when set, unauthenticated otherwise; the crawl as `read_page`'s rules allow, at most 500 pages.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | Search Console | 6.3, F17 | the Google sign-in; the daily pull into `search_stats` |
| 02 | The crawl and the audit | 6.3, F17 | `crawl_site`, `run_pagespeed`, `record_page_issue` |
| 03 | Keywords and opportunities | 6.3, F17 | `record_keyword`; the map; opportunities computed in core |
| 04 | Briefs and drafts | 6.3 | `write_draft` kinds `brief`, `article`, `meta` |
| 05 | Pull requests | 6.3, F18 | the GitHub App (write) on the site's repository; `open_pull_request`, outward, asks; allowed paths |
| 06 | Measuring changes | 6.3, F19 | merged pull requests tracked; before and after per page in the digest |

## Phase 7: Learning

Ends with: approvers' edits, send-backs, dismissals and closed pull requests, and measured results, become playbook rules that Praeco applies or asks about, that every cycle reads, that the web app lists with their evidence and an Undo, and that the weekly report announces; a replay check shows whether the playbook raises the share of drafts approved without an edit.

Decisions: ADR 0012.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | Feedback events | 5.7 | edits stored as diffs, send-backs, dismissals, closed pull requests, results; approver-only |
| 02 | The playbook | 5.7, F20 | `propose_learning`; `applyLearning` in core; `playbook_rules`; the read-only `playbook` store; Undo |
| 03 | The weekly report | 6.5, F11 | results against last week, what Praeco learned, what it plans; Notion page |
| 04 | The replay check | 5.7 | a fixed set of past drafts and their approved versions; drafts written with and without the playbook; the share approved unedited, measured by a grader cycle; run by hand |

## Phase 8: Hosted beta and design partners (M1)

Ends with: the hosted service runs invite-only with five design partners for four weeks, and the M1 gate (SPEC 11), pre-registered in step 05 before the first partner starts, is measured and recorded.

Decisions: ADR 0002 (hosting planned in `praeco-ops`); the hosted service's Anthropic account metered per workspace by its sessions' `usage.list_cost`.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | Many workspaces, hardened | 8.5 | every table under its policy, proven by one test per table family; per-workspace rate limits |
| 02 | The app's side of hosting | 8.6 | health and readiness, migrations on deploy, structured logs, the config the ops repository supplies |
| 03 | Invite-only sign-up | F24 | invites by the operator; the hosted Slack, Google and GitHub apps' settings |
| 04 | Privacy, export and deletion | 5.9, F25 | the privacy policy's facts (sub-processors: Anthropic, Slack, Notion, Google, GitHub, Exa, AWS); export; deletion within 24 hours |
| 05 | Design partners | 11 (M1) | the pre-registered gate; the runbook; four weeks; the result recorded in an ADR |

## Phase 9: Social and video

Ends with: an approved marketing plan schedules posts through Buffer, each shown ahead in the digest with a Stop; posters and short videos cut from a longer one are made within allowances.

Decisions: Catervas ADR 0042 and `docs/design/marketing-specialist.md` for the plan, posts and Stop; Catervas's kit for Buffer, Recraft and Higgsfield (`crates/roles/roles/marketing_specialist/kit.yaml`), its allowances (Catervas ADR 0037).

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | The marketing plan | 6.4, F26 | `propose_plan`; approval; the plan's outward acts |
| 02 | Posts through Buffer | 6.4 | `schedule_post`; hand-over an hour ahead; Stop |
| 03 | Posters and video | 6.4 | `generate_image` (Recraft), `generate_video` and shorts (Higgsfield), paid allowances |
| 04 | Posts' results | 5.7 | Buffer's post statistics into feedback |

## Phase 10: Launch (M2)

Ends with: `v0.1.0` tagged, its image published, the self-hosting guide, and the hosted service open to sign-up with billing.

Decisions: the price and included model use from M1's measured cost per workspace (SPEC open question 2); Stripe for billing, in `ee/billing` under the commercial license this phase adds.

| Step | Name | Spec | One line |
|---|---|---|---|
| 01 | Billing | 9, F24 | `ee/billing`; plans; included amount as the month's default limit; Stripe |
| 02 | Open sign-up and the self-hosting guide | 8.6, F23 | public sign-up; the guide with the Slack, Google and GitHub app registrations |
| 03 | `v0.1.0` | 11 (M2) | the tag, the image on GitHub's container registry, the changelog |
