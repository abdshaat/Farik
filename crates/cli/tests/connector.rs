//! `catervas connect` and `catervas disconnect` (`docs/SPEC.md` 5.6, ADR 0030): the command lists the
//! server's tools and keeps its keys in its own process, and only names reach the team file, the
//! log, or the process driving the project.
//!
//! Every test here needs the `git` program, and is `#[ignore]`d and run by
//! `cargo xtask check --integration`.
#![cfg(unix)]

#[path = "shared/project.rs"]
mod project;

use std::path::PathBuf;
use std::sync::Arc;

use catervas_protocol::command::{Command, command_to_value};
use catervas_protocol::event::{EventBody, EventKind};
use catervas_runtime::connectors::{
    ConnectorEntry, ConnectorSecretStores, ConnectorSecrets, MemoryConnectorSecrets, SecretAt,
    SecretStore,
};
use catervas_runtime::credential::CredentialError;
use catervas_store::git::fixtures::TempRepo;
use serde_json::{Value, json};

use project::{LiveDriver, a_team, a_team_with, events, files_of, log_of, run_with, scratch};

/// An authorization server and a protected MCP server, as the runtime's tests run them.
#[path = "../../runtime/tests/support/oauth_fixture.rs"]
mod oauth_fixture;
#[path = "../../runtime/tests/support/ports.rs"]
mod ports;

const KEY: &str = "a-key-typed-at-the-terminal";

/// The stdio MCP server of the runtime's tests, written for `test`: its tools are `search`, `env`,
/// `delete_repo`, and `repo.delete`, a name Catervas can't use.
fn fixture(test: &str) -> PathBuf {
    let path = scratch(test).join("server.sh");
    std::fs::write(
        &path,
        include_str!("../../runtime/tests/fixtures/mcp_server.sh"),
    )
    .expect("the script is written");
    path
}

/// `XDG_CONFIG_HOME` for `repository`'s tests: beside it and outside it, as `~/.config` is.
fn config_of(repository: &TempRepo) -> PathBuf {
    PathBuf::from(format!("{}-config", repository.path.display()))
}

/// The user's state folder for `repository`'s tests: `catervas` in [`config_of`].
fn state_of(repository: &TempRepo) -> PathBuf {
    config_of(repository).join("catervas")
}

/// Where `agent`'s keys for `server` are kept in `repository`'s project: where the daemon looks
/// for them, under the project's id on this machine, never the log's team or project id.
fn kept_at(repository: &TempRepo, agent: &str, server: &str) -> SecretAt {
    let at =
        SecretAt::of(&state_of(repository), &repository.path, agent, server).expect("an address");
    let first = log_of(repository)
        .read(&catervas_store::EventQuery {
            limit: Some(1),
            ..catervas_store::EventQuery::default()
        })
        .expect("the log reads");
    // So that a command keeping keys under either of the log's ids is caught (carry M15).
    for logged in [
        &first[0].envelope.ids.team_id,
        &first[0].envelope.ids.project_id,
    ] {
        assert_ne!(&at.project_id, logged);
    }
    at
}

/// `catervas connect <agent> fixture` against `script`, with `extra` arguments, `stdin` on standard
/// input, and its keys kept in `store`.
fn connect(
    repository: &TempRepo,
    agent: &str,
    script: &std::path::Path,
    extra: &[&str],
    stdin: &str,
    store: Arc<dyn ConnectorSecrets>,
) -> project::Ran {
    let script = script.display().to_string();
    let mut args = vec![
        "connect",
        agent,
        "fixture",
        "--command",
        "sh",
        "--arg",
        &script,
    ];
    args.extend_from_slice(extra);
    let stdin = stdin.to_string();
    let state = config_of(repository);
    run_with(&repository.path, &args, move |io| {
        io.stdin = Box::new(std::io::Cursor::new(stdin.into_bytes()));
        io.connector_secrets = store;
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
    })
}

/// `agent`'s entry named `fixture` in the team file, if it has one.
fn entry(repository: &TempRepo, agent: &str) -> Option<Value> {
    let team = serde_json::to_value(files_of(repository).read_team().expect("the team reads"))
        .expect("the team is JSON");
    team["agents"]
        .as_array()
        .expect("agents")
        .iter()
        .find(|held| held["id"] == agent)
        .and_then(|held| held["mcp_servers"].as_array().cloned())
        .into_iter()
        .flatten()
        .find(|server| server["name"] == "fixture")
}

