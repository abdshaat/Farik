# Phase 7, step 10g: Product safety and eBay, through Farik's own servers

Status: ready (executes after steps 10d, 10e and 10f land)
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.10, 8.6; F9
Depends on: the commits of steps 10d, 10e and 10f: ready, not yet executed; execution starts only after they exist. From step 10d: the Procurement Specialist's kit with `fx`, `exa`, `serpapi`, `brex` and `aws-pricing`, and its skills `checking-product-safety`, `checking-a-used-vehicle` and `using-procurement-sources` with their `farik_request_sites` lines; `farik_runtime::fx` as the pattern; `FARIK_CONNECTORS` `["osv", "google-ads", "fx"]`; the tests `the_procurement_kit_never_buys`, `connects_each_procurement_service_by_name` (`daemon/team.rs`), `procurement_kit_carries_its_skills` and `no_procurement_skill_names_a_denied_tool`; `crates/cli/tests/fx_server.rs`; `live_kit_pins.rs`'s skip line naming `crates/cli/tests/{file}_server.rs`. From steps 10e and 10f: the kit's thirteenth and fourteenth skills, and `mail-parser = "=0.11.9"` in `crates/runtime/Cargo.toml` (10f), whose `html_to_text` Task 2 uses. Step 10b2 with its landing-review fix (for a role held to approved sites, the hook judges every string of a connector call's input that parses as an address). Step 07 (ADR 0038, `farik_runtime::osv`; committed at 383a626). Phase 6 (merged in #19). File:line citations are at f2476af; the names are what count.
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-07 (one round, ADR 0032): not ready, 7 Blocking and the Should items, all folded below with the founder's answer; no second round
Decided by the founder, 2026-10-07, in conversation: asked together whether setup should tell the user to declare to eBay that they do not persist eBay data, whether answers should drop the seller's username, whether the skills should say eBay is read only through `ebay`, and whether to keep `ebay` in this step despite its setup, the founder answered "Follow eBay's rules": `ebay` stays, over eBay's official Browse API with the user's own keys (5,000 free searches a day); setup tells the user to declare "Not persisting eBay data" (eBay's marketplace account deletion choice) before the first production call; answers drop the seller's username and keep the feedback score and the listing link; the skills say eBay is read only through `ebay`, never by opening ebay.com pages. Farik's approved sites keep `ebay.com` for now: phase 12 step 01's review of the list decides it, since step 10c's purchase-order address check uses it. ADR 0039 is amended the same day.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

For any product, before the Procurement Specialist recommends it, it can see whether it has been recalled; for a car, it can decode the VIN and read its recalls, complaints and crash ratings; and for anything sold on eBay, it can read live fixed-price listings and their asking prices. Two small servers of Farik's own do it, as OSV and `fx` do (ADR 0038): `recalls`, over the United States' product-safety and vehicle-safety agencies' public data, with no account; and `ebay`, over eBay's official Browse API, with the user's own free eBay developer keys. Every tool only reads. Out of scope: other countries' recall lists (the EU's Safety Gate publishes weekly XML and Health Canada a 15.7 MB daily file, both candidates the design keeps); eBay's official server, which can call any eBay API, writes included; auctions and bids; opening ebay.com's pages, since eBay's User Agreement (posted 2026-01-20, in force 2026-02-20) bars "LLM-driven bots… to access our Services for any purpose, except with the prior express permission of eBay", so Farik reads eBay only through its API; buying anything (ADR 0039).

## Decisions

- **No mockups.** This step adds no screen and changes none. Step 05's key form on `ConnectorAdd` already takes two pasted keys (AWS Pricing's, step 10d), and `recalls` needs no setup; the new copy is text in those screens.
- **Why Farik's own.** No official server exists for the CPSC's or NHTSA's data; eBay's official `@ebay/npm-public-api-mcp` 1.1.0 (still the latest on 2026-10-07) calls any eBay API, writes included, and cannot be narrowed by tags to search. Each public API is a few fixed calls, so, as ADR 0038 decided for OSV, Farik ships thin read-only servers that change only with a Farik release. `FARIK_CONNECTORS` (`kit.rs:167`; its test `google_ads_is_one_of_farik_s_own_connectors`, `:3370`) becomes `["osv", "google-ads", "fx", "recalls", "ebay"]`.
- **The shared shape**, as `osv.rs` and `fx.rs`: a hand-written `rmcp` `ServerHandler` over stdio; one `reqwest::Client` with `redirect::Policy::none()`, `no_proxy()`, no cookies; fixed hosts as constants, never an argument, environment value or tool input; every input checked before any request; queries built with `Url::query_pairs_mut` and path segments pushed with `path_segments_mut`, never by formatting strings; a timeout of 20 seconds; at most 4 MiB read in chunks (`vehicle_complaints` alone 8 MiB, below); answers cut to fixed fields and lengths, each cut said by `<field>_cut: true` or `more: true`; a non-2xx answer said in Farik's words without its body; no address from an answer (`next`, `href`, `itemHref`, `URL`) is ever requested; `serverInfo.name` `farik-recalls` and `farik-ebay`.
- **Approved sites (step 10b2).** No input of either server is an address, so for a role held to approved sites the hook refuses a call only when the agent writes one into `words`. The servers fetch fixed hosts that are on no approved list, which is sound: no input chooses the host, and no redirect is followed (`http://` and bare `saferproducts.gov` both answer 301, which is why the constants are exact). An address in an answer reaches the agent as data, and the hook judges it when the agent passes it to `WebFetch`.
- **`recalls`**, no keys; hosts `CPSC_API`, `NHTSA_API` and `VPIC_API` (Interfaces), all answering keyless on 2026-10-05 and 2026-10-07. Errors: a non-2xx answer is "The CPSC could not answer that" or "NHTSA could not answer that" (vPIC is NHTSA's); an answer over the cap is "The CPSC's answer is too large; ask a narrower question" for `product_recalls` and "NHTSA's answer is too large to read here" for the rest, since a make, model and year cannot be narrowed. Tools:
  - `product_recalls { words, field, since? }`: `GET <CPSC_API>/Recall` with `format=json`, the parameter that `field` names set to `words` (1 to 100 characters), and `RecallDateStart` to `since` (an ISO date). `field` is `title` (`RecallTitle`), `product_name` (`ProductName`) or `product_type` (`ProductType`); never `Title`, which the CPSC's own page gives as its example but which is silently ignored (27 MB came back on 2026-10-07): only `RecallTitle` filters. The answer is the newest 50 by `RecallDate`, `more: true` when cut, each `{ number, date, title, url, description, products, hazards, remedies, manufacturers, retailers, countries }` from `RecallNumber`, `RecallDate`, `Title`, `URL`, `Description` (cut at 2,000 characters, `description_cut: true`), `Products` as `[{ name, model, type, units }]` (`Name`, `Model`, `Type`, `NumberOfUnits`), `Hazards`, `Remedies`, `Manufacturers` and `Retailers` each as its `Name` strings, and `countries` from `ManufacturerCountries[].Country`. Its search is a plain text match ("car mirror" found none where "mirror" found 24 by `ProductName` and 19 by `RecallTitle`), which `checking-product-safety` says: search the product's type and maker's name too.
  - `vehicle_recalls { make, model, model_year }`: `GET <NHTSA_API>/recalls/recallsByVehicle` with the query `make`, `model` and `modelYear`; each `{ campaign, date, component, summary, consequence, remedy, park_it, park_outside }` from `NHTSACampaignNumber`, `ReportReceivedDate`, `Component`, `Summary`, `Consequence`, `Remedy`, `parkIt` and `parkOutSide`.
  - `vehicle_complaints { make, model, model_year }`: `GET <NHTSA_API>/complaints/complaintsByVehicle` with the same query; `{ count, by_component: { <component>: <n> }, newest: [{ date, components, summary }] }`, `components` split at commas and each counted once per complaint, the 20 newest by `dateComplaintFiled` read as MM/DD/YYYY, each `summary` cut at 600 characters. It reads at most 8 MiB: the 2012 Ford Focus's answer was 3.69 MB on 2026-10-07.
  - `vehicle_safety_ratings { make, model, model_year }`: `GET <NHTSA_API>/SafetyRatings/modelyear/<y>/make/<make>/model/<model>`, the only one of the three vehicle tools whose inputs are path segments (checked live, 2026-10-07), then `GET <NHTSA_API>/SafetyRatings/VehicleId/<id>` for each of at most 10 vehicles, `more: true` when cut; a `VehicleId` that is not a positive integer is skipped. Each `{ vehicle, overall, frontal, side, rollover }` from `VehicleDescription`, `OverallRating`, `OverallFrontCrashRating`, `OverallSideCrashRating` and `RolloverRating`.
  - `decode_vin { vin }`: `GET <VPIC_API>/vehicles/decodevinvalues/<vin>?format=json`; only `Make`, `Model`, `ModelYear`, `Trim`, `BodyClass`, `EngineCylinders`, `DisplacementL`, `FuelTypePrimary`, `DriveType`, `PlantCountry`, `ErrorCode` and `ErrorText` (vPIC decoded `1HGCM82633A004352` cleanly on 2026-10-07).
  Inputs: `make` and `model` 1 to 40 of letters, digits, spaces and `-`; `model_year` 1950 to next year (UTC); `vin` 17 of `[A-HJ-NPR-Z0-9]` (no I, O or Q).
- **`ebay`**, `credential_keys: [EBAY_CLIENT_ID, EBAY_CLIENT_SECRET]`, `key_page: https://developer.ebay.com/my/keys`, the user's own keys (ADR 0043). The two keys reach the server only through the launcher's cleared environment (ADR 0030) and are held as `Secret` (`farik_runtime::claude::Secret`), never in an answer, an error or a log. With `EBAY_CLIENT_ID` or `EBAY_CLIENT_SECRET` missing or empty, `farik connector ebay` still lists both tools, and every call is the tool error "eBay is not set up; connect it again with your App ID and Cert ID". Nothing is sent. (`connector_run::ebay` passes a missing variable as an empty `Secret`.)
  - **The grant.** At its first call the server asks `POST <EBAY_API>/identity/v1/oauth2/token`, `Authorization: Basic` of `<id>:<secret>`, the form `grant_type=client_credentials` and `scope=https://api.ebay.com/oauth/api_scope`, and keeps the `access_token` in memory as a `Secret`, asked for again when fewer than 60 seconds of its `expires_in` remain (eBay's lasts 7,200 seconds; grants are limited to 1,000 a day). The grant's `token_type` ("Application Access Token") is not checked. Search and item requests carry only `Authorization: Bearer <token>`, never the Basic header. A refused grant is "eBay refused the App ID and Cert ID; connect it again with your production keys"; a 429 from either endpoint is "eBay's limit for these keys is used up for now; try again later"; a 404 from `get_item` is "eBay has no listing with that id; it may have ended"; any other non-2xx is "eBay could not answer that". The Browse API allows 5,000 calls a day (eBay's call-limits page).
  - `search_items { words, marketplace, condition?, min_price?, max_price?, limit? }`: `GET <EBAY_API>/buy/browse/v1/item_summary/search` with `q` (`words`, 1 to 100 characters, eBay's maximum), `limit` (1 to 50, 20 by default; eBay allows 200) and `filter`; `marketplace` one of `EBAY_US`, `EBAY_GB`, `EBAY_DE`, `EBAY_AU`, `EBAY_CA`, `EBAY_FR`, `EBAY_IT`, `EBAY_ES`, sent as `X-EBAY-C-MARKETPLACE-ID`; `condition` `new` or `used` as `conditions:{NEW}` or `conditions:{USED}`; prices decimal strings (`^[0-9]{1,7}(\.[0-9]{1,2})?$`, `min_price` not above `max_price`) as `price:[<min>..<max>]`, either end open. eBay's filters reference (read 2026-10-07) says `price` "must be used with the priceCurrency filter": `priceCurrency` is the marketplace's currency: US USD, GB GBP, DE/FR/IT/ES EUR, AU AUD, CA CAD; the test asserts `filter=conditions:{USED},price:[10..50],priceCurrency:GBP`. No `buyingOptions` filter is sent, so eBay's default holds: fixed-price (Buy It Now) listings only (`browse_api.json` v1.20.4, 2026-10-07), asking prices and never bids. Each item `{ item_id, title, price, currency, condition, seller_feedback_score, seller_feedback_percent, location, shipping, url }` from `itemId`, `title`, `price.value`, `price.currency`, `condition`, `seller.feedbackScore`, `seller.feedbackPercentage`, `location` `{ country, postal_code }` from `itemLocation.country`/`postalCode`, `shipping` `{ value, currency }` from `shippingOptions[0].shippingCost` (absent when eBay gives none) and `itemWebUrl`; then `total` and `more` (`total` above the items given).
  - `get_item { item_id }`: `GET <EBAY_API>/buy/browse/v1/item/<item_id>`, one segment; the same fields, `description` turned into plain text by `mail_parser::decoders::html::html_to_text` (public in 0.11.9, 10f's crate; it drops the content of `script`, `style`, `head` and `template` as well as the markup) and cut at 4,000 characters (`description_cut: true`), `return_terms` `{ accepted, period: { value, unit } }` from `returnTerms.returnsAccepted` and `returnTerms.returnPeriod`, and `item_specifics` `[{ name, value }]` from `localizedAspects`, at most 30.
  - **No seller's username**, in either answer: eBay's API License Agreement says "You will not under any circumstances collect, store or share any eBay User's User IDs" (the founder's answer). A listing's title and description are the seller's words, untrusted (spec 8.6).
  `item_id` matches `^v1\|[0-9]{6,20}\|[0-9]{1,20}$`.
- **Labels**, every tool `network`: `product_recalls` "look up product recalls", `vehicle_recalls` "look up a car's recalls", `vehicle_complaints` "read a car's complaints", `vehicle_safety_ratings` "read a car's crash ratings", `decode_vin` "decode a VIN", `search_items` "search eBay listings", `get_item` "read an eBay listing". 10d's `the_procurement_kit_never_buys` requires a label on every `network` tool.
- **The copy**, none saying "MCP", "OAuth" or "token", eBay's own labels between ‘ and ’ (step 05's exception, `setup` at most 600 characters). `recalls`: Title "Safety recalls". About "US product and vehicle safety agencies publish every recall, complaint and crash rating." Why "So the Procurement Specialist never recommends a product or a car with an open recall, and can check a used car's VIN. It only reads." Setup "Nothing to set up: Farik reads the CPSC's and NHTSA's public lists itself, with no account. It covers products and vehicles sold in the United States. What your agent looks up goes to them as written." `ebay`: Title "eBay listings". About "eBay's listing search shows what sellers ask for new and used goods right now." Why "So the Procurement Specialist can see real asking prices, and how well rated each seller is, for anything sold on eBay. It only reads; it can never bid or buy." Setup "Make a free eBay developer account and create an application. Before its production keys work, eBay asks how you handle account deletions: choose ‘Not persisting eBay data’, since Farik never keeps an eBay member's name. Then paste the ‘App ID (Client ID)’ and ‘Cert ID (Client Secret)’ from its production keys. Farik only searches and reads listings; eBay allows 5,000 searches a day. What your agent searches for goes to eBay as written."
- **Pins.** Offline through the built binary, `crates/cli/tests/recalls_server.rs` and `ebay_server.rs` (with an empty key map, which the rule for missing keys above lets list), as 10d did for `fx`; `live_kit_pins_hold` skips both with 10d's line. `live_kit_pins.rs` skips Farik's own servers, so a new `live_farik_servers_answer` beside it calls each tool once against the real hosts, by hand, with `FARIK_LIVE_TESTS=1` (Task 2; run in Task 5).
- **Three skills change, none are added.** `checking-product-safety` and `checking-a-used-vehicle` (10d) name the new tools, and keep their `farik_request_sites` lines as the fallback when the server is not connected; `using-procurement-sources` says what each server is for. By the founder's answer, the skills say eBay is read only through `ebay`: never through SerpApi's `ebay` engine and never by opening ebay.com pages. This replaces the review's "prefer `ebay` over SerpApi's `ebay` engine"; SerpApi's copy (10d) is unchanged, since it describes the service. `using-procurement-sources`' description changes, so `procurement_kit_carries_its_skills` changes with it.

## File map

```
crates/runtime/src/recalls.rs, crates/runtime/src/ebay.rs        creates (Tasks 1, 2)
crates/runtime/src/lib.rs                                        modifies: the two modules (Tasks 1, 2)
crates/cli/src/connector_run.rs, crates/cli/src/lib.rs           modifies: `farik connector recalls`, `farik connector ebay` (Tasks 1, 2)
crates/cli/tests/recalls_server.rs, crates/cli/tests/ebay_server.rs   tests (Tasks 1, 2)
crates/roles/src/kit.rs                                          modifies: FARIK_CONNECTORS; tests (Tasks 1 to 3)
crates/roles/roles/procurement_specialist/kit.yaml               modifies: the two entries after `aws-pricing` (Tasks 1, 2)
crates/roles/roles/procurement_specialist/skills/{checking-product-safety,checking-a-used-vehicle,using-procurement-sources}/SKILL.md   modifies (Task 3)
crates/runtime/src/daemon/team.rs                                tests: connects_each_procurement_service_by_name (Tasks 1, 2)
crates/runtime/tests/live_kit_pins.rs                            tests: live_farik_servers_answer (Task 2); header comment (Task 4)
docs/SPEC.md, docs/design/role-kits.md, docs/design/procurement-specialist.md, docs/plans/project-plan.md   modifies (Task 4)
docs/plans/phase-7-role-kits/step-10g-product-safety-and-ebay-sources.md   modifies: Execution notes and Status (Task 5)
```

## Interfaces

Consumes: `osv.rs` and `fx.rs` as the pattern (`osv::serve_stdio`, `osv.rs:489`; `osv::tests::uses_no_proxy_from_the_environment`, `:1021`); `FARIK_CONNECTORS`, `is_farik_connector`, `load_kit` (`farik-roles`); `CliIo`, `ConnectorCommands` (`farik-cli`; `connector_run::osv`, `:135`, and `google_ads`, `:107`, which lists with no ticket, as the pattern); `Secret` (`farik_runtime::claude`, `claude.rs:88`); `mail_parser::decoders::html::html_to_text` (step 10f's crate); `variable` and `live_kit_pins_hold` (`live_kit_pins.rs:44`, `:59`); step 10d's tests named in the header; the launcher's cleared environment.

Produces:

```rust
pub const CPSC_API: &str = "https://www.saferproducts.gov/RestWebServices";   // farik_runtime::recalls
pub const NHTSA_API: &str = "https://api.nhtsa.gov";
pub const VPIC_API: &str = "https://vpic.nhtsa.dot.gov/api";
pub fn tool_names() -> Vec<&'static str>;   // ["product_recalls", "vehicle_recalls", "vehicle_complaints", "vehicle_safety_ratings", "decode_vin"]
pub struct Recalls; impl Recalls { pub fn new(cpsc: &str, nhtsa: &str, vpic: &str) -> Result<Self, RecallsError>; pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String>; }
pub async fn serve_stdio(cpsc: &str, nhtsa: &str, vpic: &str) -> Result<(), RecallsError>;
pub enum RecallsError { Client, Serving(String) }                           // as OsvError
pub const EBAY_API: &str = "https://api.ebay.com";                             // farik_runtime::ebay
pub fn tool_names() -> Vec<&'static str>;   // ["search_items", "get_item"]
pub struct Ebay; impl Ebay { pub fn new(api: &str, client_id: Secret, client_secret: Secret) -> Result<Self, EbayError>; pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String>; }
pub async fn serve_stdio(api: &str, client_id: Secret, client_secret: Secret) -> Result<(), EbayError>;
pub enum EbayError { Client, Serving(String) }
pub fn recalls(io: &mut CliIo<'_>) -> i32; pub fn ebay(io: &mut CliIo<'_>) -> i32;   // farik_cli::connector_run
// ConnectorCommands::{Recalls, Ebay}; FARIK_CONNECTORS = ["osv", "google-ads", "fx", "recalls", "ebay"]
```

## Tasks

Tests in `recalls.rs` and `ebay.rs` run against local fixture servers, as `osv.rs`'s do. Each RED is a compile failure until its symbol exists, then the reason named.

### Task 1: `recalls`

Files: `recalls.rs`, `lib.rs`, `connector_run.rs`, cli `lib.rs` (`ConnectorCommands::Recalls`), `kit.rs` (`FARIK_CONNECTORS` and tests), `kit.yaml` (the `recalls` entry after `aws-pricing`), `daemon/team.rs` (test), `crates/cli/tests/recalls_server.rs`.

- `lists_exactly_five_tools`: `tools/list` gives `tool_names()`'s five in order, each with an input schema, and `serverInfo.name` `farik-recalls`. RED: `farik_runtime::recalls` does not exist.
- `product_recalls_searches_the_chosen_field`: `field: title`, `words: baby`, `since: 2024-01-01` sends one `GET /Recall` whose query is exactly `format=json`, `RecallTitle=baby`, `RecallDateStart=2024-01-01`; `product_name` sends `ProductName` and `product_type` `ProductType`, never `Title`; a fixture of 60 recalls in no order gives the newest 50 by `RecallDate` and `more: true`, each with exactly the Decisions' keys and shapes, a 2,001-character `Description` cut to 2,000 with `description_cut: true`. RED: the tool is unknown.
- `vehicle_recalls_reads_by_query`: `{ make: Honda, model: Accord, model_year: 2003 }` sends `GET /recalls/recallsByVehicle` with exactly `make=Honda`, `model=Accord`, `modelYear=2003`; each answer row has exactly the eight keys, `date` and `park_outside` among them. RED: the tool is unknown.
- `vehicle_complaints_counts_and_keeps_the_newest`: the same query on `/complaints/complaintsByVehicle`; 25 complaints whose text order is not their date order (`12/31/2019` before `01/02/2020`) give `count` 25, `by_component` counting `ENGINE,POWER TRAIN` once under each, and `newest` the 20 latest by date, a 601-character summary cut to 600. RED: the tool is unknown.
- `vehicle_safety_ratings_reads_each_vehicle`: model `Land Cruiser` is sent as the segment `Land%20Cruiser`; 12 vehicles give 10 `VehicleId` requests and `more: true`; a `VehicleId` of `0` or `"x"` is skipped unasked; each row has exactly the five keys. RED: the tool is unknown.
- `decode_vin_keeps_twelve_fields`: `GET /vehicles/decodevinvalues/1HGCM82633A004352` with `format=json`; the answer has exactly the twelve keys. RED: the tool is unknown.
- `refuses_bad_input_before_sending`: a VIN with `O`, one of 16 characters, `model_year` 1949 and next year plus one, a `make` with `/`, empty `words`, 101-character `words`, `field: name`, `since: 2026-02-30`: each a tool error, and the fixtures see no request. RED: a fixture sees a request.
- `a_refusal_hides_the_agencys_words`: a 500 with a body from each host is exactly "The CPSC could not answer that" or "NHTSA could not answer that". RED: the body is in the error.
- `follows_no_redirect`: a 301 from the fixture to a second fixture is "The CPSC could not answer that", and the second sees nothing. RED: the second fixture is asked.
- `uses_no_proxy_from_the_environment`: as `osv.rs:1021`'s. RED: with the client built without `no_proxy()`, the proxy is asked.
- `cuts_an_oversized_answer`: 4 MiB + 1 byte from `product_recalls` is "The CPSC's answer is too large; ask a narrower question", and from `vehicle_recalls` "NHTSA's answer is too large to read here"; `vehicle_complaints` reads 4 MiB + 1 and refuses 8 MiB + 1 with the second message. RED: the answer is returned.
- `the_hosts_are_fixed`: the three constants exactly as Interfaces gives them. RED: the constants do not exist.
- `the_kit_starts_recalls_by_its_bare_name_with_its_copy_and_labels` (`kit.rs`): `stdio`, `command: farik`, `args: [connector, recalls]`, no keys, the four copy fields exactly, its five tools `network` with the Decisions' labels; `is_farik_connector` holds for `[connector, recalls]`. RED: panics "the procurement_specialist kit has no recalls".
- `google_ads_is_one_of_farik_s_own_connectors` (`kit.rs:3370`) asserts `["osv", "google-ads", "fx", "recalls"]`. RED: left `["osv", "google-ads", "fx"]`.
- `the_procurement_kit_never_buys` (10d): six connectors, `fx`, `exa`, `serpapi`, `brex`, `aws-pricing`, `recalls`. RED: left five.
- `loads_every_shipped_kit` (`kit.rs:1047`): the Procurement Specialist leaves 10d's arm for its own, 6. RED: left 5, right 6.
- `connects_each_procurement_service_by_name` (`daemon/team.rs`, 10d) covers `recalls`, not signed in. RED: `kit_entry` is `connector_not_in_kit`.
- `recalls_server_lists_the_kits_tools` (`crates/cli/tests/recalls_server.rs`, as `fx_server.rs`): the built `farik connector recalls` lists exactly the kit's five. RED: `farik connector recalls` is not a command.

- [ ] `feat(runtime): read safety recalls through Farik's own server`

### Task 2: `ebay`

Files: `ebay.rs`, `lib.rs`, `connector_run.rs`, cli `lib.rs` (`ConnectorCommands::Ebay`), `kit.rs`, `kit.yaml` (the `ebay` entry after `recalls`), `daemon/team.rs` (test), `crates/cli/tests/ebay_server.rs`, `crates/runtime/tests/live_kit_pins.rs` (the live test).

- `lists_exactly_two_tools`: `search_items`, `get_item`, and `serverInfo.name` `farik-ebay`. RED: `farik_runtime::ebay` does not exist.
- `asks_for_an_application_grant_once`: two searches make one grant request with the Basic header of the two keys and exactly the form in Decisions, then two searches each with the bearer and no Basic header; a grant answered with `expires_in: 30` is asked for again at the next call. RED: the tool is unknown.
- `search_items_sends_the_marketplace_and_filters`: `EBAY_GB`, `used`, `"10"` to `"50"`, `limit: 5` sends `X-EBAY-C-MARKETPLACE-ID: EBAY_GB`, `limit=5`, and a `filter` whose decoded value is exactly `conditions:{USED},price:[10..50],priceCurrency:GBP` (the fixture decodes the query, so `query_pairs_mut`'s escaping is not asserted); `min_price` alone gives `price:[10..]`, `max_price` alone `price:[..50]`, neither no `price` or `priceCurrency`, and `EBAY_DE` `EUR`; no `buyingOptions`. Each item has exactly the ten keys; the fixture's `seller.username` is in no answer, its `feedbackScore` and `feedbackPercentage` are. RED: the tool is unknown.
- `get_item_strips_markup_and_cuts`: `v1|123456789|0` is sent as one segment; a description with markup, `<script>alert('x')</script>` and `<style>` comes back as its text without the script's or style's content, cut at 4,000 with `description_cut: true`; 40 `localizedAspects` give 30 `item_specifics`; `return_terms` as Decisions; no username. RED: the tool is unknown.
- `never_says_its_keys`: a grant refused with 401 and a body holding both keys is exactly "eBay refused the App ID and Cert ID; connect it again with your production keys"; no answer or error holds either key or the token. RED: the body is in the error.
- `says_each_refusal_in_its_words`: a 429 from the grant endpoint and from a search are each "eBay's limit for these keys is used up for now; try again later"; a 404 from `get_item` "eBay has no listing with that id; it may have ended"; a 500 "eBay could not answer that". RED: the 429 says "eBay could not answer that".
- `without_keys_lists_and_sends_nothing`: with either key empty, both tools are listed and each call is exactly "eBay is not set up; connect it again with your App ID and Cert ID"; the fixture sees nothing. RED: the grant endpoint is asked.
- `refuses_bad_input_before_sending`: marketplace `EBAY_XX`, `item_id` `123`, `limit` 0 and 51, `min_price` `ten`, `min_price` above `max_price`, empty and 101-character `words`, `condition: refurbished`: each a tool error, and the fixture sees no request. RED: the fixture sees a request.
- `follows_no_redirect`; `uses_no_proxy_from_the_environment`; `the_host_is_fixed` (`EBAY_API` exactly `https://api.ebay.com`). RED each, as Task 1's.
- `the_kit_starts_ebay_with_its_two_keys` (`kit.rs`): `stdio`, `command: farik`, `args: [connector, ebay]`, the two `credential_keys` and the `key_page`, the four copy fields exactly (the setup holds "‘Not persisting eBay data’"), both tools `network` with their labels. RED: panics "the procurement_specialist kit has no ebay".
- `google_ads_is_one_of_farik_s_own_connectors` asserts the five of Decisions. RED: left four.
- `the_procurement_kit_never_buys`: seven, `…aws-pricing, recalls, ebay`. RED: left six.
- `loads_every_shipped_kit`: 7. RED: left 6, right 7.
- `connects_each_procurement_service_by_name` covers `ebay`. RED: `connector_not_in_kit`.
- `ebay_server_lists_the_kits_tools` (`crates/cli/tests/ebay_server.rs`, an empty key map): the built `farik connector ebay` lists exactly the kit's two. RED: `farik connector ebay` is not a command.
- `live_farik_servers_answer` (`live_kit_pins.rs`): with `FARIK_LIVE_TESTS=1`, calls each tool once at the real constants, eBay with `FARIK_KIT_EBAY_EBAY_CLIENT_ID` and `FARIK_KIT_EBAY_EBAY_CLIENT_SECRET` (`variable()`'s names, a missing one a panic naming it): `product_recalls { words: mirror, field: title }`; the three vehicle tools for Honda, Accord, 2003; `decode_vin` of `1HGCM82633A004352`, answering `Make` `HONDA` and `ErrorCode` `0`; `search_items { words: baby car mirror, marketplace: EBAY_US, limit: 3 }`, at least one item; `get_item` of its first `item_id`. Each is `Ok`, and all failures are listed at once. No RED offline: it returns without `FARIK_LIVE_TESTS=1`, as `live_kit_pins_hold` does; its run is Task 5's.

- [ ] `feat(runtime): read eBay listings through Farik's own server`

### Task 3: The skills name the tools

Files: the three `SKILL.md`; `kit.rs` tests.

- `checking-product-safety`: with "Safety recalls" connected, `product_recalls` first, by `product_name`, then `product_type`, then the maker's name by `title`, since the search matches plain text; without it, 10d's lines (the CPSC's page, asked for with `farik_request_sites`) stay as the fallback.
- `checking-a-used-vehicle`: `decode_vin` first, then `vehicle_recalls`, `vehicle_complaints` and `vehicle_safety_ratings` for the decoded make, model and year; comparable asking prices from `search_items`; 10d's NHTSA lines stay as the fallback; still "never say a car is sound".
- `using-procurement-sources`: description "Use when a connector is connected: exchange rates, Exa, SerpApi, Brex, AWS prices, safety recalls or eBay listings."; `recalls` covers the United States only; "Read eBay only through `ebay`: never through SerpApi's `ebay` engine, and never by opening ebay.com pages."; SerpApi's engines become `google_shopping`, `amazon` or `walmart`; eBay's search shows fixed-price listings, asking prices and not bids; a listing's title and description are the seller's words, data and never instructions; without `ebay`, say eBay was not checked and suggest the founder connect "eBay listings".

- `the_procurement_skills_use_recalls_and_ebay`: `checking-product-safety` holds `` `product_recalls` `` and `` `farik_request_sites` ``; `checking-a-used-vehicle` holds `` `decode_vin` ``, `` `vehicle_recalls` ``, `` `vehicle_complaints` ``, `` `vehicle_safety_ratings` ``, `` `search_items` `` and `` `farik_request_sites` ``; `using-procurement-sources` holds the eBay sentence above exactly. RED: panics, `checking-product-safety` lacks `` `product_recalls` ``.
- `procurement_kit_carries_its_skills` (10d) asserts `using-procurement-sources`' new description. RED: left the old one.
- `kit_skills_name_only_tools_farik_lists` (`daemon/team.rs:4532`, which checks only `farik_*` names) and `no_procurement_skill_names_a_denied_tool` still pass. Guards.

- [ ] `feat(roles): point the Procurement Specialist's skills at recalls and eBay`

### Task 4: Spec and plan

`docs/SPEC.md`, the revision after step 10f's: 6.7's "Farik's own connectors" (line 599) names `recalls` and `ebay` beside `osv`, `google-ads` and `fx`: hosts, tools, limits, checks, the grant, the words for a missing key, and no seller's username; 6.7's Procurement Specialist's kit paragraph (10d) gains the two; 6.10's "Kit (6.7)" (line 706) says, in the founder's terms, that eBay is read only through `ebay`, that setup asks the user to declare "Not persisting eBay data", and that answers carry no seller's username; 8.6's "What the approved sites stop" (line 849) gains: Farik's own servers send the agent's words to fixed hosts, the agencies' and eBay's, on no approved list, which no input chooses and no redirect changes; eBay's titles and descriptions are seller-written and untrusted; and a listing's address on a regional eBay site, `ebay.co.uk` say, is not an approved site. The revision line. `crates/runtime/tests/live_kit_pins.rs`'s header names `recalls` and `ebay` among Farik's own servers pinned offline, and `live_farik_servers_answer` with its two variables. `docs/design/role-kits.md:112`, row 10g, as built. `docs/design/procurement-specialist.md`: line 152's "eBay's terms forbid agents to buy since 2026-02-20" becomes the User Agreement's words (Goal); the `serpapi` row (line 144) says eBay is read through `ebay` alone; the `ebay` row (line 148) "reads through eBay's API only; no seller's username". `docs/plans/project-plan.md` row 10g (line 540): `FARIK_CONNECTORS` `[osv, google-ads, fx, recalls, ebay]`, and "a draft until step 10f lands" dropped.

- [ ] `docs(spec): record the recalls and eBay sources`

### Task 5: The founder's live run

Gate: Tasks 1 to 4 landed and landing-reviewed; no agent holds the founder's keys. The founder runs and reports; the executor commits what the founder reports.

The last task: the founder runs `FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins`. Its new `live_farik_servers_answer` calls each tool once against the real hosts, eBay with `FARIK_KIT_EBAY_EBAY_CLIENT_ID` and `FARIK_KIT_EBAY_EBAY_CLIENT_SECRET` (the `variable()` convention). Then the web-app run in Verification, recorded in `docs(plans): record 10g's live run`.

- [ ] **The live check.** The founder makes eBay's keys as the setup copy says, declaring ‘Not persisting eBay data’ first, and reports that label as eBay's portal shows it; then the run above, with every earlier kit's variable set too, since `live_kit_pins_hold` panics on the first one missing (step 10d's Task 6).
- [ ] **The web app**, as Verification says.

Executor, from the report, each in its own commit with the tests that change with it: a different label in eBay's portal, in the copy and its test (`fix(roles): name eBay's account deletion choice as eBay shows it`); a field a live answer lacks or names otherwise, in the server and its fixture test (`fix(runtime): read <service>'s answer as it is`). Then the Execution notes record the report, and Status says the run passed.

- [ ] `docs(plans): record 10g's live run`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: test live_farik_servers_answer ... ok; test live_kit_pins_hold ... ok, recalls and ebay skipped, pinned offline by crates/cli/tests/recalls_server.rs and ebay_server.rs
```

Then, by the founder (Task 5): connect both to a Procurement Specialist, reading each setup copy as a user would; ask it to source a rear-facing baby car mirror (it reads recalls before recommending) and to price a used Honda Accord by its VIN, `1HGCM82633A004352` (decode, recalls, complaints, ratings, eBay asking prices, no ebay.com page opened). The step is not done until it passes.

## Execution notes

None yet.
