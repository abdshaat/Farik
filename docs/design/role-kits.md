# Role kits

Status: approved by the founder on 2026-09-28, in conversation, as the first cut; the decisions its landing review asked for were made the same day and are marked below. It is the design input to phase 7 (phase 9 until ADR 0029, which builds the kits on Claude before other engines). ADR 0020 records the decision, and spec 0.22 (sections 5.6, 6.7, 8.1 and 9) carries its rules. The founder amends the kit tables before phase 7 step 06 is planned; steps 01 to 05 do not depend on them.

## Why

The founder wants every role to be great at its job: each with a special set of skills and connectors, the Marketing Specialist with marketing skills and creative connectors such as Higgsfield among them. Phase 7 builds the plumbing first, connectors per agent, the human's approval of their calls, signing in to a service by three routes, and skills loaded into sessions (steps 01 to 04, pulled forward from the ecosystem by ADR 0029 and split by the project plan's revision 27), then fills it, role by role, on Claude, before other engines and before the web launch. Phase 8 checks every kit again on each engine and provider it adds.

## What a kit is

Each role ships `roles/<role>/kit.yaml`, beside its `role.yaml`, `system.md` and `skills/`, validated against `docs/schemas/kit.schema.json`:

- **Skills.** The `skills/<name>/SKILL.md` files the role carries, in the Agent Skills format the roles already use, loaded into every session of the role at the role level (phase 7 step 04). A skill is a way of working, with the checklists and templates the role needs; it is prose the model reads, not code.
- **Connectors.** The MCP servers the role may be connected to, each with:
  - its name, transport (`stdio` or `http`), command or URL, and the credential keys it needs, as phase 7 step 01 configures any server;
  - the setup copy the wizard shows: what the service is, why the role wants it, and what the user must do (sign in, or paste a key from a named page). The copy is in plain words and never says "MCP", "OAuth" or "token", with one exception the founder made on 2026-10-02: the setup steps may quote a service's own label, so the user finds it on the service's page ("On Slack's page, copy the value labelled ‘Bot User OAuth Token’."). A quoted label sits between ‘ and ’, “ and ”, or two ASCII double quotes, on one line, at most 60 characters; it is allowed only in a kit's `setup`, and Farik's own words stay plain everywhere else (phase 7 step 05 checks it in the kit loader);
  - **every tool tagged** by the kit, so the user never tags a tool by hand: `network` for a read-only remote call, `external_effect` for a call that changes anything outside the sandbox or spends the user's credits, or `denied` for a tool the agent is never offered (a creative service's `deploy_website`, `website_secrets` or `sandbox_exec`, say). A connector's tool is never `read`: `read` means the project's files (spec 5.6), and a remote read is an outbound call carrying arguments the agent chose. Connecting a connector gives the agent that server's tagged tools whatever the agent's own tiers, so a Developer without `network` can still read library documentation; the tag decides whether a call runs, asks, or is never offered;
  - **labels**, optional: a tool to what it does in the user's words ("search pages"), which the screens show where they list what the agent may do; a tool with none is shown by its name with `_` and `-` read as spaces;
  - a `stdio` command that is only `npx`, `bunx`, `uvx` or `pipx run` (or a binary Farik ships), naming its package **at an exact version**, `name@1.2.3` or `name==1.2.3`, with no flag that names code apart from it, which the loader checks (`package_not_pinned`, ADR 0036), so the code Farik runs changes only with a Farik release and its pin review;
  - the tool list **pinned**: a test lists the live server's tools and fails when they differ from the kit's, so a change at the service is caught in the repository rather than in a session. Because a tag is Farik's judgment about another company's tool, every pin update re-reviews the tags, since a tool can keep its name and gain a side effect;
  - an **allowance** where a tool spends the user's credits: the default number of calls per sprint that are pre-approved, which the user sets when connecting. An allowance is per tool and per agent. A batch tool (one call, many generations) carries no allowance and always asks. Tools that publish, send, post or pay have no allowance and always ask.
- **Optional, every one of them.** Adding a role to the team shows its connectors as "Connect" steps the user may skip. A role works without any of them; a kit makes it better, not possible.
- **Credentials are per agent**, as phase 7 step 01 keeps them. Disconnecting removes that agent's credential from the keychain, or from the private file a computer without one keeps it in, and the server from that agent, and touches no other agent's.

Kits are free forever, like the roles, MCP and skills (spec 9).

## Spending connectors

