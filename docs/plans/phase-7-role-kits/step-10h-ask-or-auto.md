# Phase 7, step 10h: Ask or auto

Status: draft. Its readiness review runs once step 10g has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 5.12, 6.7, 6.10, 8.5; F15
Depends on: step 02 (a connector call waits on Today: `hooks.rs`'s `approval_needed`, `grant_for`); step 05b (allowances); steps 10e and 10f (the Product Manager's decision on a data pipeline, seller messages); phase 6 step 15 (the team policy `plan_in_sprints` and the team rules page, the pattern); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The founder's decision of 2026-10-05 (ADR 0041): "the user can configure whether he needs to approve everything or set it on auto mode". After this step, a team's rules hold one switch, "Ask me before anything leaves Farik" (the default) or "Let the team act on its own". On auto, a connector's outward call runs without asking, a message to a seller is sent when drafted, and the Product Manager may approve a data pipeline that costs money or whose cost is unknown, each recorded as done on auto and listed on Today under "Done on its own"; an allowance becomes a limit; and the spending limits, the daily send cap, the human's gates, purchase orders and a data pipeline that sends the project's data out are unchanged. Out of scope: a switch per connector or per tool; any change to the DevOps Engineer's deploy approvals (steps 11 and 12 honour the mode from their plans); paying, which no tool can.

## Decisions

