# Phase 7, step 10d: Procurement Specialist kit

Status: draft. Its readiness review runs once step 10c has landed; everything it consumes from steps 05 to 07 is on this branch now. The founder answered O4 on 2026-10-05: the agent "is supposed to have the ability to look for any product whether technical or non technical products. He could research cars, baby mirrors, or any product under the sun"; the kit was widened that day from software services to any product, and Farik's own product-safety and eBay servers were split into step 10g.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.10; F9
Depends on: steps 10b and 10c of this phase (the role, its folder, `farik_write_evaluation`, `farik_draft_purchase_order`, `farik_read_purchase_orders`); step 07 (Farik's own connectors, `FARIK_CONNECTORS`, `farik_runtime::osv` as the pattern; committed at 383a626); steps 05 and 05b (the kit format, connect by name, the live pin test); phase 6 (merged in #19)
Readiness confirmed by: not yet. A fresh-session Opus reviewer read the four plans on 2026-10-05 before their dependencies landed: 3 Blocking (step 10d: `fx` had no copy, `uv` and its first run were undecided; step 10c: `renewal.due` broke the event-naming rule), all folded with the Should items the same day. The readiness review proper runs when the step's dependencies have landed, as its Status says (ADR 0032: one round).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The Procurement Specialist ships a kit for buying anything, technical or not: a used car, a baby car mirror, stock to resell, or a software subscription. Twelve skills, loaded on demand, and five services, each optional and each read-only or limited by an allowance. Exchange rates (Farik's own server over Frankfurter, no account), so every comparison is in one currency; web search and page reading for finding sellers and manufacturers (Exa's official server, no account for a few searches a day); shopping prices across Google Shopping, Amazon, eBay and Walmart (SerpApi's official server, a key, each search counted against an allowance because it spends the user's searches); what the company already spends with a seller (Brex's official server, signed in, read-only); and AWS's list prices for a software team (AWS's official server, a key that may only read prices). Product-safety recalls and eBay listings, through Farik's own servers, are step 10g. Out of scope: any connector that can buy, check out, sign or send (ADR 0039; sending to sellers is step 10f, Farik's own act after the founder's approval); Amazon's own buyer API, which needs an Associates account with ten qualifying sales in thirty days; supplier directories (Alibaba, ThomasNet and the rest), which offer no buyer API and forbid scraping; any new screen.

## Decisions