A call that spends the user's credits on another service (generating an image, a video, a voice) is an `external_effect`: it changes state outside the sandbox. Today that means the human approves every call, which is right for publishing a post and wrong for a marketer making twenty images for a launch.

So a kit may give such a tool an allowance. When the user connects the connector, they set how many calls per sprint are pre-approved for that tool and that agent, from the kit's default; calls inside the allowance run without a prompt; the first call beyond it asks the human, as any `external_effect` does, and the human may raise the allowance. The count resets with the sprint, and a project without sprints counts per UTC day, as the daily budget does (spec 5.5). The hook already records every MCP call (`tool.called`, spec 8.5), so the count is a projection of those events, and the board shows "14 of 20 generations this sprint" beside the model spend. An allowance counts calls, not money: Farik cannot price another service's credits, and the user reads the bill there. Looping inside the allowance is bounded by the allowance itself and by the session's tool-call limit.

Everything a connector returns is untrusted content (spec 8.6), a creative service's captions and generated text included.

## The kits, first cut

The founder amends these before step 06 is planned. A connector is named by the service; the step plan picks the server (the service's official MCP server where one exists, else a pinned community one, else Farik's own thin one) and records the choice and whether the user signs in or brings a key.

| Role | Skills | Connectors |
|---|---|---|
| Product Manager | contract and epic writing; asking the user the right questions; PRD and requirements writing; prioritisation; release scope | product analytics, `network` (PostHog, Plausible, or Google Analytics); issue-tracker import, `network` (GitHub Issues, Linear), so a request can come from an existing backlog; product docs, `network` (Notion; Google Drive after the launch, ADR 0035's amendment) |
| Scrum Master | triage; breakdown of an epic into tasks with exit criteria; sprint planning within a budget; standups, reviews and retros; escalation digests | a chat bridge that mirrors the team channel (Slack, Discord; Slack by a pasted key until after the launch, see Signing in): posting is `external_effect` and always asks; a message read from the bridge is untrusted content and never carries the human's authority, so an approval or an answer to a question is given in Farik, never in the chat |
| Architect | ADR writing; API and data-model design; dependency and licence review; threat modelling and security review before a deploy is accepted; performance budgets | library documentation, `network` (Context7); code search, `network` (GitHub); vulnerability database, `network` (OSV); the same web research the role has |
| Software Developer | test-driven development; debugging; safe migrations; testing per stack (web, API, mobile); answering a review | library documentation, `network`; browser testing (Playwright), a host process as every MCP server is, reaching the development server the task runs, `network`; the development database, `network`, read-only; deploy status, `network` (Vercel, Netlify, AWS) |
| UI/UX Designer | UX review heuristics; WCAG 2.2 accessibility checks; the project's brand and design tokens; plain-language interface wording; writing mockups; responsive and phone-width checks. Built in phase 6 step 11, not in this phase | the built-in Playwright connector, confined to the project's preview (phase 6 step 12); this phase moves its definition into the kit format. See `docs/design/designer-chats-templates.md` |
| Marketing Specialist | positioning and messaging; launch plans; SEO; copywriting in the brand's voice (landing pages, README, release notes, email); content calendars; competitor research; campaign measurement | **Higgsfield**, images, video and audio: generation `external_effect` with an allowance, batch generation always asks, its deploy, secrets, sandbox and TikTok publishing tools `denied`; a second image generator, with an allowance; social publishing (X, LinkedIn, TikTok), always asks; email marketing, stats `network`, sending asks; product analytics, `network`; design files, `network` (Figma, Canva) |
| Finance Specialist | expense categorisation; month-end close; forecasting; unit economics and pricing analysis; budget recommendations | Stripe, read tools `network`, `stripe_api_write` `denied` (connected in phase 7 step 09, moved into the kit in step 10); a paid ledger, `network` (Kick, Digits), optional. The receipts mailbox is not a connector: Farik's own tools take it (phase 12 step 02) |
| DevOps Engineer | deployment checklists; reading production logs; incident response; rollback and restore; writing postmortems; pipeline and infrastructure config. Added by ADR 0027, built in steps 11 and 12 | one of AWS (ECS, EKS), Kubernetes, Vercel or Netlify, and Render, Railway or Fly, whichever the project runs on: status, deployments, logs and metrics `network`, every other tool `denied`, since Farik's own `farik_deploy`, `farik_restart` and `farik_roll_back` make the writes. See `docs/design/devops-engineer.md` |

## Signing in

A connector is connected without a pasted key wherever its service allows (the founder, 2026-10-02; ADR 0035). Farik tries, in order: route 1, the service registers Farik itself (step 03); route 2, Farik's own registered public app, matched by the server's host (step 03b); route 3, the sign-in relay, which adds Farik's client secret to the code exchange and each refresh and keeps no token (planned as steps 03c and 03d, and deferred with Slack until after the launch, to phase 12 steps 02b and 02c, by the founder on 2026-10-02: "Connecting slack is a later step keep it simple for now"). A pasted key from a page the setup copy names is the fallback for every connector, and what the user is offered when the relay is down. Route 4, incoming hooks, is premium (phase 14) and no kit uses it.

| Service | Kits | Route |
|---|---|---|
| GitHub (issues, code search) | Product Manager, Architect | 2: Farik's GitHub App, device flow, read-only repository permissions |
| Google Analytics | Product Manager | At launch a key: Google's Analytics MCP server runs only locally, so step 06 connects it as a local server with a key, or picks PostHog or Plausible. Route 2 for Google (a desktop client, PKCE, its secret shipped as a non-confidential value) and Drive (`drive.readonly`, restricted, with a yearly security assessment) are deferred until after the launch (ADR 0035's amendment) |
| Slack (the chat bridge) | Scrum Master | A pasted key at the launch: the setup copy has the user make a Slack app in their own workspace and paste its token; the Scrum Master works without the bridge when none is pasted. Route 3, the sign-in relay with Farik's Slack app, comes after the launch in phase 12 steps 02b and 02c; until the Slack Marketplace lists that app (phase 12 step 02d, which does not block the launch), it is private to the founder's workspace and every other user keeps the key |
| Every other service (Notion, Linear, PostHog, Stripe, Higgsfield, Vercel, and the rest) | all | 1 where the service registers Farik itself, else a key. Stripe's scheduled runs keep an Agent-tagged restricted key (step 09); the DevOps Engineer's platforms take the narrow credential its kit names |

