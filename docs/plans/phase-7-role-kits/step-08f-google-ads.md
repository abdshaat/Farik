# Phase 7, step 08f: Google Ads, Farik's own connector

Status: draft. Its readiness review runs once step 08e has landed (ADR 0032: one round).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 6.5, 6.7, 8.5, 8.6; F9
Depends on: step 08e of this phase (a Farik connector that signs in, Google's registered app, the grant kept and refreshed in the daemon); step 08c (the active plan, its campaigns, `google_ads_account`, `replaces`); step 07 (ADR 0038, `osv.rs`, the offline pin `crates/cli/tests/osv_server.rs`); phase 6 (merged in #19)
Readiness confirmed by: not yet run
Amended 2026-10-06 by ADR 0043: also depends on step 03f; Google Ads is signed in to with the customer's own Google app, and the Google Ads API's access level and quota are the customer's project's, not a quota Farik's users share.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from the budget's hard stop, step 08g, at the seam between running ads and stopping them; 08g's live check is this step's too.

## Goal

The Marketing Specialist can research search words and run Google Search ads inside the owner's approved marketing plan, through `google-ads`, a connector of Farik's own (ADR 0038, ADR 0042), signed in with Google (08e). Its three reads are `network`; its seven writes are `external_effect` marked as approved by the plan: the hook runs one while a plan is active and refuses it otherwise, never asking, and Farik refuses every write the active plan does not cover (`not_in_marketing_plan`). Every campaign is created paused, ending with its plan campaign, with a budget Google itself keeps within the plan. Nothing deletes, nothing touches billing, account access, conversion tracking, another campaign type or a campaign Farik did not create for the plan. Out of scope: reading the spend and pausing at the budget, and Today's "Raise the budget" (08g); manager accounts; Performance Max, Demand Gen and video campaigns; paid Instagram ads.

## Decisions

- **The server is a shim, and the daemon is Farik's server.** `farik connector google-ads` serves the ten tools' descriptors itself, so its list is pinned offline, and forwards each call to the daemon, which holds the grant, checks the plan against the log, calls Google, and records what it created. Rejected, the design's example: the daemon hands the server a short-lived access token (never the refresh token) and the plan in its environment. The server could not record the campaigns it creates, so 08g's hard stop would find them by their names, which a rename in Google's own screens breaks; a plan the owner ends mid-session would stay in its environment; and the token would sit in a process environment a no-sandbox agent command can read (8.6). Also rejected: handing it the refresh token (months-long, and a second refresher racing the daemon's lock), and a daemon-served MCP address in `mcp.json`, which a kit cannot name (ADR 0038).
- **The ticket.** `Session` (`daemon.rs:142`) gains `tickets`. For a Farik connector that signs in, the launch route (`launch_answer`, `daemon.rs:1121`) answers `{ command, args, env: {}, cwd, ticket }`: 32 random bytes as hex, its sha256 kept on the session (`Session.tickets`, by server) until `end_session` (`daemon.rs:553`). The launcher (`connector_run.rs` `run`) sets `FARIK_CONNECTOR_TICKET` to it and `FARIK_CONNECTOR_URL` to `http://127.0.0.1:<the port daemon.json names>/connector/call` beside `KEPT_ENV`. The shim sends each call as `POST` `{ tool, arguments }` with `Authorization: Bearer <ticket>`, refusing a URL that is not `http://127.0.0.1:<port>/connector/call`, following no redirect, using no proxy, waiting at most 60 seconds, reading at most 4 MiB, and answers the daemon's `{ ok }` as the tool's text and its `{ error }` as a tool error. The route `/connector/call` sits outside the daemon-token layer (`router_serving`, `daemon.rs:869`), behind `require_project`; it compares the ticket's sha256 with every live session's in constant time (401 otherwise), refuses a stopped session (`session_stopped`), and runs the call as that session's agent. A no-sandbox command that reads the shim's environment can use the ticket until the session ends, under the same checks as the agent (8.6 says so).
- **What the route checks, in order**: the session's connector `google-ads` (`connector_not_in_session`); a kit entry that `matches_kit` (`google_ads_not_kit`); the tool tagged and not `denied`, by the session's own tags; for a plan-marked tool, an active plan (`no_active_marketing_plan`); the agent's grant, refreshed with `refreshed_entry` (`daemon/signed_in.rs:39`, `valid_for` 120 s, `wait` 10 s) (`sign_in_again`); the input (each tool's checks below, `google_ads_input`); then, for a write, `check_ads_write` (core) with what Google answers it needs (a campaign's cost, an ad group's campaign) (`not_in_marketing_plan: <why>`); then Google.
- **Google's API** (`farik_runtime::google_ads`), REST at the fixed `GOOGLE_ADS_API`, `https://googleads.googleapis.com/v25` (released 2026-07-22, sunset about August 2027; a Farik release moves it), never an argument or input; the bearer from the grant; no developer token (sunset 2026-09-09, 08e); no `login-customer-id`, so only accounts the sign-in reaches directly; `GoogleAds::new` takes `https`, or `http` on loopback for the tests, and the daemon reads its address from `set_google_ads_api` (a `OnceLock`, as `set_registered_apps`), unset meaning `GOOGLE_ADS_API`, so only a test swaps it; `reqwest` with no redirect, no proxy, 25 seconds, at most 4 MiB read in chunks; an error said in Farik's words with Google's `message` cut at 300 characters as untrusted; Google's `PERMISSION_DENIED` on keyword ideas says "Google has not yet allowed your Google app to give keyword ideas: apply for Basic access on its Google Ads API page" (Explorer access blocks that service, 08e; the access level is the customer's project's, ADR 0043). Inputs are checked before anything is sent, and GAQL is built only from fixed text and checked values (a customer id is ten digits, a resource name `^customers/[0-9]{10}/(campaigns|adGroups)/[0-9]{1,20}$`, a date `YYYY-MM-DD`); no free query.
- **The tools.** `account` is `^[0-9]{3}-[0-9]{3}-[0-9]{4}$` everywhere, as the plan's `google_ads_account` is, and goes to Google without its dashes.
  - `list_accounts {}`, `network`: `customers:listAccessibleCustomers`, then for at most 20, `SELECT customer.descriptive_name, customer.currency_code, customer.time_zone, customer.manager FROM customer`; each `{ account, name, currency, time_zone, manager }`.
  - `report { account, kind, from, to }`, `network`: `kind` `campaigns`, `ad_groups`, `keywords`, `search_terms` or `ads`, each one fixed GAQL over `campaign`, `ad_group`, `keyword_view`, `search_term_view`, `ad_group_ad` with its names, status, `metrics.clicks`, `metrics.impressions`, `metrics.cost_micros` and `metrics.conversions`, `segments.date BETWEEN` the two dates (at most 366 days apart); at most 500 rows, `more` when cut; cost as a decimal string of the account's currency.
  - `keyword_ideas { account, words, language, locations }`, `network`: `customers/<id>:generateKeywordIdeas` with `keywordSeed`; `words` 1 to 10 of 1 to 80 characters, `language` and `locations` (1 to 10) numeric constant ids; at most 100 ideas, each `{ text, avg_monthly_searches, competition, low_bid, high_bid }`.
  - `create_search_campaign { account, plan_campaign, name, bidding, max_cpc?, locations, languages }`, plan: one atomic `googleAds:mutate` with temporary ids: a budget (`delivery_method` `STANDARD`, not shared) and a campaign named `<plan id> <key>: <name>` (`name` 1 to 80), `advertising_channel_type` `SEARCH`, status `PAUSED`, Google Search only (search partners and display off), starting the later of today and the plan campaign's `starts_on` and ending on its `ends_on` (v25's start and end fields; the budgets guide of 2026-09-30 names `start_date_time` and `end_date_time`, which the executor checks against v25's reference and records), `contains_eu_political_advertising` `DOES_NOT_CONTAIN_EU_POLITICAL_ADVERTISING`, `bidding` `maximize_clicks` (with `max_cpc` as its ceiling when given), `maximize_conversions` or `manual_cpc` (`max_cpc` required), and one criterion per location (1 to 20) and language (1 to 10). Then `marketing_campaign.created`.
  - `add_ad_group { campaign, name, cpc_bid? }`; `add_keywords { ad_group, keywords }` and `add_negative_keywords { campaign, keywords }`, each 1 to 50 `{ text: 1 to 80, match: exact | phrase | broad }`; `add_responsive_search_ad { ad_group, headlines, descriptions, final_url, path1?, path2? }`, 3 to 15 headlines of at most 30 characters, 2 to 4 descriptions of at most 90, `final_url` `https` with a host and no userinfo, paths at most 15; each plan.
  - `set_campaign_budget { campaign, amount }` and `set_campaign_status { campaign, status: paused | enabled }`, plan.
