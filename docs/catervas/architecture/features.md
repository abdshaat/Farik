# What Catervas does today

One line each, with where it lives. Read from the code at commit `de0f2b8a`; the command names are
the variants of `enum Commands` in `crates/cli/src/lib.rs`, the browser screens are the routes in
`apps/web/src/app/App.tsx`.

## Setting up a project

- Make a repository a Catervas project, or rescan it: `catervas init` (`crates/cli/src/init.rs`; the scan in `crates/store/src/scan.rs`).
- First run in the browser: check the computer, connect the AI account, pick or create the project, review the scan, choose the team, its permissions and spending (`/setup/*` routes; `crates/runtime/src/daemon/setup.rs`, `crates/runtime/src/computer.rs`).
- Keep the model credential in the environment, the OS keychain or a private file (`crates/runtime/src/credential.rs`; ADR `0022`).
- Save and reuse team templates, kept privately in Catervas's state folder (`crates/runtime/src/templates.rs`, `apps/web/src/pages/SavedTeams.tsx`).
- Change the project from beside its name in the web app (`apps/web/src/pages/ChangeProject.tsx`; commit `b89e0428`).
- Show the team rules and the criterion library: `catervas rules show`, `catervas criteria list` (`crates/core/src/team.rs`, `crates/core/src/criteria.rs`).

## The team and its roles

- Eight shipped roles, each with a mandate, prompt, skills and kit: Product Manager, Scrum Master, Architect, Software Developer, Marketing Specialist, UI/UX Designer, Finance Specialist, Procurement Specialist (`crates/roles/roles/*/`, `crates/roles/src/lib.rs` `load_role`).
- Each role's default model and effort, overridable per agent in the team file (`role.yaml`, `crates/roles/src/lib.rs` `RoleDefinition`).
- Edit an agent, pause or retire it (`apps/web/src/pages/AgentEdit.tsx`, `/team/:id`).
- Add, show, confirm or remove skills in the Agent Skills format for the team or one agent: `catervas skill` (`crates/cli/src/skill.rs`, `crates/runtime/src/skills.rs`, `crates/roles/src/skill_check.rs`).
- Give an agent an MCP server with keys or a browser sign-in, label its tools, set allowances: `catervas connect` / `disconnect` (`crates/cli/src/connector.rs`, `crates/runtime/src/connectors.rs`, `sign_in.rs`, `allowances.rs`).
- Catervas's own connectors: OSV, Google Ads, exchange rates, recalls, eBay (`crates/runtime/src/{osv,google_ads,fx,recalls,ebay}.rs`; `catervas connector <name>`).

## Asking for work and planning it

- File a request, or write its contract with the Product Manager from a brief or an issue: `catervas task create`, `catervas contract new` (`crates/cli/src/task.rs`, `contract_new.rs`; `crates/store/src/requests.rs`).
- Triage a request as an epic or a task, or overrule the triage: `catervas triage` (`crates/cli/src/triage.rs`).
- Approve a contract, lock or unlock it: `catervas approve`, `catervas contract lock|unlock` (`crates/cli/src/contract.rs`; `/tasks/:id/plan`, `/tasks/:id/plan/edit`).
- Plan without doing: triage, contracts, breakdowns and assignments only: `catervas plan` (`TickRules::Planning` in `crates/runtime/src/orchestrator.rs`).
- Sprints: start, end, show, with an optional budget; planning, standup, review and retro ceremonies; a team policy that holds ready work in a Backlog until a sprint plans it (`crates/cli/src/sprint.rs`, `crates/runtime/src/sprints.rs`, `ceremonies.rs`, `orchestrator/rules.rs`; ADR `0028`).

## Running the team

