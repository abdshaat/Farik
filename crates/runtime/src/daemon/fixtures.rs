//! A daemon on a project the tools can be called on: the team of `pm`, `dev-a`, and `dev-b`, a
//! task FRK-1 of `dev-a`'s whose allowed paths are `src/**`, and `dev-a`'s session registered with
//! the task's worktree as its directory. The recorded hook inputs are read with `/workspace`
//! replaced by that worktree.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use catervas_core::budget::{DEFAULT_SESSION_LIMITS, SessionLimits};
use catervas_protocol::clock::Clock;
use catervas_protocol::event::{CatervasEvent, EventKind};
use catervas_store::git::fixtures::TempRepo;
use catervas_store::open_event_log;
use serde_json::{Value, json};

use super::{DaemonState, HookRequest, SessionRegistration};
use crate::session::SessionPurpose;
use crate::tools::fixtures::{TestProject, a_team_of_three, at, tiers_of};
use crate::tools::{ToolDeps, tool_descriptors};

/// The session of `dev-a`, on FRK-1.
pub(crate) const DEV_SESSION: &str = "3f1c2a9e-8b7d-4e6f-9a01-2b3c4d5e6f70";

/// The recorded `PreToolUse` input of a `Read`.
pub(crate) const PRE_READ: &str = include_str!("fixtures/pre_tool_use_read.json");
/// The recorded `PreToolUse` input of a `Write`.
pub(crate) const PRE_WRITE: &str = include_str!("fixtures/pre_tool_use_write.json");
/// The recorded `PostToolUse` input of a `Read`.
pub(crate) const POST_READ: &str = include_str!("fixtures/post_tool_use_read.json");

/// The name of every Catervas tool.
pub(crate) fn every_catervas_tool() -> Vec<&'static str> {
    tool_descriptors().iter().map(|tool| tool.name).collect()
}

/// A project, the worktree of its task FRK-1, and a daemon with `dev-a`'s session on it.
pub(crate) struct TestDaemon {
    pub(crate) project: TestProject,
    pub(crate) worktree: PathBuf,
    pub(crate) state: Arc<DaemonState>,
}

impl TestDaemon {
    /// The daemon, with `before` run on the repository before the worktree is made from it.
    pub(crate) fn new(name: &str, before: impl FnOnce(&TempRepo)) -> Self {
        let project = TestProject::new(name, &a_team_of_three(|_| {}));
        project.filed_with("FRK-1", "in_progress", "task", None, |wire| {
            wire["allowed_paths"] = json!(["src/**"]);
            wire["assignee"] = json!("dev-a");
        });
        before(&project.repo);
        let worktree = project.repo.path.join(".catervas/local/worktrees/FRK-1");
        project
            .repo
            .adapter()
            .create_worktree(&worktree, &project.branch("FRK-1"), "main")
            .expect("the worktree is made");
        let state = Arc::new(DaemonState::new(Arc::clone(&project.deps)));
        // The user's state folder, beside the repository and outside it, as `~/.config/catervas` is.
        state.set_state_dir(std::path::PathBuf::from(format!(
            "{}-state",
            project.repo.path.display()
        )));
        let daemon = Self {
            project,
            worktree,
            state,
        };
        daemon.register(DEV_SESSION, "dev-a", Some("FRK-1"), DEFAULT_SESSION_LIMITS);
        daemon
    }

    /// The same daemon on a clock that sleeps `delay` each time it is read, which every append
    /// does before it writes: a race between a check and the write after it falls inside the
    /// sleep, where a test can see it. Only `DEV_SESSION` is registered on it afresh.
    pub(crate) fn slowed(mut self, delay: std::time::Duration) -> Self {
        let state = Arc::new(DaemonState::new(slowed_deps(&self.project, delay)));
        state.set_state_dir(std::path::PathBuf::from(format!(
            "{}-state",
            self.project.repo.path.display()
        )));
        self.state = state;
        self.register(DEV_SESSION, "dev-a", Some("FRK-1"), DEFAULT_SESSION_LIMITS);
        self
    }

