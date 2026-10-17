# Standalone CTV-9, plan 1: the decision, and escalation choices that say what they do

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 5.7 (escalation), F12; recorded by plan 8 (revision 0.116)
Depends on: `origin/main` at `74b4a5af`, rebased onto `fix/plans-need-no-review` (ADR 0061)
Readiness confirmed by: not yet. The Opus 5.5 readiness review of 2026-10-16 found one unmade decision: what gives a `budget` or `sessions` escalation room (see plan 1's Decisions, the `budget`, `sessions` row)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

ADR 0062 records that deliveries replace sprints (the whole CTV-9 design, so later plans link it
rather than restate it). Every escalation a user can answer offers named choices, each saying in
plain words what happens next and naming the agent, exactly one marked as the team's suggestion,
no two sending the same command, for all eleven reasons of `escalation.raised`. The web app and
the command line read the same list. `catervas resolve` takes `--extra-tries <n>`, and the end of
a run prints, for each open escalation, one runnable command per choice. Out of scope: the web
view (plan 2), changing any escalation reason or limit.

## Decisions

- ADR 0062 holds the design of CTV-9 (Ada's plan, sections 3 to 6, with the corrections below and in
  plans 3, 5 and 6): chose one ADR over one per plan because every plan rests on the same choice.
  Its header line `Supersedes ADR 0028.` is what C4's search finds. ADR 0028 gets one line under
  its status, `Superseded by ADR 0062 (2026-10-16).`, and nothing else in it changes;
  `docs/design/sprint-backlog.md` gets one line under its title, `Superseded by ADR 0062: sprints
  and the Backlog left the product.`
- The choices are built once, in a new module `catervas_runtime::choices`, typed, and turned into the
  wire by the daemon and into command lines by the CLI: chose a shared typed list over the JSON
  `choices` of `daemon/gates.rs` that the CLI cannot read (the controller: nothing builds the list
  twice). `daemon::gates::choices` is deleted.
- The table of choices, suggested first (labels and next words exact; `{who}` is the display name of the task's
  `assignee_id` on the board at the moment `escalation.choices` is read (or the run ends), else "The
  team"; the words are composed then, never stored, so after plan 6 hands a task to another agent
  they name whoever holds it; `{pm}` the active Product Manager's, else "The Product Manager";
  `{before}` the status the task left for `escalated`, in `catervas_store::activity::status_in_words`
  lower-cased):

  | Reason | Choice: label, action, next |
  |---|---|
  | `iterations` | "Give 2 more tries", resolve `in_progress` `extra_tries` 2, "{who} picks up {id} again with 2 more tries." · "Ask {pm} to change the plan", resolve `refining`, "{pm} rewrites the plan of {id}; it is assigned again once it is ready." · Cancel |
  | `budget`, `sessions` | UNMADE (readiness review, 2026-10-16): "Ask {pm} to change the plan" (as above) · Cancel. A resolve to `refining` while the dollars or sessions are still spent is escalated again by the budget rule on the next `All` tick, before rule 9 gives the Product Manager a session (SPEC 5.7; `rules.rs` `escalates_again_a_task_resumed_without_room`, `escalates_a_refining_contract_out_of_sessions`), so these next words would not come true. The founder or the planner decides what gives the task room before this row is executed |
  | `blocker_age` | "Answer and carry on", resolve `in_progress`, "{who} reads your answer and carries on with {id}." · change · Cancel |
  | `permission`, `explicit_request` | "Carry on", resolve `{before}`, "{id} goes back to {before} and {who} is told what you wrote." (only when `before` is known and is neither `refining` nor `escalated`) · change · Cancel |
  | `readiness_failures` | "Ask {pm} to try the plan again", resolve `refining`, "{pm} writes the plan of {id} again with your note." · Cancel |
  | `approval`, `risk_gate` (only in logs made before ADR 0061, which no longer raises them) | "Review the plan", open `/tasks/{id}/plan`, "You read the plan of {id} and approve it or send it back." · Cancel |
  | `integration` (a push or a forge failure; ADR 0061: Catervas never merges) | "Open the pull request again", command `task_integrate`, "Catervas pushes the branch of {id} and opens its pull request again, for you to merge." (alone) |
  | `preview` | "Fix how your app opens", open `/settings/your-app`, "You tell Catervas in Settings how to start your app; then come back to {id} and choose Carry on." · "Carry on", resolve `{before}`, "{id} goes back to {before} and {who} opens your app again." · Cancel |
  | any other | change · Cancel |

  Cancel is "Cancel the task", resolve `cancelled`, "{id} stops for good. Its branch is kept, and
  nothing is merged." The first choice of each row is the suggested one. Corrections to Ada's
  table: `blocker_age`'s carry-on went back to `blocked` (today's `before`), which helps nobody, so
  it resolves to `in_progress`; `readiness_failures` offered "Carry on" and "Change the plan", both
  to `refining`, so it keeps one; an accepted task cannot be cancelled (nothing leaves `accepted`,
  5.2), and since ADR 0061 Catervas never merges, so `integration` offers one choice, to try the
  pull request again (Ada's "Add it to the project now" and its merge are gone); `preview` needs a resolve after Settings, so it adds
  Carry on.
- After the rebase onto ADR 0061 these reasons still arise: `iterations`, `budget`, `sessions`,
  `blocker_age`, `permission`, `readiness_failures` (now also for a `review` criterion outside code,
  `no_review_outside_code`), `integration` (a failed push or pull request), `explicit_request`,
  `preview`. `approval` and `risk_gate` are kept for escalations an older log left open, which
  `human_accept` still resolves (ADR 0061 keeps it for them).
- `escalation.choices` answers the integration choice for an accepted task whose integration
  escalation is open (`ceremonies::open_escalations` holds it), and for an escalated task as today;
  anything else answers `{ "choices": [] }`: chose this over a second query because the row on Today
  is the same "Waiting on you" row.
- Wire, additive: each choice gains `next` (string), `suggested` (boolean) and `kind` (`command` or
  `open`), all required; `body` becomes optional (present for `command`), and `path` (string, pattern
  `^/`) is added for `open`. The page adds the message to `body.body` of an `escalation_resolve`
  choice only, as today; `task_integrate` takes `taskIdBody`, which holds no `message` (readiness fix).
- `catervas resolve` gains `--extra-tries <n>` (1 to 5, clap's range), passed as `extra_tries`,
  before the message words. The end of a run prints, under each escalated task's line, one line per
  choice: two spaces, the command, two spaces, `# `, the next words, and ` (suggested)` after the
  suggested one's. Commands: `catervas resolve {id} {to} [--extra-tries {n}] "<your message>"`,
  `catervas integrate {id}`, and for an open choice `in the web app: {path}`. JSON gains
  `"choices": [{ "command", "next", "suggested" }]` on the item. Approval and integration rows keep
  their own lines (they are `WaitingKind::Approval` and the integration kind, not `Help`).

## File map

```
docs/decisions/0062-deliveries-replace-sprints.md     creates: the decision (Task 1)
docs/decisions/0028-plan-work-in-sprints.md           modifies: one status line (Task 1)
docs/design/sprint-backlog.md                         modifies: one line under the title (Task 1)
crates/runtime/src/choices.rs                         creates: Offer, Choice, ChoiceAction, escalation_choices, offer_for; mod tests (Task 2)
crates/runtime/src/lib.rs                             modifies: pub mod choices (Task 2)
crates/runtime/src/daemon/gates.rs                    modifies: escalation_choices uses the module; choices() and its two tests deleted (Task 2)
docs/schemas/rpc.schema.json                          modifies: escalationChoicesResult (Task 2)
crates/cli/src/lib.rs                                 modifies: Resolve gains extra_tries (Task 3)
crates/cli/src/waiting.rs                             modifies: Help rows carry their choices (Task 3)
crates/cli/tests/human.rs                             tests: the end-of-run lines (Task 3)
```

## Interfaces

Consumes (on `main`): `ceremonies::open_escalations`, `transitions::last_move_into` (the one
`daemon/gates.rs` uses), `name_of` (`catervas_store::waiting`), `status_in_words`
(`catervas_store::activity`), `EventBody::EscalationRaised`, `TaskStatus`, `Team`, `Role`.

Produces:

```rust
// catervas_runtime::choices
pub struct Offer<'a> {
    pub task_id: &'a str, pub reason: &'a str, pub before: Option<TaskStatus>,
    pub who: &'a str, pub product_manager: &'a str,
}
pub enum ChoiceAction { Resolve { to: TaskStatus, extra_tries: Option<u8> }, Integrate, Open { path: String } }
pub struct Choice { pub label: String, pub next: String, pub suggested: bool, pub action: ChoiceAction }
impl Choice {
    /// `{label, next, suggested, kind, body?, path?}` as `escalationChoicesResult` holds one.
    #[must_use] pub fn wire(&self, task_id: &str) -> serde_json::Value;
    /// The command line that makes this choice, as the end of a run prints it.
    #[must_use] pub fn command_line(&self, task_id: &str) -> String;
}
/// The choices of the table in plan 1, suggested first.
#[must_use] pub fn escalation_choices(offer: &Offer<'_>) -> Vec<Choice>;
/// What the task's open escalation offers: its reason, `before`, and the names, read from the log
/// and the board; `None` when nothing is open for the task.
pub fn offer_for(log: &EventLog, projections: &Projections, team: &Team, task_id: &TaskId)
    -> Result<Option<Vec<Choice>>, StoreError>;
```

## Tasks

### Task 1: the decision

Files: created `docs/decisions/0062-deliveries-replace-sprints.md` from `docs/decisions/0000-template.md`;
modified `docs/decisions/0028-plan-work-in-sprints.md` (the status line only),
`docs/design/sprint-backlog.md` (one line). No code, no test.

The ADR: title `# 0062. Deliveries replace sprints`; `Date: 2026-10-16`; `Status: accepted (the
founder's approval of the epic CTV-9, question 34: "Remove sprints from the product entirely")`;
then the line `Supersedes ADR 0028.`, the line `Builds on ADR 0061 (only code is reviewed, only
payments ask, Catervas never merges), which lands first.`, and `Amends ADR 0037 (allowances count per UTC day), ADR 0042
(the raise's exception to the sprint hold has nothing left to except) and ADR 0027 (the incident
fix's exception likewise; a deploy is no longer approved by a sprint's start, which phase 13 re-plans).`
Context: the four frictions of CTV-9's intent, and the options Ada ruled out (a timer, a delivery per
accepted task, a "close delivery" button). Decision, as bullets written out in the ADR itself (the
plans are where they come from, not what the ADR links to): sprints, sprint planning, the standup,
sprint budgets, the Backlog hold and `plan_in_sprints` leave the product; the delivery, its three
triggers (`TASKS_PER_DELIVERY = 5`, not a setting), its start, what each runs once and in which
order, and the changelog task (from plan 3's Decisions); the compatibility rules (from plan 5's);
the WIP, resume and hand-over rules (from plan 6's); the escalation choices (this plan's table). Consequences: easier (no ceremony to
start; a ready task goes to the first free agent), harder (no batching of work for a review by the
user's choice; a delivery's review costs a session after every epic or five tasks; the founder's
milestone runs, phase 11 step 03, lose "one sprint carrying two requests" and are re-planned there;
phase 13's DevOps plans lose "a sprint's start approves its deploys"; the founder deferred how a
deploy is approved to phase 13's planning, 2026-10-16).

- [ ] `docs(decisions): deliveries replace sprints (ADR 0062)`

### Task 2: one list of choices, read by the page

Files: created `crates/runtime/src/choices.rs`; modified `crates/runtime/src/lib.rs`,
`crates/runtime/src/daemon/gates.rs` (`escalation_choices` calls `choices::offer_for` and answers
`{ "choices": [choice.wire(id), ...] }`, or `[]`; the old `choices` fn and the tests
`offers_the_choices_for_each_reason` and `offers_to_carry_on_where_the_task_was` are deleted, as
the new test replaces them), `docs/schemas/rpc.schema.json` (`escalationChoicesResult` per the
Decisions). Consumes Task 1's ADR only as text.

- `every_reason_offers_one_suggested_choice_that_says_what_happens` (in `choices.rs`, `mod tests`,
  pure) — for each of the eleven reasons of the event schema and for an unknown one, with
  `before: Some(InProgress)`, `who: "Theo"`, `product_manager: "Mira"`, task `CTV-14`:
  `escalation_choices` is non-empty; exactly one choice is suggested and it is the first; every
  `next` is non-empty and contains "CTV-14"; no two choices have equal `action`s; and the
  `iterations` list equals exactly, label and next, "Give 2 more tries" / "Theo picks up CTV-14
  again with 2 more tries." (resolve `in_progress`, 2 tries), "Ask Mira to change the plan" /
  "Mira rewrites the plan of CTV-14; it is assigned again once it is ready." (resolve
  `refining`), "Cancel the task" / "CTV-14 stops for good. Its branch is kept, and nothing is
  merged." (resolve `cancelled`); `blocker_age`'s first resolves to `in_progress`; `integration`
  has one choice, `Integrate`, labelled "Open the pull request again"; `approval` and `preview` start with an `Open` of `/tasks/CTV-14/plan`
  and `/settings/your-app`; `permission` with `before: Some(Refining)` has no "Carry on". Each
  `wire` of the lists conforms to `escalationChoicesResult`'s item.
- `answers_the_choice_of_an_integration_waiting_on_an_accepted_task` (in `daemon/gates.rs`
  `mod tests`, `#[ignore = "needs the git program: cargo xtask check --integration"]`, on the
  `driven` harness) — a task moved to `accepted` with an `escalation.raised { reason: integration }`
  after the move: `escalation.choices` answers one choice whose `body` is
  `{ "command": "task_integrate", "body": { "task_id": <id> } }`, `kind` `command`, `suggested`
  true; after a `task.integrated` it answers `{ "choices": [] }`; the reply conforms to
  `escalationChoicesResult`. In the same test, the `next` words are composed when read: a second task CTV-2,
  escalated with reason `iterations` while held by `dev-a`, answers a first choice whose `next` names
  dev-a's display name; after a `task.transitioned` the harness appends gives CTV-2 the assignee
  `dev-b`, the same query names dev-b's.

Existing web tests that read `escalation.choices` fixtures (`apps/web/src/test/gate.ts`,
`HelpNeeded.test.tsx`) keep passing: their fixtures lack the new fields, which the page does not
read yet.

- [ ] `feat(runtime): every escalation says what each choice does and suggests one`

### Task 3: the command line answers each choice

Files: modified `crates/cli/src/lib.rs` (`Commands::Resolve` gains
`#[arg(long, value_parser = clap::value_parser!(u8).range(1..=5))] extra_tries: Option<u8>`,
passed to `Command::EscalationResolve { extra_tries }` in `humans()`), `crates/cli/src/waiting.rs`
(`Waiting` gains `choices: Vec<(String, String, bool)>`, filled for `WaitingKind::Help` from
`choices::offer_for`; `lines()` prints them after the row's line per the Decisions; `json()` adds
`choices`; the Help row's own command becomes `choose one`, so its line reads `CTV-1 is escalated (iterations): choose one`), tested by `crates/cli/tests/human.rs`.
Consumes Task 2's `offer_for` and `Choice::command_line`.

- `the_end_of_a_run_names_a_command_for_each_choice` (`#[ignore = "needs the git program: cargo
  xtask check --integration"]`, on `a_team` with the shared helpers `filed`, `record`, `moved`, as
  `cancels_and_resolves_for_the_human` sets up an escalation) — a task escalated from `in_progress`
  (moved through `assigned` with an `assignee` in `moved`'s `extra`, so the resolve back to
  `in_progress` passes its gate) with `escalation.raised { reason: iterations }`: `catervas run` exits 0 and its output holds, in
  order, `CTV-1 is escalated (iterations): choose one`, a line `  catervas resolve CTV-1
  in_progress --extra-tries 2 "<your message>"  # ` followed by the next words and ` (suggested)`,
  then the refining and the cancelled lines without `(suggested)`; `catervas --json run`'s
  `waiting_on_you[0].choices` has three entries, the first `suggested: true`; running the first
  printed command with `<your message>` replaced by `Go on.` exits 0 and records
  `escalation.resolved { to: in_progress, extra_tries: 2, message: "Go on." }`.

- [ ] `feat(cli): resolve takes more tries, and a run ends with a command for each choice`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
grep -rl 'Supersedes.*0028\|supersedes.*0028\|0028.*superseded' docs/decisions/ | grep -v '0028-plan-work-in-sprints.md' | head -n 1 | grep -q . && echo SUPERSEDED
# expected: SUPERSEDED   (epic C4)
```
