//! Every shipped `stdio` and `http` kit connector lists exactly the tools its kit pins (ADR 0036).
//! A tool's tag is Farik's judgment about another company's tool, so a change at the service is
//! caught here, in the repository, and a pin update re-reviews every tag. It talks to the real
//! services with the founder's keys, only by hand, with `FARIK_LIVE_TESTS=1`; it never runs in
//! CI, where it says so and returns. Each key comes from `FARIK_KIT_<NAME>_<KEY>` (`<NAME>`
//! upper-cased, `-` as `_`), and a service the user signs in to takes a bearer from
//! `FARIK_KIT_<NAME>_BEARER`.
//!
//! The Product Manager's three signed-in services are the shipped ones since step 06; the
//! comparison is also proven by `pin_drift`'s tests and `fixture_mcp.rs`.

use std::collections::BTreeMap;

use farik_core::team::{CustomTransport, McpServerSource, custom_server};
use farik_roles::{KitConnector, SHIPPED_ROLES, load_kit, pin_drift};
use farik_runtime::claude::Secret;
use farik_runtime::connectors::list_tools;

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
            let listed = list_tools(&server, &keys, bearer.as_ref(), &folder)
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
fn names_the_variable_a_live_run_reads() {
    assert_eq!(
        variable("my-service", "API_KEY"),
        "FARIK_KIT_MY_SERVICE_API_KEY"
    );
    assert_eq!(variable("notion", "BEARER"), "FARIK_KIT_NOTION_BEARER");
}
