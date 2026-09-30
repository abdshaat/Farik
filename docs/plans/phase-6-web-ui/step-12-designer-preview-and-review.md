# Phase 6, step 12: The Designer's preview, Playwright connector and design review

Status: built 2026-09-30 (Tasks 1 to 7), awaiting its landing review
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 4.1 (the preview commands), 5.4 (the design review), 5.6 (connectors), 5.7 (the `preview` escalation), 5.12 (`ui_paths`), 6.7, 8.2 (the hook's connector check), 8.3 (the preview and browser containers), 8.5, 8.6, F9
Depends on: step 11 of this phase (the Designer, its plan gate, and the mockups the founder approved in its Task 1, which cover this step's screens); ADR 0026 and `docs/design/designer-chats-templates.md`
Readiness confirmed by: fresh-session reviewer, 2026-09-30; round one not ready; round two ready with findings, folded in

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Designer gets a browser.

Farik prepares and starts the project's preview in Docker's sandbox from the two commands the user sets once in Settings, under "How to open your app". A built-in Playwright MCP connector, given per agent in `team.yaml`, reaches only that preview. The network is off, and a blackhole proxy catches any request that tries to leave. The governor checks every call the connector makes.

`farik_check_page` runs axe at 360 and 1280 px, in both themes. When a Developer's change touches the interface, the Designer checks it in the browser first, and only a change it passes reaches the Architect. The user reads the design review on the task's page and on the gate. Step 11's explore session gains the browser.

Out of scope: custom connectors, credentials and per-call approval (phase 8 step 01), and kits (phase 9).

## Decisions

The ADR and the design hold as written. The founder's decisions of 2026-09-30 (D2, D3) are binding. The rest are this plan's, marked with the readiness review's item.

- **The preview is two commands** (the founder, D2). `team.yaml` gains a top-level `preview { prepare?, start, port, path }`:
  - `prepare` and `start` are 1 to 500 characters each;
  - `port` is 1024 to 65535;
  - `path` starts with `/`, default `/`.
  Setup and Settings ask for both commands.
  - **`prepare`** installs and builds. It runs in the sandbox image with the worktree mounted and the network on (`bridge`), for at most 15 minutes. It is reused while its cache key holds. The key is the sha256 of the task branch's `HEAD^{tree}` id and the `prepare` text, kept in `.farik/local/previews/<task>.json` and recorded in `preview.prepared { tree, seconds }`.
  - Uncommitted edits are not in the key: `start` reads the worktree as it is. So a dev server shows them, and a prebuilt binary shows them only after a commit.
  - Left out, `prepare` is skipped.
  - **`start`** runs in a container `farik-preview-<project>-<task>` of the sandbox image, the worktree mounted, with `--network none`. Farik waits up to 120 s for `http://localhost:<port><path>` to answer inside it. The host cannot reach that namespace, so the probe is a `docker exec` in the preview container, once a second: `sh -c 'curl -fsS -o /dev/null <url> || wget -q -O /dev/null <url>'`. That is curl in the sandbox image, and busybox `wget` in the journey's `alpine:3.22` (R10).
  - Every project command runs in the sandbox (ADR 0004).
- **A preview that fails.** A `prepare` that exits non-zero or times out, a `start` that never answers, or Docker being gone, each escalates the task with a new reason, `preview`. The detail is the output's last 40 lines, and `preview.stopped { reason }` is recorded. This applies to a Designer's task in `explore` or `implement`, and to a Developer's task under design review. The human fixes the commands in Settings and resolves the escalation. Rejected: retrying, because a broken command fails the same way each time.
- **Farik's own preview** (D2):
  - `prepare`: `pnpm install --frozen-lockfile && pnpm -r --if-present generate && pnpm --filter @farik/web build && cargo build -p farik --features e2e --bin farik-e2e-serve`
  - `start`: `target/debug/farik-e2e-serve --preview --port 4400`
  - port `4400`, path `/`.
- **`--preview`** (D1):
  - `farik-runtime` gains `[features] e2e = []`, enabled by `farik`'s `e2e` feature. The admit branch is under `#[cfg(feature = "e2e")]`, so it is absent from the release binary, not merely unreached.
  - With it on, `own_host` also accepts `Host: localhost:<port>` exactly, and `from_own_page` also accepts `Origin: http://localhost:<port>` exactly. Any other host, `localhost` with another port among them, is still refused, so DNS rebinding and cross-site requests stay out.
  - `GET /` with that Host issues a browser session and sets its cookie as `POST /connect` does, then serves the app. The WebSocket and RPC paths are unchanged.
  - `--preview` implies `--no-keychain`, a temporary `XDG_CONFIG_HOME`, and a temporary copy of step 08's recorded team as the project, so a code-free browser never shares a daemon with a real credential store.
  - The knob is `CliIo.admit_local_preview` (under `e2e`), passed to `WebState.admit_local_preview` (as built: no `WebConfig` exists, and `WebState` is what the browser routes read).
- **The Designer needs Docker's sandbox** (the founder, D3). The Designer is unavailable in no-sandbox mode and without Docker, and there is no host path. When unavailable:
  - `team.propose` lists it unticked with "Needs Docker's sandbox";
  - its tasks are not assigned;
  - a UI change waits in `verifying` under the waiting row "The UI/UX Designer needs Docker's sandbox to open your app. Turn the sandbox on, or retire the Designer".
- **How "no sandbox, no assignment" is enforced** (D4). `AssignmentInput` gains `designer_browser: DesignerBrowser { Ready, NoPreview, NoSandbox }`. `check_assignment` refuses a Designer assignee with `preview_not_set` or `designer_needs_sandbox`. The runtime fills it from `team.preview()` and `PreviewFactory::available()`, which is false in no-sandbox mode and otherwise caches `docker info` for 60 s. Docker going away mid-task shows when the next preview fails to start: the `preview` escalation above.
- **No port collisions** (D5). Every preview has its own network namespace with the network off, so two previews on one port never collide, and a host server on that port is unreachable from the browser. They cannot happen, so no rule is needed.
- **Confinement** (the founder, D3):
  - The browser container is `farik-browser-<project>-<task>`, labelled as the sandbox is, run with `--user <uid>:<gid>` and `--network container:farik-preview-<project>-<task>`. It sees only the preview's loopback.
  - Chromium runs with `--proxy-server http://127.0.0.1:9`, a dead port, and Chromium's default loopback bypass. No click, redirect or page script can leave, whatever the namespace. (Task 2 found that Playwright turns Chromium's own loopback bypass off once a proxy is set, so the connector also passes `--proxy-bypass localhost`.)
  - `--allowed-origins http://localhost:<port>` is also set.
  - The governor checks every `url` argument (below).
  - These are four barriers. The integration test proves the first three, each on its own. The governor's `url` check is proved by `evaluates_connector_calls`, not in Docker.
  - `RunningPreview::stop` removes both containers by name, and the task cleanup's `remove` does too, so a killed session leaves none behind (F5).
- **The URL rule** (F1). The governor checks every field named `url`, at any depth of a connector call's input.
  - A string must be `http://localhost:<port>`, exactly or followed by `/`, `?` or `#`.
  - Anything else, including a non-string `url`, is denied with `url_outside_preview`.
- **The tags**, by the Playwright MCP server's tool names:
  - `network`: `browser_navigate`, `browser_navigate_back`, `browser_snapshot`, `browser_click`, `browser_hover`, `browser_drag`, `browser_type`, `browser_fill_form`, `browser_select_option`, `browser_press_key`, `browser_handle_dialog`, `browser_resize`, `browser_wait_for`, `browser_take_screenshot`, `browser_console_messages`, `browser_tabs`, `browser_close`;
  - `denied`: `browser_evaluate`, `browser_run_code`, `browser_file_upload`, `browser_install`, `browser_pdf_save`, `browser_network_requests`, and any tool the image lists that the table does not;
  - `external_effect` is denied until phase 8;
  - denied tools also go into `--disallowedTools`.
- **The pin.** `mcr.microsoft.com/playwright/mcp` is pinned by digest at its newest release on the day Task 2 starts. The tag, the digest and the Node module root are recorded in `playwright.yaml`. The drift test reconciles the table with the image: a tool the image adds goes in as `denied` in the same commit. CI pulls it by digest (F7). Task 2 pinned `@playwright/mcp` 0.0.82 (tag `v0.0.82`, the newest on 2026-09-30), digest `sha256:77dccc5ce9e94cb8ae7ebea87ddbb6cd54b05760c4d63c54e16accf2726b8734`, module root `/app/node_modules`. It lists five tools the table above does not (`browser_drop`, `browser_emulate_media`, `browser_find`, `browser_network_request`, `browser_run_code_unsafe`), which went in as `denied`.
- **Which sessions get the connector.** Those of an agent with it in `mcp_servers` (any agent may have it; `validate_team` checks every agent's list, F8), with purpose `explore`, `implement` or the design review, on a team whose Designer is `Ready`. Farik starts the preview for each such session.
- **The explore session's tiers** (carried from step 11's landing review, m8 and m11). Step 11 registers an explore session with `[read]` alone, whatever the Designer's grants, so the hook holds each of its calls to reading. The connector's tools are tagged `network`, so this step registers `[read, network]` for an explore session that has the connector. Browsing before the plan is approved is intended, for explore only; `check_design_plan` keeps holding only `write_workspace`, `execute`, `git_local` and `git_remote` before approval.
- **The design review session** (D9) is `read_only`. It has the read tier's built-ins, the five reading tools, the connector, `farik_check_page` and `farik_record_design_review`, and no `farik_exec`. Its container exists for the preview alone.
- **Which diff decides a UI change** (D8). The task branch's committed changes against its merge base with the integration branch, by name only (`git diff --name-only <base> <branch>`), matched against `ui_paths`. The contract's `ui_change` is the other trigger. The rule covers only tasks whose assignee is a Software Developer. The worktree is never read, because Farik's own criterion runs write into it.
- **A session that ends without its one answer** (D6). A design review session that ends without `farik_record_design_review` is started again, as `verify.rs` restarts a reviewer who wrote no note. So is one that ends with fewer than four checks, since the record is refused until all four exist. Each restart counts toward the contract's sessions allowance, which escalates with `sessions` as today.
- **A design review with no preview set** (D7). The task waits in `verifying`, `design_review.state` is `preview_missing`, and Today shows the `preview_missing` row.
- **`page.checked`** records each of `farik_check_page`'s results. `farik_record_design_review` copies the session's four latest checks, one per width and theme, into its `checks`. It is refused with `design_review_incomplete` until all four exist. Farik's own measurement gates the Architect (5.1).
- **Screenshots** are kept at `.farik/local/screenshots/<task>/<session>-<width>-<theme>.png`, and the MCP answer carries each as an image block. The query `task.screenshot { task_id, file }` answers `{ png_base64 }`. It refuses any `file` that is not named by one of that task's `page.checked` events, with `not_found` (F4).
- **axe-core 4.13.0**, `packages/ui`'s pin, is vendored as `crates/runtime/assets/axe.min.js` with its licence, with a test that the versions match. It runs the tags `wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa` and `wcag22aa` (F6).
- **The check script** is `crates/runtime/assets/check-page.mjs`, Farik's own code. It runs in the pinned image with `--entrypoint node` and `--user <uid>:<gid>`, in the same namespace and with the same proxy as the browser (R4), and takes Playwright from the recorded module root. The agent never gets it.
- **The Designer's rejection** (F9). The runtime sets `TransitionContext.design_reviewer` only when the latest `design_review.recorded` since the task entered `verifying` failed, and sets it to that Designer's id. Core only compares that id with the requester's. `DoneRule::DesignReviewPassed` joins the Definition of Done.

## File map

```
docs/schemas/{team,task-contract,event}.schema.json                       modifies (T1): ui_paths, ui_change, preview, mcp_servers, events, reason `preview`
crates/core/src/{team.rs,team/defaults.rs}                                modifies (T1)
crates/core/src/governor/{permissions.rs,team_rules.rs,gates.rs,done.rs,transition.rs}  modifies (T1), + tests
crates/protocol/src/event.rs                                              modifies (T1)
crates/roles/connectors/playwright.yaml, crates/roles/src/connectors.rs   creates (T2)
crates/runtime/Cargo.toml, crates/cli/Cargo.toml                          modifies (T2): the `e2e` feature
crates/runtime/src/{preview.rs,preview/docker.rs}                         creates (T2)
crates/runtime/src/{daemon.rs,daemon/hooks.rs,daemon/team.rs,claude.rs,session.rs,computer.rs,orchestrator.rs}  modifies (T2)
crates/runtime/src/daemon/web.rs                                          modifies (T4): `task.get`, `task.screenshot`, `settings.defaults`; (T6): the admit branch
apps/web/src/pages/setup/SetupComputer.tsx (+ test)                       modifies (T2): the Designer's browser row
xtask/src/check.rs                                                        modifies (T6): clippy with `e2e`, and the `--all-features` guard
crates/runtime/tests/playwright_connector.rs                              creates (T2, T3): `#[ignore = "needs docker"]`
.github/workflows/check.yml                                               modifies (T2): pulls the pinned image
crates/runtime/assets/{axe.min.js,axe-LICENSE.txt,check-page.mjs}          creates (T3)
crates/runtime/src/{tools.rs,tools/design.rs,daemon/mcp.rs}                modifies (T3, T4)
crates/runtime/src/orchestrator/{verify.rs,design.rs,rules.rs}             modifies (T4)
crates/runtime/src/recorded/{fixtures.rs,transcripts/*.jsonl}              modifies / creates (T4, T6)
docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, packages/protocol-client/src/*  modifies (T4)
apps/web/src/pages/{AgentEdit,Settings,TeamRules,TaskDetail,Gate,Today,Board}.tsx, setup/SetupTeam.tsx, app/lanes.ts, strings/en.ts (+ tests)  modifies (T5)
crates/cli/src/{lib.rs,bin/farik-e2e-serve.rs}, crates/cli/tests/serving.rs   modifies (T6)
apps/web/e2e/{designer.spec.ts,fixtures/serve.ts}                         creates / modifies (T6)
docs/SPEC.md, docs/plans/project-plan.md                                  modifies (T7)
```

## Interfaces

Consumes, from step 11: `Role::UiUxDesigner`, `SessionPurpose::Explore`, the explore session's tool list, `task.get.design_plan`.

Consumes, from this branch: `TeamRules`, `check_assignment`, `evaluate_done`, `TransitionContext`, `SessionRegistration`, `ToolContext`, `McpServerConfig`, `SandboxFactory`, `check_computer`, `own_host`, `from_own_page`.

Produces:

```rust
// farik-core
pub struct Preview { pub prepare: Option<String>, pub start: String, pub port: u16, pub path: String }
impl Team { pub fn preview(&self) -> Option<Preview>; pub fn designer(&self) -> Option<&Agent>; /* first active */ pub fn has_designer(&self) -> bool; /* any not retired */ }
pub const DEFAULT_UI_PATHS: [&str; 7];  TeamRules.ui_paths: Vec<String>
pub fn is_ui_change(contract: &TaskContract, assignee_role: Role, changed_paths: &[String], ui_paths: &[String]) -> bool;
pub enum ConnectorTag { Network, ExternalEffect, Denied }
pub struct SessionConnector { pub server: String, pub origin: String, pub tools: BTreeMap<String, ConnectorTag> }
pub enum ConnectorRefusal { ConnectorNotInSession, ToolNotTagged, ToolDenied, UrlOutsidePreview { url: String } }
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>) -> Result<ConnectorTag, ConnectorRefusal>;
pub enum DesignerBrowser { Ready, NoPreview, NoSandbox }  AssignmentInput.designer_browser   // refusals preview_not_set, designer_needs_sandbox
pub enum DesignReviewNeed { NotNeeded, Missing, Passed }  DoneEvidence.design_review;  DoneRule::DesignReviewPassed
TransitionContext.design_reviewer: Option<String>
// farik-roles
pub struct ConnectorDefinition { pub name: String, pub image: String, pub args: Vec<String>, pub module_root: String, pub tools: BTreeMap<String, ConnectorTag> }
pub fn builtin_connector(name: &str) -> Option<ConnectorDefinition>;
// farik-runtime
SessionRegistration.connectors / ToolContext.connectors: Vec<SessionConnector>;  ToolContext.preview: Option<Arc<dyn RunningPreview>>
pub trait PreviewFactory: Send + Sync {
    fn available(&self) -> bool;
    fn start(&self, project_id: &str, task_id: &TaskId, worktree: &Path, preview: &Preview, tree: &str) -> Result<Box<dyn RunningPreview>, PreviewError>;
}
pub trait RunningPreview: Send + Sync { fn origin(&self) -> String; fn container(&self) -> String; fn stop(&self, reason: &str) -> Result<(), PreviewError>; }
pub enum PreviewError { Prepare { tail: String }, NeverAnswered { tail: String }, DockerUnavailable { detail: String } }
DockerPreviewFactory { image: String };  OrchestratorDeps.previews: Arc<dyn PreviewFactory>
pub fn connector_server(definition: &ConnectorDefinition, preview: &dyn RunningPreview, output_dir: &Path) -> McpServerConfig;
pub enum CheckWidth { Phone, Desktop }  pub enum CheckTheme { Light, Dark }
pub struct PageCheck { pub width: CheckWidth, pub theme: CheckTheme, pub path: String, pub violations: Vec<Violation>, pub screenshot: PathBuf }
pub struct Violation { pub rule: String, pub impact: String, pub target: String, pub help: String }
pub fn check_page(definition: &ConnectorDefinition, preview: &dyn RunningPreview, path: &str, width: CheckWidth, theme: CheckTheme, out: &Path) -> Result<PageCheck, CheckError>;
#[cfg(feature = "e2e")] WebState.admit_local_preview: bool   // as built; the plan said WebConfig
```

Wire:
- events:
  - `design_review.recorded { pass, reasons, checks: [{ width, theme, violations }] }`;
  - `preview.prepared { tree, seconds }`, `preview.started { port }`, `preview.stopped { reason }`;
  - `page.checked { width, theme, path, violations: [{ rule, impact, target, help }], screenshot }`;
  - `server` and `tag` on `tool.called` and `tool.denied`;
  - `task.escalated`'s reason gains `preview`.
- tools: `farik_check_page { path, width: phone|desktop, theme: light|dark }` and `farik_record_design_review { pass, reasons }`.
- RPC:
  - `task.get` gains `ui_change` and `design_review: { state: not_needed|waiting|waiting_on_designer|preview_missing|designer_needs_sandbox|passed|failed, reasons?, checks } | null`;
  - `task.screenshot { task_id, file }` answers `{ png_base64 }`;
  - `settings.defaults.ui_paths`;
  - `waiting.list` gains `preview_missing` and `designer_needs_sandbox`;
  - `team.propose` gives the Designer `mcp_servers: [{ name: playwright, source: builtin }]`, unticked when the Designer is not `Ready` for want of a sandbox.

As built (recorded 2026-09-30 by Task 7 from the reports of Tasks 1 to 6; spec 0.33 describes this, not the first plan's guesses):
- Core: `designPlanDecidedBody` is renamed `reasonBody` (`ReasonBody`), shared with `preview.stopped`, the wire unchanged. `EscalationReason::Preview` and `TransitionContext.preview_failed` came in with Task 2 (eleven reasons). New `gates::check_design_rejection` (Task 4, outside the file map): the rejection gate required a failed criterion, which a design review never names, so without it the governor refused every Designer rejection (F9).
- Rule 5's order (Task 4, a deviation): a design review recorded since the task entered `verifying` is acted on first, a pass going to the Architect and a fail rejected, before the waits (preview, then sandbox, then paused Designer), since a recorded answer needs neither the preview nor the Designer. The plan listed the waits first.
- Tiers: every session given the connector (explore, the Designer's implement, the design review) is registered with `network` added, not only explore, or every browser call would be denied `tier_not_granted` (Task 3's finding, Task 4). `farik_check_page` is at tier `read`, offered only to a Designer's session with a task and the connector, and guarded by its own check.
- The connector: `--proxy-bypass localhost` beside the dead proxy, because Playwright turns Chromium's loopback bypass off once a proxy is set; five tools the 0.0.82 image lists and the table did not are `denied`. The check script launches `channel: "chromium"` (no headless shell in the image), takes viewport screenshots only, has a 90 s watchdog and no host-side deadline, and runs axe in the page's own world (a `ponytail:` note; SPEC 8.6 names the residual).
- New interfaces: `RunningPreview::{labels, user, run_check}`; `CheckError { detail }`; `check_page`'s `out` is the screenshot file; `Violation` is the protocol's; `Git::tree`; `SessionSpec.disallowed_tools`; the setup method `browser.pull`; `team.propose`'s required `unavailable: [{ agent_id, reason }]`; `Transitions::{set_previews, designer_browser, design_review}`; `farik_record_design_review` and its refusals `design_review_refused`, `blank_reason`, `design_review_incomplete`; `own_host` and `from_own_page` take `&WebState`; `farik-e2e-serve --sandbox-image`; workspace dependency `base64 =0.23.1`, already in the lock.
- `preview.prepared.seconds` counts prepare and start together, since the factory reports no split. `task.get`'s `design_review` is `null` for a change that is not to the interface, so `not_needed` shows only for a UI change on a team with no Designer.
- The journey: `./busybox httpd` (above); FRK-2 is high risk so that it reaches the human gate for the screenshots; step 11's `design.spec.ts` now runs in Docker with a preview, since wiring D4's refusal refused its Iris; `setup-team.spec.ts` keeps five agents in no-sandbox setup, Iris unticked. Flakes root-caused: a container already being removed counts as removed (`sandbox::docker::removed`), and `serving.rs` reads the port again on each try.
- Outside the file map: `transitions.rs`, `sandbox/docker.rs`, `cli/src/start.rs`, `daemon/app.rs`, `daemon/team.rs`, `core/governor/gates.rs`'s `check_design_rejection`, `design.spec.ts`, `setup-team.spec.ts`, `setup/PreviewFields.tsx`, `DesignReview.tsx`.
- Found by Task 7, not changed: the daemon's `preview_missing` waiting line names the agent's id ("iris needs…"), not its name; the Designer's letter is signed by the first active Designer, not the review event's agent; the approved mockup's history of earlier design reviews, "Who looked at it, in order" and the "Screens checked" date are not built (no wire gives earlier reviews); the board asks `task.get` per card in review; the agent page's words "so Farik gives Iris no work" when Playwright is off are not enforced, since the assignment checks the preview and the sandbox, not the agent's `mcp_servers`; D2's own-preview commands for Farik are in no committed file, and `farik-e2e-serve --preview` is exercised only by `serving.rs`.
- Fix wave (the landing review's runtime findings): axe runs in an isolated world of its own over CDP (I1); the connector writes into `.farik/local/browser/<task>/<session>/`, never the check's screenshot folder (I2); a Designer with Playwright off is `DesignerBrowser::NoConnector`, refused assignment with `designer_needs_browser` (I3), and its design review then waits as `designer_needs_browser`, a wire kind of its own in `task.get`, `tasks.list` and `waiting.list`, so Today links to the Designer's page to turn Playwright on and the task page and board say its browser is off rather than that it is paused (fix C); `Transitions::designer_browser` fails closed to `NoPreviews` where the driver never set previews (M-d), and the test harness tells the door what its orchestrator runs. `accepts_a_ui_change_only_once_the_designer_passed_it` wires the Definition of Done's design review item through the runtime (I4). Five rules the review's mutations left untested now each have a test that fails when its rule is mutated: a recorded review is acted on before the waits, the latest record counts, the review copies the latest check of each, `design_reviewer` is set only on a fail, and a page's path starts with `/` (M-a, M-b, M-h). The page check runs under a 150 s host deadline (`exec::supervise_with_input`, `supervise` with an input) and, like the connector, with `--pull never` (M-c). The daemon's `preview_missing` line already named the Designer by its display name through `name_of`; Task 7 saw the id only because the fixture's display name was its id, and the test now pins "Iris". `follows_serve_to_its_new_port_after_a_failed_take_on` holds serve's port for 8 s once it is let go, the landing review's reproduction, so the stale-port flake's fix has a regression test (M-g). After the re-review: an agent other than the Designer keeps its Playwright connector when the Designer has it off, since `NoConnector` means the preview and the sandbox are ready (N1). A preview does not start without the browser's pinned image, so the task escalates at once with reason `preview` and a detail that says to fetch the image in Setup, not after its sessions run out (N3; `DockerPreviewFactory.browser`). `waiting.list`'s browser-off row is tested to name the active Designer, not a paused first one, and the task page links to the same Designer (N5). Two gate tests kill the re-review's surviving mutations: a code review sent back between two design reviews is listed in its place in time (W4b), and a failed latest review is not also counted among the send-backs (W2b) (N4). A finished task's cleanup removes `.farik/local/browser/<task>/` with its worktrees (N6). The gate builds the rest of the approved GateDesignReview mockup: "Code reviewed" in "About this task", the reviewer's letter headed "reviewed the code after <Designer> passed the screens", and the send-backs' closing line, which names the builder but not the fix, since no wire carries it; the phone uses the same words as the computer, as the other letters do (N7). `follows_serve_to_its_new_port_when_the_driver_cannot_start` holds serve's port past its 5 s wait once a driver that cannot start lets it go, and `goes_back_to_setup_when_the_driver_cannot_start` now follows serve to its new port and its one new link, the designed fallback, rather than assuming the first port (fix E).

## Tasks

### Task 1: Previews, UI changes, connector checks and the design review in core

Produces: the `farik-core` items. Consumes: step 11's role.

Tests:
- `judges_a_ui_change_by_the_diff_or_the_field`:
  - a Developer's `app/Button.tsx` under the defaults is true;
  - `README.md` is false;
  - `README.md` with `ui_change: true` is true;
  - with `ui_paths: []` and no field it is false;
  - a Designer's or a Marketing Specialist's `.html` is false.
- `defaults_the_ui_paths_when_left_out`: the seven design globs; the team's own list replaces them; `[]` stays empty.
- `evaluates_connector_calls`:
  - `url: "http://localhost:4400/x"` passes;
  - each of these is `url_outside_preview` (F2): `http://localhost:44001`, `http://localhost:440`, `http://localhost:4400@evil.test`, `http://localhost:4400.evil.test/`, `http://127.0.0.1:4400`, `https://localhost:4400`, `HTTP://localhost:4400`, `/x`, a nested `url`, and a `url` holding an array;
  - `browser_evaluate` is `tool_denied`, an unlisted tool is `tool_not_tagged`, no connector is `connector_not_in_session`, and an `external_effect` tag is denied.
