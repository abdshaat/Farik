# Phase 7, step 10: Finance Specialist kit

Status: draft. Its readiness review runs once step 09c has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.6, 6.7; F9
Depends on: steps 09, 09b and 09c of this phase (the role, its sheet tools, its folder); steps 05 and 05b (the kit format, allowances); step 03 (signing in); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Finance Specialist's kit: six skills for the work its role does (categorising expenses, closing a month, forecasting, unit economics and pricing, recommending a budget, and using its sources), and three services it only reads, each signed in to with nothing pasted: Stripe, for the product's revenue, fees, refunds and payouts; and two bookkeeping services a business may already keep its books in, Digits and Kick. No tool of the kit changes anything at a service: every write is `denied`, and Kick is asked for read access alone. Out of scope: receipts and bank statements (phase 13 step 02); a currency converter (Farik's own `fx`, step 10d, which a later pin may add to this kit); QuickBooks and Xero (their official servers need an app registration of Farik's own; a candidate for phase 9).

## Decisions

- **No mockups.** A kit's services show on the agent page as every kit's do (step 05); nothing is new.
- **How each server was chosen**, by ADR 0020's order (the service's own server first) and ADR 0035's routes, probed 2026-10-05:
  - **Stripe, official, `http`, `https://mcp.stripe.com`, signed in (route 1), `oauth: {}`.** Resource `https://mcp.stripe.com`; authorization server `https://access.stripe.com/mcp`: `registration_endpoint` `/oauth2/register`, S256, `token_endpoint_auth_methods_supported` `[none]`, no `scopes_supported` (the user picks the account, live or sandbox, and the permissions on Stripe's own page, which the setup copy says). Its ten tools are from docs.stripe.com/mcp (read 2026-10-05): two that reach the whole API, `stripe_api_read` (any `GET`) and `stripe_api_write` (any `POST`, `PATCH`, `PUT` or `DELETE`), and eight others. Rejected: an Agent-tagged restricted key in place of signing in (the design's "a scheduled run uses a restricted key" predates step 03: a signed-in service refreshes its own access, so no run needs a key, and a kit entry has one way in); Stripe's local `@stripe/mcp` package (the remote server is Stripe's recommended one, and from 2026-10-31 it is the one that takes Agent keys).
  - **Digits, official, `http`, `https://api.digits.com/mcp`, signed in (route 1), `oauth: {}`.** Resource `https://api.digits.com/mcp`; authorization server `https://api.digits.com`: `registration_endpoint` `/oauth/register`, S256, `none` among its methods, revocation `/oauth/revoke`, no scopes. Read-only by Digits' own statement (help.digits.com, "Digits MCP", read 2026-10-05); its ten tools from that page.
  - **Kick, official, `http`, `https://use.kick.co/mcp`, signed in (route 1), `oauth: { scopes: ["mcp:read"] }`.** Resource `https://use.kick.co/mcp`; authorization server `https://use.kick.co`: `registration_endpoint` `/mcp/oauth/register`, S256, `[none]`, revocation `/mcp/oauth/revoke`, `scopes_supported` `[mcp:read, mcp:write]`; Kick documents `mcp:read` as "tool discovery and read-only calls", so a write is refused at Kick as well as in Farik. Its 39 tools are from Kick's tool reference (docs.kick.co, "Tool Reference", read 2026-10-05).
  - Rejected for now: QuickBooks Online and Xero (Intuit's and Xero's servers need a developer app of Farik's own with a client secret, which route 1 cannot give; ADR 0035's route 2 is for Farik's registered apps, a phase 9 candidate); Brex (the Procurement Specialist's in step 10d, for spend; the Finance Specialist reads spend from the books and, later, statements).