fn loaded(store: &dyn ConnectorSecrets, at: &SecretAt) -> Option<ConnectorEntry> {
    store.load(at).expect("the store reads")
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_reads_keys_from_stdin() {
    let repository = a_team("connect-stdin");
    let script = fixture("connect-stdin");
    let store = Arc::new(MemoryConnectorSecrets::default());

    let ran = connect(
        &repository,
        "dev-a",
        &script,
        &["--key", "API_KEY"],
        &format!("{KEY}\n"),
        Arc::clone(&store) as _,
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).expect("kept");
    assert_eq!(kept.keys["API_KEY"].expose(), KEY);
    assert!(!ran.out.contains(KEY) && !ran.err.contains(KEY));
    assert_eq!(
        entry(&repository, "dev-a").expect("written")["credential_keys"],
        json!(["API_KEY"])
    );

    let ran = connect(
        &repository,
        "dev-b",
        &script,
        &["--key", "API_KEY=a-value-typed-in-the-open"],
        "",
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.err.contains("standard input"), "{}", ran.err);
    assert!(
        !ran.err.contains("a-value-typed-in-the-open"),
        "{}",
        ran.err
    );
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-b", "fixture")).is_none());
    assert!(entry(&repository, "dev-b").is_none());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_starts_nothing_when_catervas_settings_are_inside_the_project() {
    // Re-review 2 m1: `XDG_CONFIG_HOME` inside the project puts the connector's folder back in
    // the repository, where a pulled `.npmrc` chooses what runs.
    let repository = a_team("connect-state-inside");
    let script = fixture("connect-state-inside");
    let ran_marker = repository.path.join("ran");
    std::fs::write(&script, format!("touch {}\n", ran_marker.display()))
        .expect("the script is written");
    let inside = repository.path.join(".cfg");
    let script = script.display().to_string();
    let ran = run_with(
        &repository.path,
        &[
            "connect",
            "dev-a",
            "fixture",
            "--command",
            "sh",
            "--arg",
            &script,
        ],
        move |io| {
            io.stdin = Box::new(std::io::Cursor::new(Vec::new()));
            io.connector_secrets = Arc::new(MemoryConnectorSecrets::default());
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), inside.display().to_string());
        },
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err
            .contains("state_inside_project: Catervas's settings folder"),
        "{}",
        ran.err
    );
    assert!(!ran_marker.exists(), "the server was started");
    assert!(entry(&repository, "dev-a").is_none());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_labels_with_tag_flags_and_defaults_to_external_effect() {
    let repository = a_team("connect-tags");
    let script = fixture("connect-tags");

    let ran = connect(
        &repository,
        "dev-a",
        &script,
        &["--tag", "search=network"],
        "",
        Arc::new(MemoryConnectorSecrets::default()),
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        entry(&repository, "dev-a").expect("written")["tools"],
        json!({ "search": "network", "env": "external_effect", "delete_repo": "external_effect" })
    );
    let lines: Vec<&str> = ran.out.lines().collect();
    for said in [
        "search: network",
        "env: external_effect",
        "delete_repo: external_effect",
        "repo.delete: Catervas can't use this tool",
    ] {
        assert!(lines.contains(&said), "{said} in {}", ran.out);
    }
    let connected = events(&repository, &[EventKind::ConnectorConnected]);
    assert_eq!(connected.len(), 1);

    // A misspelled tool is refused, naming the tools there are, rather than ignored.
    let store = Arc::new(MemoryConnectorSecrets::default());
    let ran = connect(
        &repository,
        "dev-b",
        &script,
        &["--tag", "delete_rep=denied"],
        "",
        store.clone(),
    );
    assert_ne!(ran.code, 0);
    assert!(
        ran.err.contains("tag_unknown_tool: delete_rep"),
        "{}",
        ran.err
    );
    assert!(ran.err.contains("search, env, delete_repo"), "{}", ran.err);
    assert_eq!(
        store.load(&kept_at(&repository, "dev-b", "fixture")),
        Ok(None)
    );
    assert_eq!(entry(&repository, "dev-b"), None);
}

/// A keychain that is not there.
struct NoKeychain;

impl ConnectorSecrets for NoKeychain {
    fn load(&self, _: &SecretAt) -> Result<Option<ConnectorEntry>, CredentialError> {
        Err(CredentialError::NoKeychain)
    }
    fn save(&self, _: &SecretAt, _: &ConnectorEntry) -> Result<SecretStore, CredentialError> {
        Err(CredentialError::NoKeychain)
    }
    fn delete(&self, _: &SecretAt) -> Result<(), CredentialError> {
        Err(CredentialError::NoKeychain)
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_says_which_store_kept_the_keys() {
    let repository = a_team("connect-store");
    let script = fixture("connect-store");
    let file = scratch("connect-store-state").join("connectors.json");
    let store: Arc<dyn ConnectorSecrets> = Arc::new(ConnectorSecretStores::new(
        Arc::new(NoKeychain),
        Some(file.clone()),
    ));

    let ran = connect(
        &repository,
        "dev-a",
        &script,
        &["--key", "API_KEY"],
        &format!("{KEY}\n"),
        Arc::clone(&store),
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.lines().last(),
        Some("Kept in a private file only you can read"),
        "{}",
        ran.out
    );
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_some());
    assert!(file.exists());

    let ran = connect(
        &repository,
        "dev-b",
        &script,
        &[],
        "",
        Arc::new(MemoryConnectorSecrets::default()),
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.out.lines().last(),
        Some("Kept in your computer's keychain"),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_sends_names_when_something_drives() {
    let repository = a_team("connect-sends");
    let script = fixture("connect-sends");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let driver = LiveDriver::new(&repository);

    let ran = connect(
        &repository,
        "dev-a",
        &script,
        &["--key", "API_KEY", "--tag", "search=network"],
        &format!("{KEY}\n"),
        Arc::clone(&store) as _,
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("handled by the run"), "{}", ran.out);
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).expect("kept");
    let commands = driver.commands();
    assert_eq!(commands.len(), 1, "{commands:?}");
    let Command::ConnectorConnect {
        agent,
        server,
        spec_sha256,
        ..
    } = &commands[0]
    else {
        panic!("connector_connect, not {:?}", commands[0]);
    };
    assert_eq!(agent, "dev-a");
    assert_eq!(server["name"], "fixture");
    assert_eq!(server["credential_keys"], json!(["API_KEY"]));
    assert_eq!(server["tools"]["search"], "network");
    assert_eq!(spec_sha256, &kept.spec_sha256);
    assert!(!command_to_value(&commands[0]).to_string().contains(KEY));
    assert!(
        entry(&repository, "dev-a").is_none(),
        "the driver writes it"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_disconnect_deletes_the_keys_and_the_entry() {
    let repository = a_team("disconnect");
    let script = fixture("disconnect");
    let store = Arc::new(MemoryConnectorSecrets::default());
    for agent in ["dev-a", "dev-b"] {
        let ran = connect(
            &repository,
            agent,
            &script,
            &["--key", "API_KEY"],
            &format!("{KEY}\n"),
            Arc::clone(&store) as _,
        );
        assert_eq!(ran.code, 0, "{}", ran.err);
    }

    let held = Arc::clone(&store);
    let config = config_of(&repository);
    let ran = run_with(
        &repository.path,
        &["disconnect", "dev-a", "fixture"],
        move |io| {
            io.connector_secrets = held;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        },
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_none());
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-b", "fixture")).is_some());
    assert!(entry(&repository, "dev-a").is_none());
    assert!(entry(&repository, "dev-b").is_some());
    let disconnected = events(&repository, &[EventKind::ConnectorDisconnected]);
    assert!(
        matches!(
            disconnected.as_slice(),
            [event] if matches!(&event.body, EventBody::ConnectorDisconnected(body) if body.agent.as_str() == "dev-a")
        ),
        "{disconnected:?}"
    );

    let ran = run_with(
        &repository.path,
        &["disconnect", "dev-a", "fixture"],
        |_| {},
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
}

/// `dev-a` given a `google-ads` entry and its keys, and a campaign Catervas made, in `repository`.
fn google_ads_with_a_campaign(
    repository: &TempRepo,
    script: &std::path::Path,
) -> Arc<MemoryConnectorSecrets> {
    let mut wire = serde_json::to_value(files_of(repository).read_team().expect("the team reads"))
        .expect("the team is JSON");
    wire["agents"][1]["mcp_servers"] = json!([{
        "name": "google-ads", "source": "custom", "transport": "stdio", "command": "sh",
        "args": [script.display().to_string()], "tools": { "search": "network" }
    }]);
    files_of(repository)
        .write_team(&catervas_core::team::validate_team(&wire).expect("a team"))
        .expect("the team is written");
    let store = Arc::new(MemoryConnectorSecrets::default());
    store
        .save(
            &kept_at(repository, "dev-a", "google-ads"),
            &ConnectorEntry {
                spec_sha256: "0".repeat(64),
                keys: std::collections::BTreeMap::new(),
                oauth: None,
            },
        )
        .expect("kept");
    project::record(
        repository,
        "",
        "marketing_campaign.created",
        &json!({
            "plan": "MP-1", "key": "search-launch", "account": "123-456-7890",
            "campaign": "customers/1234567890/campaigns/11",
            "budget": "customers/1234567890/campaignBudgets/12",
            "budget_kind": "total", "amount": "500.00"
        }),
    );
    store
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_disconnect_sends_google_ads_to_the_browser() {
    let repository = a_team("disconnect-google-ads");
    let script = fixture("disconnect-google-ads");
    let store = google_ads_with_a_campaign(&repository, &script);
    let disconnect = |store: &Arc<MemoryConnectorSecrets>| {
        let held = Arc::clone(store);
        let config = config_of(&repository);
        run_with(
            &repository.path,
            &["disconnect", "dev-a", "google-ads"],
            move |io| {
                io.connector_secrets = held;
                io.env
                    .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
            },
        )
    };

    // A campaign Catervas made is not recorded paused, and with no process driving the project there
    // is no daemon to pause it with: the browser, where Catervas pauses first, is the way.
    let ran = disconnect(&store);
    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);
    assert!(
        ran.err.contains("disconnect_in_the_browser: remove Google Ads on dev-a's page in the browser, where Catervas pauses its running ads first"),
        "{}",
        ran.err
    );
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "google-ads")).is_some());
    assert!(events(&repository, &[EventKind::ConnectorDisconnected]).is_empty());

    // Paused at its budget is no pause for its end: a raise may have started it again.
    project::record(
        &repository,
        "",
        "marketing_campaign.paused",
        &json!({
            "plan": "MP-1", "key": "search-launch",
            "campaign": "customers/1234567890/campaigns/11", "why": "budget_reached"
        }),
    );
    let ran = disconnect(&store);
    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);

    // Recorded paused for its plan's end, nothing runs, and the command disconnects as it does.
    project::record(
        &repository,
        "",
        "marketing_campaign.paused",
        &json!({
            "plan": "MP-1", "key": "search-launch",
            "campaign": "customers/1234567890/campaigns/11", "why": "plan_ended"
        }),
    );
    let ran = disconnect(&store);
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "google-ads")).is_none());
    assert_eq!(
        events(&repository, &[EventKind::ConnectorDisconnected]).len(),
        1
    );
}

