# Phase 7, step 08g: The marketing budget's hard stop

Status: draft. Its readiness review runs once step 08f has landed (ADR 0032: one round).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.5, 5.7, 6.5, 8.5; F3, F9
Depends on: step 08f of this phase (`GoogleAds`, the route's grant handling in `daemon/ads_calls.rs`, `marketing_campaign.created`, `CreatedCampaign`, `check_ads_write`); step 08c (`active_plan`, `marketing_plans`, `record_plan_end`, `marketing_plan_end`, the plan's page); phase 6 (merged in #19)
Decided by the founder, 2026-10-06: removing the Google Ads connection while a plan's campaigns run first pauses them (Farik's own call, as at a cap), then removes the connection, since Farik could no longer stop them at the budget; Remove's confirmation says so.
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from step 08f at the seam between running ads and stopping them.

## Goal

The marketing budget is a hard stop (ADR 0042). While Farik runs, a tick with no model reads each active plan's Google Ads spend every 15 minutes; when a campaign's cost reaches its plan campaign's budget, or the plan's reaches its Google Ads budget, Farik pauses the campaigns concerned itself and records `marketing_budget.reached`, and Today asks the owner to raise the budget (a new version of the plan, which the Marketing Specialist proposes and the owner approves) or end the plan. A plan that ends has the campaigns no newer version carries paused the same way. No mode, no allowance and no agent goes past it: 08f already refuses an agent's enable once the spend has reached a budget. Out of scope: a budget for posts (they cost nothing); other ad networks.

## Decisions

- **`read_marketing_spend`**, a rule with no model in `rules.rs` that `tick_within` (`orchestrator.rs:462`) runs before its pause check, after 08c's `end_marketing_plans`, on every tick whose scope names no task, so it runs while the team is paused: stopping spend is never paused. It starts no session and does not use up the tick. For each plan with a campaign recorded under it or a plan it replaces, that is active or ended with a campaign not yet paused for its end, it reads at most every 15 minutes: the last read's time is kept in the daemon's memory (`DaemonState::spend_reads`), so a restart reads at once. `farik serve` ticks at least once a minute (`RECHECK`, `orchestrator.rs:271`), so a read is at most a minute late while it runs; while no process drives the project, Google's own budgets (08f) bound the spend, as the spec says.
- **One read is one `Search`**: `SELECT campaign.resource_name, campaign.status, metrics.cost_micros FROM campaign WHERE campaign.resource_name IN (<the plan's campaigns, its replaced versions' included>) AND segments.date BETWEEN '<the first creation's UTC date less one day>' AND '<today's UTC date plus one day>'`, so the ad account's time zone cannot drop a day; cost per key and in all, in hundredths, rounded up from micros. 96 operations a day per active plan, within Explorer's 2,880 for the founder (08e records the quota for the launch).
- **Whose connection**: the plan's proposing agent's `google-ads` kit entry, else the first other active Marketing Specialist's in the team file's order, each through 08f's grant handling (`refreshed_entry`, `matches_kit`). With none usable, or Google failing, the read fails and is kept in memory with its reason; it does not count as a read, so the next tick tries again.
- **Caps**, pure in `farik_core::marketing` (`caps_reached`): a key whose cost is at least its plan campaign's budget reaches `campaign`; the plan's cost at least its `budget.google_ads` reaches `plan`, which takes every campaign of the plan. A cap already recorded for the plan (by scope and key) is not reached again. For each new cap, Farik pauses the campaigns concerned whose status is `ENABLED` with one `googleAds:mutate` of status `PAUSED` (Farik's own call, as 08d's are: no hook, a fixed act), then records `marketing_budget.reached { plan, scope: campaign | plan, key?, spent, budget, currency, paused }`, `paused` the campaigns it paused. A pause Google refuses is still recorded, with `paused` empty and `failed` Google's words cut at 300, and Today says so; every later read pauses again any campaign of a recorded cap that Google still reports `ENABLED`, and records nothing more for it.
- **A plan that ends** (by the owner, `replaced` or `expired`) stops its ads: at the next read, each of its campaigns whose key the now active plan does not carry is paused and recorded `marketing_campaign.paused { plan, key, campaign, why: plan_ended }`, once each. A campaign a newer version carries (by key, through `replaces`) keeps running under it, within that version's budget. The campaign's own end date (08f) stops it at Google in any case.
- **Raise or end, on Today.** `waiting.list` (`daemon/gates.rs`, beside `design_reviews_waiting` at `gates.rs:55`, so no store enum changes) gains, each row with the plan's proposing task as the `task_id` the schema requires, a row of kind `marketing_budget` for the active plan while a `marketing_budget.reached` exists for it: "<plan>'s ads reached their budget: <spent> of <budget> <currency>. Farik paused them." with "Raise the budget" and "End the plan"; and a row of kind `marketing_spend_unread` while the last read of an active plan with campaigns failed: "Farik can't read <plan>'s ad spend: <reason>. Google's own budget still limits it." "End the plan" sends 08c's `marketing_plan_end`. "Raise the budget" opens a dialog asking the new Google Ads budget in all, and the new budget of each campaign that reached its own, each more than its spend, then files a request through the existing `request.file` method, whose first line is the title "New version of <plan> with a raised budget" and whose text goes on: "Propose a new version of <plan> that replaces it (replaces: <plan>), with its Google Ads budget <amount> <currency>, and <key>'s <amount>; keep everything else. Its paused ads run again only after the owner approves the new version and you enable them." The request is triaged as any other (5.16). The row stays until the plan ends, which the new version's approval does. Rejected: the owner approving a raised budget directly, which would leave Farik to enable ads itself; enabling stays the agent's act inside an approved plan.
- **What the plan's page shows**: from `marketing_plan.get`, which gains `spend { read_at, total, by_key, failed? }` from the last read, and the plan's `marketing_budget.reached` and `marketing_campaign.paused`: each campaign with its spend against its budget, the plan's total against its Google Ads budget, "Read at <time>; Farik reads the spend every 15 minutes while it runs", and any pause with why.
- **The screens, mocked up first (Task 0)**: the two Today rows, the raise dialog, and the plan page's spend section, desktop and phone.
- **Events**, in `event.schema.json` and every exhaustive match: `marketing_budget.reached { plan, scope, key?, spent, budget, currency, paused, failed? }` and `marketing_campaign.paused { plan, key, campaign, why: plan_ended }`, both recorded by Farik with no agent or session on the envelope, about no contract.
- **What the spec says**: Google may spend up to one read's worth (15 minutes, and up to a minute more) past a cap while Farik runs, and while no process drives the project the total or daily budget Google keeps (08f) bounds it.

