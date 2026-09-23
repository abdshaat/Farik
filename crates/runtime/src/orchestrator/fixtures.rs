//! The orchestrator's test harness: a repository with `.farik/` initialised, a team of a Product
//! Manager `pm` and two Software Developers `dev-a` and `dev-b` with a WIP limit of one, a daemon
//! that is not served, and a runner that answers a replayed session's Farik tool calls the way the
//! daemon's MCP server would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use farik_core::contract::TaskId;
use farik_core::pricing::Usage;
use farik_protocol::clock::SequentialIds;
use farik_protocol::event::{EventKind, FarikEvent, NewEvent, event_from_value};
use farik_store::TaskProjection;
use serde_json::{Value, json};
use tokio::sync::mpsc::{Receiver, Sender, channel};

use super::{Orchestrator, OrchestratorDeps};
use crate::daemon::{DaemonState, HookRequest, decide_pre_tool_use};
use crate::exec::{ExecError, ExecResult, Executor};
use crate::recorded::{RecordedAdapter, ToolRunner, Transcript};
use crate::sandbox::host::HostSandboxFactory;
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};
use crate::session::{
    EndReason, RuntimeAdapter, RuntimeError, SessionEvent, SessionHandle, SessionSpec,
};
use crate::tools::call_tool;
use crate::tools::fixtures::{TestProject, a_team_of_three, at};

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

    /// Files `task` `ready` with `change`, and moves it through `in_progress` to `verifying`, held
    /// by `dev-a` and reviewed by `dev-b`, with its worktree made; `done.txt` committed on its
    /// branch when `commits_done`, and `dev-a`'s completion note written when `notes`.
    pub(crate) fn verifying_with(
        &self,
        task: &str,
        commits_done: bool,
        notes: bool,
        change: impl FnOnce(&mut Value),
    ) {
        self.file(task, "ready", change);
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b" });
        self.project.moved(task, "ready", "assigned", &people);
        self.project.moved(task, "assigned", "in_progress", &people);
        let worktree = self.worktree(task);
        let git = &self.project.deps.git;
        git.create_worktree(&worktree, &format!("farik/{task}"), "main")
            .expect("the task's worktree is made");
        if commits_done {
            std::fs::write(worktree.join("done.txt"), "").expect("done.txt is written");
            git.commit(&worktree, "Add done.txt", &["done.txt".to_string()])
                .expect("done.txt is committed");
        }
        if notes {
            self.project.record(
                task,
                "note.written",
                &json!({ "kind": "completion", "text": "Added done.txt; nothing left out.", "written_by": "dev-a" }),
            );
        }
        let mut body = people;
        body["actor"] = json!("assignee");
        body["requested_by"] = json!("dev-a");
        self.project.moved(task, "in_progress", "verifying", &body);
    }

    /// `verifying_with` `done.txt` committed, the note written, and no change.
    pub(crate) fn verifying(&self, task: &str) {
        self.verifying_with(task, true, true, |_| {});
    }

    /// `verifying`, then moved to `accepted` by the Product Manager, its worktree left in place.
    pub(crate) fn accepted_with_worktree(&self, task: &str) {
        self.verifying(task);
        self.project.moved(
            task,
            "verifying",
            "accepted",
            &json!({ "actor": "product_manager", "requested_by": "pm", "assignee": "dev-a", "reviewer": "dev-b" }),
        );
    }

    /// `accepted_with_worktree`, with the worktree removed, so that no cleanup is left to do.
    pub(crate) fn accepted(&self, task: &str) {
        self.accepted_with_worktree(task);
        self.project
            .deps
            .git
            .remove_worktree(&self.worktree(task))
            .expect("the task's worktree is removed");
    }

    /// Files `task` and moves it through `in_progress` to `blocked`, held by `assignee`.
    pub(crate) fn blocked(&self, task: &str, assignee: &str, reviewer: &str) {
        self.blocked_hours_ago(task, assignee, reviewer, 0);
    }

    /// `blocked`, the block recorded `hours` before the clock's now.
    pub(crate) fn blocked_hours_ago(&self, task: &str, assignee: &str, reviewer: &str, hours: i64) {
        self.assigned(task, assignee, reviewer);
        let people = json!({ "assignee": assignee, "reviewer": reviewer });
        self.project.moved(task, "assigned", "in_progress", &people);
        let mut body = people;
        body["blocker"] = json!({ "description": "the API is down", "needed": "the API" });
        self.project.moved_at(
            at() - chrono::Duration::hours(hours),
            task,
            "in_progress",
            "blocked",
            &body,
        );
    }

    /// Files `task` and moves it through `in_progress` and `verifying` to `rejected` at
    /// `iteration`, held by `dev-a` and reviewed by `dev-b`, C1 failed for `reasons`.
    pub(crate) fn rejected(&self, task: &str, iteration: u32, reasons: &str) {
        self.in_progress(task, "dev-a", "dev-b");
        let people = json!({ "assignee": "dev-a", "reviewer": "dev-b", "iteration": iteration });
        self.project
            .moved(task, "in_progress", "verifying", &people);
        let mut body = people;
        body["actor"] = json!("reviewer");
        body["requested_by"] = json!("dev-b");
        body["rejection"] = json!({ "failed_criterion_ids": ["C1"], "reasons": reasons });
        self.project.moved(task, "verifying", "rejected", &body);
    }

    /// A `cost.recorded` of `usd` dollars by `dev-a` in session `session`, against `task` when one
    /// is named, today.
    pub(crate) fn spent(&self, task: Option<&str>, session: &str, usd: f64) {
        let mut wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "agent_id": "dev-a",
            "session_id": session,
            "kind": "cost.recorded",
            "body": {
                "purpose": "implement",
                "model_id": "claude-opus-5",
                "usage": {
                    "input_tokens": 1000,
                    "output_tokens": 100,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0
                },
                "cost_usd": usd
            },
        });
        if let Some(task) = task {
            wire["task_id"] = json!(task);
        }
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        let deps = &self.project.deps;
        let appended = deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        deps.projections.apply(&appended).expect("projects");
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