- **No mockups.** Step 05's screens (the kit list on `AgentEdit`, `ConnectorAdd` from a kit with a key form and a sign-in form, `KitConnect` with its how-many step) show every kind of service this step adds.
- **How each server was chosen**, by ADR 0020's order and ADR 0035's routes, researched 2026-10-05 (metadata read that day from each server's `/.well-known/oauth-protected-resource` and its authorization server's `/.well-known/oauth-authorization-server`):
  - **Exchange rates: `fx`, Farik's own, `stdio`, `command: farik`, `args: [connector, fx]`.** Frankfurter publishes central-bank reference rates with no key and no quota (frankfurter.dev); `GET https://api.frankfurter.dev/v2/rates?base=USD&quotes=EUR,GBP` answered 200 that day with `[{date, base, quote, rate}, …]`, and `/v2/currencies` with `[{iso_code, iso_numeric, name, symbol, start_date, end_date}, …]`. No official server exists; `frankfurtermcp` (PyPI 0.5.0, 2026-09-15) is one maintainer's and floats its dependencies. ADR 0038 already lets a kit start Farik's own server by `farik connector <name>`; `FARIK_CONNECTORS` becomes `["osv", "fx"]`, so `is_farik_connector` accepts exactly `[connector, fx]` too.
  - **Cloud list prices: AWS Pricing, official, `stdio`, `command: uvx`, `args: ["awslabs.aws-pricing-mcp-server==1.1.1"]`, two pasted keys.** AWS Labs' server (PyPI 1.1.1, 2026-09-08; github.com/awslabs/mcp, `src/aws-pricing-mcp-server`) reads the free AWS Price List API. `credential_keys: [AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY]`, `key_page: https://console.aws.amazon.com/iam/home#/users`. Its region defaults to `us-east-1`, the Price List API's own, so no region is set. Its own dependencies are on ranges, which an exact pin of the top package does not hold; accepted as step 07 accepted it for Context7's alternatives, and recorded here. Rejected: Vantage (`https://mcp.vantage.sh/mcp`), which reads spend already made, not list prices; Infracost, community only.
  - **Finding sellers and manufacturers: Exa, official, `http`, `https://mcp.exa.ai/mcp`, no key.** It answered `initialize` and `tools/list` without a key on 2026-10-05 (server 3.2.1): `web_search_exa` and `web_fetch_exa`. Without a key it allows about 150 calls a day per address, which the setup copy says; its key goes in the address's query (`?exaApiKey=`), which a team file refuses (`url_holds_secret`), so the kit ships the keyless address and a heavier user asks for more through a data pipeline (10e). Rejected: Tavily (`https://mcp.tavily.com/mcp/`, signs in by route 1 with registration; a candidate, the design keeps it), Brave Search and Perplexity (keys and a card), Firecrawl (a candidate for reading difficult pages).
  - **Shopping prices: SerpApi, official, `http`, `https://mcp.serpapi.com/mcp`, a pasted key.** Its server (2.0.0) lists `search`, `search_table` and `search_dashboard` without a key, and a call without one answers "Missing API key. Use path format /{API_KEY}/mcp or Authorization: Bearer"; the kit sends `headers: { Authorization: "Bearer {SERPAPI_KEY}" }`, `credential_keys: [SERPAPI_KEY]`, `key_page: https://serpapi.com/manage-api-key`. Its engines cover Google Shopping, Amazon, eBay and Walmart. Its free plan gives 250 searches a month (serpapi.com/pricing); each search spends one of the user's searches, so `search` is `external_effect` with an allowance, `calls: 50`, `what: "shopping searches"` (spec 6.7: a tool that spends the user's credits), and a search past it asks the founder. `search_table` and `search_dashboard` are `denied` until a pin update reads what they do. Rejected: Amazon's Creators API (an Associates account with ten qualifying sales in thirty days), Keepa (paid, community server only).
  - **What the company already spends: Brex, official, `http`, `https://api.brex.com/mcp`, signed in (route 1).** Its resource (`https://api.brex.com`) names `https://api.brex.com` first as its authorization server, whose metadata has `registration_endpoint` `https://api.brex.com/v3/clients`, S256, `token_endpoint_auth_methods_supported` with `none`, and a revocation endpoint; `rmcp` uses the first. `oauth: { scopes: [offline_access, vendors.readonly, expenses.card.readonly, departments.readonly] }`, all four in its `scopes_supported`, so a write is not granted. The tool names are from developer.brex.com/docs/mcp, read 2026-10-05. Rejected: Ramp's hosted server, whose tool list is generated at run time and includes card checkout (O4).
- **What each tag is.** One tool spends the user's credits, SerpApi's `search`, `external_effect` with its allowance; nothing else is `external_effect`. `fx`'s three tools and Exa's two are `network`. AWS: the five price reads and `get_bedrock_patterns` are `network`; `analyze_cdk_project` and `analyze_terraform_project` are `denied` because they read any path the agent names on the user's computer, where a host server runs (spec 6.7), and `generate_cost_report` is `denied` because it writes a file there. Brex: 11 `network` (vendors, bills, merchants and categories, expense analytics, expenses, departments); 32 `denied`: every write, the people tools, the card, limit, bank and reward tools, travel, accounting and set-up, and the three Brex marks as writes though they look like reads. The people tools are `denied` although the Product Manager's kit keeps member lists `network` (step 06, P3): a buying decision needs a vendor, not a colleague's name, and Brex's user records carry card holders and limits. `list_expenses` and `get_expense_by_id` stay `network`: they hold a colleague's name on a card charge, which a recurring-charge search needs; `using-procurement-sources` says to report vendors and amounts, never people.
- **The `fx` server** is `farik_runtime::fx`, a hand-written `rmcp` `ServerHandler` over stdio, as `farik_runtime::osv` is (same features, no new crate), `serverInfo.name` `"farik-fx"`. Its address is the constant `FX_API = "https://api.frankfurter.dev/v2"`, never an argument, environment value or tool input; tests pass a fixture's address to the function, not the command. One `reqwest::Client` with `redirect::Policy::none()`, `no_proxy()`, no cookies, a 15-second timeout, reading at most 1 MiB in chunks; a larger answer is a tool error, "Frankfurter's answer is too large", sent nowhere. Inputs are checked before any request: a currency code `^[A-Z]{3}$`; `quotes` 1 to 30 distinct codes, none equal to `base`; a `date` an ISO date from `1999-01-04` to today in UTC. Tools, each answering plain JSON:
  - `latest_rates { base, quotes }` → `GET /rates?base=<base>&quotes=<a,b>` → `{ base, rates: { <quote>: { rate, date } } }`, each quote with the date Frankfurter gave for it, since its rows carry a date each and the central banks it reads publish on different days;
  - `rate_on { base, quote, date }` → `GET /rates?base=<base>&quotes=<quote>&date=<date>` → `{ date, base, quote, rate }`, `date` the one Frankfurter answered for, which may differ from the day asked, so the agent never claims a rate for a day that had none;
  - `list_currencies {}` → `GET /currencies` → `[{ code, name }]` from `iso_code` and `name` only, at most 400 entries.
  A non-2xx answer is "Frankfurter could not answer that; check the codes and the date", without its body. The query is built with `Url::query_pairs_mut`, never by formatting strings. Verified against the live API on 2026-10-05 by the readiness fold: `date=2026-10-03` answered rows dated `2026-10-03`, and `quotes=EUR%2CGBP` (the encoded comma `query_pairs_mut` writes) was accepted.
