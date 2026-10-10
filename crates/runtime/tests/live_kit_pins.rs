//! Every shipped `stdio` and `http` kit connector lists exactly the tools its kit pins (ADR 0036).
//! A tool's tag is Catervas's judgment about another company's tool, so a change at the service is
//! caught here, in the repository, and a pin update re-reviews every tag. It talks to the real
//! services with the founder's keys, only by hand, with `CATERVAS_LIVE_TESTS=1`; it never runs in
//! CI, where it says so and returns. Each key comes from `CATERVAS_KIT_<NAME>_<KEY>` (`<NAME>`
//! upper-cased, `-` as `_`), and a service the user signs in to takes a bearer from
//! `CATERVAS_KIT_<NAME>_BEARER`.
//!
//! The Product Manager's three signed-in services are the shipped ones since step 06; the
//! Architect's Context7 (signed in: `CATERVAS_KIT_CONTEXT7_BEARER`) and Grep (no key) join them, and
//! the Marketing Specialist's Higgsfield (`CATERVAS_KIT_HIGGSFIELD_BEARER`) and Recraft
//! (`CATERVAS_KIT_RECRAFT_BEARER`) since step 08, and its Buffer (`CATERVAS_KIT_BUFFER_BEARER`) and Kit
//! (`CATERVAS_KIT_KIT_BEARER`) since step 08b. Recraft's nine names are those of its own package,
//! whose remote server publishes none, so this run is where they are first checked. GitHub is in
//! both the Product Manager's and the Architect's kit since step 07c, with a key the user pastes:
//! one name, so one variable, `CATERVAS_KIT_GITHUB_GITHUB_KEY`, set to a fine-grained key, which
//! lists every tool whatever it may do (one that reaches public repositories only is enough);
//! each kit's headers narrow what the server lists, so the two lists are compared apart. From the
//! web launch GitHub signs in through Catervas Cloud and the variable is `CATERVAS_KIT_GITHUB_BEARER`
//! (ADR 0044).
//!
//! The Finance Specialist's three services are signed in to, each with a bearer: Stripe's
//! (`CATERVAS_KIT_STRIPE_BEARER`) from an MCP Inspector sign-in to a Stripe sandbox granted every
//! permission, Digits' (`CATERVAS_KIT_DIGITS_BEARER`) from an MCP Inspector sign-in, and Kick's
//! (`CATERVAS_KIT_KICK_BEARER`) from a sign-in granted `mcp:read` and `mcp:write` or from a
//! user-scoped personal key. The run only lists tools, so a wide grant is what shows every tool
//! each service has; the kit itself asks Kick for `mcp:read` alone. Kick lists fewer tools to a
//! narrower grant, so the run also records what an `mcp:read` grant lists.
//!
//! The Procurement Specialist's four third-party services join them since step 10d, each with what
//! it needs: Exa takes no key; `SerpApi` takes `CATERVAS_KIT_SERPAPI_SERPAPI_KEY`, a free account's
//! private key; AWS Pricing takes `CATERVAS_KIT_AWS_PRICING_AWS_ACCESS_KEY_ID` and
//! `CATERVAS_KIT_AWS_PRICING_AWS_SECRET_ACCESS_KEY`, a key of a user allowed only the pricing reads,
//! and needs the program `uv`; and Brex, which the user signs in to, takes `CATERVAS_KIT_BREX_BEARER`,
//! which is an API token from Brex's Developer settings, made by an admin after the Developer API
//! agreement, since Brex's server accepts one as a bearer. Run it twice and note the first and
//! the second time `aws-pricing` takes to list: the first downloads Python and the package.
//!
//! A connector Catervas runs itself (`command: catervas`: the Architect's OSV, the Marketing
//! Specialist's Google Ads and the Procurement Specialist's `fx`, `recalls` and `ebay`) is
//! skipped, with a line naming its pin: the offline test `crates/cli/tests/{file}_server.rs`,
//! `{file}` the connector's name with each `-` made `_`, since its tools are Catervas's own and
//! change only with a Catervas release. The comparison is also proven by `pin_drift`'s tests and
//! `fixture_mcp.rs`.
//!
//! What `recalls` and `ebay` answer is first seen by `live_catervas_servers_answer`, which calls each
//! of their tools once at the real hosts, by hand, with `CATERVAS_LIVE_TESTS=1`. `recalls` needs no
//! key; `ebay` takes the founder's own developer keys from `CATERVAS_KIT_EBAY_EBAY_CLIENT_ID` and
//! `CATERVAS_KIT_EBAY_EBAY_CLIENT_SECRET`, and a missing one is a panic naming it.