/// `catervas disconnect dev-a google-ads` in `repository`, with `store` holding its keys.
fn disconnect_google_ads(
    repository: &TempRepo,
    store: &Arc<MemoryConnectorSecrets>,
) -> project::Ran {
    let held = Arc::clone(store);
    let config = config_of(repository);
    run_with(
        &repository.path,
        &["disconnect", "dev-a", "google-ads"],
        move |io| {
            io.connector_secrets = held;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        },
    )
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_disconnect_counts_a_pause_made_for_an_earlier_removal() {
    let removed = |repository: &TempRepo| {
        project::record(
            repository,
            "",
            "marketing_campaign.paused",
            &json!({
                "plan": "MP-1", "key": "search-launch",
                "campaign": "customers/1234567890/campaigns/11", "why": "connection_removed"
            }),
        );
    };

    // Google Ads was removed from another agent before, and Catervas paused the campaign for it. But
    // dev-a still has Google Ads, and its sign-in could have enabled the campaign since, with no
    // connection recorded: the browser is the way, where Catervas pauses first.
    let repository = a_team("disconnect-google-ads-removed");
    let store = google_ads_with_a_campaign(&repository, &fixture("disconnect-google-ads-removed"));
    removed(&repository);
    let ran = disconnect_google_ads(&repository, &store);
    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);
    assert!(ran.err.contains("disconnect_in_the_browser"), "{}", ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "google-ads")).is_some());

    // dev-a was retired instead, which paused the campaign first and deleted its keys: no agent
    // that is not retired has Google Ads, nothing runs, and the command cleans up the entry the
    // retirement left.
    let retired = |wire: &mut Value| wire["agents"][1]["status"] = json!("retired");
    let repository = a_team_with("disconnect-google-ads-retired", retired);
    let store = google_ads_with_a_campaign(&repository, &fixture("disconnect-google-ads-retired"));
    removed(&repository);
    let ran = disconnect_google_ads(&repository, &store);
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "google-ads")).is_none());

    // Google Ads connected again after that pause: an agent could have enabled the campaign since,
    // so it counts no more, and the browser is the way.
    let repository = a_team_with("disconnect-google-ads-reconnected", retired);
    let store =
        google_ads_with_a_campaign(&repository, &fixture("disconnect-google-ads-reconnected"));
    removed(&repository);
    let mut connected =
        catervas_protocol::event::fixtures::a_body_wire(EventKind::ConnectorConnected);
    connected["agent"] = json!("dev-a");
    connected["server"] = json!("google-ads");
    project::record(&repository, "", "connector.connected", &connected);
    let ran = disconnect_google_ads(&repository, &store);
    assert_eq!(ran.code, 1, "{}\n{}", ran.out, ran.err);
    assert!(ran.err.contains("disconnect_in_the_browser"), "{}", ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "google-ads")).is_some());
}

/// An opener that follows the address as a browser would, on `runtime`.
fn following(runtime: &tokio::runtime::Runtime) -> catervas::Opener {
    let handle = runtime.handle().clone();
    Arc::new(move |url: &str| {
        let url = url.to_string();
        handle.spawn(async move {
            oauth_fixture::follow(&url).await;
        });
        Ok(())
    })
}

