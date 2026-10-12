# Phase 9, step 02: Tasks in a member repository

Status: ready (readiness review 2026-10-12, two rounds, findings folded)
Branch: `phase/9-multi-repo-projects` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 5.3, 5.4, 5.11, 5.14, 8.4; F4
Depends on: step 01 of this phase (planned and ready, not yet executed): `catervas_core::repos` (`Repos`, `Repo`, `RepoName`, `validate_repos`), `ProjectFiles::read_repos` and `is_project_folder`, `catervas_store::repos::repo_present`. Also the amendment to ADR 0057 of 2026-10-12 (6dd2287). Nothing of steps 02b to 08.
Mockups: none; this step has no screen.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-12, round 1: not ready on B1 (an epic naming a builder role) and B2 (a builder's document tasks in the project folder), both decided below; S1–S9 and the nits folded. Round 2 (the same reviewer, confirming): ready; its nits N1–N3 folded.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at 37510b1.

## Goal

In a project folder, a builder's task names the member repository it changes, and that member holds its work:
- the task's branch and its worktree are made from the member's integration branch
- `resume` reads the member
- the Definition of Done reads the task's commits, clean tree and changed paths from the member
- `catervas_git_status`, `catervas_git_diff`, `catervas_git_commit` and `catervas_git_push` act on the member
- the task's diff on its page (`task.diff`), in `catervas show --diff`, and in the reviewer's and the designer's prompts is the member's
- the designer's preview reads the task branch's tree from the member

Readiness refuses a builder's task with no repository, a docs task or a one-repo project's task that names one, and a name that is not a member. A contract's `repo` never changes once set. Task ids stay unique across every repository's branches. A task whose member is missing waits.

A one-repo project and every docs task work exactly as today.

Out of scope, each a later step's:
- the checks a task runs, the criteria library per member, `check_new_tests` (02b)
- integration, worktree removal, dependencies' integration in another repository, `repo_in_use` (02c). Until 02c lands, an accepted task in a member escalates when it is integrated (the merge runs in the project folder, which has no such branch, `integrate.rs:699-711`), and its worktree is removed as a plain folder (`:1001-1010`). This is known and stays within the phase.
- reading members from a session (03)

## Decisions

### The contract's `repo`

- **The field.** `repo`, optional, a member's name, `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` with at most 32 characters, the pattern copied from `repos.schema.json`'s `$defs.repoName` and held equal by a test.
  - The description says it is set by the Product Manager, read only in a project folder, and fixed once set (ADR 0057).
  - `allowed_paths`, the command criterion's and the artifact criterion's paths are relative to the task's repository: the member it names, else the project root. The hook already reads them relative to the session's worktree (`crates/runtime/src/daemon/hooks.rs:863`), so only the words change (`task-contract.schema.json:215`, `:344`, `:400`; `crates/core/src/contract.rs:39`, `:58`).
  - Rejected: `repo` in `FIELDS_FIXED_AT_CREATION`. A request the human files, and a triaged one, has no repository until the Product Manager refines it.
- **Who writes it.** `repo` joins `FIELDS_OF_THE_CONTENT` (`crates/core/src/governor/gates.rs:966`, to 19), so it is written while the contract is open and frozen after, as `plan` and `lane` are.
- **Fixed once set.** Core's `check_repo_write(before, after)` refuses a write that changes a `repo` that was already set, to another name or to none, with `ContractWriteRefusal::RepoFixed { repo }`, code `repo_fixed`, sentence "repo is fixed once set: this task stays in <repo>; cancel it and file another to change repositories".
  - Writing a `repo` where there was none passes, as does writing the same name again.
  - It holds for both writers that can change a contract's content: the agents' `write_contract`, called between `validate_contract` (`tools/contracts.rs:271`) and the write (`:280`) with the read `contract` and the validated one; and the human's `contract.save`, in `judged` (`daemon/gates.rs:1113`), once the result of its `validate_contract(&after)` (about `:1150`) is bound. `hold_contract` (`requests.rs:831`) changes only `locked` and needs none.
  - `ContractWriteRefusal` has no `Display`; its sentences live in two exhaustive matches, `crates/runtime/src/tools/refusal.rs:448` (code and sentence) and `crates/store/src/requests.rs:915`, which each gain the `RepoFixed` arm.
  - It is a refusal because a contract's expanded criteria come from its repository (ADR 0057's amendment, step 02b).

### Readiness