use std::collections::BTreeMap;

use catervas_core::team::{CustomTransport, McpServerSource, custom_server};
use catervas_roles::{KitConnector, SHIPPED_ROLES, is_catervas_connector, load_kit, pin_drift};
use catervas_runtime::claude::Secret;
use catervas_runtime::connectors::list_tools;
use catervas_runtime::ebay::{EBAY_API, Ebay};
use catervas_runtime::recalls::{CPSC_API, NHTSA_API, Recalls, VPIC_API};
use serde_json::{Value, json};

/// The integration test that pins one of Catervas's own connectors offline: `crates/cli/tests/` and
/// the connector's name with each `-` made `_`, then `_server.rs`.
fn offline_pin(connector: &str) -> String {
    format!("crates/cli/tests/{}_server.rs", connector.replace('-', "_"))
}

/// The variable holding `connector`'s `key` for the live run.
fn variable(connector: &str, key: &str) -> String {
    format!(
        "CATERVAS_KIT_{}_{key}",
        connector.to_uppercase().replace('-', "_")
    )
}

fn secret(connector: &str, key: &str) -> Secret {
    let name = variable(connector, key);
    Secret::new(std::env::var(&name).unwrap_or_else(|_| {
        panic!("the kit connector {connector} needs {name} set to list its tools")
    }))
}

#[tokio::test]
async fn live_kit_pins_hold() {
    if std::env::var("CATERVAS_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set CATERVAS_LIVE_TESTS=1 to list every kit connector's live tools");
        return;
    }
    let folder = std::env::temp_dir();
    // Every drift is listed at once, so one run is enough to fix them all.
    let mut drifted: Vec<String> = Vec::new();
    for role in SHIPPED_ROLES {
        let kit = load_kit(role).expect("a shipped kit loads");
        for connector in &kit.connectors {
            let KitConnector::Server { entry, .. } = connector else {
                continue;
            };
            let name = entry.name.as_str();
            let mut wire = entry.clone();
            wire.source = McpServerSource::Kit;
            let server = custom_server(&wire).expect("a kit entry");
            if let CustomTransport::Stdio { command, args, .. } = &server.transport
                && is_catervas_connector(command, args)
            {
                eprintln!(
                    "skipped {role}'s {name}: Catervas's own server, pinned offline by {}",
                    offline_pin(&args[1])
                );
                continue;
            }
            let signs_in = matches!(
                &server.transport,
                CustomTransport::Http { oauth: Some(_), .. }
            );
            let keys: BTreeMap<String, Secret> = server
                .credential_keys
                .iter()
                .map(|key| (key.clone(), secret(name, key)))
                .collect();
            let bearer = signs_in.then(|| secret(name, "BEARER"));
            let listed = list_tools(
                &server,
                &keys,
                bearer.as_ref(),
                &folder,
                std::path::Path::new("catervas"),
            )
            .await
            .unwrap_or_else(|error| panic!("{role}'s {name} could not be listed: {error:?}"));
            // A name Claude Code would rewrite is never offered, so no pin names it.
            let usable: Vec<String> = listed
                .iter()
                .filter(|tool| tool.usable)
                .map(|tool| tool.name.clone())
                .collect();
            let drift = pin_drift(&server.tools, &usable);
            if !(drift.added.is_empty() && drift.removed.is_empty()) {
                drifted.push(format!(
                    "{role}'s {name}: added {:?}, dropped {:?}",
                    drift.added, drift.removed
                ));
            }
        }
    }
    assert!(
        drifted.is_empty(),
        "kit pins drifted; a pin update re-reviews every tag:\n{}",
        drifted.join("\n")
    );
}

#[test]
fn names_the_offline_pin_of_each_catervas_connector() {
    assert_eq!(offline_pin("fx"), "crates/cli/tests/fx_server.rs");
    assert_eq!(
        offline_pin("google-ads"),
        "crates/cli/tests/google_ads_server.rs"
    );
    // Every connector of Catervas's own in a shipped kit has the test its skip line names.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found: Vec<String> = Vec::new();
    for role in SHIPPED_ROLES {
        for connector in &load_kit(role).expect("a shipped kit loads").connectors {
            let KitConnector::Server { entry, .. } = connector else {
                continue;
            };
            let server = custom_server(entry).expect("a kit entry");
            if let CustomTransport::Stdio { command, args, .. } = &server.transport
                && is_catervas_connector(command, args)
            {
                let pin = offline_pin(&args[1]);
                assert!(
                    root.join(&pin).is_file(),
                    "{role}'s {}: no {pin}",
                    server.name
                );
                found.push(args[1].clone());
            }
        }
    }
    found.sort();
    assert_eq!(found, ["ebay", "fx", "google-ads", "osv", "recalls"]);
}