/// `catervas connect dev-a fixture --url <fixture> --sign-in <extra>`, opened with `opener`.
fn sign_in(
    repository: &TempRepo,
    fixture: &oauth_fixture::Fixture,
    extra: &[&str],
    opener: catervas::Opener,
    store: Arc<dyn ConnectorSecrets>,
) -> project::Ran {
    sign_in_after(&[], repository, fixture, extra, opener, store)
}

/// As [`sign_in`], with `before` ahead of the command, as a global flag goes.
fn sign_in_after(
    before: &[&str],
    repository: &TempRepo,
    fixture: &oauth_fixture::Fixture,
    extra: &[&str],
    opener: catervas::Opener,
    store: Arc<dyn ConnectorSecrets>,
) -> project::Ran {
    let mut args = before.to_vec();
    args.extend_from_slice(&[
        "connect",
        "dev-a",
        "fixture",
        "--url",
        fixture.mcp_url.as_str(),
        "--sign-in",
    ]);
    args.extend_from_slice(extra);
    let config = config_of(repository);
    run_with(&repository.path, &args, move |io| {
        io.connector_secrets = store;
        io.open_url = opener;
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
    })
}

/// A table of one device app, `Dev`, whose endpoints are `fixture`'s, for the servers at its
/// address. Leaked: a table is `'static`, and a test's leak is small.
fn dev_table(
    fixture: &oauth_fixture::Fixture,
) -> &'static [catervas_runtime::registered_apps::RegisteredApp] {
    use catervas_runtime::registered_apps::{AppFlow, RegisteredApp};
    fn leaked(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }
    let origin = &fixture.origin;
    Box::leak(Box::new([RegisteredApp {
        id: "dev",
        name: "Dev",
        host: Some("127.0.0.1"),
        catervas_connector: None,
        flow: AppFlow::Device {
            device_endpoint: leaked(format!("{origin}/device/code")),
            verification_uri: leaked(format!("{origin}/login/device")),
        },
        client_id: "dev-client",
        client_secret: None,
        scopes: &[],
        issuer: leaked(format!("{origin}/login/oauth")),
        token_endpoint: leaked(format!("{origin}/token")),
        revocation_endpoint: None,
        install_url: Some("https://github.com/apps/dev/installations/new"),
        settings_url: "https://github.com/settings/apps/authorizations",
    }]))
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_prints_the_device_code() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-device");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let _driver = LiveDriver::new(&repository);
    let pages = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let opener: catervas::Opener = {
        let pages = Arc::clone(&pages);
        Arc::new(move |url: &str| {
            pages.lock().expect("pages").push(url.to_string());
            Ok(())
        })
    };
    let table = dev_table(&fixture);
    let config = config_of(&repository);
    let kept_in = Arc::clone(&store);
    let ran = run_with(
        &repository.path,
        &[
            "connect",
            "dev-a",
            "fixture",
            "--url",
            fixture.mcp_url.as_str(),
            "--sign-in",
            "--tag",
            "whoami=network",
        ],
        move |io| {
            io.connector_secrets = kept_in;
            io.open_url = opener;
            io.registered_apps = table;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        },
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    // The prompts go to the error stream: the page and the code, the warning, then who signed in.
    let lines: Vec<&str> = ran.err.lines().collect();
    assert_eq!(
        lines,
        [
            format!(
                "Open {}/login/device and enter the code WDJB-0001.",
                fixture.origin
            )
            .as_str(),
            "Only enter a code that this page shows you. Catervas never sends you a code in a chat.",
            "Signed in to Dev.",
        ],
        "{}",
        ran.err
    );
    assert_eq!(
        *pages.lock().expect("pages"),
        [format!("{}/login/device", fixture.origin)]
    );
    assert!(!ran.out.contains("WDJB"), "{}", ran.out);
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).expect("kept");
    let grant = kept.oauth.expect("a grant is kept");
    assert_eq!(grant.app.as_deref(), Some("dev"));
    assert!(
        ran.out.lines().any(|line| line == "whoami: network"),
        "{}",
        ran.out
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_names_the_provider_when_a_device_sign_in_is_refused() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    fixture.set(|flags| flags.device_error = Some("access_denied".to_string()));
    let repository = a_team("connect-device-denied");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let table = dev_table(&fixture);
    let config = config_of(&repository);
    let kept_in = Arc::clone(&store);
    let ran = run_with(
        &repository.path,
        &[
            "connect",
            "dev-a",
            "fixture",
            "--url",
            fixture.mcp_url.as_str(),
            "--sign-in",
        ],
        move |io| {
            io.connector_secrets = kept_in;
            io.open_url = Arc::new(|_: &str| Ok(()));
            io.registered_apps = table;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        },
    );

    assert_eq!(ran.code, 1, "{}", ran.out);
    // The person said no on Dev's page, not on a page at the address's host, 127.0.0.1.
    assert!(
        ran.err
            .contains("you said no on Dev's page, so nothing was connected"),
        "{}",
        ran.err
    );
    assert!(!ran.err.contains("127.0.0.1's page"), "{}", ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_none());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_signs_in_and_keeps_the_grant() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-sign-in");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let driver = LiveDriver::new(&repository);

    let ran = sign_in(
        &repository,
        &fixture,
        &["--tag", "whoami=network"],
        following(&runtime),
        Arc::clone(&store) as _,
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    // The prompts go to the error stream, so the result alone is on the output.
    let lines: Vec<&str> = ran.err.lines().collect();
    assert!(
        lines[0].starts_with("Sign in to 127.0.0.1 in your browser: http://127.0.0.1:"),
        "{}",
        ran.err
    );
    assert_eq!(lines[1], format!("Signed in to {}.", fixture.origin));
    assert!(
        ran.out.lines().any(|line| line == "whoami: network"),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains("Sign in to"), "{}", ran.out);
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).expect("kept");
    let grant = kept.oauth.expect("a grant is kept");
    assert_eq!(grant.issuer, fixture.origin);
    // The command a running daemon is sent holds names only, and says who signed in.
    let commands = driver.commands();
    assert_eq!(commands.len(), 1, "{commands:?}");
    let sent = command_to_value(&commands[0]).to_string();
    for token in [
        grant.access_token.expose(),
        grant
            .refresh_token
            .as_ref()
            .expect("a refresh token")
            .expose(),
    ] {
        assert!(!sent.contains(token), "{sent}");
    }
    let Command::ConnectorConnect { server, issuer, .. } = &commands[0] else {
        panic!("connector_connect, not {:?}", commands[0]);
    };
    assert_eq!(server["oauth"], json!({}));
    assert_eq!(issuer.as_deref(), Some(fixture.origin.as_str()));

    // An opener that does nothing, as a failed `xdg-open` does: the address was printed, and the
    // user can follow it by hand.
    let repository = a_team("connect-sign-in-by-hand");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let printed = Arc::new(std::sync::Mutex::new(String::new()));
    let (seen, handle) = (Arc::clone(&printed), runtime.handle().clone());
    let by_hand: catervas::Opener = Arc::new(move |url: &str| {
        *seen.lock().expect("a lock") = url.to_string();
        let url = url.to_string();
        // Followed later, not by the opener, after the address was printed.
        handle.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            oauth_fixture::follow(&url).await;
        });
        Err("xdg-open is not installed".to_string())
    });
    let ran = sign_in(&repository, &fixture, &[], by_hand, Arc::clone(&store) as _);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let address = printed.lock().expect("a lock").clone();
    assert!(!address.is_empty());
    assert!(ran.err.contains(&address), "{}", ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_some());

    // With --json the output is the JSON alone, and the address is still given, on stderr.
    let repository = a_team("connect-sign-in-json");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let ran = sign_in_after(
        &["--json"],
        &repository,
        &fixture,
        &["--tag", "whoami=network"],
        following(&runtime),
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let parsed: Value =
        serde_json::from_str(&ran.out).unwrap_or_else(|error| panic!("{error}: {}", ran.out));
    assert_eq!(parsed["tools"]["whoami"], "network");
    assert!(
        ran.err.contains("in your browser: http://127.0.0.1:"),
        "{}",
        ran.err
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_again_revokes_the_replaced_grant() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-sign-in-again");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let again = || {
        sign_in(
            &repository,
            &fixture,
            &[],
            following(&runtime),
            Arc::clone(&store) as _,
        )
    };
    assert_eq!(again().code, 0);
    let old = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture"))
        .and_then(|entry| entry.oauth)
        .and_then(|grant| grant.refresh_token)
        .expect("a refresh token");
    assert_eq!(fixture.count("/revoke"), 0);
    assert_eq!(again().code, 0);
    let revoked = fixture.requests("/revoke");
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0].form["token"], old.expose());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_again_with_the_same_client_revokes_nothing() {
    // Some services end every grant of a client when one is revoked.
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-sign-in-same-client");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let port = ports::free_port().to_string();
    for _ in 0..2 {
        let ran = sign_in(
            &repository,
            &fixture,
            &["--client-id", "fixed", "--callback-port", port.as_str()],
            following(&runtime),
            Arc::clone(&store) as _,
        );
        assert_eq!(ran.code, 0, "{}", ran.err);
    }
    assert_eq!(fixture.count("/revoke"), 0);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_sign_in_takes_no_key() {
    let repository = a_team("connect-sign-in-no-key");
    for extra in [["--key", "A"], ["--command", "x"]] {
        let ran = run_with(
            &repository.path,
            &[
                "connect",
                "dev-a",
                "fixture",
                "--url",
                "https://mcp.example.com/mcp",
                "--sign-in",
                extra[0],
                extra[1],
            ],
            |_| {},
        );
        assert_ne!(ran.code, 0, "{extra:?}");
        assert!(
            ran.err.contains("cannot be used with"),
            "{extra:?}: {}",
            ran.err
        );
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_says_when_a_service_offers_no_sign_in() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    fixture.set(|flags| {
        flags.challenge = false;
        flags.prm = false;
        flags.as_metadata = false;
    });
    let repository = a_team("connect-sign-in-not-offered");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let ran = sign_in(
        &repository,
        &fixture,
        &[],
        following(&runtime),
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err
            .contains("127.0.0.1 does not offer signing in; give its key with --key"),
        "{}",
        ran.err
    );
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_none());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_disconnect_asks_the_service_to_forget_the_sign_in() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("disconnect-sign-in");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let ran = sign_in(
        &repository,
        &fixture,
        &[],
        following(&runtime),
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let refresh = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture"))
        .and_then(|entry| entry.oauth)
        .and_then(|grant| grant.refresh_token)
        .expect("a refresh token");

    let held = Arc::clone(&store);
    let config = config_of(&repository);
    let ran = run_with(
        &repository.path,
        &["disconnect", "dev-a", "fixture"],
        move |io| {
            io.connector_secrets = held;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        },
    );

    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_none());
    let revoked = fixture.requests("/revoke");
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0].form["token"], refresh.expose());
}