- **The budget Google keeps** (`campaign_budget`, core): a campaign whose run, from its start to its end, is 3 to 90 days takes a total budget for that period (`period` `CUSTOM_PERIOD`, `total_amount_micros`), the plan campaign's budget less what earlier versions of it spent: Google never charges more than a total budget, and its type cannot change after creation (Google Ads Help, "About campaign total budgets", read 2026-10-05). Any other takes a daily budget (`amount_micros`) of what is left divided by the days left, rounded down to the hundredth, which bounds Google's own charging while Farik is not running. Amounts go to Google as micros, hundredths times 10,000.
- **`check_ads_write`, pure.** Every write: the account is the active plan's `google_ads_account`. Create: `plan_campaign` is a campaign key of the active plan; no campaign is recorded for that key under the active plan or a plan it `replaces` (`campaign_exists`); its end is not past. Writes naming a `campaign` or an `ad_group` (whose campaign the route reads with one `SELECT ad_group.campaign FROM ad_group WHERE ad_group.resource_name = '<name>'`): that campaign is recorded `marketing_campaign.created` for a key the active plan has, under it or a plan it replaces. `set_campaign_budget`: a total budget's new amount between what the campaign spent and the plan campaign's budget; a daily one at most what is left divided by the days left. `set_campaign_status enabled`: today within the plan campaign's dates, its spend below its budget and the plan's Google Ads spend below the plan's. Pausing is always covered. Spend is what the route reads from Google for the call, one `Search` of the plan's campaigns' `metrics.cost_micros` since the first was created. Each refusal says why in a sentence after `not_in_marketing_plan: `.
- **The plan mark.** A kit `stdio` connector may list `plan_approved: [<tool>]`, each tagged `external_effect` (`plan_mark_not_external`), none with an allowance (`plan_mark_with_allowance`), and only on Farik's own connector, the exact pair of ADR 0038 (`plan_mark_not_farik`): only Farik's server can be trusted to check the plan. It is the kit's, not the team entry's or its hash's, as `media_hosts` is. `SessionConnector` (`permissions.rs:164`) gains `plan_tools`, filled at setup from the kit for a kit entry that `matches_kit` (`session_connector`, `session.rs:215`). `evaluate_connector_call` (`permissions.rs:256`) takes `active_plan: bool`: a plan-marked `external_effect` call passes with `plan_approved: true` when it is true and is refused `NoActivePlan` when not, after `InputTooLarge` and before the grant and the allowance, so it never asks and `auto` (10h) never runs it. The hook (`judge_connector`, `hooks.rs:479`) reads the active plan (`marketing_plans`, `active_plan`, 08c) from the log only for a plan-marked tool, says the refusal "no_active_marketing_plan: <tool> of <server> runs only inside a marketing plan the owner approved", and records `tool.called` with `marketing_plan: "MP-<n>"`. Rejected: asking the owner for each write, the option ADR 0042 turned down.
- **Events**: `marketing_campaign.created { plan, key, account, campaign, budget, budget_kind: total | daily, amount }`, recorded by the route on Google's success, its envelope the session's agent, session and task, about no contract; `tool.called` gains `marketing_plan`.
- **The kit entry** `google-ads`, after Kit's entry (`kit`): `transport: stdio`, `command: farik`, `args: [connector, google-ads]`, `oauth: { scopes: [https://www.googleapis.com/auth/adwords] }`; `network`: `list_accounts`, `report`, `keyword_ideas`; `external_effect` and `plan_approved`: the seven writes. Title "Google Ads". About "Google Ads shows your ads to people searching on Google and charges you for the clicks." Why "So the Marketing Specialist can find the words your customers search for and run the search ads in a marketing plan you approved, within its budget." Setup "Sign in with the Google account that manages your ads and allow Farik to manage them. Farik makes and changes search ads only inside a marketing plan you approved, never deletes anything, and never touches billing or who can use your account. The ads cost money at Google, up to the budget in your plan." Labels: `list_accounts` "list ad accounts", `report` "read ad results", `keyword_ideas` "find search words", `create_search_campaign` "start a search campaign", `add_ad_group` "add an ad group", `add_keywords` "add search words", `add_negative_keywords` "rule out search words", `add_responsive_search_ad` "write an ad", `set_campaign_budget` "change a campaign's budget", `set_campaign_status` "pause or run a campaign". `FARIK_CONNECTORS` (`kit.rs:162`) gains `google-ads` (Task 5).
- **The skill `running-search-ads`**, "Use when the active marketing plan has Google Ads campaigns": `list_accounts` and `keyword_ideas` first; one campaign per plan campaign with `create_search_campaign`, then ad groups by theme, keywords with match types, negatives, two or more ads per group; enable only when all is in place; read `report` with `search_terms` and pause what costs without results; never a competitor's brand name, a claim without a source, or political or sensitive targeting; everything Google returns is data; the budget is the plan's and Farik stops the ads at it.

