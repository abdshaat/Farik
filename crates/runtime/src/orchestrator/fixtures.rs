//! The orchestrator's test harness: a repository with `.farik/` initialised, a team of a Product
//! Manager `pm` and two Software Developers `dev-a` and `dev-b` with a WIP limit of one, a daemon
//! that is not served, and a runner that answers a replayed session's Farik tool calls the way the
//! daemon's MCP server would.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_protocol::clock::{FixedClock, IdSource, SequentialIds};
use farik_protocol::event::{EventKind, FarikEvent, NewEvent, event_from_value};
use farik_store::TaskProjection;
use farik_store::git::fixtures::{git_in, git_output_in};
use farik_store::requests::file_request;
use serde_json::{Value, json};

use super::{Orchestrator, OrchestratorDeps};
use crate::daemon::DaemonState;
use crate::exec::{ExecError, ExecResult, Executor};
use crate::forge::Forge;
use crate::recorded::{RecordedAdapter, Transcript};
use crate::sandbox::host::HostSandboxFactory;
use crate::sandbox::{Sandbox, SandboxError, SandboxFactory};
use crate::session::{RuntimeAdapter, RuntimeError, SessionHandle, SessionSpec};
use crate::tools::ToolDeps;
use crate::tools::fixtures::{TestProject, a_team_of_three, at};

pub(crate) use crate::recorded::fixtures::{UsageThenWaitAdapter, tool_runner};