/// The Developer's kit with one service, `fixture`: the stdio fixture server at `script` with one
/// key, or, given `oauth_url`, the sign-in fixture's web address. Its tags: `search` network,
/// `env` external effect, `delete_repo` denied (and `whoami` network for the sign-in one).
fn a_kit(script: &std::path::Path, oauth_url: Option<&str>) -> catervas_runtime::KitSource {
    a_kit_of(script, oauth_url, false)
}

/// [`a_kit`], and, when `allowing`, with a spending tool `make` that may run 20 times a sprint
/// unasked and a `post` that always asks.
fn a_kit_of(
    script: &std::path::Path,
    oauth_url: Option<&str>,
    allowing: bool,
) -> catervas_runtime::KitSource {
    let mut connector = json!({
        "name": "fixture", "title": "Fixture", "about": "A stand-in service.",
        "why": "Lets the Developer search it.", "setup": "Make a key on its page and paste it.",
    });
    if let Some(url) = oauth_url {
        connector["transport"] = json!("http");
        connector["url"] = json!(url);
        connector["oauth"] = json!({});
        connector["tools"] = json!({ "whoami": "network" });
    } else {
        connector["transport"] = json!("stdio");
        connector["command"] = json!("sh");
        connector["args"] = json!([script.display().to_string()]);
        connector["credential_keys"] = json!(["API_KEY"]);
        connector["key_page"] = json!("https://fixture.example/keys");
        connector["labels"] = json!({ "search": "search the fixture" });
        connector["tools"] =
            json!({ "search": "network", "env": "external_effect", "delete_repo": "denied" });
        if allowing {
            connector["tools"]["make"] = json!("external_effect");
            connector["tools"]["post"] = json!("external_effect");
            connector["allowances"] = json!({ "make": { "calls": 20, "what": "pictures" } });
        }
    }
    let file = json!({ "role": "software_developer", "skills": [], "connectors": [connector] });
    let kit = catervas_roles::parse_fixture_kit(
        catervas_core::contract::Role::SoftwareDeveloper,
        &file.to_string(),
        &[],
        &[],
    )
    .expect("the fixture kit loads");
    Arc::new(move |role| {
        if role == catervas_core::contract::Role::SoftwareDeveloper {
            Ok(kit.clone())
        } else {
            catervas_roles::load_kit(role)
        }
    })
}

