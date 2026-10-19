# Standalone: Catervas merges when the owner allows it

Status: ready
Branch: `feat/catervas-merges` (its own pull request to `main`)
Design: `docs/design/catervas-merges.md`; ADR 0073 (amends ADR 0061)
Spec: `docs/SPEC.md` 5.12, 5.14, 8.5 (revision 0.128)
Depends on: `origin/main` at `ca21251b`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-19, against the three readiness rules (ADR 0065); see the end

## Goal

The founder's brief of 2026-10-19 (design, "The brief"). When this is done:
- Settings → How work runs has a switch, "Let Catervas merge finished work". Turning it on shows a warning the owner confirms.
- Once it is on, Catervas merges each pull request it opened once all of GitHub's checks pass, records the integration as its own, and brings the local integration branch up.
- A failing check, a branch rule, or a refused merge stops the merge and escalates to the owner. A pull request with no checks at all merges once Catervas's own verification passed (the task is accepted).

Out of scope:
- sending a task back on a failed check;
- a command-line switch;
- GitHub's auto-merge;
- any change to how pull requests are opened.

## Wire (fixed before any task)

- `team.schema.json` `$defs/policy`: `merge_when_checks_pass?: boolean`, with the description "Whether Catervas merges its pull requests once GitHub's checks pass (ADR 0073). Read only with this computer's merge_permission.granted; changed only by merging.set. Absent means false." `team-template.schema.json` is unchanged.
- `event.schema.json`: three kinds, each added to `$defs.eventKind`, with a body `$def` and a `$ref` in the body `oneOf`.
  - `merge_permission.granted` with `mergePermissionGrantedBody { by: string, into: string }`. It is about no task. `into` is the resolved integration branch, `catervas_store::git::integration_branch(team, git)`, as every other `into` in the log is.
  - `merge_permission.withdrawn` with `mergePermissionWithdrawnBody { by: string }`. It is about no task.
  - `pull_request.merged` with `pullRequestMergedBody { url: string (minLength 1), method: "merge" | "squash" | "rebase" }`. It is about no task, even for a task's pull request (so `is_about_one_contract` stays false for it); the `url` ties it to its pull request.
  - The descriptions of `taskIntegratedBody` and the folder change's integrated body gain: "governor also when Catervas merged the pull request itself (ADR 0073)".
- `rpc.schema.json`:
  - Add `mergingSetRequest` beside `filesRewriteRequest`: method `"merging.set"`, params `{ on: boolean, confirmed?: boolean }` with `required: ["on"]` and `additionalProperties: false`. It answers `{ merging_allowed: boolean }`.
  - `teamGetResult` gains `merging_allowed: boolean`.
  - In TypeScript, `mergingAllowed`.
- Refusal codes, both at `/policy/merge_when_checks_pass`:
  - `merge_warning_unconfirmed`: "Turning merging on needs the warning confirmed."
  - `merging_offline`: "Catervas can merge only when your project is on GitHub."

## Decisions

- Everything in ADR 0073 and the design (the key, the gate, the stops, the method order, `integrated_by`), linked, not restated.
- The effective answer is computed in `catervas-store`, which reads the log; core does no I/O (hard rule 5).
  - `catervas-core` answers what the file says (`Team::merge_when_checks_pass`).
  - `catervas_store::merging::merging_allowed` combines it with the log.
- `checked()` (`daemon/team.rs`) copies the saved value onto every `team.save`, `agent.replace` and template application. At setup (`setup == true`) it sets `None`.
  - Rejected: refusing a `team.save` that changes the key. The web app sends the whole team, so an old tab would be refused for a field it never touched.
- The repository's methods are read with `gh api repos/{owner}/{repo}`, whose second argument does not collide with `FakeGh`'s `view` answer. Rejected: `gh repo view`, which would share the `view` key with `gh pr view` in `FakeGh`.
- The checks are read in the existing `gh pr view` call by adding fields, not in a second call. One read keeps the state and the checks consistent.
- `Checks::None` (no checks configured) merges like `Passed`, with no wait and no escalation: the task is accepted, so Catervas's own verification passed (changed 2026-10-19 on the founder's behalf; `NO_CHECKS_GRACE` is dropped).
  - Rejected: escalating after 10 minutes, which the founder replaced.
