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

/// Where `agent`'s keys for `server` are kept in `repository`'s project: where the daemon looks
/// for them, under the project's id on this machine, never the log's team or project id.
fn kept_at(repository: &TempRepo, agent: &str, server: &str) -> SecretAt {
    let at = SecretAt::of(&repository.path, agent, server).expect("an address");
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
    run_with(&repository.path, &args, move |io| {
        io.stdin = Box::new(std::io::Cursor::new(stdin.into_bytes()));
        io.connector_secrets = store;
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
    assert!(ran.err.contains("tag_unknown_tool: delete_rep"), "{}", ran.err);
    assert!(ran.err.contains("search, env, delete_repo"), "{}", ran.err);
    assert_eq!(store.load(&kept_at(&repository, "dev-b", "fixture")), Ok(None));
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
    let ran = run_with(
        &repository.path,
        &["disconnect", "dev-a", "fixture"],
        move |io| io.connector_secrets = held,
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