/// `catervas connect <agent> <name> <extra>` against `kits`, with `stdin` on standard input.
fn connect_by_name(
    repository: &TempRepo,
    kits: catervas_runtime::KitSource,
    agent: &str,
    name: &str,
    extra: &[&str],
    stdin: &str,
    store: Arc<dyn ConnectorSecrets>,
) -> project::Ran {
    let mut args = vec!["connect", agent, name];
    args.extend_from_slice(extra);
    let stdin = stdin.to_string();
    let state = config_of(repository);
    run_with(&repository.path, &args, move |io| {
        io.stdin = Box::new(std::io::Cursor::new(stdin.into_bytes()));
        io.connector_secrets = store;
        io.kits = kits;
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
    })
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn connects_a_kit_connector_with_its_keys_from_standard_input() {
    let repository = a_team("connect-kit-stdin");
    let script = fixture("connect-kit-stdin");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let ran = connect_by_name(
        &repository,
        a_kit(&script, None),
        "dev-a",
        "fixture",
        &[],
        &format!("{KEY}\n"),
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(
        ran.err
            .contains("Make a key at https://fixture.example/keys, then paste API_KEY:"),
        "{}",
        ran.err
    );
    for line in [
        "search: network",
        "env: external_effect",
        "delete_repo: denied",
    ] {
        assert!(
            ran.out.lines().any(|found| found == line),
            "{line}: {}",
            ran.out
        );
    }
    assert!(
        ran.out.contains("Kept in your computer's keychain"),
        "{}",
        ran.out
    );
    assert!(!ran.out.contains(KEY) && !ran.err.contains(KEY));
    let written = entry(&repository, "dev-a").expect("the entry is written");
    assert_eq!(written["source"], "kit");
    assert_eq!(
        written["tools"],
        json!({ "search": "network", "env": "external_effect", "delete_repo": "denied" })
    );
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).expect("kept");
    assert_eq!(kept.keys["API_KEY"].expose(), KEY);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_takes_allowances() {
    let repository = a_team("connect-allowances");
    let script = fixture("connect-allowances");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let connect = |extra: &[&str]| {
        connect_by_name(
            &repository,
            a_kit_of(&script, None, true),
            "dev-a",
            "fixture",
            extra,
            &format!("{KEY}\n"),
            Arc::clone(&store) as _,
        )
    };
    let ran = connect(&["--allowance", "make=3"]);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert_eq!(
        entry(&repository, "dev-a").expect("written")["allowances"],
        json!({ "make": 3 })
    );
    let ran = connect(&[]);
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert_eq!(
        entry(&repository, "dev-a").expect("written")["allowances"],
        json!({ "make": 20 }),
        "the kit's default when none is given"
    );
    for (extra, said) in [
        (&["--allowance", "post=3"][..], "allowance_not_offered"),
        (&["--allowance", "make=1001"], "allowance_out_of_range"),
        (&["--allowance", "make=lots"], "allowance_out_of_range"),
        (&["--allowance", "make"], "--allowance wants tool=number"),
    ] {
        let ran = connect(extra);
        assert_eq!(ran.code, 1, "{extra:?}: {}{}", ran.out, ran.err);
        assert!(ran.err.contains(said), "{extra:?}: {}", ran.err);
    }
    assert_eq!(
        entry(&repository, "dev-a").expect("kept")["allowances"],
        json!({ "make": 20 }),
        "a refusal changes nothing"
    );
    let ran = run_with(
        &repository.path,
        &[
            "connect",
            "dev-b",
            "mine",
            "--command",
            "sh",
            "--allowance",
            "make=3",
        ],
        |_| {},
    );
    assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
    assert!(ran.err.contains("kit_names_these"), "{}", ran.err);
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_flags_the_kit_decides() {
    let repository = a_team("connect-kit-flags");
    let script = fixture("connect-kit-flags");
    let store = Arc::new(MemoryConnectorSecrets::default());
    for extra in [
        &["--key", "API_KEY"][..],
        &["--tag", "search=denied"],
        &["--arg", "x"],
        &["--header", "A: b"],
        &["--scope", "read"],
        &["--sign-in"],
        &["--client-id", "abc"],
        &["--callback-port", "33418"],
    ] {
        let ran = connect_by_name(
            &repository,
            a_kit(&script, None),
            "dev-a",
            "fixture",
            extra,
            &format!("{KEY}\n"),
            Arc::clone(&store) as _,
        );
        assert_eq!(ran.code, 1, "{extra:?}: {}{}", ran.out, ran.err);
        assert!(
            ran.err.contains("kit_names_these"),
            "{extra:?}: {}",
            ran.err
        );
        assert!(entry(&repository, "dev-a").is_none(), "{extra:?}");
        assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_none());
    }
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_flag_without_its_form() {
    let repository = a_team("connect-flag-form");
    let before = files_of(&repository).read_team().expect("the team reads");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let url = "https://mcp.example.com/mcp";
    for (extra, code, said) in [
        (vec!["--url", url, "--arg", "x"], 1, "--arg needs --command"),
        (
            vec!["--command", "sh", "--header", "A: b"],
            1,
            "--header needs --url",
        ),
        (
            vec!["--command", "sh", "--sign-in"],
            2,
            "cannot be used with",
        ),
        (
            vec!["--url", url, "--client-id", "a"],
            1,
            "--client-id needs --sign-in",
        ),
        (
            vec!["--url", url, "--sign-in", "--callback-port", "33418"],
            1,
            "--callback-port needs --client-id",
        ),
        (
            vec!["--url", url, "--scope", "read"],
            1,
            "--scope needs --sign-in",
        ),
    ] {
        let mut args = vec!["connect", "dev-a", "fixture"];
        args.extend_from_slice(&extra);
        let kept = Arc::clone(&store) as Arc<dyn ConnectorSecrets>;
        let state = config_of(&repository);
        let ran = run_with(&repository.path, &args, move |io| {
            io.connector_secrets = kept;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
        });
        assert_eq!(ran.code, code, "{extra:?}: {}{}", ran.out, ran.err);
        assert!(ran.err.contains(said), "{extra:?}: {}", ran.err);
        assert!(entry(&repository, "dev-a").is_none(), "{extra:?}");
        assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).is_none());
    }
    assert_eq!(
        files_of(&repository).read_team().expect("the team reads"),
        before,
        "the team file is unchanged"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn refuses_a_name_the_kit_lacks() {
    let repository = a_team("connect-kit-lacks");
    let script = fixture("connect-kit-lacks");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let ran = connect_by_name(
        &repository,
        a_kit(&script, None),
        "dev-a",
        "other",
        &[],
        "",
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err.contains("connector_not_in_kit") && ran.err.contains("fixture"),
        "{}",
        ran.err
    );
    // The Product Manager's kit is not the Developer's.
    let ran = connect_by_name(
        &repository,
        a_kit(&script, None),
        "pm",
        "fixture",
        &[],
        &format!("{KEY}\n"),
        Arc::clone(&store) as _,
    );
    assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
    assert!(ran.err.contains("connector_not_in_kit"), "{}", ran.err);
    assert!(entry(&repository, "pm").is_none());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn signs_in_to_a_kit_connector_with_oauth() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-kit-sign-in");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let config = config_of(&repository);
    let kits = a_kit(
        std::path::Path::new("unused"),
        Some(fixture.mcp_url.as_str()),
    );
    let opener = following(&runtime);
    let kept = Arc::clone(&store) as Arc<dyn ConnectorSecrets>;
    let ran = run_with(
        &repository.path,
        &["connect", "dev-a", "fixture"],
        move |io| {
            io.connector_secrets = kept;
            io.open_url = opener;
            io.kits = kits;
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        },
    );
    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    assert!(
        ran.err.contains("in your browser: http://127.0.0.1:"),
        "{}",
        ran.err
    );
    assert!(
        ran.out.lines().any(|line| line == "whoami: network"),
        "{}",
        ran.out
    );
    let written = entry(&repository, "dev-a").expect("the entry is written");
    assert_eq!(written["source"], "kit");
    assert_eq!(written["oauth"], json!({}));
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "fixture")).expect("kept");
    assert_eq!(kept.oauth.expect("a grant").issuer, fixture.origin);
}

/// A table of one loopback app, `Google test`, which signs in for Catervas's connector `osv`, whose
/// endpoints are `fixture`'s. Leaked: a table is `'static`, and a test's leak is small.
fn google_table(
    fixture: &oauth_fixture::Fixture,
) -> &'static [catervas_runtime::registered_apps::RegisteredApp] {
    use catervas_runtime::registered_apps::{AppFlow, RegisteredApp};
    fn leaked(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }
    let origin = &fixture.origin;
    Box::leak(Box::new([RegisteredApp {
        id: "google-test",
        name: "Google test",
        host: None,
        catervas_connector: Some("osv"),
        flow: AppFlow::Loopback {
            authorization_endpoint: leaked(format!("{origin}/o/oauth2/v2/auth")),
        },
        client_id: "google-test-client",
        client_secret: Some("the-test-secret"),
        scopes: &["https://example.test/auth/ads"],
        issuer: leaked(origin.clone()),
        token_endpoint: leaked(format!("{origin}/token")),
        revocation_endpoint: None,
        install_url: None,
        settings_url: "https://myaccount.google.com/connections",
    }]))
}

