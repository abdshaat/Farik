//! `catervas connector run` and `catervas connector headers` against a served daemon (ADR 0030): a
//! custom connector gets its keys from the daemon, with nothing else of the session's environment,
//! and nothing at all when the team file changed it since it was connected.
//!
//! Each test serves a daemon on a repository, and so needs the `git` program: it is `#[ignore]`d
//! and run by `cargo xtask check --integration`. The launcher replaces its process with the
//! server, so it is run as the `catervas` binary, never in this process.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use catervas::{CliIo, run_cli};
use catervas_core::budget::DEFAULT_SESSION_LIMITS;
use catervas_core::governor::permissions::SessionConnector;
use catervas_core::team::fixtures::{a_team_wire, an_agent_wire};
use catervas_core::team::{CustomServer, Team, custom_server, spec_sha256, validate_team};
use catervas_protocol::clock::{Clock, FixedClock};
use catervas_protocol::event::EventIds;
use catervas_runtime::claude::Secret;
use catervas_runtime::connectors::{
    ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt,
};
use catervas_runtime::daemon::{
    DaemonConfig, DaemonHandle, DaemonState, SessionRegistration, serve,
};
use catervas_runtime::sign_in::OAuthGrant;
use catervas_runtime::transitions::Transitions;
use catervas_runtime::{SessionPurpose, ToolDeps};
use catervas_store::files::ProjectFiles;
use catervas_store::git::fixtures::TempRepo;
use catervas_store::{IN_MEMORY, open_event_log, open_projections};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};

const SESSION: &str = "5a1c2a9e-8b7d-4e6f-9a01-2b3c4d5e6f71";
const KEY_VALUE: &str = "ghp-a-secret-value";

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0)
        .single()
        .expect("a real time")
}

/// `dev-a`'s team, its custom servers changed by `change`: `printenv`, started on the host as the
/// `env` program so it prints what it was given; `linear`, at a web address; and `whereami`, the
/// `pwd` program, which prints the folder it runs in.
fn a_team(change: impl FnOnce(&mut Value)) -> Team {
    let mut wire = a_team_wire();
    wire["agents"] = json!([
        an_agent_wire("pm", "product_manager"),
        an_agent_wire("dev-a", "software_developer"),
    ]);
    wire["agents"][1]["mcp_servers"] = json!([
        {
            "name": "printenv", "source": "custom", "transport": "stdio",
            "command": "env", "args": [],
            "credential_keys": ["API_KEY"],
            "tools": { "search": "network" }
        },
        {
            "name": "linear", "source": "custom", "transport": "http",
            "url": "https://mcp.linear.example/mcp",
            "headers": { "Authorization": "Bearer {API_KEY}" },
            "credential_keys": ["API_KEY"],
            "tools": { "search": "network" }
        },
        {
            "name": "whereami", "source": "custom", "transport": "stdio",
            "command": "pwd", "credential_keys": [],
            "tools": { "search": "network" }
        },
        {
            "name": "osv", "source": "custom", "transport": "stdio",
            "command": "catervas", "args": ["connector", "osv"], "credential_keys": [],
            "tools": { "query_package": "network" }
        },
        {
            "name": "google-ads", "source": "custom", "transport": "stdio",
            "command": "catervas", "args": ["connector", "google-ads"], "oauth": {},
            "tools": { "report": "network" }
        }
    ]);
    change(&mut wire);
    validate_team(&wire).expect("a team")
}

/// A sign-in the service has not ended, good for an hour, so that nothing is refreshed.
fn a_grant() -> OAuthGrant {
    OAuthGrant {
        issuer: "https://accounts.example".to_string(),
        resource: "https://ads.example/".to_string(),
        client_id: "a-client".to_string(),
        token_endpoint: "http://127.0.0.1:1/token".to_string(),
        revocation_endpoint: None,
        access_token: Secret::new("an-access-token-the-shim-never-sees".to_string()),
        refresh_token: Some(Secret::new("a-refresh-token".to_string())),
        issued_at: Utc::now(),
        expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
        scopes: Vec::new(),
        lapsed: false,
        app: None,
    }
}

fn servers(team: &Team) -> Vec<CustomServer> {
    team.agents[1]
        .mcp_servers
        .iter()
        .flatten()
        .filter_map(custom_server)
        .collect()
}

