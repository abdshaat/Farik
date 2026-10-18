# Standalone (open issues), plan 2: no refusal parks a task in silence

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.2, 5.4, 6.8 (recorded in plan 8's revision 0.122)
Depends on: plan 1 of this folder (committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

A task in `verifying` keeps moving after an agent's refused request: its reviewer's next session
starts and is told what was refused (CTV-14, CTV-24 and CTV-25 never moved again). A task Catervas
verifies alone whose acceptance the governor refuses is escalated to the owner instead of parked
(CTV-7), and a team's documents are never a UI change. A passing contract is judged with no refusal
recorded. The reviewer of a UI change is shown the Designer's checks, so it never asks the owner to
look at a page. Out of scope: the blocking rows and the instructions (plan 1).

## Decisions

Every decision is ADR 0069's (decisions 3 and 5); this section pins how.

- **What parks a review.** Rule 5 (`verify.rs::verifying`) passes over the task only when a
  `transition.refused` since it entered `verifying` has `requested_by == GOVERNOR` ("governor").
  The same rule in `verifying_epic` (`requests.rs`) is left as it is: an epic's reviewer is the
  Product Manager, whose refusals the log has not shown.
- **What the reviewer is told.** `ReviewBrief` gains `refused: &'a [String]`: for each
  `transition.refused` since `verifying` whose `requested_by` is the reviewer's id, oldest first,
  its `details` joined by "; ". `review_message` adds, after the still-unanswered paragraph,
  "Catervas refused your last request:" and the lines in an untrusted block named `refused`, then
  "Ask for a move the first message allows, or block the task with `catervas_declare_blocked`."
  The reviewer's budgets (`max_sessions`, dollars) bound how often it is asked again; nothing else
  counts the retries.
- **The governor's own refusal escalates.** In `decide_alone`, a `TransitionOutcome::Refused` calls
  `escalate(deps, team, row, &why)` with `why` "Catervas could not accept it: {details}" (or
  "reject"), details `refusal_details(&refusal).join("; ")`. `escalate` records it as the
  `criterion_unrunnable` escalation it already raises, which the owner answers on Today. A refusal of
  that escalation leaves the task passed over, as now.
- **Documents are not UI.** `is_ui_change` drops every changed path under `docs/catervas/` before it
  matches the UI globs; a contract's `ui_change: Some(true)` still counts. The constant:
  `pub const TEAM_DOCUMENTS: &str = "docs/catervas/";` in `team_rules.rs`.
- **The governor judges `ready` first.** `requests.rs::judge` asks `refining -> ready`, and only on a
  refusal asks `refining -> escalated`. Its doc comment drops the epic's approval reason (gone with
  ADR 0061). A failing contract records the `ready` refusal, then the escalation's refusal or move,
  so `readiness_failed_attempts` counts the failure just evaluated and the third failure escalates,
  as 5.2 says.
- **The Designer's checks in the brief** (ADR 0069 decision 5). `ReviewBrief` gains
  `design_review: Option<&'a str>`: the latest `design_review.recorded` since `verifying`,
  serialised with `serde_json::to_string` (its `pass`, `reasons` and `checks`). `review_message`
  adds, before the diff, "The UI/UX Designer checked this change in the running app before you. How
  its pages look and behave is their check, not yours:" and the text in an untrusted block named
  `design_review`, capped at `RESULTS_CAP_BYTES`. With `None` the paragraph is absent. The reviewer
  gets no preview and no browser.
- No new event kind, gate or row.

## File map

```
crates/runtime/src/orchestrator/verify.rs      modifies: the park rule, refused and design_review in the brief (Tasks 5, 8); decide_alone escalates (Task 6)
crates/runtime/src/orchestrator/verify.rs      tests:    reviews_again_after_the_reviewers_request_was_refused (Task 5),
                                                         escalates_what_the_governor_would_not_accept_alone (Task 6),
                                                         checks_a_ui_change_before_the_architect amended (Task 8)
crates/runtime/src/orchestrator/messages.rs    modifies: ReviewBrief.refused (Task 5), ReviewBrief.design_review (Task 8), review_message
crates/runtime/src/orchestrator/messages.rs    tests:    shows_the_reviewer_the_designers_checks (Task 8)
crates/core/src/governor/team_rules.rs         modifies: TEAM_DOCUMENTS, is_ui_change (Task 6)
crates/core/src/governor/team_rules.rs         tests:    judges_a_ui_change_by_the_diff_or_the_field amended (Task 6)
crates/runtime/src/orchestrator/requests.rs    modifies: judge asks ready first (Task 7)
crates/runtime/src/orchestrator/requests.rs    tests:    judges_a_passing_contract_without_a_refusal (Task 7)
```

`verify.rs` and `messages.rs` are touched by Tasks 5 and 8 for one reason, the reviewer's brief;
Task 8 adds to what Task 5 left.

## Interfaces

Consumes (on this branch): `verifying`, `review`, `decide_alone`, `escalate`, `since_verifying`,
`history`, `GOVERNOR` (`verify.rs`); `ReviewBrief`, `review_message`, `untrusted_block`,
`RESULTS_CAP_BYTES` (`messages.rs`); `refusal_details` (`transitions.rs`); `is_ui_change`,
`check_protected_paths` (core); `judge` (`requests.rs`); plan 1's `CLOSING_INSTRUCTIONS` (the
Verify instruction already names the Designer's check).

Produces:

```rust
// crates/runtime/src/orchestrator/messages.rs
pub(super) struct ReviewBrief<'a> { /* … */ pub(super) refused: &'a [String], pub(super) design_review: Option<&'a str> }
// crates/core/src/governor/team_rules.rs
/// The team's own documents (ADR 0051), which are never a change to the interface.
pub const TEAM_DOCUMENTS: &str = "docs/catervas/";
```

## Tasks

### Task 5: an agent's refused request does not park a review

Files: modified `verify.rs`, `messages.rs`; tested by `verify.rs`'s tests
Produces: `ReviewBrief.refused`
Consumes: `since_verifying`, `GOVERNOR`

`verifying`'s first check filters on `matches!(&event.body, EventBody::TransitionRefused(body) if
body.requested_by == GOVERNOR)`. `review` gathers `refused` from the same history for
`reviewer.id` and passes it. Every `ReviewBrief { … }` literal gains `refused: &[]` where none
applies.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `reviews_again_after_the_reviewers_request_was_refused` (ignored "needs the git program", built
  with `a_harness` as `catervas_verifies_a_task_no_agent_reviews`'s neighbours are, for a code task
  reviewed by the Architect `arch`) — after the task enters `verifying` and Catervas ran its
  criteria, a `transition.refused` is recorded with `requested_by: "arch"`, details `["no such
  transition: from verifying to blocked"]`; a tick starts a `verify` session for `arch`, whose
  initial prompt contains "Catervas refused your last request" and "no such transition"; then, on a
  second harness, a refusal with `requested_by: "governor"` since `verifying` starts no session.
  Fails first: no session starts.

- [ ] `fix(runtime): a reviewer's refused request no longer parks the review`

### Task 6: the governor's own refusal escalates, and documents are not UI

Files: modified `verify.rs`, `crates/core/src/governor/team_rules.rs`; tested by both
Produces: `TEAM_DOCUMENTS`
Consumes: `escalate`, `refusal_details`

Tests:

- `judges_a_ui_change_by_the_diff_or_the_field` (amended) — for a Software Developer,
  `docs/catervas/engineering/bug-hunt.html` alone is not a UI change, `site/index.html` still is,
  and a contract with `ui_change: Some(true)` whose only path is under `docs/catervas/` still is.
- `escalates_what_the_governor_would_not_accept_alone` (ignored "needs the git program") — a
  contract `catervas_verifies_alone` holds for (built as `catervas_verifies_a_task_no_agent_reviews`
  builds it), assigned to a Software Developer, with `ui_change: Some(true)`, on a team with a
  UI/UX Designer: after its criteria pass, the tick records the governor's refused acceptance, then
  `escalation.raised`, and the task is `escalated` with a detail containing "Catervas could not
  accept it". Fails first: the task stays `verifying`.

- [ ] `fix(runtime): what the governor will not accept alone goes to the owner`

### Task 7: the governor asks `ready` first

Files: modified `crates/runtime/src/orchestrator/requests.rs`; tested by its tests
Consumes: `judge`

Tests:

- `judges_a_passing_contract_without_a_refusal` (ignored "needs the git program", built as
  `readies_an_epic_without_asking_the_owner` is, for a small task) — after the refine session writes a
  contract that passes, the tick moves it `refining -> ready` and the log holds no
  `transition.refused` for it. Fails first: one refusal of `refining -> escalated`.
- `escalates_a_contract_that_failed_three_times` (unchanged) still passes.

- [ ] `fix(runtime): the governor judges a contract ready before it thinks of escalating`

### Task 8: the reviewer sees the Designer's checks

Files: modified `messages.rs`, `verify.rs`; tested by both
Produces: `ReviewBrief.design_review`
Consumes: Task 5's `ReviewBrief`

`review` finds the last `EventBody::DesignReviewRecorded` since `verifying` in its history and
passes its JSON text.

Tests:

- `shows_the_reviewer_the_designers_checks` (`messages.rs`) — `review_message` with
  `design_review: Some("{\"pass\":true,\"reasons\":\"fits at 375 and 1024\",\"checks\":[]}")`
  contains "How its pages look and behave is their check" and "fits at 375 and 1024" inside an
  untrusted block named `design_review`; with `None` it contains neither. Fails first: the field does
  not exist.
- `checks_a_ui_change_before_the_architect` (amended) — the Architect's `verify` session, after the
  Designer passed, has an initial prompt containing "The UI/UX Designer checked this change", and
  still no `playwright` server and no network tier.

- [ ] `feat(runtime): the reviewer of a ui change reads the designer's checks`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok, with the git tests of verify.rs and requests.rs run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
