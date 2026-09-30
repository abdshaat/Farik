# Phase 6, step 11: The UI/UX Designer and its Playwright connector

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 3, 4.1, 5.1 to 5.4, 5.6, 5.12, 6 (6.1 to 6.5, 6.7, a new 6.8), 8.2, 8.3, 8.5, 8.6, F1, F9
Depends on: steps 01 to 10 of this phase (landed); ADR 0026 and `docs/design/designer-chats-templates.md` (accepted 2026-09-30), whose decisions are binding and not restated here
Readiness confirmed by: (pending)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A team can have a sixth suggested agent, the UI/UX Designer. It explores the running app in a browser, writes a plan, and waits for the Product Manager's approval. Only then does it change code, and the Architect reviews its work. When a Developer's change touches the interface, the Designer checks it first: at 360 and 1280 px, in both themes, with an accessibility check. Only a change it passes reaches the Architect. The browser is a built-in Playwright MCP connector, given per agent in `team.yaml`. It can reach only the preview Farik starts from the command the user sets in Settings, and the governor checks every one of its calls. The user sees the Designer in setup and on the Team page, sets "How to open your app", and reads the plan, the Product Manager's decision and the design review on the task's page and on the gate. Out of scope: custom connectors, credentials and per-call approval (phase 8 step 01); kits (phase 9); chats (step 12); templates (step 13).

## Decisions

The ADR's and the design's decisions hold as written: `ui_ux_designer`, the six suggested, the Designer's tiers and permissions, the flow, the plan's shape, the reviews, `ui_paths` or `ui_change`, the preview, the connector, `farik_check_page`, the events and the tools. Below is only what they left to this plan.

- **The founder's gate (standing rule of 2026-09-26).** Task 1's mockups are approved by the founder before Task 2 starts. The approval, and the character, tag colour and name he chose, are recorded in this section in Task 1's commit. A choice different from the defaults below changes only data (`role.yaml`, `tokens.json`, the avatar key), not a task.
- **Name and persona**, the defaults offered in the mockup: Iris, "Makes it clear, calm and easy to use". Id `iris`. The design left the name open. The pool (Noor, Ivo, Lena, Sami, Rui) stays for extra agents.
- **Character options:** the five existing `extra-1` to `extra-5` (`docs/brand/assets/characters/`), each shown on the Team card. No new art, because the design asks the founder to choose from what exists.
- **Tag colour options.** Three muted, light candidates, each distinct from the five role colours and each passing the contrast test against `role-ink` #161616:
  - dusty teal `#9EC3BE`;
  - soft peach `#E3B89A`;
  - pale clay `#CDB29C`.
  The chosen one becomes the token `role-ui-ux-designer` in both themes, as the other roles' colours are.
- **No Docker on the machine.** The browser always runs in Docker, since the connector ships as an image. Without Docker:
  - `team.propose` still lists the Designer, unticked, with "Needs Docker";
  - a Designer already on the team has no task assigned;
  - a UI change waits in `verifying`, under the waiting row "The UI/UX Designer needs Docker to open your app. Install Docker, or retire the Designer";
  - retiring the Designer lets the Architect review alone, as the design says.
  Rejected: skipping the design review silently, which would be a governance hole.
- **No-sandbox mode with Docker present** (8.3). The preview runs on the host, as the design says. The browser container runs with `--network host`, so its confinement is the server's `--allowed-origins` and the governor's URL check. The no-sandbox warning already covers this. It is Linux's behaviour, and it is what the e2e journey runs on.
- **The URL rule.** The governor checks every string field named `url`, at any depth of a connector call's input. Each must be the preview's origin `http://localhost:<port>`, either exactly or followed by `/`, `?` or `#`. Anything else is denied with `url_outside_preview`: another host, `127.0.0.1`, another port or scheme, userinfo, and relative URLs. Rejected: parsing and normalising the URL, because a prefix match with a fixed delimiter is exact and cannot be fooled by `@` or by a longer port.
- **The tags**, by the tool names of the Playwright MCP server:
  - `network`: `browser_navigate`, `browser_navigate_back`, `browser_snapshot`, `browser_click`, `browser_hover`, `browser_drag`, `browser_type`, `browser_fill_form`, `browser_select_option`, `browser_press_key`, `browser_handle_dialog`, `browser_resize`, `browser_wait_for`, `browser_take_screenshot`, `browser_console_messages`, `browser_tabs`, `browser_close`;
  - `denied`: `browser_evaluate`, `browser_run_code`, `browser_file_upload`, `browser_install`, `browser_pdf_save`, `browser_network_requests`, and any tool the image lists that the table does not.
  - `external_effect` is denied in this step, since no approval flow exists before phase 8.
  - Denied tools also go to `--disallowedTools`, so they are never offered.
