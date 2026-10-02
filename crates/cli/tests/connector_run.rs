//! `farik connector run` and `farik connector headers` against a served daemon (ADR 0030): a
//! custom connector gets its keys from the daemon, with nothing else of the session's environment,
//! and nothing at all when the team file changed it since it was connected.
//!
//! Each test serves a daemon on a repository, and so needs the `git` program: it is `#[ignore]`d
//! and run by `cargo xtask check --integration`. The launcher replaces its process with the
//! server, so it is run as the `farik` binary, never in this process.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use farik::{CliIo, run_cli};
use farik_core::budget::DEFAULT_SESSION_LIMITS;
use farik_core::governor::permissions::SessionConnector;
use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
use farik_core::team::{CustomServer, Team, custom_server, spec_sha256, validate_team};
use farik_protocol::clock::{Clock, FixedClock};
use farik_protocol::event::EventIds;
use farik_runtime::claude::Secret;
use farik_runtime::connectors::{
    ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt,
};
use farik_runtime::daemon::{DaemonConfig, DaemonHandle, DaemonState, SessionRegistration, serve};
use farik_runtime::transitions::Transitions;
use farik_runtime::{SessionPurpose, ToolDeps};
use farik_store::files::ProjectFiles;
use farik_store::git::fixtures::TempRepo;
use farik_store::{IN_MEMORY, open_event_log, open_projections};
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
        }
    ]);
    change(&mut wire);
    validate_team(&wire).expect("a team")
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
    /// The user's state folder for `repo`: beside it and outside it, as `~/.config/farik` is.
    fn state_of(repo: &TempRepo) -> PathBuf {
        PathBuf::from(format!("{}-state", repo.path.display()))
    }

    fn new(name: &str) -> Self {
        let repo = TempRepo::new(name);
        let team = a_team(|_| {});
        let files = Arc::new(ProjectFiles::open(repo.path.clone()));
        files.init(&team).expect(".farik/ is made");
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        let projections = Arc::new(open_projections(Arc::clone(&log)).expect("projections"));
        let clock: Arc<dyn Clock + Send + Sync> = Arc::new(FixedClock::new(at()));
        let ids = EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
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
            kits: Arc::new(farik_roles::load_kit),
        })));
        let store = Arc::new(MemoryConnectorSecrets::default());
        for server in servers(&team) {
            let at = SecretAt::of(&Served::state_of(&repo), &repo.path, "dev-a", &server.name)
                .expect("an address");
            let entry = ConnectorEntry {
                spec_sha256: spec_sha256(&server),
                keys: [("API_KEY".to_string(), Secret::new(KEY_VALUE.to_string()))].into(),
                oauth: None,
            };
            store.save(&at, &entry).expect("kept");
        }
        state.set_connector_secrets(store);
        state.set_state_dir(Served::state_of(&repo));
        state.register_session(SessionRegistration {
            session_id: SESSION.to_string(),
            agent_id: "dev-a".to_string(),
            task_id: None,
            cwd: repo.path.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: Vec::new(),
            connectors: servers(&team)
                .into_iter()
                .map(|server| SessionConnector {
                    server: server.name,
                    origin: None,
                    tools: server.tools,
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
        let daemon_file = repo.path.join(".farik/local/daemon.json");
        let handle = runtime
            .block_on(serve(
                DaemonConfig {
                    port: farik_runtime::daemon::PortChoice::Any,
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

    /// `farik connector <verb>` for `server`, run as the binary with `env` alone.
    fn binary(&self, verb: &str, server: &str, env: &[(&str, &str)]) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_farik"))
            .args(["connector", verb, "--daemon"])
            .arg(&self.daemon_file)
            .args(["--session", SESSION, "--server", server])
            // Where Claude Code starts it: the session's worktree.
            .current_dir(&self.repo.path)
            .env_clear()
            .envs(env.iter().copied())
            .stdin(std::process::Stdio::null())
            .output()
            .expect("farik runs")
    }

    /// `farik connector headers` for `server`, in this process: it prints and never execs.
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
                "farik",
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
        ("FARIK_OTHER", "anything".to_string()),
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
        wire["agents"][1]["mcp_servers"][0]["args"] = json!(["FARIK_RAN=1"]);
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
fn connector_run_starts_the_server_in_a_folder_farik_keeps() {
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