- `refuses_to_assign_a_designer_without_its_browser`: `preview_not_set` for `NoPreview` and `designer_needs_sandbox` for `NoSandbox`; `Ready` passes; a Developer is never refused.
- `requires_the_design_review_when_needed`: `DesignReviewPassed` fails on `Missing`, and holds on `NotNeeded` and `Passed`.
- `lets_the_designer_reject_a_ui_change`: a `verifying → rejected` request from the id in `design_reviewer`, with reasons, passes; one from another agent fails as today.
- `validates_the_preview_and_the_connectors`: the bounds on `prepare`, `start`, `port` and `path`; `unknown_connector: selenium` on any agent's list; every existing fixture still validates.
- `reads_the_new_events`: each new body, and the `preview` reason, round-trips through `event.schema.json`.

- [x] `feat(core): add previews, ui changes, connector checks and the design review rule`

### Task 2: The connector base: definition, preview, launch, and the hook

Produces: `playwright.yaml`, `builtin_connector`, the preview and connector runtime, the hook's check, the computer check's row "Browser for the UI/UX Designer", and CI's pull. Consumes: Task 1.

Tests:
- `prepares_once_per_tree_then_starts` (fake factory): the second session on the same tree records no `preview.prepared`; a new commit prepares again. `preview.started` and `preview.stopped` come in order, with the port.
- `escalates_a_preview_that_fails`: a failing `prepare`, a `start` that never answers, and an unavailable Docker each give `escalated` with `preview` and the output tail.
- `offers_the_connector_only_where_the_design_says`: in `explore`, `implement` and the design review for an agent that has it on; not in the Architect's `verify`; not when the Designer is not `Ready`.
- `denies_a_connector_call_outside_the_rules` (hook): each `ConnectorRefusal` kind is recorded as `tool.denied` with `server` and `tag`; a good call is recorded as `tool.called` with `tag: network`.
- `lists_the_denied_tools_as_disallowed`: `claude_args` carries `Bash` and the denied names.
- `launches_the_browser_confined`: `connector_server`'s arguments carry the container name, the label, `--user`, `--network container:<preview>`, `--proxy-server http://127.0.0.1:9` and `--allowed-origins`.
- `proposes_the_designer_with_its_connector`, including unticked when there is no sandbox.
- `lists_the_designer_browser_row_only_with_a_designer` (R6): `check_computer` has the row "Browser for the UI/UX Designer" only when the team has a Designer. Its state is ready when `docker image inspect <image@digest>` succeeds and missing when it fails. `SetupComputer.test.tsx`'s `shows_the_designer_browser_row` shows the row and its pull button.
- Integration (`#[ignore = "needs docker"]`), with `alpine:3.22` serving a page with busybox-extras' `httpd` (F3; as built: Alpine 3.22's busybox has no `httpd` applet, so the tests fetch busybox-extras once with `apk add`, network on, and run `./busybox httpd …`):
  - `the_pinned_image_lists_the_pinned_tools`: the drift test;
  - `the_browser_reaches_only_the_preview` (R2). Navigating to the preview passes. The redirect is a busybox `httpd` CGI, `/cgi-bin/away`, answering `302` with `Location: http://example.com/`. It runs three times, each with one barrier:
    - (a) `--network none` alone: both the direct navigation to `http://example.com/` and the redirect fail;
    - (b) `bridge` plus the proxy alone: both fail;
    - (c) `bridge` plus `--allowed-origins` alone: the direct navigation fails; the redirect is recorded as reaching `example.com`, since the server's README says `--allowed-origins` does not affect redirects. That is why the proxy exists.
  - `stop_leaves_no_container`: after `stop`, neither container name exists.

