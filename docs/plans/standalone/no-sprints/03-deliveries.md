# Standalone CTV-9, plan 3: deliveries, the ceremonies' outputs without sprints

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 3, 5.8, 5.9, 5.17, 6.1, 6.2, 8.5; recorded by plan 8 (revision 0.116)
Depends on: plans 1 and 2 of this folder (committed on this branch)
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Catervas closes a delivery, as the governor, the first time since the previous one that an epic
is accepted, five standalone tasks are accepted, or the team runs out of work (ADR 0062). Closing it
appends `delivery.closed` and files the Product Manager's changelog task. Each closed delivery then
gets, once, what an ended sprint got: the review (told the open escalations' digest), the Product
Manager's and the Architect's refreshes, and the retro. Sprints still exist after this plan (plan 5
removes them) but no longer trigger a review or a retro. Out of scope: removing sprint planning,
the standup and the sprint commands (plan 5); the words of roles and skills (plan 7).

## Decisions

- The trigger, in `catervas_core::delivery` (pure): `EpicAccepted` when any accepted row is an
  epic; else `TasksAccepted` when at least `TASKS_PER_DELIVERY` (5, a constant, not a setting)
  accepted rows are tasks with no parent; else `TeamQuiet` when at least one row was accepted and
  no row is `ready`, `assigned`, `in_progress`, `verifying` or `rejected` (blocked, escalated,
  draft and refining do not hold a delivery open). Ada, section 3.
- "Accepted since" is every `task.transitioned` into `accepted` with a seq after the delivery's
  start, each task once, in seq order, leaving out the changelog tasks. Added: a changelog task
  never counts and is never in a delivery's `task_ids`; without this, accepting the changelog task
  of D1 would make the team quiet again and close D2 for it, for ever.
- The start of the next delivery is the later of the last `delivery.closed`'s seq and the seq of the
  last `sprint.ended` after which a `review` session has run (`ceremonies::has_run`), else 0 (Ada,
  section 4). It is recorded on the event as `since_seq` (added to Ada's shape): the review, the
  retro and the Architect's refresh read the events after it, as a sprint's read those after its
  `sprint.started`.
- A new delivery closes only once the previous one's retro is held, unless the team has no assigner
  (no active Scrum Master or Product Manager, so no ceremony can run): chose this over closing on
  the trigger alone, because the ceremonies read the latest delivery only (as `ended_sprint` read
  the latest sprint) and a delivery closed early would skip its predecessor's. Tasks accepted
  meanwhile wait for the next one; none is lost.
- `delivery.closed { delivery_id: "D<n>", trigger, since_seq, task_ids }`, `n` one more than the
  number of `delivery.closed` in the log; about no task (no task id on the envelope); its body names
  no actor (the governor closes it).
- The changelog task is filed in the same tick, right after `delivery.closed`, as a whole contract
  through `file_small_request` with a new `filed_as` value `changelog` (filed whole, no refine
  session, as `owner_change` is): title `Add delivery <n> to the changelog`; assignee role
  `product_manager`; reviewer role `catervas_roles::reviewer_for(team, Role::ProductManager)`;
  risk low; budget `placeholder_budget_usd(&team.rules())`; `allowed_paths: ["CHANGELOG.md"]`; its
  intent lists each task of the delivery as `CTV-n: <title>`. Since ADR 0061 a task that changes no
  code carries only criteria Catervas runs (`no_review_outside_code`), and the governor accepts it
  alone: C1 `{ "method": "artifact", "path": "CHANGELOG.md" }` and C2 `{ "method": "command",
  "command": "grep -qi 'delivery <n>' CHANGELOG.md" }`. The reviewer role is still filled, as ADR
  0061 keeps it, and never runs. It is not filed when the team has no
  active Product Manager or `reviewer_for` answers `None`; the delivery closes anyway. Correction
  to Ada's order (review, refresh, changelog, retro): filing at the close is deterministic and once
  by construction; the task's own sessions still come after the ceremonies, whose rules run before
  the numbered rules in every tick.
- The ceremonies: `review_and_retro` reads `closed_delivery` in place of `ended_sprint`, under the
  same bound, runner (`assigner`), spent-day and asleep checks, in a tick of `All` scoped to no task.
  The review is told each task's status, its cost since `since_seq` and the first line of its last
  completion note, what the delivery spent (the sum of those costs), and the digest: each open
  escalation (`escalation_line`) and each `budget.exhausted` of the day's dollars after
  `since_seq`. `due_refresh` and `product_files` take the delivery; the report is
  `docs/catervas/product/reports/D<n>` in the team's format. The Architect's refresh lists the
  delivery's tasks integrated after `since_seq`. `catervas_append_retro` answers
  `{ "delivery_id": "D<n>" }` in place of `{ "sprint_id": ... }`.
- `retro.appended` gains an optional `delivery_id` (`^D[1-9][0-9]{0,5}$`), and its `sprint_id`
  becomes optional, so old logs read and new events carry `delivery_id` alone. `folder_doc.proposed`
  is unchanged: since ADR 0061 the refreshes write `spec` and `roadmap` as folder changes and no new
  proposal is recorded; older proposals keep their `sprint_id`. `team/retro.md`'s new sections are headed `## D<n> (<date>)`. The retro tool's refusal says
  `retro_refused: no delivery has closed since the last retro`; whatever of `tools/folders.rs`'s
  review check survives ADR 0061 reads `closed_delivery` in place of `ended_sprint` and says "no
  delivery has closed".
- `folder_doc_proposals.list`'s cards now come only from proposals an older log holds; the wire is
  unchanged (its `sprint_id` is an old log's), and the page's line becomes "From {name}, after the
  team's review" (no number). Today's "What moved" lines become "{name} proposed a change to the
  product plan at the team's review." (and "{n} changes") and "{name} sent the architecture notes as a
  pull request after the team's review" (the only Architect line left once ADR 0061's plan 2 made
  every policy `pull_request`); the Architect's lines group by session, not by sprint.
- `TickReport::Delivery { delivery_id, what }` is added; `catervas run` prints `D1: <what>` as it
  prints a sprint's line. `what` for a close: `closed it: an epic was accepted` / `closed it: 5
  tasks were accepted` / `closed it: the team has no work left`, then `; filed CTV-<n> for its
  changelog` when it filed one. Ceremonies report `ran(agent, "review ceremony", end)` as today.

## File map

```
docs/schemas/event.schema.json                 modifies: delivery.closed, deliveryClosedBody; filed_as changelog; delivery_id on retroAppendedBody (Task 1)
crates/protocol/src/lib.rs, src/event.rs, src/event/fixtures.rs   modifies: the kind, its body, its fixture (Task 1)
crates/store/src/projections.rs                modifies: DeliveryClosed in the about-no-task arm (Task 1)
crates/store/src/files.rs, crates/runtime/src/tools/retro.rs  modifies: sprint_id read as Option (Task 1)
crates/core/src/delivery.rs, src/lib.rs        creates: the trigger (Task 2)
crates/runtime/src/deliveries.rs, src/lib.rs   creates: due_delivery, close_delivery, closed_delivery, delivery_start, delivery_events, changelog_tasks (Task 3; closed_delivery, delivery_events Task 4)
crates/store/src/requests.rs                   modifies: changelog_request (Task 3)
crates/runtime/src/orchestrator.rs, orchestrator/rules.rs   modifies: TickReport::Delivery, the close rule (Task 3); ceremonies per delivery (Task 4)
crates/cli/src/run.rs                          modifies: prints TickReport::Delivery (Task 3)
crates/runtime/src/orchestrator/messages.rs, src/prompt.rs   modifies: review, refresh, retro, cadence words (Task 4)
crates/runtime/src/ceremonies.rs               modifies: EndedSprint, ended_sprint, sprint_events deleted (Task 4)
crates/runtime/src/tools/retro.rs, tools/folders.rs, crates/store/src/files.rs   modifies: per delivery (Task 4)
crates/runtime/src/recorded/transcripts/review.jsonl, retro.jsonl   modifies: "S1" words become "delivery 1", the retro tool's result `{"delivery_id":"D1"}` (Task 4)
apps/web/e2e/sprints.spec.ts                   modifies: no ceremony after an ended sprint (Task 4)
crates/store/src/activity.rs                   modifies: the lines (Task 5)
apps/web/src/pages/DocumentChanges.tsx, strings/en.ts   modifies: docChangesFrom (Task 5)
```

## Interfaces

Consumes: `ceremonies::{has_run, open_escalations}`, `requests::file_small_request`,
`placeholder_budget_usd`, `catervas_roles::reviewer_for`, `rules::{assigner, day_is_spent, asleep,
ran}`, `orchestrator::session::run_session`, `messages::{escalation_line, ProductFiles, Digest}`,
`prompt::untrusted_block`, the `Harness`
fixtures (`accepted`, `moved`, `spent`, `commit_at_root`, `recorded`), `review()`, `retro()`.

Produces:

```rust
// catervas_core::delivery
pub const TASKS_PER_DELIVERY: usize = 5;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryTrigger { EpicAccepted, TasksAccepted, TeamQuiet }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedRow { pub kind: TaskKind, pub has_parent: bool }   // catervas_core::contract::TaskKind
#[must_use] pub fn delivery_trigger(accepted: &[AcceptedRow], work_left: bool) -> Option<DeliveryTrigger>;

// catervas_runtime::deliveries
pub struct DueDelivery { pub delivery_id: String, pub since_seq: u64, pub trigger: DeliveryTrigger, pub task_ids: Vec<TaskId> }
pub struct ClosedDelivery {
    pub delivery_id: String, pub since_seq: u64, pub closed_seq: u64, pub task_ids: Vec<TaskId>,
    pub reviewed: bool, pub reviewed_by: Vec<String>, pub retro_held: bool, pub retro_appended: bool,
}
pub fn delivery_start(log: &EventLog) -> Result<u64, StoreError>;
pub fn changelog_tasks(log: &EventLog) -> Result<Vec<TaskId>, StoreError>;
pub fn due_delivery(log: &EventLog, projections: &Projections, team: &Team) -> Result<Option<DueDelivery>, StoreError>;
/// Appends `delivery.closed`, then files the changelog task when the team can take it; its id.
pub fn close_delivery(deps: &ToolDeps, team: &Team, due: &DueDelivery) -> Result<Option<TaskId>, DeliveryError>;
pub fn closed_delivery(log: &EventLog) -> Result<Option<ClosedDelivery>, StoreError>;
pub fn delivery_events(log: &EventLog, delivery: &ClosedDelivery, task_id: &TaskId) -> Result<Vec<CatervasEvent>, StoreError>;
pub enum DeliveryError { Store(StoreError), Request(RequestError) }   // Display, From

// catervas_store::requests
#[must_use] pub fn changelog_request(delivery_id: &str, tasks: &[(TaskId, String)], reviewer: Role, max_cost_usd: f64) -> Value;
```

`TickReport::Delivery { delivery_id: String, what: String }`. `SprintTask` in messages.rs is renamed
`DeliveredTask` (same fields).

## Tasks

### Task 1: the wire

Files: the schema and protocol files of the map; `projections.rs` treats `delivery.closed` as about
no task; every reader and writer of the now-optional `sprint_id` compiles with `Some(...)` where it
writes today, behaviour unchanged. The new kind follows `catervafication.started`'s precedent in
`event.rs` (`body_def_name`, `is_about_one_contract` false, `attribution` none, the kinds array,
`EventBody`, `kind()`), and `fixtures.rs` gains its body wire `{ "delivery_id": "D1", "trigger":
"team_quiet", "since_seq": 0, "task_ids": ["CTV-1"] }`.

- `reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back` (existing, in
  `crates/protocol/src/event.rs`) — watched to fail once the fixture names `delivery.closed`, before
  the schema and the enum have it; then passes for it.

- [x] `feat(protocol): the delivery.closed event and changelog filings`

### Task 2: when a delivery closes

Files: created `crates/core/src/delivery.rs` (`mod tests`); `crates/core/src/lib.rs` (`pub mod delivery`).

- `closes_on_an_epic_five_tasks_or_a_quiet_team` — `delivery_trigger(&[], false)` is `None`; one
  accepted standalone task with work left is `None` and with none left `TeamQuiet`; four standalone
  tasks and a task with a parent, work left, is `None`; five standalone tasks, work left,
  `TasksAccepted`; one epic among any rows, work left, `EpicAccepted`.

- [x] `feat(core): what closes a delivery`

### Task 3: closing a delivery, and its changelog task

Files: created `crates/runtime/src/deliveries.rs` (`due_delivery`, `close_delivery`,
`delivery_start`, `changelog_tasks`); modified `crates/store/src/requests.rs` (`changelog_request`,
its contract as the Decisions say, requirement R1 "CHANGELOG.md has a section for delivery <n> at
its top saying, in the user's words, what each of its tasks changed", and only the two criteria of
the Decisions, C1 `artifact` on `CHANGELOG.md` and C2 the `command` `grep -qi 'delivery <n>'
CHANGELOG.md`, each `satisfies: ["R1"]`; no `review` criterion, which `no_review_outside_code`
refuses),
`crates/runtime/src/orchestrator.rs` (`TickReport::Delivery`; the loop treats it as `Sprint`),
`crates/runtime/src/orchestrator/rules.rs` (a rule `close_a_delivery`, first in `sprint_rules`,
only under `All` scoped to no task, returning `TickReport::Delivery`), `crates/cli/src/run.rs`.

- `closes_a_delivery_when_the_team_runs_quiet_and_files_its_changelog_once` (rules.rs `mod tests`,
  `#[ignore = "needs the git program: cargo xtask check --integration"]`, `Harness` with a Scrum
  Master; CTV-1 accepted) — the first tick answers `TickReport::Delivery { delivery_id: "D1", .. }`;
  the log holds one `delivery.closed { delivery_id: "D1", trigger: "team_quiet", since_seq: 0,
  task_ids: ["CTV-1"] }` and one `task.created` with `filed_as: "changelog"` and `created_by:
  "catervas"`, whose contract has `allowed_paths: ["CHANGELOG.md"]` and `assignee_role:
  "product_manager"`; the changelog task then moved to `accepted` (`harness.moved`), three more
  ticks append no second `delivery.closed` (the adapter holds `review()` and `retro()`, which D1's
  review and retro play in those ticks; a recorded adapter with no transcript left fails the tick).

Then run `cargo xtask check --integration` and fix each orchestrator, CLI or browser-journey
(`apps/web/e2e/`) test whose expectations the new rule changes (a quiet board after an acceptance now closes a delivery and files a task): extend
its expected events or sessions, or end its loop earlier; never drop an assertion about anything
else. Name each changed test in the commit body.

- [x] `feat(runtime): close a delivery and file its changelog task`

### Task 4: the ceremonies run per delivery

Files: `rules.rs` (`review_and_retro`, `due_refresh`, `product_files`, `review_refresh`,
`delivery_tasks` in place of `sprint_tasks`), `deliveries.rs` (`closed_delivery`, `delivery_events`),
`ceremonies.rs` (`EndedSprint`, `ended_sprint`, `sprint_events` deleted), `messages.rs`
(`sprint_review_message` becomes `delivery_review_message(delivery_id, tasks, spent_usd, digest:
&Digest, refresh)`; `product_refresh_message`, `architecture_refresh_message`, `retro_message`,
`cadence_line` say "delivery <n>" and "the delivery report" in place of the sprint's),
`prompt.rs` (`CEREMONY_INSTRUCTIONS`' review: "This session is the review of the work the team just
delivered. Post what it delivered and what it did not, from the first message, with
`catervas_post_message`, in at most three messages." and retro: "This session is the retro of the
work the team just delivered. Post what to keep and what to change with `catervas_post_message`, in
at most three messages, then record what the team should remember next time with
`catervas_append_retro`, and end the session." each keeping today's closing sentence),
`tools/retro.rs`, `tools/folders.rs`, `files.rs` (`append_retro(heading_id, ...)`), and the two
transcripts. `apps/web/e2e/sprints.spec.ts` loses its `review` and `retro` transcripts and its
step 7 wait for one `retro.appended` (an ended sprint holds no ceremony now, and nothing is accepted
in that journey); `board.spec.ts` keeps its assertions (its review and retro now come from D1).
Every test that used `a_finished_sprint` or an ended sprint for a review, a refresh or
a retro uses a closed delivery instead (its first tick closes D1); delete
`holds_no_review_once_a_new_sprint_started` and
`holds_no_review_or_retro_of_a_sprint_that_held_no_task`, whose behaviour is gone.

- `a_delivery_runs_its_review_refreshes_and_retro_once` (rules.rs, ignored as above; a Scrum Master,
  a Product Manager and an Architect; `docs/catervas/product/spec.html` and
  `docs/catervas/architecture/structure.html` committed on main; epic CTV-1 and its task CTV-2
  accepted, CTV-2 integrated; CTV-3, with no parent, escalated with reason `iterations`; the
  recorded transcripts the refresh tests use today; the changelog task CTV-4 that the first tick
  files is moved to `cancelled` with `harness.moved` right after it, so no session is about it) —
  the first tick closes D1 with trigger `epic_accepted`; then, in the next four ticks, in this order, one `review`-thread session of the Scrum Master whose first message holds "delivery 1",
  "CTV-2" and an escalation line for CTV-3, one `review` session of the Product Manager naming
  `docs/catervas/product/reports/D1.html`, one of the Architect naming CTV-2, and one `retro`-thread
  session; the log holds one `retro.appended` with `delivery_id: "D1"`; four more ticks start no
  session.

- [ ] `feat(runtime): each delivery gets its review, refreshes and retro once`

### Task 5: what the owner reads about a review

Files: `activity.rs` (the lines of the Decisions), `DocumentChanges.tsx` (`docChangesFrom` = "From
{name}, after the team's review"), `en.ts`; the tests that pin the old words
(`says_who_proposed_changes_to_the_product_plan`, `says_the_architecture_notes_were_brought_up_to_date`,
`DocumentChanges.test.tsx`'s `shows_the_changes_line_by_line`) take the new ones.

- `says_what_a_review_did_without_naming_a_sprint` (activity.rs `mod tests`) — two older
  `folder_doc.proposed` of Ada's with `sprint_id: "S4"` after `since`, and an Architect `review`
  session after a `delivery.closed` that wrote a folder document: `moved_since` holds "Ada proposed
  2 changes to the product plan at the team's review." and "<the Architect> sent the
  architecture notes as a pull request after the team's review", and no line contains "sprint".

- [ ] `feat(web): a review's proposals and notes say nothing of sprints`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