/// The Developer's kit with one service, Catervas's own connector `osv`, which signs in.
fn a_catervas_connector_kit() -> catervas_runtime::KitSource {
    a_catervas_connector_kit_asking(&json!({}))
}

/// [`a_catervas_connector_kit`], its `oauth` being `oauth`.
fn a_catervas_connector_kit_asking(oauth: &Value) -> catervas_runtime::KitSource {
    let file = json!({
        "role": "software_developer", "skills": [],
        "connectors": [{
            "name": "osv", "transport": "stdio", "command": "catervas",
            "args": ["connector", "osv"], "oauth": oauth,
            "title": "OSV", "about": "A stand-in.", "why": "Lets the Developer look up.",
            "setup": "Sign in.", "labels": { "search": "search the fixture" },
            "tools": { "search": "network", "env": "external_effect", "delete_repo": "denied" }
        }]
    });
    let kit = catervas_roles::parse_fixture_kit(
        catervas_core::contract::Role::SoftwareDeveloper,
        &file.to_string(),
        &[],
        &[],
    )
    .expect("the fixture kit loads");
    Arc::new(move |role| {
        if role == catervas_core::contract::Role::SoftwareDeveloper {
            Ok(kit.clone())
        } else {
            catervas_roles::load_kit(role)
        }
    })
}

/// `catervas connect` signs in for Catervas's own connector with the app the table names: the page is
/// printed and opened, the connector (here the fixture's stdio server, run by a program standing
/// in for Catervas's own) lists its tools, and the grant is kept as the app's.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_signs_in_a_catervas_connector() {
    use std::os::unix::fs::PermissionsExt as _;

    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    fixture.set(|flags| flags.client_secret = Some("the-test-secret".to_string()));
    let repository = a_team("connect-catervas-sign-in");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let script = self::fixture("connect-catervas-sign-in");
    // Whatever it is asked, the program runs the fixture's server: `catervas connector osv`.
    let program = script.with_file_name("catervas");
    std::fs::write(
        &program,
        format!("#!/bin/sh\nexec sh '{}'\n", script.display()),
    )
    .expect("the program is written");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("the program is made executable");
    let kits = a_catervas_connector_kit();
    let table = google_table(&fixture);
    let opener = following(&runtime);
    let config = config_of(&repository);
    let kept_in = Arc::clone(&store);
    let ran = run_with(&repository.path, &["connect", "dev-a", "osv"], move |io| {
        io.connector_secrets = kept_in;
        io.open_url = opener;
        io.kits = kits;
        io.registered_apps = table;
        io.own_program = Some(program);
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
    });

    assert_eq!(ran.code, 0, "{}{}", ran.out, ran.err);
    let lines: Vec<&str> = ran.err.lines().collect();
    assert_eq!(lines.len(), 2, "{}", ran.err);
    assert!(
        lines[0].starts_with(&format!(
            "Sign in to Google test in your browser: {}/o/oauth2/v2/auth?",
            fixture.origin
        )),
        "{}",
        ran.err
    );
    assert_eq!(lines[1], "Signed in to Google test.");
    assert!(
        ran.out.lines().any(|line| line == "search: network"),
        "{}",
        ran.out
    );
    let kept = loaded(store.as_ref(), &kept_at(&repository, "dev-a", "osv")).expect("kept");
    let grant = kept.oauth.expect("a grant is kept");
    assert_eq!(grant.app.as_deref(), Some("google-test"));
    assert_eq!(grant.issuer, fixture.origin);
    assert!(kept.keys.is_empty());
}

