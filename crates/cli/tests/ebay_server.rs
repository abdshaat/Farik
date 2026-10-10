//! `catervas connector ebay` as the built binary: the kit's `ebay` entry, listed through the same
//! path the daemon uses, answers with exactly the tools the kit tags (ADR 0038). Offline: the
//! server lists its tools without asking eBay anything, with its two keys or without either.
#![cfg(unix)]

use std::collections::BTreeMap;

use catervas_core::contract::Role;
use catervas_core::team::{CustomServer, custom_server};
use catervas_roles::{KitConnector, load_kit, pin_drift};
use catervas_runtime::claude::Secret;
use catervas_runtime::connectors::{ConnectorError, call_tool, list_tools};

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

/// A scratch folder of its own, and in it a name no `which catervas` would find, linked to the built
/// binary: only the executable a call was handed can answer.
fn scratch_with_the_binary(folder: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let scratch = std::env::temp_dir().join(format!("{folder}-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch folder");
    let under_test = scratch.join("ebay-under-test");
    let _ = std::fs::remove_file(&under_test);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_catervas"), &under_test).expect("a symlink");
    (scratch, under_test)
}

/// The names `server` lists when `catervas` is started with `keys`, from a folder of its own.
async fn listed_names(
    server: &CustomServer,
    keys: &BTreeMap<String, Secret>,
    folder: &str,
) -> Vec<String> {
    let (scratch, under_test) = scratch_with_the_binary(folder);

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
    let names = listed_names(&ebay, &keys, "catervas-ebay-under-test").await;
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
    let names = listed_names(&bare, &BTreeMap::new(), "catervas-ebay-bare-under-test").await;
    assert_the_kit_tags_exactly(&bare, &names);
}

/// A call through the built binary: `get_item` with an id that is not one, which the server
/// refuses for the id before it sends anything to eBay, once it has both keys. With the keys in
/// the launcher's environment the refusal is for the id; with none it is for the missing keys.
/// So the binary reads `EBAY_CLIENT_ID` and `EBAY_CLIENT_SECRET` from its environment, and the
/// test sends nothing to eBay.
#[tokio::test]
async fn ebay_server_reads_its_keys_from_its_environment() {
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
    let (scratch, under_test) = scratch_with_the_binary("catervas-ebay-keys-under-test");
    let arguments = || {
        serde_json::json!({ "item_id": "123" })
            .as_object()
            .cloned()
            .expect("an object")
    };

    let with_keys = call_tool(
        &ebay,
        &keys,
        None,
        &scratch,
        &under_test,
        "get_item",
        arguments(),
    )
    .await;

    let mut bare = ebay_in_the_kit();
    bare.credential_keys.clear();
    let without_keys = call_tool(
        &bare,
        &BTreeMap::new(),
        None,
        &scratch,
        &under_test,
        "get_item",
        arguments(),
    )
    .await;
    let _ = std::fs::remove_dir_all(&scratch);

    assert_eq!(
        with_keys,
        Err(ConnectorError::ToolError {
            text: "item_id is an id from a search, such as v1|123456789|0".to_string()
        })
    );
    assert_eq!(
        without_keys,
        Err(ConnectorError::ToolError {
            text: "eBay is not set up; connect it again with your App ID and Cert ID".to_string()
        })
    );
}
