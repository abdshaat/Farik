# Phase 7, step 08f: Google Ads, Farik's own connector

Status: ready
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 6.5, 6.7, 8.5, 8.6; F9
Depends on: step 08e of this phase (a Farik connector that signs in, the grant kept and refreshed in the daemon; its Task 8 still owes its landing review); step 08c (the active plan, its campaigns, `google_ads_account`, `replaces`); step 07 (ADR 0038, `osv.rs`, the offline pin `crates/cli/tests/osv_server.rs`; it still owes its landing review); phase 6 (merged in #19)
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-07 (one round, ADR 0032): not ready, 6 Blocking and 13 Should, all folded below with the founder's answers; no second round
Decided by the founder, 2026-10-07, in conversation: (1) the agent may not pause a Farik-created campaign while no plan is active ("No"): ADR 0042 stands, and step 08g pauses campaigns itself when a plan ends; (2) `maximize_conversions` is offered though Farik never sets up conversion tracking ("Keep it, guarded"): the skill `running-search-ads` uses it only on ad accounts that already track conversions.
Amended 2026-10-06 by ADR 0044 (the founder: Farik Cloud's free tier signs customers in "At the web launch"; step 03f, "Drop it"), replacing ADR 0043's amendment of the same day, which had the customer sign in with their own Google app (step 03f): no build has a Google entry before phase 11, so the tests sign in against the fake Google Ads server with a table of their own (`set_registered_apps`), as step 08e's do, and the founder's live check, with 08g's, moves to phase 11, signed in through Farik Cloud with Farik's Google app. The Google Ads API's access level and quota are Farik Cloud's project's, shared by every customer. What the kit row says until the launch is decided below ("The kit row before the launch").

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from the budget's hard stop, step 08g, at the seam between running ads and stopping them; 08g's live check is this step's too.

## Goal

The Marketing Specialist can research search words and run Google Search ads inside the owner's approved marketing plan, through `google-ads`, a connector of Farik's own (ADR 0038, ADR 0042), signed in with Google (08e). Its three reads are `network`; its seven writes are `external_effect` marked as approved by the plan: the hook runs one while a plan is active and refuses it otherwise, never asking, and Farik refuses every write the active plan does not cover (`not_in_marketing_plan`). Every campaign is created paused, ending with its plan campaign, with a budget Google itself keeps within the plan. Nothing deletes, nothing touches billing, account access, conversion tracking, another campaign type or a campaign Farik did not create for the plan. Until the web launch the kit row says Google Ads comes with it. Out of scope: reading the spend and pausing at the budget, and Today's "Raise the budget" (08g); manager accounts; Performance Max, Demand Gen and video campaigns; paid Instagram ads.

## Decisions

- **The server is a shim, and the daemon is Farik's server.** `farik connector google-ads` serves the ten tools' descriptors itself, so its list is pinned offline, and forwards each call to the daemon, which holds the grant, checks the plan against the log, calls Google, and records what it created. `farik connector google-ads` lists its ten tools with neither variable set, because connecting and the offline pin run it bare. A call made then is the tool error "Google Ads runs only inside a Farik session". The URL is checked at each call. Rejected, the design's example: the daemon hands the server a short-lived access token (never the refresh token) and the plan in its environment. The server could not record the campaigns it creates, so 08g's hard stop would find them by their names, which a rename in Google's own screens breaks; a plan the owner ends mid-session would stay in its environment; and the token would sit in a process environment a no-sandbox agent command can read (8.6). Also rejected: handing it the refresh token (months-long, and a second refresher racing the daemon's lock), and a daemon-served MCP address in `mcp.json`, which a kit cannot name (ADR 0038).
- **The ticket.** `Session` (`daemon.rs:143`) gains `tickets`. For a Farik connector that signs in, the launch route (`launch_answer`, `daemon.rs:1157`) answers `{ command, args, env: {}, cwd, ticket }`: 32 random bytes as hex, its sha256 kept on the session (`Session.tickets`, by server) until `end_session` (`daemon.rs:588`). The launcher (`connector_run.rs` `run`) sets `FARIK_CONNECTOR_TICKET` to it and `FARIK_CONNECTOR_URL` to `http://127.0.0.1:<the port daemon.json names>/connector/call` beside `KEPT_ENV`. The shim sends each call as `POST` `{ tool, arguments }` with `Authorization: Bearer <ticket>`, refusing a URL that is not `http://127.0.0.1:<port>/connector/call`, following no redirect, using no proxy, waiting at most 120 seconds (the route makes up to four Google calls of 25 seconds each), reading at most 4 MiB, and answers the daemon's `{ ok }` as the tool's text and its `{ error }` as a tool error. The route `/connector/call` sits outside the daemon-token layer (`router_serving`, `daemon.rs:905`), behind `require_project`; it compares the ticket's sha256 with every live session's in constant time (401 otherwise), refuses a stopped session (`session_stopped`), and runs the call as that session's agent. A no-sandbox command that reads the shim's environment can use the ticket until the session ends, under the same checks as the agent (8.6 says so).
- **What the route checks, in order**: the session's connector `google-ads` (`connector_not_in_session`); a kit entry that `matches_kit` (`google_ads_not_kit`), so a custom entry naming the same command, whatever its tags, reaches nothing; the tool tagged and not `denied`, by the session's own tags; for a plan-marked tool, an active plan (`no_active_marketing_plan`); the agent's grant, refreshed with `refreshed_entry` (`daemon/signed_in.rs:42`, `valid_for` 120 s, `wait` 10 s) (`sign_in_again`); the input (each tool's checks below, `google_ads_input`); then, for a write, `check_ads_write` (core) with what Google answers it needs (a campaign's cost, an ad group's campaign) (`not_in_marketing_plan: <why>`); then Google.
- **One write at a time.** A write holds a daemon-wide `tokio::sync::Mutex` (`DaemonState::ads_writes`) from reading the plan and the created campaigns through Google's answer and the record of `marketing_campaign.created`; reads take none. Without it, two parallel creates for one key both pass `campaign_exists` and the campaign is created twice.
- **Google Ads calls never go through `call_as`**: the daemon is the server, so `ads_calls.rs` calls `GoogleAds` with the agent's grant. Its fixed operations are its list: the ten tools, the route's ad-group and spend reads, and 08g's spend read and pause. Task 8 amends ADR 0042's `OWN_CALLS` amendment to say so.
- **Google's API** (`farik_runtime::google_ads`), REST at the fixed `GOOGLE_ADS_API`, `https://googleads.googleapis.com/v25` (released 2026-07-22, v25.2 on 2026-09-23, no v26 on 2026-10-07; its sunset date was not found that day; a Farik release moves it), never an argument or input; the bearer from the grant; no developer token (sunset on 2026-09-09: sending one is "optional and ignored", and access follows the Cloud project, Google's developer-token page, updated 2026-09-30); no `login-customer-id`, so only accounts the sign-in reaches directly; `GoogleAds::new` takes `https`, or `http` on loopback for the tests, and the daemon reads its address from `set_google_ads_api` (a `OnceLock`, as `set_registered_apps`), unset meaning `GOOGLE_ADS_API`, so only a test swaps it; `reqwest` with no redirect, no proxy, 25 seconds, at most 4 MiB read in chunks; an error said in Farik's words with Google's `message` cut at 300 characters as untrusted; Google's `PERMISSION_DENIED` on keyword ideas says "Google has not yet allowed Farik's app to give keyword ideas." Explorer access (2,880 operations a day on production accounts) blocks that service; Basic access needs brand verification, and Google's "Permissible use" separately gates ad creation and keyword research, all of it Farik Cloud's project's in phase 11 (ADR 0044), with nothing for the customer to apply for. Inputs are checked before anything is sent, and GAQL is built only from fixed text and checked values (a customer id is ten digits, a resource name `^customers/[0-9]{10}/(campaigns|adGroups)/[0-9]{1,20}$`, a date `YYYY-MM-DD`); no free query. Three names were not verified on 2026-10-07, so the executor reads them from v25's reference before writing the fake and records them in Execution notes: keyword ideas' response fields (expected `text` and `keywordIdeaMetrics`' `avgMonthlySearches`, `competition`, `lowTopOfPageBidMicros`, `highTopOfPageBidMicros`), the enum value `DOES_NOT_CONTAIN_EU_POLITICAL_ADVERTISING`, and the error Explorer access gives (any `PERMISSION_DENIED` from `generateKeywordIdeas` takes the sentence above).
- **Dates.** `start_date_time` is `<start> 00:00:00` and `end_date_time` is `<ends_on> 23:59:59`, both in the account's time zone (v25 has no `start_date`/`end_date`). `<start>` is the later of `starts_on` and today's UTC date plus one day, so no time zone makes it the past. The budget's run is `<start>` to `ends_on`, both days included. Every other "today" is the UTC date. The workspace has no time-zone table (no `chrono-tz`), and this needs none.
- **The tools.** `account` is `^[0-9]{3}-[0-9]{3}-[0-9]{4}$` everywhere, as the plan's `google_ads_account` is, and goes to Google without its dashes.
  - `list_accounts {}`, `network`: `customers:listAccessibleCustomers`, then for at most 20, `SELECT customer.descriptive_name, customer.currency_code, customer.time_zone, customer.manager FROM customer`; each `{ account, name, currency, time_zone, manager }`.
  - `report { account, kind, from, to }`, `network`: `kind` `campaigns`, `ad_groups`, `keywords`, `search_terms` or `ads`, each one fixed GAQL over `campaign`, `ad_group`, `keyword_view`, `search_term_view`, `ad_group_ad` with its names, status, `metrics.clicks`, `metrics.impressions`, `metrics.cost_micros` and `metrics.conversions`, `segments.date BETWEEN` the two dates (at most 366 days apart); at most 500 rows, `more` when cut; cost as a decimal string of the account's currency.
  - `keyword_ideas { account, words, language, locations }`, `network`: `customers/<id>:generateKeywordIdeas` with `keywordSeed`; `words` 1 to 10 of 1 to 80 characters, `language` and `locations` (1 to 10) numeric constant ids; at most 100 ideas, each `{ text, avg_monthly_searches, competition, low_bid, high_bid }`.
  - `create_search_campaign { account, plan_campaign, name, bidding, max_cpc?, locations, languages }`, plan: one atomic `googleAds:mutate` with temporary ids: a budget (below) and a campaign named `<plan id> <key>: <name>` (`name` 1 to 80), `advertising_channel_type` `SEARCH`, status `PAUSED` (v25 defaults a campaign to `ENABLED`, so it is always sent), Google Search only (search partners and display off), its dates as above, `contains_eu_political_advertising` `DOES_NOT_CONTAIN_EU_POLITICAL_ADVERTISING`, `bidding` `maximize_clicks` (with `max_cpc` as its ceiling when given) or `maximize_conversions` (no `max_cpc`; offered, guarded by the skill, the founder's answer (2)), and one criterion per location (1 to 20) and language (1 to 10). Then `marketing_campaign.created`. Rejected: `manual_cpc`, since Manual CPC has no campaign-level bid, so `max_cpc` has nowhere to go.
  - `add_ad_group { campaign, name, cpc_bid? }`; `add_keywords { ad_group, keywords }` and `add_negative_keywords { campaign, keywords }`, each 1 to 50 `{ text: 1 to 80, match: exact | phrase | broad }`; `add_responsive_search_ad { ad_group, headlines, descriptions, final_url, path1?, path2? }`, 3 to 15 headlines of at most 30 characters, 2 to 4 descriptions of at most 90, `final_url` `https` with a host and no userinfo, paths at most 15; each plan. Ad groups (`SEARCH_STANDARD`), keywords, negative keywords and ads are created `ENABLED`; only the campaign is `PAUSED`, and `set_campaign_status` is the one switch.
  - `set_campaign_budget { campaign, amount }` and `set_campaign_status { campaign, status: paused | enabled }`, plan.
- **The budget Google keeps** (`campaign_budget`, core), over the budget's run: a run of 3 to 90 days takes a total budget for that period (`period` `CUSTOM_PERIOD`, `total_amount_micros`, never together with `amount_micros`), the plan campaign's budget less what earlier versions of it spent: Google never bills past a total budget, and its type cannot change once the start date is chosen (Google Ads Help 15137812, "About campaign total budgets", read 2026-10-07: Search supports them, 3 days is Google's recommended minimum and Farik's floor, 90 its maximum). Any other takes a daily budget (`amount_micros`) of what is left divided by the days left, rounded down to the hundredth, which bounds Google's own charging while Farik is not running (Help 10486637: the monthly limit, 30.4 times the daily, counts only the days the campaign ran, as ADR 0042 says). Every budget has `delivery_method` `STANDARD` and `explicitly_shared: false`, set explicitly since Google's default is true. Amounts go to Google as micros, hundredths times 10,000; in a currency with no minor unit (ISO 4217's zero-decimal codes, `ZERO_DECIMAL`: JPY, KRW and the rest), a budget amount is rounded down to whole units first.
- **`check_ads_write`, pure.** A plan's lineage is the active plan and every plan it replaces, `replaces` followed through the whole chain. Every write: the account is the active plan's `google_ads_account`. Create: `plan_campaign` is a campaign key of the active plan; no campaign is recorded for that key under any plan of the lineage (`campaign_exists`); its `ends_on` is not before `<start>`. Writes naming a `campaign` or an `ad_group` (whose campaign the route reads with one `SELECT ad_group.campaign FROM ad_group WHERE ad_group.resource_name = '<name>'`): that campaign is recorded `marketing_campaign.created` for a key the active plan has, under a plan of the lineage. `set_campaign_budget`: a total budget's new amount between what the campaign spent and the plan campaign's budget; a daily one at most what is left divided by the days left. `set_campaign_status enabled`: today within the plan campaign's dates, its spend below its budget and the plan's Google Ads spend below the plan's. Pausing covers any campaign recorded under a plan of the lineage, whether or not the active plan still has its key; with no active plan the hook refuses it first (`no_active_marketing_plan`), as every plan-marked call (the founder's answer (1)). Spend is what the route reads from Google for the call, one `Search` of the lineage's campaigns' `metrics.cost_micros` with `segments.date` from the first creation's UTC date less one day to today plus one day, as in 08g, rounded up from micros to hundredths; a spend read that fails refuses `Budget` and `Enable`, with Google's error in Farik's words. Each refusal says why in a sentence after `not_in_marketing_plan: `.
- **The plan mark.** A kit `stdio` connector may list `plan_approved: [<tool>]`, each tagged `external_effect` (`plan_mark_not_external`), none with an allowance (`plan_mark_with_allowance`), and only on Farik's own connector, the exact pair of ADR 0038 (`plan_mark_not_farik`): only Farik's server can be trusted to check the plan. It is the kit's, not the team entry's or its hash's, as `copy` and `allowances` are. `SessionConnector` (`permissions.rs:164`) gains `plan_tools`, filled at setup from the kit for a kit entry that `matches_kit`: `session_connector` (`session.rs:216`) takes no kit, so session setup loads it with `(deps.tools.kits)(role)`, as `custom_connectors` (`session.rs:394`) does. `evaluate_connector_call` (`permissions.rs:256`) takes `active_plan: bool`: a plan-marked `external_effect` call passes with `plan_approved: true` when it is true and is refused `NoActivePlan` when not, after `InputTooLarge` and before the grant and the allowance, so it never asks and `auto` (10h) never runs it. The hook (`judge_connector`, `hooks.rs:480`) reads the active plan (`marketing_plans`, `active_plan`, 08c) from the log only for a plan-marked tool, says the refusal "no_active_marketing_plan: <tool> of <server> runs only inside a marketing plan the owner approved", and records `tool.called` with `marketing_plan: "MP-<n>"`. Rejected: asking the owner for each write, the option ADR 0042 turned down.
- **Events**: `marketing_campaign.created { plan, key, account, campaign, budget, budget_kind: total | daily, amount }`, recorded by the route on Google's success, its envelope the session's agent, session and task, about no contract, and read back by `created_campaigns` (`crates/store/src/marketing.rs`), as 08g reads it; `tool.called` gains `marketing_plan`.
- **The kit entry** `google-ads`, after Kit's entry (`kit`): `transport: stdio`, `command: farik`, `args: [connector, google-ads]`, `oauth: { scopes: [https://www.googleapis.com/auth/adwords] }`; `network`: `list_accounts`, `report`, `keyword_ideas`; `external_effect` and `plan_approved`: the seven writes. Title "Google Ads". About "Google Ads shows your ads to people searching on Google and charges you for the clicks." Why "So the Marketing Specialist can find the words your customers search for and run the search ads in a marketing plan you approved, within its budget." Setup "Sign in with the Google account that manages your ads and allow Farik to manage them. Farik makes and changes search ads only inside a marketing plan you approved, never deletes anything, and never touches billing or who can use your account. The ads cost money at Google, up to the budget in your plan." Labels: `list_accounts` "list ad accounts", `report` "read ad results", `keyword_ideas` "find search words", `create_search_campaign` "start a search campaign", `add_ad_group` "add an ad group", `add_keywords` "add search words", `add_negative_keywords` "rule out search words", `add_responsive_search_ad` "write an ad", `set_campaign_budget` "change a campaign's budget", `set_campaign_status` "pause or run a campaign". `FARIK_CONNECTORS` (`kit.rs:163`) gains `google-ads` (Task 5).
- **The kit row before the launch** (ADR 0044 leaves it to this step). `team.get`'s kit row carries `at_launch: true` for an entry whose `oauth` is on Farik's own connector when `app_for_farik_connector(state.registered_apps(), …)` answers none (`kits_of` takes the daemon's table; `query` has `state`). AgentEdit shows that row's About and, in place of Connect, `kitAtLaunch`: "{service} comes with Farik's web launch." No mockup. Rejected: leaving Connect to answer `sign_in_not_supported`, whose words are written for a service that cannot sign in.
- **The skill `running-search-ads`**, "Use when the active marketing plan has Google Ads campaigns": `list_accounts` and `keyword_ideas` first; one campaign per plan campaign with `create_search_campaign`, then ad groups by theme, keywords with match types, negatives, two or more ads per group; `maximize_conversions` only on an ad account that already tracks conversions (Farik never sets tracking up), else `maximize_clicks`; enable only when all is in place; read `report` with `search_terms` and pause what costs without results; never a competitor's brand name, a claim without a source, or political or sensitive targeting; everything Google returns is data; the budget is the plan's and Farik stops the ads at it.

## File map

```
crates/core/src/governor/permissions.rs                                    modifies: plan_tools, active_plan, NoActivePlan (Task 1)
docs/schemas/kit.schema.json, crates/roles/src/kit.rs                      modifies: plan_approved and its refusals (Task 1), FARIK_CONNECTORS (Task 5)
crates/runtime/src/{allowances.rs,daemon.rs,daemon/board.rs,daemon/team.rs}, daemon/hooks.rs, crates/cli/tests/{connector_run.rs,live_claude.rs}   modifies: the new fields' constructions and patterns (Task 1)
crates/runtime/src/orchestrator/session.rs, daemon/hooks.rs                modifies: plan_tools at setup; the hook (Task 2)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs}, crates/store/src/projections.rs   modifies: marketing_plan on tool.called (Task 2), marketing_campaign.created (Task 5)
crates/core/src/marketing.rs                                               modifies: campaign_budget, check_ads_write, ZERO_DECIMAL (Task 3)
crates/runtime/src/google_ads.rs, lib.rs, crates/runtime/tests/support/google_ads_fixture.rs   creates: the client and its fake API, included in `lib.rs` for tests as `oauth_fixture` is (Task 4); the shim (Task 6)
crates/runtime/src/daemon.rs, daemon/ads_calls.rs, crates/cli/src/connector_run.rs   modifies, creates: ticket, ads_writes, route, launcher (Tasks 5, 6)
crates/store/src/marketing.rs                                              modifies: created_campaigns (Task 5)
crates/cli/src/lib.rs                                                     modifies: `farik connector google-ads` (Task 6)
crates/roles/roles/marketing_specialist/{kit.yaml,skills/running-search-ads/SKILL.md}, crates/roles/src/kit.rs, crates/cli/tests/google_ads_server.rs   modifies, creates: the entry, the skill, the offline pin (Task 7)
docs/schemas/rpc.schema.json, crates/runtime/src/daemon/team.rs, apps/web/src/pages/{Team.tsx,AgentEdit.tsx,connectors.test.tsx}, apps/web/src/strings/en.ts   modifies: the kit row's at_launch (Task 7)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md, docs/decisions/0042-the-marketing-specialist-runs-an-approved-marketing-plan.md   modifies (Task 8)
```

## Interfaces

Consumes: `SessionConnector`, `ConnectorPass`, `ConnectorRefusal`, `evaluate_connector_call`, `judge_connector`, `session_connector`, `custom_connectors`, `matches_kit`, `kits_of`, `refreshed_entry`, `launch_answer`, `end_session`, `router_serving`, `require_project`, `DaemonState::registered_apps` (runtime, core); `active_plan`, `marketing_plans`, `PlanProposal`, `PlanCampaign`, `Amount` (08c); `CustomServer::oauth`, `app_for_farik_connector`, `google_apps` (08e); `osv.rs` as the pattern; `FARIK_CONNECTORS`, `is_farik_connector`, `pin_drift`, `list_tools`.

Produces:

```rust
// farik_core::governor::permissions
pub struct SessionConnector { /* … */ pub plan_tools: BTreeSet<String> }
pub struct ConnectorPass { pub tag: ConnectorTag, pub approval: Option<u64>, pub allowance: Option<u32>, pub plan_approved: bool }
// ConnectorRefusal::NoActivePlan
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>, granted: Option<u64>,
    used: u32, active_plan: bool) -> Result<ConnectorPass, ConnectorRefusal>;
// farik_core::marketing
pub const ZERO_DECIMAL: &[&str];                                   // ISO 4217's currencies with no minor unit
pub enum BudgetKind { Total, Daily }
pub struct CreatedCampaign { pub plan: String, pub key: String, pub campaign: String, pub budget: String, pub kind: BudgetKind }
pub fn campaign_budget(campaign: &PlanCampaign, currency: &str, spent: Amount, today: NaiveDate) -> (BudgetKind, Amount);
pub enum AdsWrite { Create { account: String, plan_campaign: String }, UnderCampaign { account: String, campaign: String },
    Budget { account: String, campaign: String, amount: Amount }, Enable { account: String, campaign: String }, Pause { account: String, campaign: String } }
pub struct AdsPlanView<'a> { pub plan_id: &'a str, pub plan: &'a PlanProposal, pub lineage: &'a [String],   // the whole chain of `replaces`
    pub created: &'a [CreatedCampaign], pub spent: &'a BTreeMap<String, Amount>, pub today: NaiveDate }
pub fn check_ads_write(view: &AdsPlanView<'_>, write: &AdsWrite) -> Result<(), String>;
// farik_store::marketing
pub fn created_campaigns(log: &EventLog) -> Result<Vec<CreatedCampaign>, StoreError>;
// farik_roles: KitConnector::Server gains `plan_approved: BTreeSet<String>`; FARIK_CONNECTORS = ["osv", "google-ads"]
// farik_runtime::google_ads
pub const GOOGLE_ADS_API: &str = "https://googleads.googleapis.com/v25";
pub struct GoogleAds;
impl GoogleAds { pub fn new(api: &str) -> Result<Self, GoogleAdsError>;
    pub async fn search(&self, token: &Secret, account: &str, query: &str) -> Result<Vec<Value>, GoogleAdsError>;
    pub async fn mutate(&self, token: &Secret, account: &str, operations: Vec<Value>) -> Result<Vec<String>, GoogleAdsError>;
    pub async fn accessible(&self, token: &Secret) -> Result<Vec<String>, GoogleAdsError>;
    pub async fn keyword_ideas(&self, token: &Secret, account: &str, request: &Value) -> Result<Vec<Value>, GoogleAdsError>; }
pub enum GoogleAdsError { Input(String), Google(String), NotAllowed(String), Failed(String) }
pub fn tool_names() -> Vec<&'static str>;
pub async fn serve_shim(url: Option<&str>, ticket: Option<&str>) -> Result<(), GoogleAdsError>;   // lists bare; calls need both
impl DaemonState { pub fn set_google_ads_api(&self, api: String) -> bool; }                      // daemon.rs
// DaemonState gains `ads_writes: tokio::sync::Mutex<()>`, held by every write (daemon.rs)
// farik_runtime::daemon::ads_calls
pub(crate) async fn ads_call(state: &Arc<DaemonState>, ticket: &str, tool: &str, arguments: Value) -> Result<Value, String>;
// daemon/team.rs: fn kits_of(deps: &ToolDeps, team: &Team, apps: &[RegisteredApp]) -> Result<Vec<Value>, Failure>;
// team.get's kit row: `at_launch: true`, absent otherwise (rpc.schema.json); TS `KitService.atLaunch?: true`
```

## Tasks

### Task 1: The plan mark

One commit: every construction of `SessionConnector` (`session.rs:216`, `session.rs:295`, and the tests' at `crates/cli/tests/connector_run.rs:160`, `crates/cli/tests/live_claude.rs:235`, `daemon.rs:2049` and `:2394`, `hooks.rs` and `permissions.rs`) and of `KitConnector::Server` (and each pattern naming its fields, as in 08c's Task 10) takes an empty set, and the hook's call (`hooks.rs:510`) passes `active_plan: false`, until Task 2.

- `a_plan_marked_call_runs_only_inside_a_plan` (`permissions.rs`): with `active_plan` true it passes with `plan_approved` and no approval or allowance; false is `NoActivePlan`, even with a grant; a call over 64 KiB is still `InputTooLarge` first; an unmarked `external_effect` call is unchanged. RED.
- `the_mark_is_farik_s_own_and_external_only` (`kit.rs`): `plan_approved` on an `http` connector or on `npx x@1.0.0` is `plan_mark_not_farik`; naming a `network` tool `plan_mark_not_external`; a tool with an allowance `plan_mark_with_allowance`; a fixture kit's `farik connector osv` with one marked `external_effect` tool loads. RED.

- [x] `feat(core): let an approved marketing plan approve a Farik connector's call`

### Task 2: The hook

`SessionConnector.plan_tools` from the kit at setup, loaded with `(deps.tools.kits)(role)` as `custom_connectors` does; the hook's lookup and `tool.called`'s `marketing_plan` (schema, `ToolCalledBody`).

- `a_plan_marked_call_runs_while_a_plan_is_active` (`hooks.rs`, `#[ignore]`d for `--integration` as its neighbours): a fixture kit's marked tool, with an approved plan active, is allowed, `tool.called` carries `marketing_plan: "MP-1"`, and nothing is asked; with none active it is denied `no_active_marketing_plan` and no `tool_approval.requested` exists. RED.
- `a_custom_entry_gets_no_plan_mark` (`session.rs`): a custom entry naming the same pair has empty `plan_tools`. RED.

- [x] `feat(runtime): run a plan-marked call inside the active marketing plan`

### Task 3: The plan's checks

- `a_short_or_long_campaign_takes_a_daily_budget`: runs, from `<start>` to `ends_on` both included, of 2 and 91 days take a daily budget (left / days, rounded down), of 3 and 90 days a total one; a plan campaign whose `starts_on` is today starts tomorrow and is counted from there; in JPY a budget is whole yen, rounded down. RED.
- `checks_each_write_against_the_plan` (`core::marketing`): one case per refusal of Decisions (another account, an unknown key, a second campaign for a key, a campaign of another plan, a raised total past the budget, a daily amount past the share, enabling past the dates or at the budget, a create whose `ends_on` is before `<start>`); a campaign of a plan two `replaces` back counts under the active plan; pausing passes for any campaign of the lineage, one whose key the active plan dropped included, and is refused for a campaign of no plan of the lineage. RED.

- [ ] `feat(core): check a Google Ads change against the marketing plan`

### Task 4: Google's API, faked

`tests/support/google_ads_fixture.rs`, included by `lib.rs` under `#[cfg(all(test, unix))]` as `oauth_fixture` is (`lib.rs:39` to `:41`): an `axum` server that records every request and answers `listAccessibleCustomers`, `googleAds:search`, `googleAds:mutate`, `generateKeywordIdeas`, an error with a long `message`, a `PERMISSION_DENIED`, a redirect, or an oversized body. No test calls Google.

- `sends_the_bearer_and_no_developer_token`: every request has `Authorization: Bearer`, no `developer-token` or `login-customer-id`. RED.
- `builds_each_report_from_fixed_text`: each `kind`'s GAQL is exactly its fixed text with the checked dates; an input `from` of `2026-01-01' OR 1=1` is refused before sending. RED.
- `creates_a_paused_search_campaign_in_one_request`: one `googleAds:mutate` with the budget (total for 30 days, daily for 120; `explicitly_shared` false), the campaign `PAUSED`, `SEARCH`, search only, `start_date_time` `<start> 00:00:00` and `end_date_time` `<ends_on> 23:59:59`, the EU field, and the criteria; `manual_cpc` and `max_cpc` with `maximize_conversions` are `google_ads_input`. RED.
- `creates_what_goes_under_a_campaign_enabled`: the ad group's operation is `SEARCH_STANDARD` and `ENABLED`, and the keywords', negative keywords' and ad's are `ENABLED`. RED.
- `says_google_s_refusal_in_farik_s_words`: the message cut at 300; `PERMISSION_DENIED` on ideas gives "Google has not yet allowed Farik's app to give keyword ideas."; no redirect followed, no proxy, 4 MiB + 1 refused. RED.

- [ ] `feat(runtime): speak to Google Ads' API`

### Task 5: The route

`daemon.rs` (`Session.tickets`, `DaemonState::ads_writes`, the route outside the token layer, `launch_answer`'s ticket, `set_google_ads_api`), `daemon/ads_calls.rs`, `created_campaigns` (`crates/store/src/marketing.rs`), `FARIK_CONNECTORS` gaining `google-ads` (`kit.rs:163`, so a fixture kit may mark its tools; nothing starts it before Task 6), `marketing_campaign.created` in the schema and every exhaustive match (this commit). The route knows its seven writes by a constant of `google_ads.rs` and runs one only when the session's tags make it `external_effect` and plan-marked.

- `the_launch_gives_a_ticket_and_no_token`: the answer has `ticket` and an empty `env`, no token; the session holds its sha256. RED.
- `a_call_needs_a_live_ticket`: none, another, and one of an ended session are 401; a stopped session's is `session_stopped`. RED.
- `a_custom_entry_is_not_the_kit_s`: a custom (non-kit) `farik connector google-ads` entry tagging the writes `network` is refused `google_ads_not_kit`, and the fake saw nothing. RED.
- `a_write_outside_the_plan_is_refused_and_records_nothing`: `create_search_campaign` for a key the plan lacks is `not_in_marketing_plan: …`, the fake saw no mutate. RED.
- `a_write_inside_the_plan_reaches_google_and_records_the_campaign`: `marketing_campaign.created` with the budget's kind and amount, the campaign sent `PAUSED`, and `created_campaigns` reads it back; the ad group, keywords, negatives and ad then sent under it are `ENABLED`. RED.
- `two_creates_for_one_key_make_one_campaign`: only one mutate reaches the fake, and the other call is `not_in_marketing_plan`. RED.
- `a_failed_spend_read_refuses_budget_and_enable`: with the fake failing the spend read, `set_campaign_budget` and `set_campaign_status enabled` send no mutate, and pausing still runs. RED.
- `reads_run_without_a_plan`: `report` and `list_accounts` answer with no plan active. RED.
- `a_lapsed_sign_in_says_sign_in_again`. RED.
- Carried from step 08e's landing review, mutation 18 (`call_as`'s `definition.oauth().is_some()` read, `daemon/own_calls.rs:109`, made `Http` alone): Google Ads calls never go through `call_as` (Decisions), so Execution notes record mutation 18 as unreachable while `OWN_CALLS`, a `const`, holds no `stdio` pair.

- [ ] `feat(runtime): run Google Ads calls in the daemon, behind a session's ticket`

### Task 6: The shim

`google_ads.rs` `serve_shim`; `cli/src/lib.rs` (`ConnectorCommands`, `lib.rs:702`, gains `GoogleAds` beside `Osv` at `lib.rs:708`, reading the two variables); `connector_run.rs` (the variables).

- `the_shim_lists_ten_tools_offline` (neither variable set) and `forwards_a_call_with_its_ticket` (against a local stand-in for the route). RED each.
- `a_call_without_a_session_says_so`: with neither variable set, a call is the tool error "Google Ads runs only inside a Farik session". RED.
- `refuses_a_url_that_is_not_the_daemon_s`: `http://10.0.0.1:1/connector/call`, `https://…`, another path, each refused at the call. RED.
- `the_launcher_gives_the_shim_its_ticket_and_url` (`connector_run.rs`): the two variables from the answer and `daemon.json`'s port, no others but `KEPT_ENV`. RED.

- [ ] `feat(cli): serve Google Ads as Farik's own connector`

### Task 7: In the kit

The entry and the skill; the kit row's `at_launch` in `rpc.schema.json`, `kits_of` (`daemon/team.rs`), `KitService` (`Team.tsx`), `AgentEdit.tsx`'s kit row and `kitAtLaunch` in `strings/en.ts`.

- `google_ads_runs_only_inside_the_plan` (`kit.rs`): the entry exactly as Decisions, the three `network`, the seven `external_effect` all `plan_approved`, no allowance, the copy exactly; `the_marketing_kits_services_in_order` gains `google-ads`, and its loop, which asserts every connector is `http` and signs in, skips `google-ads`; `loads_every_shipped_kit` counts 5. RED.
- `marketing_kit_carries_running_search_ads`: 08d's fourteen then `running-search-ads`. RED.
- `google_ads_server_lists_the_kits_tools` (`crates/cli/tests/google_ads_server.rs`, the built binary, as `osv_server.rs`): no drift, ten tools. RED.
- `a_kit_row_without_an_app_comes_at_launch` (`team.rs`): the row has `at_launch` with the empty table, and not with `google_apps(fixture)`. RED.
- `agent_edit_says_a_kit_service_comes_at_launch` (`connectors.test.tsx`): AgentEdit's row shows its About and "Google Ads comes with Farik's web launch.", and no Connect. RED.
- `connects_each_marketing_service_by_name` (`daemon/team.rs:4013`) gains `google-ads`. Guard.

- [ ] `feat(roles): give the Marketing Specialist Google Ads`

### Task 8: Spec and plan

`docs/SPEC.md` 6.7 ("Farik's own connectors": `google-ads`, its shim, ticket and route, the API version, the tools and their checks; the plan mark; the kit row's `at_launch` before the launch), 6.5 (search ads as built), 5.6 (a plan-marked call), 8.5 (`marketing_campaign.created`, `marketing_plan` on `tool.called`), 8.6 (the ticket; the grant never leaves the daemon); the revision line. ADR 0042's `OWN_CALLS` amendment gains: Google Ads calls never go through `call_as`, the daemon being the server; their fixed operations are their list (the ten tools, the route's ad-group and spend reads, and 08g's spend read and pause). `docs/design/role-kits.md`. Project plan row 08f.

- [ ] `docs(spec): record Google Ads as Farik's own connector`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

The founder's live check runs with step 08g's, once the budget stops the ads: until then this step claims none. (Moved 2026-10-06 by ADR 0044 to phase 11, through Farik Cloud.)

## Execution notes

Corrections against the code, read at HEAD `0fb3a5c` on 2026-10-07 before Task 1 (step 10's fixes and 07c landed since the plan's fold). The plan's intent holds in every case; nothing here needed a decision.

- **Cited lines that moved**: `custom_connectors` is at `orchestrator/session.rs:383` (plan: `:394`). Every other cite holds at its line: `Session` `daemon.rs:143`, `end_session` `:588`, `router_serving` `:905`, `launch_answer` `:1157`, `refreshed_entry` `signed_in.rs:42`, `SessionConnector` `permissions.rs:164`, `evaluate_connector_call` `:256`, `session_connector` `session.rs:216` and the browser's `SessionConnector` `:295`, `judge_connector` `hooks.rs:480` and its call `:510`, `FARIK_CONNECTORS` `kit.rs:163`, `oauth_fixture` `lib.rs:39` to `:41`, `ConnectorCommands` `cli/src/lib.rs:702` with `Osv` at `:708`, `connects_each_marketing_service_by_name` `daemon/team.rs:4013`, `call_as`'s read `own_calls.rs:109`.
- **Task 1's constructions**: `SessionConnector` is built at `session.rs:217` and `:295`, in `daemon.rs` tests at `:2049` and `:2394`, in `hooks.rs` tests at `:1712`, `:1840`, `:1856` and `:2243`, in `permissions.rs` tests at `:548`, `:566`, `:670`, `:773` and `:865`, and in `cli/tests/connector_run.rs:160` and `live_claude.rs:235`. `KitConnector::Server` is built once, `kit.rs:659`, and matched naming every field at `daemon/team.rs` (`kits_of`); every other pattern already ends in `..`.
- **`refreshed_entry` takes six arguments** (`state, at, server, valid_for, wait, fallback_when_valid`). The ads route passes `120 s`, `10 s` and `true`, as `call_as` does: a refresh still running after the wait leaves a still-valid token in use, and the route's own Google calls fail with Google's words if it was not.
- **The launch route's own values** are `LAUNCH_VALID_FOR` 60 s and `LAUNCH_REFRESH_WAIT` 3 s; the ads route is another route with its own, as Decisions say.
- **Test names**: the marketing kit's skills test is `marketing_kit_carries_running_social_channels` (`kit.rs:1203`), which Task 7 renames `marketing_kit_carries_running_search_ads`; "`loads_every_shipped_kit` counts 5" means the Marketing Specialist's four connectors become five (`kit.rs:993`).
- **SPEC**: the latest revision is 0.67 (2026-10-07), so this step's is 0.68.
- **`SearchGoogleAdsRequest.page_size`** is deprecated and "returns a PAGE_SIZE_NOT_SUPPORTED error if this field is set" (v25 reference), so no search sends it: a report's GAQL ends `LIMIT 501`, one page, and `more` is true when 501 rows come back. The request body is `{ query }` alone.

Google's v25 reference, read on 2026-10-07 from developers.google.com/google-ads/api/reference/rpc/v25, the RPC reference (its REST pages 404 under the names tried, and REST's JSON names are proto3 JSON's camelCase of these; the page texts were fetched with `curl` and read, since the fetch tool could not read the 450 KB pages):
1. **Keyword ideas' response** (`GenerateKeywordIdeaResponse`): `results[]`, `aggregate_metric_results`, `next_page_token`, `total_size`. Each result (`GenerateKeywordIdeaResult`): `text`, `keyword_idea_metrics` (`KeywordPlanHistoricalMetrics`), `keyword_annotations`, `close_variants[]`. The metrics: `avg_monthly_searches` (int64), `competition` (`LOW`, `MEDIUM`, `HIGH`, `UNSPECIFIED`, `UNKNOWN`), `competition_index`, `low_top_of_page_bid_micros` and `high_top_of_page_bid_micros` (int64), `average_cpc_micros`, `monthly_search_volumes[]`. So the expected names hold, in REST's camelCase: `text`, `keywordIdeaMetrics`, `avgMonthlySearches`, `competition`, `lowTopOfPageBidMicros`, `highTopOfPageBidMicros`. **int64 is a string in REST JSON** (`"avgMonthlySearches": "1000"`), so the client reads a string or a number. The request (`GenerateKeywordIdeasRequest`) takes `language` and `geo_target_constants[]` as **resource names**, `languageConstants/<id>` and `geoTargetConstants/<id>` (at most 10), `keyword_seed { keywords[] }` ("at least one and no more than 20"), and `page_size` (at most 10,000); Farik builds the resource names from the tool's numeric ids and sends `pageSize` 100.
2. **The EU enum** (`EuPoliticalAdvertisingStatus`): `CONTAINS_EU_POLITICAL_ADVERTISING`, **`DOES_NOT_CONTAIN_EU_POLITICAL_ADVERTISING`**, `UNKNOWN`, `UNSPECIFIED`. The value in Decisions is right. `Campaign.start_date` and `end_date` are absent in v25; `start_date_time` and `end_date_time` ("yyyy-MM-dd HH:mm:ss", the customer's time zone; "set the time component to 00:00:00 for daily granularity" and `23:59:59`) are as Decisions say.
3. **Explorer access's error**: Google's access-levels page (updated 2026-09-30) lists `KeywordPlanIdeaService` among the services Explorer restricts, with `KeywordPlanService`, `AudienceInsightsService`, `ReachPlanService` and the billing and user-management services, and **names no error value**. Nothing was found that does. The plan's rule stands as written, and is the safe one: any `PERMISSION_DENIED` from `generateKeywordIdeas` gets the sentence "Google has not yet allowed Farik's app to give keyword ideas." (Google's `USER_PERMISSION_DENIED`, the manager-account case, is also `PERMISSION_DENIED`; manager accounts are out of scope, so the sentence is the right one for the accounts this step reaches.) A live check in phase 11 settles the exact error.
4. **API names the plan's words map to**: Maximize Clicks is the campaign's `target_spend { cpc_bid_ceiling_micros }` (the tool's `bidding: maximize_clicks`, with `max_cpc` as the ceiling), Maximize Conversions is `maximize_conversions {}`; a campaign's networks are `network_settings { target_google_search, target_search_network, target_content_network }` (Google Search only: true, false, false); a budget's `period` is `CUSTOM_PERIOD` with `total_amount_micros`, else `DAILY` with `amount_micros`; `MutateGoogleAdsRequest` takes `mutate_operations[]` and is one transaction unless `partial_failure` is true (Farik never sets it); an ad group's type is `SEARCH_STANDARD`; a keyword's match type is `EXACT`, `PHRASE` or `BROAD`.
