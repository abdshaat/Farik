# Standalone 2 of 8: Catervas never merges

Status: done
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.14, 5.17 (folder changes), 4.1 (setup's last step); F16
Depends on: plan 1 committed on this branch; ADR 0061
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin `auto_merge` or `manual` are changed in the task that removes them, never skipped.

## Goal

Every accepted task that has a branch, and every folder change, is pushed and opened as a pull request. The human merges it, and Catervas records the integration once the forge says it is merged.
- A project with no `origin` remote cannot open one. Its accepted work waits on Today for a merge by hand, and counts as integrated once its branch is in the integration branch.
- `auto_merge` and `manual` still load from older team files and templates, and are read as `pull_request`.
- Setup and the Team rules page no longer offer the choice.

Out of scope: what waits on integration (plan 3), and sprints (CTV-9).

## Decisions

- **Normalised at load.** The schema's enum keeps its three values, so old files validate. `validate_team` (`crates/core/src/team.rs:259`) sets `policy.integration = Integration::PullRequest` after parsing, whatever was written, and so does the template reader, `validate_template` (`crates/core/src/team/template.rs:71`), so that `team_wire` and `apply_template` copy `pull_request`.
  - This was chosen over dropping the variants, which would break every old `team.yaml` and the generated type.
  - The default written for a new team (`team/defaults.rs:36`) becomes `"pull_request"`.
  - `team/describe.rs` says "Finished work opens a pull request for you to merge." and nothing else.
  - `crates/cli/src/init.rs`'s `AUTO_MERGE_NOTICE` becomes `PULL_REQUEST_NOTICE`, "Accepted work is pushed to origin and opened as a pull request for you to merge; with no origin, merge it by hand."
- **One pull request per subject.** Each accepted task and each folder change gets its own, as `through_the_forge` already does.
  - This was chosen over batching, which would need a branch Catervas merges into, and Catervas never merges.
  - Plan 3 exempts an epic's plan task.
- **No `origin`.** `open` (`integrate.rs:702`), finding no remote named `origin` (`has_remote` false), no longer escalates "there is no remote named origin". It asks `target.git.commit_count(into, branch)` instead, on every attempt, a tick's or the human's:
  - `0` (the branch is in `into`): `record_integrated(.., TaskIntegratedBodyIntegratedBy::Human)` with `sha` = `target.git.merge_base(into, into)`, answering `Merged { sha, scan }` as a merge did (a task's scan is refreshed by `integrate_locked` as today);
  - anything else, on a tick: `IntegrationOutcome::AwaitingHand { branch, into }`, which `awaiting` and `awaiting_folder_changes` pass over with no report, as they do `AwaitingForge`;
  - anything else, for the human (`catervas integrate`, `task_integrate`, `integrate_folder_change`): the refusal `not_merged: <branch> is not in <into> yet; merge it, then ask again`.

  The same holds for a folder change. A push or forge failure escalates as today.
- **`catervas integrate` never merges.**
  - Under a pull request it reads the forge as today (`through_the_forge`, `closed_for_the_human`).
  - The `merge` path (`integrate.rs`'s `Integration::Manual` and `Integration::AutoMerge` arms, and the function it calls) goes.
  - `Git::merge` (`crates/store/src/git.rs`) and its tests in `crates/store/tests/git.rs` stay: it is public, so no lint flags it once `integrate.rs:823` no longer calls it, and plan 3's `merge_in` sits beside it.
- **Gone, as dead once the policy is one:**
  - `awaiting`'s and `awaiting_folder_changes`' `Manual` skip (`integrate.rs:52`, `:201`);
  - `rules.rs:769`'s wait under `AutoMerge`;
  - the `Manual` and `AutoMerge` sentences of `tools/folders.rs`'s `next_by`, which keeps the pull-request sentence when `origin` exists and otherwise says "<branch> waits for you to merge it into <into>.";
  - `activity.rs:582`'s check of the policy, which is now always true.
- **Today lists every merge waited on.** `crates/store/src/waiting.rs:465` lists a `WaitingKind::Integration` row for every task `awaiting_integration`, under every policy:
  - with a `pull_request.opened` since its acceptance: "Accepted: its pull request waits for you to merge it";
  - otherwise: "Accepted: waiting for you to merge <branch>".

  The `Integration` row the web already draws carries the address when there is one.

  Folder changes likewise: `tools/folders.rs`'s `integration_rows` (called by `daemon/gates.rs:253`) lists every folder change neither integrated nor escalated, under every policy, with the titles and agents it has today. Its line is "Its pull request waits for you to merge it" when a `folder_change.opened` names the change; otherwise today's two lines end "it waits for you to merge it" in place of "it waits for you to add it" ("You approved it; it waits for you to merge it", "<name> wrote it; it waits for you to merge it"). This keeps the plan's own rule that Today lists every merge waited on; removing `integration_rows`, as the draft said, would have hidden a folder change's hand merge.
- **Web.**
  - `IntegrationChoice` (`apps/web/src/pages/setup/SetupFinish.tsx`) and its use in `SetupFinish` and `TeamRules.tsx` go. Setup's last step keeps the rest; its lead loses the choice's words. `TeamRules.tsx`'s `Finish` part (`rules-finish`) keeps its title and, in place of the choice, says `rulesFinishPullRequest`, "Finished work opens a pull request for you to merge.", with no foot, having nothing to save.
  - Each per-policy branch reads as `pull_request`:
    - `TaskDetail.tsx` (around line 742);
    - `ProductPlanCard.tsx`;
    - `DocumentChanges.tsx`;
    - `FileDocument.tsx`;
    - `Today.tsx`;
    - `Gate.tsx`;
    - `files.ts`;
    - `SavedTeams.tsx`.
  - The TypeScript type keeps its three values, because old fixtures and files carry them.
  - The strings that only `auto_merge` or `manual` used are removed from `apps/web/src/strings/en.ts`.

## File map

```
crates/runtime/src/orchestrator.rs, orchestrator/integrate.rs                    pull request only, no origin (Task 1)
crates/runtime/src/orchestrator/rules.rs, tools/folders.rs, daemon/gates.rs       dead branches; folder changes' rows (Task 1)
crates/runtime/src/daemon/documents.rs, orchestrator/human.rs, orchestrator/fixtures.rs   tests under the old policies; merge_by_hand (Task 1)
crates/store/src/waiting.rs, crates/store/src/activity.rs                         Today's rows (Task 1)
crates/core/src/team.rs, team/defaults.rs, team/describe.rs, team/template.rs    normalise; default; words (Task 2)
crates/cli/src/init.rs, crates/cli/tests/commands.rs                              the notice (Task 2)
apps/web/src/pages/** (the files above), apps/web/src/strings/en.ts               (Task 3)
```

## Interfaces

Consumes (on `main`): `Integration`, `through_the_forge`, `open`, `record_integrated`, `Git::commit_count`, `Git::merge_base`, `Git::has_remote`, `WaitingKind::Integration`; harnesses of `integrate.rs`'s `mod tests` (the fake forge, `harness.accepted`).

Produces:

```rust
// catervas_runtime::orchestrator
pub enum IntegrationOutcome { /* as today */ AwaitingHand { branch: String, into: String } }   // crates/runtime/src/orchestrator.rs:316
```

## Tasks

The runtime goes first: once Task 1 lands, Catervas opens a pull request whatever the team file says, so Task 2's normalising at load changes no behaviour and no integration test. In the other order every `--integration` test on the default harness team (`auto_merge`, no `origin`) would break between the two commits.

### Task 1: open pull requests, never merge

Files: the runtime and store files of the File map. `integrate_locked` no longer matches on the policy: every subject goes `through_the_forge`, and `merge` (`integrate.rs:812`) goes. Tests that pinned merges under `auto_merge` or `manual` change to the pull-request flow, or are deleted where they asserted only the removed merge:
- `integrate.rs` from 1238 (`merges_and_pushes_under_auto_merge`) to 2097, and folder changes from 2663 to 2754;
- `transitions.rs:3523`;
- `rules.rs` 9129 and 10553;
- `rules.rs:769`'s wait goes with its tests;
- the folder-change tests of `daemon/gates.rs` (3142 to 3252), `daemon/documents.rs` (those whose harness is `project(.., "manual")` or `project(.., "auto_merge")`) and `orchestrator/human.rs` (7313 to 7534), which change to the pull-request words and flow;

Every other test whose flow relied on Catervas merging an accepted task (the runtime harness's team, `a_team_of_three`, is `auto_merge` with no `origin`) merges the branch into `main` at the root itself, with a new `Harness::merge_by_hand(&self, task: &str)` in `crates/runtime/src/orchestrator/fixtures.rs` (`git merge --no-ff <branch>` at the root, as `commit_at_root` commits there), before the tick that is to record it.

Tests are `#[ignore = "needs the git program: cargo xtask check --integration"]`.

Tests:
- `integrates_only_through_a_pull_request_or_a_hand_merge` (`integrate.rs`):
  - With `origin` (`with_origin`) and the fake forge, an accepted task under a team file saying `auto_merge` is pushed and gets a pull request (`pull_request.opened`) and no merge commit on `main`. The forge answering merged records `task.integrated { integrated_by: "human" }`.
  - With no `origin`, the tick records nothing and escalates nothing, and Today's waiting list holds "Accepted: waiting for you to merge feature/CTV-1". After `merge_by_hand("CTV-1")`, the next tick records `task.integrated { integrated_by: "human" }`.
  - `integrate` before that refuses with the message starting `not_merged: `.
  - A folder change behaves the same way, and is listed by `integration_rows` until it is integrated.

- [x] `feat(runtime): integrate only by pull request, never by a merge of Catervas's`

### Task 2: every team integrates by pull request

Files: the core and CLI files of the File map. In `team.rs`, the tests at 1437 and 1478 change to assert `PullRequest`; so do `describe.rs`'s tests at 360 to 389 (no line is said for an integration change, since every team reads the same) and `template.rs`'s at 454 (`template_from_team`'s output) and 1033 (`takes_the_four_answers`), which now read `pull_request`. `crates/cli/tests/commands.rs:254` asserts `PullRequest` and keeps its "pushed to origin" assertion, which the new notice holds.

Tests:
- `reads_every_integration_policy_as_a_pull_request` (`team.rs`): a team wire with `integration` of `auto_merge`, of `manual`, and with none, each validates with `policy.integration == Integration::PullRequest`; so does a template wire with `manual` (`validate_template`); `team::defaults`' wire holds `"pull_request"`.

- [x] `feat(core): read every integration policy as a pull request`

### Task 3: setup and settings stop offering a merge

Files: the web files of the File map; tests in `apps/web/src/pages/setup/team.test.tsx`, `settings/settings.test.tsx`, and those of the pages above that pinned per-policy words.

Tests:
- `setup_and_rules_offer_no_integration_choice` (`setup/team.test.tsx`): setup's last step has no radio group named for the integration, and the Team rules page has no such group either. With a team whose file says `auto_merge`, the Team rules page's `rules-finish` part shows "Finished work opens a pull request for you to merge." and no Save button.

- [x] `feat(web): drop the integration choice; finished work opens a pull request`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
