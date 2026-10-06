//! `farik connect` and `farik disconnect` (`docs/SPEC.md` 5.6, ADR 0030): the command lists the
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

use farik_protocol::command::{Command, command_to_value};
use farik_protocol::event::{EventBody, EventKind};
use farik_runtime::connectors::{
    ConnectorEntry, ConnectorSecretStores, ConnectorSecrets, MemoryConnectorSecrets, SecretAt,
    SecretStore,
};
use farik_runtime::credential::CredentialError;
use farik_store::git::fixtures::TempRepo;
use serde_json::{Value, json};

use project::{LiveDriver, a_team, events, files_of, log_of, run_with, scratch};

/// An authorization server and a protected MCP server, as the runtime's tests run them.
#[path = "../../runtime/tests/support/oauth_fixture.rs"]
mod oauth_fixture;
#[path = "../../runtime/tests/support/ports.rs"]
mod ports;

const KEY: &str = "a-key-typed-at-the-terminal";

/// The stdio MCP server of the runtime's tests, written for `test`: its tools are `search`, `env`,
/// `delete_repo`, and `repo.delete`, a name Farik can't use.
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

/// The user's state folder for `repository`'s tests: `farik` in [`config_of`].
fn state_of(repository: &TempRepo) -> PathBuf {
    config_of(repository).join("farik")
}

/// Where `agent`'s keys for `server` are kept in `repository`'s project: where the daemon looks
/// for them, under the project's id on this machine, never the log's team or project id.
fn kept_at(repository: &TempRepo, agent: &str, server: &str) -> SecretAt {
    let at =
        SecretAt::of(&state_of(repository), &repository.path, agent, server).expect("an address");
    let first = log_of(repository)
        .read(&farik_store::EventQuery {
            limit: Some(1),
            ..farik_store::EventQuery::default()
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

/// `farik connect <agent> fixture` against `script`, with `extra` arguments, `stdin` on standard
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
fn farik_connect_reads_keys_from_stdin() {
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
fn farik_connect_starts_nothing_when_farik_settings_are_inside_the_project() {
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
            .contains("state_inside_project: Farik's settings folder"),
        "{}",
        ran.err
    );
    assert!(!ran_marker.exists(), "the server was started");
    assert!(entry(&repository, "dev-a").is_none());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_connect_labels_with_tag_flags_and_defaults_to_external_effect() {
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
        "repo.delete: Farik can't use this tool",
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
fn farik_connect_says_which_store_kept_the_keys() {
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
fn farik_connect_sends_names_when_something_drives() {
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
fn farik_disconnect_deletes_the_keys_and_the_entry() {
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

/// An opener that follows the address as a browser would, on `runtime`.
fn following(runtime: &tokio::runtime::Runtime) -> farik::Opener {
    let handle = runtime.handle().clone();
    Arc::new(move |url: &str| {
        let url = url.to_string();
        handle.spawn(async move {
            oauth_fixture::follow(&url).await;
        });
        Ok(())
    })
}

/// `farik connect dev-a fixture --url <fixture> --sign-in <extra>`, opened with `opener`.
fn sign_in(
    repository: &TempRepo,
    fixture: &oauth_fixture::Fixture,
    extra: &[&str],
    opener: farik::Opener,
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
    opener: farik::Opener,
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
) -> &'static [farik_runtime::registered_apps::RegisteredApp] {
    use farik_runtime::registered_apps::{AppFlow, RegisteredApp};
    fn leaked(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }
    let origin = &fixture.origin;
    Box::leak(Box::new([RegisteredApp {
        id: "dev",
        name: "Dev",
        host: "127.0.0.1",
        flow: AppFlow::Device {
            device_endpoint: leaked(format!("{origin}/device/code")),
            verification_uri: leaked(format!("{origin}/login/device")),
        },
        client_id: "dev-client",
        issuer: leaked(format!("{origin}/login/oauth")),
        token_endpoint: leaked(format!("{origin}/token")),
        revocation_endpoint: None,
        install_url: Some("https://github.com/apps/dev/installations/new"),
        settings_url: "https://github.com/settings/apps/authorizations",
    }]))
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_connect_prints_the_device_code() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = runtime.block_on(oauth_fixture::Fixture::start());
    let repository = a_team("connect-device");
    let store = Arc::new(MemoryConnectorSecrets::default());
    let _driver = LiveDriver::new(&repository);
    let pages = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let opener: farik::Opener = {
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
            "Only enter a code that this page shows you. Farik never sends you a code in a chat.",
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
fn farik_connect_names_the_provider_when_a_device_sign_in_is_refused() {
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
fn farik_connect_signs_in_and_keeps_the_grant() {
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
    let by_hand: farik::Opener = Arc::new(move |url: &str| {
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
fn farik_connect_again_revokes_the_replaced_grant() {
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
fn farik_connect_again_with_the_same_client_revokes_nothing() {
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
fn farik_connect_sign_in_takes_no_key() {
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
fn farik_connect_says_when_a_service_offers_no_sign_in() {
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
fn farik_disconnect_asks_the_service_to_forget_the_sign_in() {
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
fn a_kit(script: &std::path::Path, oauth_url: Option<&str>) -> farik_runtime::KitSource {
    a_kit_of(script, oauth_url, false)
}

/// [`a_kit`], and, when `allowing`, with a spending tool `make` that may run 20 times a sprint
/// unasked and a `post` that always asks.
fn a_kit_of(
    script: &std::path::Path,
    oauth_url: Option<&str>,
    allowing: bool,
) -> farik_runtime::KitSource {
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
    let kit = farik_roles::parse_fixture_kit(
        farik_core::contract::Role::SoftwareDeveloper,
        &file.to_string(),
        &[],
        &[],
    )
    .expect("the fixture kit loads");
    Arc::new(move |role| {
        if role == farik_core::contract::Role::SoftwareDeveloper {
            Ok(kit.clone())
        } else {
            farik_roles::load_kit(role)
        }
    })
}

/// `farik connect <agent> <name> <extra>` against `kits`, with `stdin` on standard input.
fn connect_by_name(
    repository: &TempRepo,
    kits: farik_runtime::KitSource,
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
fn farik_connect_takes_allowances() {
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

/// `farik connect` starts Farik's own connector as the program the process was found at (ADR
/// 0038), with `PATH` empty so no `farik` there can stand in; without that program it says so.
#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn farik_connect_starts_farik_s_own_connector_as_its_own_program() {
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
                "farik",
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

    let ran = connect_osv("dev-a", Some(PathBuf::from(env!("CARGO_BIN_EXE_farik"))));
    assert_eq!(ran.code, 0, "{}\n{}", ran.out, ran.err);
    for tool in ["query_package", "query_packages", "get_vulnerability"] {
        assert!(ran.out.contains(tool), "{tool} in {}", ran.out);
    }

    let ran = connect_osv("dev-b", None);
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(
        ran.err.contains("farik could not find its own program"),
        "{}",
        ran.err
    );
}