/// A daemon serving a repository whose team is `a_team`, with `dev-a`'s session given both
/// servers, and each connected as it is now.
struct Served {
    repo: TempRepo,
    daemon_file: PathBuf,
    handle: Option<DaemonHandle>,
    runtime: tokio::runtime::Runtime,
}

impl Served {
    /// The user's state folder for `repo`: beside it and outside it, as `~/.config/catervas` is.
    fn state_of(repo: &TempRepo) -> PathBuf {
        PathBuf::from(format!("{}-state", repo.path.display()))
    }

    fn new(name: &str) -> Self {
        let repo = TempRepo::new(name);
        let team = a_team(|_| {});
        let files = Arc::new(ProjectFiles::open(repo.path.clone()));
        files.init(&team).expect(".catervas/ is made");
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        let projections = Arc::new(open_projections(Arc::clone(&log)).expect("projections"));
        let clock: Arc<dyn Clock + Send + Sync> = Arc::new(FixedClock::new(at()));
        let ids = EventIds {
            team_id: "catervas".to_string(),
            project_id: "catervas".to_string(),
            ..EventIds::default()
        };
        let transitions = Arc::new(Transitions::new(
            Arc::clone(&log),
            Arc::clone(&projections),
            Arc::clone(&files),
            repo.adapter(),
            Arc::clone(&clock),
            ids.clone(),
        ));
        let state = Arc::new(DaemonState::new(Arc::new(ToolDeps {
            log,
            projections,
            files,
            transitions,
            git: repo.adapter(),
            clock,
            ids,
            kits: Arc::new(catervas_roles::load_kit),
        })));
        let store = Arc::new(MemoryConnectorSecrets::default());
        for server in servers(&team) {
            let at = SecretAt::of(&Served::state_of(&repo), &repo.path, "dev-a", &server.name)
                .expect("an address");
            // A server that signs in is kept with a grant, which never leaves the daemon.
            let entry = if server.oauth().is_some() {
                ConnectorEntry {
                    spec_sha256: spec_sha256(&server),
                    keys: std::collections::BTreeMap::new(),
                    oauth: Some(a_grant()),
                }
            } else {
                ConnectorEntry {
                    spec_sha256: spec_sha256(&server),
                    keys: [("API_KEY".to_string(), Secret::new(KEY_VALUE.to_string()))].into(),
                    oauth: None,
                }
            };
            store.save(&at, &entry).expect("kept");
        }
        state.set_connector_secrets(store);
        state.set_state_dir(Served::state_of(&repo));
        state.register_session(SessionRegistration {
            session_id: SESSION.to_string(),
            web: catervas_core::governor::sites::WebAccess::Open,
            reads: catervas_core::folders::ReadAccess::Open,
            agent_id: "dev-a".to_string(),
            task_id: None,
            cwd: repo.path.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            catervas_tools: Vec::new(),
            tiers: Vec::new(),
            connectors: servers(&team)
                .into_iter()
                .map(|server| SessionConnector {
                    server: server.name,
                    origin: None,
                    tools: server.tools,
                    allowances: std::collections::BTreeMap::new(),
                    plan_tools: std::collections::BTreeSet::new(),
                })
                .collect(),
            preview: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
        });
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let daemon_file = repo.path.join(".catervas/local/daemon.json");
        let handle = runtime
            .block_on(serve(
                DaemonConfig {
                    port: catervas_runtime::daemon::PortChoice::Any,
                    daemon_file: Some(daemon_file.clone()),
                },
                state,
            ))
            .expect("the daemon is up");
        Self {
            repo,
            daemon_file,
            handle: Some(handle),
            runtime,
        }
    }

    /// Writes the team file again, with `change` made to its servers.
    fn change_team(&self, change: impl FnOnce(&mut Value)) {
        ProjectFiles::open(self.repo.path.clone())
            .write_team(&a_team(change))
            .expect("the team is changed");
    }

