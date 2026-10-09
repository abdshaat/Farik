# A UI/UX Designer, one-to-one chats and team templates

Status: the founder's decisions of 2026-09-30, written down for the founder's review before any step plan is written. ADR 0026 records the decision. It is the design input to phase 6 steps 11 to 14; the milestone runbook moves to step 15 (revision 23 of the project plan split the Designer into steps 11 and 12), and to step 16 since revision 25 (ADR 0028); revision 26 moves it to phase 7 step 12 (ADR 0029), revision 27 to step 14, and revision 41 to phase 9 step 03 (ADR 0049). Where this document says "decided here", the founder did not rule on the point and this design chose; ADR 0026 lists those points for the founder to confirm.

Three small fixes landed on the phase branch before this design: the Ready pill on the computer check, the waiting rows' buttons, and the rail's breathing Connected dot (`docs/design/web-ui.md`).

## Why

- **The Designer.** Farik is for non-technical users (ADR 0016), and what they judge a product by is its interface. Today nobody on the team owns it: the Developer builds screens as a side effect of a task, and the Architect reviews the code, not the page. The founder wants a role that looks at the running app the way a user does, plans changes before making them, and checks every screen the team ships.
- **Chats.** A user who wants to ask one agent something can only post in the team channel, where every agent reads it and a mention starts a reply in public. The spec's one-on-one (4.3) was planned for phase 8 step 04. The founder wants it in the product the Milestone 1 users test.
- **Team templates.** A user who has tuned a team (names, pictures, models, rules) must build it again by hand for every project. The founder wants a team saved once and used anywhere.

## Placement

Four steps join phase 6 before the milestone runs (three in revision 22; revision 23 split the Designer in two). Each starts with mockups the founder approves before any code, as every page did (the founder's standing gate, 2026-09-26).

| Step | Delivers | SPEC sections it changes |
|---|---|---|
| 11 | The UI/UX Designer and its plan gate | 3 (agent, role), 4.1 (the suggested six), 5.1, 5.2, 5.3, 5.4 (the Designer's reviewer, the plan gate), 5.6 (the Designer's tiers), 5.12 (`document_paths`), 6 (a new 6.8, and "only the Developer and the UI/UX Designer" in 6.1 to 6.5), 8.2 (the `explore` session), 8.5 (events), F1 |
| 12 | The Designer's preview, Playwright connector and design review | 4.1 (the preview commands), 5.4 (the design review), 5.6 (connectors), 5.12 (`ui_paths`), 6.7, 8.2 (the hook's connector check), 8.3 (the preview and browser containers), 8.5 (events), 8.6 (browsing only the preview), F9 |
| 13 | One-to-one chats | 3 (channel), 4.3, 5.1 (chat is not command), 5.5 (what a chat costs), 5.9, 8.2 (the `chat` session), 8.4, 8.5, F7, F8 |
| 14 | Team templates | 1 (the non-goal stays true), 3 (team), 4.1 (three starts), 4.4, 8.4 (the state folder), F1 |
| 15 | The milestone runbook, formerly step 11, unchanged in scope | none |

## A. The UI/UX Designer

### The role

