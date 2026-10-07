# Phase 7, step 08g: The marketing budget's hard stop

Status: ready once its mockups are approved
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 3, 5.5, 5.7, 5.16, 6.5, 8.5; F3, F9
Depends on: step 08f of this phase (landed and landing-reviewed at `8e6c25a`: `GoogleAds`, `spend_query`, `ads_calls.rs` with `access_of`, `grant_of`, `Writing::spend` and `ads_writes`, `marketing_campaign.created`, `CreatedCampaign`, `check_ads_write`, `campaign_budget`, `first_day`); step 08d (`hand_over.rs`, a rule with no model); step 08c (`active_plan`, `marketing_plans`, `check_proposal`, `record_plan_end`, `marketing_plan_end`, the plan's page); phase 6 step 15 (`SprintHold`, `sprint_hold`, `waits_for_a_sprint`, `in_the_backlog`, `in_the_open_sprint`, `the_sprint_pays`); phase 6 (merged in #19)
Decided by the founder, 2026-10-06: removing the Google Ads connection while a plan's campaigns run first pauses them (Farik's own call, as at a cap), then removes the connection, since Farik could no longer stop them at the budget; Remove's confirmation says so.
Decided by the founder, 2026-10-07, in conversation: (1) a pause that fails when the owner removes Google Ads, "Remove regardless": the connection is removed all the same, and the confirmation and Today say the ads keep running at Google until their end date or their budget there, and to pause them in Google Ads; (2) Google reporting cost up to about an hour late, so that Farik's stop can come about an hour past a cap: "Farik must have a plan of what exactly to advertise and get a final price before beginning. If the price is dynamic, then just dont say so before committing to a plan", and, asked to choose, "Disclose dynamic ones": each campaign states what it advertises, and the plan shows each campaign's price kind before the owner approves; (3) a raise request under "Plan work in sprints", "Skip the queue": it is worked on at once, outside sprints, as ADR 0027's incident fix is, and its row does not say it waits for a sprint.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-07 (one round, ADR 0032): not ready, 7 Blocking and the Should items, all folded below with the founder's answers; no second round
Mockups approved by: pending
Amended 2026-10-06 by ADR 0044 (replacing ADR 0043's amendment of the same day): Google Ads signs in through Farik Cloud with Farik's Google app from phase 11, so the quota below is Farik Cloud's project's, shared by every customer (Explorer's 2,880 operations a day carry about 30 active plans, a phase 11 concern), and the founder's live check of 08e to 08g is phase 11's (step 01f). The tests sign in against the fake with a table of their own, as 08e's and 08f's do.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from step 08f at the seam between running ads and stopping them.

## Goal

The marketing budget is a hard stop (ADR 0042). While a process drives the project, a watch with no model, beside the ticks, reads each active plan's Google Ads spend every 15 minutes; when a campaign's cost reaches its plan campaign's budget, or the plan's reaches its Google Ads budget, Farik pauses the campaigns concerned itself and records `marketing_budget.reached`, and Today asks the owner to raise the budget (a new version of the plan, whose request skips the sprint queue) or end the plan. A plan that ends has the campaigns the active plan does not carry paused within a minute; removing Google Ads pauses Farik's campaigns first, and is removed even when Google refuses. Before the owner approves a plan, each campaign says what it advertises and whether its price is fixed. No mode, no allowance and no agent goes past the stop: 08f already refuses an agent's enable once the spend has reached a budget. Out of scope: a budget for posts (they cost nothing); other ad networks; raising from the command line.

## Decisions

- **The watch** (Blocking 1). A tick waits for a running session to end (`hand_over.rs:8-9`; a session may run 30 minutes, SPEC 5.5), so the spend is not read in `tick_within` (`orchestrator.rs:464`). `Orchestrator::watch_marketing_spend` (`orchestrator/spend.rs`, new) runs in its own task, which `start_listening` (`crates/cli/src/start.rs`) spawns once recovery is done, so `farik serve` and `farik run` alike have it; `Driver` keeps its `JoinHandle`, the watch ends once `is_stopped`, and `Driver::finish` aborts it if it still runs. It wakes every `RECHECK` (`orchestrator.rs:273`, one minute) through `deps.sleeper`, so the tests' paused clock drives it, and at each wake handles the plans due a read, then the ended plans' pauses, holding `ads_writes` from each read through its pause. It starts no session and runs while the team is paused: stopping spend is never paused. A store error ends the watch and stops the orchestrator, so `farik serve` ends saying it, as on a tick's error; a Google, sign-in or team-file failure is the read's `failed`, and the watch goes on. Rejected: a rule in `tick_within` beside `end_marketing_plans` (`rules.rs:77`), which a running session holds up.
- **When a plan is read.** An active plan with a campaign recorded under its lineage is due when its last attempt, kept in `DaemonState::spend_reads` by plan id, is 15 minutes old or absent, so a restart reads at once. A failed read is an attempt too, tried again 15 minutes later: a retry every minute would spend Farik Cloud's shared quota (ADR 0044) on failures that rarely clear within a minute. `SpendRead` keeps the last good spend and its time beside the last failure, so the plan's page still shows what was last known, and when. An ended plan's spend is never read.
- **One read** (Blocking 2): one `Search` per ad account the lineage's campaigns are in, each 08f's `spend_query` (`google_ads.rs:1201`) over that account's campaigns, from the first creation's UTC date less one day to today's plus one, so the account's time zone cannot drop a day; costs summed per key and in all, in hundredths, rounded up from micros. The plan's cost is every campaign of the lineage's, as 08f's enabling counts it (`check_enable` sums `view.spent`); `Writing::spend` (08f) sums every account the same way (its `held` still from the enabled campaign's account), so enabling and the cap count one spend. 96 operations a day per active plan and ad account.
- **Whose connection.** The plan's proposing agent's `google-ads` entry, else each other active Marketing Specialist's in the team file's order, through `agent_access` (new, beside `access_of`, `ads_calls.rs:134`: the agent's entry that `matches_kit` and its `SecretAt`, with no session ticket) and `grant_of`. The next agent is tried when this one's grant cannot be had (no entry, not the kit's, a lapsed, unconfirmed or failed sign-in) or Google answers `NotAllowed` (this sign-in does not reach the account); Google's other answers (`Google`, `Failed`) end the read, since another sign-in meets the same and spends more quota. With none left the read fails with the last reason, cut at 300 characters.
- **Caps**, pure in `farik_core::marketing` (`caps_reached`): a key whose cost is at least its plan campaign's budget reaches `campaign`; the plan's cost at least its `budget.google_ads` reaches `plan`, which takes every campaign the active plan carries. A cap recorded for the active plan, by scope and key, is not reached again. Caps are the active plan's alone (Blocking 3): Farik pauses a recorded cap's campaigns again only while the cap's plan is the active plan, so a raised version's own budgets decide from its approval on; an ended plan is read only for `to_pause_for_end`.
- **Carried** (Blocking 2): a campaign is carried only when its plan is in the active plan's lineage, the active plan has its key, and it is in the active plan's `google_ads_account` (the resource name's customer, as `check_ads_write` reads it). `to_pause_for_end` takes the active plan's id and lineage.
- **Every pause** reads status first: one `Search` per ad account with `status_query` (new in `google_ads.rs`: `SELECT campaign.resource_name, campaign.status FROM campaign WHERE campaign.resource_name IN (<the campaigns>)`, no metrics, so a campaign with no cost still has its row), then one `googleAds:mutate` of `PAUSED` per campaign reported `ENABLED` or missing from the rows, each campaign on its own (Blocking 5: one mutate is atomic, and 08f never sets `partial_failure`); a campaign reported `PAUSED` or `REMOVED` counts as paused, with no call. Farik's own fixed act, no hook, on `google_ads.rs`'s list of operations (ADR 0042, 08f's amendment).
- **At a cap** (Blocking 5): for each new cap Farik pauses its campaigns, then records `marketing_budget.reached { plan, scope, key?, spent, budget, currency, paused, failed? }`: `paused` the campaigns Google accepted or reported paused, `failed` the first refusal's words cut at 300. While the cap's plan is active, each later read also reads the status of the cap's campaigns and pauses any `ENABLED` (a refused pause, or one enabled at Google by hand), recording `marketing_campaign.paused { why: budget_reached }` once for a campaign not in the cap's `paused`.
- **A plan that ends** (Blocking 4), by the owner, `replaced` or `expired`: at the watch's first wake after `marketing_plan.ended`, not the next 15-minute read, Farik pauses each campaign `to_pause_for_end` lists and records `marketing_campaign.paused { plan, key, campaign, why: plan_ended }` for each pause Google accepts and each campaign reported `PAUSED` or `REMOVED`; a refusal is kept as the plan's `SpendRead.unstopped`, tried again every 15 minutes, and cleared when a later pause of the plan succeeds. A campaign the active plan carries keeps running under it, within its budget; the campaign's own end (08f) stops it at Google in any case. Once every campaign of an ended plan is recorded paused for its end, the plan is not looked at again. The pause covers every ad account of the lineage and takes `ads_writes`, which every end takes first (both from 08f's landing review), so no write is in flight.
- **Removing Google Ads** (Blocking 6; the founder's answer 1). `connector.disconnect` (`connector_disconnect`, `daemon/team.rs:1270`, dispatched at `:1801`) for `google-ads` first calls `pause_before_removing`: under `ads_writes` and with this agent's grant, every campaign Farik made that is not recorded paused for its plan's end is paused as every pause is, and recorded `marketing_campaign.paused` with `why: plan_ended` for one `to_pause_for_end` lists and `connection_removed` otherwise. Then the connection is removed whatever Google answered; a refusal, or a grant that cannot be had, is kept as the plan's `unstopped`. It pauses even when another Marketing Specialist still has Google Ads: rejected, pausing only when the last connection goes, which the dialog would have to work out from the whole team for a rare team of two. `connector_disconnect` holds `&DaemonState`; the route passes the `Arc` that `refreshed_entry` needs. The Remove confirmation for Google Ads (AgentEdit's `connectorRemoveTitle` dialog) adds `connectorRemoveGoogleAds`: "Farik pauses your marketing plan's running ads first, since without this connection it could not stop them at their budget. If Google refuses, Google Ads is removed anyway, and the ads keep running at Google until their end date or their budget there; pause them in Google Ads." `farik disconnect <agent> google-ads` (`crates/cli/src/connector.rs:585`) refuses `disconnect_in_the_browser` while a campaign Farik made is not recorded paused, since with no process driving it has no daemon to pause with: "remove Google Ads on <agent>'s page in the browser, where Farik pauses its running ads first".
- **What each campaign advertises, and its price** (the founder's answer 2). This changes step 08c's proposal format: each campaign of `farik_propose_marketing_plan` gains a required `advertises`, the product, service or offer, 3 to 200 characters (`marketing_plan_campaign`, field `campaigns[<i>].advertises`); `PlanCampaign` gains it; on `marketing_plan.proposed` it is optional in `event.schema.json`, since plans proposed before have none (read back empty, shown "Not stated"). `price_kind(campaign, currency, day)`, pure, is `campaign_budget`'s kind for the campaign made on `day` (its run from `first_day` to `ends_on`; 3 to 90 days takes a total budget): `Fixed` for a total budget, which Google never bills past, `NotFixed` for a daily one, which may spend past its cap for about an hour since Google's reported cost lags. `marketing_plan.get` gives each campaign `advertises` and `price` (`fixed | not_fixed`), as of today while proposed and as of the approval day once approved; the plan's page shows "Advertises: <advertises>" and "Price: fixed at <budget> <currency>" or "Price: up to <budget> <currency>, may run over by about an hour's spend". **A fixed price stays fixed**: `AdsPlanView` gains `approved_on`, and the create refuses a key that was `Fixed` on it when `campaign_budget` now gives a daily budget ("<key> was approved at a fixed price, which Google keeps only for a run of 3 days or more; propose a new version"). Rejected: the kind at creation alone, which would turn a price the owner saw as fixed into a daily one unseen. `writing-the-marketing-plan` and `running-search-ads` each gain: "Prefer campaigns at a fixed price: 3 to 90 days, made at least two days before they start. Say in the plan's text what each campaign advertises, and which campaigns are not at a fixed price and why."
- **Raise or end, on Today.** `waiting.list` gains, beside `design_reviews_waiting` (`gates.rs:58`, so no store enum changes), one row per plan, with the plan's proposing task as the `task_id` the schema requires and the fields `plan`, `currency`, `google_ads`, `caps: [{ scope, key?, spent, budget }]` and `campaigns: [{ key, budget, spent }]`; `gates::query` gains `state: &DaemonState`, as `team::query` has (`web.rs:858-863`). By precedence: kind `marketing_budget`, the active plan with a `marketing_budget.reached`, in the newest cap's words: "<plan>'s ads reached their budget: <spent> of <budget> <currency>. Farik paused them." (a campaign's cap: "<plan>'s campaign <key> reached its budget: …"), or while a cap's campaign is unpaused "… Farik could not pause them: <failed>. It tries again every 15 minutes; pause them in Google Ads.", with "Raise the budget" and "End the plan", until the plan ends; kind `marketing_ads_running`, a plan with `unstopped`: "Farik could not pause <plan>'s ads: <reason>. They keep running at Google until <ends_on> or their budget there. Pause them in Google Ads."; kind `marketing_spend_unread`, the active plan whose last read failed: "Farik can't read <plan>'s ad spend: <reason>. Any of its ads still running keep running at Google until <ends_on> or their budget there; pause them in Google Ads." `unstopped` is memory, but the founder's warning outlives a restart: an ended plan's pause is tried again at the first wake, and an active plan's read fails while no connection is left, so one of the two rows says it again within a minute. "End the plan" sends 08c's `marketing_plan_end`.
- **The raise** (Blocking 7). "Raise the budget" opens `RaiseBudget`: the new Google Ads budget, and the new budget of each campaign that reached its own, each more than its spend, the campaigns' sum within the new Google Ads budget; `total` rises by the same amount as `google_ads`, since `check_proposal` refuses `google_ads` above `total`. It calls `marketing_budget.raise { plan, google_ads, campaigns: [{ key, budget }] }`, a write among the gates' methods (`gates.rs:43`), which checks the same (`raise_refused`), that `plan` is active with a `marketing_budget.reached`, and that no raise of it is open (`raise_open`), then files the request through `file_request` as `request.file`'s `file_words` does (`gates.rs:607`), with `task.created`'s new `raises: <plan>`, and records `request.triaged { size: small, reason: "a raised marketing budget for <plan>", triaged_by: "farik" }` at once, so no triage session runs. Its title is "New version of <plan> with a raised budget" and its text, with one "<key>'s budget" clause per campaign raised: "Propose a new version of <plan> that replaces it (replaces: <plan>), starting today, with its Google Ads budget <amount> <currency> and its total <amount>, and <key>'s budget <amount>; keep its campaigns and post slots that are not over, each with its key and what it advertises, their dates from today on. Once the owner approves it, raise each paused campaign's budget at Google with set_campaign_budget, then enable it." (`check_proposal` refuses a start before yesterday and dates outside the plan's, so "keep everything else" would be refused.) `running-search-ads` gains the last sentence. It is refined and approved as any request (5.16). Rejected: the owner approving a raised budget directly, which leaves Farik to enable ads itself; `request.file` with a mark the browser sets on free text, which would let any request skip the sprint queue.
- **The raise skips the queue** (the founder's answer 3). ADR 0027's incident fix is ADR 0028's one exception, planned in step 11e (drafted, not built), so 08g builds the exception and 11e reuses it. `TaskProjection.skips_sprints`, from `task.created`'s `raises`, is a column `task_projections.skips_sprints` added by a migration at the next free number when this step runs. `SprintHold` gains `skips_sprints`, which `sprint_hold` (`sprints.rs:93`) fills: `waits_for_a_sprint` (`gates.rs:144`) and `in_the_backlog` (`gates.rs:154`) are false for it. `AssignmentInput` (built at `transitions.rs:1189`) gains it: `in_the_open_sprint` (`gates.rs:177`) is true for it with or without the policy (without it, a standalone task outside an open sprint is held, `parent_sprint == Some(None)` being only an epic's child's), and `the_sprint_pays` (`gates.rs:204`) does not hold it. It joins no sprint, unlike 11e's fix, since a plan's raise is not product work; its own budget, the task cap and the day's still hold.
- **The plan's page.** `marketing_plan.get` gains `spend { read_at?, total?, by_key?, failed?, failed_at? }` from memory, and the plan's `marketing_budget.reached` and `marketing_campaign.paused`: each campaign's spend against its budget, the plan's total against its Google Ads budget, "Read at <time>; Farik reads the spend every 15 minutes while it runs", and each pause with why. The End confirmation (08c) adds "Farik pauses its running ads within a minute.", since the browser always has `farik serve` driving; `farik marketing plan end` (`crates/cli/src/lib.rs:1292`) handled with no process driving adds "Farik pauses <plan>'s ads when it next runs."
- **What moved** (`moved_since`, `store/src/activity.rs:283`), as 08d's posts do, since each of Farik's acts on the owner's money shows there: "Farik paused <plan>'s ads at their budget." for a `reached` with `paused`; "Farik paused <plan>'s campaign <key>: the plan ended." / "…: Google Ads was removed." / "…: it reached its budget."
- **Events**, in `event.schema.json` and every exhaustive match: `marketing_budget.reached` as above and `marketing_campaign.paused { plan, key, campaign, why: plan_ended | budget_reached | connection_removed }`, both by Farik with no agent or session on the envelope, about no contract; `task.created` gains `raises?`; `marketing_plan.proposed`'s campaigns gain `advertises?`.
- **What the spec says**: Google reports cost up to about an hour late (Google Ads Help 2544985), so a campaign at a daily budget may spend about an hour past its cap, and up to a minute more, while a process drives the project (`farik serve`; `farik run` exits when idle); a total budget is never passed; with no process, Google's own budgets (08f) bound the spend.

## File map

```
docs/design/mockups/{TodayAdBudget,PhoneAdBudget,RaiseBudget}.dc.html, {MarketingPlan,PhoneMarketingPlan,GitHubSignIn,PhoneGitHubSignIn}.dc.html, canvas.json   creates, modifies (Task 1)
crates/core/src/marketing.rs                                    modifies: advertises, PriceKind, price_kind, approved_on (Task 2); PlanSpend, caps_reached, to_pause_for_end (Task 3)
crates/runtime/src/tools/marketing.rs, docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs}   modifies: advertises (Task 2); the two kinds (Task 4); raises (Task 7)
crates/roles/roles/marketing_specialist/skills/{writing-the-marketing-plan,running-search-ads}/SKILL.md, crates/roles/src/kit.rs   modifies: the sentences, and the pin at kit.rs:1349 in `marketing_kit_carries_running_search_ads` (Tasks 2, 7)
crates/store/src/{marketing.rs,projections.rs}                  modifies: advertises read back (Task 2); reached and paused folded (Task 4); skips_sprints (Task 7)
crates/runtime/src/google_ads.rs, crates/runtime/tests/support/google_ads_fixture.rs   modifies: status_query; the fake's rows per customer and its status answers (Task 4)
crates/runtime/src/daemon.rs, daemon/ads_calls.rs               modifies: `approved_on` in `Writing::covers` (Task 2); spend_reads, agent_access, read_spend, pause_campaigns, Writing::spend (Task 4); pause_before_removing (Task 6)
crates/runtime/src/orchestrator.rs, orchestrator/spend.rs, crates/cli/src/start.rs   modifies, creates: the watch (Task 4), plan ends (Task 5)
crates/runtime/src/daemon/team.rs, crates/cli/src/connector.rs  modifies: removing Google Ads (Task 6)
crates/core/src/governor/gates.rs, crates/runtime/src/{sprints.rs,transitions.rs}, crates/store/src/migrations/<n>_skips_sprints.sql, migrations.rs   modifies, creates (Task 7)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/{gates.rs,web.rs}   modifies: price on the plan (Task 2); marketing_budget.raise (Task 7); the rows, spend, `state` (Task 8)
crates/store/src/activity.rs, crates/cli/src/lib.rs             modifies: what moved, the end's line (Task 8)
apps/web/src/pages/{Today.tsx,Today.test.tsx,MarketingPlan.tsx,MarketingPlan.test.tsx,AgentEdit.tsx,connectors.test.tsx}, apps/web/src/strings/en.ts   modifies (Task 9)
apps/web/src/pages/dialogs/{RaiseBudget.tsx,RaiseBudget.test.tsx}   creates (Task 9)
docs/SPEC.md, docs/design/{role-kits.md,marketing-specialist.md}, docs/plans/project-plan.md   modifies (Task 10)
```

## Interfaces

Consumes: `GoogleAds::search`, `GoogleAds::mutate`, `spend_query`, `resource_customer`, `access_of`, `grant_of`, `refreshed_entry`, `matches_kit`, `Writing::spend`, `ads_writes`, `CreatedCampaign`, `created_campaigns`, `campaign_budget`, `first_day`, `check_ads_write`, `AdsPlanView` (08f); `active_plan`, `marketing_plans`, `PlanProposal`, `PlanCampaign`, `Amount`, `check_proposal`, `marketing_plan_end` (08c); `tick_within`, `RECHECK`, `is_stopped`, `Driver`, `start_listening`, `design_reviews_waiting`, `file_request`, `file_words`, `connector_disconnect`, `moved_since`, `SprintHold`, `AssignmentInput`.

Produces:

```rust
// farik_core::marketing
pub struct PlanCampaign { /* 08c's fields */ pub advertises: String }
pub enum PriceKind { Fixed, NotFixed }
pub fn price_kind(campaign: &PlanCampaign, currency: &str, day: NaiveDate) -> PriceKind;
pub struct AdsPlanView<'a> { /* 08f's fields */ pub approved_on: NaiveDate }
pub struct PlanSpend { pub by_key: BTreeMap<String, Amount>, pub total: Amount }
pub enum CapScope { Campaign, Plan }
pub struct Cap { pub scope: CapScope, pub key: Option<String>, pub spent: Amount, pub budget: Amount, pub campaigns: Vec<String> }
pub struct Lineage<'a> { pub id: &'a str, pub plan: &'a PlanProposal, pub lineage: &'a [String] }
pub fn caps_reached(active: &Lineage<'_>, created: &[CreatedCampaign], spend: &PlanSpend, recorded: &[(CapScope, Option<String>)]) -> Vec<Cap>;
pub fn to_pause_for_end(active: Option<&Lineage<'_>>, created: &[CreatedCampaign], paused_for_end: &[String]) -> Vec<CreatedCampaign>;
// farik_core::governor::gates: SprintHold and AssignmentInput gain `pub skips_sprints: bool`
// farik_store: TaskProjection gains `pub skips_sprints: bool`; farik_store::marketing:
pub enum PausedWhy { PlanEnded, BudgetReached, ConnectionRemoved }
pub struct BudgetReached { pub plan: String, pub scope: CapScope, pub key: Option<String>, pub spent: Amount, pub budget: Amount,
    pub currency: String, pub paused: Vec<String>, pub failed: Option<String>, pub at: DateTime<Utc> }
pub struct CampaignPaused { pub plan: String, pub key: String, pub campaign: String, pub why: PausedWhy, pub at: DateTime<Utc> }
pub fn budgets_reached(log: &EventLog) -> Result<Vec<BudgetReached>, StoreError>;
pub fn campaigns_paused(log: &EventLog) -> Result<Vec<CampaignPaused>, StoreError>;
// farik_runtime
pub fn status_query(campaigns: &[String]) -> Result<String, GoogleAdsError>;          // google_ads
pub(crate) struct SpendRead { pub attempted_at: DateTime<Utc>, pub spend: Option<(PlanSpend, DateTime<Utc>)>,
    pub failed: Option<(String, DateTime<Utc>)>, pub unstopped: Option<String> }
impl DaemonState { pub(crate) fn spend_reads(&self) -> MutexGuard<'_, BTreeMap<String, SpendRead>>; }   // by plan id
fn agent_access(state: &Arc<DaemonState>, deps: &ToolDeps, agent_id: &str) -> Result<Access, Refusal>;   // daemon::ads_calls
pub(crate) async fn read_spend(state: &Arc<DaemonState>, agents: &[String], campaigns: &[CreatedCampaign],
    from: NaiveDate, to: NaiveDate) -> Result<BTreeMap<String, u64>, String>;            // cost micros by campaign
pub(crate) async fn pause_campaigns(state: &Arc<DaemonState>, agents: &[String], campaigns: &[String])
    -> Vec<(String, Result<(), String>)>;                                                // status first; Ok when paused or already
pub(crate) async fn pause_before_removing(state: &Arc<DaemonState>, deps: &ToolDeps, agent_id: &str) -> Result<(), String>;
impl Orchestrator { pub async fn watch_marketing_spend(&self) -> Result<(), OrchestratorError>; }
// rpc: marketing_budget.raise { plan, google_ads, campaigns: [{ key, budget }] } -> { task_id }
```

## Tasks

### Task 1: Mockups

On the canvas the earlier steps used, desktop and phone, copied to `docs/design/mockups/`: Today's `marketing_budget` row in its "paused" and "could not pause" forms; the `marketing_spend_unread` row and the `marketing_ads_running` row; the raise dialog; the plan page's spend section; the plan's approval view with each campaign's "Advertises" and price lines; the "Remove Google Ads from Kai?" confirmation (board 7 of `GitHubSignIn.dc.html` and `PhoneGitHubSignIn.dc.html`) with its new paragraph. The founder's approval, with its date and the canvas version, goes into the header's "Mockups approved by:" in the same commit; Task 9 waits for it.

- [ ] `docs(design): mock up the marketing budget's stop and its raise`

### Task 2: What each campaign advertises, and its price

Every construction of `PlanCampaign` takes `advertises`, and `Writing::covers` (`daemon/ads_calls.rs`) gives `AdsPlanView` the approval day, in this commit.

- `a_campaign_says_what_it_advertises` (`core::marketing`): a campaign with no `advertises`, or one of 2 or 201 characters, is refused `marketing_plan_campaign` on `campaigns[0].advertises`; 3 and 200 pass. RED.
- `price_kind_follows_the_total_budget_rule`: runs of 3 and 90 days from `first_day` are `Fixed`, of 2 and 91 `NotFixed`; a campaign starting tomorrow is counted from today plus two. RED.
- `a_fixed_price_is_never_made_daily`: a key `Fixed` on `approved_on` whose create today would take a daily budget is refused with "approved at a fixed price"; one `NotFixed` on it is created daily. RED.
- `the_tool_asks_what_each_campaign_advertises` (`tools/marketing.rs`): the tool refuses a campaign without it; a `marketing_plan.proposed` without it in the log reads back with `advertises` empty. RED.
- `the_plan_says_each_campaign_s_price` (`daemon/gates.rs`): `marketing_plan.get` gives `advertises` and `price` per campaign, as of today while proposed and of the approval day once approved. RED.
- `the_skills_prefer_a_fixed_price` (`kit.rs`): both skills hold the sentence of Decisions word for word. RED.

- [ ] `feat(core): say what each campaign advertises and whether its price is fixed`

### Task 3: Caps, decided

- `a_campaign_at_its_budget_reaches_its_cap`: 500.00 spent of 500.00 reaches `campaign` with that campaign; 499.99 does not. RED.
- `the_plan_at_its_budget_takes_every_carried_campaign`: two carried campaigns, each under its own budget, together at `google_ads`, reach `plan` with both; a dropped key's cost counts toward it, and its campaign is not in the cap. RED.
- `a_recorded_cap_is_not_reached_again`: with `(Campaign, "search-a")` recorded, the same spend reaches nothing; the plan's cost then reaching `google_ads` still reaches `plan`. RED.
- `an_ended_plan_s_campaigns_are_paused_unless_carried`: a key the active plan keeps in its account is not listed; the same key in another `google_ads_account`, a key the active plan dropped, and a campaign of a plan outside its lineage with a key it has are; one recorded paused for its end is not; with no active plan every unpaused campaign is. RED.

- [ ] `feat(core): decide when the marketing budget is reached`

### Task 4: The watch, the read and the pause at a cap

The two kinds in the schema and every exhaustive match (this commit); `status_query`; `spend_reads`, `agent_access`, `read_spend`, `pause_campaigns`; `Writing::spend` over every account; the watch, spawned by `start_listening`. The fake (`tests/support/google_ads_fixture.rs`) gains rows per customer and status answers.

- `reads_every_fifteen_minutes_beside_the_ticks`: on the paused clock, wakes at 0, 14 and 15 minutes make two `Search` requests per ad account, each exactly `spend_query` for that account's campaigns, one of them while a fixture session runs; a new `DaemonState` reads at once. RED.
- `sums_the_spend_of_every_ad_account`: MP-1 in one account and MP-2, replacing it, in another: one `Search` each, costs summed per key and in all; 08f's enabling is refused once that sum reaches the plan's Google Ads budget. RED.
- `pauses_at_a_campaign_s_cap_and_records_it`: 500.00 against 500.00 makes one `status_query` `Search`, then one `mutate` pausing that campaign, then `marketing_budget.reached { scope: campaign, key, paused: [it] }`; the next read records nothing and sends no mutate. RED.
- `pauses_everything_at_the_plan_s_cap`: two campaigns at `google_ads` together get one `mutate` each, then `reached { scope: plan, paused: [both] }`; a third that Google reports `PAUSED` gets no mutate and is in `paused`. RED.
- `a_refused_pause_is_recorded_and_tried_again`: Google refusing the second of two pauses gives `paused: [first]` and `failed` its words cut at 300; 15 minutes later it is paused and `marketing_campaign.paused { budget_reached }` recorded once; a third read records nothing. RED.
- `a_raised_plan_does_not_repause_the_old_cap`: a cap under MP-1; MP-2 replaces it with the key's budget raised; the agent enables the campaign; the next read sends no mutate. RED.
- `reads_while_the_team_is_paused`: with the team paused, a read is made and a cap paused and recorded as with the team running. RED.
- `the_read_holds_the_ads_lock`: while the test holds `ads_writes`, no `Search` is sent; released, the read runs (as 08f's `the_dates_end_of_a_plan_waits_for_a_google_ads_write`). RED.
- `falls_back_to_another_marketing_specialist_and_says_when_it_cannot_read`: the proposer's lapsed sign-in, then Google's `NotAllowed` for it, each fall to a second Marketing Specialist's grant; a `Google` error does not; with none left `failed` is kept beside the last good spend, nothing is recorded, and the next attempt is 15 minutes later. RED.
- `a_store_error_stops_the_watch`: a log that refuses the append ends the watch with the error, and `is_stopped` is true. RED.
- `an_enable_after_the_cap_is_refused`: 08f's route refuses `set_campaign_status enabled` once the read spend reaches the budget. Guard.

- [ ] `feat(runtime): stop a marketing plan's ads at its budget`

### Task 5: A plan that ends stops its ads

- `ending_a_plan_pauses_its_campaigns_within_a_minute`: after `marketing_plan_end`, the next wake sends one `status_query` per account (no metrics) and a pause per `ENABLED` campaign, recording `marketing_campaign.paused { plan_ended }` once each, a campaign reported `PAUSED` included; a refusal sets `unstopped` and is tried again 15 minutes later. RED.
- `a_new_version_keeps_its_carried_campaigns`: replaced by a version with the same key in the same account, the campaign gets no mutate; in another account it is paused. RED.
- `an_ended_plan_all_paused_is_not_looked_at_again`: once each campaign is recorded paused for its end, later wakes send nothing for it. RED.
- `the_end_pause_holds_the_ads_lock`: as Task 4's. RED.

- [ ] `feat(runtime): stop the ads of a marketing plan that ends`

### Task 6: Removing Google Ads pauses first

- `removing_google_ads_pauses_first` (`daemon/team.rs`): with an active plan's campaign and an ended plan's `ENABLED`, `connector.disconnect` sends both pauses before the entry leaves the team file, recording `connection_removed` and `plan_ended`. RED.
- `removing_google_ads_goes_on_when_google_refuses`: with Google refusing, the entry is removed, its keys deleted, and the plan's `unstopped` holds Google's words. RED.
- `farik_disconnect_sends_google_ads_to_the_browser` (`cli`): refused `disconnect_in_the_browser` while a campaign is not recorded paused; once all are, it disconnects. RED.

- [ ] `feat(runtime): pause Farik's ads before Google Ads is removed`

### Task 7: The raise, out of the sprint queue

- `a_raise_never_waits_for_a_sprint` (`governor/gates.rs`): under the policy a `ready` row with `skips_sprints` neither waits nor is in the Backlog; `in_the_open_sprint` is true for it while another sprint is open, with the policy and without; `fits_the_open_sprint` holds with the sprint's budget spent. RED.
- `the_mark_is_kept_on_the_row` (`projections.rs`): `task.created` with `raises` gives `skips_sprints` true through the new migration; without it, false. RED.
- `raise_files_a_request_that_skips_triage` (`daemon/gates.rs`): `marketing_budget.raise` files a draft whose title and text are exactly those of Decisions with the amounts, `raises`, and Farik's `request.triaged`; refused `raise_refused` for a plan with no cap, a budget at or below its spend, and campaigns above the new Google Ads budget, and `raise_open` while one is open. RED.
- `a_raise_runs_during_a_sprint_it_is_not_in` (orchestrator): under the policy, with a sprint open, the refined raise is assigned and no `sprint.planned` names it. RED.

- [ ] `feat(runtime): file a raised marketing budget that skips the sprint queue`

### Task 8: What Today, the plan's page and the command line are told

- `waiting_lists_a_reached_budget_until_the_plan_ends`: the `marketing_budget` row with its fields and line, in its "could not pause" form while a cap's campaign is unpaused; gone once `marketing_plan_end` is recorded. RED.
- `waiting_says_when_ads_keep_running`: `marketing_ads_running` for an ended plan's refused pause and after a removal's, in place of any `marketing_spend_unread` for the same plan; after a removal and a restart (a new `DaemonState`), the active plan's `marketing_spend_unread` row still says its ads keep running. RED.
- `waiting_says_when_the_spend_cannot_be_read`: the row's line with the read's reason. RED.
- `the_plan_carries_its_spend_and_pauses`: `marketing_plan.get` gives `spend` (the last good read and its time beside a later failure) and the plan's reached and paused events. RED.
- `says_what_moved_with_the_ads` (`activity.rs`): each line of Decisions. RED.
- `ending_with_no_process_says_when_ads_pause` (`cli`): `farik marketing plan end` handled here prints the sentence. RED.

- [ ] `feat(runtime): tell the owner when a marketing plan's ads reach their budget`

### Task 9: The screens

As the approved mockups.

- `today_offers_to_raise_or_end`; `raise_sends_the_raised_budget` (`RaiseBudget.test.tsx`: `marketing_budget.raise` with the typed amounts; an amount at or below its spend, and campaigns above the Google Ads budget, refused in the dialog with no call); `end_sends_marketing_plan_end`; `today_says_when_ads_keep_running`; `today_says_when_the_spend_cannot_be_read`; `the_plan_page_shows_spend_against_budget`; `the_plan_shows_what_each_campaign_advertises_and_its_price`; `the_end_confirmation_says_the_ads_pause`; `removing_google_ads_says_it_pauses_first` (`connectors.test.tsx`). RED each.

- [ ] `feat(web): raise or end a marketing plan whose ads reached their budget`

### Task 10: Spec and plan

`docs/SPEC.md` 6.5 (the hard stop as built: the watch, the 15-minute read across ad accounts, the pauses, the fall-back connection, removing Google Ads, a campaign's `advertises` and price kind, and what Google may spend past a cap, as Decisions say), 3 (a raise skips the sprint queue), 5.5 (the marketing budget beside the five: no mode, no allowance and no agent passes it), 5.7 (Today's rows and the raise), 5.16 (a request Farik triages), 8.5 (the two kinds, `raises`, `advertises`); the revision line. `docs/design/role-kits.md`; `docs/design/marketing-specialist.md` (`advertises` in the proposal, the price kind, the events list, a step-table row 08g, and 08f's row without the stop). `docs/plans/project-plan.md`: a row 08g after 08f, which has none yet, taking row 08f's "the 15-minute spend read …, Today's raise or end"; step 13's marketing task as ADR 0042 says.

- [ ] `docs(spec): record the marketing budget's hard stop`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

The founder's live check of steps 08e to 08g is phase 11's (step 01f, ADR 0044), through Farik Cloud: approve a three-day plan with one Search campaign of 5.00 at a fixed price; once Google reports 5.00 spent, Farik pauses it within 16 minutes and Today offers to raise or end; record whether Google's charge passed the cap, and by how much.

## Execution notes

None yet.
