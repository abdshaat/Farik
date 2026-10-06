# Phase 7, step 10c: Purchase orders and renewals

Status: draft. Its readiness review runs once step 10b has landed. The founder answered O1 on 2026-10-05: the agent "never directly buy[s]"; it sets up a purchase order, and "the final decision is the founder['s]".
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.7, 6.10, 8.5; F9
Depends on: step 10b of this phase (the role and its folder); step 09 (`calamine` and the sheet writer); step 02 (the human-only decision path of `tool_approve`, `waiting.list`, Today's dialog pattern); phase 6 (merged in #19)
Readiness confirmed by: not yet. A fresh-session Opus reviewer read the plans on 2026-10-05 before their dependencies landed: 3 Blocking, all folded (here: `renewal.due` renamed `renewal.flagged`). The plan was rewritten the same day from purchase requests to purchase orders on the founder's answer to O1; the readiness review proper runs when step 10b has landed (ADR 0032: one round).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Procurement Specialist sets up a purchase order: the seller, each line with its quantity and unit price, the currency, delivery and terms, and the evaluation behind it, written as `orders/PO-<n>.xlsx` in its folder. The founder approves it ("I'll place it myself") or rejects it on Today or at the command line, and later marks it received, with what was paid; the agent reads the outcome in its next task. Sending an approved order to its seller is step 10f's. Once a day, with no model, Farik reads the register's renewal dates and puts each renewal whose decision date is two weeks off or nearer on Today. Out of scope: Farik paying or placing an order (ADR 0039: never); Farik writing the register (the agent does); reminders by email or notification (phase 9 step 03).

## Decisions

- **A purchase order is a record and a document, not a gate.** `farik_draft_purchase_order` writes the order's workbook and records `purchase_order.drafted`; it does not end the session, hold the task, or ask before running, unlike an `external_effect` call (ADR 0031), because nothing outside Farik happens. Rejected: tagging it `external_effect` and reusing `tool_approval`, which would make the approval a permission for Farik to act when the founder, not Farik, places the order.
- **Its input**, `{ seller, seller_contact, lines, currency, period, delivery, terms, url, evaluation, why }`, checked in this order, each refusal named: only a Procurement Specialist in a session about a procurement task (`purchase_order_refused`); `seller` 1 to 100 characters with no control character, `seller_contact` 0 to 200 (an address, a phone number or a name, shown as text); `lines` 1 to 50, each `{ item, quantity, unit_price, unit }`, `item` 1 to 200 characters, `quantity` an integer from 1 to 1,000,000, `unit_price` a decimal string matching `^(0|[1-9][0-9]{0,8})(\.[0-9]{1,2})?$`, `unit` 0 to 20 (`purchase_order_line_invalid` with the line's index and field); the order's total at most 10,000,000 (`purchase_order_too_large`); `currency` `^[A-Z]{3}$`; `period` `once`, `month` or `year` (a subscription's lines are per period); `delivery` and `terms` 0 to 600 each; `url` empty or an `https` address with a host, no userinfo, at most 2,000 characters (`purchase_order_url_invalid`); `evaluation` a relative path `evaluations/<name>.md` that exists in the folder (`evaluation_missing`); `why` 20 to 600 characters; then no open order for the same `seller` (case-insensitive) on the same task (`purchase_order_open`). An order is open until approved or rejected.
- **The workbook.** Farik writes `orders/PO-<n>.xlsx`, `<n>` the event's sequence number, with step 09's writer: sheet `Order`, a header block (order number, date, buyer from the team's business name, seller and contact, currency, period, delivery, terms) then a line table (item, quantity, unit, unit price, line total) and the total; every cell a value, line totals computed by Farik, not formulas, so the seller and the founder read the same numbers in any program. The agent never writes `orders/`; `farik_write_sheet` refuses it (`orders_are_farik_s`). Step 09b's `write_workbook(folder, path, sheets, now)` trusts its caller to have run `private_path`, and `SheetInput` gains a public constructor here for writing a purchase order outside a tool call.
- **Events.** `purchase_order.drafted { order, agent_id, task_id, seller, seller_contact, lines, currency, period, total, delivery, terms, url, evaluation, why }`, `order` its own sequence number; `purchase_order.approved { order, note? }`; `purchase_order.rejected { order, note? }`; `purchase_order.fulfilled { order, paid, currency, received_on?, renews_on? }`; `renewal.flagged { vendor, renews_on, decide_by }`; `renewal.dismissed { renewal }`, `renewal` the sequence number of the `renewal.flagged`; `renewal.checked { due, unreadable }`, once per day's run. All seven in `event.schema.json`, in `snake_case`, and in spec 8.5.
- **Only the founder decides.** `purchase_order_decide { order, decision: approve | reject, note? }` and `purchase_order_fulfil { order, paid, currency?, received_on?, renews_on? }` are accepted from the daemon's token or the browser's cookie alone, as `tool_approve` is (ADR 0031). `paid` follows `unit_price`'s rule up to 10,000,000, `currency` defaults to the order's; dates are ISO; `note` at most 600. `unknown_purchase_order` for a number that is not a drafted order, `purchase_order_decided` for a second decision, `purchase_order_not_approved` for fulfilling an order not approved, `purchase_order_fulfilled` for a second fulfilment. An agent session's recorded decision is ignored by the projection.
- **`waiting.list` gains `purchase_order` rows** `{ kind: purchase_order, order, agent, task, seller, seller_contact, lines, currency, period, total, host, url, evaluation, why, drafted_at }`, for drafted orders, and **`renewal` rows** `{ kind: renewal, renewal, vendor, renews_on, decide_by }`. `host` is the URL's host, computed by the daemon so the page never parses it. An approved order waiting to be received is listed on the Procurement Specialist's page, not on Today.
- **`PurchaseOrder`** (Today), mocked up first: the seller and contact; each line; the total with "a month", "a year" or "once"; the why; "Read the comparison", which opens the evaluation's text in an untrusted frame (read through `purchase_order.evaluation { order }`, which canonicalises the path, requires it under the folder's `evaluations/`, refuses a link anywhere on it, and answers `not_found` otherwise); "Open the order" (the workbook, downloaded through `purchase_order.file { order }`); the seller's address as text with the host in bold and "Check this is <seller>'s own site before you pay" beside "Open", a link with `rel="noopener noreferrer"` and `target="_blank"`; "Approve, I'll place it myself" and "Reject", each with an optional note. "Mark received", on the agent's page, asks "What did you pay?" (prefilled with the total) and the dates.
- **`farik_read_purchase_orders {}`**, `read` tier, Procurement Specialist only: every order, oldest first, with `state` `drafted`, `approved`, `rejected` or `fulfilled` (with `paid`, `currency`, the dates). The founder's notes are quoted as untrusted text (spec 8.6), since they may hold anything pasted.
- **The renewal tick.** A rule in `orchestrator/rules.rs`, after the sprint rules, once per UTC day while a process drives the project, only when the team has an active Procurement Specialist and `vendors.xlsx` exists; it reads the `Vendors` sheet with step 09's reader, never starting a session. Columns are found by their header in row 1 (`vendor`, `renews_on`, `notice_days`, `status`), so a column the user moved still reads; a sheet without those headers is skipped and counted. A date cell is an Excel date or an ISO text date; `notice_days` an integer from 0 to 365 or blank (0). The decision date is `renews_on` less `notice_days`; due when `status` is `active` or `trial` and today (UTC) is from 14 days before the decision date to `renews_on` inclusive, and no `renewal.flagged` exists for that `vendor` (case-insensitive) and `renews_on`. Each day's run records `renewal.checked { due, unreadable }` so the rule knows it ran (tested by `the_tick_runs_once_a_day_without_a_session`). A run while a procurement task is `in_progress` reads the file as it is; step 09's writer renames into place, so a half-written file is never read.
- **`renewals.list {}`** answers `{ open: [...], unreadable }`, `unreadable` the last `renewal.checked`'s; Today shows "<n> rows in the register have a renewal date Farik can't read" when it is not 0.
- **"Ask for a review"** sends the existing request command with the text "Review <vendor> before it renews on <renews_on>; decide by <decide_by>." and then `renewal_dismiss { renewal }`, so the row leaves Today; the request is triaged as any other (spec 5.16). **"Dismiss"** sends `renewal_dismiss` alone; `renewal_dismissed` refuses a second, `unknown_renewal` a number that is not one.
- **Command line.** `farik order list [--json]`; `farik order approve <n> [--note <text>]`; `farik order reject <n> [--note <text>]`; `farik order received <n> --paid <amount> [<currency>] [--received-on <date>] [--renews-on <date>]`; `farik renewal list`, `farik renewal dismiss <n>`; each through `here_or_sent`, as `farik tool approve` is.
- **One migration**, at the next free number (0013 today), `<n>_purchase_orders.sql`: `open_purchase_orders` on the agent's projection, so the board's agent card can say "1 order waiting on you". Rejected: reading the log on each `waiting.list`, which step 02 already avoided for approvals.

## File map

```
docs/design/mockups/purchase-order.*                        creates: the mockups (Task 0)
docs/schemas/event.schema.json, command.schema.json, rpc.schema.json   modifies: kinds, commands, waiting rows, queries (Tasks 1, 3, 5)
crates/protocol/src/event.rs                                modifies: the seven kinds (Tasks 1, 3, 4)
crates/runtime/src/tools/purchase_order.rs                  creates: farik_draft_purchase_order, farik_read_purchase_orders, the order workbook (Tasks 1, 2)
crates/runtime/src/tools.rs, orchestrator/session.rs        modifies: descriptors and the offer (Tasks 1, 2)
crates/store/src/migrations/<next>_purchase_orders.sql, projections.rs, waiting.rs   creates/modifies (Task 3)
crates/runtime/src/orchestrator/human.rs                    modifies: purchase_order_decide, purchase_order_fulfil, renewal_dismiss (Tasks 3, 5)
crates/runtime/src/daemon/gates.rs                          modifies: waiting.list rows, purchase_order.evaluation, purchase_order.file, renewals.list (Tasks 3, 5)
crates/runtime/src/orchestrator/rules.rs, renewals.rs        modifies/creates: the renewal tick (Task 4)
crates/cli/src/order.rs, renewal.rs                       creates (Task 6)
apps/web/src/pages/Today.tsx, dialogs/PurchaseOrder.tsx(+test), the Procurement Specialist's agent page   modifies/creates (Task 7)
docs/SPEC.md, docs/plans/project-plan.md                     modifies (Task 8)
```

## Interfaces

Consumes: `Call`, `FarikTool`, `offered_tools`, `EventBody`, `append` (runtime, protocol, store); `here_or_sent` (cli); step 02's human-only command check and `waiting.list`; step 09's sheet reader; step 10b's procurement folder and `Role::ProcurementSpecialist`.

Produces:

```rust
pub struct OrderLine { pub item: String, pub quantity: u32, pub unit_price: String, pub unit: String }
pub struct DraftPurchaseOrderInput { pub seller: String, pub seller_contact: String, pub lines: Vec<OrderLine>, pub currency: String,
    pub period: OrderPeriod, pub delivery: String, pub terms: String, pub url: String, pub evaluation: String, pub why: String }
pub enum OrderPeriod { Once, Month, Year }
pub fn draft_purchase_order(call: &Call<'_>, input: DraftPurchaseOrderInput) -> Result<Value, ToolError>;
pub fn read_purchase_orders(call: &Call<'_>) -> Result<Value, ToolError>;
pub fn order_total(lines: &[OrderLine]) -> Result<Decimal, OrderError>;   // farik_core::order, pure: exact decimal sums, no floats
pub struct DueRenewal { pub vendor: String, pub renews_on: NaiveDate, pub decide_by: NaiveDate }
pub fn due_renewals(rows: &[RegisterRow], today: NaiveDate, recorded: &[(String, NaiveDate)]) -> (Vec<DueRenewal>, u32);
```

`due_renewals` is pure (no I/O), so it lives in `farik-core` (`core::renewals`), with `RegisterRow { vendor, renews_on: Option<NaiveDate>, notice_days: Option<u16>, status }` as parsed text; the reader in runtime turns cells into rows.

## Tasks

### Task 0: Mockups

`PurchaseOrder` (desktop and phone), the Today rows for an order and a renewal, "Mark received" on the agent's page, and the unreadable-dates line, approved by the founder; the approval recorded in the Execution notes.

- [ ] `docs(design): mock up purchase orders and renewals`

### Task 1: `farik_draft_purchase_order`

- `order_total_adds_exactly` (core): `3 × 19.99` and `1 × 0.01` total `59.98`, with no float rounding; a total over 10,000,000 is refused. RED.
- `drafts_an_order_and_its_workbook`: a valid input records `purchase_order.drafted` with every field and the total, and writes `orders/PO-<n>.xlsx` whose cells read back as the header and lines, every cell a value. RED.
- `refuses_another_role_and_a_task_less_session`: a Finance Specialist, and a Procurement Specialist's chat, are `purchase_order_refused`. RED.
- `refuses_each_bad_field`: one case per refusal in Decisions (`unit_price` `"1,000"`, `"10.999"`, `"-1"`; `quantity` 0; 51 lines; `currency` `"usd"`; `url` `http://x`, `https://u:p@x`; `evaluation` missing or `../x.md`), each writing nothing. RED.
- `refuses_a_second_open_order_for_a_seller`: same task, "Acme" then "acme", the second `purchase_order_open`; after a decision, a new one is accepted. RED.
- `the_agent_cannot_write_orders`: `farik_write_sheet { path: orders/PO-1.xlsx }` is `orders_are_farik_s`. RED.

- [ ] `feat(runtime): let the Procurement Specialist draft a purchase order`

### Task 2: `farik_read_purchase_orders`

- `reads_every_order_and_its_outcome`: drafted, approved, rejected and fulfilled (with what was paid), oldest first; a note comes back inside the untrusted-content notice. RED.

- [ ] `feat(runtime): let the Procurement Specialist read its purchase orders`

### Task 3: The founder decides

Files: the purchase-orders migration, projections, `waiting.rs`, `human.rs`, `gates.rs`, the command and rpc schemas.

- `an_approved_order_leaves_today`: `purchase_order_decide` approve records `purchase_order.approved`, the row leaves `waiting.list`, and `open_purchase_orders` falls to 0. RED.
- `a_received_order_carries_what_was_paid`: `purchase_order_fulfil` on an approved order records `purchase_order.fulfilled`; on a drafted one it is `purchase_order_not_approved`; a second is `purchase_order_fulfilled`. RED.
- `decided_once`: a second decision is `purchase_order_decided`; an unknown number `unknown_purchase_order`. RED.
- `only_the_founder_decides`: a `purchase_order.approved` appended by an agent session's identity leaves the order drafted. RED.
- `waiting_list_gives_the_host`: an order row carries `host` `acme.example` for `https://acme.example/shop`, and none for an order with no `url`. RED.
- `the_evaluation_and_the_file_read_only_their_own`: `purchase_order.evaluation` and `purchase_order.file` answer the order's own files and `not_found` otherwise, a link on the path refused. RED.

- [ ] `feat(runtime): let the founder approve, reject and receive a purchase order`

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

- `order_received_sends_what_was_paid` and `order_list_prints_drafted_orders_with_their_total` (`--json` stdout pure); `renewal_dismiss_sends_the_number`. RED each.

- [ ] `feat(cli): decide purchase orders and dismiss renewals`

### Task 7: Today

As the approved mockups.

- `shows_an_order_with_its_lines_total_and_host`; `approve_sends_approve_with_the_note`; `reject_sends_reject`; `mark_received_asks_what_was_paid_then_sends`; `the_comparison_opens_in_an_untrusted_frame` (markup inert); `ask_for_a_review_files_a_request_then_dismisses`; `says_how_many_dates_cannot_be_read`. RED each.

- [ ] `feat(web): decide purchase orders and renewals on Today`

### Task 8: Spec and plan

`docs/SPEC.md` 6.10 (what was built), 8.5 (the seven kinds), 5.7 (a purchase order waits on the founder like a question but holds no task); the revision line. Project plan row 10c.

- [ ] `docs(spec): record purchase orders and renewals`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: the Procurement Specialist drafts an order for three baby car mirrors from one seller; Today shows its lines, total and host; "Approve, I'll place it myself"; later "Mark received" with what was paid; the agent's next task writes the row; a row given a renewal date ten days out puts the renewal on Today the next day, and "Ask for a review" files the request.

## Execution notes

None yet.