/// An adapter that says, for each session it starts, whether the daemon registered it with an
/// executor, and starts it with the adapter it wraps.
pub(crate) struct ExecutorWitness {
    inner: Arc<dyn RuntimeAdapter>,
    daemon: Arc<DaemonState>,
    seen: Mutex<Vec<bool>>,
}

impl ExecutorWitness {
    /// A witness of `inner`'s sessions as `daemon` registered them.
    pub(crate) fn new(inner: Arc<dyn RuntimeAdapter>, daemon: Arc<DaemonState>) -> Self {
        Self {
            inner,
            daemon,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// For each session started, in order, whether its registration had an executor.
    pub(crate) fn had_executor(&self) -> Vec<bool> {
        self.seen.lock().expect("no test panics holding it").clone()
    }
}

impl RuntimeAdapter for ExecutorWitness {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let executor = self
            .daemon
            .tool_context(&spec.session_id)
            .expect("the session is registered before it starts")
            .executor
            .is_some();
        self.seen
            .lock()
            .expect("no test panics holding it")
            .push(executor);
        self.inner.start_session(spec)
    }

    fn resume(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        self.inner.resume(session_id, prompt)
    }
}

/// A host sandbox factory that records, for each task, whether each sandbox it made for it had
/// the network on, and how many times its sandboxes were removed.
#[derive(Default)]
pub(crate) struct CountingSandboxFactory {
    created: Mutex<BTreeMap<String, Vec<bool>>>,
    removed: Mutex<BTreeMap<String, u32>>,
}

impl CountingSandboxFactory {
    /// How many times `remove` was called for `task`.
    pub(crate) fn removed(&self, task: &str) -> u32 {
        self.removed
            .lock()
            .expect("no test panics holding it")
            .get(task)
            .copied()
            .unwrap_or_default()
    }

    /// How many sandboxes `create` made for `task`.
    pub(crate) fn created(&self, task: &str) -> u32 {
        u32::try_from(self.networks(task).len()).expect("a test makes few sandboxes")
    }

    /// For each sandbox `create` made for `task`, in order, whether its network was on.
    pub(crate) fn networks(&self, task: &str) -> Vec<bool> {
        self.created
            .lock()
            .expect("no test panics holding it")
            .get(task)
            .cloned()
            .unwrap_or_default()
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
        self.created
            .lock()
            .expect("no test panics holding it")
            .entry(task_id.as_str().to_string())
            .or_default()
            .push(network);
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

    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError> {
        *self
            .removed
            .lock()
            .expect("no test panics holding it")
            .entry(task_id.as_str().to_string())
            .or_default() += 1;
        HostSandboxFactory.remove(project_id, task_id)
    }
}

/// A sandbox factory whose first `broken` sandboxes answer every command with `error`, and whose
/// others are host sandboxes; it counts, as `CountingSandboxFactory` does, what it made.
pub(crate) struct BrokenSandboxFactory {
    error: ExecError,
    broken: AtomicU32,
    counting: CountingSandboxFactory,
}

impl BrokenSandboxFactory {
    /// A factory whose first `broken` sandboxes fail every command with `error`.
    pub(crate) fn new(error: ExecError, broken: u32) -> Self {
        Self {
            error,
            broken: AtomicU32::new(broken),
            counting: CountingSandboxFactory::default(),
        }
    }