- `BEHIND` reuses `stopped_merging` unchanged: a task refreshes, a folder change escalates.
- `Asker::Human` never merges, and the merge runs on ticks alone.
- The setup strings `mayPushNote*` ("Nothing reaches your main branch without you.") stay as they are, since setup never turns merging on.
- One test per major functionality (ADR 0060). The no-merge guarantee is a security guarantee and has its own test.

## File map

```
docs/schemas/team.schema.json                      modifies: policy.merge_when_checks_pass (Task 1)
crates/core/src/team.rs                             modifies: Team::merge_when_checks_pass, carried_team drops it; tests (Task 1)
crates/core/src/team/describe.rs                    modifies: the effect line (Task 1)
docs/schemas/event.schema.json                      modifies: three kinds and bodies (Task 2)
crates/protocol/src/event.rs                        modifies: wiring of the three kinds; tests (Task 2)
crates/store/src/merging.rs                         creates: merging_allowed; tests (Task 2)
crates/store/src/lib.rs                             modifies: pub mod merging (Task 2)
crates/store/src/waiting.rs                         modifies: the integration row's line when merging (Task 2)
crates/runtime/src/tools/folders.rs                 modifies: the folder change row's line when merging (Task 2)
docs/schemas/rpc.schema.json                        modifies: merging.set, teamGetResult.merging_allowed (Task 3)
crates/runtime/src/daemon/team.rs                   modifies: merging.set, checked() keeps the key, team.get answer; tests (Task 3)
crates/runtime/src/forge.rs                         modifies: open state's checks, merge_methods, merge_pull_request; tests (Task 4)
crates/runtime/src/orchestrator/integrate.rs        modifies: `..` in Open patterns (Task 4, compile only); merge when checks pass, integrated_by; tests (Task 5)
apps/web/src/pages/TeamRules.tsx                    modifies: Finish gains the switch and the dialog (Task 6)
apps/web/src/strings/en.ts                          modifies: the switch, info and warning words (Task 6)
apps/web/src/pages/settings/settings.test.tsx       tests: the warning (Task 6)
docs/SPEC.md                                        modifies: 5.12, 5.14, 8.5, revision 0.128 (Task 7)
```

## Interfaces

Consumes, all on main:
- `Team`, `TeamPolicy`, `carried_team`, `describe_change` (`catervas-core`);
- `EventLog::read`, `EventQuery`, `waiting` (`catervas-store`);
- `checked`, `write_team`, `online`, `append` (`daemon/team.rs`);
- `Forge`, `Forge::run`, `PullRequestState`, `Mergeable` (`forge.rs`);
- `through_the_forge`, `stopped_merging`, `escalate`, `record_integrated`, `Subject`, `IntegrationTarget`, `Asker` (`integrate.rs`);
- `Harness`, `FakeGh::answers`, `FakeGh::calls` (`fixtures.rs`, unchanged); `integration_branch` (`crates/store/src/git.rs`);
- `Switch`, `InfoTip`, `Dialog`, `Bullets` (`packages/ui`, `apps/web`).

Produces:

```rust
// catervas-core, crates/core/src/team.rs
impl Team { pub fn merge_when_checks_pass(&self) -> bool }   // policy.merge_when_checks_pass == Some(true)

// catervas-store, crates/store/src/merging.rs
pub fn merging_allowed(log: &EventLog, team: &Team) -> Result<bool, StoreError>;
// true iff team.merge_when_checks_pass() and the newest of merge_permission.granted / .withdrawn is granted

// catervas-runtime, crates/runtime/src/forge.rs
pub enum PullRequestState {
    Open { mergeable: Mergeable, merge_state: MergeState, head: String, checks: Checks },
    Merged { sha: String },
    Closed,
}
pub enum MergeState { Clean, Behind, Blocked, Dirty, Unstable, HasHooks, Draft, Unknown } // mergeStateStatus; absent or unknown text = Unknown
pub enum Checks { None, Pending, Passed, Failed { names: Vec<String> } }                  // statusCheckRollup; absent = None
pub enum MergeMethod { Merge, Squash, Rebase }                                             // wire "merge" | "squash" | "rebase"
impl Forge {
    pub fn merge_methods(&self) -> Result<Vec<MergeMethod>, ForgeError>;  // allowed, in the order Merge, Squash, Rebase
    pub fn merge_pull_request(&self, url: &str, method: MergeMethod, head: &str) -> Result<(), ForgeError>;
}

// catervas-runtime, crates/runtime/src/orchestrator/integrate.rs
fn merge_when_checks_pass(
    tools: &ToolDeps, team: &Team, subject: &Subject, target: &IntegrationTarget,
    branch: &str, url: &str, open: &OpenPullRequest,
) -> Result<IntegrationOutcome, OrchestratorError>;
```

`OpenPullRequest` is a private struct in `integrate.rs`: `{ mergeable, merge_state, head, checks }` copied out of `PullRequestState::Open`. `Head` is `headRefOid`.

The `gh` calls:
- `gh pr view <url> --json state,mergeCommit,mergeable,mergeStateStatus,headRefOid,statusCheckRollup`
- `gh api repos/{owner}/{repo}`, with the braces literal; gh fills them in from the repository it runs in.
- `gh pr merge <url> --merge|--squash|--rebase --match-head-commit <head>`

## Tasks

### Task 1: the policy key

Files:
- modified: `docs/schemas/team.schema.json`, `crates/core/src/team.rs` (`merge_when_checks_pass`, `carried_team` sets `policy.merge_when_checks_pass = None`), `crates/core/src/team/describe.rs`
- tested in: `team.rs` `mod tests`

Produces: `Team::merge_when_checks_pass`.

The describe line, when the key changes:
- on: "Catervas merges finished work once GitHub's checks pass."
- off: "Finished work opens a pull request for you to merge."

Test, watched to fail before the key exists:
- `merging_is_off_unless_written_and_never_carried`. It asserts four things:
  - a team without the key answers `false`;
  - `merge_when_checks_pass: true` answers `true`;
  - `integration: "auto_merge"` with no key answers `false`;
  - `carried_team` of a team with `true` answers `false`.

- [x] `feat(core): add the merge when checks pass policy`

### Task 2: the permission events, the effective answer and Today's lines

Files:
- modified: `docs/schemas/event.schema.json`, `crates/protocol/src/event.rs` (every place `TeamUpdated` is wired: the kind-to-def table, the kinds list, the `EventBody` variant, `kind()`, and the actor tables), `crates/store/src/lib.rs`, `crates/store/src/waiting.rs`, `crates/runtime/src/tools/folders.rs`
- created: `crates/store/src/merging.rs`

Produces:
- `EventBody::MergePermissionGranted(MergePermissionGrantedBody)`, `EventBody::MergePermissionWithdrawn(MergePermissionWithdrawnBody)`, `EventBody::PullRequestMerged(PullRequestMergedBody)`;
- `merging_allowed`.

Today's lines while `merging_allowed` is true:
- `waiting` gives an accepted task whose `pull_request.opened` follows its acceptance the line "Accepted: Catervas merges its pull request when GitHub's checks pass".
- `integration_rows` gives an opened folder change the line "Catervas merges its pull request when GitHub's checks pass".
- Otherwise both lines are unchanged.

Tests:
- `merging_needs_this_computer_s_permission` (in `merging.rs`). This is the security guarantee. It asserts that `merging_allowed` is:
  - `false` for a key of `true` with an empty log;
  - `true` after `merge_permission.granted`;
  - `false` after a later `merge_permission.withdrawn`;
  - `false` with the grant in the log and the key absent.