- **The pin.** `mcr.microsoft.com/playwright/mcp` at its newest release on the day Task 4 starts, pinned by digest. The tag, the digest and the image's Node module root are recorded in `playwright.yaml` and in this plan's Interfaces. The drift test reconciles the table above with the image: a tool the image adds is written into the table as `denied` in the same commit.
- **Which sessions get the connector.** A session gets it when:
  - its agent has it in `mcp_servers`;
  - its purpose is `explore`, `implement` or the design review;
  - the team has a preview.
  Farik starts the preview for such a session, and a Developer with the switch on gets it in `implement`. With no preview command, a non-Designer's session runs without the connector.
- **The design review's checks are Farik's own measurements.** `farik_check_page` records a new event, `page.checked { width, theme, path, violations, screenshot }`. `farik_record_design_review` copies its session's four latest checks into `checks`. It is refused with `design_review_incomplete` until all four width and theme pairs have been checked in that session. Rejected: trusting the agent's list, because the pass gates the Architect (5.1: governance is code).
- **Screenshots** are written to `.farik/local/screenshots/<task>/<session>-<width>-<theme>.png`, which is machine-local and never committed. Farik's MCP server returns the PNG as an image block beside the JSON. The page reads it through a new query, `task.screenshot { task_id, file }`, answering `{ png_base64 }`.
- **axe-core 4.13.0**, the version `packages/ui` pins, is vendored as `crates/runtime/assets/axe.min.js` with its licence. A test checks that its version banner equals `packages/ui/package.json`'s pin. Rejected: `include_str!` from `node_modules`, which would make the Rust build need `pnpm install`.
- **The check script.** `crates/runtime/assets/check-page.mjs` is Farik's own code. It runs in the pinned Playwright image with `--entrypoint node` and the preview's network. It takes Playwright from the module root that `playwright.yaml` records. The agent never gets it.
- **The Designer's rejection.** `TransitionContext` gains `design_reviewer: Option<String>`. The `verifying → rejected` row's `Reviewer` actor is satisfied by the contract's reviewer, or by that id when it holds the latest failing `design_review.recorded` since the task entered `verifying`. No row is added. The Definition of Done gains `DoneRule::DesignReviewPassed`.
- **When a change counts as a UI change.** It is judged from the task's diff each time rule 5 runs, since the diff does not move while the task is `verifying`, with the contract's `ui_change` as the other trigger. No event records it: `task.get` reports it.
- **No preview command.** This is a new assignment refusal, `preview_not_set` (`check_assignment` in core), for a Designer assignee. Today gains a waiting row, "Tell Farik how to open your app", linking to Settings.
- **`preview` in `team.yaml`:**
  - `command`: 1 to 500 characters;
  - `port`: 1024 to 65535;
  - `path`: starts with `/`, default `/`.
  `validate_team` refuses a Designer's `mcp_servers` naming an unknown built-in (`unknown_connector: <name>`).
- **The plan gate is read from the log at each call.** It is the latest `design_plan.*` event of the session's task. It is judged in both the hook and `call_tool`, with the refusal `design_plan_not_approved` for the Designer's `write_workspace`, `execute` and `git_local` calls. Rejected: fixing it at session start, because a plan proposed after the session started must close it.
- **The tools' tiers.** `farik_propose_design_plan`, `farik_decide_design_plan`, `farik_check_page` and `farik_record_design_review` are all `read`: they write only the log. Each also checks its own session's purpose and role, so each is refused with its own `*_refused` kind outside the session that gives it.
- **Farik's own preview** (the design's open item, for the founder to confirm): `cargo run -q -p farik --features e2e --bin farik-e2e-serve -- --preview --port 4400`, port 4400, path `/`. `--preview` is new and exists only in the `e2e` build. It opens a temporary project holding step 08's recorded team, and admits a browser from `localhost` without the one-time code. The shipped `farik` has no such path.
- **The mockups are new files**, so the approved ones stay untouched: `DesignerTeam`, `SettingsPreview`, `DesignPlan`, `GateDesignReview`. They are registered in `canvas.json` under the existing pages `team`, `settings`, `daily` and `gates`.

