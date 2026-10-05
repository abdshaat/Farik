# Phase 7, step 10c: Purchase requests and renewals

Status: draft. Its readiness review runs once step 10b has landed and the founder has answered O1 of `docs/design/procurement-specialist.md`.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.7, 6.10, 8.5; F9
Depends on: step 10b of this phase (the role and its folder); step 09 (`calamine`, the sheet reader); step 02 (the human-only decision path of `tool_approve`, `waiting.list`, Today's dialog pattern); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Procurement Specialist can ask the human to buy something, and the human can say they bought it, with what they paid, or that they will not, on Today or at the command line; the agent reads the outcome in its next task. Once a day, with no model, Farik reads the register's renewal dates and puts each renewal whose decision date is two weeks off or nearer on Today, where the human asks for a review or dismisses it. Out of scope: Farik buying anything (ADR 0039: never); Farik writing the register (the agent does); reminders by email or notification (phase 9 step 03).

## Decisions

- **A purchase request is a record, not a gate.** `farik_request_purchase` records `purchase.requested` and returns its number; it does not end the session, hold the task, or ask before running, unlike an `external_effect` call (ADR 0031), because nothing outside Farik happens. Rejected: tagging it `external_effect` and reusing `tool_approval`, which would make the approval the purchase's permission when the human, not Farik, makes the purchase.
- **Its input**, checked in this order, each refusal named: only a Procurement Specialist in a session about a procurement task (`purchase_refused`); `vendor` and `item` 1 to 100 characters with no control character, `plan` 0 to 100 (`purchase_field_invalid` with the field's name); `price` a decimal string matching `^(0|[1-9][0-9]{0,6})(\.[0-9]{1,2})?$`, at most `1000000` (`purchase_price_invalid`); `currency` `^[A-Z]{3}$`; `period` `once`, `month` or `year`; `url` parsed by `url::Url`, scheme `https`, a host, no userinfo, at most 2,000 characters (`purchase_url_invalid`); `evaluation` a relative path `evaluations/<name>.md` that exists in the folder (`evaluation_missing`); `why` 20 to 600 characters; then no open request for the same `vendor` (compared case-insensitively) on the same task (`purchase_already_requested`). A request is open until decided.
- **Events.** `purchase.requested { purchase, agent_id, task_id, vendor, item, plan, price, currency, period, url, evaluation, why }`, `purchase` its own sequence number; `purchase.bought { purchase, paid, currency, renews_on? }`; `purchase.declined { purchase, note? }`; `renewal.due { vendor, renews_on, decide_by }`; `renewal.dismissed { renewal }`, `renewal` the sequence number of the `renewal.due`. All in `event.schema.json`, in `snake_case`, and in spec 8.5. Rejected: `purchase.approved` before `purchase.bought`, a state that tracks nothing Farik can see.
- **Only the human decides.** The command `purchase_decide { purchase, decision: bought | declined, paid?, currency?, renews_on?, note? }` is accepted from the daemon's token or the browser's cookie alone, as `tool_approve` is (ADR 0031); `paid` is required with `bought` (`purchase_paid_missing`) and refused with `declined`; `paid` and `currency` follow `price`'s and `currency`'s rules, `currency` defaulting to the request's; `renews_on` an ISO date; `note` at most 600 characters. `unknown_purchase` for a number that is not a `purchase.requested`, `purchase_decided` for one already decided. A `purchase.bought` or `purchase.declined` an agent's session recorded is ignored by the projection.
- **`waiting.list` gains `purchase` rows** `{ kind: purchase, purchase, agent, task, vendor, item, plan, price, currency, period, host, url, evaluation, why, requested_at }` and **`renewal` rows** `{ kind: renewal, renewal, vendor, renews_on, decide_by }`. `host` is the URL's host, computed by the daemon so the page never parses it.
- **`PurchaseRequest`** (Today), mocked up first: the vendor, item and plan; "<price> <currency> a month" (or "a year", "once"); the why; "Read the comparison", which opens the evaluation's text in an untrusted frame (the text is the agent's, read through `purchase.evaluation { purchase }`, which answers the file's text and refuses any other path); the address as text with the host in bold and "Check this is <vendor>'s own site before you pay", and "Open", a link with `rel="noopener noreferrer"` and `target="_blank"`; "I bought it", which asks "What did you pay?" (prefilled with the request's price and currency) and "When does it renew?" (optional), then sends `purchase_decide` `bought`; "Not buying", with an optional note. The renewal row: "<vendor> renews on <date>. Decide by <date>." with "Ask for a review" and "Dismiss".
- **`farik_read_purchases {}`**, `read` tier, Procurement Specialist only: every request, oldest first, with `state` `open`, `bought` (with `paid`, `currency`, `renews_on`) or `declined` (with `note`). The human's note is quoted as untrusted text (spec 8.6), since it may hold anything pasted.
- **The renewal tick.** A rule in `orchestrator/rules.rs`, after the sprint rules, once per UTC day while a process drives the project, only when the team has an active Procurement Specialist and `vendors.xlsx` exists; it reads the `Vendors` sheet with step 09's reader, never starting a session. Columns are found by their header in row 1 (`vendor`, `renews_on`, `notice_days`, `status`), so a column the user moved still reads; a sheet without those headers is skipped and counted. A date cell is an Excel date or an ISO text date; `notice_days` an integer from 0 to 365 or blank (0). The decision date is `renews_on` less `notice_days`; due when `status` is `active` or `trial` and today (UTC) is from 14 days before the decision date to `renews_on` inclusive, and no `renewal.due` exists for that `vendor` (case-insensitive) and `renews_on`. Each day's run records `renewal.checked { due, unreadable }` so the rule knows it ran, as `sweep.started` does for receipts. A run while a procurement task is `in_progress` reads the file as it is; step 09's writer renames into place, so a half-written file is never read.
- **`renewals.list {}`** answers `{ open: [...], unreadable }`, `unreadable` the last `renewal.checked`'s; Today shows "<n> rows in the register have a renewal date Farik can't read" when it is not 0.
- **"Ask for a review"** sends the existing request command with the text "Review <vendor> before it renews on <renews_on>; decide by <decide_by>." and then `renewal_dismiss { renewal }`, so the row leaves Today; the request is triaged as any other (spec 5.16). **"Dismiss"** sends `renewal_dismiss` alone; `renewal_dismissed` refuses a second, `unknown_renewal` a number that is not one.
- **Command line.** `farik purchase list [--json]`; `farik purchase bought <n> --paid <amount> [<currency>] [--renews-on <date>]`; `farik purchase decline <n> [--note <text>]`; `farik renewal list`, `farik renewal dismiss <n>`; each through `here_or_sent`, as `farik tool approve` is.
- **One migration**, `0013_purchases.sql`: `open_purchases` on the agent's projection, so the board's agent card can say "1 purchase waiting on you". Rejected: reading the log on each `waiting.list`, which step 02 already avoided for approvals.

## File map

```
docs/design/mockups/purchase-request.*                      creates: the mockups (Task 0)
docs/schemas/event.schema.json, command.schema.json, rpc.schema.json   modifies: kinds, commands, waiting rows, queries (Tasks 1, 3, 5)
crates/protocol/src/event.rs                                modifies: the five kinds (Task 1)
crates/runtime/src/tools/purchase.rs                        creates: farik_request_purchase, farik_read_purchases (Tasks 1, 2)
crates/runtime/src/tools.rs, orchestrator/session.rs        modifies: descriptors and the offer (Tasks 1, 2)
crates/store/src/migrations/0013_purchases.sql, projections.rs, waiting.rs   creates/modifies (Task 3)
crates/runtime/src/orchestrator/human.rs                    modifies: purchase_decide, renewal_dismiss (Tasks 3, 5)
crates/runtime/src/daemon/gates.rs                          modifies: waiting.list rows, purchase.evaluation, renewals.list (Tasks 3, 5)
crates/runtime/src/orchestrator/rules.rs, renewals.rs        modifies/creates: the renewal tick (Task 4)
crates/cli/src/purchase.rs, renewal.rs                       creates (Task 6)
apps/web/src/pages/Today.tsx, dialogs/PurchaseRequest.tsx(+test)   modifies/creates (Task 7)
docs/SPEC.md, docs/plans/project-plan.md                     modifies (Task 8)
```

## Interfaces

Consumes: `Call`, `FarikTool`, `offered_tools`, `EventBody`, `append` (runtime, protocol, store); `here_or_sent` (cli); step 02's human-only command check and `waiting.list`; step 09's sheet reader; step 10b's procurement folder and `Role::ProcurementSpecialist`.

Produces:

```rust
pub struct RequestPurchaseInput { pub vendor: String, pub item: String, pub plan: String, pub price: String,
    pub currency: String, pub period: PurchasePeriod, pub url: String, pub evaluation: String, pub why: String }
pub enum PurchasePeriod { Once, Month, Year }
pub fn request_purchase(call: &Call<'_>, input: RequestPurchaseInput) -> Result<Value, ToolError>;
pub fn read_purchases(call: &Call<'_>) -> Result<Value, ToolError>;
pub struct DueRenewal { pub vendor: String, pub renews_on: NaiveDate, pub decide_by: NaiveDate }
pub fn due_renewals(rows: &[RegisterRow], today: NaiveDate, recorded: &[(String, NaiveDate)]) -> (Vec<DueRenewal>, u32);
```

`due_renewals` is pure (no I/O), so it lives in `farik-core` (`core::renewals`), with `RegisterRow { vendor, renews_on: Option<NaiveDate>, notice_days: Option<u16>, status }` as parsed text; the reader in runtime turns cells into rows.

## Tasks

### Task 0: Mockups

`PurchaseRequest` (desktop and phone), the Today rows for a purchase and a renewal, and the unreadable-dates line, approved by the founder; the approval recorded in the Execution notes.

- [ ] `docs(design): mock up purchase requests and renewals`

### Task 1: `farik_request_purchase`

- `records_a_purchase_request`: a valid input records `purchase.requested` with every field and returns `{ purchase: <seq> }`. RED.
- `refuses_another_role_and_a_task_less_session`: a Finance Specialist, and a Procurement Specialist's chat, are `purchase_refused`. RED.
- `refuses_each_bad_field`: one case per refusal in Decisions (`price` `"1,000"`, `"10.999"`, `"-1"`; `currency` `"usd"`; `url` `http://x`, `https://u:p@x`, `ftp://x`; `evaluation` missing or `../x.md`), each writing nothing. RED.
- `refuses_a_second_open_request_for_a_vendor`: same task, "Postmark" then "postmark", the second `purchase_already_requested`; after a decision, a new one is accepted. RED.

- [ ] `feat(runtime): let the Procurement Specialist request a purchase`

### Task 2: `farik_read_purchases`

- `reads_every_request_and_its_outcome`: open, bought (with what was paid) and declined, oldest first; a note comes back inside the untrusted-content notice. RED.

- [ ] `feat(runtime): let the Procurement Specialist read its purchase requests`

### Task 3: The human decides

Files: migration 0013, projections, `waiting.rs`, `human.rs`, `gates.rs`, the command and rpc schemas.

- `a_bought_purchase_leaves_waiting`: `purchase_decide` `bought` with `paid` records `purchase.bought`, the row leaves `waiting.list`, and `open_purchases` falls to 0. RED.
- `bought_needs_what_was_paid`: without `paid`, `purchase_paid_missing`; `declined` with `paid` is refused. RED.
- `decided_once`: a second decision is `purchase_decided`; an unknown number `unknown_purchase`. RED.
- `only_the_human_decides`: a `purchase.bought` appended by an agent session's identity leaves the request open. RED.
- `waiting_list_gives_the_host`: a purchase row carries `host` `postmarkapp.com` for `https://postmarkapp.com/pricing`. RED.
- `purchase_evaluation_reads_only_its_file`: `purchase.evaluation` answers the named file's text and `not_found` for a purchase with none. RED.

- [ ] `feat(runtime): let the human mark a purchase bought or not`

### Task 4: The renewal tick

Files: `core::renewals` (pure), `runtime::renewals` (the reader), `rules.rs`.

- `due_two_weeks_before_the_decision_date` (core): renews 2026-11-30 with 30 days' notice is due from 2026-10-17 to 2026-11-30, not on 2026-10-16 or 2026-12-01. RED.
- `never_for_a_cancelled_or_planned_row` (core). RED.
- `once_per_vendor_and_date` (core): a recorded `("Vercel", 2026-11-30)` is not due again; a new `renews_on` is. RED.
- `an_unreadable_date_is_counted_not_guessed` (core): `"next month"`, `""` and `notice_days` `-3` are skipped and counted. RED.
- `reads_columns_by_their_header` (runtime): a sheet with `renews_on` moved to column A reads the same rows. RED.
- `the_tick_runs_once_a_day_without_a_session` (runtime): two ticks on one UTC day record one `renewal.checked` and start no session; no Procurement Specialist, or no `vendors.xlsx`, records nothing. RED.

- [ ] `feat(runtime): remind the human of renewals from the register`

### Task 5: Renewal rows

- `renewals_list_answers_open_and_unreadable`; `dismissing_closes_a_renewal` (`renewal_dismissed` on a second, `unknown_renewal` otherwise). RED each.

- [ ] `feat(runtime): list and dismiss renewals`

### Task 6: The command line

- `purchase_bought_sends_what_was_paid` and `purchase_list_prints_open_requests_with_their_host` (`--json` stdout pure); `renewal_dismiss_sends_the_number`. RED each.

- [ ] `feat(cli): decide purchases and dismiss renewals`

### Task 7: Today

As the approved mockups.

- `shows_a_purchase_with_its_host_in_bold`; `i_bought_it_asks_what_was_paid_then_sends`; `not_buying_sends_declined_with_the_note`; `the_comparison_opens_in_an_untrusted_frame` (markup inert); `ask_for_a_review_files_a_request_then_dismisses`; `says_how_many_dates_cannot_be_read`. RED each.

- [ ] `feat(web): decide purchases and renewals on Today`

### Task 8: Spec and plan

`docs/SPEC.md` 6.10 (what was built), 8.5 (the five kinds and `renewal.checked`), 5.7 (a purchase request waits like a question but holds no task); the revision line. Project plan row 10c.

- [ ] `docs(spec): record purchase requests and renewals`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: the Procurement Specialist files a request; Today shows it with the host; "I bought it" with a price; the agent's next task writes the row with its renewal date ten days out; the next day's tick puts the renewal on Today; "Ask for a review" files the request.

## Execution notes

None yet.