- [x] `feat(runtime): prepare and start a preview in the sandbox, and a confined Playwright connector per agent`

### Task 3: `farik_check_page`

Produces: `check_page`, the tool and `page.checked`. Consumes: Task 2.

Tests:
- `checks_a_page_through_the_runner` (fake runner): the arguments per width and theme, `--user <uid>:<gid>`, the proxy, `page.checked` recorded, and the answer's shape, under the untrusted notice.
- `refuses_outside_a_designer_session`: `check_page_refused` for the Architect, and for a session with no preview.
- `returns_the_screenshot_as_an_image_block`: `mcp.rs` answers text plus an `image/png` block.
- `bundles_the_axe_the_web_tests_pin`: the banner equals `packages/ui`'s `axe-core`, and the tag list has all five.
- Integration: `checks_a_page_on_the_pinned_image`: a page with an unlabelled button reports `button-name`, and the dark and light screenshots differ.

- [x] `feat(runtime): check a preview page for accessibility at two widths and two themes`

### Task 4: The design review of a Developer's UI change, and the wire

Produces: the design review in rule 5, the explore session's browser, and the RPC fields. Consumes: Tasks 1 to 3, and step 11's flow.

In rule 5, before the reviewer's session, when `is_ui_change` holds and the team `has_designer`:
- no preview set: wait (`preview_missing`);
- the Designer is not `Ready`: wait (`designer_needs_sandbox`);
- the only Designer is paused: wait (`waiting_on_designer`);
- the latest review since the task entered `verifying` failed: reject in the Designer's name, with its reasons (checked before the next case, R1);
- no review since the task entered `verifying`: start the Designer's read-only session (D9).