- **One policy, `policy.approvals`, `ask` or `auto`**, in `team.schema.json` beside `plan_in_sprints`; absent means `ask`, so every existing team asks. `Team::acts_on_its_own() -> bool`. A value other than the two is a schema error at `/policy/approvals`. Rejected: per connector or per tool switches (ADR 0041).
- **Changing it.** The team rules page's "Approvals" card (mocked up first), saved through `team.save` as every rule is, and `farik rules approvals ask|auto`. The daemon records `approval_mode.changed { from, to }` when a save changes it; turning `auto` on in the browser first shows a dialog, once per change, saying what the team may then do without asking (send messages to sellers, use connectors that change things outside Farik, have the Product Manager approve data sources that cost money) and what stays (purchase orders, data sources that send the project's data out, spending limits, the daily send cap, the review and acceptance of work), with "Turn on" and "Keep asking"; at the command line the same text is printed and `--yes` is required.
- **What `auto` changes**, read at each decision point from the team file, so a change applies from the next call:
  - **A connector's `external_effect` call** (`hooks.rs`): with no grant and no allowance left to count, under `auto` it is allowed, not `approval_needed`, and its `tool.called` carries `approved_by: "auto"` (otherwise `"grant"`, `"allowance"` or absent). The Designer's plan gate (`design_plan_not_approved`) still refuses first.
  - **An allowance** (step 05b): under `auto`, a call within it runs as now; the first call beyond it is denied `allowance_reached` with "<used> of <of> <what> used; raise it to let the team do more", and no approval is asked. Under `ask` nothing changes.
  - **A seller message** (step 10f): under `auto`, `farik_draft_seller_message` sends at once after its checks, through the same send path, `seller_message.sent` carrying `sent_by: "auto"`; the 50-a-day cap refuses the rest (`seller_send_limit`), leaving them as drafts on Today.
  - **A post outside the plan** (step 08d): under `auto`, a post the Marketing Specialist writes with no slot is not left waiting for the owner as `social_post.requested`: Farik records `social_post.scheduled { approved_by: auto }` at once, in the place of the owner's allowance (`approved_by` is `plan` or `owner` until this step, and gains `auto` in `event.schema.json` and every exhaustive match), and `hand_over_posts` hands it over an hour before its time, or at once, as for any scheduled post, with the owner's Stop still there. A request whose time is within five minutes is still `missed { why: undecided }`, since nobody decided it. `auto_acts.list` gives such a post the kind `post` with its text, channel, time and pictures.
  - **A data pipeline** (step 10e; the founder's answers of 2026-10-07 to its readiness review, "Auto may approve" and "Yes, always to me"): under `auto`, `pipeline_needs_owner` gains the mode and `farik_decide_data_pipeline` takes the Product Manager's `approve` of a `paid` or `unknown` request instead of refusing it, recording `data_pipeline.approved { by: "auto" }` from the Product Manager's session (`by` gains `auto` in `event.schema.json`), which files the request in the Product Manager's name as its own approval does; one that sends the project's data out is refused `pipeline_needs_owner` in both modes, and a request the Product Manager escalates, or did not decide, waits for the owner under `auto` too.
- **What `auto` never changes:** a purchase order waits for the founder (ADR 0039, O1); the human's gates on contracts, epics and acceptance (spec 5.4, 5.16) and `human_accepts_contracts`; spending limits and session limits; `denied` tools; a data pipeline that sends the project's data out, or that the Product Manager escalates or did not decide (each waits for the owner); the DevOps Engineer's rules of spec 6.9.
- **Kit copy and skills that say a call always waits** are reworded to hold under `auto` too, in one commit after Task 2's (`feat(roles): word the kits for ask and auto`), their tests changing with them: step 07c's Product Manager GitHub setup sentence "Farik asks you before each issue or comment it posts." (`crates/roles/roles/product_manager/kit.yaml`) and its `using-product-sources` section 4 line "each call waits for the human", with every other such `why`, `setup` or skill line the re-planning finds (on 2026-10-06, the Marketing Specialist's Higgsfield, Recraft, Buffer and Kit entries).
- **"Done on its own"** on Today: the query `auto_acts.list { since? }` answers the outward acts recorded with `approved_by`, `sent_by` or `by` `auto` since `since` (default: the last 7 days), newest first, each `{ at, agent, kind, what, input }` (a connector call's tool and its whole input, a message's seller, subject and body, a pipeline's name), the input as untrusted text. The page keeps the time of the newest act seen in local storage, and shows the count since then.
- **No migration.** The acts are read from the log's existing events by kind, through the store's existing query of events by kind (`EventQuery` in `crates/store/src/event_log.rs`).

## File map

```
docs/design/mockups/ask-or-auto.*                               creates (Task 0)
docs/schemas/team.schema.json, event.schema.json, rpc.schema.json   modifies (Tasks 1, 2, 5)
crates/core/src/team.rs                                         modifies: Team::acts_on_its_own (Task 1)
crates/core/src/governor/permissions.rs                         modifies: the auto path of an external_effect call (Task 2)
crates/runtime/src/daemon/hooks.rs, crates/runtime/src/allowances.rs   modifies (Tasks 2, 3)
crates/runtime/src/tools/seller.rs, crates/runtime/src/tools/posts.rs, crates/runtime/src/tools/pipeline.rs, crates/core/src/pipeline.rs   modifies (Task 4)
crates/runtime/src/daemon/team.rs, daemon/gates.rs              modifies: approval_mode.changed, auto_acts.list (Tasks 1, 5)
crates/cli/src/lib.rs, crates/cli/src/team.rs                   modifies: `RulesCommands::Approvals`, beside `RulesCommands::Show` (Task 6)
apps/web/src/pages/TeamRules.tsx, Today.tsx, dialogs/AutoMode.tsx (+tests)   modifies/creates (Task 7)
docs/SPEC.md, docs/plans/project-plan.md                        modifies (Task 8)
```

## Interfaces

Consumes: `Team`, `policy`, `team.save` (phase 6 step 15); `evaluate_tool_call`, `grant_for`, the allowance counter (steps 02, 05b); `send` of `mailbox.rs` (10f); the pipeline decision path (10e); the store's `EventQuery` by kind.

Produces:

```rust
pub enum ApprovalMode { Ask, Auto }                       // farik_core::team
impl Team { pub fn approval_mode(&self) -> ApprovalMode; pub fn acts_on_its_own(&self) -> bool; }
pub enum ApprovedBy { Grant, Allowance, Auto }           // on tool.called, serialised snake_case
```

## Tasks

### Task 0: Mockups

The team rules' "Approvals" card, the turn-on dialog, and Today's "Done on its own", desktop and phone, approved by the founder.

- [ ] `docs(design): mock up ask or auto`

### Task 1: The policy

- `a_team_without_the_policy_asks`: `approval_mode()` is `Ask` for a team file with no `policy.approvals`. RED.
- `auto_is_read_from_the_team_file`; `a_foreign_value_is_a_schema_error_at_its_pointer`. RED each.
- `a_save_that_changes_it_records_the_change`: `approval_mode.changed { from: ask, to: auto }`, and none for a save that keeps it. RED.

- [ ] `feat(core): let a team choose to ask or act on its own`

### Task 2: A connector's outward call on auto

- `auto_runs_an_external_effect_call_without_asking`: no `tool_approval.requested`, the call allowed, `tool.called` with `approved_by: auto`. RED.
- `ask_still_waits`: the same call under `ask` is `approval_needed`. RED (guard against a default flip).
- `the_designers_plan_gate_still_refuses_first`. RED.
- `a_denied_tool_stays_denied_on_auto`. RED.

- [ ] `feat(runtime): run a connector's outward call on auto`

### Task 3: Allowances as limits on auto

- `beyond_the_allowance_auto_refuses_rather_than_asks`: the 51st SerpApi search is `allowance_reached` with the count sentence, and nothing waits on Today. RED.
- `within_the_allowance_nothing_changes`. RED.

- [ ] `feat(runtime): hold an allowance as a limit on auto`

### Task 4: Messages and pipelines on auto

- `a_drafted_message_is_sent_on_auto`: the fixture's SMTP receives it, `sent_by: auto`; the 51st of a day stays a draft. RED.
- `the_product_manager_approves_a_paid_pipeline_on_auto`: its `approve` of a `paid` request records `approved { by: auto }` and files a request; one that sends the project's data out is still `pipeline_needs_owner`, and an escalated one waits for the owner. RED.
- `a_post_outside_the_plan_is_scheduled_on_auto`: `farik_schedule_post` with no slot records `social_post.scheduled { approved_by: auto }` and no `requested`, and the next tick within the hour hands it over; under `ask` it is still `requested`. RED.
- `a_purchase_order_still_waits_on_auto`: `purchase_order.drafted` is followed by nothing until the founder decides. RED.

- [ ] `feat(runtime): send and approve on auto within the limits`

### Task 5: Done on its own

- `auto_acts_list_answers_each_kind_newest_first`, with the input as untrusted text; `since` filters. RED.

- [ ] `feat(runtime): list what the team did on its own`

### Task 6: The command line

- `rules_approvals_auto_needs_yes`: without `--yes` it prints the warning and changes nothing; with it, the team file says `auto`. RED.

- [ ] `feat(cli): set ask or auto`

### Task 7: The web app

- `the_rules_card_shows_the_mode`; `turning_auto_on_shows_the_dialog_once`; `keep_asking_changes_nothing`; `today_counts_what_was_done_on_its_own_since_last_look`. RED each.

- [ ] `feat(web): choose ask or auto, and see what was done on its own`

### Task 8: Spec and plan

`docs/SPEC.md` 5.6 (the tier table's `external_effect` row and the pre-authorization rule), 5.12 (the policy), 6.7 (allowances as limits on auto), 6.10 (messages and pipelines on auto), 8.5 (`approval_mode.changed`, `approved_by`, `sent_by`); the revision line. Project plan row 10h.

- [ ] `docs(spec): record ask or auto`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check, and the mail fixture's tests)
```

Then, by the founder: turn auto on and read the dialog; have the Procurement Specialist draft a quote request and see it sent and listed under "Done on its own"; use up a small SerpApi allowance and see the refusal; draft a purchase order and see it wait; turn auto off and see the next message wait.

## Execution notes

None yet.