## File map

```
docs/design/mockups/{TodayAdBudget,PhoneAdBudget}.dc.html, MarketingPlan.dc.html, canvas.json   creates, modifies (Task 0)
crates/core/src/marketing.rs                                                    modifies: PlanSpend, caps_reached, to_pause_for_end (Task 1)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs}, crates/store/src/projections.rs   modifies (Task 2)
crates/store/src/marketing.rs                                                   modifies: reached and paused folded (Task 2)
crates/runtime/src/daemon.rs, daemon/ads_calls.rs                               modifies: spend_reads, the read and the pause (Task 2)
crates/runtime/src/orchestrator/rules.rs, orchestrator.rs                      modifies: read_marketing_spend (Task 2), plan ends (Task 3)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/gates.rs               modifies: the rows, spend on the plan (Task 4)
apps/web/src/pages/{Today.tsx,Today.test.tsx,MarketingPlan.tsx,MarketingPlan.test.tsx}, dialogs/RaiseBudget.tsx(+test), strings/en.ts   modifies, creates (Task 5)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md              modifies (Task 6)
```

## Interfaces

Consumes: `GoogleAds::search`, `GoogleAds::mutate`, `CreatedCampaign`, the grant handling of `daemon/ads_calls.rs` (08f); `active_plan`, `marketing_plans`, `PlanProposal`, `Amount`, `marketing_plan_end` (08c); `tick_within`, `RECHECK`, `design_reviews_waiting`, `request.file`.

Produces:

```rust
// farik_core::marketing
pub struct PlanSpend { pub by_key: BTreeMap<String, Amount>, pub total: Amount }
pub enum CapScope { Campaign, Plan }
pub struct Cap { pub scope: CapScope, pub key: Option<String>, pub spent: Amount, pub budget: Amount, pub campaigns: Vec<String> }
pub fn caps_reached(plan: &PlanProposal, created: &[CreatedCampaign], spend: &PlanSpend, recorded: &[(CapScope, Option<String>)]) -> Vec<Cap>;
pub fn to_pause_for_end(ended: &[(String, PlanProposal)], active: Option<&PlanProposal>, created: &[CreatedCampaign],
    paused: &[String]) -> Vec<CreatedCampaign>;
// farik_runtime
pub(crate) struct SpendRead { pub at: DateTime<Utc>, pub spend: Option<PlanSpend>, pub failed: Option<String> }
impl DaemonState { pub(crate) fn spend_reads(&self) -> MutexGuard<'_, BTreeMap<String, SpendRead>>; }   // by plan id
pub(crate) async fn read_spend(state: &Arc<DaemonState>, agents: &[String], account: &str, campaigns: &[String],
    from: NaiveDate, to: NaiveDate) -> Result<BTreeMap<String, (u64, String)>, String>;   // daemon::ads_calls: cost micros and status
pub(crate) async fn pause_campaigns(state: &Arc<DaemonState>, agents: &[String], account: &str, campaigns: &[String]) -> Result<(), String>;
pub(super) async fn read_marketing_spend(orchestrator: &Orchestrator) -> Result<(), OrchestratorError>;   // orchestrator::rules
```

## Tasks

### Task 0: Mockups

Today's budget row and spend-unread row, the raise dialog, the plan page's spend; the founder's approval in the Execution notes; Task 5 waits for it.