## File map

```
docs/design/mockups/{DesignerTeam,SettingsPreview,DesignPlan,GateDesignReview}.dc.html, canvas.json   creates / modifies (T1)
docs/schemas/{team,task-contract,role,event}.schema.json                  modifies (T2): role, ui_paths, ui_change, preview, mcp_servers, events
crates/core/src/{team.rs,team/defaults.rs,team/describe.rs,branch.rs}     modifies (T2)
crates/core/src/governor/{permissions.rs,readiness.rs,team_rules.rs,gates.rs,done.rs,transition.rs}  modifies (T2), + tests
crates/protocol/src/event.rs                                              modifies (T2)
crates/roles/roles/ui_ux_designer/{role.yaml,system.md,skills/*/SKILL.md} creates (T3)
crates/roles/roles/*/system.md, crates/roles/src/{lib.rs,reviewer.rs}      modifies (T3)
crates/runtime/src/daemon/team.rs                                         modifies (T3 propose; T7 wire)
packages/brand/tokens/tokens.json, packages/brand/assets/avatars/          modifies (T3): the chosen colour and avatar key
crates/roles/connectors/playwright.yaml, crates/roles/src/connectors.rs   creates (T4)
crates/runtime/src/{preview.rs,preview/docker.rs,preview/host.rs}          creates (T4)
crates/runtime/src/{daemon.rs,daemon/hooks.rs,claude.rs,session.rs,computer.rs}  modifies (T4)
crates/runtime/tests/playwright_connector.rs                              creates (T4, T5): Docker, #[ignore = "needs docker"]
crates/runtime/assets/{axe.min.js,axe-LICENSE.txt,check-page.mjs}          creates (T5)
crates/runtime/src/{tools.rs,tools/design.rs,daemon/mcp.rs}                modifies / creates (T5, T6, T7)
crates/runtime/src/orchestrator/{rules.rs,verify.rs,design.rs,messages.rs}  modifies / creates (T6, T7)
crates/runtime/src/recorded/{fixtures.rs,transcripts/*.jsonl}              modifies / creates (T6, T7, T10)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, packages/protocol-client/src/*  modifies (T7)
apps/web/src/pages/setup/{SetupTeam,TeamSetup}.tsx, pages/{Team,AgentEdit,Settings,TeamRules}.tsx, strings/en.ts (+ tests)  modifies (T8)
apps/web/src/pages/{TaskDetail,Gate,Today,Board}.tsx, app/lanes.ts, strings/en.ts (+ tests)  modifies (T9)
crates/cli/src/bin/farik-e2e-serve.rs, crates/cli/tests/serving.rs         modifies (T10)
apps/web/e2e/{designer.spec.ts,fixtures/serve.ts}                         creates / modifies (T10)
docs/SPEC.md, docs/plans/project-plan.md                                  modifies (T11)
```

## Interfaces

Consumes: `Team`, `Agent::tiers`, `TeamRules`, `default_tiers`, `evaluate_tool_call`, `check_assignment`, `evaluate_done`, `TransitionContext` (`farik-core`, on this branch); `default_reviewer_role` (`farik-roles`); `SessionRegistration`, `ToolContext`, `SessionAsk`, `run_session`, `McpServerConfig`, `SandboxFactory`, `check_computer`, `RecordedAdapter::with_tools` (`farik-runtime`); `startServe` (`apps/web/e2e/fixtures/serve.ts`).

Produces:

```rust
// farik-core
Role::UiUxDesigner; RoleWire::UiUxDesigner;  plain_role(Role::UiUxDesigner) == "UI/UX Designer"
pub fn changes_code(role: Role) -> bool;                       // SoftwareDeveloper | UiUxDesigner
pub struct Preview { pub command: String, pub port: u16, pub path: String }
impl Team { pub fn preview(&self) -> Option<Preview>; pub fn designer(&self) -> Option<&Agent>; /* first active */ pub fn has_designer(&self) -> bool; /* any not retired */ }
pub const DEFAULT_UI_PATHS: [&str; 7];  TeamRules.ui_paths: Vec<String>
pub fn is_ui_change(contract: &TaskContract, changed_paths: &[String], ui_paths: &[String]) -> bool;
pub enum ConnectorTag { Network, ExternalEffect, Denied }
pub struct SessionConnector { pub server: String, pub origin: String, pub tools: BTreeMap<String, ConnectorTag> }
pub enum ConnectorRefusal { ConnectorNotInSession, ToolNotTagged, ToolDenied, UrlOutsidePreview { url: String } }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>) -> Result<ConnectorTag, ConnectorRefusal>;
ToolRefusal::DesignPlanNotApproved;  pub fn check_design_plan(role: Role, tier: PermissionTier, approved: bool) -> Result<(), ToolRefusal>;
AssignmentInput.preview_set: bool;  GateFailure kind preview_not_set
pub enum DesignReviewNeed { NotNeeded, Missing, Passed }  DoneEvidence.design_review;  DoneRule::DesignReviewPassed
TransitionContext.design_reviewer: Option<String>
// farik-roles
pub struct ConnectorDefinition { pub name: String, pub image: String, pub args: Vec<String>, pub module_root: String, pub tools: BTreeMap<String, ConnectorTag> }
pub fn builtin_connector(name: &str) -> Option<ConnectorDefinition>;
// farik-runtime
SessionPurpose::Explore;  SessionRegistration.connectors / ToolContext.connectors: Vec<SessionConnector>;  ToolContext.preview: Option<Arc<dyn RunningPreview>>
pub trait PreviewFactory: Send + Sync { fn start(&self, project_id: &str, task_id: &TaskId, worktree: &Path, preview: &Preview, network: bool) -> Result<Box<dyn RunningPreview>, PreviewError>; }
pub trait RunningPreview: Send + Sync { fn origin(&self) -> String; fn browser_network(&self) -> String; fn stop(&self, reason: &str) -> Result<(), PreviewError>; }
DockerPreviewFactory { image: String }; HostPreviewFactory;  OrchestratorDeps.previews: Arc<dyn PreviewFactory>
pub fn connector_server(definition: &ConnectorDefinition, preview: &dyn RunningPreview, output_dir: &Path) -> McpServerConfig;   // stdio `docker run -i --rm --network <browser_network> ...`
pub enum CheckWidth { Phone, Desktop }  pub enum CheckTheme { Light, Dark }
pub struct PageCheck { pub width: CheckWidth, pub theme: CheckTheme, pub path: String, pub violations: Vec<Violation>, pub screenshot: PathBuf }
pub struct Violation { pub rule: String, pub impact: String, pub target: String, pub help: String }
pub fn check_page(definition: &ConnectorDefinition, preview: &dyn RunningPreview, path: &str, width: CheckWidth, theme: CheckTheme, out: &Path) -> Result<PageCheck, CheckError>;
```

Wire:
- events:
  - `design_plan.proposed { plan }`;
  - `design_plan.approved { reason }` and `design_plan.returned { reason }`;
  - `design_review.recorded { pass, reasons, checks: [{ width, theme, violations }] }`;
  - `preview.started { port }` and `preview.stopped { reason }`;
  - `page.checked { width, theme, path, violations: [{ rule, impact, target, help }], screenshot }`;
  - `tool.called` and `tool.denied` gain the optional `server` and `tag`.
- Farik tools:
  - `farik_propose_design_plan { plan }`;
  - `farik_decide_design_plan { approve, reason }`;
  - `farik_check_page { path, width: phone|desktop, theme: light|dark }`;
  - `farik_record_design_review { pass, reasons }`.
- RPC:
  - `task.get` gains `design_plan: { plan, state: proposed|approved|returned, reason? } | null`, `ui_change: bool` and `design_review: { state: not_needed|waiting|waiting_on_designer|passed|failed, reasons?, checks: [{ width, theme, violations, screenshot }] } | null`;
  - `task.screenshot { task_id, file }` answers `{ png_base64 }`;
  - `settings.defaults` gains `ui_paths`;
  - `waiting.list` gains the kinds `preview_missing` and `designer_needs_docker`;
  - `team.propose` answers six;
  - `team.save` carries `preview` and `mcp_servers` through the schema.

## Tasks

### Task 1: Mockups, and the founder's approval

Files: created `docs/design/mockups/{DesignerTeam,SettingsPreview,DesignPlan,GateDesignReview}.dc.html`; modified `canvas.json`. They are in the style of the existing boards: the rail, the palette, Space Grotesk, the 1440 px frame, and a phone frame where a screen has one. What each shows:
- `DesignerTeam`:
  - the "Your team" row for the Designer, ticked, with "Needs Docker" in its unticked variant;
  - the Designer's Team card;
  - the five characters and the three tag colours, side by side, labelled for the founder to choose;
  - the agent editor's "Connectors" list, with the Playwright switch.
