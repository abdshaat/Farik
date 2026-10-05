# Phase 7, step 11b: Deploy tasks and `farik_deploy`

Status: draft. Its readiness review runs once step 11 has landed (its mockups approved).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.2, 5.3, 5.6, 5.11, 6.9, 8.5; F9
Depends on: step 11 (`Role::DevopsEngineer`, ADR 0043); step 09c (`TransitionEffect::NothingToIntegrate`, `WorkState.folder` and the branch-less sites it keys by `private_folder`); step 02 (`tool_approval.requested`, `open_grants`, the ask that stops a session); step 10h (`ApprovedBy` on `tool.called`, `Team::acts_on_its_own`); phase 6 step 15 (sprints); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from row 11 (see step 11's header). The decisions shared with steps 11c to 11f are ADR 0043's; this plan states the ones it builds.

## Goal

A team that plans a deploy can run it: the Product Manager writes a deploy task (`change: deploy`) that depends on the tasks it ships, the sprint that holds it is started, and the DevOps Engineer's session calls `farik_deploy`, which takes nothing: Farik deploys the commit the integration branch held after the last of those tasks integrated, to the service the team's production settings name. A deploy task has no branch; it reaches `verifying` when Farik has recorded its deploy settled healthy, its reviewer reads the deployment rather than a diff, and accepting it integrates nothing. A `farik_deploy` the sprint did not approve asks the human, as a connector's call does. Built over a fake platform; the real ones are step 12's. Out of scope: watching the deploy settle (11c); incidents (11d, 11e); the pages (11f).

## Decisions

- **`change: deploy`.** `task-contract.schema.json`'s `change` enum gains `deploy`, its description saying a deploy task is the DevOps Engineer's and ships its dependencies. The generated `Change::Deploy` makes `task_branch` (`branch.rs:19`) need an arm: `deploy/<id>`, never created.
- **No branch.** `farik_core::branch::works_on_a_branch(contract) -> bool` is false for a contract whose assignee role has a private folder (step 09c) or whose `change` is `deploy`, true otherwise. Every site step 09c made skip a branch for a private folder asks it instead: rule 7's worktree (`rules.rs` `assigned`), `resume`, `Transitions::work`, `record_move`'s effect, `verify.rs`'s diff, the human's integrate and `daemon/gates.rs`'s `diff_of`. A deploy task's `allowed_paths` pass the ordinary rules and are not used: it has no worktree and no diff.
- **Readiness** (`readiness.rs`): a new rule, `DeployTaskShape`, checked for a contract whose `change` is `deploy`: its `assignee_role` is `devops_engineer`; it is a task; it lists at least one dependency; every exit criterion is `review` or `human` (nothing runs in a sandbox, and no file is made); `ui_change` is absent or false. Its sentence in `plain.rs`: "A deploy task is the DevOps Engineer's, ships at least one task it depends on, and is checked only by its reviewer or by you." `CHECKS` grows by one. `required_criteria_present` and `new_tests_required_by_rule` do not apply to it, as step 09c exempts a private-folder task.
- **The production settings**, in `team.schema.json` beside `preview`: `production: { connector, service, health_url, settling_minutes?, error_rate_percent? }`, `additionalProperties: false`. `connector` matches the connector name pattern; `service` is 1 to 200 characters with no control character (how the platform names the service; each adapter of step 12 says its shape); `health_url` matches `^https://`, at most 2,000 characters; `settling_minutes` 1 to 60, default 5; `error_rate_percent` a number from 0.1 to 100, absent meaning no threshold. `validate_team` refuses `production_connector_unknown` when no DevOps Engineer that is not retired has an `mcp_servers` entry of that name, and `health_url_invalid` for a URL with userinfo or no host. `template_from_team` leaves it out, as it leaves out `preview`. Rejected: a private file (ADR 0043, 3).
- **The platform.** `crates/runtime/src/platform.rs` holds the `Platform` trait and `PlatformError`. The daemon gives the team's platform: `DaemonState` gains `platforms: OnceLock<PlatformSource>`, set once with `set_platforms` as `set_connector_secrets` is (`daemon.rs:283`), and `platforms()` answers it, or `shipped_platforms()` while unset; the tool route copies it into `ToolContext.platforms` where it builds the context (`daemon.rs:572`). Rejected: `ToolDeps.platforms`, which `start` builds before the daemon (`crates/cli/src/start.rs:219`), so step 12's source, which needs the daemon's key stores, could not be put there. The shipped source, `shipped_platforms()`, answers `NotSupported { connector }` for every connector in this step ("Farik cannot drive <connector> yet"), the same honest answer step 05's `live_kit_pins` gave before step 06; step 12 adds the first adapter. Tests use `FakePlatform` (`platform/fixtures.rs`, `pub` for other crates' tests), which scripts each answer and records each call.
- **The events**, in `event.schema.json`, `protocol/src/event.rs` and spec 8.5: `deployment.started { deployment_id, commit, holds }` (`deployment_id` the platform's, 1 to 200 characters; `commit` a full sha; `holds` the task ids it ships), the envelope naming the deploy task, the agent and the session; `deployment.succeeded { started, healthy_minutes }` and `deployment.failed { started, why, detail }` (`started` the seq of its `deployment.started`; `why` `platform`, `unhealthy` or `timed_out`; `detail` at most 500 characters, the platform's words, untrusted). This step records only `deployment.started`; step 11c records the other two, and this step's gate reads `deployment.succeeded`.
- **`farik_deploy {}`**, tier `external_effect`, in `tools/production.rs`. `default_tiers(DevopsEngineer)` gains `ExternalEffect`, which no other Farik tool and no built-in has, and which no connector call asks (5.6). Checked in this order, each refusal named: the caller is a DevOps Engineer in an `implement` session about a deploy task it is assigned (`deploy_refused`); the hook allowed this call (below; `production_call_not_allowed`); no deploy of the task is running, that is no `deployment.started` of it without its `deployment.succeeded` or `deployment.failed` (`deploy_running`); every dependency has a `task.integrated` (`dependency_not_integrated`, which the assignment gate already makes rare). The commit is the `sha` of the newest `task.integrated` among the dependencies, by seq; the agent cannot name another. `platform.deploy(commit)`: `Refused` answers `platform_refused: <detail>` and records nothing, so the sprint's approval is not used up; `Failed` answers `platform_failed: <detail>`; a started deploy records `deployment.started` and answers `{ deployment, commit, said: "Deploying <short sha>. Farik watches it for <n> minutes; write your note and end your turn." }`.
- **The gate is the hook's** (ADR 0043, 1). In `judge_call` (`hooks.rs`), `mcp__farik__farik_deploy` meets `production_gate` before `evaluate_tool_call`, in this order: a session that is not about a deploy task is denied `deploy_refused` without asking; a deploy task in the open sprint (`row.sprint` equals the open sprint's id) with no `deployment.started` yet is approved by the sprint's start (`ApprovedBy::Sprint`); else an open grant of step 02 for server `farik`, tool `farik_deploy`, input `{}` (`approval: <seq>`); else, when `team.acts_on_its_own()` (ADR 0041), `ApprovedBy::Auto`; else it records `tool_approval.requested { server: "farik", tool: "farik_deploy", input: "{}", input_sha256 }` and denies `approval_needed`, which stops the session, so the second attempt of a deploy task, or a deploy task outside a sprint, asks once. A pass gives `evaluate_tool_call` the tool in `preauthorized_external_tools` and records `tool.called` with `approved_by`, whose wire enum gains `sprint`. `farik` is never a connector's name (`connector_name_reserved`), so an approval for server `farik` cannot be a connector's. `Call::permit` gives the same tool in `preauthorized_external_tools` to a DevOps Engineer alone, and `production_call_not_allowed` holds the handler to the hook's pass: the session's newest `tool.called` of `mcp__farik__farik_deploy` carries `approved_by` and no `deployment.started` of the session follows it.
- **The deploy session** (rule 6, `rules.rs` `in_progress`): for a deploy task, `SessionAsk { purpose: Implement, cwd: <project root>, executor: None, read_only: true, tools: Some(DEPLOY_TOOLS) }`; it is a session about a task, so it is given its agent's connectors (8.2), the platform's read tools among them. `DEPLOY_TOOLS`: `farik_read_task`, `farik_read_board`, `farik_read_rules`, `farik_read_decisions`, `farik_write_note`, `farik_declare_blocked`, `farik_ask_human`, `farik_deploy`. Its first message, `deploy_message` (`messages.rs`), names the commit, the tasks it holds with their titles, and the paths changed since the commit of the newest `deployment.succeeded` (`Git::changed_paths`), all of it in an `untrusted` block cut at 16 KiB. Rule 6 passes over a deploy task whose deploy is running.
- **`verifying`** (`gates.rs`): `WorkState` gains `deploy: Option<DeployWork>`, `DeployWork { settled: bool }`, which `Transitions::work` fills for a deploy task: `settled` when the newest `deployment.started` of the task has a `deployment.succeeded`. `check_criteria_recorded` skips the commit and clean-worktree checks for it and refuses "the deploy has not settled healthy yet" unless settled.
- **The review** (`verify.rs` `review()`): in place of the diff, the message lists the deployment: commit, the tasks it holds, when it went live, how long it stayed healthy. **Acceptance** records `NothingToIntegrate` (step 09c), so its dependants may start and `task.diff` answers as for a private-folder task.
- **The skill.** `running-production` gains "Deploy tasks": read what goes out, check production first with the platform's read tools, call `farik_deploy` once, write a completion note, end the turn; never call a platform's tool that deploys.

## File map

```
docs/schemas/task-contract.schema.json, team.schema.json, event.schema.json   modifies (Tasks 1, 2, 3)
crates/core/src/branch.rs                                    modifies: deploy arm, works_on_a_branch (Task 1)
crates/core/src/governor/readiness.rs, plain.rs              modifies: DeployTaskShape (Task 1)
crates/core/src/team.rs, team/template.rs                    modifies: Production, Team::production, the two refusals (Task 2)
crates/protocol/src/event.rs                                 modifies: the three kinds (Task 3)
crates/runtime/src/platform.rs, platform/fixtures.rs         creates: Platform, PlatformError, shipped_platforms, FakePlatform (Task 3)
crates/runtime/src/tools.rs, tools/production.rs, tools/fixtures.rs   modifies/creates: ToolContext.platforms, farik_deploy (Task 4)
crates/runtime/src/daemon.rs                                 modifies: DaemonState::set_platforms, platforms, the context (Task 4)
crates/core/src/governor/permissions.rs                      modifies: default_tiers (Task 4)
crates/runtime/src/daemon/hooks.rs                           modifies: production_gate (Task 5)
crates/runtime/src/orchestrator/rules.rs, messages.rs, verify.rs   modifies: the deploy session, the review (Tasks 6, 7)
crates/runtime/src/transitions.rs, crates/core/src/governor/gates.rs   modifies: DeployWork, the gate, the effect (Task 7)
crates/runtime/src/orchestrator/integrate.rs, daemon/gates.rs   modifies: works_on_a_branch (Task 7)
crates/roles/roles/devops_engineer/skills/running-production/SKILL.md   modifies (Task 6)
docs/SPEC.md, docs/plans/project-plan.md                     modifies (Task 8)
```

## Interfaces

Consumes: `task_branch`, `private_folder` (09c), `ReadinessRule`, `check_criteria_recorded`, `WorkState`, `TransitionEffect::NothingToIntegrate` (09c), `Team`, `validate_team` (`farik-core`); `ToolDeps`, `ToolContext`, `DaemonState::set_connector_secrets` as the pattern, `Call`, `tool`, `call_tool`, `judge_call`, `open_grants`, `APPROVAL_NEEDED`, `SessionAsk`, `run_session`, `Transitions::work`, `review` (`farik-runtime`); `ApprovedBy`, `Team::acts_on_its_own` (10h).

Produces:

```rust
pub fn works_on_a_branch(contract: &TaskContract) -> bool;                              // farik_core::branch
pub struct Production { pub connector: String, pub service: String, pub health_url: String,
    pub settling_minutes: u16, pub error_rate_percent: Option<f64> }                    // farik_core::team
impl Team { pub fn production(&self) -> Option<Production>; }
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
pub type PlatformSource = Arc<dyn Fn(&Team) -> Result<Arc<dyn Platform>, PlatformError> + Send + Sync>;
pub fn shipped_platforms() -> PlatformSource;
impl DaemonState { pub fn set_platforms(&self, source: PlatformSource) -> bool; pub fn platforms(&self) -> PlatformSource; }
// ToolContext.platforms: PlatformSource; ReadinessRule::DeployTaskShape; ApprovedBy::Sprint
```

## Tasks

### Task 1: The deploy task's shape

- `a_deploy_task_has_no_branch` (`branch.rs`): `works_on_a_branch` is false for `change: deploy` and for a finance task, true for a Developer's fix. RED.
- `a_deploy_task_is_ready`: a DevOps Engineer's `change: deploy` task with two dependencies and one `review` criterion passes every rule, with a team requiring `test` criteria. RED.
- `a_deploy_task_must_be_shaped`: another role's assignee, no dependency, a `command`, `test` or `artifact` criterion, an epic, and `ui_change: true` each fail `DeployTaskShape` with its sentence. RED.

- [ ] `feat(core): add the deploy task`

### Task 2: The production settings

- `reads_the_production_settings`: `production()` gives the five fields, `settling_minutes` 5 when left out. RED.
- `refuses_a_production_nobody_connected`: a `connector` no DevOps Engineer has is `production_connector_unknown`; a retired one's does not count. RED.
- `refuses_a_bad_health_address`: `http://x`, `https://u:p@x`, `https://` alone are refused at `/production/health_url`. RED.
- `a_template_leaves_production_out`. RED.

- [ ] `feat(core): keep the team's production settings`

### Task 3: The platform and the deploy events

- `a_fake_platform_answers_as_scripted_and_records_each_call`. RED.
- `shipped_platforms_drive_nothing_yet`: `NotSupported` naming the connector. RED.
- `reads_and_writes_the_three_deploy_events` (protocol): round trip, and `why: other` refused. RED.

- [ ] `feat(runtime): add the platform a deploy goes through`

### Task 4: `farik_deploy`

Files: `tools/production.rs`, `tools.rs` (descriptor, `call_tool` arm, `Call::permit`, `ToolContext.platforms`), `daemon.rs` (`set_platforms`, `platforms`, the context), `tools/fixtures.rs` (a fake by default), `default_tiers`.

- `deploys_the_integrated_commit`: dependencies integrated at seqs 40 (`aaa…`) and 52 (`bbb…`); the fake records `deploy("bbb…")`, and `deployment.started { holds: [FRK-1, FRK-2], commit: bbb… }` is recorded with the deploy task on its envelope. RED.
- `refuses_outside_a_deploy_task`: a Developer, and a DevOps Engineer's fix task session, are `deploy_refused`. RED.
- `refuses_while_a_deploy_runs`: a second call after `deployment.started` and before its outcome is `deploy_running`. RED.
- `a_platform_refusal_records_nothing`: `Refused` answers `platform_refused` and no event. RED.
- `refuses_a_call_the_hook_did_not_allow`: with no `tool.called` carrying `approved_by`, `production_call_not_allowed`. RED.
- `devops_holds_external_effect` (`permissions.rs`). RED.
- `the_daemon_gives_the_platform_it_was_set`: unset, `platforms()` is `shipped_platforms()`; after `set_platforms`, the context of a registered session carries it; a second `set_platforms` answers false. RED.

- [ ] `feat(runtime): let the DevOps Engineer deploy the integrated commit`

### Task 5: The sprint approves, else the human

Files: `hooks.rs` (`production_gate`), `event.schema.json` (`approved_by` gains `sprint`).

- `the_open_sprint_approves_its_deploy_task`: allowed, `tool.called { approved_by: sprint }`, no approval asked. RED.
- `a_second_attempt_asks`: after a `deployment.started` of the task, the call records `tool_approval.requested { server: farik, tool: farik_deploy }`, is denied `approval_needed`, and stops the session. RED.
- `outside_a_sprint_asks_then_a_grant_allows_once`: no sprint open, asked; after `tool_approve`, the next session's call is allowed with `approval: <seq>`, and a third asks again. RED.
- `auto_runs_it`: under `approvals: auto`, allowed with `approved_by: auto`. RED.
- `outside_a_deploy_task_refuses_without_asking`: a chat session's call is `deploy_refused`, and no approval is recorded. RED.

- [ ] `feat(runtime): approve a deploy by the sprint's start, else ask`

### Task 6: The deploy session

- `assigning_a_deploy_task_makes_no_worktree`: rule 7 moves it to `in_progress` and `.farik/local/worktrees/FRK-3` does not exist. RED.
- `a_deploy_session_reads_and_deploys`: rule 6's session is `implement`, read-only, in the project's root, with no executor, offered exactly `DEPLOY_TOOLS` and the agent's connectors; its message names the commit, the two tasks and the changed paths inside `untrusted`. RED.
- `waits_while_its_deploy_runs`: rule 6 starts no session for it. RED.
- `running_production_teaches_deploy_tasks` (guard over the skill's text: names `farik_deploy` and says once). Guard.

- [ ] `feat(runtime): run a deploy task's session`

### Task 7: Settled, reviewed, accepted

- `a_deploy_task_verifies_once_its_deploy_settled` (core): `deploy: Some(DeployWork { settled: true })` passes with no commit; `settled: false` refuses with its sentence. RED.
- `work_reads_the_deploys_outcome`: `Transitions::work` gives `settled` from a fixture `deployment.succeeded`. RED.
- `the_reviewer_reads_the_deployment`: the review message lists the commit, the tasks and the healthy minutes, and no diff. RED.
- `acceptance_integrates_nothing`: the move to `accepted` carries `NothingToIntegrate`, a dependant is assignable, the human's integrate is `nothing_to_integrate`. RED.

- [ ] `feat(runtime): finish a deploy task without a branch`

### Task 8: Spec and plan

`docs/SPEC.md` 5.2 (the deploy task's `verifying`), 5.3 (`DeployTaskShape`), 5.6 (the three tools' tier; the DevOps Engineer holds `external_effect`; the gate), 5.11 (`change: deploy`), 6.9 (the production settings; `farik_deploy` as built), 8.5 (the three kinds; `approved_by: sprint`); the revision line. Project plan row 11b.

- [ ] `docs(spec): record deploy tasks`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

No live check: no platform is driven before step 12.

## Execution notes

None yet.