    /// How many sandboxes `create` made for `task`.
    pub(crate) fn created(&self, task: &str) -> u32 {
        self.counting.created(task)
    }
}

impl SandboxFactory for BrokenSandboxFactory {
    fn create(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        let sandbox = self
            .counting
            .create(project_id, task_id, worktree, network)?;
        let broken = self
            .broken
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if broken {
            return Ok(Box::new(BrokenSandbox(self.error.clone())));
        }
        Ok(sandbox)
    }

    fn create_base(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        HostSandboxFactory.create_base(project_id, task_id, worktree)
    }

    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError> {
        self.counting.remove(project_id, task_id)
    }
}

/// A sandbox that answers every command with its error.
struct BrokenSandbox(ExecError);

impl Executor for BrokenSandbox {
    fn run(
        &self,
        _command: &str,
        _cwd: &str,
        _timeout: Duration,
        _env: &BTreeMap<String, String>,
    ) -> Result<ExecResult, ExecError> {
        Err(self.0.clone())
    }
}

impl Sandbox for BrokenSandbox {
    fn discard(self: Box<Self>) -> Result<(), SandboxError> {
        Ok(())
    }
}

/// An adapter whose every session reports `usage` at once and then either ends `completed` at
/// once or waits for `abort` and ends `aborted`, or, when its abort fails, waits for ever: the
/// shapes a recorded transcript, which reports usage only on its last line, cannot show.
pub(crate) struct UsageThenWaitAdapter {
    usage: Usage,
    completes: bool,
    abort_fails: bool,
    started: Mutex<Vec<SessionSpec>>,
    aborts: Arc<AtomicU32>,
}

impl UsageThenWaitAdapter {
    /// Sessions that report `usage` and wait to be aborted.
    pub(crate) fn waiting(usage: Usage) -> Self {
        Self::new(usage, false, false)
    }

    /// Sessions that report `usage` and end `completed`.
    pub(crate) fn completing(usage: Usage) -> Self {
        Self::new(usage, true, false)
    }

    /// Sessions that report `usage` and wait, and whose every abort is counted and fails.
    pub(crate) fn failing_to_abort(usage: Usage) -> Self {
        Self::new(usage, false, true)
    }

    fn new(usage: Usage, completes: bool, abort_fails: bool) -> Self {
        Self {
            usage,
            completes,
            abort_fails,
            started: Mutex::new(Vec::new()),
            aborts: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Every spec a session was started with, in order.
    pub(crate) fn started(&self) -> Vec<SessionSpec> {
        self.started
            .lock()
            .expect("no test panics holding it")
            .clone()
    }

    /// How many times a session of this adapter was aborted.
    pub(crate) fn aborts(&self) -> u32 {
        self.aborts.load(Ordering::SeqCst)
    }
}

impl RuntimeAdapter for UsageThenWaitAdapter {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let (sender, receiver) = channel(4);
        sender
            .try_send(SessionEvent::UsageReported(self.usage))
            .expect("the channel has room");
        let sender = if self.completes {
            sender
                .try_send(SessionEvent::Ended {
                    reason: EndReason::Completed,
                    detail: "done".to_string(),
                })
                .expect("the channel has room");
            None
        } else {
            Some(sender)
        };
        let handle = WaitingSession {
            session_id: spec.session_id.clone(),
            receiver,
            sender: Mutex::new(sender),
            aborts: Arc::clone(&self.aborts),
            abort_fails: self.abort_fails,
        };
        self.started
            .lock()
            .expect("no test panics holding it")
            .push(spec);
        Ok(Box::new(handle))
    }

    fn resume(
        &self,
        _session_id: &str,
        _prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        Err(RuntimeError::Spawn {
            detail: "this adapter does not resume".to_string(),
        })
    }
}

/// A session of `UsageThenWaitAdapter`.
struct WaitingSession {
    session_id: String,
    receiver: Receiver<SessionEvent>,
    sender: Mutex<Option<Sender<SessionEvent>>>,
    aborts: Arc<AtomicU32>,
    abort_fails: bool,
}

impl SessionHandle for WaitingSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn events(&mut self) -> &mut Receiver<SessionEvent> {
        &mut self.receiver
    }

    fn send(&self, _text: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn abort(&self) -> Result<(), RuntimeError> {
        self.aborts.fetch_add(1, Ordering::SeqCst);
        if self.abort_fails {
            return Err(RuntimeError::Spawn {
                detail: "the session would not stop".to_string(),
            });
        }
        if let Some(sender) = self
            .sender
            .lock()
            .expect("no test panics holding it")
            .take()
        {
            let _ = sender.try_send(SessionEvent::Ended {
                reason: EndReason::Aborted,
                detail: "aborted".to_string(),
            });
        }
        Ok(())
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