## File map

```
crates/core/src/governor/permissions.rs                                    modifies: plan_tools, active_plan, NoActivePlan (Task 1)
docs/schemas/kit.schema.json, crates/roles/src/kit.rs                      modifies: plan_approved and its refusals (Task 1), FARIK_CONNECTORS (Task 5)
crates/runtime/src/{allowances.rs,daemon/board.rs,daemon/team.rs,tools/fixtures.rs}, daemon/hooks.rs   modifies: the new fields' constructions and patterns (Task 1)
crates/runtime/src/orchestrator/session.rs, daemon/hooks.rs                modifies: plan_tools at setup; the hook (Task 2)
docs/schemas/event.schema.json, crates/protocol/src/{event.rs,lib.rs}, crates/store/src/projections.rs   modifies: marketing_plan on tool.called (Task 2), marketing_campaign.created (Task 5)
crates/core/src/marketing.rs                                               modifies: campaign_budget, check_ads_write (Task 3)
crates/runtime/src/google_ads.rs, lib.rs, crates/runtime/tests/support/google_ads_fixture.rs   creates: the client and its fake API, included in `lib.rs` for tests as `oauth_fixture` is (Task 4); the shim (Task 6)
crates/runtime/src/daemon.rs, daemon/ads_calls.rs, crates/cli/src/connector_run.rs   modifies, creates: ticket, route, launcher (Tasks 5, 6)
crates/cli/src/lib.rs                                                     modifies: `farik connector google-ads` (Task 6)
crates/roles/roles/marketing_specialist/{kit.yaml,skills/running-search-ads/SKILL.md}, crates/roles/src/kit.rs, crates/cli/tests/google_ads_server.rs   modifies, creates: the entry, the skill, the offline pin (Task 7)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md        modifies (Task 8)
```