Step 11's explore session gains the connector and `farik_check_page`, and loses its "no browser yet" line.

The recorded transcripts are `implement_css_frk_2`, `design_review_passes_frk_2` and `design_review_fails_frk_2`.

Tests:
- `checks_a_ui_change_before_the_architect`: the Designer's read-only session first, with the listed tools and no `farik_exec`; the Architect's second.
- `sends_a_failed_design_review_back_to_the_developer`: `rejected` by the Designer's id, with its reasons; no Architect session and no second Designer session.
- `leaves_a_non_ui_change_to_the_architect`, and `reviews_alone_without_a_designer` (every Designer retired).
- `waits_on_a_paused_designer`, and `waits_for_a_missing_preview`: no session starts, and `design_review.state` is `waiting_on_designer` or `preview_missing`.
- `refuses_an_incomplete_design_review`: three checks give `design_review_incomplete`; the fourth lets the review record, with `checks` copied from the events.
- `starts_the_design_review_again_without_an_answer`: a session that ends with no record is followed by a new one, and at the sessions allowance the task escalates with `sessions`.
- `runs_both_again_after_a_send_back`: after a send-back, the Designer's pass is required again.
- `explores_with_the_browser`: the explore session's tools now include the connector and `farik_check_page`.
- `answers_the_task_with_its_review`: `task.get`'s fields, `task.screenshot`, and `settings.defaults.ui_paths`.
- `refuses_a_screenshot_the_task_did_not_take`: `../x.png` and another task's file each give `not_found`.