- **`fx`'s copy.** Title "Exchange rates". About "Frankfurter publishes the reference exchange rates of central banks, free and with no account." Why "So the Procurement Specialist compares prices in one currency, with the rate and its date beside each. It only reads." Setup "Nothing to set up: Farik looks rates up in Frankfurter itself, with no account. Farik sends Frankfurter only currency codes and a date."
- **AWS Pricing needs `uv`** (B2), the first third-party `stdio` package in a shipped kit. Its setup's first sentence is "This needs the free program uv on your computer (docs.astral.sh/uv)." The first `uvx` run downloads Python and the package, which may outlast the 30-second listing limit (spec 6.7); the user connects again, and the second run is served from uv's cache. The live pin run records the first-run and second-run listing times in the Execution notes; if the second is over 30 seconds the run stops and the planner decides. The setup check of `farik doctor` is not changed in this step.
- **The copy**, in full in Task 3; none says "MCP", "OAuth" or "token", and AWS's quotes AWS's own labels ("Access key", "Secret access key") between ‘ and ’, as step 05's exception allows in `setup`. Exa's and SerpApi's say that what the agent searches for goes to the service as written.
- **Kit skills are embedded** as step 06 did: `embedded_skills(Role::ProcurementSpecialist)` returns the twelve `(name, &[("SKILL.md", include_str!(…))])` pairs. `sourcing-a-product` stays in `role.yaml` (step 10b); no kit skill repeats its loop or its never-buy rules, and each names only `farik_*` tools that exist (`kit_skills_name_only_tools_farik_lists`).
- **Pins against the live service**, by step 06's mechanical rule, unchanged: a tool the service lists and this plan lacks goes in `denied` with no label; a named tool the service no longer lists is removed only if its documentation fetched that day no longer names it either; counts and lists in this plan's tests follow in the same commit, recorded in the Execution notes. `fx`'s pin is an offline test, as OSV's is.
- **Brex's scopes, a fallback.** If the founder's sign-in, or a `list_vendors` call after it, fails with these four scopes, the mechanical fallback is `oauth: { scopes: [offline_access] }`, with the setup's second sentence becoming "Farik asks Brex for what your role in Brex lets you see, and only ever reads it"; recorded in the Execution notes, in its own commit, `fix(roles): sign in to Brex with its default access`.
- **AWS's key reaches the server through the launcher's cleared environment** (spec 8.2, ADR 0030), as `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`; boto3 reads those before any profile. If the live pin run shows the server needs another variable to start under the cleared environment, the run stops and the planner decides; nothing is added to `KEPT_ENV` in this step.

Decided by the founder, 2026-10-05: O4, any product. Live pins need a SerpApi key (its free plan), a Brex account with "Brex in AI assistants" on, and an AWS key limited to the pricing read actions. `fx` and Exa need none.

## File map