Three rules, reported in this order after `AssigneeAvailable`. `ReadinessContext` gains `repos: Option<Vec<String>>`: the members' names, or `None` for a one-repo project. It derives no `Default`, so its two builders, `a_ready_context` (`readiness/fixtures.rs`) and `readiness_parts` (`crates/runtime/src/transitions.rs:713`), set it: the first to `None`, the second from `read_repos`. A `repos.yaml` that cannot be read is a `TransitionError::Files`. The contract's `repo` is a generated newtype (typify takes no external `$ref`), compared by `as_str()`.

**Where a task works in a project folder.** A task naming a `repo` works in that member and may change anything there, its README included. A task naming none works in the project folder, which holds documents and no code. So a builder's task with no `repo` must be a document task: every one of its `allowed_paths` is a document path by `is_a_document_path` against `rules.document_paths`, the test `document_paths_only` uses (`readiness.rs:647-675`). The Catervafication's Developer and Designer tasks are such tasks (`catervafication.rs:118-131`), and so are step 04's `repos/<name>/engineering/` documents. Only a task, never an epic, names a `repo`: an epic spans the project's repositories, whatever role it names (every filed request starts as `software_developer`, `crates/store/src/requests.rs:71`).

| Rule | Fails when | Message |
|---|---|---|
| `RepoNotAllowed` | `repo` is set and: `context.repos` is `None`; or the contract is an epic; or the assignee's role does not change code (`changes_code`, `crates/core/src/team.rs:102`) | in that order, the first that holds: "this project is one repository, so repo is left out"; "an epic spans the project's repositories, so repo is left out"; "a <role>'s task works in the project's own folder, so repo is left out" |
| `RepoRequired` | `context.repos` is `Some`, the contract is a task, `repo` is not set, the assignee's role changes code, and some of its `allowed_paths` is not a document path | this project has several repositories: set repo to the one this task changes (<names joined ", ">), or keep its allowed paths to the project's documents |
| `RepoUnknown` | `context.repos` is `Some`, `repo` is set, `RepoNotAllowed` does not fail, and `repo` is not in `context.repos` | <repo> is not one of this project's repositories: <names joined ", "> |

So a contract fails at most one of the three. When the team's document globs do not compile, `RepoRequired` stays silent and `DocumentPathsOnly` reports the broken glob, so the refusal names the real cause. A builder's document task with no `repo` gets its usual `feature/CTV-<n>` branch, in the project folder (`branch.rs:16-22`). `CHECKS` goes to 29 and `EVERY_RULE` (`plain.rs:83`) to 31. Each gets a plain sentence (`crates/core/src/governor/plain.rs:9`). The readiness totals asserted at `daemon/gates.rs:3758` and `:4359` go to 30 and 32, and the codes are `repo_not_allowed`, `repo_required` and `repo_unknown` by `rule_name` (`gates.rs:455`). `PathsWithinParent` and `LanesOverlap` still compare paths without their repositories until step 05 says how; a member task's paths are compared with its epic's as if both were the project folder's.

### The task's repository

