//! One real Claude Code session, on haiku, through the whole harness: the adapter, the served
//! daemon, and the `farik` binary as its hooks. It costs a few cents, talks to the model, and runs
//! only by hand, with `FARIK_LIVE_TESTS=1` and `ANTHROPIC_API_KEY` or `CLAUDE_CODE_OAUTH_TOKEN` in
//! the environment; it never runs in CI. Without the variable it says so and returns.
#![cfg(unix)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use farik_core::budget::DEFAULT_SESSION_LIMITS;
use farik_core::governor::permissions::PermissionTier;
use farik_core::governor::permissions::SessionConnector;
use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
use farik_core::team::{CustomServer, Effort, custom_server, spec_sha256, validate_team};
use farik_protocol::clock::Clock;
use farik_protocol::event::{EventIds, EventKind};
use farik_runtime::claude::{
    ClaudeAdapter, ClaudeConfig, ClaudeCredential, Secret, allowed_builtins, credential_from_env,
};
use farik_runtime::connectors::{
    ConnectorEntry, ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt,
};
use farik_runtime::daemon::{DaemonConfig, DaemonState, SessionRegistration, serve};
use farik_runtime::session::{McpServerConfig, McpTransport, SessionSkill};
use farik_runtime::sign_in::OAuthGrant;
use farik_runtime::transitions::Transitions;
use farik_runtime::{
    EndReason, RuntimeAdapter, SessionEvent, SessionPurpose, SessionSpec, ToolDeps,
};
use farik_store::files::ProjectFiles;
use farik_store::git::fixtures::TempRepo;
use farik_store::{EventLog, EventQuery, IN_MEMORY, open_event_log, open_projections};
use serde_json::{Value, json};

const SECRET: &str = "s3cr3t-farik-live-value";
/// The key a custom connector is connected with.
const CONNECTOR_KEY: &str = "fixture-key-value";
/// The stdio MCP server the custom connector runs (`fixtures/mcp_server.sh`).
const FIXTURE_SERVER: &str = include_str!("../../runtime/tests/fixtures/mcp_server.sh");

/// The variables of this process's environment a session is given besides its credential.
const BASE_ENV: [&str; 6] = ["PATH", "HOME", "USER", "LANG", "TERM", "TMPDIR"];

/// The time as it is: the session is real, and so is its log.
struct Now;

