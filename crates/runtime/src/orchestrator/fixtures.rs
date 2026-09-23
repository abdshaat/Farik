//! The orchestrator's test harness: a repository with `.farik/` initialised, a team of a Product
//! Manager `pm` and two Software Developers `dev-a` and `dev-b` with a WIP limit of one, a daemon
//! that is not served, and a runner that answers a replayed session's Farik tool calls the way the
//! daemon's MCP server would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use farik_core::contract::TaskId;
use farik_protocol::clock::SequentialIds;
use farik_protocol::event::{EventKind, FarikEvent};
use farik_store::TaskProjection;
use serde_json::{Value, json};

use super::{Orchestrator, OrchestratorDeps};
use crate::daemon::{DaemonState, HookRequest, decide_pre_tool_use};
use crate::recorded::{RecordedAdapter, ToolRunner, Transcript};
use crate::sandbox::host::HostSandboxFactory;
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};
use crate::session::RuntimeAdapter;
use crate::tools::call_tool;
use crate::tools::fixtures::{TestProject, a_team_of_three};

/// The prefix Claude Code gives the tools of Farik's own MCP server.
const FARIK_PREFIX: &str = "mcp__farik__";

/// A project the orchestrator runs on, and the daemon its sessions register with.
pub(crate) struct Harness {
    pub(crate) project: TestProject,
    pub(crate) daemon: Arc<DaemonState>,
}

impl Harness {
    /// The project, with `change` applied to the team's wire after the WIP limit is set to one,
    /// and nothing on the board.
    pub(crate) fn new(name: &str, change: impl FnOnce(&mut Value)) -> Self {
        let team = a_team_of_three(|wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(1);
            change(wire);
        });
        let project = TestProject::new(name, &team);
        let daemon = Arc::new(DaemonState::new(Arc::clone(&project.deps)));
        Self { project, daemon }
    }

    /// A recorded adapter playing `transcripts`, whose Farik tool calls this harness's daemon
    /// answers.
    pub(crate) fn recorded(&self, transcripts: Vec<Transcript>) -> Arc<RecordedAdapter> {
        Arc::new(RecordedAdapter::with_tools(
            transcripts,
            tool_runner(Arc::clone(&self.daemon)),
        ))
    }

    /// An orchestrator over this project with `adapter`, host sandboxes, and session ids
    /// `session-1`, `session-2`, and so on.
    pub(crate) fn orchestrator(&self, adapter: Arc<dyn RuntimeAdapter>) -> Orchestrator {
        self.orchestrator_with(adapter, Arc::new(HostSandboxFactory))
    }

    /// An orchestrator over this project with `adapter` and `sandboxes`.
    pub(crate) fn orchestrator_with(
        &self,
        adapter: Arc<dyn RuntimeAdapter>,
        sandboxes: Arc<dyn SandboxFactory>,
    ) -> Orchestrator {
        Orchestrator::new(OrchestratorDeps {
            tools: Arc::clone(&self.project.deps),
            daemon: Arc::clone(&self.daemon),
            adapter,
            sandboxes,
            session_ids: Arc::new(SequentialIds::new()),
        })
    }

    /// Files `task` in `status`: a standalone task of a Software Developer's, reviewed by another,
    /// whose one allowed path is `done.txt` and whose one criterion C1 runs `test -f done.txt`,
    /// with `change` applied to its wire last.
    pub(crate) fn file(&self, task: &str, status: &str, change: impl FnOnce(&mut Value)) {
        self.project.filed_with(task, status, "task", None, |wire| {
            wire["assignee_role"] = json!("software_developer");
            wire["reviewer_role"] = json!("software_developer");
            wire["allowed_paths"] = json!(["done.txt"]);
            wire["exit_criteria"] = json!([{
                "id": "C1",
                "text": "done.txt exists.",
                "satisfies": ["R1"],
                "verification": {
                    "method": "command",
                    "command": "test -f done.txt",
                    "expect": { "exit_code": 0 }
                }
            }]);
            change(wire);
        });
    }

    /// Files `task` `ready`, as `file` does.
    pub(crate) fn ready(&self, task: &str) {
        self.file(task, "ready", |_| {});
    }

    /// Files `task` `ready` and moves it to `assigned` to `assignee`, reviewed by `reviewer`.
    pub(crate) fn assigned(&self, task: &str, assignee: &str, reviewer: &str) {
        self.ready(task);
        self.project.moved(
            task,
            "ready",
            "assigned",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": assignee, "reviewer": reviewer }),
        );
    }

    /// Files `task` and moves it through `assigned` to `in_progress`, held by `assignee` and
    /// reviewed by `reviewer`, with its worktree made on `farik/<task>` from `main`.
    pub(crate) fn in_progress(&self, task: &str, assignee: &str, reviewer: &str) {
        self.assigned(task, assignee, reviewer);
        self.project.moved(
            task,
            "assigned",
            "in_progress",
            &json!({ "actor": "assignee", "requested_by": assignee, "assignee": assignee, "reviewer": reviewer }),
        );
        self.project
            .deps
            .git
            .create_worktree(&self.worktree(task), &format!("farik/{task}"), "main")
            .expect("the task's worktree is made");
    }

    /// Files `task` and moves it through `in_progress` to `blocked`, held by `assignee`.
    pub(crate) fn blocked(&self, task: &str, assignee: &str, reviewer: &str) {
        self.assigned(task, assignee, reviewer);
        let people = json!({ "assignee": assignee, "reviewer": reviewer });
        self.project.moved(task, "assigned", "in_progress", &people);
        let mut body = people;
        body["blocker"] = json!({ "description": "the API is down", "needed": "the API" });
        self.project.moved(task, "in_progress", "blocked", &body);
    }

    /// The task's worktree, `.farik/local/worktrees/<task>`.
    pub(crate) fn worktree(&self, task: &str) -> PathBuf {
        self.project
            .repo
            .path
            .join(".farik/local/worktrees")
            .join(task)
    }

    /// The task's row on the board.
    pub(crate) fn row(&self, task: &str) -> TaskProjection {
        let id: TaskId = task.parse().expect("a task id");
        self.project
            .deps
            .projections
            .task(&id)
            .expect("the board reads")
            .expect("the task is on the board")
    }

    /// Every event of these kinds, oldest first; every event when `kinds` is empty.
    pub(crate) fn events(&self, kinds: &[EventKind]) -> Vec<FarikEvent> {
        self.project.events(kinds)
    }
}