- Drive the team until idle: `catervas run`; keep driving and serve the browser app: `catervas serve` on port 7420 or the nine after it (`crates/cli/src/run.rs`, `serve.rs`).
- One session at a time, each in the task's worktree on its own branch and a Docker sandbox per task (`crates/runtime/src/orchestrator.rs`, `sandbox/`; `crates/core/src/branch.rs`).
- The governor: transitions, readiness, permissions by tier, allowed and protected paths, approved sites, budgets, escalations (`crates/core/src/governor/`).
- Every tool call checked before it runs and recorded after, through Claude Code's hooks (`crates/runtime/src/daemon/hooks.rs`, `crates/cli/src/hook.rs`).
- Exit criteria run and judged by method `command`, `test`, `artifact` or `review` (`crates/runtime/src/criteria.rs`).
- Review by a fresh session of a different agent; the human accepts or sends back with criteria: `catervas accept`, `catervas send-back` (`crates/runtime/src/orchestrator/verify.rs`, `crates/roles/src/reviewer.rs`; `/tasks/:id/accept`; ADR `0024`).
- Questions, escalations and their resolution: `catervas answer`, `resolve`, `cancel` (`crates/runtime/src/orchestrator/human.rs`; the browser's side in `crates/runtime/src/daemon/gates.rs`; `/tasks/:id/questions`, `/tasks/:id/help`).
- Integrate an accepted task by the team's policy: `manual` merge, `auto_merge` with push, or `pull_request` through `gh` (`crates/runtime/src/orchestrator/integrate.rs`, `forge.rs`; `catervas integrate`).
- Allow or refuse one connector call an agent asked to make: `catervas tool approve|refuse` (`crates/runtime/src/daemon/hooks.rs`, `crates/runtime/src/orchestrator/human.rs`; ADR `0031`).
- Pause and resume the team; stop the run or one session: `catervas pause`, `resume`, `stop` (`crates/runtime/src/pause.rs`).
- Pick up a killed run: interrupted sessions recorded, leftover worktrees removed (`crates/runtime/src/orchestrator/recover.rs`).
- Agents sleep until their model provider's limit resets (`crates/runtime/src/sleep.rs`).

## Talking to the team

- The team channel with `@` mentions: `catervas say`, `catervas channel`; `/channel` (`crates/runtime/src/channel.rs`, `crates/cli/src/channel.rs`).
- Private one-to-one chats with each agent, answered while the team is paused: `catervas chat`; `/channel/:id` (`crates/runtime/src/chat.rs`; ADR `0026`).

## The UI/UX Designer

- A design plan the Product Manager approves before the Designer changes code (`crates/runtime/src/orchestrator/design.rs`, `tools/design.rs`; `apps/web/src/pages/DesignReview.tsx`).
- The project's preview in the sandbox, and a Playwright browser confined to it, used to check the Developer's UI changes (`crates/runtime/src/preview.rs`, `preview/docker.rs`).

## Marketing (Marketing Specialist)

- Marketing plans the owner approves, returns or ends: `catervas marketing plan` (`crates/core/src/marketing.rs`, `crates/runtime/src/marketing.rs`; `/marketing/plans/:id`).
- Scheduled social posts handed to Buffer, which the owner can stop until their time: `catervas marketing post` (`crates/runtime/src/orchestrator/hand_over.rs`, `tools/posts.rs`; `PostGoingOut.tsx`).
- Google Ads campaigns under a budget that is a hard stop (`crates/runtime/src/orchestrator/spend.rs`, `daemon/ads_calls.rs`, `google_ads.rs`; ADR `0042`).

## Finance (Finance Specialist)

- Private spreadsheet books in `.catervas/local/finance`, read and written as workbooks, and the team's AI spending (`crates/runtime/src/tools/sheets.rs`, `tools/costs.rs`; `crates/runtime/src/orchestrator.rs` private folders; ADR `0019`).

## Procurement (Procurement Specialist)

- Approved sites it may read, and sites it asks for: `catervas site` (`crates/roles/src/sites.rs`, `crates/store/src/sites.rs`; `Sites.tsx`).
- Purchase orders it drafts and the owner decides, places and receives: `catervas order` (`crates/core/src/order.rs`, `crates/runtime/src/procurement.rs`; `Orders.tsx`).
- Renewals flagged from the vendor register: `catervas renewal` (`crates/core/src/renewals.rs`; `Renewals.tsx`).
- Data pipeline requests the Product Manager decides, with paid or data-out ones sent to the owner: `catervas pipeline` (`crates/core/src/pipeline.rs`, `crates/runtime/src/orchestrator/pipeline.rs`).
- Messages to sellers the owner sends through the business's mailbox, replies read as untrusted text: `catervas procurement` (`crates/runtime/src/mailbox.rs`, `procurement/mail.rs`, `procurement/replies.rs`; `SellerMail.tsx`, `ProcurementMailbox.tsx`).

## Reading what happened

- The board, one line per task: `catervas board`; `/board` (`crates/store/src/projections.rs`).
- One task and everything that happened to it: `catervas task show`; `/tasks/:id` (`crates/cli/src/show.rs`).
- The event log, filtered: `catervas log`; `/events` (`crates/store/src/event_log.rs`).
- Costs and harness metrics: `catervas metrics`; `/costs` (`crates/store/src/metrics.rs`, `crates/core/src/pricing.rs`).
- Where the files and the log disagree: `catervas doctor` (`crates/store/src/reconcile.rs`, `crates/cli/src/doctor.rs`).
- Today: what waits on the human and what goes out soon; `/` (`apps/web/src/pages/Today.tsx`, `crates/store/src/waiting.rs`).

## Where the documents disagree with the code

- `README.md` ("Meet the team") says a team has one of five roles, "with a sixth, the Finance Specialist, planned". The code ships eight roles, the Finance Specialist among them (`crates/roles/src/lib.rs`, test `holds_every_shipped_role_to_its_schema`).
- `README.md`'s command table lists a subset of the commands; `send-back`, `connect`, `skill`, `pause`, `resume`, `chat`, `marketing`, `site`, `procurement`, `order`, `pipeline`, `renewal` and `tool` are in `crates/cli/src/lib.rs` and not in the table.