- `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` (extended, `event.rs`): `EVERY_KIND` and `an_event_wire` gain the three kinds, and the test asserts each reads back as its own kind with no `task_id`.

- [x] `feat(store): merge only with this computer's permission`

### Task 3: `merging.set`, and `team.save` keeps the key

Files:
- modified: `docs/schemas/rpc.schema.json`, `crates/runtime/src/daemon/team.rs`
  - `METHODS` becomes `[&str; 20]` with `"merging.set"`;
  - its handler runs off the worker under `team_writes()`;
  - `checked()` copies `before.policy.merge_when_checks_pass`, or sets `None` when `setup`;
  - `team.get` answers `merging_allowed`.

`merging.set { on, confirmed }`:
- `on: true`:
  - without `confirmed: true`, refused with `merge_warning_unconfirmed`;
  - when `online(state, deps)` is false, refused with `merging_offline`;
  - when `merging_allowed` is already true, it answers and records nothing;
  - otherwise it writes the key `Some(true)` through `write_team` (which records `team.updated`), appends `merge_permission.granted { by: "human", into }`, and wakes the orchestrator.
- `on: false`: when the key is `None` and `merging_allowed` is false, it records nothing; otherwise it writes `None` through `write_team` and appends `merge_permission.withdrawn { by: "human" }`.
- It answers `{ merging_allowed }` read after the write.

Test, watched to fail before the method exists:
- `merging_turns_on_only_with_the_warning_confirmed` (in `daemon/team.rs` `mod tests`, on a harness with an `origin`). It asserts, in order:
  - `on: true` without `confirmed` is refused `merge_warning_unconfirmed`, and the file and the log are unchanged;
  - `team.save` of the team with `merge_when_checks_pass: true` leaves the key absent and `merging_allowed` false;
  - `on: true, confirmed: true` writes the key, appends `merge_permission.granted`, and `team.get` answers `merging_allowed: true`;
  - `on: false` removes the key and appends `merge_permission.withdrawn`.

  The offline refusal is part of the same guarantee. The same test asserts it on a second harness with no `origin` (`merging_offline`).

- [x] `feat(runtime): turn merging on only after the owner confirms the warning`

### Task 4: the forge reads the checks and merges

Files:
- modified: `crates/runtime/src/forge.rs`. `Viewed` gains `mergeStateStatus`, `headRefOid` and `statusCheckRollup`, each `#[serde(default)]`. Add `merge_methods` and `merge_pull_request`.
- tested in: `forge.rs` `mod tests`. Every existing match on `PullRequestState::Open { mergeable }` across the crate gains `..`.

Reading the rollup:
- Each item with `__typename: "CheckRun"`:
  - passes when `status` is `COMPLETED` and `conclusion` is one of `SUCCESS`, `NEUTRAL`, `SKIPPED`;
  - is pending when `status` is not `COMPLETED`;
  - fails otherwise. Its name is `name`.
- Each item with `__typename: "StatusContext"`:
  - passes on `state` `SUCCESS`;
  - is pending on `PENDING` or `EXPECTED`;
  - fails otherwise. Its name is `context`.
- Combining them:
  - any item failed gives `Failed { names }`, in rollup order;
  - otherwise, any item pending gives `Pending`;
  - otherwise, a non-empty rollup gives `Passed`;
  - an empty rollup gives `None`.

`merge_methods`:
- reads `allow_merge_commit`, `allow_squash_merge` and `allow_rebase_merge`, where a missing field counts as `true`;
- an answer that is not JSON is `ForgeError::Failed`.