- `SettingsPreview`: Settings' "How to open your app" (command, port, first page), the Designer's setup card asking for it, and the Today waiting row "Tell Farik how to open your app".
- `DesignPlan`:
  - the task page's "The plan" with "Waiting for Mira to approve the plan";
  - then approved, and returned with the reason;
  - the board card and task page showing "Waiting on the Designer" (paused) and "Checking the screens" (in review).
- `GateDesignReview`: the gate's review letter from the Designer, with its four checks (phone and desktop, light and dark), each with its screenshot and its violations. The Architect's review follows below it.

**Gate: the founder approves these mockups, with the character, the tag colour and the name, before Task 2 starts.** The approval is recorded in Decisions in this task's commit. Nothing later starts without it.

- [ ] `docs(design): mock up the UI/UX Designer's team, preview, plan and review screens`

### Task 2: The role and its rules in core, and the schemas

Files: the four schemas, core as mapped, `crates/protocol/src/event.rs`. Tests in each module's `mod tests`.
- `changes_code_for_the_developer_and_the_designer_only`: true for the two, false for the other four roles and the human.
- `gives_the_designer_the_developers_tiers_and_permission_answers`: `read`, `write_workspace`, `execute`, `git_local`. `run_commands: false` removes `execute`, and `push: true` adds `git_remote`, for a Designer as for a Developer.
- `holds_neither_the_developer_nor_the_designer_to_the_document_paths`: a Designer's task with `src/**` passes `DocumentPathsOnly`, and a Marketing Specialist's still fails.
- `puts_a_designers_task_on_a_feature_or_fix_branch`: `task_branch` gives `feature/FRK-n` and `fix/FRK-n` by `change`.
- `refuses_a_write_before_the_plan_is_approved`: `check_design_plan` refuses a Designer's `write_workspace`, `execute` and `git_local` when not approved (`design_plan_not_approved`). It allows `read`, allows all three when approved, and allows a Developer anything.
- `judges_a_ui_change_by_the_diff_or_the_field`: `app/Button.tsx` under the defaults is true; `README.md` is false; `README.md` with `ui_change: true` is true; with `ui_paths: []` and no field it is false.
- `defaults_the_ui_paths_when_left_out`: the seven design globs, the team's own list replacing them, and `[]` kept empty.
- `evaluates_connector_calls`:
  - a `network` tool with `url: "http://localhost:4400/x"` passes;
  - `http://localhost:44001`, `http://localhost:4400@evil.test`, `http://127.0.0.1:4400` and `/x` are each `url_outside_preview`;
  - a nested `url` is checked too;
  - `browser_evaluate` is `tool_denied`, an unlisted tool is `tool_not_tagged`, and no connector gives `connector_not_in_session`;
  - an `external_effect` tag is denied.
- `refuses_to_assign_a_designer_without_a_preview`: `preview_not_set` for a Designer assignee with `preview_set: false`, and nothing for a Developer.
- `requires_the_design_review_when_needed`: `DesignReviewPassed` fails on `Missing` and holds on `NotNeeded` and `Passed`.
- `lets_the_designer_reject_a_ui_change`: `verifying → rejected` by the `design_reviewer` id passes with reasons, and by another agent fails as today.
- `validates_the_preview_and_the_connectors`: the bounds on `command`, `port` and `path`, and `unknown_connector: selenium`. Every existing fixture still validates.
- `reads_the_new_events`: each new body, and `tool.called` with `server` and `tag`, round-trips through `event.schema.json`.

- [ ] `feat(core): add the UI/UX Designer, its plan gate, ui changes, previews and connector checks`

### Task 3: The role's content, its reviewer, and the suggested six

Files:
- `role.yaml`: `id: ui_ux_designer`, the mandate, the persona, the Developer's model (Opus 5, `high`), the chosen avatar key;
- `system.md`, which covers explore, plan, approval, implement, and the design review;
- six skills in the Agent Skills format (`SKILL.md` with `name` and `description` front matter), named as the design's table names them;
- the other roles' `system.md`, where "only the Developer changes code" becomes "only the Developer and the UI/UX Designer";
- `REVIEWER_ROLE_FOR` gains `(UiUxDesigner, [Architect, SoftwareDeveloper])`;
- `team.propose`;
- the tag colour token.