impl Clock for Now {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// The `claude` on `PATH`, or the one the installer puts in `~/.local/bin`.
fn find_claude() -> PathBuf {
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    on_path
        .into_iter()
        .chain([home.join(".local/bin")])
        .map(|directory| directory.join("claude"))
        .find(|candidate| candidate.is_file())
        .expect("a claude program on PATH or in ~/.local/bin")
}

/// A version 4 UUID from the kernel's random source, which `--session-id` requires.
fn a_session_id() -> String {
    let mut bytes = [0_u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .expect("random bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A `claude` that is the real one with its standard output also kept in `copy`, so the test
/// can read the `init` line the adapter's parser passes over, and its debug log in `debug`, so a
/// failure can say what happened between it and Farik's MCP server.
fn a_recording_claude(directory: &Path, real: &Path, copy: &Path, debug: &Path) -> PathBuf {
    let path = directory.join("claude");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n'{}' --debug-file '{}' \"$@\" | tee -a '{}'\n",
            real.display(),
            debug.display(),
            copy.display()
        ),
    )
    .expect("the wrapper is written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("the wrapper is executable");
    path
}

/// The Farik tool the session is given, and calls.
const FARIK_TOOL: &str = "farik_read_board";
/// What Claude Code calls it.
const FARIK_TOOL_IN_CLAUDE: &str = "mcp__farik__farik_read_board";

/// A repository with a note and a protected `.env`, a team with `dev-a`, and a daemon serving
/// `dev-a`'s session in the repository itself, with no task and `farik_read_board`: every write
/// is outside its allowed paths.
struct Project {
    repo: TempRepo,
    log: Arc<EventLog>,
    state: Arc<DaemonState>,
    tiers: BTreeSet<PermissionTier>,
    session_id: String,
}

impl Project {
    fn new() -> Project {
        Project::with_team(|_, _| {})
    }

    /// `new`, with `change` made to the team's wire, given the repository's folder: each custom
    /// server `dev-a` then has is given to its session, and connected as it is.
    #[allow(clippy::too_many_lines, reason = "one fixture, built in one place")]
    fn with_team(change: impl FnOnce(&mut Value, &Path)) -> Project {
        Project::with_team_signed_in(change, |_| None)
    }

    /// [`Project::with_team`], each custom server connected with the sign-in `grants` gives it.
    #[allow(clippy::too_many_lines, reason = "one fixture, built in one place")]
    fn with_team_signed_in(
        change: impl FnOnce(&mut Value, &Path),
        grants: impl Fn(&CustomServer) -> Option<OAuthGrant>,
    ) -> Project {
        let repo = TempRepo::new("live-claude");
        repo.write("note.txt", "hello live\n");
        repo.write(".env", &format!("FARIK_LIVE_SECRET={SECRET}\n"));
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("pm", "product_manager"),
            an_agent_wire("dev-a", "software_developer"),
        ]);
        change(&mut wire, &repo.path);
        let team = validate_team(&wire).expect("a team");
        let custom: Vec<CustomServer> = team.agents[1]
            .mcp_servers
            .iter()
            .flatten()
            .filter_map(custom_server)
            .collect();
        let tiers: BTreeSet<PermissionTier> = team
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == "dev-a")
            .expect("dev-a")
            .tiers(&team.permissions())
            .into_iter()
            .collect();
        let files = Arc::new(ProjectFiles::open(repo.path.clone()));
        files.init(&team).expect(".farik/ is made");
        let clock: Arc<dyn Clock + Send + Sync> = Arc::new(Now);
        let log =
            Arc::new(open_event_log(Path::new(IN_MEMORY), clock.now()).expect("the log opens"));
        let projections = Arc::new(open_projections(Arc::clone(&log)).expect("projections"));
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
            log: Arc::clone(&log),
            projections,
            files,
            transitions,
            git: repo.adapter(),
            clock,
            ids,
            kits: Arc::new(farik_roles::load_kit),
        })));
        // The user's state folder, outside the repository, where each server runs.
        let state_dir = PathBuf::from(format!("{}-state", repo.path.display()));
        let store = Arc::new(MemoryConnectorSecrets::default());
        for server in &custom {
            let at =
                SecretAt::of(&state_dir, &repo.path, "dev-a", &server.name).expect("an address");
            let entry = ConnectorEntry {
                spec_sha256: spec_sha256(server),
                keys: server
                    .credential_keys
                    .iter()
                    .map(|key| (key.clone(), Secret::new(CONNECTOR_KEY.to_string())))
                    .collect(),
                oauth: grants(server),
            };
            store.save(&at, &entry).expect("kept");
        }
        state.set_connector_secrets(store);
        state.set_state_dir(state_dir);
        let session_id = a_session_id();
        state.register_session(SessionRegistration {
            session_id: session_id.clone(),
            agent_id: "dev-a".to_string(),
            task_id: None,
            cwd: repo.path.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: vec![FARIK_TOOL.to_string()],
            tiers: tiers.iter().copied().collect(),
            connectors: custom
                .iter()
                .map(|server| SessionConnector {
                    server: server.name.clone(),
                    origin: None,
                    tools: server.tools.clone(),
                    allowances: std::collections::BTreeMap::new(),
                })
                .collect(),
            preview: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
        });
        Project {
            repo,
            log,
            state,
            tiers,
            session_id,
        }
    }

    fn spec(&self) -> SessionSpec {
        SessionSpec {
            session_id: self.session_id.clone(),
            agent_id: "dev-a".to_string(),
            task_id: None,
            purpose: SessionPurpose::Implement,
            system_prompt: "You are a test agent. Do exactly what the user asks, one tool call \
                            at a time, and nothing else."
                .to_string(),
            model: "claude-haiku-4-5-20251001".to_string(),
            effort: Effort::Low,
            farik_tools: vec![FARIK_TOOL.to_string()],
            builtin_tools: allowed_builtins(&self.tiers),
            mcp_servers: Vec::new(),
            disallowed_tools: Vec::new(),
            cwd: self.repo.path.clone(),
            limits: DEFAULT_SESSION_LIMITS,
            skills: Vec::new(),
            initial_prompt: format!(
                "First, use the Read tool to read note.txt. Second, use the Write tool to create \
                 out.txt containing the word yes. Third, use the Grep tool to search this \
                 directory for the text {SECRET}. Fourth, call the {FARIK_TOOL} tool. Then \
                 reply with what note.txt says, whether the write worked, what the search \
                 found, and what the board holds."
            ),
        }
    }
}