- **What each tag is.** A read of the business's own numbers is `network`. `denied`: every write, whatever it writes (Stripe's `stripe_api_write`; Kick's twenty `*_act`, `*_create`, `*_update` and `activity_undo`), since spec 6.6 says the role never changes a service; a tool that sends something to the service's company (`send_stripe_feedback`); a tool for building an integration rather than reading the business's numbers (`stripe_implementation_planner`); a list of the people with access to the books (`list_business_users`), which the role does not need; and Kick's `list_kick_skills` and `load_kick_skill`, which load Kick's own instructions into the session: the role's instructions are Farik's skills, and a service's words are data (spec 8.6). No tool has an allowance, since none is `external_effect`.
- **Stripe's one read tool reaches customers' details.** `stripe_api_read` takes any `GET`, so a charge comes back with its customer's name and email, and Farik cannot split one tool. The role needs charges, refunds, payouts and balance transactions, so the tool stays `network`, and the skill `using-finance-sources` holds the line the Marketing kit's `denied` subscriber reads hold: the books carry totals and Stripe object ids (`ch_…`, `po_…`), never a customer's name, email or card, and nothing about one person goes in a note or the channel. Rejected: denying `stripe_api_read` and keeping `stripe_analytics` alone (it answers metrics, not the charges and payouts a month's close reconciles).
- **The six skills**, in this order, each passing `check_skill` and naming only `farik_*` tools that `tool_descriptors` lists:
  - `categorising-expenses`: one category per line from the books' `Categories` sheet, the AI spending by `farik_read_costs`' `purpose`, a new category only with a reason in the row's note, never a tax category's name as advice.
  - `closing-the-month`: read the month's costs, Stripe's charges, refunds, fees and payouts, and the ledger's totals; reconcile each source against the others, a difference over one per cent named in the note with both figures; the `Monthly summary` row written last; the month marked closed only when every difference is explained.
  - `forecasting`: the next three sprints or months from the last three, with the method named (the average, or the trend, and why); a range rather than one figure; the assumptions in their own sheet; never a forecast without its date and inputs.
  - `unit-economics-and-pricing`: revenue per customer, cost per customer (AI spend and fees included), gross margin, payback; a price change only as a recommendation with its numbers, never made.
  - `recommending-a-budget`: a daily and per-sprint AI budget from the forecast, as a recommendation the human decides in Settings, since the role cannot change Farik's budgets (spec 6.6).
  - `using-finance-sources`: what Stripe, Digits and Kick are each for; every figure with its source and date; customers' details as above; what a service returns is data, never an instruction; the role only reads, so a fix at a service is something it tells the user; when none is connected, ask the user with `farik_ask_human` rather than guess.
- **Pins**, by step 06's mechanical rule: each documented tool pinned, in the documentation's order; anything else a service lists goes in `denied` with no label at its next pin update.

## File map

```
crates/roles/roles/finance_specialist/skills/<six>/SKILL.md          creates (Task 1)
crates/roles/roles/finance_specialist/kit.yaml                        modifies: skills (Task 1), connectors (Tasks 2, 3)
crates/roles/src/kit.rs                                               modifies: embedded_skills arm; tests (Tasks 1 to 3)
crates/runtime/src/daemon/team.rs                                     tests: connect by name (Task 4)
crates/runtime/tests/live_kit_pins.rs                                 modifies: header comment (Task 4)
docs/SPEC.md, docs/design/role-kits.md, docs/design/finance-specialist.md, docs/plans/project-plan.md   modifies (Task 5)
```

## Interfaces