Test:
- `reads_the_checks_and_merges_with_the_allowed_method`. A fake `gh` script, as the module's tests already make one, answers:
  - `view` with one passed check run, one failed status and `mergeStateStatus: CLEAN`;
  - `api` with `allow_merge_commit: false`.

  It asserts:
  - `pull_request_state` is `Open` with `Failed { names: [<the status's context>] }`, `MergeState::Clean` and the `head` given;
  - `merge_methods` is `[Squash, Rebase]`;
  - `merge_pull_request(url, Squash, "abc")` runs exactly `pr merge <url> --squash --match-head-commit abc`, with no `--admin` and no `--auto`.

- [x] `feat(runtime): read a pull request's checks and merge it through gh`

### Task 5: Catervas merges on its tick

Files:
- modified: `crates/runtime/src/orchestrator/integrate.rs`. `fixtures.rs` is unchanged: `FakeGh::answers` already keys on any second argument (`merge`, `repos/{owner}/{repo}`).

Consumes: Task 2's `merging_allowed` and `PullRequestMerged`, and Task 4's forge.

In `through_the_forge`, the `Open` arm:
- When `asker` is `Asker::Tick` and `merging_allowed(&tools.log, team)?` is true, it calls `merge_when_checks_pass`, following the design's table in its order.
- Otherwise its behaviour is today's.

The escalation details are the design's sentences, word for word. On a successful merge, `merge_when_checks_pass`:
1. appends `pull_request.merged { url, method }`, about no task;
2. reads the state again;
3. on `Merged { sha }`, goes through the same code as the `Merged` arm.

The `Merged` arm passes `TaskIntegratedBodyIntegratedBy::Governor` when the log holds a `pull_request.merged` whose `url` is this one, and `Human` otherwise. It updates `Asker`'s doc comment and `validate_team`'s comment ("Catervas merges only as ADR 0073 allows").

Tests, both `#[tokio::test] #[ignore = "needs the git program: cargo xtask check --integration"]` beside `records_a_pull_request_merged_on_the_forge`:
- `merges_a_pull_request_once_its_checks_pass`. Setup: the harness has an `origin` and a team with the key, and the log holds `merge_permission.granted`. The fake `gh` answers:
  - `view` with open, `MERGEABLE`, `CLEAN`, one passed check;
  - `api` with all methods allowed;
  - `merge` with success;
  - then `view` with merged, where `merge_on_the_forge` made the commit on the origin.

  It asserts:
  - `gh` was called with `pr merge <url> --merge --match-head-commit <head>`;
  - a second run on a fresh harness whose `view` answers an empty `statusCheckRollup` also calls `gh pr merge` (no checks configured);
  - the log holds `pull_request.merged { method: "merge" }`, then `task.integrated { integrated_by: "governor", sha: <the merge commit> }`;
  - the local integration branch's head is that commit.
- `never_merges_without_permission_or_passing_checks`. This is the security guarantee. Four ticks, each on a fresh harness, and none calls `gh pr merge`:
  1. a key of `true` with no grant;
  2. `integration: auto_merge` with a grant and no key;
  3. a grant and a failed check. The tick escalates with "A check failed on GitHub: lint. Catervas did not merge <url>; fix it or merge it yourself." and the task stays `accepted`;
  4. a grant and `mergeStateStatus: BLOCKED` with checks passed. The tick escalates with the BLOCKED sentence.

- [x] `feat(runtime): merge a pull request once its checks pass, when the owner allows it`

### Task 6: the switch and the warning

Files:
- modified:
  - `apps/web/src/pages/TeamRules.tsx`. `Finish` takes the `Part`, as `IntegrationBranch` does, plus `mergingAllowed` from `team.get`;
  - `apps/web/src/strings/en.ts`;
  - `apps/web/src/pages/settings/Work.tsx`: its `<Finish>` call passes the part and `mergingAllowed`;
- tested in: `apps/web/src/pages/settings/settings.test.tsx`.

