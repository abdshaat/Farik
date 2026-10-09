//! Every shipped `stdio` and `http` kit connector lists exactly the tools its kit pins (ADR 0036).
//! A tool's tag is Farik's judgment about another company's tool, so a change at the service is
//! caught here, in the repository, and a pin update re-reviews every tag. It talks to the real
//! services with the founder's keys, only by hand, with `FARIK_LIVE_TESTS=1`; it never runs in
//! CI, where it says so and returns. Each key comes from `FARIK_KIT_<NAME>_<KEY>` (`<NAME>`
//! upper-cased, `-` as `_`), and a service the user signs in to takes a bearer from
//! `FARIK_KIT_<NAME>_BEARER`.
//!
//! The Product Manager's three signed-in services are the shipped ones since step 06; the
//! Architect's Context7 (signed in: `FARIK_KIT_CONTEXT7_BEARER`) and Grep (no key) join them, and
//! the Marketing Specialist's Higgsfield (`FARIK_KIT_HIGGSFIELD_BEARER`) and Recraft
//! (`FARIK_KIT_RECRAFT_BEARER`) since step 08, and its Buffer (`FARIK_KIT_BUFFER_BEARER`) and Kit
//! (`FARIK_KIT_KIT_BEARER`) since step 08b. Recraft's nine names are those of its own package,
//! whose remote server publishes none, so this run is where they are first checked. GitHub is in
//! both the Product Manager's and the Architect's kit since step 07c, with a key the user pastes:
//! one name, so one variable, `FARIK_KIT_GITHUB_GITHUB_KEY`, set to a fine-grained key, which
//! lists every tool whatever it may do (one that reaches public repositories only is enough);
//! each kit's headers narrow what the server lists, so the two lists are compared apart. From the
//! web launch GitHub signs in through Farik Cloud and the variable is `FARIK_KIT_GITHUB_BEARER`
//! (ADR 0044).
//!
//! The Finance Specialist's three services are signed in to, each with a bearer: Stripe's
//! (`FARIK_KIT_STRIPE_BEARER`) from an MCP Inspector sign-in to a Stripe sandbox granted every
//! permission, Digits' (`FARIK_KIT_DIGITS_BEARER`) from an MCP Inspector sign-in, and Kick's
//! (`FARIK_KIT_KICK_BEARER`) from a sign-in granted `mcp:read` and `mcp:write` or from a
//! user-scoped personal key. The run only lists tools, so a wide grant is what shows every tool
//! each service has; the kit itself asks Kick for `mcp:read` alone. Kick lists fewer tools to a
//! narrower grant, so the run also records what an `mcp:read` grant lists.
//!
//! The Procurement Specialist's four third-party services join them since step 10d, each with what
//! it needs: Exa takes no key; `SerpApi` takes `FARIK_KIT_SERPAPI_SERPAPI_KEY`, a free account's
//! private key; AWS Pricing takes `FARIK_KIT_AWS_PRICING_AWS_ACCESS_KEY_ID` and
//! `FARIK_KIT_AWS_PRICING_AWS_SECRET_ACCESS_KEY`, a key of a user allowed only the pricing reads,
//! and needs the program `uv`; and Brex, which the user signs in to, takes `FARIK_KIT_BREX_BEARER`,
//! which is an API token from Brex's Developer settings, made by an admin after the Developer API
//! agreement, since Brex's server accepts one as a bearer. Run it twice and note the first and
//! the second time `aws-pricing` takes to list: the first downloads Python and the package.
//!
//! A connector Farik runs itself (`command: farik`: the Architect's OSV, the Marketing
//! Specialist's Google Ads and the Procurement Specialist's `fx`) is skipped, with a line naming
//! its pin: the offline test `crates/cli/tests/{file}_server.rs`, `{file}` the connector's name
//! with each `-` made `_`, since its tools are Farik's own and change only with a Farik release.
//! The comparison is also proven by `pin_drift`'s tests and `fixture_mcp.rs`.

use std::collections::BTreeMap;

use farik_core::team::{CustomTransport, McpServerSource, custom_server};
use farik_roles::{KitConnector, SHIPPED_ROLES, is_farik_connector, load_kit, pin_drift};
use farik_runtime::claude::Secret;
use farik_runtime::connectors::list_tools;

/// The integration test that pins one of Farik's own connectors offline: `crates/cli/tests/` and
/// the connector's name with each `-` made `_`, then `_server.rs`.
fn offline_pin(connector: &str) -> String {
    format!("crates/cli/tests/{}_server.rs", connector.replace('-', "_"))
}

/// The variable holding `connector`'s `key` for the live run.
fn variable(connector: &str, key: &str) -> String {
    format!(
        "FARIK_KIT_{}_{key}",
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
    if std::env::var("FARIK_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set FARIK_LIVE_TESTS=1 to list every kit connector's live tools");
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
                && is_farik_connector(command, args)
            {
                eprintln!(
                    "skipped {role}'s {name}: Farik's own server, pinned offline by {}",
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
                std::path::Path::new("farik"),
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
fn names_the_offline_pin_of_each_farik_connector() {
    assert_eq!(offline_pin("fx"), "crates/cli/tests/fx_server.rs");
    assert_eq!(
        offline_pin("google-ads"),
        "crates/cli/tests/google_ads_server.rs"
    );
    // Every connector of Farik's own in a shipped kit has the test its skip line names.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found: Vec<String> = Vec::new();
    for role in SHIPPED_ROLES {
        for connector in &load_kit(role).expect("a shipped kit loads").connectors {
            let KitConnector::Server { entry, .. } = connector else {
                continue;
            };
            let server = custom_server(entry).expect("a kit entry");
            if let CustomTransport::Stdio { command, args, .. } = &server.transport
                && is_farik_connector(command, args)
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
    assert_eq!(found, ["fx", "google-ads", "osv", "recalls"]);
}

#[test]
fn names_the_variable_a_live_run_reads() {
    assert_eq!(
        variable("my-service", "API_KEY"),
        "FARIK_KIT_MY_SERVICE_API_KEY"
    );
    assert_eq!(variable("notion", "BEARER"), "FARIK_KIT_NOTION_BEARER");
}