/// A guard: the connector asks for the app's scopes and no others, the kit's own among them.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_asks_only_for_the_apps_scopes() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-catervas-scopes");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let table = google_table(&fixture);
    let pages = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    for (scope, said) in [
        (
            "https://example.test/auth/other",
            Some("Catervas's Google test sign-in asks only for https://example.test/auth/ads"),
        ),
        ("https://example.test/auth/ads", None),
    ] {
        let kits = a_catervas_connector_kit_asking(&json!({ "scopes": [scope] }));
        let opener: catervas::Opener = {
            let pages = Arc::clone(&pages);
            Arc::new(move |url: &str| {
                pages.lock().expect("pages").push(url.to_string());
                // Said yes, as the browser would, so that the sign-in ends.
                std::thread::spawn({
                    let url = url.to_string();
                    move || {
                        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
                        runtime.block_on(oauth_fixture::follow(&url));
                    }
                });
                Ok(())
            })
        };
        let config = config_of(&repository);
        let kept_in = Arc::clone(&store);
        let ran = run_with(&repository.path, &["connect", "dev-a", "osv"], move |io| {
            io.connector_secrets = kept_in;
            io.open_url = opener;
            io.kits = kits;
            io.registered_apps = table;
            io.own_program = Some(PathBuf::from("unused"));
            io.env
                .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
        });
        if let Some(sentence) = said {
            assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
            assert!(ran.err.contains(sentence), "{}", ran.err);
            assert!(
                pages.lock().expect("pages").is_empty(),
                "no page was opened"
            );
            assert!(fixture.seen().is_empty(), "no request was made");
        } else {
            let pages = pages.lock().expect("pages");
            assert_eq!(pages.len(), 1, "{}", ran.err);
            let address = reqwest::Url::parse(&pages[0]).expect("an address");
            let asked = address
                .query_pairs()
                .find(|(name, _)| name == "scope")
                .map(|(_, value)| value.into_owned());
            assert_eq!(asked.as_deref(), Some(scope));
        }
    }
}

/// A guard: with no app for the connector, as in a build without Google's client secret, there is
/// no way to sign in, and nothing is kept.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_says_when_no_app_signs_in_for_a_connector() {
    let repository = a_team("connect-catervas-no-app");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let kits = a_catervas_connector_kit();
    let config = config_of(&repository);
    let kept_in = Arc::clone(&store);
    let ran = run_with(&repository.path, &["connect", "dev-a", "osv"], move |io| {
        io.connector_secrets = kept_in;
        io.kits = kits;
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
    });
    assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err
            .contains("osv does not let Catervas sign in by itself yet"),
        "{}",
        ran.err
    );
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "osv")).is_none());
}

/// A guard: the app that signs a connector in is the one whose connector it is, and not any app
/// that serves one of Catervas's connectors. With a table whose one entry is for another connector,
/// `osv` has no way to sign in, no page is opened, and nothing is kept.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_uses_only_the_app_for_the_connector_s_name() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-catervas-other-app");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let kits = a_catervas_connector_kit();
    let for_another: &'static [catervas_runtime::registered_apps::RegisteredApp] =
        Box::leak(Box::new([
            catervas_runtime::registered_apps::RegisteredApp {
                catervas_connector: Some("other"),
                ..google_table(&fixture)[0]
            },
        ]));
    let pages = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    // A page that is opened is followed, as a browser would, so that a sign-in that was wrongly
    // started ends in a failed assertion here and not in ten minutes of waiting.
    let opener: catervas::Opener = {
        let pages = Arc::clone(&pages);
        Arc::new(move |url: &str| {
            pages.lock().expect("pages").push(url.to_string());
            std::thread::spawn({
                let url = url.to_string();
                move || {
                    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
                    runtime.block_on(oauth_fixture::follow(&url));
                }
            });
            Ok(())
        })
    };
    let config = config_of(&repository);
    let kept_in = Arc::clone(&store);
    let ran = run_with(&repository.path, &["connect", "dev-a", "osv"], move |io| {
        io.connector_secrets = kept_in;
        io.open_url = opener;
        io.kits = kits;
        io.registered_apps = for_another;
        io.own_program = Some(PathBuf::from("unused"));
        io.env
            .insert("XDG_CONFIG_HOME".to_string(), config.display().to_string());
    });
    assert_eq!(ran.code, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err
            .contains("osv does not let Catervas sign in by itself yet"),
        "{}",
        ran.err
    );
    assert!(
        pages.lock().expect("pages").is_empty(),
        "no page was opened"
    );
    assert!(fixture.seen().is_empty(), "no request was made");
    assert!(loaded(store.as_ref(), &kept_at(&repository, "dev-a", "osv")).is_none());
}

/// `catervas connect` starts Catervas's own connector as the program the process was found at (ADR
/// 0038), with `PATH` empty so no `catervas` there can stand in; without that program it says so.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn catervas_connect_starts_catervas_s_own_connector_as_its_own_program() {
    let repository = a_team("connect-own");
    let connect_osv = |agent: &str, own: Option<PathBuf>| {
        let state = config_of(&repository);
        run_with(
            &repository.path,
            &[
                "connect",
                agent,
                "osv",
                "--command",
                "catervas",
                "--arg",
                "connector",
                "--arg",
                "osv",
            ],
            move |io| {
                io.own_program = own;
                io.env
                    .insert("XDG_CONFIG_HOME".to_string(), state.display().to_string());
                io.env.insert("PATH".to_string(), String::new());
            },
        )
    };

    let ran = connect_osv("dev-a", Some(PathBuf::from(env!("CARGO_BIN_EXE_catervas"))));
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    for tool in ["query_package", "query_packages", "get_vulnerability"] {
        assert!(ran.out.contains(tool), "{tool} in {}", ran.out);
    }

    let ran = connect_osv("dev-b", None);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("catervas could not find its own program"),
        "{}",
        ran.err
    );
}
