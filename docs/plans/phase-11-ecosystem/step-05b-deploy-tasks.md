# Phase 11, step 05b: Deploy tasks and `farik_deploy`

Status: drafted; ready against phase 7's code on 2026-10-07 (to execute after the commits of phase 7 steps 10c to 10f, 10h and 11), its readiness review runs again when phase 11 starts
Branch: `phase/11-ecosystem` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.2, 5.3, 5.6, 5.7, 5.11, 5.14, 6.9, 8.4, 8.5, 8.6; F9
Depends on: the commits of phase 7 steps 10c to 10f, of phase 9 step 01 (ask or auto, phase 7 step 10h until ADR 0049) and of step 05: execution starts only after they exist. Step 05 (`Role::DevopsEngineer`, ADR 0045, the approved boards `TodayDeployApproval` and `SprintStartDeploys`); phase 9 step 01 (`ApprovedBy` on `tool.called`, `approval_mode` in `farik_store::approvals`, the mode kept in this computer's log, `auto_acts`, `pages/DoneOnItsOwn.tsx`, the act table); phase 7 step 09c (`TransitionEffect::NothingToIntegrate`, `WorkState.folder`, the branch-less sites keyed by `task_private_folder`); phase 7 step 02 (`tool_approval.requested`, `open_grants`, the ask that stops a session, `ToolApproval.tsx`); phase 6 step 15 (sprints, `StartSprint.tsx`); phase 6 (merged in #19). File:line citations are at 8fd2751; the names are what count.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-07 (one round, ADR 0032): not ready, 6 Blocking and the Should items, all folded below with the founder's answers; no second round
Mockups approved by: step 11's boards (2026-10-07) and this step's new ones, by the founder on 2026-10-08 as drawn ("Approve as drawn"): `DoneOnItsOwnDeploy`, `PhoneDoneOnItsOwnDeploy`, `PhoneDeployApproval`, `PhoneSprintStartDeploys` (`.dc.html`), with the choice shown on them (a sprint approves a task's first deploy in it; a retry, or a deploy again after a review sent it back, asks)
Decided by the founder, 2026-10-07, in conversation: (1) whether starting a sprint approves a deploy task that joins it after it started, under an epic the sprint holds, "Yes, the sprint covers it": the sprint's start approves every deploy task in the running sprint, however it got there (its first deploy there: a retry asks, as the approved `TodayDeployApproval` says), against the reviewer's recommendation (only those the assigner planned); the gate below, ADR 0045's item 1 (step 11's Task 1) and spec 5.6 say so; (2) how a deploy made on auto reads under "Done on its own", `"Lena put <version> live for <task>"`: a `deploy` kind of its own, "{name} put {sha} live for {task}", drawn on new boards for the founder's approval with the phone versions of the deploy question and of the sprint-start line (Task 0).
Amended 2026-10-07 by step 11's readiness review and the founder's answer to it ("On this computer only"; ADR 0045, 3): the production settings live in this computer's log, never in `team.yaml` or a template; "The production settings" below carries it, and steps 11c to 12e read them with `farik_store::production::production(log)` wherever they name `Team::production`.
Moved 2026-10-09 by ADR 0049 (project plan revision 41; the founder: "DevOps later, rest after Cloud"): phase 7 step 11b until then (its file was `step-11b-deploy-tasks.md` in phase 7's folder). The DevOps Engineer is built in the Ecosystem phase, phase 11, after its own steps 01 to 04: phase 7's steps 11 to 11f are steps 05 to 05f here, and 12 to 12e are 06 to 06e. The text below names them by their new numbers, and phase 7's other steps as phase 7's; the dated lines above, and the founder's words, keep the numbers of their day. Phase 7 step 10h, ask or auto, is phase 9 step 01; step 13, the kit check, is phase 9 step 02 and has no DevOps task, so this phase checks the DevOps Engineer's kit itself; the phases after phase 8 moved up by one.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 05 (see step 05's header). The decisions shared with steps 05c to 05f are ADR 0045's; this plan states the ones it builds.

## Goal

A team that plans a deploy can run it: the Product Manager writes a deploy task (`change: deploy`) that depends on the tasks it ships, the sprint that holds it is started, and the DevOps Engineer's session calls `farik_deploy`, which takes nothing: Farik deploys the commit the integration branch held after the last of those tasks integrated, to the service named by the production settings the owner set on this computer. A deploy task has no branch; it reaches `verifying` when Farik has recorded its deploy settled healthy, its reviewer reads the deployment rather than a diff, and accepting it integrates nothing. A `farik_deploy` the sprint did not approve asks the owner on Today in plain words, or, when the team acts on its own, runs and is listed under "Done on its own"; the sprint-start dialog says what starting approves. Built over a fake platform; the real ones are step 06's. Out of scope: watching the deploy settle and moving the task to `verifying` (05c); incidents (05d, 05e); the pages, the settings' form and the command line (05f).

## Decisions

- **`change: deploy`.** `task-contract.schema.json`'s `change` enum gains `deploy`, its description saying a deploy task is the DevOps Engineer's and ships its dependencies. The generated `Change::Deploy` makes `task_branch` (`branch.rs:14`) need an arm: `deploy/<id>`, never created.
- **No branch, site by site** (Should). `farik_core::branch::works_on_a_branch(contract) -> bool` is false for a contract whose assignee role has a private folder (phase 7 step 09c) or whose `change` is `deploy`, true otherwise. For a deploy task: rule 7 (`assigned`, `rules.rs:1354`) makes no worktree and no copy; rule 6 (`in_progress`, `:1209`) runs the deploy session (below); `session_dir` (`orchestrator.rs:740`) gives the project root, so the reviewer's session starts there too; `Transitions::work` (`transitions.rs:715`) gives `DeployWork`; `check_criteria_recorded` (`governor/gates.rs:504`) reads it; `transition` (`transition.rs:670`) adds `NothingToIntegrate` to the move into `accepted` of any contract that does not work on a branch; `review` (`verify.rs:475`) sends the deployment; `farik_store::diff::diff_of` (`diff.rs:37`, its folder arm at `:48`) answers `TaskDiff { deploy: true }` with no diff and no files, so `task.diff` (`daemon/gates.rs:422`) answers `deploy: true` where a folder task answers `private_folder: true`, and `farik show` (`cli/src/show.rs:306`) prints "A deploy task changes no file."; the human's integrate (`integrate.rs:83`) refuses "nothing_to_integrate: CTV-40 is a deploy task and has no branch; its acceptance was its end". Unchanged, as a deploy task never reaches them: `verify.rs:174` (no criterion Farik runs), `done.rs:297` (no changed path), `implement_message` (`messages.rs:723`), `resume` (no worktree, so no last commit). Its `allowed_paths` pass the ordinary rules and are not used.
- **Readiness** (`readiness.rs`): a new rule, `DeployTaskShape`, for a contract whose `change` is `deploy`: its `assignee_role` is `devops_engineer`; it is a task; it lists at least one dependency, and each is a task that works on a branch (Blocking 5: an epic, a private-folder task or another deploy task never gets a `task.integrated`, so it would refuse the deploy forever); every exit criterion is `review` or `human`; `ui_change` is absent or false. `ReadinessContext` gains `dependency_branches: BTreeMap<String, bool>` (a dependency's id to whether it is a task that works on a branch), filled where `dependency_statuses` is (`transitions.rs:685`). Its sentence in `plain.rs`: "A deploy task is the DevOps Engineer's, ships at least one task it depends on whose work Farik adds to your project, and is checked only by its reviewer or by you." `CHECKS` (`readiness.rs:151`) grows from 22 to 23. `required_criteria_present` (`:458`) asks a deploy task for the team's required `review` and `human` only, as it asks a private-folder task for no `command` or `test`.
- **The production settings live in this computer's log** (Blocking 1; ADR 0045, 3). `$defs/production` in `command.schema.json` and `event.schema.json`, `additionalProperties: false`: `connector` (the connector name pattern), `service` (1 to 200 characters, no control character: how the platform names the service; each adapter of step 06 says its shape), `health_url` (`^https://`, at most 2,000 characters), `settling_minutes` (1 to 60, default 5), `error_rate_percent` (0.1 to 100, absent meaning no threshold). The owner's command `production_set { production | null }`, on `POST /command` behind the daemon's token or the browser's RPC `command` behind its cookie, is handled in `orchestrator/human.rs` under a static `PRODUCTION` lock beside `DECIDING` (`human.rs:488`). It refuses `production_connector_unknown` at `/production/connector` (no DevOps Engineer that is not retired has an `mcp_servers` entry of that name) and `health_url_invalid` at `/production/health_url` (userinfo, or no host), through `farik_core::team::check_production(&Production, &Team)`; otherwise it records `production.changed { from, to }` naming no agent and no session, or nothing when the settings are unchanged. `farik_store::production::production(log)` is the newest such `to` (`null` clearing), and ignores one that names an agent or a session. The query `production.get {}` (`daemon/board.rs`) answers `{ production | null }`. `Team` gains nothing: the root of `team.schema.json` already refuses a `production` key, and templates carry none. Rejected: `team.yaml` beside `preview`, which a pull, a stale page's save or a hand edit could re-point at another service.
- **The platform.** `crates/runtime/src/platform.rs` holds `Platform` and `PlatformError`. `DaemonState` gains `platforms: OnceLock<PlatformSource>`, set once with `set_platforms` as `set_google_ads_api` is (`daemon.rs:385`), and `platforms()` answers it, or `shipped_platforms()` while unset. `farik_deploy` reaches it through `ToolContext.daemon`, upgraded as `posts.rs:323` does; a daemon that is gone answers `platform_unavailable: Farik is not running its connections`. Rejected: a `ToolContext.platforms` field, since the context already reaches the daemon (`tool_context`, `daemon.rs:745`); `ToolDeps.platforms`, which `start` builds before the daemon (`command_deps`, `cli/src/start.rs:208`; `connected_daemon`, `:230`), so step 06's source, which reads the daemon's key stores, could not be put there. `shipped_platforms()` answers `NotSupported { connector }` for every connector in this step ("Farik cannot drive <connector> yet"), as phase 7 step 05's `live_kit_pins` did before phase 7 step 06; step 06 adds the first adapter. Tests use `FakePlatform` (`platform/fixtures.rs`, `pub` for other crates' tests), which scripts each answer and records each call.
- **The events**, in `event.schema.json`, `protocol/src/event.rs` and spec 8.5: `deployment.started { deployment_id, commit, holds, connector, service, approved_by?, approval? }` (`deployment_id` the platform's, 1 to 200 characters; `commit` a full sha; `holds` the deploy task's dependencies; `connector` and `service` the settings it used; exactly one of `approved_by` and `approval`, copied from the `tool.called` the handler checked, so "Done on its own" finds a deploy made on auto by its own kind and the log says what let each deploy go), the envelope naming the deploy task, the agent and the session; `deployment.succeeded { started, healthy_minutes }` and `deployment.failed { started, why, detail }` (`started` the seq of its `deployment.started`; `why` `platform`, `unhealthy` or `timed_out`; `detail` at most 500 characters, the platform's words, untrusted). `$defs/approvedBy` gains `sprint`. This step records only `deployment.started`; step 05c records the other two, and this step reads them.
- **`farik_deploy {}`**, tier `external_effect`, in `tools/production.rs`. `default_tiers(DevopsEngineer)` gains `ExternalEffect`, which no other Farik tool and no built-in has, and which no connector call asks (5.6). It is offered only in a session whose `tools` names it, as only `DEPLOY_TOOLS` does (`offered_tools`, `session.rs:829`, leaves it out otherwise), so a DevOps Engineer's chat or code task is not offered it (Should). The handler holds `DaemonState::deploys()`, a `tokio::sync::Mutex<()>` beside `ads_writes` (`daemon.rs:397`), from its first check to its record (Blocking 6), and checks in this order, each refusal named: the caller is the assigned DevOps Engineer in an `implement` session about a deploy task (`deploy_refused`); the hook let this call go (`production_call_not_allowed`: the session's newest `tool.called` of `mcp__farik__farik_deploy` carries `approved_by` or `approval`, and no `deployment.started` of the session follows it; Blocking 3, since phase 9 step 01 records a grant as `approval`, never `approved_by`); the owner set production settings (`no_production`); no deploy of the task runs, that is no `deployment.started` of it without its `deployment.succeeded` or `deployment.failed` (`deploy_running`); every dependency has a `task.integrated` (`dependency_not_integrated`). The commit is the `sha` of the newest `task.integrated` among the dependencies, by seq; the agent cannot name another. `NotSupported` and `NotConnected`, from the source or from `platform.deploy(commit)`, answer `platform_unavailable: <sentence>`, `Refused` answers `platform_refused: <detail>` and `Failed` `platform_failed: <detail>`; none records anything, so the approval is not used up. A started deploy records `deployment.started` and answers `{ deployment, commit, said: "Deploying <short sha>. Farik watches it for <n> minutes; write your note and end your turn." }`.
- **The gate is the hook's** (ADR 0045, 1; Blocking 4). In `judge` (`hooks.rs:254`), `mcp__farik__farik_deploy` meets `production_gate` before `judge_call` (`:314`). It first refuses without asking, in the handler's words: `deploy_refused` for any caller but the assigned DevOps Engineer's `implement` session on a deploy task; `no_production`; `platform_unavailable` when the daemon's source answers no platform for the team and the settings (in this step always, so nobody is asked to allow a deploy that cannot run); `deploy_running`; `dependency_not_integrated`. None records more than the hook's `tool.denied`. Then, in order: the sprint's start approves (`approved_by: sprint`) a deploy task whose row's `sprint` is the open sprint's id, whoever planned it there (the assigner, or the governor putting a breakdown's task under an epic the sprint holds, `sprints.rs:262`: the founder's answer 1), when no `deployment.started` of it follows the `sprint.planned` that put it there; else an open grant of phase 7 step 02 for server `farik`, tool `farik_deploy`, input `{}` (`approval: <seq>`); else, when `approval_mode(&log)` is `Auto` (ADR 0041), `approved_by: auto`; else it records `tool_approval.requested { server: "farik", tool: "farik_deploy", input: "{}", input_sha256 }` and denies `approval_needed`, which stops the session. So a second deploy of a task in its sprint, a deploy after a send-back, a deploy task outside the running sprint and one with no sprint running each ask once. On a pass, `judge_call` takes the tool in `preauthorized_external_tools`, a new parameter that every other caller passes empty; `Pass` (`hooks.rs:193`) holds `approved_by: Option<ApprovedBy>` (phase 9 step 01 fills it with `Auto` for a connector; this step with `Sprint` or `Auto`), which `tool.called` records. `farik` is never a connector's name (`connector_name_reserved`), so an approval for server `farik` cannot be a connector's. `Call::permit` (`tools.rs:586`) gives the bare name `farik_deploy` in `preauthorized_external_tools` to a DevOps Engineer alone, and `production_call_not_allowed` holds the handler to the hook's pass. Neither reads `team.yaml`'s `preauthorized_external_tools`, as today (Should).
- **The deploy session** (rule 6, `rules.rs` `in_progress`): for a deploy task, `SessionAsk { purpose: Implement, cwd: <project root>, executor: None, read_only: true, tools: Some(DEPLOY_TOOLS) }`; being about a task, it is given its agent's connectors (8.2), the platform's read tools among them. `DEPLOY_TOOLS`: `farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_decisions`, `farik_write_note`, `farik_declare_blocked`, `farik_ask_human`, `farik_deploy`. Its first message, `deploy_message` (`messages.rs`), names the commit, the tasks it holds with their titles, the rejection that sent it back when `resume` finds one, and the paths changed since the commit of the newest `deployment.succeeded` of any task (`Git::changed_paths`), or, with none, "Farik has no earlier healthy deploy to compare with, so no paths are listed." (Should), all but its first line in an `untrusted` block cut at 16 KiB. Rule 6 passes over a deploy task while its newest deploy runs, and once it succeeded unless the task was sent back since (Should; step 05c then moves it to `verifying`).
- **A send-back** (Should). A reviewer's failing criterion returns a deploy task to `in_progress` through `rejected`, as any task (5.2); its next session may deploy again, which asks (the gate above), or declare itself blocked. Today says why in a fourth sentence, drawn on `PhoneDeployApproval`: "Its last deploy was sent back, so Lena asks before deploying again." Rejected: refusing a redeploy of a settled commit, which would leave the task no way back to `verifying` but a blocker; and step 05's `retry` sentence, "Its first deploy failed", which would be false.
- **Today** (Blocking 2), as the approved `TodayDeployApproval` draws it. The `tool_approval` row of `waiting.list` (`farik_store::waiting`, `undecided`, `waiting.rs:434`; `waiting_row` in `daemon/gates.rs`; `rpc.schema.json`; the client mapping) for server `farik`, tool `farik_deploy` gains `deploy: { commit, holds: [{ task_id, title }], service, connector, settling_minutes, why }`, and its `line` is "Lena wants to put a new version live". `why` is read as the log stood at the ask's seq: `retry` when the task's newest `deployment.started` has a `deployment.failed`, `sent_back` when it has a `deployment.succeeded`, else `not_planned` when a sprint was open (a `sprint.started` with no later `sprint.ended`), else `no_sprint`. `commit`, `service` and `connector` are what a deploy would use now. The band line (`waiting_line`, `activity.rs:174`) is "Waiting on you: may Lena deploy CTV-40?". `Today.tsx` shows the row through `DeployApprovalRow` and the new `dialogs/DeployApproval.tsx`, never as `farik farik_deploy {}`, sending `tool_approve` or `tool_refuse`. "Where" names the platform by its kit's title for the connector, else the connector's name (the DevOps kit has none before step 06), and "Your production" links nothing until step 05f's page. `StartSprint.tsx` adds the line while the team has an active DevOps Engineer.
- **"Done on its own"** (the founder's answer 2). `farik_store::auto_acts` gains `AutoActKind::Deploy`, a `deployment.started` with `approved_by: auto`, and `connector_call` skips a `tool.called` whose tool starts with `mcp__farik__`; `auto_acts.list` answers it `{ kind: "deploy", commit, holds, service, connector }`. A deploy the sprint or a grant let go is not listed, and one the platform refused records nothing to list.
- **`verifying`** (`gates.rs`): `WorkState` gains `deploy: Option<DeployWork>`, `DeployWork { settled: bool }`, which `Transitions::work` fills for a deploy task: `settled` when its newest `deployment.started` has a `deployment.succeeded`. `check_criteria_recorded` skips the commit and clean-worktree checks for it and refuses "the deploy has not settled healthy yet" unless settled.
- **The review** (`verify.rs` `review()`): in place of the diff, `Changes::Deployment` lists the commit; the tasks it holds; every other task integrated after the commit of the newest earlier `deployment.succeeded`, all of them for a first deploy, since they go live with it (Should); "Started at <time of `deployment.started`>"; "Settled healthy at <time of `deployment.succeeded`>, after <healthy_minutes> minutes" (Should: nothing records "when it went live"). **Acceptance** records `NothingToIntegrate`, so its dependants may start.
- **The skill.** `running-production` gains "Deploy tasks": read what goes out, check production first with the platform's read tools, call `farik_deploy` once, write a completion note, end the turn; after a send-back, read why before deploying again; never call a platform's tool that deploys.

## The web app's words

As the approved boards and Task 0's; `{name}` the DevOps Engineer, what an agent wrote through `visibly`.

- **Today's row** (`DeployApprovalRow`): "{name} wants to put a new version live" (`deployAsk`); "For {task} {title}. {why}" (`deployFor`), `{why}` one of "The running sprint did not plan this deploy, so {name} waits until you decide." (`deployWhyNotPlanned`), "Its first deploy failed, so {name} asks before trying again." (`deployWhyRetry`), "Its last deploy was sent back, so {name} asks before deploying again." (`deployWhySentBack`), "No sprint is running, so {name} waits until you decide." (`deployWhyNoSprint`); "Review".
- **The dialog** (`DeployApproval`): `deployAsk`; "{name} stopped to ask before putting a new version of your app live." (`deployStopped`); "What goes live" (`deployWhat`) over "{sha}, which holds {tasks}" (`deployHolds`, each task "CTV-12 Pie pre-order page", joined by commas and a last "and"); "Where" (`deployWhere`) over "{service} on {platform}, from Your production" (`deployWhereLine`); "For" (`deployForLabel`); the why; "A note for {name}" "(optional)"; "“Allow once” lets {name} deploy this version once, in {name}’s next session on this task. Farik watches it for {minutes} minutes. Any other deploy asks you again." (`deployAllowNote`); "Don’t allow", "Allow once".
- **The sprint start** (`sprintStartDeploys`, by the DevOps Engineer's picture): "Starting it also approves the deploys {planner} plans into it: {name} puts each one live once the work it ships is in, and Farik watches it. A deploy outside the sprint asks you first."
- **"Done on its own"**: "{name} put {sha} live for {task}" (`autoActDeploy`, `{task}` the id); opened, "What went live" (`autoActWentLive`) over `deployHolds`, "Where" over "{service} on {platform}" (`autoActWhere`), "For" over the task's link, and "{name} went ahead without asking, because your team acts on its own." (`autoActDeployWhy`).

## File map

```
docs/design/mockups/{DoneOnItsOwnDeploy,PhoneDoneOnItsOwnDeploy,PhoneDeployApproval,PhoneSprintStartDeploys}.dc.html, canvas.json   creates, modifies (Task 0)
docs/schemas/task-contract.schema.json                       modifies: change gains deploy (Task 1)
crates/core/src/branch.rs, governor/readiness.rs, governor/plain.rs   modifies: works_on_a_branch, DeployTaskShape (Task 1)
crates/runtime/src/transitions.rs                            modifies: dependency_branches (Task 1); DeployWork (Task 7)
docs/schemas/{command,event,rpc}.schema.json, crates/protocol/src/{command.rs,event.rs,event/fixtures.rs}   modifies: production (Task 2); the deploy kinds, approvedBy (Task 3); the row's deploy, the deploy act (Task 8)
crates/core/src/team.rs                                      modifies: Production, check_production (Task 2)
crates/store/src/production.rs, crates/store/src/lib.rs      creates: production (Task 2)
crates/runtime/src/orchestrator/human.rs, crates/runtime/src/daemon/board.rs   modifies: production_set, production.get (Task 2)
crates/runtime/src/platform.rs, crates/runtime/src/platform/fixtures.rs   creates: Platform, shipped_platforms, FakePlatform (Task 3)
crates/runtime/src/{tools.rs,tools/production.rs,tools/fixtures.rs,daemon.rs}, orchestrator/session.rs   modifies, creates: farik_deploy, permit, set_platforms, deploys, offered_tools (Task 4)
crates/core/src/governor/permissions.rs                      modifies: default_tiers (Task 4)
crates/runtime/src/daemon/hooks.rs                           modifies: production_gate, Pass, judge_call (Task 5)
crates/runtime/src/orchestrator.rs, orchestrator/{rules.rs,messages.rs}   modifies: session_dir, the deploy session (Task 6)
crates/roles/roles/devops_engineer/skills/running-production/SKILL.md   modifies (Task 6)
crates/core/src/governor/{gates.rs,transition.rs}, crates/runtime/src/orchestrator/{verify.rs,integrate.rs}   modifies (Task 7)
crates/store/src/diff.rs, crates/runtime/src/daemon/gates.rs, crates/cli/src/show.rs   modifies: deploy: true (Task 7); waiting_row (Task 8)
crates/store/src/{waiting.rs,activity.rs,auto_acts.rs}       modifies: the deploy ask, the band line, the deploy act (Task 8)
apps/web/src/pages/{Today,DoneOnItsOwn}.tsx, pages/dialogs/{DeployApproval,StartSprint}.tsx, src/strings/en.ts   modifies, creates (Task 8)
apps/web/src/pages/{Today,Board,DoneOnItsOwn}.test.tsx, pages/dialogs/DeployApproval.test.tsx   tests (Task 8)
docs/SPEC.md, docs/plans/project-plan.md                     modifies (Task 9)
```

## Interfaces

Consumes: `task_branch`, `task_private_folder`, `ReadinessRule`, `ReadinessContext`, `check_criteria_recorded`, `WorkState`, `TransitionEffect::NothingToIntegrate` (phase 7 step 09c), `Team`, `ValidationError` (`farik-core`); `EventLog`, `EventQuery`, `Git::changed_paths`, `diff_of`, `TaskDiff`, `Waiting`, `open_grants`, `waiting_line` (`farik-store`); `ToolContext.daemon`, `call_tool`, `Call::permit`, `offered_tools`, `judge`, `judge_call`, `Pass`, `Denial`, `APPROVAL_NEEDED`, `SessionAsk`, `run_session`, `resume`, `session_dir`, `Transitions::work`, `review`, `ads_writes` as the pattern (`farik-runtime`); `ApprovedBy`, `approval_mode`, `auto_acts`, `AutoActKind`, `DoneOnItsOwn.tsx` (phase 9 step 01); `ToolApproval.tsx`, `StartSprint.tsx`, `visibly` (web).

Produces:

```rust
pub fn works_on_a_branch(contract: &TaskContract) -> bool;                              // farik_core::branch
pub struct Production { pub connector: String, pub service: String, pub health_url: String,
    pub settling_minutes: u16, pub error_rate_percent: Option<f64> }                    // farik_core::team
pub fn check_production(production: &Production, team: &Team) -> Result<(), Vec<ValidationError>>;
pub fn production(log: &EventLog) -> Result<Option<Production>, StoreError>;           // farik_store::production
pub struct DeployWork { pub settled: bool }   // WorkState.deploy: Option<DeployWork>    // governor::gates
pub type PlatformFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, PlatformError>> + Send + 'a>>;
pub enum DeploymentState { Building, Live, Failed, Superseded }                        // farik_runtime::platform
pub struct Deployment { pub id: String, pub version: String, pub state: DeploymentState, pub created_at: Option<DateTime<Utc>> }
pub trait Platform: Send + Sync {
    fn deployments(&self) -> PlatformFuture<'_, Vec<Deployment>>;          // newest first, at most 20
    fn live(&self) -> PlatformFuture<'_, Option<Deployment>>;                // what serves production now
    fn error_rate(&self, since: DateTime<Utc>) -> PlatformFuture<'_, Option<f64>>;   // percent, when offered
    fn deploy<'a>(&'a self, commit: &'a str) -> PlatformFuture<'a, Deployment>;
    fn restart<'a>(&'a self, live: &'a Deployment) -> PlatformFuture<'a, Option<Deployment>>;
    fn roll_back<'a>(&'a self, to: &'a Deployment) -> PlatformFuture<'a, Deployment>;
}
pub enum PlatformError { NotSupported { connector: String }, NotConnected { why: String }, Refused { detail: String }, Failed { detail: String } }
pub type PlatformSource = Arc<dyn Fn(&Team, &Production) -> Result<Arc<dyn Platform>, PlatformError> + Send + Sync>;
pub fn shipped_platforms() -> PlatformSource;
impl DaemonState { pub fn set_platforms(&self, source: PlatformSource) -> bool; pub fn platforms(&self) -> PlatformSource;
    pub(crate) fn deploys(&self) -> &tokio::sync::Mutex<()>; }
fn production_gate(request: &HookRequest, registration: &SessionRegistration, deps: &ToolDeps,
    state: &DaemonState, team: &Team) -> Result<Pass, Denial>;                          // daemon::hooks
pub enum DeployWhy { NotPlanned, Retry, SentBack, NoSprint }                            // farik_store::waiting
pub struct DeployAsk { pub commit: String, pub holds: Vec<(TaskId, String)>, pub service: String,
    pub connector: String, pub settling_minutes: u16, pub why: DeployWhy }              // Waiting.deploy: Option<DeployAsk>
// ReadinessRule::DeployTaskShape; ReadinessContext.dependency_branches; ApprovedBy::Sprint; TaskDiff.deploy: bool;
// Command::ProductionSet { production: Option<…> }; kinds production.changed, deployment.started/succeeded/failed;
// AutoActKind::Deploy; Pass.approved_by: Option<ApprovedBy>; judge_call(…, preauthorized: BTreeSet<String>)
```

## Tasks

Each test is watched to fail for the reason given, before the code that satisfies it.

### Task 0: Mockups

An Opus session (ADR 0032) draws four boards in the approved boards' tokens, components and words. On `canvas.json`'s page "Ask or auto": `DoneOnItsOwnDeploy` and `PhoneDoneOnItsOwnDeploy`, the "Done on its own" page with Lena's deploy new and opened above two of phase 9 step 01's acts, drawn as new boards rather than a revision of phase 9 step 01's approved `DoneOnItsOwn`, so that approval stands as given. On "DevOps Engineer": `PhoneDeployApproval` (Today's row and band; the dialog filling the screen, its two buttons pinned; the three other reasons, `sent_back`'s among them) and `PhoneSprintStartDeploys` (the sprint-start dialog filling the screen, with Lena's line). Gate: the founder approves them; the approval and its date go into this header in the same commit. Task 8 waits for it.

- [x] `docs(design): mock up deploys on auto and on the phone`

### Task 1: The deploy task's shape

- `a_deploy_task_has_no_branch` (`branch.rs`): `works_on_a_branch` is false for `change: deploy` and for a finance task, true for a Developer's fix; `task_branch` gives `deploy/CTV-40`. RED: no `deploy` value.
- `a_deploy_task_is_ready`: a DevOps Engineer's `change: deploy` task with two Developer tasks as dependencies and one `review` criterion passes all 23 rules, under a team that requires `test` and `review` criteria. RED: `RequiredCriteriaPresent` asks it for a `test`.
- `a_deploy_task_must_be_shaped`: another role's assignee; no dependency; a dependency that is an epic, a finance task or another deploy task; a `command`, `test` or `artifact` criterion; an epic; `ui_change: true`: each fails `DeployTaskShape` with its sentence. RED: no such rule.

- [ ] `feat(core): add the deploy task`

### Task 2: The production settings, in this computer's log

- `reads_the_settings_from_this_computers_log` (store): a log with none gives `None`; after two changes, the newest `to`, `settling_minutes` 5 when left out; a change stamped with an agent is ignored; `to: null` clears. RED: no `farik_store::production`.
- `setting_production_records_one_change` (runtime): `production_set` records `production.changed { from: null, to }` naming no agent and no session; the same settings again record nothing; `production.get` answers them. RED: no such command.
- `refuses_a_production_nobody_connected`: a `connector` no DevOps Engineer has is `production_connector_unknown` at `/production/connector`, a retired one's does not count, and nothing is recorded. RED: recorded.
- `refuses_a_bad_health_address`: `http://x` is a schema error at `/production/health_url`; `https://u:p@x` and `https://` alone are `health_url_invalid` there. RED: the last two recorded.
- `a_team_file_cannot_hold_production` (core): a team file with a `production` key is a schema error at `/production`. A guard: watched to fail by adding `production` to `team.schema.json`'s root, then reverted.
- `round_trips_the_production_change` (protocol): `production.changed` and `production_set` validate and read back equal; `settling_minutes: 0` is refused. RED: no such kind.

- [ ] `feat(runtime): keep the production settings on this computer`

### Task 3: The platform and the deploy events

- `a_fake_platform_answers_as_scripted_and_records_each_call`: scripted `Refused`, then a `Deployment`, two `deploy("bbb…")` answer each in turn, and its calls read `[Deploy("bbb…"), Deploy("bbb…")]`; `live()` with nothing scripted answers `None`. RED: no `FakePlatform`.
- `shipped_platforms_drive_nothing_yet`: for settings naming `vercel`, the source answers `NotSupported { connector: "vercel" }`, which reads "Farik cannot drive vercel yet". RED: no `shipped_platforms`.
- `reads_and_writes_the_three_deploy_events` (protocol): each round-trips, `approved_by: sprint` included; `why: other`, and a `deployment.started` naming both `approved_by` and `approval`, are refused. RED: no such kinds.

- [ ] `feat(runtime): add the platform a deploy goes through`

### Task 4: `farik_deploy`

Each test's session has a `tool.called` of the tool carrying `approval: 7` unless it says otherwise, and a daemon whose source is a `FakePlatform`.

- `deploys_the_integrated_commit`: dependencies integrated at seqs 40 (`aaa…`) and 52 (`bbb…`); the fake records `deploy("bbb…")`; `deployment.started { holds: [CTV-1, CTV-2], commit: bbb…, connector: vercel, service: shop, approval: 7 }` names the deploy task, the agent and the session; `said` names the short sha and 5 minutes. RED: no such tool.
- `refuses_outside_a_deploy_task`: a Developer, and a DevOps Engineer's fix task, are `deploy_refused`. RED: they deploy.
- `refuses_without_settings`: with no `production.changed`, `no_production`, and the fake records nothing. RED: an internal error.
- `refuses_while_a_deploy_runs`: after `deployment.started` and before its outcome, `deploy_running`; after its `deployment.failed`, it deploys. RED: a second deploy.
- `refuses_before_the_work_is_in`: a dependency with no `task.integrated` is `dependency_not_integrated`. RED: it deploys the other's commit.
- `two_calls_at_once_deploy_once`: two calls in one session, each with its `tool.called`, the fake's `deploy` held until both have started: one `deployment.started`; the other, checked under the lock after it, `production_call_not_allowed`, its pass used up. RED: two deploys.
- `a_platform_refusal_records_nothing`: `Refused`, `Failed` and `NotConnected` answer `platform_refused`, `platform_failed` and `platform_unavailable`, and no event is recorded. RED: an event per call.
- `refuses_a_call_the_hook_did_not_allow`: with no `tool.called` carrying `approved_by` or `approval`, and with one the session's `deployment.started` follows, `production_call_not_allowed`; with `approved_by: auto` it deploys and `deployment.started` names `approved_by: auto`. RED: it deploys without a pass.
- `devops_holds_external_effect` (`permissions.rs`): `default_tiers(DevopsEngineer)` is exactly `[Read, WriteWorkspace, Execute, Network, GitLocal, ExternalEffect]`, and `grants_git_remote_and_external_effect_to_nobody_by_default` no longer lists `DevopsEngineer`. RED: the tier is missing.
- `deploy_is_offered_only_where_named`: a DevOps Engineer's chat and its fix task's session are not offered `farik_deploy`; a session whose `tools` names it is. RED: its tiers offer it everywhere.
- `the_daemon_gives_the_platform_it_was_set`: unset, `platforms()` answers `NotSupported` for any settings; after `set_platforms(fake)`, `farik_deploy` deploys through the fake; a second `set_platforms` answers false and the first stays. RED: no `set_platforms`.

- [ ] `feat(runtime): let the DevOps Engineer deploy the integrated commit`

### Task 5: The sprint approves, else a grant, auto or the owner

Each test's daemon has production settings and a `FakePlatform` source unless it says otherwise, so the gate reaches the path under test.

- `the_open_sprint_approves_its_deploy_task`: allowed, `tool.called { approved_by: sprint }`, no approval asked. RED: `requires_human_approval`, `evaluate_tool_call`'s refusal of an `external_effect` tool.
- `a_task_that_joins_the_running_sprint_is_approved`: a breakdown task the governor planned in under the sprint's epic after the sprint started is allowed with `approved_by: sprint` (the founder's answer 1). A guard: watched to fail by reading only the assigner's `sprint.planned`, then reverted.
- `a_second_attempt_asks`: after a `deployment.failed` of the task in its sprint, the call records `tool_approval.requested { server: farik, tool: farik_deploy, input: "{}" }`, is denied `approval_needed` and stops the session. RED: the sprint approves it again.
- `outside_a_sprint_asks_then_a_grant_allows_once`: no sprint open, asked; after `tool_approve`, the next session's call is allowed with `approval: <seq>`, and a third asks again. RED: the grant is not read.
- `auto_runs_it`: on `auto`, allowed with `approved_by: auto`, nothing asked. RED: it asks.
- `refuses_before_asking`: a chat session's call is `deploy_refused`; with no settings, `no_production`; with the shipped source, `platform_unavailable`; while a deploy runs, `deploy_running`; before a dependency integrates, `dependency_not_integrated`; none records `tool_approval.requested`. RED: each asks.
- `team_yaml_cannot_pre_approve_a_deploy`: an agent whose `preauthorized_external_tools` names `mcp__farik__farik_deploy` and `farik_deploy`, with no sprint, still asks. A guard: watched to fail by passing the agent's list to `evaluate_tool_call`, then reverted.

- [ ] `feat(runtime): approve a deploy by the sprint's start, else ask`

### Task 6: The deploy session

- `assigning_a_deploy_task_makes_no_worktree`: rule 7 moves it to `in_progress` and `.farik/local/worktrees/CTV-3` does not exist. RED: rule 7 makes a worktree on `deploy/CTV-3`.
- `a_deploy_session_reads_and_deploys`: rule 6's session is `implement`, read-only, in the project's root, with no executor, offered exactly `DEPLOY_TOOLS` and the agent's connectors; its message names the commit, the two tasks and the changed paths inside `untrusted`. RED: an implement session in a worktree.
- `a_first_deploy_lists_no_paths`: with no `deployment.succeeded`, the message says "Farik has no earlier healthy deploy to compare with, so no paths are listed." RED: the line is absent.
- `waits_while_it_runs_and_once_it_settled`: no session while the deploy runs, nor after its `deployment.succeeded`; after a send-back, one whose message carries the rejection. RED: a session on every tick.
- `running_production_teaches_deploy_tasks`: the skill names `farik_deploy`, says to call it once, and says to read a send-back's reason first; step 05's `kit_skills_name_only_tools_farik_lists` passes over it. A guard: watched to fail with the section removed.

- [ ] `feat(runtime): run a deploy task's session`

### Task 7: Settled, reviewed, accepted

- `a_deploy_task_verifies_once_its_deploy_settled` (core): `deploy: Some(DeployWork { settled: true })` passes with no commit and no worktree; `settled: false` refuses with its sentence. RED: "the task branch has no commit on it".
- `work_reads_the_deploys_outcome`: `Transitions::work` gives `settled` from a fixture `deployment.succeeded`, and not once a later `deployment.started` follows it. RED: `WorkState::default()`.
- `the_reviewer_reads_the_deployment`: the review session starts in the project's root; its message lists the commit, the two tasks it holds, a third task integrated since the last healthy deploy, the start and settle times and the healthy minutes, and no diff. RED: git refuses the branch `deploy/CTV-3`.
- `acceptance_integrates_nothing`: the move to `accepted` carries `NothingToIntegrate`, a dependant is assignable, the human's integrate is `nothing_to_integrate` naming a deploy task, and `task.diff` answers `deploy: true` with no diff. RED: the effect is missing.

- [ ] `feat(runtime): finish a deploy task without a branch`

### Task 8: Today, the sprint start and "Done on its own"

As the approved `TodayDeployApproval` and `SprintStartDeploys` and Task 0's boards, in the words above.

- `waiting_list_describes_a_deploy` (`daemon/gates.rs`): the row's `deploy` holds the commit, both holds with their titles, `shop`, `vercel` and 5, and each of the four `why`s from its fixture; its `line`; the band reads "Waiting on you: may Lena deploy CTV-40?". RED: no `deploy` on the row.
- `done_on_its_own_lists_a_deploy` (store): a `deployment.started` with `approved_by: auto` is a `deploy` act with its commit and holds, and its `tool.called` is no `connector_call`; one with `approved_by: sprint` is not listed. RED: a `connector_call` of `farik`.
- `today_asks_about_a_deploy_in_plain_words` (`Today.test.tsx`): the row reads `deployAsk` and "For CTV-40 Ship the pie pre-order page." with its why; neither `farik_deploy` nor `{}` is on screen. RED: phase 7 step 02's generic row.
- `the_deploy_dialog_says_what_where_and_why` (`DeployApproval.test.tsx`): what goes live, where, for, the why, the note and `deployAllowNote` with 5 minutes; "Allow once" sends `tool_approve` with the approval and the note, "Don’t allow" `tool_refuse`. RED: no such dialog.
- `sprint_start_names_its_deploys` (`Board.test.tsx`): with an active DevOps Engineer the line names Sol and Lena; with none it is absent. RED: no line.
- `done_on_its_own_shows_a_deploy` (`DoneOnItsOwn.test.tsx`): "Lena put a1b2c3d live for CTV-40", opening to what went live, where, for and `autoActDeployWhy`. RED: the kind is unknown.

- [ ] `feat(web): ask about a deploy on Today, and list one made on its own`

### Task 9: Spec and plan

`docs/SPEC.md` 5.2 (a deploy task's `verifying`; its send-back), 5.3 (`DeployTaskShape`), 5.6 (the DevOps Engineer holds `external_effect`; `farik_deploy` offered only in a deploy session; the gate's order, the sprint covering a deploy task that joined it after it started, by the founder's answer 1; `team.yaml` never pre-approves a deploy), 5.7 (a deploy that waits on Today, and its four reasons), 5.11 (`change: deploy`), 5.14 (a deploy task integrates nothing), 6.9 (the production settings as kept; `farik_deploy` as built; "Done on its own"), 8.4 (the settings in this computer's log, under `.farik/local/`), 8.5 (`production.changed`, the three deploy kinds, `approved_by: sprint`), 8.6 (why the settings stay out of `team.yaml`); the revision line. Project plan row 05b.

- [ ] `docs(spec): record deploy tasks`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

No live check: no platform is driven before step 06.

## Execution notes

None yet.