/// A host sandbox factory that counts the sandboxes it made for each task.
#[derive(Default)]
pub(crate) struct CountingSandboxFactory {
    created: Mutex<BTreeMap<String, u32>>,
}

impl CountingSandboxFactory {
    /// How many sandboxes `create` made for `task`.
    pub(crate) fn created(&self, task: &str) -> u32 {
        self.created
            .lock()
            .expect("no test panics holding it")
            .get(task)
            .copied()
            .unwrap_or(0)
    }
}

impl SandboxFactory for CountingSandboxFactory {
    fn create(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        *self
            .created
            .lock()
            .expect("no test panics holding it")
            .entry(task_id.as_str().to_string())
            .or_insert(0) += 1;
        HostSandboxFactory.create(project_id, task_id, worktree, network)
    }

    fn create_base(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        HostSandboxFactory.create_base(project_id, task_id, worktree)
    }
}

/// Answers a replayed Farik tool call as a real session's would be: the `PreToolUse` decision
/// first, a deny answered `{"error": "<reason>"}`; then the tool, called with the session's tool
/// context as the daemon has it at that moment, an error answered `{"error": "<its words>"}`. No
/// `PostToolUse` is recorded.
pub(crate) fn tool_runner(daemon: Arc<DaemonState>) -> ToolRunner {
    Arc::new(move |session_id, tool, input| {
        let daemon = Arc::clone(&daemon);
        Box::pin(async move {
            let request = HookRequest {
                session_id: session_id.clone(),
                cwd: PathBuf::new(),
                hook_event_name: "PreToolUse".to_string(),
                tool_name: tool.clone(),
                tool_input: input.clone(),
                tool_use_id: None,
                tool_response: None,
                duration_ms: None,
            };
            let decision = decide_pre_tool_use(&request, &daemon);
            if !decision.allow {
                return json!({ "error": decision.reason });
            }
            let Some(context) = daemon.tool_context(&session_id) else {
                return json!({ "error": format!("the daemon answers for no session {session_id}") });
            };
            let name = tool.strip_prefix(FARIK_PREFIX).unwrap_or(&tool);
            match call_tool(&context, name, input).await {
                Ok(answer) => answer,
                Err(error) => json!({ "error": error.to_string() }),
            }
        })
    })
}
