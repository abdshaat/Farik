//! A daemon on a project the tools can be called on: the team of `pm`, `dev-a`, and `dev-b`, a
//! task FRK-1 of `dev-a`'s whose allowed paths are `src/**`, and `dev-a`'s session registered with
//! the task's worktree as its directory. The recorded hook inputs are read with `/workspace`
//! replaced by that worktree.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use farik_core::budget::{DEFAULT_SESSION_LIMITS, SessionLimits};
use farik_protocol::event::{EventKind, FarikEvent};
use farik_store::git::fixtures::TempRepo;
use farik_store::open_event_log;
use serde_json::{Value, json};

use super::{DaemonState, HookRequest, SessionRegistration};
use crate::tools::ToolDeps;
use crate::tools::fixtures::{TestProject, a_team_of_three, at};

/// The session of `dev-a`, on FRK-1.
pub(crate) const DEV_SESSION: &str = "3f1c2a9e-8b7d-4e6f-9a01-2b3c4d5e6f70";

/// The recorded `PreToolUse` input of a `Read`.
pub(crate) const PRE_READ: &str = include_str!("fixtures/pre_tool_use_read.json");
/// The recorded `PreToolUse` input of a `Write`.
pub(crate) const PRE_WRITE: &str = include_str!("fixtures/pre_tool_use_write.json");
/// The recorded `PostToolUse` input of a `Read`.
pub(crate) const POST_READ: &str = include_str!("fixtures/post_tool_use_read.json");

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
        let worktree = project.repo.path.join(".farik/local/worktrees/FRK-1");
        project
            .repo
            .adapter()
            .create_worktree(&worktree, "farik/FRK-1", "main")
            .expect("the worktree is made");
        let state = Arc::new(DaemonState::new(Arc::clone(&project.deps)));
        let daemon = Self {
            project,
            worktree,
            state,
        };
        daemon.register(DEV_SESSION, "dev-a", Some("FRK-1"), DEFAULT_SESSION_LIMITS);
        daemon
    }

    /// Registers a session of `agent` in the worktree.
    pub(crate) fn register(
        &self,
        session_id: &str,
        agent: &str,
        task: Option<&str>,
        limits: SessionLimits,
    ) {
        self.state.register_session(SessionRegistration {
            session_id: session_id.to_string(),
            agent_id: agent.to_string(),
            task_id: task.map(|task| task.parse().expect("a task id")),
            cwd: self.worktree.clone(),
            executor: None,
            limits,
        });
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
        let path = self.project.repo.path.join(".farik/local/read-only.db");
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
        }));
        state.register_session(SessionRegistration {
            session_id: DEV_SESSION.to_string(),
            agent_id: "dev-a".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            cwd: self.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
        });
        state
    }

    /// Every event of this kind in the log, oldest first.
    pub(crate) fn events(&self, kind: EventKind) -> Vec<FarikEvent> {
        self.project.events(&[kind])
    }
}