- [x] `feat(runtime): check a Developer's UI change in the browser before the Architect reviews it`

### Task 5: The pages

Produces: the page changes, as the step 11 mockups show them. Consumes: Task 4's wire.

Tests, each also running axe:
- `asks_how_to_open_the_app_when_the_designer_is_on`: the Designer's setup card shows the prepare, start, port and first-page fields, and the daemon's refusal of port `80` appears at its field.
- `switches_a_connector`: turning Playwright on for Theo sends `mcp_servers` with it, after the validate effect shows.
- `saves_the_preview_in_settings`: `team.save` is sent with `preview { prepare, start, port, path }`, after the effect line.
- `edits_the_ui_paths_in_advanced`: the seven defaults come from `settings.defaults`; adding `**/*.strings` saves eight.
- `shows_the_four_checks_with_their_screenshots`: four figures, alt text "Phone, light" and so on, and each violation's help.
- `puts_the_designers_letter_first_on_the_gate`: the Designer's letter precedes the Architect's review in document order.
- `marks_a_task_waiting_on_the_designer`: a `verifying` card with `waiting_on_designer` shows "Waiting on the Designer".
- `links_the_waiting_rows_to_settings`: `preview_missing` links to `/settings#preview`, and `designer_needs_sandbox` shows its sentence.

