//! A project the tools can be called on: a repository with `.farik/` initialised, a log in
//! memory, the criterion library `farik-core`'s fixture describes, and a team of a Product Manager `pm` and two Software Developers `dev-a` and `dev-b`.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use farik_core::contract::fixtures::a_contract_wire;
use farik_core::contract::validate_contract;
use farik_core::criteria::fixtures::a_criteria_library_wire;
use farik_core::criteria::validate_criteria;
use farik_core::team::fixtures::{a_team_wire, an_agent_wire};
use farik_core::team::{Team, validate_team};
use farik_protocol::clock::{Clock, FixedClock};
use farik_protocol::event::{EventIds, EventKind, FarikEvent, NewEvent, event_from_value};
use farik_store::files::ProjectFiles;
use farik_store::git::fixtures::TempRepo;
use farik_store::{EventQuery, IN_MEMORY, Projections, open_event_log, open_projections};
use serde_json::{Value, json};

use super::{ToolContext, ToolDeps, ToolError, call_tool};
use crate::exec::Executor;
use crate::transitions::Transitions;

/// The time every fixture event and every tool call is stamped with.
pub(crate) fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0)
        .single()
        .expect("a real time")
}

/// The team of `pm`, `dev-a`, and `dev-b`, with `change` applied to its wire first.
pub(crate) fn a_team_of_three(change: impl FnOnce(&mut Value)) -> Team {
    let mut wire = a_team_wire();
    wire["agents"] = json!([
        an_agent_wire("pm", "product_manager"),
        an_agent_wire("dev-a", "software_developer"),
        an_agent_wire("dev-b", "software_developer"),
    ]);
    change(&mut wire);
    validate_team(&wire).expect("the fixture is a team")
}

/// A project the tools run on.
pub(crate) struct TestProject {
    pub(crate) repo: TempRepo,
    pub(crate) deps: Arc<ToolDeps>,
}

impl TestProject {
    pub(crate) fn new(name: &str, team: &Team) -> Self {
        let repo = TempRepo::new(name);
        let files = Arc::new(ProjectFiles::open(repo.path.clone()));
        files.init(team).expect(".farik/ is made");
        files
            .write_criteria(
                &validate_criteria(&a_criteria_library_wire()).expect("the fixture is a library"),
            )
            .expect("the library is written");
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        let projections: Arc<Projections> =
            Arc::new(open_projections(Arc::clone(&log)).expect("the projections open"));
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
        let deps = Arc::new(ToolDeps {
            log,
            projections,
            files,
            transitions,
            git: repo.adapter(),
            clock,
            ids,
        });
        Self { repo, deps }
    }

    /// A session of `agent` on `task`, with no executor.
    pub(crate) fn context(&self, agent: &str, task: Option<&str>) -> ToolContext {
        ToolContext {
            agent_id: agent.to_string(),
            task_id: task.map(|task| task.parse().expect("a task id")),
            session_id: "session-1".to_string(),
            executor: None,
            deps: Arc::clone(&self.deps),
        }
    }

    /// Calls one tool as `agent` on `task`, with no executor.
    pub(crate) fn call(
        &self,
        agent: &str,
        task: Option<&str>,
        name: &str,
        input: Value,
    ) -> Result<Value, ToolError> {
        run(&self.context(agent, task), name, input)
    }

    /// Calls one tool as `agent` on `task` with `executor`.
    pub(crate) fn call_with(
        &self,
        executor: Arc<dyn Executor>,
        agent: &str,
        task: Option<&str>,
        name: &str,
        input: Value,
    ) -> Result<Value, ToolError> {
        let mut context = self.context(agent, task);
        context.executor = Some(executor);
        run(&context, name, input)
    }

    /// Every event of these kinds, oldest first; every event when `kinds` is empty.
    pub(crate) fn events(&self, kinds: &[EventKind]) -> Vec<FarikEvent> {
        self.deps
            .log
            .read(&EventQuery {
                kinds: kinds.to_vec(),
                ..EventQuery::default()
            })
            .expect("the log reads")
    }

    /// How many events the log holds.
    pub(crate) fn event_count(&self) -> usize {
        self.events(&[]).len()
    }

    /// The contract file of `task`, as a value.
    pub(crate) fn file(&self, task: &str) -> Value {
        serde_json::to_value(
            self.deps
                .files
                .read_contract(&task.parse().expect("a task id"))
                .expect("the file reads"),
        )
        .expect("a contract serialises")
    }

    /// The fixture contract as `task`, a Software Developer's reviewed by another, written to its
    /// file with `change` applied, and put on the board by a `task.created` in `status`.
    pub(crate) fn filed_with(
        &self,
        task: &str,
        status: &str,
        kind: &str,
        parent: Option<&str>,
        change: impl FnOnce(&mut Value),
    ) {
        let mut wire = a_contract_wire();
        wire["id"] = json!(task);
        wire["status"] = json!(status);
        wire["kind"] = json!(kind);
        wire["reviewer_role"] = json!("software_developer");
        if let Some(parent) = parent {
            wire["parent"] = json!(parent);
        }
        change(&mut wire);
        let contract = validate_contract(&wire).expect("the fixture is a contract");
        // The counter hands out every id a project has, so a fixture task takes one too, and a
        // task filed after it is not given its id.
        self.deps.log.next_task_id().expect("an id is handed out");
        self.deps
            .files
            .write_contract(&contract)
            .expect("the file is written");
        let mut summary =
            json!({ "kind": kind, "title": "Add a login page", "status": status, "risk": "low" });
        if let Some(parent) = parent {
            summary["parent"] = json!(parent);
        }
        self.record(
            task,
            "task.created",
            &json!({ "summary": summary, "created_by": "human" }),
        );
    }

    /// `filed_with` and no change.
    pub(crate) fn filed(&self, task: &str, status: &str, kind: &str, parent: Option<&str>) {
        self.filed_with(task, status, kind, parent, |_| {});
    }

    /// A `task.transitioned` of `task` from `from` into `to`, by the governor, with `extra`
    /// merged over it (`assignee`, `reviewer`, and so on).
    pub(crate) fn moved(&self, task: &str, from: &str, to: &str, extra: &Value) -> FarikEvent {
        let mut body = json!({
            "from": from,
            "to": to,
            "actor": "governor",
            "requested_by": "governor",
            "gate": "none",
            "effects": [],
            "iteration": 0
        });
        if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                body.insert(key.clone(), value.clone());
            }
        }
        self.record(task, "task.transitioned", &body)
    }

    /// Appends one event about `task` (or none) and projects it, as a command does.
    pub(crate) fn record(&self, task: &str, kind: &str, body: &Value) -> FarikEvent {
        let mut wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "kind": kind,
            "body": body,
        });
        if !task.is_empty() {
            wire["task_id"] = json!(task);
        }
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        let appended = self
            .deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        self.deps.projections.apply(&appended).expect("projects");
        appended
    }
}

/// Runs one call to the end on a runtime of its own.
fn run(context: &ToolContext, name: &str, input: Value) -> Result<Value, ToolError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime is made")
        .block_on(call_tool(context, name, input))
}