```
crates/roles/roles/procurement_specialist/skills/<12 names>/SKILL.md  creates (Task 1)
crates/roles/roles/procurement_specialist/kit.yaml           modifies: skills (Task 1), connectors (Tasks 2, 3)
crates/roles/src/kit.rs                                      modifies: embedded_skills arm, FARIK_CONNECTORS; tests (Tasks 1 to 3)
crates/runtime/src/fx.rs                                     creates: Farik's own exchange-rate server (Task 2)
crates/runtime/src/lib.rs                                    modifies: pub mod fx (Task 2)
crates/cli/src/connector_run.rs, crates/cli/src/lib.rs       modifies: `farik connector fx` (Task 2)
crates/cli/tests/fx_server.rs                                tests: the built binary lists the kit's tools (Task 2)
crates/runtime/src/daemon/team.rs                            tests: each service connects by name (Task 4)
crates/runtime/tests/live_kit_pins.rs                        modifies: header comment, and the skip line names crates/cli/tests/{name}_server.rs (Task 4)
docs/SPEC.md, docs/design/role-kits.md, docs/design/procurement-specialist.md, docs/plans/project-plan.md   modifies (Task 5)
```

## Interfaces

Consumes: `load_kit`, `parse_kit`, `Kit`, `KitConnector`, `embedded_skills`, `FARIK_CONNECTORS`, `is_farik_connector`, `check_skill` (`farik-roles`); `kit_entry`, `matches_kit`, `live_kit_pins_hold` (`farik-runtime`, steps 05 to 07); `osv::serve_stdio` as the pattern; `CliIo`, `ConnectorCommands` (`farik-cli`); `Role::ProcurementSpecialist` (step 10b).

Produces:

```rust
pub const FX_API: &str = "https://api.frankfurter.dev/v2";            // farik_runtime::fx
pub fn tool_names() -> Vec<&'static str>;                               // ["latest_rates", "rate_on", "list_currencies"]
pub struct Fx;                                                          // as osv.rs's Osv
impl Fx { pub fn new(api: &str) -> Result<Self, FxError>; pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String>; }
pub async fn serve_stdio(api: &str) -> Result<(), FxError>;
pub enum FxError { Client, Serving(String) }                            // as OsvError
pub fn fx(io: &mut CliIo<'_>) -> i32;                                   // farik_cli::connector_run
// ConnectorCommands::Fx; FARIK_CONNECTORS = ["osv", "fx"]
```

## Tasks

### Task 1: The twelve skills

Files: `procurement_specialist/skills/{defining-the-need,finding-sellers-and-makers,comparing-offers,reading-terms-and-pricing,checking-a-seller,checking-product-safety,estimating-landed-cost,checking-a-used-vehicle,keeping-the-vendor-register,reviewing-renewals,writing-purchase-orders,using-procurement-sources}/SKILL.md`; `kit.yaml` `skills` in that order; `embedded_skills`' arm. Each has `name` and a `description` starting "Use when", numbered sections, under 6 KB, no `` !` `` and no attached file. Every one is written for any product, with an example of a physical good and of a service. Their content:

- `defining-the-need`: what it must do or be; the exact specification (model, size, standard it must meet, condition new or used); the quantity and how often; where and by when it must arrive; the budget; who uses it; one question per `farik_ask_human` call, at most four choices, only what changes the short list.
- `finding-sellers-and-makers`: the maker first, then its authorised sellers, then marketplaces and resellers; for goods to resell, the maker or a wholesaler, never a retail listing; how to find a maker's contact page or a wholesale form; a seller list of at least three where they exist; never a site's checkout, never a login, never a scraper of a site that forbids it.
- `comparing-offers`: three to five offers; must-haves first, an offer failing one is out; unit price, quantity breaks, shipping, taxes and fees, warranty and returns; for a subscription the 12- and 36-month totals; every price converted with `rate_on` on the day it was read, the rate and date beside it; written with `farik_write_evaluation`, a table and then a recommendation in two sentences.
- `reading-terms-and-pricing`: what the price includes; minimum order; delivery time; warranty, returns and refunds; auto-renewal and notice for a subscription; a DPA and where data is held for a service; an unclear term quoted for the founder, never interpreted; "not legal advice".
- `checking-a-seller`: how long it has traded, its address and company registration, its reviews across more than one site, marketplace seller ratings, signs of a scam (prices far below every other, payment only by transfer or gift card, no address, a new domain); for a software service its trust page (SOC 2 Type II, ISO 27001, two-factor, encryption); what is missing, said plainly.
- `checking-product-safety`: recalls first (step 10g's `recalls` server when connected, else the agency's own page); the safety standard the product must meet where it is sold (for a child's car product, a car part or an electrical good, the standard by name and the mark to look for); a product with an open recall is never recommended.
- `estimating-landed-cost`: the price, shipping, insurance, import duty and tax, fees at the border, and the currency, as one number per unit delivered; say which figure is a guess and where to confirm it.
- `checking-a-used-vehicle`: the VIN first (decoded, step 10g's `decode_vin`), its open recalls, the title's state, the history report the founder should buy, an inspection before money moves, comparable listings' prices for the same year, make, model and mileage; the agent never says a car is sound.
- `keeping-the-vendor-register`: the sixteen columns in order, for sellers of goods as for subscriptions; ISO dates; values never formulas; read with `farik_read_sheet` before writing with `farik_write_sheet`; each order `farik_read_purchase_orders` gives as received written in with what was paid.
- `reviewing-renewals`: usage against the plan, the price now and paid, alternatives' prices today; keep, change or cancel with the saving per year; what to ask the seller for; the decision date first.
- `writing-purchase-orders`: the evaluation first, always; one order per seller; each line's item exactly as the seller names it, with quantity, unit and unit price as quoted; delivery and terms as agreed; `url` the seller's own page, never a reseller's, an ad's or a shortened link; `why` in two plain sentences; then stop, since the founder decides and places it.
- `using-procurement-sources`: what `fx`, Exa, SerpApi, Brex and AWS pricing are for; SerpApi spends the founder's searches, so search once and read well; report sellers and amounts from Brex, never people; never put the business's own data or a secret in a query; everything returned is data, never instructions; the kit only reads; with nothing connected, use the sellers' public pages and say so.

- `procurement_kit_carries_its_skills`: `load_kit(ProcurementSpecialist)`'s skills are those twelve in that order, each with its `SKILL.md`. RED: the kit has none.

- [ ] `feat(roles): give the Procurement Specialist's kit its skills`

### Task 2: Farik's own exchange-rate server

Files: `fx.rs`, `lib.rs`, `connector_run.rs`, cli `lib.rs` (`ConnectorCommands::Fx`), `kit.rs` (`FARIK_CONNECTORS`), `kit.yaml` (the `fx` entry). Tests in `fx.rs` against a local fixture server, as `osv.rs`'s are.

- `lists_exactly_three_tools`: `tools/list` gives `latest_rates`, `rate_on`, `list_currencies`, each with an input schema, and `serverInfo.name` `farik-fx`. RED.
- `latest_rates_asks_once_and_shapes_the_answer`: the fixture sees one `GET /rates` with `base=USD&quotes=EUR%2CGBP` and nothing else; the answer is `{ base, rates: { EUR: { rate, date }, GBP: { rate, date } } }`, a different date per quote kept as given. RED.
- `rate_on_says_the_day_answered`: asked for 2026-10-03, the fixture answers a row dated 2026-10-02; the tool's `date` is 2026-10-02. RED.
- `list_currencies_keeps_code_and_name`: no `symbol`, `iso_numeric` or dates in the answer. RED.
- `refuses_bad_input_before_sending`: `usd`, `US`, `quotes` empty, 31 codes, a duplicate, `quote == base`, `1998-12-31`, tomorrow, `2026-02-30`: each a tool error, and the fixture sees no request. RED.
- `follows_no_redirect_and_no_proxy`: a 302 from the fixture is an error; with `HTTPS_PROXY` set to a listener, the listener sees nothing. RED.
- `cuts_an_oversized_answer`: a 1 MiB + 1 byte body is "Frankfurter's answer is too large". RED.
- `fx_api_is_frankfurters_v2`: `FX_API` is exactly `https://api.frankfurter.dev/v2`. RED.
- `the_kit_starts_fx_by_its_bare_name_with_its_copy_and_labels` (`kit.rs`): the `fx` entry is `stdio`, `command: farik`, `args: [connector, fx]`, no keys, the four copy fields exactly as Decisions gives them, its three tools `network` with labels "latest exchange rates", "an exchange rate on a day", "list currencies"; `is_farik_connector` holds for `[connector, fx]` and not `[connector, fx, x]`. RED.
- `fx_server_lists_the_kits_tools` (`crates/cli/tests/fx_server.rs`, mirroring `osv_server.rs`'s `osv_server_lists_the_kits_tools`): the built `farik connector fx`, listed through `list_tools`, gives exactly the kit's three tools. RED.

- [ ] `feat(runtime): serve exchange rates through Farik's own server`

### Task 3: Exa, SerpApi, Brex and AWS Pricing

Files: `kit.yaml` `connectors` after `fx`, in this order: `exa`, `serpapi`, `brex`, `aws_pricing`; `kit.rs` tests (`loads_every_shipped_kit`: the Procurement Specialist has 5).

**`exa`**, `transport: http`, `url: https://mcp.exa.ai/mcp`, no keys, no headers. Title "Exa web search". About "Exa searches the web and reads pages, built for assistants that research." Why "So the Procurement Specialist can find makers, sellers and their price pages for anything you need to buy. It only reads." Setup "Nothing to set up: Exa answers a few searches a day with no account. What your agent searches for goes to Exa as written."
- `network`, with labels: `web_search_exa` "search the web", `web_fetch_exa` "read a page".

**`serpapi`**, `transport: http`, `url: https://mcp.serpapi.com/mcp`, `headers: { Authorization: "Bearer {SERPAPI_KEY}" }`, `credential_keys: [SERPAPI_KEY]`, `key_page: https://serpapi.com/manage-api-key`, `allowances: { search: { calls: 50, what: "shopping searches" } }`. Title "Shopping prices". About "SerpApi reads shopping results from Google Shopping, Amazon, eBay and Walmart." Why "So the Procurement Specialist can compare what a product costs across the big shops in one search. Each search uses one of your SerpApi searches, so Farik counts them." Setup "Make a free SerpApi account, which includes 250 searches a month, then copy your private key from its ‘Api Key’ page and paste it here. What your agent searches for goes to SerpApi as written."
- `external_effect`, with its label and allowance: `search` "search shops".
- `denied`: `search_table`, `search_dashboard` (2).

**`aws_pricing`**, `transport: stdio`, `command: uvx`, `args: ["awslabs.aws-pricing-mcp-server==1.1.1"]`, `credential_keys: [AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY]`, `key_page: https://console.aws.amazon.com/iam/home#/users`. Title "AWS prices". About "AWS publishes the price of every one of its services, by region and by plan." Why "So the Procurement Specialist can price an AWS option exactly before anyone buys it. It only reads public prices." Setup "This needs the free program uv on your computer (docs.astral.sh/uv). In your AWS account, make a user that may only read prices: give it a policy allowing pricing:GetProducts, pricing:DescribeServices, pricing:GetAttributeValues, pricing:ListPriceLists and pricing:GetPriceListFileUrl, and nothing else. Make a key for it, then paste the ‘Access key’ and the ‘Secret access key’ here. Reading prices costs nothing."
- `network`, with labels: `get_pricing` "read a service's prices", `get_pricing_service_codes` "list AWS services", `get_pricing_service_attributes` "list what a price depends on", `get_pricing_attribute_values` "list the options for a price", `get_price_list_urls` "find a full price list", `get_bedrock_patterns` "read AI service pricing patterns".
- `denied`: `analyze_cdk_project`, `analyze_terraform_project`, `generate_cost_report` (3).

**`brex`**, `transport: http`, `url: https://api.brex.com/mcp`, `oauth: { scopes: [offline_access, vendors.readonly, expenses.card.readonly, departments.readonly] }`. Title "Brex". About "Brex holds your company's cards, bills and the vendors you pay." Why "So the Procurement Specialist can see what you already pay a vendor, and charges that repeat every month that nobody listed. It only reads." Setup "First, a Brex admin turns on ‘Brex in AI assistants’ in Brex's settings, under beta features. Then sign in with your Brex account and allow Farik to read your vendors, card spending and departments; Farik asks for reading only."
- `network`, with labels: `list_vendors` "list vendors", `get_vendor_by_id` "read a vendor", `list_bills` "list bills", `get_bill_by_id` "read a bill", `list_merchants` "list merchants", `list_merchant_categories` "list merchant types", `list_expense_categories` "list spending categories", `query_expense_analytics` "ask about spending", `list_expenses` "list card charges", `get_expense_by_id` "read a card charge", `list_departments` "list departments" (11).
- `denied` (32): `update_expense_memo`, `upload_card_expense_receipt_from_urls`, `replace_attendees_for_card_expense`, `assign_limit_for_card_expenses`, `submit_feedback`, `get_user_myself`, `get_user_by_id`, `list_users_by_name_or_email`, `list_users`, `list_titles`, `list_roles`, `list_cards`, `get_card_by_id`, `list_my_limits`, `list_business_accounts`, `get_business_account`, `list_banking_transactions`, `get_banking_transaction`, `get_reward_points`, `list_trips`, `list_bookings`, `list_group_events`, `list_cost_centers`, `list_locations`, `list_legal_entities`, `get_expense_policy`, `get_active_integration`, `list_accounting_records`, `list_gl_accounts`, `get_reimbursement_payout_date`, `start_expense_download`, `get_expense_download_result`.

Tests (`kit.rs`):
- `aws_pricing_reads_prices_and_never_the_disk`: `stdio`, `uvx` with that exact pin, the two keys and the key page; the six `network` names exactly; the three `denied`. RED.
- `brex_reads_spend_and_never_writes`: `http` at that URL, `oauth.scopes` exactly the four, no keys or headers; the 11 `network` names exactly; `update_expense_memo`, `list_users`, `get_card_by_id` and `list_banking_transactions` `denied`; 32 `denied`. RED.
- `exa_searches_without_a_key`: `http` at that URL, no keys, no headers; both tools `network`. RED.
- `serpapi_counts_each_search`: `http` at that URL, the one header with its placeholder, the one key and the key page; `search` `external_effect` with `allowances.search` `{ calls: 50, what: "shopping searches" }`; the other two `denied`. RED.
- `the_procurement_kit_never_buys`: connectors exactly `fx`, `exa`, `serpapi`, `brex`, `aws_pricing`, in that order; the only `external_effect` tool is SerpApi's `search`; every `network` tool labelled. RED.

- [ ] `feat(roles): give the Procurement Specialist search, shopping prices, Brex and AWS prices`

### Task 4: Each service connects by name

Files: `daemon/team.rs` test; `live_kit_pins.rs`: its header names Exa, SerpApi, Brex and AWS Pricing among the pinned services, and its skip line for a Farik connector (today hard-coded to `osv_server_lists_the_kits_tools`, `live_kit_pins.rs:60`) says "pinned offline by crates/cli/tests/{name}_server.rs", so `fx` is skipped with a true line.

- `connects_each_procurement_service_by_name` (a guard): for a team with a Procurement Specialist and a Finance Specialist, `kit_entry` is `Ok` and `matches_kit` true for each of the five on the Procurement Specialist; `kit_entry` of `brex` on the Finance Specialist is `connector_not_in_kit`.

- [ ] `test(runtime): connect each of the Procurement Specialist's services by name`

### Task 5: Spec and plan

`docs/SPEC.md` 6.10: "The Procurement Specialist's kit" paragraph, as 6.7's kit paragraphs are (the five servers, their routes, the one allowance, what is `denied` and why); 6.7's "Farik's own connectors" names `fx` beside `osv`, its address, tools, limits and checks; the revision line. `docs/design/role-kits.md` and `docs/design/procurement-specialist.md`: anything changed in execution. Project plan row 10d.

- [ ] `docs(spec): record the Procurement Specialist's kit`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, Exa, SerpApi, Brex and AWS Pricing listed with no drift
```

The live run reads `FARIK_KIT_SERPAPI_SERPAPI_KEY`, `FARIK_KIT_AWS_PRICING_AWS_ACCESS_KEY_ID`, `FARIK_KIT_AWS_PRICING_AWS_SECRET_ACCESS_KEY` and `FARIK_KIT_BREX_BEARER` (a Brex API token from Settings → Developer, made by an admin; if Brex's server refuses an API token as a bearer, the bearer is taken from a Farik sign-in through `farik connect`, and the Execution notes record which was used). Then, in the web app, by the founder: connect all three to a Procurement Specialist, reading each setup copy as a user would, and run step 13's eighth task.

## Execution notes

None yet.