Each kit's step plan records the route of each connector it ships.

What is not a kit: Farik's own tools (`farik_*`), which every role has by its tiers; the sandbox; git. A kit adds what the role does beyond the harness.

## The web app

Adding a role in the team builder, or opening its card on the Team page, lists the kit's connectors with a one-line reason each and a "Connect" button. Connecting is a sign-in or a pasted key from a page the copy names, then the allowances where there are any, then done. The Costs page shows each allowance's use beside the model spend. These are the connector screens (`Connector`, `ConnectorAllowance`, and the connector list on `AgentEdit`), distinct from the `Connect` screen that links the browser to the daemon, and they are mocked up on the canvas before code, as every page is (`docs/design/web-ui.md`). `farik connect <agent> <connector>` and `farik disconnect <agent> <connector>` do the same from the command line, and `connector.connected` and `connector.disconnected` record it. Step 01 built them for a server the user labels (spec 0.38, 6.7 and F9): the agent page's "Added by you" list, with "Connect again" for a server the team file changed since it was connected on this computer and "Remove", which asks first, and `ConnectorAdd`, three steps (how to start it and its keys, label its tools, done); the keys are pasted, since signing in is steps 03 and 03b. Step 05 connects a kit's connector by name through them.

## Steps

| Step | Delivers |
|---|---|
| 01 Connectors per agent | Moved from the ecosystem (ADR 0029): servers per agent, stdio and remote, tool listing and the user's tags, keys in the keychain or a private file, the hash of what was connected, `farik connect` and `farik disconnect` and the connector list on `AgentEdit`; it extends phase 6 step 12's connector base. `external_effect` tools are refused until step 02 |
| 02 Approving a connector's calls | Split from step 01: an `external_effect` call waits for the human like a question, who allows that one call or refuses it, from Today or the command line |
| 03 Signing in to a service | OAuth sign-in for remote servers, per agent, before the first kit (the founder, 2026-10-01): route 1. Built 2026-10-02 (spec 0.41, ADR 0033): Farik runs the sign-in and keeps the grant with the keys, `farik connect --sign-in`, "Sign in with <service>" on `ConnectorAdd`, "Sign in again" on the agent page |
| 03b Farik's registered apps | Route 2: GitHub by device flow, through a GitHub App the founder registers (ADR 0035); Google after the launch |
| 03c to 03e, deferred | The sign-in relay, signing in through it, and a homepage and a privacy policy, planned for Slack alone; deferred with Slack by the founder on 2026-10-02 until after the launch, to phase 12 steps 02b to 02d. Their plans stay in phase 7's folder; Slack takes a pasted key until then |
| 04 Skills per agent | Moved from the ecosystem (ADR 0029): skill folders at agent, role and team level, loaded into sessions. Built 2026-10-02 (spec 0.40, ADR 0034): the team's and an agent's skills load on demand through a per-session plugin folder, pinned in the team file and confirmed on this computer, the role's own staying in the prompt; a kit's skills (step 05) load through the same folder |
| 04b Skills on the agent page | Split from step 04: the agent page's Skills section and its add, edit and review dialogs. Built 2026-10-02 (spec 0.42): see the project plan's row for the changes in execution |
| 05 Kit format and loader | `kit.schema.json` and its generated types; `kit.yaml` per role, loaded with the role, its skills on demand; a kit connector connected by name through step 01's commands and screens, written whole into the team file with its tags applied; the pinned tool list of shipped kits and its drift test; the UI/UX Designer's built-in Playwright connector moved into its `kit.yaml`; the connector screens of 05 and 05b mocked up first |
| 05b Allowances | Split from step 05: allowances per tool and per agent (ADR 0037), the count read from `tool.called` per sprint or UTC day and kept by the daemon, on the board and the Costs page, and `ConnectorAllowance` as a step of the kit connect (`KitConnect`) and a dialog from the agent page |
| 06 Product Manager and Scrum Master kits | Their skills and connectors, each connector's server chosen and pinned, its setup copy written and checked in the web app |
| 07 Architect and Developer kits | The same, including the browser-testing connector as a host process and the security-review skill |
| 08 Marketing Specialist kit | The same, Higgsfield first, with the allowance flow proven end to end, its dangerous tools `denied`, and the publishing connectors always asking |
| 09 Finance Specialist role | Moved from the ecosystem (ADR 0029), since its kit needs the role: the role, its private books and its tools (`docs/design/finance-specialist.md`), with Stripe connected per agent |
| 10 Finance Specialist kit | The same; Stripe's connection moves into the kit from step 09's per-agent setup, and the ledger connectors are optional |
| 11 DevOps Engineer | The role, the deploy task, its three Farik tools, the watch tick and the incident flow (ADR 0027; `docs/design/devops-engineer.md`) |
| 12 DevOps Engineer kit | The same as the other kits, with four platform connectors, every write tool `denied` |
| 13 Kit check | Seven tasks in the web app, run by the founder on Claude, one per role: the Product Manager files an epic from an imported issue and an analytics read-back; the Scrum Master plans a sprint and mirrors the channel to the bridge, Slack connected by a pasted key; the Architect writes an ADR from library documentation and a vulnerability lookup; the Developer implements a task and proves it with a browser test; the Marketing Specialist produces a launch post with a Higgsfield image inside its allowance; the Finance Specialist closes a month with Stripe's numbers; the DevOps Engineer deploys a test project, then restores a deliberately broken deploy by restart, rollback, fix, review and redeploy. Recorded in `docs/milestones/role-kits.md` and signed off by the founder. Phase 8 runs them again on each engine and provider it adds |
| 14 Milestones 0 and 1 | Moved from phase 6 step 16 (ADR 0029): the recorded team sprint and the thirty-minute test, in the browser, with the fully equipped team |

## Tests

The step plans turn each of these into a test that fails first:
- A `kit.yaml` that names a skill folder that does not exist, a tool without a tag, a tool tagged `read`, or an allowance on a tool that is not `external_effect` fails validation.
- Every role's kit validates, and every skill it names loads into a session.
- A connector's live tool list that differs from the pinned one fails the drift test (run with the live tests, since it needs the service).
- A `denied` tool is absent from the session's tool list and refused by the hook if called.
- A `network`-tagged connector tool runs for an agent without the `network` tier; the tag, not the tier, governs.
- A call inside the allowance runs without a prompt; the first call beyond it asks; a batch tool and a publishing tool ask at any allowance.
- The allowance is counted per tool and per agent, resets with the sprint, and per day without sprints; the board's count matches the `tool.called` events.
- Connector output reaches the agent under the untrusted-content notice, and a message read from the chat bridge answers no question and approves nothing.
- Disconnecting one agent's connector removes its credential and server and leaves another agent's untouched.
