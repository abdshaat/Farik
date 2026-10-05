# Phase 7, step 10g: Product safety and eBay, through Farik's own servers

Status: draft. Its readiness review runs once step 10f has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 6.10; F9
Depends on: step 10d of this phase (the kit, `fx` as the second of Farik's own servers, the `crates/cli/tests/<name>_server.rs` offline pin); step 07 (ADR 0038, `farik_runtime::osv`); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

For any product, before the Procurement Specialist recommends it, it can see whether it has been recalled; for a car, it can decode the VIN and read its recalls, complaints and crash ratings; and for anything sold on eBay, it can read live listings and their prices. Two small servers of Farik's own do it, as OSV and `fx` do (ADR 0038): `recalls`, over the United States' product-safety and vehicle-safety agencies' public data, with no account; and `ebay`, over eBay's public listing search, with a free eBay developer key. Every tool only reads. Out of scope: other countries' recall lists (the EU's Safety Gate publishes weekly XML and Health Canada a 15.7 MB daily file, both candidates the design keeps); eBay's official server, which can call any eBay API, writes included; buying on eBay, which eBay's terms forbid to agents since 2026-02-20.

## Decisions

- **Why Farik's own.** No official server exists for the CPSC's or NHTSA's data; eBay's official `@ebay/npm-public-api-mcp` 1.1.0 calls any eBay API, writes included, and cannot be narrowed by tags to search. Each public API is a few fixed calls, so, as ADR 0038 decided for OSV, Farik ships thin read-only servers that change only with a Farik release. `FARIK_CONNECTORS` becomes `["osv", "fx", "recalls", "ebay"]`.
- **The shared shape**, as `osv.rs` and `fx.rs`: a hand-written `rmcp` `ServerHandler` over stdio; one `reqwest::Client` with `redirect::Policy::none()`, `no_proxy()`, no cookies; fixed hosts as constants, never an argument, environment value or tool input; every input checked before any request; queries built with `Url::query_pairs_mut`; a timeout of 20 seconds; at most 4 MiB read in chunks, then "<service>'s answer is too large; ask a narrower question"; a non-2xx answer said in Farik's words without its body; answers cut to fixed fields and lengths, each cut said; `serverInfo.name` `farik-recalls` and `farik-ebay`.
- **`recalls`**, no keys; hosts `https://www.saferproducts.gov/RestWebServices`, `https://api.nhtsa.gov` and `https://vpic.nhtsa.dot.gov/api`, all answering keyless on 2026-10-05:
  - `product_recalls { words, field, since? }`: CPSC's `Recall?format=json` with `RecallTitle`, `ProductName` or `ProductType` (by `field`) set to `words` (1 to 100 characters) and `RecallDateStart` to `since` (an ISO date); newest first, at most 50, each `{ number, date, title, url, products, hazards, remedies, manufacturers, retailers, countries }`, `description` cut at 2,000 characters; `more: true` when cut. Its search is a plain text match ("car mirror" found none where "mirror" found 24), which `checking-product-safety` says: search the product's type and maker's name too.
  - `vehicle_recalls { make, model, model_year }`: NHTSA's `recalls/recallsByVehicle`; each `{ campaign, component, summary, consequence, remedy, park_it }`.
  - `vehicle_complaints { make, model, model_year }`: NHTSA's `complaints/complaintsByVehicle`; the count, the count per component, and the 20 newest summaries cut at 600 characters each.
  - `vehicle_safety_ratings { make, model, model_year }`: NHTSA's `SafetyRatings/modelyear/<y>/make/<m>/model/<m>` then each vehicle's ratings; overall, frontal, side and rollover stars only.
  - `decode_vin { vin }`: vPIC's `vehicles/decodevinvalues/<vin>?format=json`; only `Make`, `Model`, `ModelYear`, `Trim`, `BodyClass`, `EngineCylinders`, `DisplacementL`, `FuelTypePrimary`, `DriveType`, `PlantCountry`, `ErrorCode` and `ErrorText`.
  Inputs: `make` and `model` 1 to 40 of letters, digits, spaces and `-`; `model_year` 1950 to next year; `vin` 17 of `[A-HJ-NPR-Z0-9]` (no I, O or Q). Each path segment is pushed with `path_segments_mut`. All five tools `network`.
