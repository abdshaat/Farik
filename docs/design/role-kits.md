# Role kits

Status: approved by the founder on 2026-09-28, in conversation, as the first cut. It is the design input to phase 8. ADR 0020 records the decision, and spec 0.22 (sections 5.6, 6.7 and 8.1) carries its rules. The founder amends the kit tables as the phase is planned.

## Why

The founder wants every role to be great at its job: each with a special set of skills and connectors, the Marketing Specialist with marketing skills and creative connectors such as Higgsfield among them. Phase 7 builds the plumbing, MCP servers per agent and skills loaded into sessions. This phase fills it, role by role, before the web launch.

## What a kit is

Each role ships `roles/<role>/kit.yaml`, beside its `role.yaml`, `system.md` and `skills/`, validated against `docs/schemas/kit.schema.json`:

- **Skills.** The `skills/<name>/SKILL.md` files the role carries, in the Agent Skills format the roles already use, loaded into every session of the role at the role level (phase 7 step 03). A skill is a way of working, with the checklists and templates the role needs; it is prose the model reads, not code.
- **Connectors.** The MCP servers the role may be connected to, each with:
  - its name, transport (`stdio` or `http`), command or URL, and the credential keys it needs, as phase 7 step 01 configures any server;
  - the setup copy the wizard shows: what the service is, why the role wants it, and what the user must do (sign in, or paste a key from a named page);
  - **every tool pre-tagged** with its tier (spec 5.6), so the user never tags a tool by hand, and the tool list **pinned**: a test lists the live server's tools and fails when they differ from the kit's, so a change at the service is caught in the repository rather than in a session;
  - an **allowance** where the connector spends the user's credits: the default number of calls per sprint that are pre-approved, which the user sets when connecting. Tools that publish, send, post or pay have no allowance and always ask.
- **Optional, every one of them.** Adding a role to the team shows its connectors as "Connect" steps the user may skip. A role works without any of them; a kit makes it better, not possible.

Kits are free forever, like the roles, MCP and skills (spec 9).

## Spending connectors

A call that spends the user's credits on another service (generating an image, a video, a voice) is an `external_effect`: it changes state outside the sandbox. Today that means the human approves every call, which is right for publishing a post and wrong for a marketer making twenty images for a launch.

So a kit may give such a connector an allowance. When the user connects it, they set how many calls per sprint are pre-approved, from the kit's default; calls inside the allowance run without a prompt; the first call beyond it asks the human, as any `external_effect` does, and the human may raise the allowance. The count resets with the sprint, and a project without sprints counts per UTC day, as the daily budget does (spec 5.5). Each call is recorded (`connector.called`) with the connector, the tool and the count against the allowance, so the board shows "14 of 20 generations this sprint" beside the model spend. An allowance counts calls, not money: Farik cannot price another service's credits, and the user reads the bill there.

Everything a connector returns is untrusted content (spec 8.6), a creative service's captions and generated text included.

## The kits, first cut

The founder amends these when phase 8 is planned. A connector is named by the service; the step plan picks the server (the service's official MCP server where one exists, else a pinned community one, else Farik's own thin one) and records the choice.

| Role | Skills | Connectors |
|---|---|---|
| Product Manager | contract and epic writing; asking the user the right questions; PRD and requirements writing; prioritisation; release scope | product analytics, read (PostHog, Plausible, or Google Analytics); issue-tracker import, read (GitHub Issues, Linear), so a request can come from an existing backlog; product docs, read (Notion, Google Drive) |
| Scrum Master | triage; breakdown of an epic into tasks with exit criteria; sprint planning within a budget; standups, reviews and retros; escalation digests | a chat bridge that mirrors the team channel and takes the user's replies (Slack, Discord); posting to it asks until the user allows it |
| Architect | ADR writing; API and data-model design; dependency and licence review; threat modelling and security review before a deploy is accepted; performance budgets | library documentation, read (Context7 or the like); code search, read (GitHub); vulnerability database, read (OSV); the same web research the role has |
| Software Developer | test-driven development; debugging; safe migrations; testing per stack (web, API, mobile); answering a review | library documentation, read; browser testing inside the sandbox (Playwright); the development database, read; deploy status, read (Vercel, Netlify, AWS) |
| Marketing Specialist | positioning and messaging; launch plans; SEO; copywriting in the brand's voice (landing pages, README, release notes, email); content calendars; competitor research; campaign measurement | **Higgsfield**, images, video and audio, with an allowance; a second image generator, with an allowance; social publishing (X, LinkedIn, TikTok), always asks; email marketing, stats read, sending asks; product analytics, read; design files, read (Figma, Canva) |
| Finance Specialist | expense categorisation; month-end close; forecasting; unit economics and pricing analysis; budget recommendations | Stripe, read (phase 7 step 02); the receipts mailbox (phase 10 step 02); a paid ledger, read (Kick, Digits), optional |

What is not a kit: Farik's own tools (`farik_*`), which every role has by its tiers; the sandbox; git. A kit adds what the role does beyond the harness.

## The web app

Adding a role in the team builder, or opening its card on the Team page, lists the kit's connectors with a one-line reason each and a "Connect" button. Connecting is a sign-in or a pasted key from a page the copy names, then the allowance where there is one, then done. The Costs page shows each allowance's use beside the model spend. Disconnecting removes the credential from the keychain and the server from the agent.

## Steps

| Step | Delivers |
|---|---|
| 01 Kit format and loader | `kit.schema.json` and its generated types; `kit.yaml` per role, loaded with the role; pre-tagged tools applied at connection; the pinned tool list and its drift test; allowances, `connector.called`, and the board's count; the connect, allowance and disconnect screens; `farik connect <agent> <connector>` and `farik disconnect` |
| 02 Product Manager and Scrum Master kits | Their skills and connectors, each connector's server chosen and pinned, its setup copy written and checked in the web app |
| 03 Architect and Developer kits | The same, including the sandboxed browser-testing connector and the security-review skill |
| 04 Marketing Specialist kit | The same, Higgsfield first, with the allowance flow proven end to end and the publishing connectors always asking |
| 05 Finance Specialist kit | The same; Stripe's kit entry moves here from phase 7 step 02's ad-hoc setup, and the ledger connectors are optional |
| 06 Kit check | Each role runs one real task with its kit, in the web UI, by the founder: the Marketing Specialist produces a launch post with a Higgsfield image inside its allowance, the Developer runs a browser test, and so on. Recorded in `docs/milestones/role-kits.md` and signed off by the founder |

## Tests

The step plans turn each of these into a test that fails first:
- A `kit.yaml` that names a skill folder that does not exist, a tool without a tier, or an allowance on a tool that is not `external_effect` fails validation.
- Every role's kit validates, and every skill it names loads into a session.
- A connector's live tool list that differs from the pinned one fails the drift test (run with the live tests, since it needs the service).
- A tool the kit tags `read` is given `read`, and the session's `permissions.deny` and tiers reflect the kit, not a default.
- A call inside the allowance runs without a prompt; the first call beyond it asks; a publishing tool asks at zero allowance.
- The allowance count resets with the sprint, and per day without sprints.
- `connector.called` is recorded for every call, and the board's count matches the log.
- Connector output reaches the agent under the untrusted-content notice.
- Disconnecting removes the credential and the server, and a session started after it has neither.
