# Praeco Specification

Version 0.1 (draft for the founder's review). Owner: project founder. Status: not yet implemented; this document is the contract phase 0 is built against. "Praeco" (Latin: the herald, the town crier) is a working name; renaming it is a find-and-replace across this repository before phase 0 step 01 creates the first package.

Praeco's origin is the Marketing Specialist of Catervas (`abdshaat/farik`, ADRs 0042 and 0072, `docs/design/marketing-specialist.md`, `docs/design/marketing-agent.md`), taken out of the team and made a product of its own (ADR 0001 here). Where a rule below came from Catervas, the Catervas record is named so its reasoning can be read.

## 1. Goals and non-goals

Goals for the first release, `v0.1.0`:

1. A company installs Praeco into its Slack workspace, gives it its product's website, and within an hour has a research brief on its product, its audience and its competitors, every fact with its source and the day it was read.
2. From then on Praeco works constantly, in bounded cycles, on three programs: **research** (the product, the problem it solves, the people who have it, the competitors, what is said about them), **prospects** (public posts and companies that show the need the product meets) and **SEO** (the site's search health, its keywords, briefs, drafts, and fixes proposed as pull requests). Social posting and video come after (section 6.4).
3. Every effect outside the agent's own sandbox goes through a Praeco tool that code decides: allowed, refused with a reason, or asked of a person. Nothing reaches the outside world without a person's approval or a plan a person approved (section 5).
4. Every cycle has a budget, and the workspace's daily and monthly limits are hard stops.
5. Praeco learns from the company: an approver's edits and rejections, and the measured results of its work, become rules in a playbook the company can read and undo (section 5.7).
6. People talk to Praeco in Slack: they ask it for work, approve what it proposes, and read its digest there. A web app holds setup, connections, rules, budgets, the prospect inbox, SEO, and the record of everything it did.
7. The repository is public under Apache 2.0, a company can run all of it on its own server with its own Anthropic API key, and a hosted Praeco runs the same code for those who would rather not (section 9).

Non-goals for `v0.1.0`:

- Sending anything to a prospect. Praeco finds and drafts; a person contacts (ADR 0010).
- Reading private data it was not given: no scraping behind a login, no data brokers, no personal contact details (ADR 0009, ADR 0010).
- Publishing to a website. SEO changes are pull requests and drafts a person merges (ADR 0011).
- Paid advertising (Google Ads and the like). Catervas's Google Ads connector and its budget's hard stop are the model when it is added; not before `v0.1.0`.
- Microsoft Teams, Discord, or a chat other than Slack (ADR 0007).
- An AI engine other than Claude through Claude Managed Agents (ADR 0004).
- Several products per workspace. One workspace markets one product.
- Training any model on customer data. Learning is a playbook of written rules (section 5.7).

## 2. Who it is for

- **The software founder** of a company of one to twenty people, with a product live and no one whose job is marketing. Built for them first. They live in Slack and GitHub, their site is in a repository, and they will approve a pull request faster than they will read a report.
- **The first marketer** at a small company, who wants a researcher and an SEO analyst that never stops, and keeps the decisions.
- **The agency** running marketing for several small clients, one workspace per client. Served by the hosted multi-workspace account from phase 8; not designed for before then.

Deferred: enterprises (SSO beyond Slack's, SOC 2, data residency), non-software businesses (Catervas ADR 0040's car flippers), and products with no website.

## 3. Core concepts

- **Workspace.** One company and its one product. Everything Praeco stores belongs to exactly one workspace, and no query crosses two (section 8.5).
- **Member.** A person in the workspace, known by their Slack user. A member's role is `owner` (settings, budget, connections, members), `approver` (approves and gives feedback that teaches) or `member` (asks and reads). Every workspace has at least one owner.
- **Product profile.** What Praeco knows about the product as settled fact: name, site, one-line description, pricing page, repository, the documents it was pointed to. Kept by Praeco from research and editable by an owner; the owner's edit wins.
- **Program.** A standing body of work with its own cadence: `research`, `prospects`, `seo`, `learning`, `reporting`, from phase 9 `social`, and `request` for work a member asked for.
- **Job.** One piece of work for the agent, written as a **job contract** before it starts: its program, its intent in plain words, what it is expected to record, the tools it may use, and its budget (section 5.3). The Catervas task contract, cut to what one bounded agent run needs.
- **Cycle.** One run of one job: one Claude Managed Agents session from start to finish, bounded by the job's budget (section 5.2). Praeco "works constantly" by running cycles one after another, never by keeping one session open.
- **Governor.** Deterministic code, not an agent, that decides every tool call the agent makes (section 5.4). "The agent proposes; Praeco disposes."
- **Tool.** A capability the agent calls. Built-in tools run in the agent's sandbox (reading and writing scratch files, web search). Praeco tools are custom tools Praeco's own process executes after the governor allows them; they are the only way the agent changes anything outside its sandbox.
- **Outward act.** A tool call whose effect a person outside Praeco can see: a pull request, a Notion page, a scheduled post, a Slack message in a channel other than Praeco's own. Needs approval or an approved plan (section 5.6).
- **Record.** Something a cycle leaves in Praeco's store: a **finding** (a fact with its source), a **prospect** (a public signal of need), a **page issue** or **keyword** (SEO), a **draft** (text a person may use), a **report**.
- **Source.** Where a record's fact came from: a URL and the time it was read, or a connected service and the time it answered. A record without a source is refused (principle 4).
- **Playbook.** The workspace's written rules for how Praeco works ("lead with the outcome, not the feature", "never cite Reddit threads older than a year"), learned from feedback and results, each with the events it came from, each undoable (section 5.7).
- **Budget.** Money and time limits: per cycle, per day, per month, plus allowances for paid tools (section 5.5).
- **Event.** Every action, decision and change is an event in an append-only log (section 8.4).
- **Approval.** A question a member with the approver role answers, in Slack or the web app: approve, send back with a reason, or let it expire after seven days.

## 4. User journeys

### 4.1 Install and first hour

1. The owner opens Praeco (hosted: the sign-up page; self-hosted: their server's address) and signs in with Slack, which installs Praeco's Slack app into their workspace. They are the workspace's first owner.
2. Setup asks for three things, each skippable but the first: the product's website; the Slack channel Praeco posts in (default: it creates `#praeco`); a monthly budget (default $50 in the hosted product, none until set when self-hosted, where it is the owner's own Anthropic bill; the first cycle waits for one).
3. Optional connections, each one click through the service's own sign-in: Notion (a page Praeco writes under), Google Search Console (read), GitHub (the product's repository, read; the site's repository, pull requests).
4. Praeco queues the research foundation (section 6.1) and says so in the channel. Within the hour the channel has a summary of the product brief, the audience, and the competitors, with a link to each document (in Notion when connected, else in the web app).
5. From the next day the programs run on their cadences (section 6) and the channel gets one digest a day.

### 4.2 A day with Praeco

The daily digest in the channel, posted at 09:00 in the workspace's time zone: what Praeco did since the last digest (cycles, records by kind, spend against budget), what waits for someone (approvals, with their buttons), the day's five best prospects, and anything that stopped (a budget reached, a connection that failed). Nothing else is posted in the channel unless a member asked for it.

### 4.3 Asking Praeco

A member mentions `@Praeco` in any channel it is in, or sends it a direct message: "find people complaining about invoice reconciliation on Reddit this month", "write a comparison page against Acme". Praeco answers in the thread within a minute: what it will do, as the job's intent, and when. The request becomes a `request` job at the head of the queue; its result comes back to the same thread. A request that would need an outward act becomes a draft and an approval, never the act itself. Praeco reads only messages addressed to it (ADR 0007).

### 4.4 Approving

An approval arrives as a Slack message to the channel with the thing to approve (a pull request's summary and diff link, a Notion page, a post) and three buttons: Approve, Send back (asks for a reason), Open. Only members with the approver or owner role can press Approve; another member's press is answered privately with who can. An approval unanswered for seven days expires and its draft stays a draft. Every press is an event.

### 4.5 The prospect inbox

The web app's Prospects page lists prospects, newest first by default, sortable by fit: where it was said, by whom (the public handle as shown), when, the quoted need, why it fits, and a drafted reply. A member can Save, Dismiss (with an optional reason, which teaches), Copy the reply, Open the thread, or mark "I contacted them". Praeco never posts the reply (ADR 0010).

### 4.6 An SEO fix

Praeco finds that the pricing page has no meta description and ranks 14th for a query it gets 900 impressions a month on. It writes a brief, a fix, and a pull request to the site's repository on a `praeco/` branch, touching only the paths the owner allowed. The approval in Slack links the pull request; the approver merges it on GitHub as any pull request. Two and four weeks later Praeco compares the page's impressions, clicks and position with before, and says so in the digest.

### 4.7 Stopping, undoing, forgetting

`@Praeco pause` (or Pause in the web app) stops new cycles and lets the running one finish its current tool call, then stops it. Any playbook rule has an Undo. An owner can delete a prospect and add its handle to the suppression list, export the workspace's data, or delete the workspace, which removes everything Praeco stored for it within 24 hours (section 5.9).

## 5. The harness

### 5.1 Principles

1. **The agent proposes; Praeco disposes.** Every effect outside the agent's sandbox is a Praeco tool, decided by the governor, in code, before it runs. A prompt is not a control.
2. **Nothing leaves without a person or a plan.** Outward acts need an approver's yes, or a plan an approver approved that names them (section 5.6).
3. **Bounded everything.** Every cycle has a budget in money, minutes and tool calls; the workspace's day and month have limits; reaching one stops work, and the digest says so.
4. **Every fact has a source and a date.** A record without a source is refused. A guess is marked as one.
5. **Every action is an event.** Tool calls, refusals, approvals, spend, learnings, setting changes: all in an append-only log a member can read.
6. **People are the only source of rules.** Budgets, approvals, sources, connections and programs are set by people. The playbook changes how Praeco writes and chooses, never what the governor allows (section 5.7).
7. **Public data, within its terms.** Praeco reads what anyone can read without signing in, as the site's terms and `robots.txt` allow, and keeps no personal contact details (sections 5.8, 5.9).

### 5.2 The cycle

A cycle runs one job. States and who moves them:

| From | To | By | When |
|---|---|---|---|
| (none) | `queued` | scheduler, or a member's request | the job's contract passes `validateJob` |
| `queued` | `running` | scheduler | the workspace is not paused, no other cycle of the workspace is running, and the budget allows one more cycle (5.5) |
| `queued` | `cancelled` | a member, or the scheduler | cancelled by hand, or queued longer than 24 hours |
| `running` | `finished` | governor | the agent called `finish_cycle` and the job's expectations are met, or it gave up with a reason (5.3) |
| `running` | `stopped` | governor | a limit was reached (`budget`, `time`, `tool_calls`), the agent was refused five times in a row (`refused`), it ended its turn twice without calling `finish_cycle` (`unfinished`), the session failed or the process running it stopped answering (`error`), or the workspace was paused (`paused`) |

One cycle runs at a time per workspace. A stopped cycle's records stay. The next cycle of the same program starts fresh and reads what the last one recorded.

### 5.3 The job contract

```
{ job_id, workspace_id, program, intent,           // intent: 20 to 600 characters, plain words
  expects: [ { kind, at_least } ],                 // kind: finding, prospect, draft, page_issue, keyword, report
  allowed_tools: [ tool_name ],                    // every Praeco tool the cycle may call; built-ins are per program
  budget: { usd_micros, minutes, tool_calls },     // within the workspace's per-cycle limit
  requested_by: member_id | null, request_thread: { channel, ts } | null,
  created_at }
```

`validateJob` refuses a contract whose tools include one outside the program's list (section 5.4), whose budget is above the workspace's per-cycle limit, or whose expectations name a kind no allowed tool records. `finish_cycle { summary, gave_up? }` is refused while an expectation is unmet, unless `gave_up` carries a reason of 20 characters or more, which the digest shows.

### 5.4 Tools and their tiers

Every Praeco tool has one tier, fixed in code (`@praeco/core` `TOOL_RULES`):

| Tier | Effect | Governor |
|---|---|---|
| `read` | reads outside data: `read_page`, `exa_search`, `hn_search`, `read_search_console`, `read_repository` | allowed when in the job's tools; `read_page` also checks the source policy (5.8) |
| `record` | writes a record in Praeco's store: `record_finding`, `record_prospect`, `record_page_issue`, `record_keyword`, `update_product_profile` | allowed when in the job's tools and the record validates (source present, lengths, redaction) |
| `draft` | writes a draft a person may use: `write_draft`, `propose_learning`, `propose_plan` | allowed when in the job's tools |
| `outward` | changes something people outside Praeco see: `open_pull_request`, `write_notion_page`, `post_to_channel`, `schedule_post` | asks (5.6), or allowed under an approved plan that names it |
| `paid` | spends money beyond model use: `generate_image`, `generate_video` | allowed within the allowance (5.5), else asks |
| `control` | reports on the cycle itself: `finish_cycle` | always allowed; `finish_cycle` is then checked against the job's expectations (5.3) |
| `denied` | anything that contacts a person outside the workspace or spends without a plan | always refused; no tool of this tier is offered to the agent |

Built-in tools: the agent's file tools (`read`, `write`, `edit`, `glob`, `grep`) on its scratch folder, and `web_search` with the source policy's blocked domains. `bash` and the built-in `web_fetch` are off (ADR 0004): pages are read through `read_page`, which applies the source policy.

### 5.5 Budgets

- **Model spend** is the session's own cost as Managed Agents reports it (`usage.list_cost`, priced at public list rates, in cents), read from every `session.usage` event and kept in integer micro-dollars.
- **Limits:** per cycle (default $1.50), per day (default a thirtieth of the month), per month (set in setup). A cycle starts only when the day and the month have room for its whole per-cycle budget.
- **Two walls.** Praeco's own check on every `session.usage` event and before each tool result it returns, and the session budget it sets on the Managed Agents session to the cycle's limit, which the platform enforces even if Praeco's process dies.
- **Allowances** for `paid` tools count calls per workspace per month (Catervas ADR 0037); a call past its allowance asks.
- Reaching a day's limit pauses the programs until the next day; the month's, until the next month or a raised limit. The digest and the web app say which.

### 5.6 Approvals and plans

An `outward` call without a covering plan becomes an approval: the call's inputs are stored, the cycle is told "asked; an approver will decide", and the cycle goes on with other work or finishes. On approval Praeco executes the stored call itself, with the inputs as approved, and records the result; the agent is not involved again. Sent back: the reason is recorded and reaches the next cycle of that program as the approver's own words. A plan (phase 9, social) approved by an approver allows the outward calls it lists, each within its dates and amounts (Catervas ADR 0042). An owner may set the workspace to `auto` for `write_notion_page` and `post_to_channel` only; pull requests and posts always ask or follow a plan.

### 5.7 The playbook and learning

- **What teaches.** An approver's edit of a draft (the diff), a send-back with its reason, a prospect dismissed with a reason, a pull request closed unmerged, a member's explicit "remember this" in Slack, and measured results (an SEO change's before and after; from phase 9 a post's reach). Only feedback from members with the approver or owner role teaches. Text Praeco read on the web never does.
- **How.** A `learning` cycle runs weekly and after every ten new feedback events. The agent reads the feedback and proposes rules with `propose_learning { rule, scope, evidence: [event_id] }`; `rule` is at most 300 characters, `scope` is a program or `all`. The governor applies a rule at once when its evidence is two or more feedback events from approvers that agree, and asks an approver otherwise. The playbook holds at most 100 active rules; a new one past that asks which to retire.
- **What it changes.** The playbook is mounted read-only in every cycle's sandbox as `/mnt/memory/playbook/rules.md`. It changes what the agent writes and chooses. The governor never reads it: no rule can change a budget, an approval, a source policy, a connection or a program's tools.
- **Undo.** Every rule shows in the web app with its evidence and an Undo, and each application is announced in the digest with "I learned: …".

### 5.8 Sources

- Praeco reads public pages through `read_page`: the URL's host is not on the blocked list, its `robots.txt` allows the user agent `PraecoBot`, the response is at most 2 MB, and the text is extracted before the agent sees it. One page per host per second.
- Blocked from reading: hosts whose terms forbid automated access to public pages: `linkedin.com`, `facebook.com`, `instagram.com`, `x.com`, `twitter.com`, `tiktok.com` (Catervas `docs/design/marketing-agent.md`, "Sources, within their terms"). Search results may name their pages; Praeco records the search result's URL and snippet, never the page.
- Reddit is read through Exa (`exa_search`, its contents included), which Catervas found returns threads with comments (2026-10-18); Hacker News through its public Algolia API; Stack Exchange through its public API. Each is a connector with its own tool.
- An owner may add hosts to the blocked list; nobody may remove the built-in ones.

### 5.9 Prospects and personal data

- A prospect is either a **signal** (a public post showing the need: platform, URL, the author's public handle as shown, when it was posted, a quote of at most 280 characters, the need in one sentence) or a **company** (name, domain, why it fits, the source). Nothing else about a person is kept.
- `record_prospect` refuses a quote containing an email address or a phone number, and refuses any field named for contact details. Praeco has no enrichment service and no email finder.
- Unsaved prospects are deleted 90 days after they were found. Saved ones stay until dismissed or deleted.
- A deletion request (by handle or URL) deletes the prospect and adds the handle to the workspace's suppression list, which `record_prospect` checks.
- Praeco never contacts a prospect. Drafted replies are for a member to post themselves (ADR 0010).

### 5.10 What the harness does not solve

It cannot make a draft good, only make it visible before it goes out. It cannot stop a member from posting a bad reply by hand. It cannot know that a public page's author would object to being quoted; the 280-character limit, the handle-only rule and the 90-day deletion are what it does about that.

## 6. Programs

Cadences are defaults an owner can change in Rules; each program can be turned off.

### 6.1 Research

- **The foundation**, once at setup and again when 92 days old: five stages in order, each a document with every fact sourced (Catervas ADR 0072): the product; the people it is for (in their own quoted words); the competitors in three groups, direct, indirect, and substitutes or doing nothing; how they reach people; where to start (positioning, the message per group, three cheap tests).
- **Continuous**, daily: what changed at each direct competitor (pricing page, home page, changelog: a page read and compared with the last read), new mentions of the product and of the problem's words, new threads in the audience's places.
- **The product's state**, on every new release or tag of the connected repository and weekly otherwise: the changelog, README and release notes become findings and update the product profile. This is how Praeco knows what shipped.
- Output: findings, the documents (web app; Notion when connected).

### 6.2 Prospects

- **The profile of a good fit**, written from the research (who, the need, the words they use, where they gather, what disqualifies), editable by an owner.
- **Scans**, daily: each source searched with the profile's words; each candidate checked against the profile's criteria; recorded with evidence for each criterion met. The fit score is computed by `@praeco/core` from the criteria met and their weights, never written by the model.
- **Drafts**: a reply for each saved prospect, in the playbook's voice, that answers the need before it mentions the product.

### 6.3 SEO

- **Search Console**, daily pull: queries, pages, impressions, clicks, position.
- **The audit**, weekly: a crawl of the site (same host, `robots.txt` respected, at most 500 pages), PageSpeed Insights for the ten pages with the most impressions, issues recorded per page with evidence.
- **Keywords**, weekly: the keyword map (query, page, impressions, position, intent), opportunities (impressions without clicks, positions 8 to 20, queries with no page).
- **Briefs and drafts**: for the top opportunities, a brief and a draft (an article, a new title and description, a comparison page).
- **Fixes**: a pull request to the site's repository, touching only the owner's allowed paths, on a `praeco/<job_id>` branch, never merged by Praeco.
- **Measurement**: two and four weeks after a merged change, the page's numbers against the four weeks before.

### 6.4 Social and video (phase 9)

Catervas's design, unchanged in its rules: a marketing plan an approver approves allows the posts it lists; Buffer schedules them; each post shows ahead in the digest with a Stop; posters through Recraft and short videos made from a longer one through Higgsfield, within allowances.

### 6.5 Reporting

The daily digest (4.2) and, on Mondays, the weekly report: results against the last week, what Praeco learned, and what it plans next, as a Notion page when connected.

## 7. Functional requirements

- F1 Sign in with Slack; the first person to install a workspace is its owner.
- F2 Members and roles, changed by owners in the web app.
- F3 The product profile, kept by Praeco and editable by owners.
- F4 Job contracts validated before a job is queued.
- F5 The cycle lifecycle of 5.2, enforced in code.
- F6 The governor's decision for every Praeco tool call, recorded as an event.
- F7 Budgets per cycle, day and month, and paid allowances, as hard stops.
- F8 The scheduler: programs on cadences, requests first, one cycle at a time per workspace, pause.
- F9 Requests from Slack mentions and direct messages, answered in thread.
- F10 Approvals in Slack and the web app, approver-only, expiring after seven days.
- F11 The daily digest and the weekly report.
- F12 The research foundation and continuous research.
- F13 The product's state from its repository.
- F14 Notion as a destination for documents and reports.
- F15 The prospect profile, scans, scoring, inbox, drafts.
- F16 Prospect retention, deletion and the suppression list.
- F17 Search Console, the crawl and audit, keywords, briefs and drafts.
- F18 SEO fixes as pull requests to allowed paths.
- F19 Measuring merged SEO changes.
- F20 The playbook: proposals, automatic application, approval, undo.
- F21 The activity log in the web app, filterable by program, cycle and kind.
- F22 Connections: Slack, Notion, Google Search Console, GitHub; from phase 9 Buffer, Recraft, Higgsfield.
- F23 Self-hosting with one image and one `docker compose up`.
- F24 The hosted service: invite-only from phase 8, open with billing from phase 10.
- F25 Data export and workspace deletion.
- F26 Social plans, posts with Stop, posters and video (phase 9).

## 8. Architecture

### 8.1 Shape

One TypeScript repository (ADR 0003), one container image, one program `praeco` with two long-running roles: `praeco serve` (the HTTP server: the API, the web app, Slack's events and interactions, connection sign-ins) and `praeco work` (the scheduler and the cycle runner). Both talk to one Postgres. Self-hosted runs both from `docker compose`; hosted runs each as its own service.

| Package | Directory | Holds |
|---|---|---|
| `@praeco/core` | `packages/core` | decisions with no I/O: job contracts, the cycle lifecycle, the governor, budgets, scoring, the playbook's rules |
| `@praeco/schemas` | `packages/schemas` | JSON Schemas, generated types, validators, the snake_case mapping |
| `@praeco/store` | `packages/store` | Postgres: migrations, the event log, records, settings |
| `@praeco/agent` | `packages/agent` | the Managed Agents client: the agent's definition, running a cycle's session, dispatching custom tools |
| `@praeco/tools` | `packages/tools` | the Praeco tools' implementations and the connections they use |
| `@praeco/slack` | `packages/slack` | the Slack app (phase 2) |
| `praeco` | `apps/praeco` | the program: `serve`, `work`, `migrate`, the command line |
| `@praeco/web` | `apps/web` | the web app (phase 3) |
| `ee/` | `ee` | the hosted service's billing (phase 10), under its own license |

`@praeco/core` imports nothing that does I/O; `pnpm check` fails if it does (ADR 0003).

### 8.2 The agent runtime

The agent runs on Claude Managed Agents (ADR 0004): one agent definition, versioned, synced by `praeco agent sync`; one cloud environment per Praeco installation; one session per cycle, created with the job's instructions, the playbook and knowledge memory stores mounted read-only, and a session budget equal to the cycle's. `praeco work` streams the session's events, executes each `agent.custom_tool_use` after the governor's decision, answers with `user.custom_tool_result`, records usage, and ends the cycle on `finish_cycle`, a limit, or an error. The model is Claude Opus 5.5 at effort `high` for the research foundation and `learning`, `medium` for every other program; an owner may choose Claude Sonnet 5.5 for all programs to cost less.

### 8.3 Storage

Postgres 18 is the only store (ADR 0005). Every table but the installation's own has `workspace_id`; every workspace transaction runs as the role `praeco_app`, whose row-level security policies restrict it to the workspace set on that transaction (ADR 0005). The event log (`events`) refuses `UPDATE` and `DELETE` by trigger, except the deletion of a whole workspace, which sets `praeco.deleting_workspace` to that workspace's id for its one transaction. Events carry a record's id, never a prospect's quote or handle, so deleting a prospect (5.9) leaves no copy of it in the log. Secrets (connection tokens) are encrypted with AES-256-GCM under `PRAECO_SECRET_KEY` before they are stored. Wire, file and database formats are `snake_case`; TypeScript is `camelCase`, mapped once in `@praeco/schemas` and once in `@praeco/store`.

### 8.4 Events

Kinds are `<entity>.<past_tense_verb>`. The first ones: `workspace.created`, `cycle.queued`, `cycle.cancelled`, `cycle.started`, `cycle.finished`, `cycle.stopped`, `tool.allowed`, `tool.refused`, `tool.asked`, `tool.completed`, `tool.failed`, `spend.recorded`, `budget.reached`, `finding.recorded`, `workspace.paused`, `workspace.resumed`. Each later step that first emits a kind adds it to `event.schema.json`.

### 8.5 Security

- The sandbox holds no secret: connections are used by Praeco's process when it executes a tool; the agent sees results only.
- Everything the agent reads from the web is untrusted. Praeco tool results wrap outside text as data and the instructions say so (Catervas ADR 0011); the governor's decisions do not depend on it.
- A workspace's data is reachable only through a transaction that set its id; one test proves a query in workspace A cannot read workspace B's rows (phase 0 step 03, phase 8 step 01).
- Web sessions are `HttpOnly`, `Secure` (hosted) and `SameSite=Lax` cookies; every state-changing request checks `Origin`.
- Slack requests are verified by signature; Slack data is used only to answer the message it came from (ADR 0007).

### 8.6 Self-hosted and hosted

The same image. Self-hosted: the owner's Postgres (or the compose file's), their Anthropic API key, their own Slack app created from `slack-manifest.yaml`, their own Google and GitHub apps. Hosted: Praeco's Slack, Google and GitHub apps and Anthropic account, metered per workspace; its infrastructure is planned in the private `praeco-ops` repository (ADR 0002).

## 9. Open source and hosted

Everything outside `ee/` is Apache 2.0, the governor and every safety rule included: nothing that makes the agent safer or more controllable is ever paid for (Catervas's rule, kept). The hosted service sells convenience: no server, no keys, no app registrations, model use included up to a plan's amount. `ee/` holds billing only.

## 10. Non-functional requirements

- A cycle's first tool call within 60 seconds of `running`.
- A Slack mention answered within 60 seconds.
- The web app's pages load in under two seconds on the hosted service.
- No data loss on a crash: a running cycle whose process has not written its heartbeat for two minutes is `stopped` with `error` by any `praeco work`, its records kept.
- Self-hosting needs one machine with 2 GB of memory beside Postgres.

## 11. Milestones

- **M0, the loop** (end of phase 1): on one computer, Praeco researches a product continuously within a daily budget, every tool call decided and logged.
- **M1, design partners** (end of phase 8): five companies use the hosted beta for four weeks. The gate, pre-registered before the first partner starts: at least three still active in week four (an approver acted on something that week), at least half of the drafts approved without an edit in week four, and at least one merged SEO change and one contacted prospect per active partner. Failing it changes the product before launch, as Catervas's proof of concept would have.
- **M2, launch** (end of phase 10): `v0.1.0`, the public image, and the hosted service open with billing.

## 12. Open questions

1. Slack's API terms (mid-2025) restrict using Slack data "in LLMs". Praeco sends only the text of messages addressed to it to the model, to answer them, and stores only that text on the job. Confirm this reading with Slack's current terms and the Marketplace review before phase 2 executes; if it fails, requests move to the web app and Slack keeps the digest and approvals.
2. The hosted plan's price and included model use: decided from M1's measured cost per workspace, not before.
3. Whether the agency persona needs one member to belong to several workspaces in one sign-in: decided in phase 8.
4. Search keyword volumes beyond Search Console's (DataForSEO or Google Ads' planner): after M1, if partners ask.

## 13. Glossary

Approval, Cycle, Event, Finding, Governor, Job, Member, Outward act, Playbook, Program, Prospect, Record, Source, Tool, Workspace: as in section 3. "Task" means a plan task (`docs/standards/workflow.md`) and nothing in the product.