The page:
- Below the lead line there is a `Switch` with `id="catervas-merges"`, label `t("mergingSwitch")` and `info={<Bullets text={t("mergingSwitchNote")} />}`, checked from `mergingAllowed`.
- When not `online`, the switch is disabled and `t("mergingOffline")` shows under it.
- Switching it on opens a `Dialog` with title `t("mergingWarnTitle")`, the bullets `t("mergingWarn")` (with `{branch}`) plus `t("mergingWarnSolo")` (with `{name}`) when `policy.solo`, an `InfoTip` with `t("mergingWarnNote")`, and two buttons, `t("mergingTurnOn")` and `t("mergingCancel")` ("Cancel", a key of its own as `agentCancel` and `filesCancel` are).
- **Turn on** sends `merging.set { on: true, confirmed: true }`. **Cancel** closes the dialog and sends nothing. Switching off sends `merging.set { on: false }` with no dialog.
- After each call the page refetches `team.get`.

The strings are the design's words exactly:
- `mergingSwitch`
- `mergingSwitchNote`
- `mergingOffline`
- `mergingWarnTitle`
- `mergingWarn`: the three bullets joined by `\n`, with `{branch}`
- `mergingWarnSolo`
- `mergingWarnNote`
- `mergingTurnOn`
- `mergingMainBranch`: "your main branch"
- `mergingCancel`: "Cancel"

Test:
- `asks_before_letting_catervas_merge`. Settings → How work runs, online and merging off. It asserts:
  1. clicking the switch shows a dialog named "Let Catervas merge?" holding "You won't see the work before it lands.", and no call was sent;
  2. Cancel closes it with no call;
  3. clicking again and pressing Turn on sends `merging.set` with `{ on: true, confirmed: true }`;
  4. with `mergingAllowed: true`, clicking the switch sends `{ on: false }` and shows no dialog.

Check the page at 1280 and 390 px wide against screenshots.

- [ ] `feat(web): ask before letting Catervas merge finished work`

### Task 7: the spec

Files: modified `docs/SPEC.md`.

- Revision line 0.128 (2026-10-19): ADR 0073, standalone plan `catervas-merges`, `docs/design/catervas-merges.md`.
- 5.12: the "three policies" paragraph says `integration` still reads `pull_request`. A new paragraph adds `merge_when_checks_pass`: the key, the permission, `merging.set`, its refusals, and the Settings switch and warning, quoted.
- 5.14:
  - "Catervas never merges" becomes "Catervas never merges unless the owner allows it (0.128)";
  - a new paragraph, "When Catervas merges (added in 0.128; ADR 0073)", carries the design's table, the merge, the methods, `integrated_by`, Today's lines, and the cases for member repositories, folder changes and solo;
  - the `gh pr view` fields of 0.122's paragraph gain the new ones.
- 8.5: an "Added in 0.128" line names the three kinds, their bodies, and what each is about.

- [ ] `docs(spec): record that Catervas merges when the owner allows it (0.128)`

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check --integration
# expected: xtask check: ok
```

The output goes in the pull request. The founder's own test is in the web app, on a project on GitHub with a check workflow:
1. Turn the switch on and read the warning.
2. Ask for a small change, and watch its pull request merge once the check is green.
3. Then break the check and see the escalation on Today.

## Readiness (the planner, 2026-10-19)

- **Every decision made.** The key, the gate, the RPC, the stop rules and their words, the method order, `integrated_by`, the scope (members, folder changes, solo, offline, plan tasks, private folders) and the web control are fixed in ADR 0073, the design or above. No conditional is left to the executor.
- **No ambiguity.** Every path is exact. Signatures and `gh` arguments are exact. Each test names what it asserts. The single command has its expected output.
- **No forward dependency.** Task 1 depends on main. Task 2 depends on Task 1. Task 3 depends on Tasks 1 and 2. Task 4 depends on main. Task 5 depends on Tasks 2 and 4. Task 6 depends on Task 3's wire. Task 7 depends on nothing in the code. No task touches a file another task changes for another reason, with one exception: Task 4 adds `..` to the `Open { mergeable }` patterns in `integrate.rs` so that it compiles, and Task 5 owns the file's behaviour. `fixtures.rs` is untouched.