    /// `catervas connector <verb>` for `server`, run as the binary with `env` alone.
    fn binary(&self, verb: &str, server: &str, env: &[(&str, &str)]) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_catervas"))
            .args(["connector", verb, "--daemon"])
            .arg(&self.daemon_file)
            .args(["--session", SESSION, "--server", server])
            // Where Claude Code starts it: the session's worktree.
            .current_dir(&self.repo.path)
            .env_clear()
            .envs(env.iter().copied())
            .stdin(std::process::Stdio::null())
            .output()
            .expect("catervas runs")
    }

    /// `catervas connector headers` for `server`, in this process: it prints and never execs.
    fn headers(&self, server: &str) -> (i32, String, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = {
            let mut io = CliIo::new(
                std::env::temp_dir(),
                Box::new(&mut out),
                Box::new(&mut err),
                Arc::new(FixedClock::new(at())),
            );
            let daemon = self.daemon_file.display().to_string();
            let arguments: Vec<String> = [
                "catervas",
                "connector",
                "headers",
                "--daemon",
                &daemon,
                "--session",
                SESSION,
                "--server",
                server,
            ]
            .map(ToString::to_string)
            .to_vec();
            run_cli(&arguments, &mut io)
        };
        (
            code,
            String::from_utf8(out).expect("text"),
            String::from_utf8(err).expect("text"),
        )
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = self.runtime.block_on(handle.shutdown());
        }
    }
}