    /// The same daemon on a clock that says it is `now`. Only `DEV_SESSION` is registered on it
    /// afresh.
    pub(crate) fn on_the_clock(mut self, now: chrono::DateTime<chrono::Utc>) -> Self {
        let deps = &self.project.deps;
        let state = Arc::new(DaemonState::new(Arc::new(ToolDeps {
            log: Arc::clone(&deps.log),
            projections: Arc::clone(&deps.projections),
            files: Arc::clone(&deps.files),
            transitions: Arc::clone(&deps.transitions),
            git: self.project.repo.adapter(),
            clock: Arc::new(catervas_protocol::clock::FixedClock::new(now)),
            ids: deps.ids.clone(),
            kits: Arc::clone(&deps.kits),
        })));
        state.set_state_dir(std::path::PathBuf::from(format!(
            "{}-state",
            self.project.repo.path.display()
        )));
        self.state = state;
        self.register(DEV_SESSION, "dev-a", Some("FRK-1"), DEFAULT_SESSION_LIMITS);
        self
    }

    /// Registers a session of `agent` in the worktree, given every Catervas tool, so that its tiers
    /// alone decide which it may call.
    pub(crate) fn register(
        &self,
        session_id: &str,
        agent: &str,
        task: Option<&str>,
        limits: SessionLimits,
    ) {
        self.register_with_tools(session_id, agent, task, limits, &every_catervas_tool());
    }

    /// Registers a session of `agent` in the worktree, given only the Catervas tools `catervas_tools`
    /// names.
    pub(crate) fn register_with_tools(
        &self,
        session_id: &str,
        agent: &str,
        task: Option<&str>,
        limits: SessionLimits,
        catervas_tools: &[&str],
    ) {
        self.state.register_session(SessionRegistration {
            session_id: session_id.to_string(),
            web: self.web_of(agent),
            agent_id: agent.to_string(),
            task_id: task.map(|task| task.parse().expect("a task id")),
            cwd: self.worktree.clone(),
            executor: None,
            limits,
            catervas_tools: catervas_tools.iter().map(ToString::to_string).collect(),
            tiers: tiers_of(&self.project.deps, agent),
            connectors: Vec::new(),
            preview: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
        });
    }

    /// How far `agent`'s web reading reaches, from its role as the team file says now: what a
    /// session of it is registered with. An agent the team does not have is open, as no one asks.
    pub(crate) fn web_of(&self, agent: &str) -> catervas_core::governor::sites::WebAccess {
        use catervas_core::contract::Role;
        use catervas_core::governor::sites::{WebAccess, web_access};

        self.project
            .deps
            .files
            .read_team()
            .ok()
            .and_then(|team| {
                team.agents
                    .iter()
                    .find(|one| one.id.as_str() == agent)
                    .map(|one| web_access(Role::from(one.role)))
            })
            .unwrap_or(WebAccess::Open)
    }

    /// A recorded hook input with `/workspace` made the worktree.
    pub(crate) fn recorded(&self, fixture: &str) -> Value {
        let text = fixture.replace("/workspace", &self.worktree.display().to_string());
        serde_json::from_str(&text).expect("a recorded hook input is JSON")
    }

    /// The recorded `Read` input, for `session_id`, calling `tool` with `input`.
    pub(crate) fn call(&self, session_id: &str, tool: &str, input: &Value) -> HookRequest {
        let mut wire = self.recorded(PRE_READ);
        wire["session_id"] = json!(session_id);
        wire["tool_name"] = json!(tool);
        wire["tool_input"] = input.clone();
        serde_json::from_value(wire).expect("the hook input reads")
    }

    /// `dev-a`'s session calling `tool` with `input`.
    pub(crate) fn dev_call(&self, tool: &str, input: &Value) -> HookRequest {
        self.call(DEV_SESSION, tool, input)
    }

    /// A path inside the worktree, as a string.
    pub(crate) fn inside(&self, relative: &str) -> String {
        self.worktree.join(relative).display().to_string()
    }

    /// A daemon on the same project and sessions' worktree whose log is a file made read-only
    /// after it was opened and closed, so that every append is refused.
    pub(crate) fn with_a_log_that_refuses(&self) -> DaemonState {
        let path = self.project.repo.path.join(".catervas/local/read-only.db");
        drop(open_event_log(&path, at()).expect("the log is made"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444))
            .expect("the log is made read-only");
        let log = Arc::new(open_event_log(&path, at()).expect("a read-only log opens"));
        let deps = &self.project.deps;
        let state = DaemonState::new(Arc::new(ToolDeps {
            log,
            projections: Arc::clone(&deps.projections),
            files: Arc::clone(&deps.files),
            transitions: Arc::clone(&deps.transitions),
            git: self.project.repo.adapter(),
            clock: Arc::clone(&deps.clock),
            ids: deps.ids.clone(),
            kits: Arc::clone(&deps.kits),
        }));
        state.register_session(SessionRegistration {
            session_id: DEV_SESSION.to_string(),
            web: catervas_core::governor::sites::WebAccess::Open,
            agent_id: "dev-a".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            cwd: self.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            catervas_tools: every_catervas_tool()
                .iter()
                .map(ToString::to_string)
                .collect(),
            tiers: tiers_of(deps, "dev-a"),
            connectors: Vec::new(),
            preview: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
        });
        state
    }