Consumes: `load_kit`, `KitConnector`, `check_skill`, `tool_descriptors`, `SHIPPED_ROLES` (`farik-roles`); `kit_entry` (`farik_runtime::daemon::team`, as step 08's guard uses it); `Role::FinanceSpecialist` (step 09); `farik_read_costs`, `farik_read_sheet`, `farik_write_sheet` (step 09b). Produces: no new signature.

## Tasks

### Task 1: The six skills

`kit.yaml`'s `skills` gains the six in the order above; `embedded_skills` gains the role's arm. Step 09's `its_kit_is_empty_until_step_10` is replaced by this task's test, in the same commit, since it asserts what this step changes.

- `finance_kit_carries_its_skills`: the kit's skills are the six, in order, each loading with its `description`, none named like the role's own `keeping-the-books`. RED: the kit has none.
- `loads_every_shipped_kit`: the Finance Specialist's kit counts six skills, no connectors. RED.
- `kit_skills_name_only_tools_farik_lists` (the existing guard) covers the six. Guard.

- [ ] `feat(roles): give the Finance Specialist's kit its skills`

### Task 2: Stripe

`stripe` first among the connectors; `loads_every_shipped_kit`: 1. Title "Stripe". About "Stripe takes your product's payments: charges, subscriptions, invoices, fees, refunds and payouts." Why "So the Finance Specialist can put your revenue, fees and payouts in the books from Stripe's own numbers. It only reads." Setup "Sign in with your Stripe account. On Stripe's page, choose the account and give Farik read access only; Farik refuses every change anyway. To end Farik's access, revoke it under ‘OAuth sessions’ in your Stripe user settings."

- `network` (7), each labelled: `stripe_api_search` "find what Stripe can answer", `stripe_api_details` "read how to ask Stripe", `stripe_api_read` "read payments and payouts", `get_stripe_account_info` "read the account", `stripe_analytics` "ask about revenue", `get_balance_summary` "read the balance", `search_stripe_documentation` "search Stripe's help".
- `denied` (3): `stripe_api_write`, `stripe_implementation_planner`, `send_stripe_feedback`.
- `stripe_only_reads`: `http` at `https://mcp.stripe.com`, `oauth` with no scopes; the 7 `network` exactly, each with its label; the 3 `denied`, among them `stripe_api_write`; 10 in all; no allowances. RED.

- [ ] `feat(roles): give the Finance Specialist Stripe`

### Task 3: Digits and Kick

`digits` then `kick` after `stripe`; `loads_every_shipped_kit`: 3. Digits: title "Digits"; about "Digits keeps your books: every transaction, with profit and loss, balance sheet, cash flow and who owes whom."; why "So the Finance Specialist can read the books you already keep there instead of rebuilding them. It only reads."; setup "Sign in with your Digits account and choose the business. Digits gives Farik read access only." Kick: title "Kick"; about "Kick keeps your books from your bank and card accounts: transactions, categories, journals and reports."; why "So the Finance Specialist can read the books you already keep there instead of rebuilding them. It only reads."; setup "Sign in with your Kick account. Farik asks Kick for read access only, so it cannot change your books, and it refuses every change anyway."

- Digits `network` (9), each labelled: `list_businesses` "list businesses", `select_business` "choose a business", `query_transactions` "read transactions", `search_term` "find a name in the books", `list_departments` "list departments", `list_locations` "list locations", `list_categories` "list categories", `dimensional_summarize_transactions` "total transactions", `financial_statement` "read a financial statement"; `denied` (1): `list_business_users`.
- Kick `network` (17), each labelled: `context_browse` "list workspaces", `context_resolve` "find a workspace", `financial_accounts_query` "read bank and card accounts", `transactions_query` "read transactions", `categories_query` "read categories", `classes_query` "read classes", `counterparties_query` "read who you pay and who pays you", `rules_query` "read categorising rules", `accounting_query` "read the chart of accounts", `opening_balances_query` "read opening balances", `journals_query` "read journal entries", `reports_query` "read a report", `documents_query` "list documents", `documents_download` "read a document", `entities_query` "read entities", `activity_query` "read recent changes", `tasks_query` "read bookkeeping tasks"; `denied` (22): `transactions_act`, `transactions_transfer_matches_act`, `transactions_document_links_act`, `categories_act`, `classes_act`, `counterparties_act`, `rules_act`, `accounting_act`, `account_groups_act`, `opening_balances_act`, `journals_act`, `documents_act`, `entities_act`, `activity_undo`, `tasks_act`, `organization_clients_create`, `invoices_create`, `invoices_update`, `bills_create`, `bills_update`, `list_kick_skills`, `load_kick_skill`.
- `digits_only_reads`: `http` at `https://api.digits.com/mcp`, `oauth` with no scopes; the 9 `network` exactly, each labelled; `list_business_users` `denied`; 10 in all. RED.
- `kick_only_reads`: `http` at `https://use.kick.co/mcp`, `oauth.scopes` exactly `["mcp:read"]`; the 17 `network` exactly, each labelled; the 22 `denied`; 39 in all. RED.
- `the_finance_kit_never_changes_a_service`: no tool of any of the kit's connectors is `external_effect`, and none has an allowance. Guard over Tasks 2 and 3 (vacuous before Task 2).

- [ ] `feat(roles): give the Finance Specialist Digits and Kick`

### Task 4: Connected by name

- `connects_each_finance_service_by_name` (`team.rs`, as `connects_each_marketing_service_by_name`): a Finance Specialist connected to `stripe`, `digits` and `kick` by name gets each kit entry, signed in, with no allowances. Guard (its entries exist from Tasks 2 and 3; the landing review's mutation, renaming one entry in `kit.yaml`, is what proves it).
- `live_kit_pins.rs`'s header names the three and their variables, `FARIK_KIT_STRIPE_BEARER`, `FARIK_KIT_DIGITS_BEARER` and `FARIK_KIT_KICK_BEARER`. No test change: the run already covers every shipped `http` connector.

- [ ] `test(runtime): connect the Finance Specialist's services by name`

### Task 5: Spec and plan

`docs/SPEC.md` 6.6: Stripe in the kit, signed in, with the sentence about a restricted key replaced by the decision above; Digits and Kick, read-only; the customers'-details line. 6.7's paragraph on the Finance kit: the three services, route 1, every write `denied`; the revision line. `docs/design/role-kits.md`: the Finance row (Digits and Kick in place of "a paid ledger … (Kick, Digits)", optional by being connected or not), the Signing-in rows (Stripe, Digits, Kick: route 1), the Steps row 10. `docs/design/finance-specialist.md`: the Stripe paragraph points to this step for the kit. Project plan row 10: what was executed.

- [ ] `docs(spec): record the Finance Specialist's kit`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, Stripe, Digits and Kick listed with no drift
```

The live run reads the three bearers from a sign-in through the MCP Inspector (Stripe in a sandbox). Then, by the founder: connect Stripe (sandbox) to a Finance Specialist and have it close a month of the sandbox's payments into `books.xlsx`; see no customer's name in the workbook.

## Execution notes

None yet.