/// What a session's environment holds besides what a connector keeps: the model credential
/// among it.
fn session_env(temp: &str) -> Vec<(&'static str, String)> {
    vec![
        ("PATH", std::env::var("PATH").unwrap_or_default()),
        ("HOME", "/home/someone".to_string()),
        ("LANG", "C.UTF-8".to_string()),
        ("TMPDIR", temp.to_string()),
        ("ANTHROPIC_API_KEY", "sk-ant-model-secret".to_string()),
        ("CLAUDE_CODE_OAUTH_TOKEN", "oauth-model-secret".to_string()),
        ("CATERVAS_OTHER", "anything".to_string()),
    ]
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn connector_run_execs_with_a_clean_environment() {
    let served = Served::new("connector-run-env");
    let env = session_env("/tmp");
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let output = served.binary("run", "printenv", &env);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).expect("text");
    let names: BTreeSet<&str> = printed
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name))
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["API_KEY", "HOME", "LANG", "PATH", "TMPDIR"]),
        "{printed}"
    );
    assert!(
        printed.contains(&format!("API_KEY={KEY_VALUE}\n")),
        "{printed}"
    );
    assert!(printed.contains("HOME=/home/someone\n"), "{printed}");
    assert!(!printed.contains("model-secret"), "{printed}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn the_headers_helper_prints_the_filled_headers() {
    let served = Served::new("connector-headers");
    let (code, out, err) = served.headers("linear");
    assert_eq!(code, 0, "{err}");
    let printed: Value = serde_json::from_str(&out).expect("one JSON object");
    assert_eq!(
        printed,
        json!({ "Authorization": format!("Bearer {KEY_VALUE}") })
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn a_server_changed_since_connect_gets_nothing() {
    let served = Served::new("connector-changed");
    // A pulled commit moves `linear` and has `printenv` run something else.
    served.change_team(|wire| {
        wire["agents"][1]["mcp_servers"][1]["url"] = json!("https://attacker.example/mcp");
        wire["agents"][1]["mcp_servers"][0]["args"] = json!(["CATERVAS_RAN=1"]);
    });

    let (code, out, err) = served.headers("linear");
    assert_ne!(code, 0);
    assert_eq!(out, "", "the helper printed headers");
    assert!(err.contains("connector_not_confirmed"), "{err}");
    assert!(!err.contains(KEY_VALUE), "{err}");

    let env = session_env("/tmp");
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let output = served.binary("run", "printenv", &env);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "the server ran");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(said.contains("connector_not_confirmed"), "{said}");
    assert!(!said.contains(KEY_VALUE), "{said}");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn the_launcher_refuses_a_server_the_session_was_not_given() {
    let served = Served::new("connector-not-given");
    let env = session_env("/tmp");
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let output = served.binary("run", "jira", &env);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(said.contains("connector_not_in_session"), "{said}");
    // The helper asked about a server started on the host prints nothing either.
    let (code, out, _) = served.headers("printenv");
    assert_ne!(code, 0);
    assert_eq!(out, "");
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn connector_run_starts_the_server_in_a_folder_catervas_keeps() {
    // Claude Code starts the launcher in the task's worktree, which agents write to: a server
    // started there would run `node_modules/.bin` or a `server.py` an agent left (finding C1).
    let served = Served::new("connector-run-folder");
    let env = session_env("/tmp");
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let output = served.binary("run", "whereami", &env);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).expect("text");
    // Outside the repository, in the user's state folder (fix wave C).
    let at = SecretAt::of(
        &Served::state_of(&served.repo),
        &served.repo.path,
        "dev-a",
        "whereami",
    )
    .expect("an address");
    let folder = Served::state_of(&served.repo)
        .canonicalize()
        .expect("the state folder")
        .join("connectors")
        .join(&at.project_id)
        .join("dev-a/whereami");
    assert_eq!(printed.trim_end(), folder.display().to_string());
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn the_launcher_starts_its_own_executable_for_catervas() {
    use std::io::{BufRead as _, Write as _};

    // `command: catervas` is Catervas's own program, run as this very binary, with no PATH at all to
    // find another `catervas` in (ADR 0038).
    let served = Served::new("connector-run-catervas");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_catervas"))
        .args(["connector", "run", "--daemon"])
        .arg(&served.daemon_file)
        .args(["--session", SESSION, "--server", "osv"])
        .current_dir(&served.repo.path)
        .env_clear()
        .env("PATH", "")
        .env("HOME", "/home/someone")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("catervas runs");
    let mut stdin = child.stdin.take().expect("stdin");
    writeln!(
        stdin,
        "{}",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": { "name": "launcher-test", "version": "1" } } })
    )
    .expect("initialize is sent");
    let stdout = child.stdout.take().expect("stdout");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let answered = receiver.recv_timeout(std::time::Duration::from_secs(30));
    let _ = child.kill();
    let output = child.wait_with_output().expect("catervas ends");
    let line = answered.unwrap_or_else(|_| {
        panic!(
            "no answer to initialize: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let reply: Value = serde_json::from_str(&line).unwrap_or_else(|_| {
        panic!(
            "not JSON: {line:?} {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(
        reply["result"]["serverInfo"]["name"], "catervas-osv",
        "{reply}"
    );
}

#[test]
#[ignore = "needs the git program: cargo xtask check --integration"]
fn the_launched_shim_reaches_the_daemon_with_its_ticket() {
    use std::io::{BufRead as _, Write as _};

    // `catervas connector run` starts Google Ads' shim with the ticket and the daemon's address, so
    // a call reaches the daemon's route, which knows the session by the ticket: here it answers
    // that the entry is not the kit's, which only a call it accepted gets (a ticket it did not
    // accept is a 401, and a shim with no ticket says it runs only inside a Catervas session).
    let served = Served::new("connector-run-ads-shim");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_catervas"))
        .args(["connector", "run", "--daemon"])
        .arg(&served.daemon_file)
        .args(["--session", SESSION, "--server", "google-ads"])
        .current_dir(&served.repo.path)
        .env_clear()
        .env("PATH", "")
        .env("HOME", "/home/someone")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("catervas runs");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            let _ = sender.send(line);
        }
    });
    let mut send = |message: Value| writeln!(stdin, "{message}").expect("a line is sent");
    let answer_to = |id: u64| -> Value {
        loop {
            let line = receiver
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap_or_else(|_| panic!("no answer to {id}"));
            let reply: Value = serde_json::from_str(&line).expect("JSON");
            if reply["id"] == json!(id) {
                return reply;
            }
        }
    };
    send(
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": { "name": "launcher-test", "version": "1" } } }),
    );
    assert_eq!(
        answer_to(1)["result"]["serverInfo"]["name"],
        "catervas-google-ads"
    );
    send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let listed = answer_to(2);
    assert_eq!(listed["result"]["tools"].as_array().map(Vec::len), Some(10));
    send(
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "name": "list_accounts", "arguments": {} } }),
    );
    let called = answer_to(3);
    let _ = child.kill();
    let output = child.wait_with_output().expect("catervas ends");
    assert_eq!(called["result"]["isError"], json!(true), "{called}");
    let said = called["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        said.starts_with("google_ads_not_kit: "),
        "{said} {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