    /// A daemon on the same project whose log is a file that has lost its table of events after
    /// it was opened, so that every read of it is refused, with `proc`'s session registered on it
    /// as `session-proc`, about FRK-1.
    pub(crate) fn with_a_log_that_cannot_be_read(&self) -> DaemonState {
        let path = self.project.repo.path.join(".catervas/local/unreadable.db");
        let log = Arc::new(open_event_log(&path, at()).expect("the log is made"));
        rusqlite::Connection::open(&path)
            .expect("the file opens")
            .execute_batch("DROP TABLE events")
            .expect("the table is dropped");
        let deps = &self.project.deps;
        let state = DaemonState::new(Arc::new(ToolDeps {
            projections: Arc::clone(&deps.projections),
            log,
            files: Arc::clone(&deps.files),
            transitions: Arc::clone(&deps.transitions),
            git: self.project.repo.adapter(),
            clock: Arc::clone(&deps.clock),
            ids: deps.ids.clone(),
            kits: Arc::clone(&deps.kits),
        }));
        state.register_session(SessionRegistration {
            session_id: "session-proc".to_string(),
            web: self.web_of("proc"),
            agent_id: "proc".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            cwd: self.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            catervas_tools: every_catervas_tool()
                .iter()
                .map(ToString::to_string)
                .collect(),
            tiers: tiers_of(deps, "proc"),
            connectors: vec![catervas_core::governor::permissions::SessionConnector {
                server: "github".to_string(),
                origin: None,
                tools: [(
                    "search_issues".to_string(),
                    catervas_core::governor::permissions::ConnectorTag::Network,
                )]
                .into(),
                allowances: std::collections::BTreeMap::new(),
                plan_tools: std::collections::BTreeSet::new(),
            }],
            preview: None,
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
        });
        state
    }

    /// Every event of this kind in the log, oldest first.
    pub(crate) fn events(&self, kind: EventKind) -> Vec<CatervasEvent> {
        self.project.events(&[kind])
    }
}

/// `project`'s tool dependencies on a clock that sleeps `delay` each time it is read, which every
/// append does before it writes (see `TestDaemon::slowed`).
pub(crate) fn slowed_deps(project: &TestProject, delay: std::time::Duration) -> Arc<ToolDeps> {
    struct Slow(Arc<dyn Clock + Send + Sync>, std::time::Duration);
    impl Clock for Slow {
        fn now(&self) -> chrono::DateTime<chrono::Utc> {
            std::thread::sleep(self.1);
            self.0.now()
        }
    }
    let deps = &project.deps;
    Arc::new(ToolDeps {
        log: Arc::clone(&deps.log),
        projections: Arc::clone(&deps.projections),
        files: Arc::clone(&deps.files),
        transitions: Arc::clone(&deps.transitions),
        git: project.repo.adapter(),
        clock: Arc::new(Slow(Arc::clone(&deps.clock), delay)),
        ids: deps.ids.clone(),
        kits: Arc::clone(&deps.kits),
    })
}

/// The reply frame to `method` with `params`, as the web page's socket is answered: for a test
/// outside the daemon that asks what the page asks.
pub(crate) async fn answered(state: &Arc<DaemonState>, method: &str, params: &Value) -> Value {
    let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    super::web::answer(state, &frame.to_string(), &mut None).await
}

/// An executable standing in for Catervas's own program, written for `test`: it runs the stdio
/// fixture server of `tests/fixtures/mcp_server.sh` whatever its arguments are, so that `catervas
/// connector osv` lists that server's tools.
pub(crate) fn own_program_serving_the_fixture(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "catervas-own-program-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder is made");
    let server = dir.join("server.sh");
    std::fs::write(&server, include_str!("../../tests/fixtures/mcp_server.sh"))
        .expect("the script is written");
    let program = dir.join("catervas");
    std::fs::write(
        &program,
        format!("#!/bin/sh\nexec sh '{}'\n", server.display()),
    )
    .expect("the program is written");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("the program is made executable");
    program
}

/// What `team.get` answers of each agent's custom connectors and their states.
pub(crate) fn connector_states(state: &Arc<DaemonState>) -> Value {
    super::gates::tests::query(state, "team.get", &serde_json::json!({}), "teamGetResult")
        ["connectors"]
        .clone()
}
