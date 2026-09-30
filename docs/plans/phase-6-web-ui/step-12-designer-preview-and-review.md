# Phase 6, step 12: The Designer's preview, Playwright connector and design review

Status: draft
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 4.1 (the preview commands), 5.4 (the design review), 5.6 (connectors), 5.12 (`ui_paths`), 6.7, 8.2 (the hook's connector check), 8.3 (the preview and browser containers), 8.5, 8.6, F9
Depends on: step 11 of this phase (the Designer, its plan gate, and the mockups the founder approved in its Task 1); ADR 0026 and `docs/design/designer-chats-templates.md`
Readiness confirmed by: round one not ready (2026-09-30); founder decisions D2, D3 and S1 made; round two pending

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Designer gets a browser. Farik starts the project's preview in the sandbox from the commands the user sets in Settings under "How to open your app". A built-in Playwright MCP connector, given per agent in `team.yaml`, can reach only that preview, and the governor checks every one of its calls. `farik_check_page` runs an accessibility check at 360 and 1280 px, in both themes. When a Developer's change touches the interface, the Designer checks it in the browser first, and only a change it passes reaches the Architect. The user reads the design review on the task's page and on the gate.

Out of scope: custom connectors, credentials and per-call approval (phase 8 step 01), and kits (phase 9).

## Decisions

These are the step 11 plan's decisions that concern the preview, the connector and the design review. They are carried over unchanged; round two settles the open ones.

- **The URL rule.** The governor checks every string field named `url`, at any depth of a connector call's input. Each must be the preview's origin `http://localhost:<port>`, either exactly or followed by `/`, `?` or `#`. Anything else is denied with `url_outside_preview`.
- **The tags**, by the Playwright MCP server's tool names:
  - `network`: `browser_navigate`, `browser_navigate_back`, `browser_snapshot`, `browser_click`, `browser_hover`, `browser_drag`, `browser_type`, `browser_fill_form`, `browser_select_option`, `browser_press_key`, `browser_handle_dialog`, `browser_resize`, `browser_wait_for`, `browser_take_screenshot`, `browser_console_messages`, `browser_tabs`, `browser_close`;
  - `denied`: `browser_evaluate`, `browser_run_code`, `browser_file_upload`, `browser_install`, `browser_pdf_save`, `browser_network_requests`, and any tool the image lists that is not in this table;
  - `external_effect` is denied in this step;
  - denied tools also go into `--disallowedTools`.
- **The pin.** `mcr.microsoft.com/playwright/mcp` is pinned by digest at its newest release on the day Task 2 starts. The tag, the digest and the module root are recorded in `playwright.yaml`. The drift test reconciles this plan's table with the image.
- **Which sessions get the connector.** Those of an agent that has it in `mcp_servers`, with purpose `explore`, `implement` or the design review, on a team that has a preview.
- **`page.checked`** records each of `farik_check_page`'s results. `farik_record_design_review` copies the session's four latest checks into its record, and is refused with `design_review_incomplete` until all four are in.
- **Screenshots** are kept in `.farik/local/screenshots/<task>/`. The MCP answer carries each as an image block, and the page reads them through `task.screenshot`.
- **axe-core 4.13.0** is vendored as `crates/runtime/assets/axe.min.js`, and a test checks that it matches `packages/ui`'s pin.
- **The check script** is `crates/runtime/assets/check-page.mjs`, run in the pinned image with `--entrypoint node`.
- **The Designer's rejection.** A new field, `TransitionContext.design_reviewer`, and a new Definition of Done rule, `DoneRule::DesignReviewPassed`. No transition row is added.
- **No preview command.** The Designer's tasks are refused assignment with `preview_not_set`, and Today shows a waiting row.

## File map

```
docs/schemas/{team,task-contract,event}.schema.json                       modifies (T1): ui_paths, ui_change, preview, mcp_servers, events
crates/core/src/{team.rs,team/defaults.rs}                                modifies (T1)
crates/core/src/governor/{permissions.rs,team_rules.rs,gates.rs,done.rs,transition.rs}  modifies (T1), + tests
crates/protocol/src/event.rs                                              modifies (T1)
crates/roles/connectors/playwright.yaml, crates/roles/src/connectors.rs   creates (T2)
crates/runtime/src/{preview.rs,preview/docker.rs}                         creates (T2)
crates/runtime/src/{daemon.rs,daemon/hooks.rs,daemon/team.rs,claude.rs,session.rs,computer.rs}  modifies (T2)
crates/runtime/tests/playwright_connector.rs                              creates (T2, T3)
crates/runtime/assets/{axe.min.js,axe-LICENSE.txt,check-page.mjs}          creates (T3)
crates/runtime/src/{tools.rs,tools/design.rs,daemon/mcp.rs}                modifies (T3, T4)
crates/runtime/src/orchestrator/{verify.rs,design.rs}                      modifies (T4)
crates/runtime/src/recorded/{fixtures.rs,transcripts/*.jsonl}              modifies / creates (T4, T6)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, packages/protocol-client/src/*  modifies (T4)
apps/web/src/pages/{AgentEdit,Settings,TeamRules,TaskDetail,Gate,Today,Board}.tsx, setup/SetupTeam.tsx, app/lanes.ts, strings/en.ts (+ tests)  modifies (T5)
crates/cli/src/bin/farik-e2e-serve.rs, crates/cli/tests/serving.rs         modifies (T6)
apps/web/e2e/{designer.spec.ts,fixtures/serve.ts}                         creates / modifies (T6)
docs/SPEC.md, docs/plans/project-plan.md                                  modifies (T7)
```

## Interfaces

Consumes: from step 11, `Role::UiUxDesigner`, `SessionPurpose::Explore` and the design-plan flow. From this branch, `TeamRules`, `check_assignment`, `evaluate_done`, `TransitionContext`, `SessionRegistration`, `ToolContext`, `McpServerConfig`, `SandboxFactory` and `check_computer`.

Produces:

```rust
// farik-core
pub struct Preview { pub command: String, pub port: u16, pub path: String }
impl Team { pub fn preview(&self) -> Option<Preview>; pub fn designer(&self) -> Option<&Agent>; pub fn has_designer(&self) -> bool; }
pub const DEFAULT_UI_PATHS: [&str; 7];  TeamRules.ui_paths: Vec<String>
pub fn is_ui_change(contract: &TaskContract, changed_paths: &[String], ui_paths: &[String]) -> bool;
pub enum ConnectorTag { Network, ExternalEffect, Denied }
pub struct SessionConnector { pub server: String, pub origin: String, pub tools: BTreeMap<String, ConnectorTag> }
pub enum ConnectorRefusal { ConnectorNotInSession, ToolNotTagged, ToolDenied, UrlOutsidePreview { url: String } }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>) -> Result<ConnectorTag, ConnectorRefusal>;
AssignmentInput.preview_set: bool
pub enum DesignReviewNeed { NotNeeded, Missing, Passed }  DoneEvidence.design_review;  DoneRule::DesignReviewPassed
TransitionContext.design_reviewer: Option<String>
// farik-roles
pub struct ConnectorDefinition { pub name: String, pub image: String, pub args: Vec<String>, pub module_root: String, pub tools: BTreeMap<String, ConnectorTag> }
pub fn builtin_connector(name: &str) -> Option<ConnectorDefinition>;
// farik-runtime
SessionRegistration.connectors / ToolContext.connectors: Vec<SessionConnector>;  ToolContext.preview: Option<Arc<dyn RunningPreview>>
pub trait PreviewFactory: Send + Sync { fn start(&self, project_id: &str, task_id: &TaskId, worktree: &Path, preview: &Preview) -> Result<Box<dyn RunningPreview>, PreviewError>; }
pub trait RunningPreview: Send + Sync { fn origin(&self) -> String; fn browser_network(&self) -> String; fn stop(&self, reason: &str) -> Result<(), PreviewError>; }
pub fn connector_server(definition: &ConnectorDefinition, preview: &dyn RunningPreview, output_dir: &Path) -> McpServerConfig;
pub enum CheckWidth { Phone, Desktop }  pub enum CheckTheme { Light, Dark }
pub struct PageCheck { pub width: CheckWidth, pub theme: CheckTheme, pub path: String, pub violations: Vec<Violation>, pub screenshot: PathBuf }
pub struct Violation { pub rule: String, pub impact: String, pub target: String, pub help: String }
pub fn check_page(definition: &ConnectorDefinition, preview: &dyn RunningPreview, path: &str, width: CheckWidth, theme: CheckTheme, out: &Path) -> Result<PageCheck, CheckError>;
```

Wire:
- events:
  - `design_review.recorded { pass, reasons, checks }`;
  - `preview.started { port }` and `preview.stopped { reason }`;
  - `page.checked { width, theme, path, violations, screenshot }`;
  - `server` and `tag` on `tool.called` and `tool.denied`.
- tools: `farik_check_page { path, width, theme }` and `farik_record_design_review { pass, reasons }`.
- RPC:
  - `task.get` gains `ui_change` and `design_review`;
  - `task.screenshot { task_id, file }`;
  - `settings.defaults.ui_paths`;
  - `waiting.list`'s `preview_missing`;
  - `team.propose` gives the Designer `mcp_servers: [{ name: playwright, source: builtin }]`.

## Tasks

### Task 1: Previews, UI changes, connector checks and the design review in core

Tests:
- `judges_a_ui_change_by_the_diff_or_the_field`: `app/Button.tsx` under the defaults is true; `README.md` is false; `README.md` with `ui_change: true` is true; with `ui_paths: []` and no field, false.
- `defaults_the_ui_paths_when_left_out`: the seven design globs; the team's own list replaces them; `[]` stays empty.
- `evaluates_connector_calls`:
  - a `network` tool with `url: "http://localhost:4400/x"` passes;
  - `http://localhost:44001`, `http://localhost:4400@evil.test`, `http://127.0.0.1:4400` and `/x` are each `url_outside_preview`;
  - a nested `url` is checked too;
  - `browser_evaluate` is `tool_denied`, an unlisted tool is `tool_not_tagged`, and no connector gives `connector_not_in_session`;
  - an `external_effect` tag is denied.
- `refuses_to_assign_a_designer_without_a_preview`: `preview_not_set` for a Designer assignee with no preview; nothing for a Developer.
- `requires_the_design_review_when_needed`: `DesignReviewPassed` fails on `Missing`, and holds on `NotNeeded` and `Passed`.
- `lets_the_designer_reject_a_ui_change`: `verifying → rejected` by the `design_reviewer` id, with reasons, passes; by another agent it fails as today.
- `validates_the_preview_and_the_connectors`: the bounds, and `unknown_connector: selenium`.
- `reads_the_new_events`: each new event body round-trips.

- [ ] `feat(core): add previews, ui changes, connector checks and the design review rule`

### Task 2: The connector base: definition, preview, launch, and the hook

Tests:
- `starts_and_stops_a_preview_around_a_designer_session` (fake factory): `preview.started` and `preview.stopped` come in order, with the port.
- `fails_a_session_whose_preview_never_answers`: the reason carries the output's tail.
- `offers_the_connector_only_where_the_design_says`: in `explore`, `implement` and the design review for an agent that has it on; not in the Architect's `verify`, and not without a preview.
- `denies_a_connector_call_outside_the_rules` (hook): each `ConnectorRefusal` kind is recorded as `tool.denied` with `server` and `tag`; a good call is recorded as `tool.called` with `tag: network`.
- `lists_the_denied_tools_as_disallowed`: `claude_args` carries `Bash` and the denied names.
- `proposes_the_designer_with_its_connector`.
- Integration, in `playwright_connector.rs` (`#[ignore = "needs docker"]`): `the_pinned_image_lists_the_pinned_tools` and `the_browser_reaches_only_the_preview`.

- [ ] `feat(runtime): run a preview and a governed Playwright connector per agent`

### Task 3: `farik_check_page`

Tests:
- `checks_a_page_through_the_runner`: the runner is given each width and theme, `page.checked` is recorded, and the answer has its shape.
- `refuses_outside_a_designer_session`: `check_page_refused`.
- `returns_the_screenshot_as_an_image_block`.
- `bundles_the_axe_the_web_tests_pin`.
- Integration: `checks_a_page_on_the_pinned_image`, where an unlabelled button reports `button-name`.

- [ ] `feat(runtime): check a preview page for accessibility at two widths and two themes`

### Task 4: The design review of a Developer's UI change, and the wire

Tests:
- `checks_a_ui_change_before_the_architect`: the Designer's session comes first, the Architect's second.
- `sends_a_failed_design_review_back_to_the_developer`: `rejected` by the Designer's id with its reasons, and no Architect session.
- `leaves_a_non_ui_change_to_the_architect`: no Designer's session.
- `reviews_alone_without_a_designer`: with every Designer retired, the Architect reviews alone.
- `waits_on_a_paused_designer`: no session starts, and `design_review.state` is `waiting_on_designer`.
- `refuses_an_incomplete_design_review`: three checks give `design_review_incomplete`; after the fourth, the review records.
- `runs_both_again_after_a_send_back`: after a send-back, the Designer's pass is required again.
- `answers_the_task_with_its_review`: the new `task.get` fields, `task.screenshot`, and `settings.defaults.ui_paths`.
- `does_not_assign_a_designer_without_a_preview`: the task stays `ready`, and `waiting.list` has `preview_missing`.

- [ ] `feat(runtime): check a Developer's UI change in the browser before the Architect reviews it`

### Task 5: The pages

Tests, each also running axe:
- `asks_how_to_open_the_app_when_the_designer_is_on`: the Designer's setup card shows the preview fields.
- `switches_a_connector`: turning Playwright on sends `mcp_servers` with it.
- `saves_the_preview_in_settings`.
- `edits_the_ui_paths_in_advanced`.
- `shows_the_four_checks_with_their_screenshots`: four figures, each with its alt text and its violations' help.
- `puts_the_designers_letter_first_on_the_gate`: the Designer's letter precedes the Architect's review.
- `marks_a_task_waiting_on_the_designer`: the board shows "Waiting on the Designer".
- `links_the_missing_preview_to_settings`: the Today row links to `/settings#preview`.

- [ ] `feat(web): show the design review, and ask how to open the app`

### Task 6: The Designer's journey

`designer.spec.ts`: Iris explores, plans, Mira approves, Iris implements, and Ada reviews. Then Theo's change to `site/style.css` is checked by Iris first, and Ada reviews it after. `farik-e2e-serve --preview` for Farik's own preview is tested by `admits_a_local_browser_without_a_code_in_preview_mode`.

- [ ] `test(web): walk a Designer's task and a design review through the real server and browser`

### Task 7: Spec and plan

- `docs/SPEC.md`: 4.1 (the preview commands), 5.4 (the design review, and the Definition of Done item), 5.6, 5.12, 6.7, 8.2, 8.3, 8.5, 8.6 and F9, under revision 0.32.
- The project plan: step 12's line.

- [ ] `docs(spec): the Designer's preview, connector and design review`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed; last line: xtask check: ok
```