/// What one session reported, the program's own copy of its output, and its debug log.
struct Run {
    events: Vec<SessionEvent>,
    stream: String,
    debug: String,
}

impl Run {
    /// What the debug log says of Farik's MCP server and of listing tools, with anything shaped like a credential
    /// cut out.
    fn farik_mcp_lines(&self) -> String {
        self.debug
            .lines()
            .filter(|line| line.contains("MCP server \"farik\"") || line.contains("tools/list"))
            .map(|line| {
                line.split_whitespace()
                    .map(|word| {
                        if word.contains("sk-ant-") {
                            "[redacted]"
                        } else {
                            word
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Runs one session of `spec` against a served daemon.
fn run(
    project: &Project,
    spec: SessionSpec,
    credential: ClaudeCredential,
    env: &BTreeMap<String, String>,
) -> Run {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = project.repo.path.join(".farik/local");
    let daemon_file = local.join("daemon.json");
    let handle = runtime
        .block_on(serve(
            DaemonConfig {
                port: farik_runtime::daemon::PortChoice::Any,
                daemon_file: Some(daemon_file.clone()),
            },
            Arc::clone(&project.state),
        ))
        .expect("the daemon is up");
    let stdout_copy = local.join("stdout.jsonl");
    let debug_log = local.join("claude-debug.log");
    let config = ClaudeConfig {
        claude_path: a_recording_claude(&local, &find_claude(), &stdout_copy, &debug_log),
        hook_command: PathBuf::from(env!("CARGO_BIN_EXE_farik")),
        daemon_file,
        daemon: handle.info.clone(),
        sessions_dir: local.join("sessions"),
        skills_dir: skills_state(project),
        team_file: project.repo.path.join(".farik/team.yaml"),
        env: BASE_ENV
            .iter()
            .filter_map(|name| {
                env.get(*name)
                    .map(|value| ((*name).to_string(), value.clone()))
            })
            .collect(),
    };
    let adapter = ClaudeAdapter::new(
        std::sync::Arc::new(std::sync::Mutex::new(credential)),
        config,
    )
    .expect("claude is new enough");
    let events = runtime.block_on(async {
        let mut session = adapter.start_session(spec).expect("the session starts");
        let mut events = Vec::new();
        let read_all = async {
            while let Some(event) = session.events().recv().await {
                events.push(event);
            }
        };
        tokio::time::timeout(Duration::from_secs(300), read_all)
            .await
            .expect("the session ends within five minutes");
        events
    });
    let _ = runtime.block_on(handle.shutdown());
    Run {
        events,
        stream: std::fs::read_to_string(&stdout_copy).expect("the stream was kept"),
        debug: std::fs::read_to_string(&debug_log).unwrap_or_default(),
    }
}

/// The tools the program's `init` line says the session has, built-in and MCP.
fn init_tools(stream: &str) -> Vec<String> {
    let init: Value = stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["type"] == "system" && line["subtype"] == "init")
        .expect("an init line");
    let mut tools: Vec<String> = init["tools"]
        .as_array()
        .expect("the init line lists tools")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    tools.sort();
    tools
}

#[test]
fn live_session_reads_is_denied_and_completes() {
    if std::env::var("FARIK_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set FARIK_LIVE_TESTS=1 to run a live Claude Code session");
        return;
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let credential = credential_from_env(&env)
        .expect("FARIK_LIVE_TESTS=1 needs ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN");
    let project = Project::new();
    let spec = project.spec();
    let builtin_tools = spec.builtin_tools.clone();
    let run = run(&project, spec, credential, &env);
    let events = &run.events;
    let init = init_tools(&run.stream);
    assert!(
        init.iter().any(|tool| tool == FARIK_TOOL_IN_CLAUDE),
        "the session has no {FARIK_TOOL_IN_CLAUDE}: {init:?}\n{}",
        run.farik_mcp_lines()
    );

    match events.last() {
        Some(SessionEvent::Ended {
            reason: EndReason::Completed,
            ..
        }) => {}
        other => panic!("expected a completed session, got {other:?} after {events:?}"),
    }
    let has = |matches: &dyn Fn(&SessionEvent) -> bool| events.iter().any(matches);
    assert!(
        has(&|event| matches!(event, SessionEvent::UsageReported(_))),
        "{events:?}"
    );
    assert!(
        has(
            &|event| matches!(event, SessionEvent::ToolReturned { tool, output }
            if tool == "Read" && output.contains("hello live"))
        ),
        "{events:?}"
    );
    assert!(
        has(&|event| matches!(event, SessionEvent::ToolDenied { tool, .. } if tool == "Write")),
        "{events:?}"
    );
    assert!(
        has(&|event| matches!(event, SessionEvent::ToolCalled { tool, .. } if tool == "Grep")),
        "{events:?}"
    );
    assert!(
        !has(
            &|event| matches!(event, SessionEvent::ToolReturned { output, .. }
            if output.contains(SECRET))
        ),
        "the secret reached the session: {events:?}"
    );
    assert!(
        has(
            &|event| matches!(event, SessionEvent::ToolReturned { tool, output }
            if tool == FARIK_TOOL_IN_CLAUDE && output.contains("\"tasks\""))
        ),
        "{events:?}"
    );
    assert!(!project.repo.path.join("out.txt").exists());
    let logged = project
        .log
        .read(&EventQuery::default())
        .expect("the log reads");
    let kinds: Vec<EventKind> = logged.iter().map(|event| event.body.kind()).collect();
    assert!(kinds.contains(&EventKind::ToolCalled), "{kinds:?}");
    assert!(kinds.contains(&EventKind::ToolDenied), "{kinds:?}");
    let farik_tool_logged = |kind: EventKind| {
        logged.iter().any(|event| {
            event.body.kind() == kind
                && serde_json::to_value(&event.body)
                    .is_ok_and(|body| body.to_string().contains(FARIK_TOOL_IN_CLAUDE))
        })
    };
    assert!(farik_tool_logged(EventKind::ToolCalled), "{kinds:?}");
    assert!(farik_tool_logged(EventKind::ToolReturned), "{kinds:?}");
    let builtins: Vec<String> = init
        .into_iter()
        .filter(|tool| !tool.starts_with("mcp__"))
        .collect();
    assert_eq!(builtins, builtin_tools);
}

#[test]
#[ignore = "a live Claude Code session needs the founder's credential: FARIK_LIVE_TESTS=1 with \
            ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN, by hand"]
fn a_live_session_calls_a_custom_connector() {
    if std::env::var("FARIK_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set FARIK_LIVE_TESTS=1 to run a live Claude Code session");
        return;
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let credential = credential_from_env(&env)
        .expect("FARIK_LIVE_TESTS=1 needs ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN");
    let project = Project::with_team(|wire, root| {
        let script = root.join(".farik/local/fixture-server.sh");
        std::fs::create_dir_all(root.join(".farik/local")).expect("the folder");
        std::fs::write(&script, FIXTURE_SERVER).expect("the server is written");
        wire["agents"][1]["mcp_servers"] = json!([{
            "name": "fixture", "source": "custom", "transport": "stdio",
            "command": "sh", "args": [script.display().to_string()],
            "credential_keys": ["API_KEY"],
            "tools": { "search": "network", "delete_repo": "denied" }
        }]);
    });
    let mut spec = project.spec();
    spec.mcp_servers = vec![McpServerConfig {
        name: "fixture".to_string(),
        transport: McpTransport::Launched,
        headers: BTreeMap::new(),
    }];
    spec.disallowed_tools = vec!["mcp__fixture__delete_repo".to_string()];
    spec.initial_prompt = "First, call the mcp__fixture__search tool with the query hello. \
                           Second, call the mcp__fixture__delete_repo tool. Then reply with what \
                           the search returned."
        .to_string();
    let run = run(&project, spec, credential, &env);
    let init = init_tools(&run.stream);
    assert!(
        init.iter().any(|tool| tool == "mcp__fixture__search"),
        "the session has no mcp__fixture__search: {init:?}\n{}",
        run.farik_mcp_lines()
    );
    assert!(
        !init.iter().any(|tool| tool == "mcp__fixture__delete_repo"),
        "{init:?}"
    );
    assert!(
        run.events.iter().any(
            |event| matches!(event, SessionEvent::ToolReturned { tool, output }
            if tool == "mcp__fixture__search" && output.contains("fixture-found"))
        ),
        "{:?}",
        run.events
    );
    let logged = project
        .log
        .read(&EventQuery::default())
        .expect("the log reads");
    let called = |tool: &str| {
        logged.iter().any(|event| {
            event.body.kind() == EventKind::ToolCalled
                && serde_json::to_value(&event.body)
                    .is_ok_and(|body| body.to_string().contains(tool))
        })
    };
    assert!(called("mcp__fixture__search"));
    assert!(!called("mcp__fixture__delete_repo"));
    // The key went to the server, and to nothing the session can read.
    assert!(!run.stream.contains(CONNECTOR_KEY));
}

/// An authorization server and a protected MCP server, as the runtime's tests run them.
#[path = "../../runtime/tests/support/oauth_fixture.rs"]
mod oauth_fixture;

#[test]
#[ignore = "a live Claude Code session needs the founder's credential: FARIK_LIVE_TESTS=1 with \
            ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN, by hand"]
fn a_live_session_calls_a_signed_in_connector() {
    if std::env::var("FARIK_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set FARIK_LIVE_TESTS=1 to run a live Claude Code session");
        return;
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let credential = credential_from_env(&env)
        .expect("FARIK_LIVE_TESTS=1 needs ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN");
    // The fixture's server runs on this runtime, kept alive for the whole test.
    let serving = tokio::runtime::Runtime::new().expect("a runtime");
    let fixture = serving.block_on(oauth_fixture::Fixture::start());
    let (access, refresh) = fixture.mint();
    let granted = access.clone();
    let url = fixture.mcp_url.clone();
    let project = Project::with_team_signed_in(
        |wire, _| {
            wire["agents"][1]["mcp_servers"] = json!([{
                "name": "fixture", "source": "custom", "transport": "http",
                "url": url, "oauth": {}, "tools": { "whoami": "network" }
            }]);
        },
        |_| {
            let now = Utc::now();
            Some(OAuthGrant {
                issuer: fixture.origin.clone(),
                resource: fixture.mcp_url.clone(),
                client_id: "client-live".to_string(),
                token_endpoint: format!("{}/token", fixture.origin),
                revocation_endpoint: None,
                access_token: Secret::new(granted.clone()),
                refresh_token: Some(Secret::new(refresh.clone())),
                issued_at: now,
                expires_at: Some(now + chrono::Duration::hours(2)),
                scopes: Vec::new(),
                lapsed: false,
            })
        },
    );
    let mut spec = project.spec();
    spec.mcp_servers = vec![McpServerConfig {
        name: "fixture".to_string(),
        transport: McpTransport::Helped {
            url: fixture.mcp_url.clone(),
        },
        headers: BTreeMap::new(),
    }];
    spec.initial_prompt =
        "Call the mcp__fixture__whoami tool, then reply with what it returned.".to_string();
    let run = run(&project, spec, credential, &env);
    let init = init_tools(&run.stream);
    assert!(
        init.iter().any(|tool| tool == "mcp__fixture__whoami"),
        "the session has no mcp__fixture__whoami: {init:?}\n{}",
        run.farik_mcp_lines()
    );
    assert!(
        run.events.iter().any(
            |event| matches!(event, SessionEvent::ToolReturned { tool, output }
            if tool == "mcp__fixture__whoami" && output.contains("whoami-ok"))
        ),
        "{:?}",
        run.events
    );
    // The server was sent the access token, by the headers helper and by nothing else.
    let sent: Vec<String> = fixture
        .requests("/mcp")
        .into_iter()
        .filter_map(|request| request.authorization)
        .collect();
    assert!(
        sent.iter()
            .any(|header| *header == format!("Bearer {access}")),
        "{sent:?}"
    );
    // And it reached nothing the session can read.
    assert!(!run.stream.contains(&access));
}

/// Where the plugin folders go: outside the repository, as the shipped layout has them
/// (`<state>/skills/<project id>`). Inside `.farik/local` Claude Code's own deny rule refuses the
/// read of a skill's reference.
fn skills_state(project: &Project) -> PathBuf {
    PathBuf::from(format!("{}-state", project.repo.path.display()))
        .join("skills")
        .join("test-project")
}

/// What the program's `init` line lists as its skills.
fn init_skills(stream: &str) -> Vec<String> {
    let init: Value = stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["type"] == "system" && line["subtype"] == "init")
        .expect("an init line");
    init["skills"]
        .as_array()
        .map(|skills| {
            skills
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The marker the skill's body holds and the prompt must not.
const SKILL_BODY_MARKER: &str = "FIXTURE-SKILL-BODY-MARKER";

#[test]
#[ignore = "a live Claude Code session needs the founder's credential: FARIK_LIVE_TESTS=1 with \
            ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN, by hand; the credential-free equivalent \
            is the readiness review's fake-API probe (ADR 0034)"]
fn a_live_session_loads_a_skill_on_use() {
    if std::env::var("FARIK_LIVE_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set FARIK_LIVE_TESTS=1 to run a live Claude Code session");
        return;
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let credential = credential_from_env(&env)
        .expect("FARIK_LIVE_TESTS=1 needs ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN");
    let project = Project::new();
    let local = project.repo.path.join(".farik/local");
    // A chat session, which works in the project's root: the harder case, where Claude Code's own
    // deny rule for `.farik/local/**` applies, and the skills' folder lies outside it.
    let skills_root = skills_state(&project)
        .join(&project.session_id)
        .join("skills");
    project.state.register_session(SessionRegistration {
        session_id: project.session_id.clone(),
        agent_id: "dev-a".to_string(),
        task_id: None,
        cwd: project.repo.path.clone(),
        executor: None,
        limits: DEFAULT_SESSION_LIMITS,
        farik_tools: Vec::new(),
        tiers: project.tiers.iter().copied().collect(),
        connectors: Vec::new(),
        preview: None,
        purpose: SessionPurpose::Chat,
        in_reply_to: None,
        thread: None,
        skills: vec!["fixture-skill".to_string()],
        skills_root: Some(skills_root),
    });
    let mut spec = project.spec();
    spec.purpose = SessionPurpose::Chat;
    spec.farik_tools = Vec::new();
    spec.skills = vec![SessionSkill {
        name: "fixture-skill".to_string(),
        files: BTreeMap::from([
            (
                "SKILL.md".to_string(),
                format!(
                    "---\nname: fixture-skill\ndescription: Use when asked to use the fixture \
                     skill.\n---\n{SKILL_BODY_MARKER}: read references/note.md, in this skill's \
                     folder, and report the one word in it.\n"
                ),
            ),
            ("references/note.md".to_string(), "pelican\n".to_string()),
        ]),
    }];
    spec.initial_prompt = "Use the fixture-skill skill, read the note it points to, say the one \
                           word the note holds, and end."
        .to_string();
    let run = run(&project, spec.clone(), credential, &env);
    assert!(
        init_skills(&run.stream)
            .iter()
            .any(|skill| skill == "farik:fixture-skill"),
        "{}",
        run.stream
    );
    let logged = project
        .log
        .read(&EventQuery::default())
        .expect("the log reads");
    let called = |tool: &str, needle: &str| {
        logged.iter().any(|event| {
            event.body.kind() == EventKind::ToolCalled
                && serde_json::to_value(&event.body)
                    .is_ok_and(|body| body["tool"] == tool && body.to_string().contains(needle))
        })
    };
    assert!(called("Skill", "farik:fixture-skill"), "{logged:?}");
    assert!(called("Read", "references/note.md"), "{logged:?}");
    let answer = run
        .stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["type"] == "result")
        .and_then(|line| line["result"].as_str().map(str::to_string))
        .expect("a result line");
    assert!(answer.contains("pelican"), "the read succeeded: {answer}");
    let prompt = std::fs::read_to_string(
        local
            .join("sessions")
            .join(&spec.session_id)
            .join("system-prompt.md"),
    )
    .expect("the prompt is kept");
    assert!(!prompt.contains(SKILL_BODY_MARKER), "{prompt}");
}