- [x] `feat(web): show the design review, and ask how to open the app`

### Task 6: `--preview`, and the Designer's journey

Produces: the `e2e` admit branch, and `designer.spec.ts`. Consumes: everything above.

`farik-e2e-serve` gains `--preview` (D1) and `--sandbox-image <image>`, both e2e-only.

The journey:
- runs in Docker sandbox mode on `alpine:3.22`, with the fixture team `pm-architect-developer-designer` (Mira, Ada, Theo and Iris, with `judgment.required: never`);
- has `preview: { start: "./busybox httpd -f -p 4401 -h site", port: 4401 }` over a two-file `site/`, the project carrying busybox-extras' binary, which the fixture fetches once per machine with `apk add` (so the first run on a machine needs the network; CI has it);
- has review-only criteria.

Steps:
1. A request is triaged, refined for Iris, and assigned to her.
2. Iris explores, and her `farik_check_page` really runs on the Playwright image.
3. The plan waits for Mira, then is approved.
4. Iris implements, and Ada reviews and accepts. The log holds `design_plan.proposed`, `design_plan.approved`, and `review.recorded` by Ada, in that order.
5. Theo's change to `site/style.css` shows "Checking the screens". Then `design_review.recorded { pass: true }` comes with four `page.checked` events, and Ada's review follows.
6. Screenshots of the team, the task page and the gate at 360 and 1280 px, with no sideways scroll at 360.

