# Phase 3, step 16: Harness metrics

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` F17, sections 5.2, 5.4, 5.5, 5.7, 8.4, 8.5, 10
Depends on: step 15 of this phase, a start gate: Task 1 does not begin until step 15's last commit is on this branch, because Task 3 edits `crates/cli/src/lib.rs` after step 15's commands and Task 1's migration takes the number after the last migration on the branch when Task 1 starts (0007 when step 14's 0006 is the last; step 15's plan adds none). That is checked by listing `crates/store/src/migrations/` at the start gate. Step 15 writes ADR 0014, which is an ADR number and no concern of the migrations; step 14 (`human.accepted`, `escalation.resolved`, the human's moves recorded with `actor: human`, migration 0006); step 13 (`escalation.raised { integration }` on an `accepted` task, `migrations::apply_through`); step 12 (the moves into `verifying` and `rejected` the orchestrator files); step 04 (`task.transitioned`, `escalation.raised`); step 03 (`cost.recorded` with its purpose, `cost_records`); phase 2 (the projections, `ProjectFiles::read_contract`, `farik`'s `Report`, on main)
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (one round; findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`farik metrics` prints the five numbers F17 and `docs/PRODUCT_ANALYSIS.md` say to track from Milestone 0: how often a task is accepted on its first verification, how often the human had to step in per accepted task, what an accepted task cost and on which kind of session, how much of the verification was mechanical, and in how many weeks the team worked. Each is defined exactly, over the whole project, from the projections of the log and, for the criteria, the contracts. Step 18 prints them for the Milestone 0 exit. Out of scope: metrics per sprint (phase 4 step 02, which adds the sprints), the app's panel (phase 6 step 04), and any history or trend of a metric over time.

## Decisions

- An accepted task is a board row with `kind: task` and `status: accepted`. Epics are not accepted tasks. An epic is done when its tasks are, it cannot be rejected in this phase (step 14 sends a failed epic back through `escalated`), and the human accepts every one, so counting it would add a first-pass success and a denominator unit that measure nothing. What an epic cost and what the human did on it are still counted: they are the price of the tasks it produced. Cancelled tasks are never accepted tasks; their cost and interventions are still counted, like an epic's. Every rate below is `None` when there is no accepted task, because a rate with no denominator is not zero.
- First-pass acceptance rate = (accepted tasks that entered `verifying` exactly once and never entered `rejected`) / (accepted tasks). "Entered" counts `task.transitioned` events whose `to` is that status, whoever moved it. Both conditions are needed. A rejection that the human resolved straight to `accepted` has one verification and one rejection. A verification the human cut short (`verifying → escalated → in_progress → verifying`) has two and no rejection. A task the human moved from `escalated` to `accepted` without ever verifying has zero, and is not first pass. Rejected: "the first `review.recorded` passed", because a verification can end with no review (the human pulling it back, an acceptance refused and escalated), and `PRODUCT_ANALYSIS.md` asks for "the first verification". `review.recorded` stays an audit summary for the human reading the log. The step 12 and 14 plans said this step would read it; they are amended with this plan to say it does not.
- Human interventions per accepted task = (interventions on every row, of any kind and status) / (accepted tasks). An intervention is the human having to act where the process did not ask for them by design. That is one of these two events:
  - an `escalation.raised` whose reason is anything but `approval` or `risk_gate`. Those two are the `ContractRequiresHuman` gate: every epic and every `high` risk contract waits for the human on purpose (5.16, 5.2). The other eight count: `budget`, `sessions`, `iterations`, `blocker_age`, `permission`, `readiness_failures`, `integration`, and `explicit_request`.
  - a `task.transitioned` with `actor: human` whose `from` and `to` are both other than `escalated`. That covers unblocking, cancelling, a resume after a pause, and every move of step 14's `TaskTransition`. A move out of `escalated` is the answer to an escalation already counted, the approval included. A move into `escalated` is counted once, by the `explicit_request` escalation that comes with it.
  Not interventions: `question.asked` and `question.answered` (5.7: a question is not an escalation, and an epic asks by design), `human.accepted`, `request.triaged` by the human, a lock or an unlock, and `task.integrated { integrated_by: human }`, which is the `manual` policy the team chose. `PRODUCT_ANALYSIS.md` says "escalations per accepted task". This definition, unplanned interventions only, drops the two designed escalations and adds the human's unprompted moves. It is the founder's decision of 2026-09-22.
- Cost per accepted task = `CostSplit`, where `total` is the sum of every `cost.recorded` (any task, any status, and sessions with no task) divided by the accepted tasks. `by_purpose` holds the same division per purpose, with all seven wire purposes as keys in schema order (`triage`, `refine`, `plan`, `implement`, `verify`, `ceremony`, `conversation`) and `0.0` for a purpose with no cost. That keeps the output's shape stable, and the seven add up to `total`. F17 names five purposes; `triage` and `plan` arrived with 5.16 and step 11, and leaving them out would make the split miss part of the total. Everything is summed because the question is what the team's whole spend bought, and cancelled work, epics, and triage are part of that spend.
- The key of `by_purpose` is `farik_protocol::event::CostRecordedBodyPurpose`, the generated wire enum, which typify derives `Copy` and `Ord` for. The line in the project plan named `SessionPurpose`, but that type lives in `farik-runtime`, which `farik-store` cannot depend on (step 03 put `CostError` in the runtime for the same reason).
- Mechanically verified criteria share = (exit criteria whose method is `command`, `test`, or `artifact`) / (all exit criteria), over the contracts of every accepted row, epics included, each criterion counted once. `artifact` counts as mechanical because Farik runs it (step 12, ADR 0013); F17's "command or test rather than review or human" was written without naming it. Epics are included here because the number measures how contracts are written, and the Product Manager writes epics. The methods are read from the contract files with `ProjectFiles::read_contract`. The log names no criterion's method. An accepted contract is frozen and takes no write but a note (5.11), so its file is the contract it was accepted against, and 8.4 makes the files the source of truth for what a contract says. An accepted row whose contract cannot be read fails the whole call with that file's `FilesError`, rather than being left out of the count. `farik doctor` is where the human repairs it. Rejected: adding per-method counts to `contractSummary`, which touches every event that writes a summary, just for one metric.
- Active weeks = the number of distinct ISO 8601 weeks (week-numbering year and week, Monday first, UTC) that hold the `day` of at least one `cost_records` row. Every session records at least one `cost.recorded`: steps 11 and 13 record a zero-usage one for a session that reported none. So this counts the weeks in which the team ran a session. A week in which the human only ran commands is not active. The weeks are computed in Rust: `SELECT DISTINCT day FROM cost_records`, each day parsed as a `NaiveDate` and mapped through chrono's `NaiveDate::iso_week()`, not through SQLite's `%W`, which is not the ISO week. It is a count, not a streak, and it is `0` for a project with no session. Days come from step 03's UTC `day` column, so 2026-12-31 and 2027-01-01 fall in one week (2026-W53).
- Stored or computed: the per-task facts are kept as projection columns, and the rates are computed on demand from them. `task_projections` gains `verifications`, `rejections`, and `interventions` (integers, default 0), counted up by `apply_to` from `task.transitioned` and `escalation.raised`. `TaskProjection` gains them as `u32`. `Projections::metrics` is then one aggregate query over `task_projections`, one over `cost_records` (totals by purpose, and the distinct days), and one `read_contract` per accepted row. Rejected: scanning the log on every call. Section 10 says projections, not raw log scans, back every view, and F17 puts these in the app. Also rejected: a metrics table of running totals, because first pass is only known once a task is accepted, so it needs the per-task facts anyway.
- Migration `0007_metrics.sql` adds the three columns and then empties the projections, deleting every row of `task_projections`, `cost_records`, and `projection_cursor`, which are what `rebuild` clears. `open_projections` then replays the whole log on the next open, so an older project gets exact counts. Rejected: a backfill in SQL with `json_extract` over `events`. It would be a second copy of `apply_to`'s rules that can disagree with the first, and it would have to stop at the cursor or count the events after it twice. A one-time replay costs nothing that `rebuild` does not already cost.
- `farik metrics` has no `--sprint` in this phase. Sprints do not exist until phase 4, so a flag that could only refuse would be the placeholder that stage 2 forbids. Phase 4 step 02 adds it together with the sprints. clap refuses it now (exit 2). The project plan's step 16 line and phase 4 step 02 row are changed to match.
- Output. The lines are exactly those in Task 3's test. Rates are printed as a percentage with one decimal, the interventions with two decimals, and dollars as `$` with two decimals. A `None` prints as `none yet, no task has been accepted`. With `--json` the output is one object: `accepted_tasks`, `first_pass_acceptance_rate`, `interventions_per_accepted_task`, `cost_per_accepted_task_usd` (`{ total, by_purpose: { <purpose>: <usd> } }`), `mechanically_verified_criteria_share`, `active_weeks`, with `null` for `None`. The JSON is built in `crates/cli/src/metrics.rs`, the command line's one mapping layer for it, as `farik board` builds its own. `HarnessMetrics` derives no serde. `accepted_tasks` is added to `HarnessMetrics` because it is the denominator every rate shares, and a rate means little without it.
- `MetricsError { Store(StoreError), Files(FilesError) }` has a hand-written `Display` (ADR 0006): the store's or the file's own sentence.
- No ADR. The definitions are recorded in SPEC F17, which the metrics serve, and nothing but this step and step 18's report reads them.
- Changed 2026-09-23 in execution (Task 1): the counts are added up in `apply_to` inside the two existing updates, `task.transitioned`'s and `apply_waiting`'s `escalation.raised`, rather than by updates of their own. The migration deletes `projection_cursor`'s row rather than writing it back to 0, which `read_cursor` already reads as 0.
- Changed 2026-09-23 in execution (Task 2): the contract schema holds an exit criterion's id to `C<n>` (as step 12 found), so the recorded project's criteria are numbered `C1`, `C2` in the order listed whatever their method, rather than `R1` and `H1` for a review and a human one; and a `command` criterion's minimal body carries `expect: { exit_code: 0 }`, which the schema requires. `mechanically_verified_criteria_share` is also `None` when the accepted rows' contracts hold no criterion at all, which the schema's `minItems` never lets happen, rather than a division by zero.

## File map

```
crates/store/src/migrations/0007_metrics.sql   creates: the three columns; empties the projections so the next open replays the log
crates/store/src/migrations.rs                 modifies: lists 0007
crates/store/src/projections.rs                modifies: TaskProjection's three counts, apply_to, SELECT_PROJECTION; `connection` becomes pub(crate); tests, including steps 13's and 14's migration tests
crates/store/src/metrics.rs                    creates: HarnessMetrics, CostSplit, MetricsError, Projections::metrics
crates/store/src/lib.rs                        modifies: `pub mod metrics;` and its re-exports
crates/store/tests/metrics.rs                  creates: the recorded project and one test per metric (a temporary directory only; default check)
crates/cli/src/metrics.rs                      creates: `farik metrics`, its lines and its JSON
crates/cli/src/lib.rs                          modifies: the `Metrics` subcommand
crates/cli/tests/reading.rs                    modifies: the command's tests (ignored: they need git, as the file's others do)
docs/SPEC.md                                   modifies: F17, the five formulas and their edges (added in 0.8); section 3, `farik metrics` among the reading commands
docs/plans/project-plan.md                     modifies: step 16's interface line, as landed (it, the step's row, phase 4 step 02's row, and F17's coverage row were changed with this plan)
```

## Interfaces

Consumes: `Projections`, `open_projections`, `TaskProjection`, `rebuild`, `apply_to`, `cost_records` (main, step 03); `migrations::apply_through` (step 13); `ProjectFiles::read_contract`, `FilesError`, `files::fixtures::TempProject` (main); `Verification::method` (`farik-core`); `CostRecordedBodyPurpose`, `EscalationRaisedBodyReason`, `TransitionActorWire`, `TaskStatusWire` (`farik-protocol`); the events of steps 04, 12, 13, and 14; `Report`, `Project::projections` (`farik`, main).

Produces:

```rust
// farik-store::projections
pub struct TaskProjection { .., pub verifications: u32, pub rejections: u32, pub interventions: u32 }
// farik-store::metrics
#[derive(Debug, Clone, PartialEq)]
pub struct CostSplit { pub total: f64, pub by_purpose: BTreeMap<CostRecordedBodyPurpose, f64> }
#[derive(Debug, Clone, PartialEq)]
pub struct HarnessMetrics {
    pub accepted_tasks: u32,
    pub first_pass_acceptance_rate: Option<f64>,
    pub interventions_per_accepted_task: Option<f64>,
    pub cost_per_accepted_task_usd: Option<CostSplit>,
    pub mechanically_verified_criteria_share: Option<f64>,
    pub active_weeks: u32,
}
#[derive(Debug)]
pub enum MetricsError { Store(StoreError), Files(FilesError) }
impl Projections { pub fn metrics(&self, files: &ProjectFiles) -> Result<HarnessMetrics, MetricsError>; }
// farik (binary crate)
pub fn metrics(project: &Project) -> Result<Report, String>;   // crates/cli/src/metrics.rs
```

## Tasks

### Task 1: each task's verifications, rejections, and interventions

Files: `0007_metrics.sql`, `migrations.rs`, `crates/store/src/projections.rs` (tests in its `mod tests`, on a log in memory; it also holds the migration tests of steps 13 and 14)
Produces: the three columns and fields

- `counts_each_move_into_verifying_and_into_rejected`: FRK-1 moved `in_progress → verifying` (assignee), `verifying → rejected` (reviewer), `rejected → in_progress` (governor), `in_progress → verifying` (assignee), then `verifying → accepted` (product manager); none by the human. `task(FRK-1)` has `verifications == 2`, `rejections == 1`, and `interventions == 0`. A task with no move has all three at 0.
- `counts_every_escalation_but_the_two_the_process_asks_for`: FRK-1 gets one `escalation.raised` for each of the ten reasons, so `interventions == 8`. FRK-2 gets only `approval` and `risk_gate`, so 0.
- `counts_the_humans_own_moves_and_not_their_answers`: FRK-1 receives, in order, assignee `in_progress → blocked`, human `blocked → in_progress` (+1), human `in_progress → escalated` with `escalation.raised { explicit_request }` (+1, from the escalation), human `escalated → in_progress` (0), governor `in_progress → escalated`, appended with no `escalation.raised` (0), human `escalated → cancelled` (0). It also gets a `question.asked`, a `question.answered`, a `human.accepted { contract }`, a `request.triaged { triaged_by: human }`, and a `contract.locked`, none of which counts. `interventions == 2`.
- `rebuilds_the_counts_from_the_log`: after `rebuild`, FRK-1 from the first test has the same three counts, not doubled.
- `replays_an_older_project_into_the_new_counts`: a database file brought to version 6 by `apply_through(.., 6, ..)` on a `rusqlite::Connection`. Raw SQL then inserts into `events` FRK-1's `task.created`, two moves into `verifying`, an `escalation.raised { blocker_age }`, and a `cost.recorded` of 0.5 (each body the canonical JSON of the protocol fixtures), FRK-1's `task_projections` row as version 6 would hold it (`status: verifying`), its one `cost_records` row, and the `projection_cursor` row at the last seq. The connection is closed and the file opened with `open_event_log` and then `open_projections`. Opened by this build, FRK-1 has `verifications == 2`, `interventions == 1`, and `cost_usd == 0.5`, which is neither doubled nor zero.
- The existing assertion on `known_versions()` gains 7.
- Steps 13's `reads_an_older_accepted_task_as_awaiting` and 14's `reads_an_older_log_into_the_new_columns` build a version-4 or version-5 database with projection rows and then open it with `open_event_log`. That now runs 0007, which empties `task_projections`, so both would fail. Each is changed to test its own migration at its own version: it builds the older database as before, applies `apply_through(.., 5, ..)` (step 13) or `apply_through(.., 6, ..)` (step 14) on the same connection, and asserts on the column with a SQL query before any full open. Chose that over inserting the events each row came from: after 0007 a replay rebuilds those columns through `apply_to`, which the two steps' other tests already cover, and what these two tests exist for is the backfill SQL an older database still runs on its way to 0007. Neither test is skipped or deleted (rule 9).

- [x] `feat(store): count each task's verifications, rejections, and interventions`