- **Wire id** `ui_ux_designer`; display name "UI/UX Designer". It joins the `role` enum in `team.schema.json`, `task-contract.schema.json` and `role.schema.json`, and ships as `roles/ui_ux_designer/` (`role.yaml`, `system.md`, `skills/`), as every role does (spec 6).
- **In the suggested team.** `team.propose` suggests six: Product Manager, Scrum Master, Architect, Developer, UI/UX Designer, Marketing Specialist. The user may untick the Designer. The cap stays seven active agents, and a team still needs an active Product Manager and Developer (F1). The Finance Specialist stays optional and unsuggested (ADR 0019).
- **Mandate.** A developer focused on the interface. It explores the running app, plans changes to it, implements them once the plan is approved, and reviews every interface change the Developer makes.
- **What it may change.** Any code, like the Developer, on the tasks assigned to it. The rule "only the Software Developer changes code" (spec 0.11, the founder's decision of 2026-09-24) becomes "only the Developer and the UI/UX Designer change code". In practice:
  - the `document_paths` readiness rule (5.3, 5.12) holds every role but the Developer and the Designer;
  - its tasks work on `feature/FRK-<n>` or `fix/FRK-<n>`, by the contract's `change` (5.14);
  - 6.1, 6.2, 6.3 and 6.5 say "which only the Developer and the UI/UX Designer do".
- **Default tiers** (5.6): `read`, `write_workspace`, `execute`, `git_local`, the Developer's. The team's `permissions` answers apply to it as to the Developer (decided here): `run_commands: false` takes away `execute`, and `push: true` grants `git_remote`.
- **Default model**: the Developer's (Claude Opus 5 at `high`).
- **Character and tag colour**: open, for the founder to choose in the step 11 mockups. The tag colour follows the founder's rule of muted, light colours, one colour per job (`docs/brand/brand.md`).

### Its flow: explore, plan, approval, implement

Every task assigned to a Designer runs this way, whatever its risk:

1. **Explore.** Its first session has purpose `explore`. It holds the read tier's built-ins and the Playwright connector, and no `write_workspace`, `execute` or `git_local`, whatever the agent's tiers. It browses the preview of the task's worktree (below), which at this point is the integration branch's head, and writes its plan.
2. **Plan.** It ends the session with `farik_propose_design_plan { plan }`. The plan is plain text of 200 to 8,000 characters. It opens with a summary for the user (20 to 600 characters, then a blank line), as a note does (5.4). It says what the agent saw, what it will change, which screens and sizes, and what it will leave alone. The tool records `design_plan.proposed`. It is refused in any other session with `design_plan_refused`.
3. **The Product Manager's approval.** A new gate. Farik starts a Product Manager session of purpose `verify` about the task. Its message holds the contract and the plan, and its one Farik tool is `farik_decide_design_plan { approve, reason }`, which records `design_plan.approved` or `design_plan.returned`. A returned plan starts a new `explore` session with the Product Manager's reason. Each return counts, and when the returns reach the contract's `max_iterations` (plus any extra tries, ADR 0024), the governor escalates the task with reason `iterations`, as it does for rejections. The human's existing gates are unchanged: an epic's and a `high` risk contract's approval (5.16), and a `high` risk result's acceptance (5.4). The human sees the plan on the task's page, and is not asked to approve it.
4. **Implement.** Only once the latest `design_plan.proposed` has a later `design_plan.approved` does Farik start an `implement` session, with the agent's full tiers and the approved plan in its message. The governor enforces the order: a Designer's `write_workspace`, `execute` and `git_local` calls are denied with `design_plan_not_approved` in any session of a task whose plan is not approved. From here on the task is an ordinary task: completion note, `verifying`, review, acceptance.

The task stays `in_progress` from step 1 to step 4. The plan gate adds events, not lifecycle states, so spec 5.2's diagram is unchanged and only its gate text gains the rule.

### Reviews

- **Who reviews the Designer.** The Architect, because the author never accepts its own work (5.1). With no active Architect, a Developer reviews it (decided here, on the pattern of the Developer's own rule). F1 means a team always has a Developer, so a Designer's task always has a reviewer.
- **The Designer checks the Developer's interface changes first** (the founder, 2026-09-30: "The UI designer checks it in the browser before sending it to the architect for approval"). The design review comes before the code review, and only a change the Designer passed goes to the Architect. When a Developer's task is a UI change and the team has a Designer who is not retired:
  1. when the task enters `verifying`, Farik starts the Designer's `verify` session on the task's worktree, in a fresh sandbox as for any criterion run;
  2. the Designer opens the preview at phone width (360 px) and desktop width (1280 px), in the light and dark themes, and runs `farik_check_page` for each of the four, which is the accessibility check;
  3. it ends with `farik_record_design_review { pass, reasons }`, which records `design_review.recorded`. A fail sends the change back to the Developer with its reasons: the rejection is filed in the Designer's name, as a failed review does, and the Architect never sees the change. A pass sends it on;
  4. only when a passing `design_review.recorded` exists since the task last entered `verifying` does Farik start the reviewer's session (5.4), which then runs as today.

  How this maps onto the reviewer mechanics (decided here, so that the design changes nothing about who a contract names): the contract still names the Architect as its reviewer (5.4, 5.16), and the Designer is never named in a contract as a reviewer. The Designer's pass is the event `design_review.recorded { pass: true }`, a gate on the start of the reviewer's session, in the way a passing criterion run gates it, and not a lifecycle state: the task stays in `verifying` throughout. The transition table gains no row; its `verifying → rejected` row names the reviewer or, for a UI change, the Designer, whoever fails it first. The board shows "waiting on the Designer" while the only Designer is paused. The Definition of Done (5.4) gains an item: a UI change on such a team needs the Designer's pass since the task last entered `verifying`, and the reviewer's review is not started before it. A send-back that returns the task to `verifying` runs both again, in the same order.

  The Architect still reviews the Designer's own tasks (above), and still reviews a Developer's non-UI work as before. With no Designer on the team, none added or all retired, no design review exists and the Architect reviews alone, as today. A change that is both UI and code gets both: the Designer's pass first, then the Architect's review of the whole diff.
- **What counts as a UI change** (decided here: both, paths and a field). A Developer's task is a UI change when either holds:
  - **its diff touches a UI path.** A new team rule, `ui_paths` (globs, 5.12), matched against the task's diff by the governor when the task enters `verifying`. Its default, when the key is left out, is `**/*.tsx`, `**/*.jsx`, `**/*.vue`, `**/*.svelte`, `**/*.css`, `**/*.scss`, `**/*.html`. Settings shows it in the advanced view, as it shows `document_paths`.
  - **its contract says so.** A new optional boolean contract field, `ui_change`, which the Product Manager or the Scrum Master sets for interface work the globs do not see, such as the words in a strings file.

  Why both: the diff is mechanical and cannot be forgotten, since a contract's author may never think of it, and it is judged at `verifying` on the real change, exactly, with the glob matcher the protected paths already use. The field covers what no glob can name. Paths alone would miss copy changes, and the field alone would trust the author. Matching `allowed_paths` against `ui_paths` at readiness was rejected: two globs can overlap without either lying within the other (`**/*.tsx` and `apps/web/**`), and the diff answers the question exactly.

### The preview

- **The commands** (amended 2026-09-30 by the founder's decision D2; step 12's plan has the detail). The user sets them once in Settings, under "How to open your app", and the Designer's card in setup asks for them:
  - `preview.prepare` (optional): installs and builds, in the sandbox with the network on, for at most 15 minutes, reused while the task's committed tree and the command are unchanged;
  - `preview.start`: the command that starts the app, in the sandbox with the network off;
  - `preview.port`: the port it serves on;
  - `preview.path`: the page to open first, default `/`.

  The command is kept in `team.yaml` under a new top-level `preview` (decided here: it travels with the repository, like the criterion library's commands, and agents cannot write `.farik/`, 5.8). Templates leave it out, as they leave out paths and checks. For Farik's own repository the commands build and start Farik's own web app, through the `farik-e2e-serve` test binary over a recorded team; step 12's plan writes them out.
- **Where it runs** (amended 2026-09-30, the founder's decision D3). `prepare` runs in a container of the task's sandbox image with the network on; `start` runs in a preview container of that image, with the task's worktree mounted and the network off. Farik starts it when a Designer's session, or a design review, starts, and stops it when the session ends. It waits up to 120 seconds for the page to answer inside the container, then escalates the task with the preview's output tail. It never runs on the host, because it runs code an agent wrote. The Designer needs Docker's sandbox: in no-sandbox mode, or without Docker, it is unavailable.
- **With no preview command set**, a Designer's task is not assigned and a design review does not start. Today's "Waiting on you" shows "Tell Farik how to open your app", linking to Settings.

### The Playwright connector

"Just enough, built to grow": the base phase 7 extends, not a special case it replaces.

- **Per agent in the team file.** An agent gains `mcp_servers: [{ name, source }]`. Step 12 accepts one `source`, `builtin`, and one built-in, `playwright`. `team.propose` gives it to the Designer. The agent editor lists an agent's connectors with a switch for each built-in. It is on by default for the Designer only, and any agent may have it.
- **The built-in's definition** ships in the `farik` binary, as the `container` connector `playwright` of the Designer's kit, `roles/ui_ux_designer/kit.yaml` (moved there in phase 7 step 05, ADR 0036; it was `crates/roles/connectors/playwright.yaml`):
  - the server: the official Playwright MCP server's container image, pinned by digest;
  - its arguments: headless, isolated, `--allowed-origins` set to the preview's origin, and an output folder in the session's folder;
  - its tool list, pinned;
  - each tool's tag, in role-kits' vocabulary (`network`, `external_effect`, `denied`).

  A test lists the pinned image's tools and fails when they differ from the pinned list, which is role-kits' drift test in its first form. It runs with the integration tests, since it needs Docker, not the live service.
- **Tags.** The tools that navigate, read the page, click, type, press keys, resize, wait, and take screenshots are `network`: they reach only the preview and change nothing outside the sandbox. Everything else is `denied` and never offered: running script in the page, uploading files, installing browsers, saving PDFs, and any tool the pinned version adds before a person tags it. The step plan pins the exact names from the pinned version.
- **The browser is confined to the preview:**
  - it runs in a container that shares the preview container's network namespace, which has the network off, so it reaches only the preview;
  - Chromium runs with `--proxy-server` pointed at a dead port, with loopback bypassed, so no click, redirect or page script can leave (the founder, 2026-09-30, D3);
  - the server's `--allowed-origins` limits the browser to the preview's origin;
  - the governor checks every `url` argument of every Playwright call against the preview's origin (`http://localhost:<port>`), and denies anything else with `url_outside_preview`. This is Farik's own check, so the confinement is not the server's promise alone (5.1: governance is code).
- **The governor sees every connector call.** The hook judges `mcp__<server>__<tool>` by the server's entry in the agent's `mcp_servers` and the tool's tag, and denies an unknown server or an untagged tool, as it does today for any server that is not Farik's (5.6). `tool.called` and `tool.denied` gain `server` and `tag` for a connector's call, so the log shows each call with its tier and tag. A connector tool runs whatever the agent's own tiers, as role-kits decided, and only in the sessions that give it: `explore`, `implement` and the design review, never a chat.
- **The accessibility check** is a Farik tool, `farik_check_page { path, width: phone | desktop, theme: light | dark }`, tier `read`, offered only in the Designer's sessions. Farik runs it itself, with no agent in the loop, in a browser container beside the preview:
  - it opens the page at 360 or 1280 px, with the colour scheme emulated;
  - it runs axe-core (the version the web app's tests pin, bundled in the binary) for WCAG 2.2 A and AA;
  - it returns the violations, each with its rule, impact, element and help text, under the untrusted notice, and a screenshot as the tool's image result.

  The agent never gets a tool that runs script in the page; Farik's own code does.
- **Engine-neutral.** It is an MCP server and a Farik MCP tool, so any engine that speaks MCP can use it (ADR 0023). On an engine without a pre-tool hook, phase 10 routes connector calls through Farik's own server, as ADR 0023 requires for every tool.

### The Designer's kit

Skills, in the Agent Skills format, under `roles/ui_ux_designer/skills/`:

| Skill | What it teaches |
|---|---|
| `ux-review-heuristics` | Reviewing a screen against usability heuristics (visibility of status, match with the user's words, error prevention, recognition over recall), with a checklist per screen |
| `wcag-accessibility-checks` | WCAG 2.2 AA: reading `farik_check_page`'s results; contrast, focus, target size (2.5.8), labels and names, reflow at 320 CSS px, motion; what axe cannot see and must be checked by eye |
| `brand-and-design-tokens` | Reading and using the project's brand and design tokens; never inventing a colour or a size; for Farik, `@farik/brand` and `docs/brand/brand.md` |
| `plain-language-interface-wording` | Words on a screen for a non-technical user: sentence case, verbs on buttons, no jargon, errors that say what to do |
| `writing-mockups` | Writing a plan's mockups as annotated wireframes and screen descriptions a Product Manager can approve |
| `responsive-and-phone-checks` | Checking 360, 390 and 1280 px, touch targets, no sideways scroll, the phone's bottom bar |

Connector: the Playwright connector above. Phase 7 moves both into `roles/ui_ux_designer/kit.yaml` (step 05), and its kit check (step 13, phase 9 step 02 since ADR 0049) gains a seventh task, a Designer's: a screen explored, planned, approved and changed, and a Developer's change design-reviewed.

### Events, tools and queries (steps 11 and 12)

- **Events**, `<entity>.<past_tense_verb>`, each about one task:
  - `design_plan.proposed { plan }`, by the Designer;
  - `design_plan.approved { reason }` and `design_plan.returned { reason }`, by the Product Manager;
  - `design_review.recorded { pass, reasons, checks: [{ width, theme, violations }] }`, by the Designer;
  - `preview.started { port }` and `preview.stopped { reason }`, by the governor.
- **Farik tools**: `farik_propose_design_plan`, `farik_decide_design_plan`, `farik_check_page`, `farik_record_design_review`.
- **Session purposes**: `explore` joins `SessionPurpose`. The Product Manager's plan decision and the design review are `verify` sessions.
- **Contract**: optional `ui_change: boolean`.
- **Team file**: `ui_ux_designer` in the role enum; `agents[].mcp_servers`; `rules.ui_paths`; top-level `preview { prepare?, start, port, path }`.
- **RPC**: `settings.defaults` gains `ui_paths`; `task.get` gains the plan and the design review; `team.propose` suggests six; the existing `team.save` saves `preview` and `mcp_servers`.
- **Pages**: the Designer in the team builder and on the Team page; "How to open your app" in Settings and in setup; the plan on the task's page ("The plan", with the Product Manager's decision); the design review's four checks on the task's page, each with its screenshot; the waiting row for a missing preview.

## B. Chats

### What the user sees

- The Channel page becomes a chat list. The first chat is **Team**, today's group channel, unchanged. Below it is one private chat per agent who is not retired, each with its avatar, name, role and last line. A retired agent's chat keeps its history, read-only, under "Past teammates". The rail's label is settled in the step 13 mockups ("Chats", confirmed by the founder on 2026-09-30), and the address `/channel` stays, with `/channel/<agent_id>` for a one-to-one.
- **A one-to-one follows spec 4.3's read-only rule.** The agent answers from its memory and read access to the project, and changes nothing.
- **"Send as a request".** When the agent thinks work is needed, its reply carries a proposed request, shown under the reply as the request's words with a "Send as a request" button. The user may edit the words first. Pressing it files a request through the existing `request.file`, triaged and contracted like any other (5.16). The chat then shows "Sent as FRK-12" with a link. Nothing is filed without the user.
- **Private.** A chat is never posted to the team channel, never in the channel summary, and never shown to another agent. The agent itself sees its own chat's history only in its later chats, never in its task sessions (decided here).
- **History is kept**, in the event log, which is machine-local and never committed (8.4).
- **An agent answers while the team is paused.** The pause stops work; a chat is not work. A paused agent answers too (decided here: pausing means it takes no work). A retired agent does not.
- **Cost.** A chat's cost is recorded under the purpose `chat`, shown on the Costs page as "Conversations". It counts toward the daily budget, and no chat starts on a day whose budget is spent (decided here: the budget is the user's spending ceiling). The chat then says so, in plain words, as it does when the agent sleeps at its provider's limit.
- **Mockups first**: `Chats` (the list, with Team first), the existing `OneOnOne` redrawn with the proposal and its button, and `PhoneChats`.

### How a one-to-one is recorded

- **Event** `chat_message.posted`, about no task:
  - `{ chat: <agent_id>, author: "human", text }` for the user's message;
  - `{ chat: <agent_id>, author: <agent_id>, text, in_reply_to: <seq>, request?: { title, text } }` for the agent's reply.

  The text is 1 to 4,000 characters, and line breaks are kept (a chat is not the channel's one line). It is a kind of its own, not a `message.posted`, so nothing that reads the channel reads it: not `channel.messages`, not the channel summary, not `farik channel`.
- **Sending a proposal**: `request.file` gains an optional `from_chat_message: <seq>`, and `task.created` carries it, so the chat can show which request came from which reply. No new event.
- **Sessions**: `session.started { purpose: chat, chat: <agent_id> }` and `session.ended`, as any session.
- **Command** `chat_message_post { agent_id, text }`, over the RPC's `command`, and at the command line `farik chat <agent> <text>` (and `farik chat <agent>` to print the history), sent to the driving process as every command is (ADR 0014).
- **Queries** `chats.list`, giving each chat's agent and last message, and `chat.messages { agent_id, before_seq?, limit }`, shaped as `channel.messages` is.

### The chat session, and the read-only guarantee

- **When it runs.** A chat is pending when its latest message is the user's and no chat session has started since it was posted. A tick rule, the first in the order, starts one `chat` session for the oldest pending chat whose agent is active or paused and not asleep, while the daily budget has room. It is the one rule that runs while the team is paused (5.2's "no rule runs" gains the exception). The command wakes `farik serve` at once.
- **Model.** The agent's own model at `low` effort (decided here: the agent answers in its own voice and knowledge, briefly; `low` keeps a chat cheap).
- **Prompt.** The usual order (ADR 0011):
  - the role, the persona, the memory, the project scan, the team rules and the decisions, each as today;
  - the chat's history, the last 16 KiB, oldest first. The user's lines are the user's own; the agent's are wrapped `untrusted`, as agent-written text is;
  - a closing instruction: answer once with `farik_chat_reply`; you can read the project but change nothing; when work is needed, put a request in the reply for the user to send.
- **Tiers.** The read tier alone, whatever the agent's own grants:
  - built-ins `Read`, `Glob`, `Grep`, `ToolSearch`, in the project's root, under the protected-path deny rules (8.2);
  - Farik tools `farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_criteria`, `farik_read_decisions`, and `farik_chat_reply { text, request?: { title, text } }`;
  - no connector, no web tool, no `farik_write_memory`, no `farik_post_message`, no `farik_create_task`.

  The session is registered with those tools, so the hook denies any other with `tool_not_in_session` (8.2). `farik_chat_reply` records the reply, refuses a second call and any call outside a `chat` session (`chat_reply_refused`), and ends the session. So a chat's guarantee is the governor's, not the prompt's: the one thing a chat session can write is its reply, and the one way from a chat to work is the user's own button.
- **This pulls one-on-ones forward from phase 8 step 04.** It replaces phase 8's decision that a one-on-one files a `draft` itself through `farik_propose_task`: the agent proposes, and the user files. Phase 8 step 04 (now phase 11 step 01) keeps the memory history with revert and the decisions view.

## C. Team templates

### What a template holds

- **Per agent**: the role, name, persona, picture (a shipped avatar's name, or an uploaded image copied with the template), and the model and effort. Persona is included (decided here): it is the agent's character, not the project's.
- **The team's rules**, the four answers setup asks:
  - permissions (`policy.permissions`);
  - the plan check (`policy.judgment`);
  - spending (`budgets.daily_usd`);
  - finishing work (`policy.integration`).
- **Not held**: anything project-specific, which means `rules` (paths, commands, criteria), the criterion library, `preview`, `integration_branch`, per-agent grants and revokes, connectors, and the team's name. A template's other policy settings are not held either. Applying one keeps the project's own values, or the defaults in setup.

### Where it lives

Machine-local, in the user's Farik state folder beside `state.json`: `templates/<slug>.yaml`, one file per template, readable by its owner alone, held to `docs/schemas/team-template.schema.json`:

```yaml
version: 1
name: My usual team        # 1 to 60 characters; the slug is the file's name
saved_at: 2026-09-30T12:00:00Z
agents:
  - { display_name: Mira, role: product_manager, persona: "...", avatar: pm-01, model: { id: claude-opus-5, effort: high } }
policy:
  permissions: { run_commands: true, push: false }
  judgment: { required: always, judge: auto }
  integration: auto_merge
budgets: { daily_usd: 20 }
```

Every project on the machine sees every template. Saving and deleting a template records no event, since no project's log owns it, as browser sessions record none. Applying one records the `team.updated` that any team save records, with `template: <name>`.

### Where the user meets it

- **Setup's "Your team" step** offers three starts: the suggested team (six), a saved team, or from scratch (an empty builder that asks for a Product Manager and a Developer first). A saved team fills the builder, and the user may change anything before Continue.
- **The Team page** gains "Save as a template", which asks for a name and saves the current team, and "Use a saved team", which shows, before anything is saved, who joins, who is kept, and who is retired or removed.
- **Switching keeps the safety rules.** Using a saved team on a live team is one `team.save`, checked as a whole (4.4):
  - an agent in the template whose id and role match an active agent is kept, and takes the template's picture, persona, model and effort;
  - every other agent who has worked, meaning any event names it, is retired, not deleted; its unfinished tasks are blocked with "agent retired by the user", as a retirement does today;
  - an agent who never worked is removed;
  - the template's other agents are added, with an id suffixed when it clashes with a retired agent's;
  - the result must have an active Product Manager and Developer and at most seven active agents, and a refusal says what to change, as `team.validate` does.
- **One team per project stays true.** A template is reuse, not a second live team (spec 1, "One human, one team"; spec 3).

### RPC (step 14)

- query `templates.list` → `[{ name, saved_at, agents: [{ display_name, role, avatar }] }]`;
- query `template.get { name }` → the template;
- method `template.save { name, replace? }`, from the current team, refused with `template_exists` when the name is taken and `replace` is not set;
- method `template.delete { name }`;
- `team.propose` gains `from?: "suggested" | "empty" | { template: <name> }`, default `suggested`, and answers a team file for this project with the switching rules applied, which the page shows as before and after and saves with `team.save`.

Mockups first: the three starts in `SetupTeam`, "Save as a template" and "Use a saved team" on `Team`, and the before-and-after dialog.

## How phases 7 and 8 extend the connector base

Nothing step 12 builds is replaced; each later step widens it.

| Later step | Extends |
|---|---|
| Phase 10, engines and providers | Runs the Playwright connector and `farik_check_page` on each engine; an engine without a pre-tool hook reaches connector tools only through Farik's own server (ADR 0023) |
| Phase 7 step 01, connectors per agent | `mcp_servers` gains `source: custom`, with `transport`, `command` and `args` or `url` and `headers`, `credential_keys` and the user's `tools` tags; per-agent keys in the keychain or a private file (8.6); tool listing with `rmcp`; `SessionConnector.origin` becomes optional. The hook's server-and-tag check, `tool.called`'s `server` and `tag`, and the per-agent list are step 12's, unchanged. The note on this step in the project plan says so |
| Phase 7 step 02, approving a connector's calls | Per-call approval of `external_effect` (`tool_approve`) |
| Phase 7 step 04, skills per agent | The Designer's skills load at the role level as every role's do; agent and team levels are added around them |
| Phase 11 step 01, memory and decisions view | Keeps the memory history with revert and the decisions view; the chat is step 13's |
| Phase 7 step 05, kit format | `source: kit`; the built-in's definition moves into `roles/ui_ux_designer/kit.yaml`; allowances and `ConnectorAllowance`; a kit connector connected through step 01's screens; the drift test becomes the kits' |
| Phase 7 step 07, Developer kit | The Developer's browser testing reuses the Playwright connector and the preview, on the Developer's own task |

## Risks

- **A heavier machine.** The Designer needs Docker, which the sandbox needs anyway, and two more images: the Playwright server's, some hundreds of megabytes, and the preview's. The computer check gains a row for the browser image when the team has a Designer, and builds or pulls it as it builds the sandbox image.
- **Previews differ by project.** A preview that needs a database, secrets, or a network the sandbox does not give will not start, and the user who set the command may not know why. The task escalates with the preview's output tail rather than guessing. There is no host path: the Designer needs Docker's sandbox.
- **Two reviews cost more.** A UI change now takes a design review and then a code review, and a Designer's task takes a plan session and the Product Manager's decision before any code. Both are `verify` and `explore` costs the Costs page shows. A team that finds this too slow can untick the Designer.
- **The `ui_paths` defaults are a guess.** They catch React, Vue, Svelte and plain web projects, and miss others (a native app's layout files). Settings' advanced view is the fix, and the step plan checks the defaults against the scan's detected stacks.
- **A third-party server.** The Playwright MCP server is Microsoft's, and its tools change between versions. The pin, the drift test, and denying any tool a person has not tagged are the defence. Every pin update re-reviews the tags (role-kits).
- **Emulating the colour scheme and the width** depends on what the pinned image supports. `farik_check_page` is Farik's own script in the Playwright image, not the MCP server's tools, so this depends on Playwright's library, which supports both. The step plan confirms it on the pinned image.
- **Chats cost money in the background.** A user who chats a lot while the team is paused is still spending. The daily budget still stops chats, and "Conversations" on the Costs page shows it.
- **Privacy is local, not secret.** A chat is kept out of the channel and away from other agents, but it lives in the event log like everything else, and `farik log` shows it to anyone at the machine.
- **Templates and providers.** A template saved with a model the project's provider cannot run (phase 10) falls back to the role's default, and the before-and-after dialog says so.
- **Milestone runbook.** Phase 9 step 03's team sprint (phase 7 step 14 until ADR 0049, phase 7 step 12 until revision 27, phase 6 step 16 until ADR 0029, step 15 until revision 25) uses seven agents, the six of phase 4 and the Designer, the cap (decided by the founder, 2026-09-30). Its two requests are CLI work, so the Designer's review of UI changes occurs only if the run touches UI files.

## Open items

- **The Designer's character and tag colour.** The founder chooses both in the step 11 mockups.
- **The rail's label** for the chat list: "Chats" (the founder, 2026-09-30).
- **Farik's own preview command**: a `prepare` and a `start` command, both in the sandbox (the founder, 2026-09-30), written out in step 12's plan.
- **Every point marked "decided here"**, which ADR 0026 lists for the founder to confirm.