- [ ] `docs(design): mock up the marketing budget's stop and its raise`

### Task 1: Caps, decided

- `a_campaign_at_its_budget_reaches_its_cap`: 500.00 spent of 500.00 reaches `campaign` with that campaign; 499.99 does not. RED.
- `the_plan_at_its_budget_takes_every_campaign`: two campaigns under their own, together at `google_ads`, reach `plan` with both. RED.
- `a_recorded_cap_is_not_reached_again`. RED.
- `an_ended_plan_s_campaigns_are_paused_unless_carried`: a replaced plan's key the new version keeps is not listed; another key is; one already paused is not. RED.

- [ ] `feat(core): decide when the marketing budget is reached`

### Task 2: Reading the spend, and pausing

The two kinds in the schema and every exhaustive match (this commit); `spend_reads`; `read_spend` and `pause_campaigns`; `read_marketing_spend` called from `tick_within`. Against 08f's `tests/support/google_ads_fixture.rs`.

- `reads_every_fifteen_minutes_while_farik_runs`: on the paused clock, ticks at 0, 14 and 15 minutes make two `Search` requests, each exactly the GAQL of Decisions; a restart (a new `DaemonState`) reads at once. RED.
- `pauses_at_a_campaign_s_cap_and_records_it`: a cost of 500.00 against 500.00 makes one `mutate` pausing that campaign, then `marketing_budget.reached { scope: campaign, paused }`; the next read records nothing. RED.
- `pauses_everything_at_the_plan_s_cap`. RED.
- `a_refused_pause_is_recorded_and_tried_again`: Google refusing gives `failed` with its words cut; the next read pauses it. RED.
- `reads_while_the_team_is_paused`. RED.
- `falls_back_to_another_marketing_specialist_and_says_when_it_cannot_read`: the proposer's lapsed sign-in, then another agent's entry is used; with none, the read's `failed` is kept and nothing recorded. RED.
- `an_enable_after_the_cap_is_refused`: 08f's route refuses `set_campaign_status enabled` with `not_in_marketing_plan` once the read spend reaches the budget. Guard (08f's check).

- [ ] `feat(runtime): stop a marketing plan's ads at its budget`

### Task 3: A plan that ends stops its ads

- `ending_a_plan_pauses_its_campaigns`: after `marketing_plan_end`, the next read pauses each enabled campaign of it and records `marketing_campaign.paused { plan_ended }` once each. RED.
- `a_new_version_keeps_its_carried_campaigns`: a plan replaced by a version with the same key leaves that campaign running. RED.

- [ ] `feat(runtime): stop the ads of a marketing plan that ends`

### Task 4: What Today and the plan's page are told

- `waiting_lists_a_reached_budget_until_the_plan_ends`: the row with its line; gone once `marketing_plan_end` is recorded. RED.
- `waiting_says_when_the_spend_cannot_be_read`. RED.
- `the_plan_carries_its_spend_and_pauses`: `marketing_plan.get` gives `spend` from the last read and the plan's reached and paused events. RED.

- [ ] `feat(runtime): tell the owner when a marketing plan's ads reach their budget`

### Task 5: The screens

As the approved mockups.

- `today_offers_to_raise_or_end`; `raise_files_a_request_for_a_new_version` (the request's text exactly as Decisions, with the amounts typed; an amount at or below the spend is refused in the dialog); `end_sends_marketing_plan_end`; `today_says_when_the_spend_cannot_be_read`; `the_plan_page_shows_spend_against_budget`. RED each.

- [ ] `feat(web): raise or end a marketing plan whose ads reached their budget`

### Task 6: Spec and plan

`docs/SPEC.md` 6.5 (the hard stop as built, the 15-minute read, what Google may spend past a cap, the fall-back connection), 5.5 (the marketing budget beside the five: no mode, no allowance and no agent passes it), 5.7 (Today's raise or end), 8.5 (the two kinds); the revision line. `docs/design/role-kits.md`. Project plan row 08g, and step 13's marketing task as ADR 0042 says (a plan with a small Google Ads budget, approved, a post sent by the plan, its ads paused at the cap).

- [ ] `docs(spec): record the marketing budget's hard stop`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, no drift (google-ads is pinned offline by crates/cli/tests/google_ads_server.rs)
```

Then the founder's live check of steps 08e to 08g, recorded in the pull request, with Explorer access granted (08e) and a Google Ads account with billing: connect Google Ads to the Marketing Specialist by signing in with Google (the warning page of an app in Testing shows); `list_accounts` names the account; approve a three-day plan with one Search campaign of a small budget (5.00 in the account's currency) and one post slot; the agent creates the campaign (paused, its total budget at Google 5.00, its end the plan's), an ad group, keywords and an ad, and enables it; once Google reports 5.00 spent, within 16 minutes Farik pauses it and Today offers to raise or end; end the plan. Record whether Google's charge passed the cap and by how much.

## Execution notes

None yet.