### Task 2: the five metrics

Files: `metrics.rs`, `lib.rs`, `projections.rs` (`connection` made `pub(crate)`), `crates/store/tests/metrics.rs`, `docs/SPEC.md` (F17, section 3)
Produces: `HarnessMetrics`, `CostSplit`, `MetricsError`, `Projections::metrics`
Consumes: Task 1

The recorded project, `recorded_project()` in the test file, is a `TempProject` with a log on disk and its projections. Every contract below is written with `write_contract` from `a_contract_wire()`, with the id replaced and the exit criteria replaced by the methods listed, in the order listed, with ids `C1`, `C2` for mechanical ones and `R1` for a review, `H1` for a human one, each `satisfies: [R1]` (the fixture's requirement), and a minimal body per method as `crates/cli/tests/reading.rs` writes one: `command` `{ command: "true" }`, `test` `{ command: "cargo test" }`, `artifact` `{ path: "done.txt" }`, `review` `{ rubric: ["Is it right?"] }`, `human` `{ question: "Is it right?" }`. Its events are appended in this order per task. Each cost is a `cost.recorded` with a distinct session, recorded at 10:00 UTC on the day given. Moves not listed are the ones needed to reach the status shown, by the governor or the named agent, never by the human, and none of them into `verifying` or `rejected`.

| Row | Kind, status | Moves and events that count | Criteria | Costs (purpose, USD, day) |
|---|---|---|---|---|
| FRK-1 | epic, accepted | `readiness_failures` escalation, human `escalated → refining`; `approval` escalation, human `escalated → ready`; a `question.asked`; `human.accepted { contract }`; one move into `verifying` | command, review | triage 0.25 09-22; refine 0.5 09-22; plan 0.5 09-27 |
| FRK-2 | task under FRK-1, accepted | one move into `verifying` | command, test | implement 0.5 09-28; verify 0.25 10-06 |
| FRK-3 | task under FRK-1, accepted | two moves into `verifying`, one into `rejected` | artifact, review | implement 1.0 09-28; verify 0.5 10-06 |
| FRK-4 | task, accepted | `risk_gate` escalation, human `escalated → ready`; one move into `verifying`; after acceptance an `integration` escalation | command, human | implement 1.0 09-29; verify 0.25 10-06 |
| FRK-5 | task, cancelled | governor `blocked → escalated` with `blocker_age`; human `escalated → cancelled` | review | implement 0.5 09-29 |
| FRK-6 | task, escalated | human `blocked → in_progress`; human `in_progress → escalated` with `explicit_request` | command | none |
| FRK-7 | task under FRK-1, accepted | one move into `verifying` | command, review | refine 0.5 09-22 |
| none | | | | conversation 0.25 10-06 |

All days are in 2026. Accepted tasks: FRK-2, FRK-3, FRK-4, FRK-7. Interventions: FRK-1's readiness failures, FRK-4's integration, FRK-5's blocker age, FRK-6's move and its stop. Total cost: 6.0.

- `measures_first_pass_acceptance`: `accepted_tasks == 4` and `first_pass_acceptance_rate == Some(0.75)`. FRK-3 is the miss, and the epic FRK-1 is not in either count.
- `measures_interventions_per_accepted_task`: `interventions_per_accepted_task == Some(1.25)`, which is 5 over 4.
- `measures_cost_per_accepted_task_by_purpose`: `total == 1.5`, and `by_purpose` is exactly `triage 0.0625, refine 0.25, plan 0.125, implement 0.75, verify 0.25, ceremony 0.0, conversation 0.0625`, seven keys in that order.
- `measures_the_share_of_mechanically_verified_criteria`: `mechanically_verified_criteria_share == Some(0.6)`, which is 6 of 10 over FRK-1, 2, 3, 4, and 7. FRK-5 and FRK-6 are not counted, and counting them would give 7 of 12.
- `counts_active_weeks`: `active_weeks == 3` (2026-W39, W40, W41). 09-27, a Sunday, and 09-28, a Monday, are in different weeks.
- `says_none_for_every_rate_before_a_task_is_accepted`: FRK-1 `in_progress` with an `iterations` escalation and one cost on 2026-09-22 gives `accepted_tasks == 0`, all four `Option`s `None`, and `active_weeks == 1`.
- `does_not_count_an_acceptance_without_a_verification_as_first_pass`: FRK-1, whose contract is written with one `command` criterion C1, moved by the human `escalated → accepted` and never verified, gives `accepted_tasks == 1`, `first_pass_acceptance_rate == Some(0.0)`, and `mechanically_verified_criteria_share == Some(1.0)`.
- `counts_the_turn_of_a_year_as_one_week`: costs on 2026-12-31 and 2027-01-01 give `active_weeks == 1`. Adding one on 2027-01-04 gives 2.
- `refuses_metrics_over_an_accepted_contract_it_cannot_read`: the recorded project with FRK-2's contract file deleted gives `Err(MetricsError::Files(FilesError::NotFound { path }))` with `path` ending in `contracts/FRK-2.yaml`.

- [x] `feat(store): compute the five harness metrics`

### Task 3: `farik metrics`

Files: `crates/cli/src/metrics.rs`, `crates/cli/src/lib.rs`, `crates/cli/tests/reading.rs`, `docs/plans/project-plan.md`
Produces: `farik metrics`
Consumes: Task 2

The `accepted_project` helper in the test is `a_project_with_a_task` (FRK-1's criterion C1 is `test`). The test then opens `.farik/local/farik.db` with `open_event_log` and appends FRK-1's `in_progress → verifying` (assignee) and `verifying → accepted` (product manager), plus a `cost.recorded { implement, cost_usd: 0.5 }` for FRK-1 with session `s1` and agent `dev-a`.

- `prints_the_harness_metrics`: in `accepted_project`, `farik metrics` exits 0 and prints exactly these lines:
  ```
  accepted tasks: 1
  first-pass acceptance: 100.0%
  human interventions per accepted task: 0.00
  cost per accepted task: $0.50
    triage: $0.00
    refine: $0.00
    plan: $0.00
    implement: $0.50
    verify: $0.00
    ceremony: $0.00
    conversation: $0.00
  criteria verified by command, test, or artifact: 100.0%
  active weeks: 1
  ```
- `prints_none_before_a_task_is_accepted`: in `a_project_with_a_task`, the output is `accepted tasks: 0`, then the three lines `first-pass acceptance`, `human interventions per accepted task`, `cost per accepted task`, each followed by `: none yet, no task has been accepted`, with no purpose lines, then `criteria verified by command, test, or artifact: none yet, no task has been accepted` and `active weeks: 0`.
- `prints_the_harness_metrics_as_json`: `--json` in `accepted_project` prints one object in which `accepted_tasks` is 1, `first_pass_acceptance_rate` is 1.0, `interventions_per_accepted_task` is 0.0, `cost_per_accepted_task_usd.total` is 0.5, `by_purpose` has seven keys with `implement` 0.5 and `triage` 0.0, `mechanically_verified_criteria_share` is 1.0, and `active_weeks` is 1. In `a_project_with_a_task`, the four `Option` fields are `null` and `active_weeks` is 0.
- `has_no_sprint_flag_until_sprints_exist`: `farik metrics --sprint S1` exits 2.

- [ ] `feat(cli): print the harness metrics`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
