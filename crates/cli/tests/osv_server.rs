//! `catervas connector osv` as the built binary: the kit's OSV entry, listed through the same path
//! the daemon uses, answers with exactly the tools the kit tags (ADR 0038). Offline: the server
//! lists its tools without asking OSV anything.
#![cfg(unix)]

use std::collections::BTreeMap;

use catervas_core::contract::Role;
use catervas_core::team::custom_server;
use catervas_roles::{KitConnector, load_kit, pin_drift};
use catervas_runtime::connectors::list_tools;

#[tokio::test]
async fn osv_server_lists_the_kits_tools() {
    let kit = load_kit(Role::Architect).expect("the Architect's kit");
    let osv = kit
        .connectors
        .iter()
        .find_map(|connector| match connector {
            KitConnector::Server { entry, .. } if connector.name() == "osv" => custom_server(entry),
            _ => None,
        })
        .expect("the Architect's kit has osv");
    // A name no `which catervas` would find: only the executable `list_tools` was handed can answer.
    let scratch =
        std::env::temp_dir().join(format!("catervas-osv-under-test-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch folder");
    let under_test = scratch.join("osv-under-test");
    let _ = std::fs::remove_file(&under_test);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_catervas"), &under_test).expect("a symlink");

    let listed = list_tools(&osv, &BTreeMap::new(), None, &scratch, &under_test)
        .await
        .expect("the server lists its tools");
    let _ = std::fs::remove_dir_all(&scratch);

    let names: Vec<String> = listed.iter().map(|tool| tool.name.clone()).collect();
    assert!(listed.iter().all(|tool| tool.usable), "{names:?}");
    let drift = pin_drift(&osv.tools, &names);
    assert!(
        drift.added.is_empty(),
        "the server lists more than the kit tags: {drift:?}"
    );
    assert!(
        drift.removed.is_empty(),
        "the kit tags more than the server lists: {drift:?}"
    );
    assert_eq!(names.len(), 3);
}