Tests:
- `ships_the_designer_with_its_six_skills`: the role loads, and the skill names equal the design's six.
- `says_who_changes_code_in_every_prompt`: no role's `system.md` contains "only the Developer changes code" or "Only the Software Developer", and the Developer's and the Designer's contain the new sentence.
- `sends_a_designers_task_to_the_architect_then_a_developer`: `default_reviewer_role` for a Designer's task.
- `proposes_the_suggested_six`: ids, roles, order (PM, SM, Architect, Developer, Designer, Marketing), and the Designer's `mcp_servers: [{ name: playwright, source: builtin }]`. Without Docker, the Designer is left out of the ticked set.
- `contrast.test.ts`: `role-ui-ux-designer` against `role-ink` passes AA in both themes. This is the existing test's new pair.

- [ ] `feat(roles): add the UI/UX Designer's role, skills and reviewer, and suggest six`

### Task 4: The connector base: definition, preview, launch, and the hook

Files as mapped. `playwright.yaml` holds the image by digest, the args and the tag table. The arguments are `--headless`, `--isolated`, `--browser chromium`, `--allowed-origins <origin>` and `--output-dir /out`, with the session folder's `playwright/` mounted at `/out`.

The preview:
- In sandbox mode it is a container `farik-preview-<project>-<task>`: the sandbox image, the worktree at `/workspace`, the task's network rule, and `sh -c <command>`.
- It is ready when `http://localhost:<port><path>` answers inside the container, polled for up to 120 s. On time-out it fails the session with the output's last 40 lines.
- `preview.started` and `preview.stopped` are recorded around each session.

The session and the hook:
- Its registration carries `SessionConnector`s.
- The hook sends `mcp__<server>__<tool>` for any server but `farik` to `evaluate_connector_call`, and records `server` and `tag`.
- `claude_args` adds the denied tools to `--disallowedTools`.
- The computer check gains the row "Browser for the UI/UX Designer" when the team has one, pulled like the sandbox image.

Tests:
- `starts_and_stops_a_preview_around_a_designer_session` (fake factory): the events are in order and the port matches.
- `fails_a_session_whose_preview_never_answers`: the reason carries the tail.
- `offers_the_connector_only_where_the_design_says`: yes in `explore`, `implement` and the design review for an agent with it on; no in `verify` for the Architect, and none without a preview.
- `denies_a_connector_call_outside_the_rules` (hook): each `ConnectorRefusal` kind as a `tool.denied` with `server` and `tag`, and a good call as `tool.called` with `tag: network`.
- `refuses_a_designer_write_before_approval` (hook and `call_tool`): `Edit` and `farik_exec` are denied with `design_plan_not_approved`.
- `lists_the_denied_tools_as_disallowed`: `claude_args` carries `Bash` and the denied names.
- Integration, in `playwright_connector.rs`, each `#[ignore = "needs docker"]`:
  - `the_pinned_image_lists_the_pinned_tools`: the drift test;
  - `the_browser_reaches_only_the_preview`: `alpine:3.22` serves a page with `busybox httpd`. `browser_navigate` to it succeeds, and navigating to `http://example.com` fails in the server.

- [ ] `feat(runtime): run a preview and a governed Playwright connector per agent`

### Task 5: `farik_check_page`

Files: `assets/*`, the tool in `tools/design.rs`, the image block in `daemon/mcp.rs`. It opens the preview's `path` at 360 or 1280 px with `colorScheme` emulated, injects axe, runs `wcag2a`, `wcag2aa`, `wcag21aa` and `wcag22aa`, and screenshots the page. It records `page.checked` and answers the violations under the untrusted notice.

Tests:
- `checks_a_page_through_the_runner` (fake runner): arguments per width and theme, the event recorded, and the answer's shape.
- `refuses_outside_a_designer_session`: `check_page_refused` for the Architect and for a session with no preview.
- `returns_the_screenshot_as_an_image_block`: `mcp.rs` answers text plus an `image/png` block.
- `bundles_the_axe_the_web_tests_pin`: the banner equals `packages/ui`'s `axe-core`.
- Integration: `checks_a_page_on_the_pinned_image`. A page with an unlabelled button reports `button-name`, and dark and light screenshots differ.

- [ ] `feat(runtime): check a preview page for accessibility at two widths and two themes`

### Task 6: The Designer's flow: explore, plan, approval, implement