- **`ebay`**, `credential_keys: [EBAY_CLIENT_ID, EBAY_CLIENT_SECRET]`, `key_page: https://developer.ebay.com/my/keys`; eBay's Browse API allows 5,000 calls a day per application, free. The server gets an application access grant from `https://api.ebay.com/identity/v1/oauth2/token` (client credentials, scope `https://api.ebay.com/oauth/api_scope`) at its first call and again when it expires, kept in memory only; the two keys reach it only through the launcher's cleared environment (ADR 0030) and are never in an answer or an error. Tools:
  - `search_items { words, marketplace, condition?, min_price?, max_price?, limit? }`: `buy/browse/v1/item_summary/search`; `marketplace` one of `EBAY_US`, `EBAY_GB`, `EBAY_DE`, `EBAY_AU`, `EBAY_CA`, `EBAY_FR`, `EBAY_IT`, `EBAY_ES` (sent as `X-EBAY-C-MARKETPLACE-ID`); `condition` `new` or `used`; prices decimal strings; `limit` 1 to 50 (20 by default); each item `{ item_id, title, price, currency, condition, seller, seller_feedback_percent, location, shipping, url }`; `total` and `more`.
  - `get_item { item_id }`: `buy/browse/v1/item/<id>`; the same fields and `description` cut at 4,000 characters as plain text (markup stripped), `return_terms`, `item_specifics` (at most 30 pairs).
  `item_id` matches `^v1\|[0-9]{6,20}\|[0-9]{1,20}$`; `words` 1 to 100 characters. Both `network`.
- **The copy.** `recalls`: Title "Safety recalls". About "US product and vehicle safety agencies publish every recall, complaint and crash rating." Why "So the Procurement Specialist never recommends a product or a car with an open recall, and can check a used car's VIN. It only reads." Setup "Nothing to set up: Farik reads the CPSC's and NHTSA's public lists itself, with no account. It covers products and vehicles sold in the United States." `ebay`: Title "eBay listings". About "eBay's public listing search shows what sellers ask for new and used goods right now." Why "So the Procurement Specialist can see real asking prices and sellers for anything sold on eBay. It only reads; it can never bid or buy." Setup "Make a free eBay developer account, create an application, and paste its ‘App ID (Client ID)’ and ‘Cert ID (Client Secret)’ from the production keys. Farik only searches; eBay allows 5,000 searches a day."
- **Pins.** Both are pinned offline through the built binary, `crates/cli/tests/recalls_server.rs` and `ebay_server.rs`, as 10d set up for `fx`.
- **Two skills change, none are added.** `checking-product-safety` and `checking-a-used-vehicle` (10d) name the tools; `using-procurement-sources` says what each server is for.

## File map

```
crates/runtime/src/recalls.rs, crates/runtime/src/ebay.rs        creates (Tasks 1, 2)
crates/runtime/src/lib.rs                                        modifies: the two modules (Tasks 1, 2)
crates/cli/src/connector_run.rs, crates/cli/src/lib.rs           modifies: `farik connector recalls`, `farik connector ebay` (Tasks 1, 2)
crates/cli/tests/recalls_server.rs, crates/cli/tests/ebay_server.rs   tests (Tasks 1, 2)
crates/roles/src/kit.rs                                          modifies: FARIK_CONNECTORS; tests (Tasks 1, 2)
crates/roles/roles/procurement_specialist/kit.yaml, skills/{checking-product-safety,checking-a-used-vehicle,using-procurement-sources}/SKILL.md   modifies (Tasks 1 to 3)
docs/SPEC.md, docs/design/procurement-specialist.md, docs/plans/project-plan.md   modifies (Task 4)
```

## Interfaces

Consumes: `osv.rs` and `fx.rs` as the pattern; `FARIK_CONNECTORS`, `is_farik_connector`; `CliIo`, `ConnectorCommands`; the launcher's cleared environment.

Produces:

```rust
pub const CPSC_API: &str = "https://www.saferproducts.gov/RestWebServices";   // farik_runtime::recalls
pub const NHTSA_API: &str = "https://api.nhtsa.gov";
pub const VPIC_API: &str = "https://vpic.nhtsa.dot.gov/api";
pub struct Recalls; impl Recalls { pub fn new(cpsc: &str, nhtsa: &str, vpic: &str) -> Result<Self, RecallsError>; pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String>; }
pub const EBAY_API: &str = "https://api.ebay.com";                             // farik_runtime::ebay
pub struct Ebay; impl Ebay { pub fn new(api: &str, client_id: String, client_secret: String) -> Result<Self, EbayError>; pub async fn call(&self, tool: &str, input: &Value) -> Result<Value, String>; }
pub fn recalls(io: &mut CliIo<'_>) -> i32; pub fn ebay(io: &mut CliIo<'_>) -> i32;   // farik_cli::connector_run
```

## Tasks

### Task 1: `recalls`

Tests against local fixtures, as `osv.rs`'s.

- `lists_exactly_five_tools` and `serverInfo.name` `farik-recalls`. RED.
- `product_recalls_searches_the_chosen_field`: `field: title` sends `RecallTitle=baby`, and `since` sends `RecallDateStart`; the answer keeps only the listed fields, newest first, at most 50, `more` when cut. RED.
- `vehicle_tools_read_their_paths`: each of the three sends exactly its path and parameters, segments encoded. RED.
- `decode_vin_keeps_twelve_fields`. RED.
- `refuses_bad_input_before_sending`: a VIN with `O`, 16 characters, `model_year` 1949, a `make` with `/`, empty `words`; the fixture sees nothing. RED.
- `follows_no_redirect_and_no_proxy`; `cuts_an_oversized_answer`; `the_hosts_are_fixed` (the three constants exactly). RED each.
- `the_kit_starts_recalls_by_its_bare_name_with_its_copy_and_labels` (`kit.rs`) and `recalls_server_lists_the_kits_tools` (the built binary). RED each.

- [ ] `feat(runtime): read safety recalls through Farik's own server`

### Task 2: `ebay`

- `asks_for_an_application_grant_once`: two searches make one request to the grant endpoint with the client-credentials form and basic auth from the two keys, then two searches with the bearer; an expired grant is asked for again. RED.
- `search_items_sends_the_marketplace_and_filters`: `X-EBAY-C-MARKETPLACE-ID: EBAY_GB`, `conditions:{USED}` and the price range in `filter`; the answer keeps only the listed fields. RED.
- `get_item_strips_markup_and_cuts`: a description with `<script>` comes back as text, cut at 4,000. RED.
- `never_says_its_keys`: a refused grant's error holds neither key, and no answer does. RED.
- `refuses_bad_input_before_sending`: an unknown marketplace, an `item_id` of another shape, `limit` 51. RED.
- `follows_no_redirect_and_no_proxy`; `the_host_is_fixed`. RED each.
- `the_kit_starts_ebay_with_its_two_keys` (`kit.rs`) and `ebay_server_lists_the_kits_tools` (the built binary). RED each.

- [ ] `feat(runtime): read eBay listings through Farik's own server`

### Task 3: The skills name the tools

- `kit_skills_name_only_tools_farik_lists` (step 06's guard) still passes with the tool names added to the three skills; `the_procurement_kit_never_buys` (10d) now lists seven connectors. Guards, no RED.

- [ ] `feat(roles): point the Procurement Specialist's skills at recalls and eBay`

### Task 4: Spec and plan

`docs/SPEC.md` 6.7 "Farik's own connectors" names `recalls` and `ebay`, their hosts, tools, limits and checks; 6.10's kit paragraph; the revision line. Project plan row 10g.

- [ ] `docs(spec): record the recalls and eBay sources`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

Then, by the founder: connect both to a Procurement Specialist; ask it to source a rear-facing baby car mirror (it reads recalls before recommending) and to price a used Honda Accord by its VIN, `1HGCM82633A004352` (decode, recalls, complaints, ratings, eBay asking prices).

## Execution notes

None yet.