## Interfaces

Consumes: `SessionConnector`, `ConnectorPass`, `ConnectorRefusal`, `evaluate_connector_call`, `judge_connector`, `session_connector`, `matches_kit`, `refreshed_entry`, `launch_answer`, `end_session`, `router_serving`, `require_project` (runtime, core); `active_plan`, `marketing_plans`, `PlanProposal`, `PlanCampaign`, `Amount` (08c); `CustomServer::oauth`, `app_for_farik_connector` (08e); `osv.rs` as the pattern; `FARIK_CONNECTORS`, `is_farik_connector`, `pin_drift`, `list_tools`.

Produces:

```rust
// farik_core::governor::permissions
pub struct SessionConnector { /* … */ pub plan_tools: BTreeSet<String> }
pub struct ConnectorPass { pub tag: ConnectorTag, pub approval: Option<u64>, pub allowance: Option<u32>, pub plan_approved: bool }
// ConnectorRefusal::NoActivePlan
pub fn evaluate_connector_call(tool: &str, input: &Value, connector: Option<&SessionConnector>, granted: Option<u64>,
    used: u32, active_plan: bool) -> Result<ConnectorPass, ConnectorRefusal>;
// farik_core::marketing
pub enum BudgetKind { Total, Daily }
pub struct CreatedCampaign { pub plan: String, pub key: String, pub campaign: String, pub budget: String, pub kind: BudgetKind }
pub fn campaign_budget(campaign: &PlanCampaign, spent: Amount, today: NaiveDate) -> (BudgetKind, Amount);
pub enum AdsWrite { Create { account: String, plan_campaign: String }, UnderCampaign { account: String, campaign: String },
    Budget { account: String, campaign: String, amount: Amount }, Enable { account: String, campaign: String }, Pause { account: String, campaign: String } }
pub struct AdsPlanView<'a> { pub plan_id: &'a str, pub plan: &'a PlanProposal, pub lineage: &'a [String],
    pub created: &'a [CreatedCampaign], pub spent: &'a BTreeMap<String, Amount>, pub today: NaiveDate }
pub fn check_ads_write(view: &AdsPlanView<'_>, write: &AdsWrite) -> Result<(), String>;
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
pub async fn serve_shim(url: &str, ticket: &str) -> Result<(), GoogleAdsError>;
impl DaemonState { pub fn set_google_ads_api(&self, api: String) -> bool; }                      // daemon.rs
// farik_runtime::daemon::ads_calls
pub(crate) async fn ads_call(state: &Arc<DaemonState>, ticket: &str, tool: &str, arguments: Value) -> Result<Value, String>;
```

