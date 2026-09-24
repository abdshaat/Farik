# Phase 4, step 02: Code and document branches

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 5.3, 5.11, 5.12, 5.14, 6.1 to 6.5; F15
Depends on: phase 3 (merged in #11); step 01 of this phase (committed through 04fae50, with its landing fixes)
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the default the first found undecided; its findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Only the Software Developer changes code, and a task's branch says what it changes (the founder, 2026-09-24, approving revision 11). A task assigned to any other role is held to the team's document paths before it is ready, so the Architect's design notes, the Marketing Specialist's research and marketing plan, and every change to the plan are documents that the path checks already keep where they belong. A Developer's task works on `feature/FRK-<n>` or `fix/FRK-<n>`, every other task on `docs/FRK-<n>`, where today every task works on `farik/FRK-<n>`. Out of scope: any push by an agent (Farik still pushes under the team's `integration` policy, ADR 0012), a per-role path ceiling, and the team editor that will show the rule (phase 5 step 07).

## Decisions

- `document_paths` is a team rule under `rules` in `team.yaml` (5.12): globs, each non-empty, at most 100. Its default lives in `team.schema.json` as the property's `"default": ["docs/**", "**/*.md", "CHANGELOG.md"]`, so that the generated type fills it when the key is left out and keeps an explicit `[]`, which means no task but a Developer's can be ready; the schema's description says both. Chose the schema default over mapping an empty list to the defaults, which would make `[]` impossible to say, and over adding to a shipped list as `protected_paths` does, which would make the defaults impossible to narrow. `TeamRules` (in `crates/core/src/governor/team_rules.rs`) gains `document_paths`, its `Default` giving `DEFAULT_DOCUMENT_PATHS`; `team.rs` maps the wire value as it maps the others. Chose a team rule over a role-level ceiling because the rules are what the human already writes, reads with `farik rules show`, and will edit in the team editor, and one rule serves all four document roles.
- `DocumentPathsOnly` is a structural Definition of Ready rule, placed after `AllowedPathsWithinCeiling` (`ReadinessRule` gains its 20th variant, `CHECKS` its 17th check): a contract of kind `task` whose `assignee_role` is not `software_developer` fails when any of its `allowed_paths` is outside `document_paths`, the failure naming the paths outside and the rule. An allowed path is inside when it is within a document glob by the ceiling's containment (`is_within_any`: `docs/adr/**` is within `docs/**`), or when it has no wildcard and a document glob matches it as a path (`check_allowed_paths` of that one path against `document_paths` passes: `README.md` and `notes/plan.md` are inside by `**/*.md`). A wildcard path that is not within a directory glob is outside (`notes/**`, `notes/*.md`), because it could name code. Chose extending the containment for literal paths over the ceiling's containment alone, under which `**/*.md` admits nothing. A `document_paths` glob that does not compile makes every path outside (fail closed, as 5.6 says of any glob), and the failure says which glob. A literal document glob such as `CHANGELOG.md` is a directory to the ceiling's containment, so `CHANGELOG.md/**` is inside; accepted, since it needs a directory named `CHANGELOG.md`, and a second containment rule would diverge from the ceiling's. `team.rs` maps the wire value and never reads `DEFAULT_DOCUMENT_PATHS`, which serves `TeamRules::default()` alone; the first test holds the two equal. An epic is not checked: its tasks are, each against its own assignee role. Being structural, it is reported before the Scrum Master's judgment is asked for (step 01).
- Nothing else enforces it: a document role's `write_workspace` already writes only under the contract's `allowed_paths`, and the Definition of Done refuses a diff outside them (5.4 item 2).
- Agents learn the rule where they learn every rule: `rules_json` (`farik_read_rules`, `farik rules show --json`) and the prompt's `Team rules` section (`rules_section`) carry `document_paths`; `farik rules show` labels it "document paths"; `farik doctor` reports a `document_paths` glob that does not compile, as it does the ceiling's. The Product Manager's skill `writing-task-contracts` and the Scrum Master's `keeping-work-flowing` gain one line each: a task for any role but the Developer keeps its `allowed_paths` inside the document paths, and a Developer's task says `change: fix` when it repairs a defect.
- `change` is a new optional contract field, `feature` or `fix`, in `task-contract.schema.json`; absent reads as `feature`. Chose defaulting to `feature` over requiring it, which would refuse every contract written before this step and every document task that has no use for it. It is content (`FIELDS_OF_THE_CONTENT`, 14 entries), written by whoever writes content (the Product Manager, the Scrum Master on a task, the human), frozen with the rest once the task leaves `refining`. On a task for any role but the Developer it is accepted and ignored, so that a contract whose assignee role changes while refining is not refused for a field it no longer reads.
- `task_branch(contract: &TaskContract) -> String` in `farik-core` (`crates/core/src/branch.rs`, no I/O) is the one place a branch name is made: `feature/<id>` or `fix/<id>` for `assignee_role: software_developer` by `change`, `docs/<id>` for every other. Every production site that builds `farik/<id>` calls it: `tools/git.rs` (`farik_git_diff` and `farik_git_push`; commit and status work on the worktree and name no branch), `transitions.rs` (the Definition of Done's diff), `orchestrator/rules.rs` (rule 7's worktree, the resume's commit count), `orchestrator/verify.rs` (Farik's criterion runs, the review diff), `orchestrator/integrate.rs` (merge, the pull request's head, cleanup's words), `orchestrator/messages.rs` (the implement and review messages' branch names), `cli/src/show.rs` (`--diff`). The worktree stays `.farik/local/worktrees/<id>`, the container's name is unchanged, and the pull request's body names no branch.
- Where a site has only the task's id, it reads the contract through the files as the rules already do, and a contract that cannot be read fails that tick with `OrchestratorError::Files` (or the command with its files error) as every other contract read does; there is no fallback name. The one exception is cleanup's words after a task is accepted or cancelled, which say "kept its branch" when the contract cannot be read, since the removal it reports already happened.
- The branch is derived, not recorded. Chose deriving over recording it at assignment, because `assignee_role` and `change` are frozen once the task leaves `refining` (5.11) and a task has a branch only from `assigned` on. A hand edit of a frozen contract's file, which the spec says is made only by sending the task back to `refining`, would make the name differ from the worktree's; git then refuses the unknown branch at the Definition of Done, and the refusal reaches the user as any git failure does. Not guarded further.
- No migration: a branch `farik/FRK-<n>` made before this step is not renamed or read. Chose this over renaming at startup because Farik has no users with work in flight (Milestone 0 has not run); the spec's revision note says so.
- A repository with a branch named exactly `feature`, `fix`, or `docs` cannot hold a branch under that name as a directory; git refuses the worktree at assignment, and that refusal reaches the user as any git failure does. Chose this over a fallback name, which would give one task two possible branches; renaming the user's branch fixes it.

## File map

```
docs/schemas/team.schema.json                    modifies: rules.document_paths with its default
docs/schemas/task-contract.schema.json           modifies: change (feature | fix)
crates/core/src/governor/team_rules.rs           modifies: TeamRules::document_paths, DEFAULT_DOCUMENT_PATHS, Default
crates/core/src/team.rs                          modifies: rules() maps document_paths; tests
crates/core/src/governor/readiness.rs            modifies: DocumentPathsOnly and its check; tests
crates/core/src/governor/gates.rs                modifies: FIELDS_OF_THE_CONTENT gains change
crates/core/src/contract.rs                      modifies: a test reading a contract without change
crates/core/src/branch.rs, crates/core/src/lib.rs creates / modifies: task_branch; tests
crates/store/src/requests.rs                     modifies: rules_json carries document_paths
crates/runtime/src/prompt.rs                     modifies: rules_section carries document_paths
crates/cli/src/team.rs, crates/cli/src/doctor.rs modifies: "document paths" line; the glob check
crates/runtime/src/tools/git.rs, transitions.rs  modifies: branch through task_branch
crates/runtime/src/orchestrator/{rules,verify,integrate,messages}.rs   modifies: branch through task_branch; tests
crates/cli/src/show.rs                           modifies: --diff through task_branch
crates/runtime/src/orchestrator/{fixtures,requests,recover}.rs, crates/runtime/src/orchestrator.rs,
crates/runtime/src/daemon/fixtures.rs, crates/runtime/tests/new_tests.rs   modifies: tests and fixtures name the branch task_branch gives
crates/cli/tests/reading.rs                      modifies: the rules and diff tests
crates/roles/roles/product_manager/skills/writing-task-contracts/SKILL.md, crates/roles/roles/scrum_master/skills/keeping-work-flowing/SKILL.md   modifies: one line each
docs/SPEC.md                                     modifies: revision 0.11; 5.3, 5.11, 5.12, 5.14, 6.1 to 6.5
docs/plans/project-plan.md                       modifies: step 02's interface line
```

The git adapter's tests (`crates/store/src/git.rs`, `crates/store/tests/git.rs`), the forge's (`forge.rs`), and the protocol fixture (`event/fixtures.rs`) name `farik/FRK-1` as any branch name, with no contract behind it, and are left as they are.

## Interfaces

Consumes: `TaskContract`, `Role`, `TeamRules`, `is_within_any`, `check_allowed_paths`, `ReadinessRule`, `FIELDS_OF_THE_CONTENT` (`farik-core`); `rules_json` (`farik-store`); `rules_section` (`farik-runtime`).

Produces:

```rust
// farik-core, from the schemas by import_types!: TaskContract::change: Option<Change> (feature | fix); the team wire's rules.document_paths
// farik-core::governor::team_rules
pub const DEFAULT_DOCUMENT_PATHS: [&str; 3]; // "docs/**", "**/*.md", "CHANGELOG.md"
pub struct TeamRules { /* as before */ pub document_paths: Vec<String> }
// farik-core::governor::readiness
pub enum ReadinessRule { /* as before; after AllowedPathsWithinCeiling: */ DocumentPathsOnly }
// farik-core::branch
pub fn task_branch(contract: &TaskContract) -> String;
```

## Tasks

### Task 1: the rule

Files: `team.schema.json`, `team_rules.rs`, `team.rs`, `readiness.rs`, `requests.rs` (store), `prompt.rs`, `cli/src/team.rs`, `cli/src/doctor.rs`, `reading.rs`, the two skills
Produces: `TeamRules::document_paths`, `DEFAULT_DOCUMENT_PATHS`, `ReadinessRule::DocumentPathsOnly`

- `defaults_the_document_paths_when_left_out` (team.rs) — a team file with no `rules.document_paths`: `team.rules().document_paths` equals `DEFAULT_DOCUMENT_PATHS`; one with `document_paths: ["notes/**"]` has exactly that; one with `document_paths: []` has an empty list.
- `keeps_an_architects_task_to_the_document_paths` — an `architect` task with `allowed_paths: ["src/**"]` under the defaults: refused on `DocumentPathsOnly`, the message naming `src/**`.
- `passes_an_architects_task_inside_them` — `["docs/adr/**", "README.md", "notes/plan.md"]`: no `DocumentPathsOnly` failure.
- `refuses_a_wildcard_outside_a_document_directory` — `["notes/*.md"]`: refused on `DocumentPathsOnly`.
- `leaves_a_developers_task_to_the_ceiling_alone` — a `software_developer` task with `["src/**"]`: no `DocumentPathsOnly` failure.
- `does_not_hold_an_epic_to_the_document_paths` — an epic with `["src/**"]`: no `DocumentPathsOnly` failure.
- `refuses_a_document_task_when_a_document_glob_does_not_compile` — `document_paths: ["docs/[**"]`, an `architect` task with `["docs/adr/**"]`: refused on `DocumentPathsOnly`, the message naming `docs/[**`.
- `refuses_every_document_task_with_an_empty_list` — `document_paths: []`, a `marketing_specialist` task with `["docs/marketing/**"]`: refused on `DocumentPathsOnly`.
- `writes_each_team_rule_on_its_own_line` (prompt.rs, extended) — the `Team rules` section holds `- document_paths: docs/**, **/*.md, CHANGELOG.md`.
- `shows_the_team_rules_and_the_criterion_library` (reading.rs, extended) — `farik rules show` prints `document paths: docs/**, **/*.md, CHANGELOG.md`, and `--json` has `document_paths` with the three.
- `reports_a_glob_that_does_not_compile` (reading.rs, existing parameterised test) — gains a `document_paths` case: `farik doctor` names `document_paths` and the pattern and exits 1.

If a test named here as existing has another name, the implementer uses the test that covers that output and says so in the report.

- [x] `feat(core): keep every role but the developer to the team's document paths`

### Task 2: the contract's `change` field

Files: `task-contract.schema.json`, `gates.rs`, `contract.rs`
Produces: `TaskContract::change`

- `gives_every_field_of_the_schema_to_exactly_one_owner` (gates.rs, existing) — fails once the schema has `change` and the content set does not; passes with `change` in it.
- `lets_the_product_manager_write_the_change` (gates.rs) — a write of `change` by the Product Manager on a `refining` task passes the contract-write gate; by a Developer, refused as content, as `risk` is.
- `reads_a_contract_without_a_change` (contract.rs, guard) — a contract with no `change` validates and reads `change` as `None`.

- [x] `feat(core): let a contract say whether its code change is a feature or a fix`

### Task 3: `task_branch`

Files: creates `crates/core/src/branch.rs`; modifies `crates/core/src/lib.rs`
Produces: `task_branch`

- `names_a_developers_feature_branch` — `software_developer`, `change` absent: `feature/FRK-7`.
- `names_a_developers_fix_branch` — `change: fix`: `fix/FRK-7`.
- `names_every_other_roles_branch_docs` — `architect`, `marketing_specialist`, `product_manager`, `scrum_master`, each with `change: fix`: `docs/FRK-7`.

- [x] `feat(core): name a task's branch after what it changes`

### Task 4: every branch through `task_branch`

Files: every production site and test file of the file map that names a task's branch
Consumes: `task_branch` (Task 3)

- `starts_an_assigned_task_in_its_worktree` (`#[ignore]`, git; rules.rs, changed) — the worktree is on `feature/FRK-1` for a Developer's task, and no branch `farik/FRK-1` exists.
- `starts_a_document_task_on_a_docs_branch` (`#[ignore]`, git; rules.rs) — the harness team gains an active Architect; an Architect's task assigned to it: its worktree is on `docs/FRK-1`.
- `diffs_the_tasks_branch_through_the_tool` (`#[ignore]`, git; tools/git.rs) — after a commit through the tool in a Developer's `change: fix` task, `farik_git_diff` shows it (the diff of `fix/FRK-1`, where today it asks git for `farik/FRK-1` and fails).
- `integrates_a_fix_branch` (`#[ignore]`, git; integrate.rs) — an accepted `fix` task under `auto_merge`: the merge commit's second parent is `fix/FRK-1`'s head, and cleanup's words name `fix/FRK-1`.
- `names_the_branch_in_the_implement_message` (messages.rs) — the implement message of an Architect's task names `docs/FRK-1`, and the review message's diff line names it too.
- `shows_a_tasks_diff_before_and_after_integration` (reading.rs, changed) — its Developer task's branch is `feature/FRK-<n>`.
- Every other runtime and cli test that named `farik/FRK-<n>` names the branch its contract now gets; none is deleted or skipped.

- [x] `feat(runtime): work each task on the branch its contract names`

### Task 5: the spec

Files: `docs/SPEC.md`, `docs/plans/project-plan.md`

Revision 0.11 in the header, naming each change: 5.3 gains `DocumentPathsOnly`; 5.11 names `change` among the content; 5.12's table gains `document_paths`, and its "Defaults" sentence names its default and what `[]` means; 5.14's branch is `feature/`, `fix/`, or `docs/FRK-<n>` by the contract, with no migration of `farik/` branches and git's refusal where a branch `feature`, `fix`, or `docs` exists; 6.1 to 6.5 say that only the Software Developer writes application code and every other role's tasks are documents under `document_paths`. The step's interface line in the project plan is written as landed.

- [x] `docs(docs): record code and document branches in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
grep -rnE 'farik/(\{|FRK)' --exclude=forge.rs crates/runtime/src crates/cli/src crates/runtime/tests crates/cli/tests
# expected: no output
```
