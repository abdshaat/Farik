//! `farik connector google-ads` as the built binary: the kit's Google Ads entry, listed through the
//! same path the daemon uses, answers with exactly the tools the kit tags (ADR 0038, ADR 0042).
//! Offline: the shim lists its ten tools itself and asks neither the daemon nor Google anything,
//! since no ticket or address is in its environment.
#![cfg(unix)]

use std::collections::BTreeMap;

use farik_core::contract::Role;
use farik_core::team::custom_server;
use farik_roles::{KitConnector, load_kit, pin_drift};
use farik_runtime::connectors::list_tools;

#[tokio::test]
async fn google_ads_server_lists_the_kits_tools() {
    let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
    let google_ads = kit
        .connectors
        .iter()
        .find_map(|connector| match connector {
            KitConnector::Server { entry, .. } if connector.name() == "google-ads" => {
                custom_server(entry)
            }
            _ => None,
        })
        .expect("the Marketing Specialist's kit has google-ads");
    // A name no `which farik` would find: only the executable `list_tools` was handed can answer.
    let scratch = std::env::temp_dir().join(format!(
        "farik-google-ads-under-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("a scratch folder");
    let under_test = scratch.join("google-ads-under-test");
    let _ = std::fs::remove_file(&under_test);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_farik"), &under_test).expect("a symlink");

    let listed = list_tools(&google_ads, &BTreeMap::new(), None, &scratch, &under_test)
        .await
        .expect("the server lists its tools");
    let _ = std::fs::remove_dir_all(&scratch);

    let names: Vec<String> = listed.iter().map(|tool| tool.name.clone()).collect();
    assert!(listed.iter().all(|tool| tool.usable), "{names:?}");
    let drift = pin_drift(&google_ads.tools, &names);
    assert!(
        drift.added.is_empty(),
        "the server lists more than the kit tags: {drift:?}"
    );
    assert!(
        drift.removed.is_empty(),
        "the kit tags more than the server lists: {drift:?}"
    );
    assert_eq!(names.len(), 10);
}