Files: `orchestrator/design.rs`, `rules.rs` (`in_progress` branches for a Designer's task), `messages.rs`, `tools/design.rs`.

By the latest `design_plan.*` event:
- none, or `returned`: the Designer's `explore` session (read tier, the connector, the five reading tools, `farik_check_page`, `farik_propose_design_plan`);
- `proposed`: the Product Manager's `verify` session, with only `farik_decide_design_plan`;
- `approved`: `implement` with the plan in its message.

Returns at the contract's `max_iterations` plus extra tries escalate with `iterations`. The recorded transcripts are `explore_plans_frk_1`, `decide_design_plan_approves_frk_1`, `decide_design_plan_returns_frk_1` and `implement_by_iris_frk_1`.

Tests:
- `explores_first_then_asks_the_product_manager`: the session order and purposes, and the explore session's tools as listed.
- `implements_only_after_approval`: after `approves`, `implement` with the plan in its message.
- `explores_again_with_the_returned_reason`: the reason is in the new message.
- `escalates_after_too_many_returns`: at 3 returns, `escalated` with `iterations`.
- `refuses_the_plan_outside_explore`: `design_plan_refused` in `implement`, or by a Developer. The plan's length and summary bounds are refused with their sentences.
- `refuses_the_decision_but_from_the_product_manager`: `design_plan_refused` for the Architect, and for the Product Manager outside a `verify` session about the task.
- `does_not_assign_a_designer_without_a_preview`: the task stays `ready`, and `waiting.list` has `preview_missing`.
- `sends_the_designers_work_to_the_architect`: the reviewer's session after the completion is the Architect's.

- [ ] `feat(runtime): run the Designer's explore, plan, approval and implement flow`

### Task 7: The design review of a Developer's UI change, and the wire

Files: `verify.rs`, `orchestrator/design.rs`, `tools/design.rs`, `daemon/team.rs`, and the RPC schema and client.

In rule 5, before the reviewer's session, when the change is a UI change and `has_designer`:
- no passing review since entering `verifying`: start the Designer's session (verify purpose, a fresh sandbox, the preview, the connector, `farik_check_page`, `farik_record_design_review`);
- the latest review failed: reject in the Designer's name with its reasons;
- the only Designer is paused: wait, with the task's `design_review.state` `waiting_on_designer`;
- no Docker: `designer_needs_docker`.

The recorded transcripts are `implement_css_frk_2`, `design_review_passes_frk_2` and `design_review_fails_frk_2`.

Tests:
- `checks_a_ui_change_before_the_architect`: the Designer's session comes first and the Architect's second.
- `sends_a_failed_design_review_back_to_the_developer`: `rejected` by the Designer's id with its reasons, and no Architect session.
- `leaves_a_non_ui_change_to_the_architect`, and `reviews_alone_without_a_designer` (all retired).
- `waits_on_a_paused_designer`: no session starts, and `task.get`'s `design_review.state` is `waiting_on_designer`.
- `refuses_an_incomplete_design_review`: three checks give `design_review_incomplete`, and the fourth lets the review record with `checks` copied from the events.
- `runs_both_again_after_a_send_back`: the Designer's pass is required afresh.
- `answers_the_task_with_its_plan_and_review`: the `task.get` fields, `task.screenshot`, and `settings.defaults.ui_paths`.

- [ ] `feat(runtime): check a Developer's UI change in the browser before the Architect reviews it`

### Task 8: The Designer in setup, on the Team page, and "How to open your app"

Files as mapped. The Designer's row carries its "Needs Docker" state, and ticking it asks for the preview on its card. The Team card uses the chosen avatar and colour. AgentEdit's "Connectors" has a Playwright switch writing `mcp_servers`. Settings' "How to open your app" has command, port and first page, saved by `team.save` with its effect line. `ui_paths` sits in the advanced team rules.

Tests:
- `offers_the_designer_among_the_six`: six rows, the Designer ticked, and unticking it saves five.
- `asks_how_to_open_the_app_when_the_designer_is_on`: the card's fields, with the daemon's refusal of port `80` shown at the field.
- `shows_the_designer_card_in_its_colour`: the card's role tag uses `--role-ui-ux-designer` and the chosen avatar.
- `switches_a_connector`: turning Playwright on for Theo sends `mcp_servers` with it, and the validate effect shows first.
- `saves_the_preview_in_settings`: sends `team.save` with `preview { command, port, path }`, after showing the effect line.
- `edits_the_ui_paths_in_advanced`: the seven defaults come from `settings.defaults`, and adding `**/*.strings` saves `ui_paths` with eight globs.

All of them run axe.

- [ ] `feat(web): add the Designer to setup and the Team page, and ask how to open the app`

### Task 9: The plan, the design review, and the waiting states

Files as mapped:
- the task page's "The plan", with the PM's decision;
- "Design review", with four checks, screenshots from `task.screenshot` and violations;
- the Gate's review letter from the Designer above the Architect's;
- the board mark "Waiting on the Designer";
- Today's rows `preview_missing` and `designer_needs_docker`.

Tests:
- `shows_the_plan_waiting_for_the_product_manager`, then approved, then returned with its reason.
- `shows_the_four_checks_with_their_screenshots`: four figures, each with its alt text "Phone, light" and so on, and each violation's help.
- `puts_the_designers_letter_first_on_the_gate`: the Designer's letter precedes the Architect's review in document order.
- `marks_a_task_waiting_on_the_designer` (board): a `verifying` card whose `design_review.state` is `waiting_on_designer` shows "Waiting on the Designer".
- `links_the_missing_preview_to_settings` (Today): the `preview_missing` row's link goes to `/settings#preview`.

- [ ] `feat(web): show the design plan, the design review and who the work waits on`

### Task 10: The Designer's journey, and Farik's own preview

Files as mapped.
- The fixture team `pm-architect-developer-designer` is Mira, Ada, Theo and Iris, with `judgment.required: never`, no-sandbox mode, and `preview: { command: "python3 -m http.server 4401 --bind 127.0.0.1 --directory site", port: 4401 }` over a two-file `site/`.
- `farik-e2e-serve` gains `--preview`, and new transcripts are named in its match.

`designer.spec.ts`, paced like `board.spec.ts`:
1. File a request for a screen change. It is triaged, refined for Iris and assigned.
2. Iris explores. Its `farik_check_page` really runs on the Playwright image.
3. The task page shows the plan waiting for Mira, then approved.
4. Iris implements, and the Architect reviews and accepts: the log holds `design_plan.proposed`, `design_plan.approved`, `review.recorded` by Ada, in order.
5. A second request is assigned to Theo, whose diff touches `site/style.css`. The task page shows "Checking the screens". Then `design_review.recorded { pass: true }` comes with four `page.checked` events, and Ada's review follows.
6. It takes screenshots of the team, the task page's plan and review, and the gate at 360 and 1280 px, with no sideways scroll at 360.

Also `admits_a_local_browser_without_a_code_in_preview_mode` (`serving.rs`): with `--preview`, `GET /` from `localhost` gets the app with no code. Without `--preview`, it gets `/connect`.

- [ ] `test(web): walk a Designer's task and a design review through the real server and browser`

### Task 11: Spec and plan

- `docs/SPEC.md`, the design's list for step 11:
  - 3: the agent and the role;
  - 4.1: the suggested six, and the preview command;
  - 5.1 and 5.2: the plan gate's text;
  - 5.3: `document_paths` for two roles;
  - 5.4: the Designer's reviewer, the design review, and the Definition of Done item;
  - 5.6: the Designer's tiers, connectors, tags and `url_outside_preview`;
  - 5.12: `ui_paths`;
  - 6.1 to 6.5: "only the Developer and the UI/UX Designer";
  - a new 6.8, the Designer;
  - 6.7;
  - 8.2: the `explore` session, and the hook's connector check;
  - 8.3: the preview and browser containers;
  - 8.5: the events above, `page.checked` among them;
  - 8.6: browsing only the preview;
  - F1 and F9.
- The header gains "Revision 0.31 (<date>) …", from phase 6 step 11, in the form of 0.29 and 0.30.
- `docs/plans/project-plan.md`: step 11's line gains "Built <date> (spec 0.31): …" with the founder's choices, as steps 06 to 10 do.

- [ ] `docs(spec): the UI/UX Designer, its plan gate, the design review, and connectors`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (T2 13 new, T3 4, T4 6 plus 2 Docker, T5 4 plus 1 Docker, T6 8, T7 8, T10 1);
#   @farik/brand: the count at this step's start plus 0 (one pair added to an existing test);
#   @farik/web: the count at this step's start plus 11 (T8 6, T9 5);
#   playwright: the count at this step's start plus 1 (10 when this plan was written) passed;
#   last line: xtask check: ok
```

The integration run needs Docker with `alpine:3.22` and the pinned Playwright image pulled, as CI already pulls the first.