Tests in `serving.rs`, built with `e2e`:
- `admits_a_local_browser_without_a_code_in_preview_mode`: with `--preview`, `GET /` with `Host: localhost:<port>` sets the session cookie and serves the app. `Host: localhost:<other>` and `evil.example` are refused.

- `refuses_preview_without_the_e2e_feature` (R12), under `#[cfg(not(feature = "e2e"))]` so the default `cargo test --workspace` runs it: `farik serve --preview` is refused by the argument parser as an unknown argument.

In `xtask/src/check.rs`:
- `integration_steps` gains `cargo clippy -p farik-runtime --features e2e -- -D warnings`, since `cargo clippy -p farik --features e2e` does not lint `farik-runtime`'s `#[cfg(feature = "e2e")]` code. It is asserted by `integration_steps_lint_the_runtime_with_e2e`.
- The release build never uses `--all-features`, which would switch the admit branch on (R13). This is enforced by `never_builds_with_all_features`, which fails when any xtask command or any `.github/workflows/*.yml` contains `--all-features`.

- [x] `test(web): walk a Designer's task and a design review through the real server and browser`

### Task 7: Spec and plan

`docs/SPEC.md`, under revision 0.33 (2026-09-30; 0.32 is the DevOps Engineer, ADR 0027), "from phase 6 step 12":
- 4.1: the preview commands, and the Designer needing Docker's sandbox;
- 5.4: the design review, and the Definition of Done item;
- 5.6: connectors, tags and `url_outside_preview`;
- 5.7: the `preview` escalation;
- 5.12: `ui_paths`;
- 6.7;
- 8.2: the hook's connector check;
- 8.3: `prepare`, `start` and the browser container, with the proxy; the release build is never built with `--all-features`;
- 8.5: the events;
- 8.6: "The Designer's browsing is limited to the project's preview. `prepare` runs the project's install and build with the network on, as the founder decided, and the browser's confinement does not cover it." (R14);
- F9.

The project plan: step 12's line gains "Built 2026-09-30 (spec 0.33): …".

- [x] `docs(spec): record the Designer's preview, page check and design review`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (T1 8 new, T2 8 plus 3 Docker, T3 4 plus 1 Docker, T4 12, T6 4);
#   @farik/web: step 11's landed count plus 9 (T5 8, T2 1);
#   playwright: step 11's landed count plus 1 (designer.spec.ts) passed;
#   last line: xtask check: ok
# built 2026-09-30 (Task 6's run): cargo 1685 passed, 0 failed (step 11's landed 1637, plus 48: the
#   named tests, extras and guards of Tasks 1 to 4 and 6); protocol-client 8, brand 30, ui 43,
#   @farik/web 145 (133 plus 12: T5's 8 and 3 extras, T2's 1); playwright 11 passed (10 plus
#   designer.spec.ts); xtask check: ok. Task 7 (docs only) ran `cargo xtask check`.
```

The integration run needs Docker with `alpine:3.22` and the pinned Playwright image pulled, which CI's workflow does.
