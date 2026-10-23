# Standalone: keep everything on this computer

Status: ready
Branch: `feat/keep-everything-local` (its own pull request to `main`)
Spec: `docs/SPEC.md` 5.6, 5.14, 8.5 (revision 0.155); ADR 0085
Depends on: `main` at `d6599bd7`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22, against the three readiness rules (ADR 0065)

## Goal

Settings, Allowed actions, and setup's permission step gain a third answer to "May the Developer send its work in progress online?": "No, keep everything on this computer". With it, nothing goes online: no agent or Catervas pushes, no pull request is opened, read or merged, nothing is fetched, and accepted work is merged by Catervas into the project folder's integration branch on this computer. A team that answered "Yes" or "No" is unchanged. Out of scope: connectors (none opens a pull request), a team with no git at all.

## Decisions

- Where the choice lives: `policy.integration: local`, over a new `permissions` field, because integration is what changes and `push` already means agents' own branches. ADR 0085 decision 1.
- The third answer writes `push: false` and `integration: local`; "Yes" and "No" write `integration: pull_request`. Over keeping `integration` untouched by "Yes"/"No": a team that leaves local must get its pull requests back.
- Templates: `validate_template` keeps reading every integration as `pull_request` (`template.rs:110`), over carrying `local` into a template, because a template is shared and the answer is this computer's.
- Under `local`, `Agent::tiers` removes `git_remote` after the agent's own grants, over refusing a team file with `push: true`, so a hand-edited file still loads and is still held.
- The gate in integration is one check at the top of `through_the_forge`: under `local` and with no pull request opened since acceptance, `land_here` runs instead of `open`; with one opened, the forge path runs as now (ADR 0085 decision 5).
- `land_here` requires the target's checkout to be on `into` (`Git::current_branch`), else escalates with ADR 0085 decision 3's words; it never switches the owner's checkout. Over `Git::merge`'s checkout-and-restore, which would move the owner's folder under them.
- A conflict: a new event `task.conflicted { into, paths }` over making `pull_request.conflicted`'s `url` optional, because readers of that event link the url. The task moves to `assigned` with `TransitionAsk.pull_request_conflicts`, as `stopped_merging` does, its worktree holding the merge in progress (`merge_keeping_conflicts`). A folder change escalates.
- `bring_up_to_date` (`rules.rs:1836`) skips its fetch under `local`.
- `catervas_exec`: under `local`, a git segment whose subcommand is `push` or `send-pack` is refused `kept_local: this team keeps everything on this computer`, before the owner's `git_subcommands` are consulted.
- `team.get`'s `online` is false under `local` (`daemon/team.rs::online`), which refuses the merge switch already; the web reads `policy.integration` for the finish line.
- `folders.rs:207`'s `next_by` reads "online" as `has_remote("origin") && integration != local`.

## File map

```
docs/schemas/team.schema.json, team-template.schema.json   modifies: `local` in policy.integration's enum
docs/schemas/events/ (task.conflicted)                     creates: the event's body, as its siblings are
crates/core/src/team.rs                                    modifies: Integration::Local read as itself; tiers() drops git_remote
crates/core/src/team/describe.rs                           modifies: "Everything stays on this computer; Catervas adds finished work to your project folder."
crates/core/src/governor/permissions.rs                    modifies: pushing git subcommands refused under local
crates/runtime/src/orchestrator/integrate.rs               modifies: land_here, the gate in through_the_forge
crates/runtime/src/orchestrator/rules.rs                   modifies: no fetch under local
crates/runtime/src/tools/folders.rs, tools/exec (caller)   modifies: next_by; pass `local` to the exec check
crates/runtime/src/daemon/team.rs                          modifies: online() false under local
apps/web/src/pages/setup/SetupPermissions.tsx              modifies: third answer, reading both fields
apps/web/src/pages/TeamRules.tsx, setup/SetupFinish.tsx    modifies: the local finish line; merge switch hidden
apps/web/src/strings/en.ts                                 modifies: strings below
apps/web/e2e/setup-team.spec.ts                            modifies: the permission step's labels if they move
```

Tests live where their siblings do: `team.rs`'s `mod tests`, `integrate.rs`'s `mod tests` (its harness has a fake forge and an `origin` remote), the exec check's tests, `TeamRules`/`team.test.tsx`.

## Interfaces

Consumes: `Git::merge_what_is_checked_out` via `Git::merge` (`catervas-store`; call it only with the checkout already on `into`), `Git::merge_keeping_conflicts`, `Git::add_worktree`, `record_integrated`, `escalate`, `TransitionAsk.pull_request_conflicts`, `copy_in` (all in `integrate.rs` or on main).