#[test]
fn names_the_variable_a_live_run_reads() {
    assert_eq!(
        variable("my-service", "API_KEY"),
        "CATERVAS_KIT_MY_SERVICE_API_KEY"
    );
    assert_eq!(variable("notion", "BEARER"), "CATERVAS_KIT_NOTION_BEARER");
}

/// Whether `answer` is what the live run should get from `tool`: rows that exist, each with the
/// fields Catervas reads from the service as text. `scalar` and `plain` turn a field that is missing
/// or renamed into null, so a check that accepts any answer cannot see the drift this run is for.
fn live_answer_holds(tool: &str, answer: &Value) -> bool {
    // Every one of `fields` of `row` is text.
    let text = |row: &Value, fields: &[&str]| fields.iter().all(|field| row[*field].is_string());
    // The first row of the list `list` holds `fields` as text, so the list is not empty.
    let first = |list: &str, fields: &[&str]| {
        answer[list]
            .as_array()
            .and_then(|rows| rows.first())
            .is_some_and(|row| text(row, fields))
    };
    match tool {
        "product_recalls" => first("recalls", &["number", "date", "title"]),
        "vehicle_recalls" => first("recalls", &["campaign", "component", "summary"]),
        "vehicle_complaints" => {
            answer["count"].as_u64().is_some_and(|count| count > 0)
                && answer["by_component"]
                    .as_object()
                    .is_some_and(|components| !components.is_empty())
                && first("newest", &["date", "summary"])
        }
        "vehicle_safety_ratings" => first("ratings", &["vehicle"]),
        "decode_vin" => answer["Make"] == "HONDA" && answer["ErrorCode"] == "0",
        "search_items" => first("items", &["item_id", "title", "price", "currency", "url"]),
        "get_item" => text(answer, &["item_id", "title", "price"]),
        _ => false,
    }
}

/// Every tool the live run calls, with an answer that holds what a real one does and the places
/// in it a field the service renamed would leave null (`scalar` and `plain` turn a field that is
/// missing into null, so a renamed field never fails by itself).
fn live_samples() -> Vec<(&'static str, Value, Vec<&'static str>)> {
    vec![
        (
            "product_recalls",
            json!({ "recalls": [{ "number": "24-100", "date": "2024-02-29T00:00:00",
                "title": "Baby mirrors recalled" }], "more": false }),
            vec!["/recalls/0/number", "/recalls/0/date", "/recalls/0/title"],
        ),
        (
            "vehicle_recalls",
            json!({ "recalls": [{ "campaign": "18V268", "date": "26/04/2018",
                "component": "AIR BAGS", "summary": "The inflator can rupture." }] }),
            vec![
                "/recalls/0/campaign",
                "/recalls/0/component",
                "/recalls/0/summary",
            ],
        ),
        (
            "vehicle_complaints",
            json!({ "count": 25, "by_component": { "ENGINE": 12 },
                "newest": [{ "date": "01/02/2020", "summary": "It stalled." }] }),
            vec![
                "/count",
                "/by_component",
                "/newest/0/date",
                "/newest/0/summary",
            ],
        ),
        (
            "vehicle_safety_ratings",
            json!({ "ratings": [{ "vehicle": "2003 Honda Accord 4 DR", "overall": "5" }],
                "more": false }),
            vec!["/ratings/0/vehicle"],
        ),
        (
            "decode_vin",
            json!({ "Make": "HONDA", "ErrorCode": "0", "Model": "Accord" }),
            vec!["/Make", "/ErrorCode"],
        ),
        (
            "search_items",
            json!({ "items": [{ "item_id": "v1|110000000001|0", "title": "Baby car mirror",
                "price": "12.99", "currency": "USD",
                "url": "https://www.ebay.com/itm/110000000001" }], "total": 1, "more": false }),
            vec![
                "/items/0/item_id",
                "/items/0/title",
                "/items/0/price",
                "/items/0/currency",
                "/items/0/url",
            ],
        ),
        (
            "get_item",
            json!({ "item_id": "v1|110000000001|0", "title": "Baby car mirror", "price": "12.99",
                "currency": "USD" }),
            vec!["/item_id", "/title", "/price"],
        ),
    ]
}

