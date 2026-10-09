//! `farik connector ebay` as the built binary: the kit's `ebay` entry, listed through the same
//! path the daemon uses, answers with exactly the tools the kit tags (ADR 0038). Offline: the
//! server lists its tools without asking eBay anything, with its two keys or without either.
#![cfg(unix)]

use std::collections::BTreeMap;

use farik_core::contract::Role;
use farik_core::team::{CustomServer, custom_server};
use farik_roles::{KitConnector, load_kit, pin_drift};
use farik_runtime::claude::Secret;
use farik_runtime::connectors::list_tools;

/// The Procurement Specialist's `ebay`, as the daemon would hold it.
fn ebay_in_the_kit() -> CustomServer {
    let kit = load_kit(Role::ProcurementSpecialist).expect("the Procurement Specialist's kit");
    kit.connectors
        .iter()
        .find_map(|connector| match connector {
            KitConnector::Server { entry, .. } if connector.name() == "ebay" => {
                custom_server(entry)
            }
            _ => None,
        })
        .expect("the Procurement Specialist's kit has ebay")
}

/// The names `server` lists when `farik` is started with `keys`, from a folder of its own.
async fn listed_names(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
    folder: &str,
) -> Vec<String> {
    // A name no `which farik` would find: only the executable `list_tools` was handed can answer.
    let scratch = std::env::temp_dir().join(format!("{folder}-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch folder");
    let under_test = scratch.join("ebay-under-test");
    let _ = std::fs::remove_file(&under_test);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_farik"), &under_test).expect("a symlink");

    let listed = list_tools(server, keys, None, &scratch, &under_test)
        .await
        .expect("the server lists its tools");
    let _ = std::fs::remove_dir_all(&scratch);

    let names: Vec<String> = listed.iter().map(|tool| tool.name.clone()).collect();
    assert!(listed.iter().all(|tool| tool.usable), "{names:?}");
    names
}

fn assert_the_kit_tags_exactly(server: &CustomServer, names: &[String]) {
    let drift = pin_drift(&server.tools, names);
    assert!(
        drift.added.is_empty(),
        "the server lists more than the kit tags: {drift:?}"
    );
    assert!(
        drift.removed.is_empty(),
        "the kit tags more than the server lists: {drift:?}"
    );
    assert_eq!(names.len(), 2);
}

/// The two keys are named in the kit, and `list_tools` will not start a server without them, so
/// the test gives it two obviously fake ones.
#[tokio::test]
async fn ebay_server_lists_the_kits_tools() {
    let ebay = ebay_in_the_kit();
    let keys = BTreeMap::from([
        (
            "EBAY_CLIENT_ID".to_string(),
            Secret::new("test-app-id".to_string()),
        ),
        (
            "EBAY_CLIENT_SECRET".to_string(),
            Secret::new("test-cert-id".to_string()),
        ),
    ]);
    let names = listed_names(&ebay, &keys, "farik-ebay-under-test").await;
    assert_the_kit_tags_exactly(&ebay, &names);
}

/// A copy of the entry that names no key starts the binary with neither variable set, and it
/// still lists both tools: the rule for missing keys is that every call says so, not that the
/// server will not start.
#[tokio::test]
async fn ebay_server_lists_its_tools_without_its_keys() {
    let mut bare = ebay_in_the_kit();
    assert_eq!(bare.credential_keys.len(), 2);
    bare.credential_keys.clear();
    let names = listed_names(&bare, &BTreeMap::new(), "farik-ebay-bare-under-test").await;
    assert_the_kit_tags_exactly(&bare, &names);
}