- **One resolution point**, `catervas_store::repos::task_repository(files, team, project, repo)`, answers a `TaskRepository { repo, git, integration_branch }`:
  - `repo: None` gives the project's own `Git` (the project root) and `integration_branch(team, project)` (`crates/store/src/git.rs:1031`), today's answer.
  - `repo: Some(name)` reads `repos.yaml` and gives `Git::open(<member's path>)` and the member's `integration_branch`, else that member's `default_branch()`. The team's `policy.integration_branch` names the project folder's own branch alone (ADR 0057's amendment).
  - Its refusals are `TaskRepositoryError::NotAProjectFolder { repo }` (no `repos.yaml`), `UnknownRepo { repo }`, `Missing { repo, path }` (`!repo_present`), `Files`, and `Git`.
  - Every task-side read named in Goal goes through it. The project-folder reads stay on `deps.tools.git` and are not touched: `stale_twins`, `product_plan_on`, the epic plan's `file_at`, and the folder tools.
- **The worktree stays under the project folder**, at `.catervas/local/worktrees/CTV-<n>`, made by the member's `Git` (`git -C <member> worktree add -b <branch> <project>/.catervas/local/worktrees/CTV-<n> <member's integration branch>`). So the sandbox mount, the session's `cwd`, the hook and the helpers that compute the path (`orchestrator.rs:757`, `tools/git.rs:107`, `transitions.rs:777`) are unchanged. Its git common directory is the member's, so `require_worktree_of_this_repository` (`git.rs:835`) passes only for the member's `Git`. That is why every read named here moves together.
- **A missing member makes its tasks wait.** `assigned` (`rules.rs:1726`), finding `Missing`, returns `Ok(None)` before any transition: the task stays `assigned` with no worktree, and the next tick tries again. Every other `TaskRepositoryError` there is an `OrchestratorError::Refused` with its sentence. Rejected: an escalation per tick, which the missing member's own line on Today replaces (step 07).
- **Errors' sentences**:
  - `NotAProjectFolder`: "<id> names repository <repo>, but this project is one repository"
  - `UnknownRepo`: "<id> names repository <repo>, which this project does not have"
  - `Missing`: "<repo> is not at <path>: locate it in Settings"
  - each followed by git's or the files' own words where they are the cause

  `TransitionError` maps them to `Git { detail }` (git) and `Files { detail }` (the rest). A tool maps them to `ToolError::Failed { detail }`. In `verify.rs` (`:387`, `:481`) and `open_preview`, each is an `OrchestratorError::Refused { reason }` with its sentence.
- **`work()` runs on every transition** (`context()`, `transitions.rs:552`), so it must not block a cancel, escalation or hold. It resolves the repository only after its `worktree.is_dir()` check (`:780`). A `Missing` member reads as no worktree (`WorkState::default()`, no paths), which still refuses `verifying` truthfully; the other errors map as above.
- **The preview** (`orchestrator/session.rs:494`) reads the task branch's tree through `task_repository`, so a designer's browser on a member task opens; the preview cache, at the project folder's `.catervas/local/previews/`, is unchanged.
- **`diff_of` resolves per contract.** Its `git: &Git` becomes `files: &ProjectFiles, project: &Git`. A task's branch diff resolves its own repository; an epic's children each resolve their own, still in id order under their `# CTV-n` lines. A private folder is read from `project.root()` as today.
- **Ids.** `file` (`requests.rs:579-591`) takes the highest of the contracts, the project root's task branches, and every present member's task branches, through `task_branch_numbers_everywhere(files)`. A member that is missing, or whose branches cannot be listed, counts as having none, as a repository does today (spec 8.4). A `repos.yaml` that cannot be read leaves the project root's branches alone.

## File map

```
docs/schemas/task-contract.schema.json, crates/core/src/contract.rs   modifies: repo; the paths' wording (Task 1)
crates/core/src/governor/gates.rs                             modifies, tests: FIELDS_OF_THE_CONTENT, RepoFixed, check_repo_write (Task 1)
crates/runtime/src/tools/refusal.rs, crates/store/src/requests.rs   modifies: RepoFixed's code and sentence (Task 1)
crates/core/src/contract/fixtures.rs                          modifies: a contract with repo (Task 1)
crates/core/src/governor/readiness.rs, readiness/fixtures.rs, plain.rs   modifies, tests: the three rules (Task 2)
crates/runtime/src/daemon/gates.rs                            modifies: the totals (Task 2); check_repo_write in save, task.diff (Task 5)
crates/store/src/repos.rs, crates/store/tests/repos.rs        modifies, tests: task_repository, task_branch_numbers_everywhere (Task 3)
crates/store/src/requests.rs                                  modifies: the id counter (Task 3)
crates/runtime/src/tools/fixtures.rs, orchestrator/fixtures.rs   modifies: project-folder fixtures (Task 4)
crates/runtime/src/transitions.rs                             modifies, tests: readiness's repos, work() (Task 4)
crates/runtime/src/orchestrator/rules.rs                      modifies, tests: assigned, resume (Task 4)
crates/store/src/diff.rs                                      modifies, tests: diff_of per contract (Task 5)
crates/runtime/src/tools.rs, tools/git.rs, tools/contracts.rs modifies, tests: Call::task_repository, the git tools, check_repo_write (Task 5)
crates/runtime/src/orchestrator/verify.rs                     modifies: the design review's and reviewer's diffs (Task 5)
crates/runtime/src/orchestrator/session.rs                    modifies, tests: open_preview's tree (Task 5)
crates/cli/src/show.rs                                        modifies, tests: diff_of's arguments (Task 5)
docs/SPEC.md                                                  modifies (Task 6)
```

## Interfaces

Consumes: step 01's `read_repos`, `Repo`, `repo_present`; `check_contract_write`, `ContractWriteRefusal` (`gates.rs:1038`, `:877`); `ReadinessRule`, `ReadinessContext`, `CHECKS`, `failure` (`readiness.rs:27`, `:150`, `:185`); `plain_readiness`, `EVERY_RULE` (`plain.rs:9`, `:83`); `changes_code` (`team.rs:102`); `Git` with `create_worktree`, `commit_count`, `is_clean`, `changed_paths`, `diff`, `status`, `commit`, `push`, `merge_base`, `default_branch`, `task_branch_numbers` (`git.rs`); `integration_branch`; `task_branch` (`core/branch.rs:14`); `TestProject` (`tools/fixtures.rs:175`), `Harness` (`orchestrator/fixtures.rs:38`), `TempRepo`.

Produces:

```rust
// catervas_core
// TaskContract gains `pub repo: Option<CatervasTaskContractRepo>` (generated from the copied pattern; compared by as_str())
pub enum ContractWriteRefusal { /* gains */ RepoFixed { repo: String } }   // code repo_fixed
pub fn check_repo_write(before: &TaskContract, after: &TaskContract) -> Result<(), ContractWriteRefusal>;
pub enum ReadinessRule { /* gains, after AssigneeAvailable */ RepoNotAllowed, RepoRequired, RepoUnknown }
// ReadinessContext gains `pub repos: Option<Vec<String>>` (None in a_ready_context)
// catervas_store::repos
pub struct TaskRepository { pub repo: Option<String>, pub git: Git, pub integration_branch: String }
pub enum TaskRepositoryError { NotAProjectFolder { repo: String }, UnknownRepo { repo: String },
    Missing { repo: String, path: PathBuf }, Files(FilesError), Git(GitError) }   // Display: the Decisions' sentences
pub fn task_repository(files: &ProjectFiles, team: &Team, project: &Git, repo: Option<&str>) -> Result<TaskRepository, TaskRepositoryError>;
pub fn task_branch_numbers_everywhere(files: &ProjectFiles) -> Vec<u64>;
// catervas_store::diff
pub fn diff_of(files: &ProjectFiles, project: &Git, team: &Team, contract: &TaskContract,
    history: &[CatervasEvent], children: &[(TaskContract, Vec<CatervasEvent>)]) -> Result<TaskDiff, String>;
// catervas_runtime::tools::Call
pub(crate) fn task_repository(&self, contract: &TaskContract) -> Result<TaskRepository, ToolError>;
// test fixtures
pub(crate) fn TestProject::project_folder(name: &str, team: &Team, members: &[&str]) -> (TestProject, Vec<TempRepo>);
pub(crate) fn Harness::project_folder(name: &str, members: &[&str]) -> (Harness, Vec<TempRepo>);
pub(crate) fn Harness::project_folder_with(name: &str, members: &[&str], change: impl FnOnce(&mut Value)) -> (Harness, Vec<TempRepo>);
```

`TestProject::project_folder` makes the `TestProject` as `new` does, one `TempRepo` per name, and writes `repos.yaml` naming each by its name, at its canonicalised path, with no `integration_branch`. `Harness::project_folder` does the same for an orchestrator harness, with `Harness::new`'s team of three (pm, dev-a, dev-b). `Harness::project_folder_with` takes a change to the team's wire, as `Harness::new`'s callers make one (`with_the_suggested_six`, `with_a_scrum_master_and_an_architect`, a policy), for a test that needs other roles; `project_folder` calls it with no change. Added here, after the cross-plan pass of 2026-10-12, so that steps 02b, 02c and 04 consume it. Neither touches the members beyond `TempRepo::new`.

## Tasks

### Task 1: A contract names its repository

Files: the schema (`repo` beside `lane`, `:178`); `contract.rs` (the paths' doc comments); `gates.rs` (`FIELDS_OF_THE_CONTENT`, `RepoFixed`, `check_repo_write`); `tools/refusal.rs:448` and `requests.rs:915` (the `RepoFixed` arms); `contract/fixtures.rs`.

- `reads_a_repo`: a contract with `repo: "app"` reads and writes back unchanged. `"App"`, `"1app"` and a 33-character name are refused at `/repo`. The schema's pattern equals `repos.schema.json`'s `$defs.repoName` pattern. RED.
- `the_product_manager_writes_a_repo` (`gates.rs`, beside `the_product_manager_writes_a_plan_and_a_lane`, `:2592`): `repo` changed by the Product Manager in `refining` is allowed, and in `ready` is `ContractFrozen`. `gives_every_field_of_the_schema_to_exactly_one_owner` passes with `repo` in the content. RED: `repo` is `UnknownFields`.
- `keeps_a_repo_once_set`: `None` → `app`, `app` → `app` and `None` → `None` are `Ok`; `app` → `infra` and `app` → `None` are `RepoFixed { repo: "app" }`. Its code in `refusal.rs` is `repo_fixed` with the Decisions' sentence. RED.

- [x] `feat(core): a task names the repository it changes`

### Task 2: Readiness knows the repositories

Files: `readiness.rs` (the rules, the context field, `CHECKS`); `readiness/fixtures.rs` (`a_project_folder_context(&["app", "infra"])`); `plain.rs` (sentences, `EVERY_RULE`, the `listed` match); `daemon/gates.rs` (`:3758`, `:4359`).

- `a_one_repo_project_takes_no_repo`: a Developer's contract with `repo: app` and `repos: None` fails `RepoNotAllowed` with the one-repo message; without `repo` it passes every repo rule. RED.
- `a_builders_task_names_its_repo`: with `repos: [app, infra]` and the default document paths:
  - a Developer's task with `allowed_paths: [src/**]` and no `repo` fails `RepoRequired`, with the Decisions' message naming `app, infra`
  - a UI/UX Designer's likewise
  - `repo: web` fails `RepoUnknown` naming `web` and the two, and no other repo rule
  - `repo: infra` passes
  - a Developer's task with no `repo` and `allowed_paths: [docs/catervas/repos/app/engineering/**]` passes every repo rule

  RED.
- `a_docs_task_names_no_repo`: with `repos: [app, infra]`, an Architect's task with `repo: app` fails `RepoNotAllowed` with the third message naming the Architect, and with no `repo` passes. An epic naming `software_developer`, with no `repo`, passes every repo rule; with `repo: app` it fails `RepoNotAllowed` with the epic's message alone. An Architect's task with `repo: web` fails `RepoNotAllowed` and not `RepoUnknown`. RED.
- `plain_readiness_covers_every_rule` passes with 31. The daemon's readiness totals read 30 and 32. RED: the counts.

- [x] `feat(core): readiness asks which repository a builder's task changes`

### Task 3: The task's repository, resolved

Files: `store/src/repos.rs` (the two functions, `TaskRepository`, its error); `requests.rs` (`:579-591`); `tests/repos.rs` (integration, `#[ignore = "needs the git program: cargo xtask check --integration"]`, on `TempRepo`s and a `repos.yaml` written with step 01's `write_repos`).

- `resolves_a_tasks_repository`, on a project folder with `app` (no branch named) and `infra` (`integration_branch: release`, a branch it has), and the team's policy naming `trunk`:
  - `None` gives the project's `Git` and `trunk`
  - `app` gives `app`'s `Git` and `main`, its default branch
  - `infra` gives `release`
  - `web` is `UnknownRepo`
  - `app` once its folder is moved is `Missing` with its path
  - `app` on a one-repo project is `NotAProjectFolder`

  RED.
- `counts_task_branches_in_every_repository`: `feature/CTV-4` in `app`, `docs/CTV-2` in the project folder, `fix/CTV-9` in `infra` gives a maximum of 9. With `infra` moved away it is 4. RED.
- `files_past_every_repositorys_task_branches` (`requests.rs`): with `feature/CTV-7` in `app` alone, the next filed request is `CTV-8`. RED: it is `CTV-1`.

- [x] `feat(store): resolve a task's repository`

### Task 4: The task's work is in its member

Files: `tools/fixtures.rs`, `orchestrator/fixtures.rs` (the two fixtures); `transitions.rs` (`readiness_parts`' `repos`, `work()` through `task_repository`, the errors' mapping); `rules.rs` (`assigned` `:1750-1754`, `resume` `:1645-1648`). Tests are integration tests on `Harness::project_folder("x", &["app", "infra"])`.

- `assigns_a_task_in_its_member` (`rules.rs`): a Developer's ready task with `repo: app`, assigned and ticked, has `.catervas/local/worktrees/CTV-1` in the project folder. `git rev-parse --git-common-dir` there is `app`'s `.git`. `app` has branch `feature/CTV-1` from its `main`, and the project folder has no such branch. The task is `in_progress`. RED: the worktree is the project folder's.
- `a_docs_task_stays_in_the_project_folder` (`rules.rs`, on `Harness::project_folder_with("x", &["app", "infra"], with_the_suggested_six)`): an Architect's task with no `repo`, assigned and ticked, has its worktree on the project folder's `docs/CTV-1`. RED: guards the `None` path.
- `waits_while_its_member_is_missing` (`rules.rs`): with `app` moved away, the assigned task stays `assigned` across two ticks with no worktree and no event. Once `repos.yaml` names the new path, the next tick makes the worktree in `app`. RED.
- `reads_the_work_from_the_member` (`transitions.rs`): after a commit of `src/a.rs` in the task's worktree, the Definition of Done's evidence has `commits: 1`, `worktree_clean: true` and `changed_paths: [src/a.rs]`, and `allowed_paths: [src/**]` passes. RED: git refuses the worktree as another repository's.
- `resumes_from_the_member` (`rules.rs`): the resume of that task names the commit's summary. RED.
- `a_missing_member_blocks_no_cancel` (`transitions.rs`): with that task `in_progress` and `app` moved away, the human's cancel is recorded, and `verifying` is refused for no commits. RED: git refuses.
- `refuses_a_builders_task_with_no_repo` (`transitions.rs`): the readiness of a Developer's task with no `repo` in the project folder fails `RepoRequired`, and of one with `repo: app` passes it. RED.

- [x] `feat(runtime): work a task in its member repository`

### Task 5: The task's diff and git tools are its member's

Files: `diff.rs` (the new signature, `branch_diff` resolving per contract); `daemon/gates.rs` (`task_diff`, `:512-525`; `save`, `check_repo_write` after `:1127`); `show.rs` (`:306-319`); `verify.rs` (`:386-387`, `:480-481`); `tools.rs` (`Call::task_repository`); `tools/git.rs` (the four tools through it); `tools/contracts.rs`, `requests.rs:831`, `cli/src/contract.rs` (`check_repo_write`).

- `diffs_each_task_in_its_repository` (`diff.rs`, integration): an epic whose children `CTV-2` (`repo: app`) and `CTV-3` (`repo: infra`) are both integrated, each by the test merging its branch with `--no-ff` in its member and recording `task.integrated { sha, into: "main" }`, gives both diffs under `# CTV-2` and `# CTV-3`, each from its own member. A task in `app` before integration diffs `app`'s branch against `app`'s `main`. RED.
- `the_git_tools_act_on_the_member` (`tools/git.rs`, integration, `TestProject::project_folder` with the task's worktree made by `app`'s `Git`): `catervas_git_status` shows the new file. `catervas_git_commit` commits it to `app`'s `feature/CTV-1`. `catervas_git_diff` shows it against `app`'s `main`. `catervas_git_push` pushes to `app`'s `origin`, a bare repository the test makes. RED: git refuses the worktree.
- `task_diff_reads_the_member` (`daemon/gates.rs`): `task.diff { task_id: CTV-1 }` answers the member's diff. RED.
- `shows_the_members_diff` (`show.rs`): `catervas show CTV-1 --diff` prints the member's diff. RED.
- `previews_the_members_branch` (`orchestrator/session.rs`): the designer's browser on a member task opens its preview with the tree id `git -C <member> rev-parse feature/CTV-1^{tree}` gives. RED: the project folder has no such branch.
- `the_reviewer_sees_the_members_diff` (`verify.rs`): the reviewer's session prompt, and the UI/UX Designer's design-review prompt, carry the member's diff of the task. RED.
- `every_writer_keeps_a_repo` (`tools/contracts.rs`, and in `daemon/gates.rs` for `contract.save`): changing a set `repo` is refused `RepoFixed`, the contract file unchanged and no event recorded. Setting it the first time is written. RED.

- [ ] `feat(runtime): read a task's diff and git from its member`

### Task 6: Spec

`docs/SPEC.md`, at the next revision:
- 3, the contract's fields: `repo`, and paths relative to the task's repository
- 5.3: the three rules
- 5.11: `repo` fixed once set, by every writer
- 5.14: a task's branch and worktree from its member, under the project folder's `.catervas/local/worktrees/`; a task whose member is missing waits
- 8.4: the id counter past every repository's task branches

The task-contract schema's description changes in Task 1.

- [ ] `docs(spec): record tasks in a member repository`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container
# expected: xtask check: ok
```