## Tasks

### Task 1: The plan mark

One commit: every construction of `SessionConnector` (`session.rs:215`, `session.rs:294`, the hook's and tools' fixtures) and of `KitConnector::Server` (and each pattern naming its fields, as in 08c's Task 10) takes an empty set, and the hook's call (`hooks.rs:509`) passes `active_plan: false`, until Task 2.

- `a_plan_marked_call_runs_only_inside_a_plan` (`permissions.rs`): with `active_plan` true it passes with `plan_approved` and no approval or allowance; false is `NoActivePlan`, even with a grant; a call over 64 KiB is still `InputTooLarge` first; an unmarked `external_effect` call is unchanged. RED.
- `the_mark_is_farik_s_own_and_external_only` (`kit.rs`): `plan_approved` on an `http` connector or on `npx x@1.0.0` is `plan_mark_not_farik`; naming a `network` tool `plan_mark_not_external`; a tool with an allowance `plan_mark_with_allowance`; a fixture kit's `farik connector osv` with one marked `external_effect` tool loads. RED.

- [ ] `feat(core): let an approved marketing plan approve a Farik connector's call`

### Task 2: The hook

`SessionConnector.plan_tools` from the kit at setup; the hook's lookup and `tool.called`'s `marketing_plan` (schema, `ToolCalledBody`).

- `a_plan_marked_call_runs_while_a_plan_is_active` (`hooks.rs`, `#[ignore]`d for `--integration` as its neighbours): a fixture kit's marked tool, with an approved plan active, is allowed, `tool.called` carries `marketing_plan: "MP-1"`, and nothing is asked; with none active it is denied `no_active_marketing_plan` and no `tool_approval.requested` exists. RED.
- `a_custom_entry_gets_no_plan_mark` (`session.rs`): a custom entry naming the same pair has empty `plan_tools`. RED.

- [ ] `feat(runtime): run a plan-marked call inside the active marketing plan`

### Task 3: The plan's checks

- `a_short_or_long_campaign_takes_a_daily_budget`: 2 days and 91 days daily (left / days, rounded down), 3 and 90 days total. RED.
- `checks_each_write_against_the_plan` (`core::marketing`): one case per refusal of Decisions (another account, an unknown key, a second campaign for a key, a campaign of another plan, a raised total past the budget, a daily amount past the share, enabling past the dates or at the budget), and pausing always passes; a replaced plan's campaign of the same key counts under its replacement. RED.

- [ ] `feat(core): check a Google Ads change against the marketing plan`

### Task 4: Google's API, faked

`tests/support/google_ads_fixture.rs`, included by `lib.rs` under `#[cfg(all(test, unix))]` as `oauth_fixture` is (`lib.rs:36`): an `axum` server that records every request and answers `listAccessibleCustomers`, `googleAds:search`, `googleAds:mutate`, `generateKeywordIdeas`, an error with a long `message`, a `PERMISSION_DENIED`, a redirect, or an oversized body. No test calls Google.

- `sends_the_bearer_and_no_developer_token`: every request has `Authorization: Bearer`, no `developer-token` or `login-customer-id`. RED.
- `builds_each_report_from_fixed_text`: each `kind`'s GAQL is exactly its fixed text with the checked dates; an input `from` of `2026-01-01' OR 1=1` is refused before sending. RED.
- `creates_a_paused_search_campaign_in_one_request`: one `googleAds:mutate` with the budget (total for 30 days, daily for 120), the campaign `PAUSED`, `SEARCH`, search only, its dates, the EU field, and the criteria. RED.
- `says_google_s_refusal_in_farik_s_words`: the message cut at 300; `PERMISSION_DENIED` on ideas gives the Explorer sentence; no redirect followed, no proxy, 4 MiB + 1 refused. RED.

- [ ] `feat(runtime): speak to Google Ads' API`

### Task 5: The route

`daemon.rs` (`Session.tickets`, the route outside the token layer, `launch_answer`'s ticket, `set_google_ads_api`), `daemon/ads_calls.rs`, `FARIK_CONNECTORS` gaining `google-ads` (`kit.rs:162`, so a fixture kit may mark its tools; nothing starts it before Task 6), `marketing_campaign.created` in the schema and every exhaustive match (this commit). The route knows its seven writes by a constant of `google_ads.rs` and runs one only when the session's tags make it `external_effect` and plan-marked.

- `the_launch_gives_a_ticket_and_no_token`: the answer has `ticket` and an empty `env`, no token; the session holds its sha256. RED.
- `a_call_needs_a_live_ticket`: none, another, and one of an ended session are 401; a stopped session's is `session_stopped`. RED.
- `a_write_outside_the_plan_is_refused_and_records_nothing`: `create_search_campaign` for a key the plan lacks is `not_in_marketing_plan: …`, the fake saw no mutate. RED.
- `a_write_inside_the_plan_reaches_google_and_records_the_campaign`: `marketing_campaign.created` with the budget's kind and amount. RED.
- `reads_run_without_a_plan`: `report` and `list_accounts` answer with no plan active. RED.
- `a_lapsed_sign_in_says_sign_in_again`. RED.
- Carried from step 08e's landing review: mutation 18 (`call_as`'s `definition.oauth().is_some()` read, `daemon/own_calls.rs:109`, made `Http` alone) survived because `OWN_CALLS` lists Buffer's tools alone, so no call reaches a signed-in `stdio` connector. This task either tests `call_as` with a signed-in `stdio` connector, in a test that fails under that mutation, or records in Execution notes that `ads_calls.rs` bypasses `call_as`, so that the read has no caller to test.

- [ ] `feat(runtime): run Google Ads calls in the daemon, behind a session's ticket`

### Task 6: The shim

`google_ads.rs` `serve_shim`; `cli/src/lib.rs` (`ConnectorCommands` beside `Osv` at `lib.rs:696`, `GoogleAds` reading the two variables); `connector_run.rs` (the variables).

- `the_shim_lists_ten_tools_offline` and `forwards_a_call_with_its_ticket` (against a local stand-in for the route). RED each.
- `refuses_a_url_that_is_not_the_daemon_s`: `http://10.0.0.1:1/connector/call`, `https://…`, another path. RED.
- `the_launcher_gives_the_shim_its_ticket_and_url` (`connector_run.rs`): the two variables from the answer and `daemon.json`'s port, no others but `KEPT_ENV`. RED.

- [ ] `feat(cli): serve Google Ads as Farik's own connector`

### Task 7: In the kit

- `google_ads_runs_only_inside_the_plan` (`kit.rs`): the entry exactly as Decisions, the three `network`, the seven `external_effect` all `plan_approved`, no allowance, the copy exactly; `the_marketing_kits_services_in_order` gains `google-ads`; `loads_every_shipped_kit` counts 5. RED.
- `marketing_kit_carries_running_search_ads`: 08d's fourteen then `running-search-ads`. RED.
- `google_ads_server_lists_the_kits_tools` (`crates/cli/tests/google_ads_server.rs`, the built binary, as `osv_server.rs`): no drift, ten tools. RED.
- `connects_each_marketing_service_by_name` (`daemon/team.rs:3975`) gains `google-ads`. Guard.

- [ ] `feat(roles): give the Marketing Specialist Google Ads`

### Task 8: Spec and plan

`docs/SPEC.md` 6.7 ("Farik's own connectors": `google-ads`, its shim, ticket and route, the API version, the tools and their checks; the plan mark), 6.5 (search ads as built), 5.6 (a plan-marked call), 8.5 (`marketing_campaign.created`, `marketing_plan` on `tool.called`), 8.6 (the ticket; the grant never leaves the daemon); the revision line. `docs/design/role-kits.md`. Project plan row 08f.

- [ ] `docs(spec): record Google Ads as Farik's own connector`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

The founder's live check runs with step 08g's, once the budget stops the ads: until then this step claims none.

## Execution notes

None yet.