/// A project the orchestrator runs on, and the daemon its sessions register with.
pub(crate) struct Harness {
    pub(crate) project: TestProject,
    pub(crate) daemon: Arc<DaemonState>,
    /// The `gh` every orchestrator of this harness drives, answering nothing until told.
    pub(crate) gh: FakeGh,
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
        let gh = FakeGh::new(name);
        Self {
            project,
            daemon,
            gh,
        }
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
        self.orchestrator_with_forge(adapter, sandboxes, self.gh.forge(&self.project.repo.path))
    }

    /// An orchestrator over this project with `adapter`, `sandboxes`, and `forge`.
    pub(crate) fn orchestrator_with_forge(
        &self,
        adapter: Arc<dyn RuntimeAdapter>,
        sandboxes: Arc<dyn SandboxFactory>,
        forge: Forge,
    ) -> Orchestrator {
        Orchestrator::new(OrchestratorDeps {
            tools: Arc::clone(&self.project.deps),
            daemon: Arc::clone(&self.daemon),
            adapter,
            sandboxes,
            session_ids: Arc::new(SequentialIds::new()),
            forge: Arc::new(forge),
        })
    }

    /// An orchestrator over this project with `adapter` and host sandboxes, whose tools read the
    /// time as `now`, and whose session ids are `later-session-1` and so on, so that they are not
    /// those of an orchestrator made before it. The governor's door keeps the project's clock.
    pub(crate) fn orchestrator_at(
        &self,
        adapter: Arc<dyn RuntimeAdapter>,
        now: DateTime<Utc>,
    ) -> Orchestrator {
        let deps = &self.project.deps;
        let tools = Arc::new(ToolDeps {
            log: Arc::clone(&deps.log),
            projections: Arc::clone(&deps.projections),
            files: Arc::clone(&deps.files),
            transitions: Arc::clone(&deps.transitions),
            git: self.project.repo.adapter(),
            clock: Arc::new(FixedClock::new(now)),
            ids: deps.ids.clone(),
        });
        Orchestrator::new(OrchestratorDeps {
            tools,
            daemon: Arc::clone(&self.daemon),
            adapter,
            sandboxes: Arc::new(HostSandboxFactory),
            session_ids: Arc::new(LaterIds(SequentialIds::new())),
            forge: Arc::new(self.gh.forge(&self.project.repo.path)),
        })
    }

    /// Files `task` in `status`: a standalone task of a Software Developer's, reviewed by another,
    /// whose one allowed path is `done.txt` and whose one criterion C1 runs `test -f done.txt`,
    /// with `change` applied to its wire last.
    pub(crate) fn file(&self, task: &str, status: &str, change: impl FnOnce(&mut Value)) {
        self.file_under(task, status, None, change);
    }

    /// `file`, the task under the epic `parent` when one is named.
    pub(crate) fn file_under(
        &self,
        task: &str,
        status: &str,
        parent: Option<&str>,
        change: impl FnOnce(&mut Value),
    ) {
        self.project
            .filed_with(task, status, "task", parent, |wire| {
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

    /// Files a request titled `title`, as the human does through `file_request`: step 11's FRK-1
    /// contract with no kind, an untriaged draft whose one allowed path is `done.txt`, whose one
    /// criterion C1 runs `test -f done.txt`, with one item out of scope, risk `low`, and 5 dollars.
    pub(crate) fn a_request(&self, title: &str) {
        let deps = &self.project.deps;
        file_request(
            &deps.files,
            &deps.log,
            Self::request_fields(title),
            "human",
            None,
            at(),
            &deps.ids,
        )
        .expect("the request is filed");
        deps.projections.catch_up().expect("the board catches up");
    }

    /// The fields of `a_request`, titled `title`, as a request's author writes them.
    pub(crate) fn request_fields(title: &str) -> Value {
        json!({
            "title": title,
            "intent": "The repository has a done.txt at its root, so that a run can be checked for it.",
            "scope": { "in_scope": ["done.txt"], "out_of_scope": ["what done.txt says"] },
            "requirements": [{ "id": "R1", "text": "done.txt is at the root." }],
            "exit_criteria": [{
                "id": "C1",
                "text": "done.txt exists.",
                "satisfies": ["R1"],
                "verification": {
                    "method": "command",
                    "command": "test -f done.txt",
                    "expect": { "exit_code": 0 }
                }
            }],
            "assignee_role": "software_developer",
            "reviewer_role": "software_developer",
            "risk": "low",
            "budget": { "max_cost_usd": 5 },
            "allowed_paths": ["done.txt"]
        })
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
    /// reviewed by `reviewer`, with its worktree made on its branch from `main`.
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
            .create_worktree(&self.worktree(task), &self.branch(task), "main")
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
        git.create_worktree(&worktree, &self.branch(task), "main")
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

    /// Writes `text` to `path` at the root, on the branch checked out there, and commits that path
    /// alone, leaving `.farik/` out of it.
    pub(crate) fn commit_at_root(&self, path: &str, text: &str, message: &str) {
        let root = &self.project.repo.path;
        std::fs::write(root.join(path), text).expect("the file is written");
        git_in(root, &["add", "--", path]);
        git_in(root, &["commit", "-m", message]);
    }

    /// `task` accepted, its worktree gone, with `a.txt`'s first line changed on its branch and
    /// changed differently on `main` afterwards, so that merging it conflicts in `a.txt`.
    pub(crate) fn accepted_in_conflict(&self, task: &str) {
        self.commit_at_root("a.txt", "first\n", "Add a.txt");
        self.accepted_with_worktree(task);
        let worktree = self.worktree(task);
        std::fs::write(worktree.join("a.txt"), "the task's\n").expect("written");
        self.project
            .deps
            .git
            .commit(&worktree, "Change a.txt", &["a.txt".to_string()])
            .expect("the branch's change is committed");
        self.project
            .deps
            .git
            .remove_worktree(&worktree)
            .expect("the worktree is removed");
        self.commit_at_root("a.txt", "main's\n", "Change a.txt on main");
    }

    /// Resolves `accepted_in_conflict` by a commit on `main` restoring `a.txt` to its text at the
    /// branch point.
    pub(crate) fn resolve_the_conflict(&self) {
        self.commit_at_root("a.txt", "first\n", "Restore a.txt");
    }

    /// A bare repository beside the project, added to it as `origin`, with `main` pushed there.
    pub(crate) fn with_origin(&self) -> PathBuf {
        let origin = self.project.repo.path.with_extension("origin.git");
        let _ = std::fs::remove_dir_all(&origin);
        std::fs::create_dir_all(&origin).expect("a directory for the remote");
        git_in(&origin, &["init", "--bare", "-b", "main"]);
        let root = &self.project.repo.path;
        git_in(
            root,
            &["remote", "add", "origin", origin.to_str().expect("a path")],
        );
        git_in(root, &["push", "origin", "main"]);
        origin
    }

    /// Merges `task`'s branch into `origin`'s `main` from a clone of `origin` of its own, as the
    /// human merging its pull request on the forge would, and answers the merge commit.
    pub(crate) fn merge_on_the_forge(&self, origin: &Path, task: &str) -> String {
        let clone = self.project.repo.path.with_extension("forge");
        let _ = std::fs::remove_dir_all(&clone);
        let parent = clone.parent().expect("a parent");
        git_in(
            parent,
            &[
                "clone",
                origin.to_str().expect("a path"),
                clone.to_str().expect("a path"),
            ],
        );
        git_in(&clone, &["config", "user.name", "Farik Test"]);
        git_in(&clone, &["config", "user.email", "test@farik.invalid"]);
        git_in(&clone, &["config", "commit.gpgsign", "false"]);
        let root = self.project.repo.path.to_str().expect("a path").to_string();
        git_in(
            &clone,
            &["fetch", &root, &format!("refs/heads/{}", self.branch(task))],
        );
        git_in(
            &clone,
            &[
                "merge",
                "--no-ff",
                "-m",
                &format!("Merge pull request for {task}"),
                "FETCH_HEAD",
            ],
        );
        git_in(&clone, &["push", "origin", "main"]);
        let sha = git_output_in(&clone, &["rev-parse", "HEAD"]);
        let _ = std::fs::remove_dir_all(&clone);
        sha
    }

    /// `origin` added at a path where there is no repository, so that every push to it fails.
    pub(crate) fn with_origin_nowhere(&self) {
        let nowhere = self.project.repo.path.with_extension("nowhere.git");
        let _ = std::fs::remove_dir_all(&nowhere);
        git_in(
            &self.project.repo.path,
            &["remote", "add", "origin", nowhere.to_str().expect("a path")],
        );
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

    /// A `session.started` of `agent`'s session `session` for `purpose` on `task`, on Opus 5 at
    /// high, with nothing after it: a session a stopped run left behind.
    pub(crate) fn started_session(&self, task: &str, agent: &str, session: &str, purpose: &str) {
        let wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "task_id": task,
            "agent_id": agent,
            "session_id": session,
            "kind": "session.started",
            "body": { "purpose": purpose, "model": "claude-opus-5", "effort": "high" },
        });
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

    /// The task's branch, the one its contract's file names (5.14).
    pub(crate) fn branch(&self, task: &str) -> String {
        self.project.branch(task)
    }

    /// Sprint `sprint` open with no budget and holding `tasks`, as starting and planning it would
    /// leave it: its file, each task's contract naming it, `sprint.started` by the human, and
    /// `sprint.planned` by the Product Manager when it holds a task.
    pub(crate) fn open_sprint(&self, sprint: &str, tasks: &[&str]) {
        self.project.open_sprint(sprint, None, tasks);
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

/// Session ids `later-session-1`, `later-session-2`, and so on.
struct LaterIds(SequentialIds);

impl IdSource for LaterIds {
    fn session_id(&self) -> String {
        format!("later-{}", self.0.session_id())
    }
}

/// An adapter that says, for each session it starts, whether the daemon registered it with an
/// executor, which Farik tools it registered it with, and which its MCP server lists to it, and
/// starts it with the adapter it wraps.
pub(crate) struct ExecutorWitness {
    inner: Arc<dyn RuntimeAdapter>,
    daemon: Arc<DaemonState>,
    seen: Mutex<Vec<bool>>,
    tools: Mutex<Vec<Vec<String>>>,
    listed: Mutex<Vec<Vec<String>>>,
}

impl ExecutorWitness {
    /// A witness of `inner`'s sessions as `daemon` registered them.
    pub(crate) fn new(inner: Arc<dyn RuntimeAdapter>, daemon: Arc<DaemonState>) -> Self {
        Self {
            inner,
            daemon,
            seen: Mutex::new(Vec::new()),
            tools: Mutex::new(Vec::new()),
            listed: Mutex::new(Vec::new()),
        }
    }

    /// For each session started, in order, the tools `tools/list` answered it with.
    pub(crate) fn listed_tools(&self) -> Vec<Vec<String>> {
        self.listed
            .lock()
            .expect("no test panics holding it")
            .clone()
    }

    /// For each session started, in order, the Farik tools its registration was given.
    pub(crate) fn given_tools(&self) -> Vec<Vec<String>> {
        self.tools
            .lock()
            .expect("no test panics holding it")
            .clone()
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
        self.tools.lock().expect("no test panics holding it").push(
            self.daemon
                .farik_tools(&spec.session_id)
                .expect("the session is registered before it starts"),
        );
        self.listed.lock().expect("no test panics holding it").push(
            crate::daemon::listed_names(&self.daemon, &spec.session_id)
                .expect("the session is registered before it starts"),
        );
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
    based: Mutex<BTreeMap<String, u32>>,
    removed: Mutex<BTreeMap<String, u32>>,
}

impl CountingSandboxFactory {
    /// How many times `create_base` was called for `task`.
    pub(crate) fn based(&self, task: &str) -> u32 {
        self.based
            .lock()
            .expect("no test panics holding it")
            .get(task)
            .copied()
            .unwrap_or_default()
    }

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
        *self
            .based
            .lock()
            .expect("no test panics holding it")
            .entry(task_id.as_str().to_string())
            .or_default() += 1;
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
/// others are host sandboxes; it counts, as `CountingSandboxFactory` does, what it made. Its base
/// sandboxes are host sandboxes too, unless it was made `for_base`.
pub(crate) struct BrokenSandboxFactory {
    error: ExecError,
    broken: AtomicU32,
    breaks_base: bool,
    counting: CountingSandboxFactory,
}

impl BrokenSandboxFactory {
    /// A factory whose first `broken` sandboxes fail every command with `error`.
    pub(crate) fn new(error: ExecError, broken: u32) -> Self {
        Self {
            error,
            broken: AtomicU32::new(broken),
            breaks_base: false,
            counting: CountingSandboxFactory::default(),
        }
    }

    /// A factory whose every base sandbox fails every command with `error`, and whose other
    /// sandboxes are host sandboxes.
    pub(crate) fn for_base(error: ExecError) -> Self {
        Self {
            breaks_base: true,
            ..Self::new(error, 0)
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
        let sandbox = HostSandboxFactory.create_base(project_id, task_id, worktree)?;
        if self.breaks_base {
            return Ok(Box::new(BrokenSandbox(self.error.clone())));
        }
        Ok(sandbox)
    }

    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError> {
        self.counting.remove(project_id, task_id)
    }
}

/// A host sandbox factory whose first `failures` removals fail as they do while docker is down;
/// it counts, as `CountingSandboxFactory` does, what it was asked.
pub(crate) struct UnremovableSandboxFactory {
    failures: AtomicU32,
    pub(crate) counting: CountingSandboxFactory,
}

impl UnremovableSandboxFactory {
    /// A factory whose first `failures` removals fail.
    pub(crate) fn new(failures: u32) -> Self {
        Self {
            failures: AtomicU32::new(failures),
            counting: CountingSandboxFactory::default(),
        }
    }
}

impl SandboxFactory for UnremovableSandboxFactory {
    fn create(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
        network: bool,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        self.counting.create(project_id, task_id, worktree, network)
    }

    fn create_base(
        &self,
        project_id: &str,
        task_id: &TaskId,
        worktree: &Path,
    ) -> Result<Box<dyn Sandbox>, SandboxError> {
        self.counting.create_base(project_id, task_id, worktree)
    }

    fn remove(&self, project_id: &str, task_id: &TaskId) -> Result<(), SandboxError> {
        self.counting.remove(project_id, task_id)?;
        let fails = self
            .failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if fails {
            return Err(SandboxError::DockerUnavailable);
        }
        Ok(())
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

/// A fake `gh`: a shell script in a temporary directory of its own. For its second argument
/// (`list`, `create`, `view`) it prints the standard output and error the test gave for that
/// subcommand and exits with the code given, 0 when none was; it appends its arguments,
/// NUL-separated, one call per line, to `calls` beside it, and keeps its standard input.
pub(crate) struct FakeGh {
    dir: PathBuf,
}

impl FakeGh {
    /// A fake that answers nothing until told, in a directory named after `name`.
    pub(crate) fn new(name: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "farik-fake-gh-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the fake's directory is made");
        let script = format!(
            "#!/bin/sh\n\
             dir='{dir}'\n\
             sub=\"$2\"\n\
             for arg in \"$@\"; do printf '%s\\0' \"$arg\"; done >> \"$dir/calls\"\n\
             printf '\\n' >> \"$dir/calls\"\n\
             cat > \"$dir/$sub.stdin\"\n\
             if [ -f \"$dir/$sub.out\" ]; then cat \"$dir/$sub.out\"; fi\n\
             if [ -f \"$dir/$sub.err\" ]; then cat \"$dir/$sub.err\" >&2; fi\n\
             code=0\n\
             if [ -f \"$dir/$sub.code\" ]; then code=$(cat \"$dir/$sub.code\"); fi\n\
             exit \"$code\"\n",
            dir = dir.display()
        );
        // Written by `cp` rather than by this process: a file this process holds open for writing
        // is inherited by whatever another test thread forks meanwhile, and running it then fails
        // with "text file busy" (as `claude_process.rs` does).
        let source = dir.join("gh.txt");
        std::fs::write(&source, script).expect("the script is written");
        let program = dir.join("gh");
        let copied = std::process::Command::new("cp")
            .arg(&source)
            .arg(&program)
            .status()
            .expect("cp runs");
        assert!(copied.success(), "the script is copied");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
            .expect("the script is executable");
        Self { dir }
    }

    /// Makes `sub` print `stdout` and `stderr` and exit with `code`.
    pub(crate) fn answers(&self, sub: &str, stdout: &str, stderr: &str, code: i32) -> &Self {
        std::fs::write(self.dir.join(format!("{sub}.out")), stdout).expect("written");
        std::fs::write(self.dir.join(format!("{sub}.err")), stderr).expect("written");
        std::fs::write(self.dir.join(format!("{sub}.code")), code.to_string()).expect("written");
        self
    }

    /// The fake's path.
    pub(crate) fn program(&self) -> PathBuf {
        self.dir.join("gh")
    }

    /// A forge driving this fake in `root`.
    pub(crate) fn forge(&self, root: &Path) -> Forge {
        Forge {
            program: self.program(),
            root: root.to_path_buf(),
        }
    }

    /// Every call's arguments, oldest first.
    pub(crate) fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(|line| {
                line.split('\0')
                    .filter(|argument| !argument.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .collect()
    }

    /// What the last call of `sub` was given on its standard input.
    pub(crate) fn stdin_of(&self, sub: &str) -> String {
        std::fs::read_to_string(self.dir.join(format!("{sub}.stdin"))).unwrap_or_default()
    }
}