Produces:

```rust
pub enum Integration { PullRequest, AutoMerge, Manual, Local }       // the wire enum, generated
impl Team { pub fn keeps_local(&self) -> bool; }                      // policy.integration == Local
fn land_here(tools: &ToolDeps, team: &Team, subject: &Subject, target: &IntegrationTarget,
             branch: &str) -> Result<IntegrationOutcome, OrchestratorError>;
EventBody::TaskConflicted(TaskConflictedBody { into: String, paths: Vec<String> })  // kind task.conflicted
```

## Tasks

### Task 1: the policy and the tiers

Files: the two schemas, `team.rs`, `describe.rs`, `permissions.rs`.

- `a_local_team_keeps_everything_on_this_computer` (core) — asserts that a team wire with `integration: local` reads back as `Integration::Local` and writes `"local"`; that with `push: true` and a Developer's own grant of `git_remote`, no agent's `tiers()` holds `git_remote`; that the exec git check refuses `git push origin x` and `git send-pack` with `kept_local` under local even when `git_subcommands` lists `push`, and runs `git push` listed so when not local; and that `describe` says the local line on the change.

- [x] `feat(core): a team may keep everything on this computer`

### Task 2: accepted work lands here

Files: `integrate.rs`, `rules.rs`, the `task.conflicted` event (schema, `EventBody`, projection passes it through).

- `a_local_team_lands_finished_work_on_this_computer` (runtime) — with an `origin` remote and the fake forge, under `local`: an accepted task's tick records `task.integrated { integrated_by: governor }`, its branch is in `into` at the root, `origin`'s refs are unchanged and the forge saw no call; a folder change lands the same way; with the root checked out on another branch the task is escalated with the decision's words and `into` is unchanged; a task with a pull request opened before the switch still goes through the forge.
- `a_local_conflict_goes_back_to_the_builder` (runtime) — two tasks changing the same line, the first landed: the second records `task.conflicted { into, paths }`, moves to `assigned`, and its worktree has the merge in progress; nothing is pushed.

- [x] `feat(runtime): a local team's finished work lands on this computer`

### Task 3: nothing else goes online

Files: `rules.rs` (fetch), `folders.rs`, `daemon/team.rs`, the exec call site.

- `a_local_team_never_reaches_origin` (runtime) — under `local`, `team.get` answers `online: false` with an `origin`; `bring_up_to_date` does not fetch (a fake `origin` with a new commit on `into` is not merged in); `catervas_exec` with `git push` listed in `rules.git_subcommands` is refused `kept_local`; `catervas_git_push` is refused for a Developer of a `push: true` team.

- [x] `feat(runtime): a local team never reaches its remote`

### Task 4: the third answer

Files: `SetupPermissions.tsx`, `TeamRules.tsx`, `SetupFinish.tsx`, `en.ts`, the e2e spec.

Strings:
- `mayPushLocal`: "No, keep everything on this computer". `mayPushLocalNote`: "Nothing goes online. Catervas adds finished work to your project folder itself."
- `mayPushNoNote`: "Finished work still opens a pull request on GitHub." (was "The safer choice.")
- `mayPushNoteOne`/`Many`/`Solo`: the last line becomes "With \"No\", Catervas still opens a pull request for finished work. Choose \"Keep everything on this computer\" for nothing to go online."
- `finishLeadKept`: "Everything stays on this computer. Catervas adds finished work to your project folder itself." Shown in setup's finish and on Allowed actions when `integration` is `local`, over `finishLead`/`finishLeadLocal`; the merge switch is not shown then.

`Answer` gains `"local"`; the shown value is `local` when `policy.integration === "local"`, else `word(push)`; choosing writes both fields as Decisions says. The mapping layer carries `integration` already.

- `the push question keeps everything local` (web, `team.test.tsx`) — on Allowed actions, choosing the third answer saves `permissions.push: false` and `integration: "local"`, shows `finishLeadKept` and no merge switch; choosing "No" afterwards saves `integration: "pull_request"`.

- [x] `feat(web): the push question can keep everything local`

### Task 5: spec

The planner wrote 5.6, 5.14, 8.5 and the header (0.155) in this branch's first commit; the executor corrects them if the built behaviour differs.

## Verification

```
CARGO_BUILD_BUILD_DIR=$PWD/target cargo xtask check
# expected: xtask check: ok
```

Then the founder's own run: in the live team, Settings, Allowed actions, choose the third answer, let a task finish, and see it in the project folder with no new pull request on GitHub.