/// The live run's own check has to be able to fail on the drift it exists to find: a field the
/// agencies or eBay renamed comes out of Catervas's servers as null, and a list that is empty is not
/// an answer. Offline, so it runs in every check.
#[test]
fn the_live_checks_refuse_answers_with_renamed_fields() {
    let samples = live_samples();
    assert_eq!(samples.len(), 7);
    for (tool, present, fields) in samples {
        assert!(
            live_answer_holds(tool, &present),
            "{tool} refuses an answer with every field: {present}"
        );
        // Each field alone gone.
        for field in &fields {
            let mut renamed = present.clone();
            *renamed.pointer_mut(field).expect("a place in the sample") = Value::Null;
            assert!(
                !live_answer_holds(tool, &renamed),
                "{tool} accepts an answer whose {field} is null: {renamed}"
            );
        }
        // Every field gone at once.
        let mut all = present.clone();
        for field in &fields {
            *all.pointer_mut(field).expect("a place in the sample") = Value::Null;
        }
        assert!(
            !live_answer_holds(tool, &all),
            "{tool} accepts an answer whose fields are all null: {all}"
        );
        // A list with no rows, and an answer with nothing in it.
        assert!(
            !live_answer_holds(
                tool,
                &json!({ "recalls": [], "ratings": [], "items": [], "newest": [], "count": 0, "by_component": {} })
            ),
            "{tool} accepts an answer with no rows"
        );
        assert!(
            !live_answer_holds(tool, &json!({})),
            "{tool} accepts an empty answer"
        );
    }
    // A tool the run does not call is never held.
    assert!(!live_answer_holds("other_tool", &json!({})));
}

/// Catervas's own servers `recalls` and `ebay`, called once per tool against the real hosts, by hand
/// (step 10g): the agencies' public data, and eBay with the founder's own developer keys in
/// `CATERVAS_KIT_EBAY_EBAY_CLIENT_ID` and `CATERVAS_KIT_EBAY_EBAY_CLIENT_SECRET`. Their tool lists are
/// pinned offline; this is where what each real answer holds is first seen. Every failure is
/// listed at once.
#[tokio::test]
async fn live_catervas_servers_answer() {
    if std::env::var("CATERVAS_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!(
            "skipped: set CATERVAS_LIVE_TESTS=1 to call Catervas's own servers at their real hosts"
        );
        return;
    }
    // A missing key is a panic naming it, before anything is asked of anyone.
    let client_id = secret("ebay", "EBAY_CLIENT_ID");
    let client_secret = secret("ebay", "EBAY_CLIENT_SECRET");
    let mut failed: Vec<String> = Vec::new();
    let mut check = |tool: &str,
                     answered: Result<Value, String>,
                     holds: &dyn Fn(&Value) -> bool| {
        match answered {
            Ok(answer) if holds(&answer) => {}
            Ok(answer) => failed.push(format!("{tool} answered without what it should: {answer}")),
            Err(why) => failed.push(format!("{tool}: {why}")),
        }
    };

    let recalls = Recalls::new(CPSC_API, NHTSA_API, VPIC_API).expect("the recalls server");
    let honda = json!({ "make": "Honda", "model": "Accord", "model_year": 2003 });
    for (tool, input) in [
        (
            "product_recalls",
            json!({ "words": "mirror", "field": "title" }),
        ),
        ("vehicle_recalls", honda.clone()),
        ("vehicle_complaints", honda.clone()),
        ("vehicle_safety_ratings", honda),
        ("decode_vin", json!({ "vin": "1HGCM82633A004352" })),
    ] {
        check(tool, recalls.call(tool, &input).await, &|answer| {
            live_answer_holds(tool, answer)
        });
    }

    let ebay = Ebay::new(EBAY_API, client_id, client_secret).expect("the eBay server");
    let searched = ebay
        .call(
            "search_items",
            &json!({ "words": "baby car mirror", "marketplace": "EBAY_US", "limit": 3 }),
        )
        .await;
    let first = searched
        .as_ref()
        .ok()
        .and_then(|answer| answer["items"][0]["item_id"].as_str().map(str::to_string));
    let searched_ok = searched.is_ok();
    check("search_items", searched, &|answer| {
        live_answer_holds("search_items", answer)
    });
    match first {
        Some(item_id) => check(
            "get_item",
            ebay.call("get_item", &json!({ "item_id": item_id })).await,
            &|answer| live_answer_holds("get_item", answer) && answer["item_id"] == item_id,
        ),
        // A search that answered but named no item must not hide that get_item was never tried.
        None if searched_ok => {
            failed.push("search_items gave no item_id, so get_item was not called".to_string());
        }
        None => {}
    }
    assert!(
        failed.is_empty(),
        "Catervas's own servers did not answer as they should:\n{}",
        failed.join("\n")
    );
}
