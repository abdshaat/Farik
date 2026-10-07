//! A project the tools can be called on: a repository with `.farik/` initialised, a log in
//! memory, the criterion library `farik-core`'s fixture describes, and a team of a Product Manager `pm` and two Software Developers `dev-a` and `dev-b`.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use farik_core::branch::task_branch;
use farik_core::contract::fixtures::a_contract_wire;
use farik_core::contract::{TaskId, validate_contract};
use farik_core::criteria::fixtures::a_criteria_library_wire;
use farik_core::criteria::validate_criteria;
use farik_core::governor::permissions::PermissionTier;
use farik_core::sprint::fixtures::an_open_sprint_wire;
use farik_core::sprint::validate_sprint;
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
use crate::session::SessionPurpose;
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

/// Adds the UI/UX Designer `iris` and the Architect `ada` to a team's wire.
pub(crate) fn with_the_designer(wire: &mut Value) {
    let agents = wire["agents"].as_array_mut().expect("a list of agents");
    agents.push(an_agent_wire("iris", "ui_ux_designer"));
    agents.push(an_agent_wire("ada", "architect"));
}

/// Adds the Marketing Specialist `kai` to a team's wire.
pub(crate) fn with_the_marketing_specialist(wire: &mut Value) {
    wire["agents"]
        .as_array_mut()
        .expect("a list of agents")
        .push(an_agent_wire("kai", "marketing_specialist"));
}

/// Adds the Finance Specialist `fin` to a team's wire.
pub(crate) fn with_the_finance_specialist(wire: &mut Value) {
    wire["agents"]
        .as_array_mut()
        .expect("a list of agents")
        .push(an_agent_wire("fin", "finance_specialist"));
}

/// Adds the Procurement Specialist `proc` to a team's wire.
pub(crate) fn with_the_procurement_specialist(wire: &mut Value) {
    wire["agents"]
        .as_array_mut()
        .expect("a list of agents")
        .push(an_agent_wire("proc", "procurement_specialist"));
}

/// The Designer `iris` and the Architect `ada` added, both with the Playwright connector on,
/// and a preview set.
pub(crate) fn browsing(wire: &mut Value) {
    with_the_designer(wire);
    let on = json!([{ "name": "playwright", "source": "builtin" }]);
    wire["agents"][3]["mcp_servers"] = on.clone();
    wire["agents"][4]["mcp_servers"] = on;
    wire["preview"] = json!({
        "prepare": "make site",
        "start": "busybox httpd -f -p 4401 -h site",
        "port": 4401
    });
}

/// The Software Developer's kit for a test: its skills `(name, body)`, each in a folder of one
/// `SKILL.md`, and, when `search` is given, the service `github` (a program started on the host,
/// one key) with `search` tagged so and `create_issue` and `delete_repo` as they are in
/// `a_kit_server`.
pub(crate) fn a_developer_kit(skills: &[(&str, &str)], search: Option<&str>) -> farik_roles::Kit {
    let connectors: Vec<Value> = search
        .map(|tag| {
            json!({
                "name": "github", "transport": "stdio", "command": "github-mcp",
                "args": ["stdio"], "credential_keys": ["API_KEY"],
                "title": "GitHub", "about": "Where the code lives.",
                "why": "Lets the Developer read issues.",
                "setup": "Make a key on GitHub's page and paste it.",
                "key_page": "https://github.example/keys",
                "tools": { "search": tag, "create_issue": "external_effect", "delete_repo": "denied" }
            })
        })
        .into_iter()
        .collect();
    let texts: Vec<(String, String)> = skills
        .iter()
        .map(|(name, body)| {
            (
                (*name).to_string(),
                format!("---\nname: {name}\ndescription: Use when {name}.\n---\n{body}"),
            )
        })
        .collect();
    let folders: Vec<(&str, Vec<(&str, &str)>)> = texts
        .iter()
        .map(|(name, text)| (name.as_str(), vec![("SKILL.md", text.as_str())]))
        .collect();
    let folders: Vec<(&str, &[(&str, &str)])> = folders
        .iter()
        .map(|(name, files)| (*name, files.as_slice()))
        .collect();
    let file = json!({
        "role": "software_developer",
        "skills": skills.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        "connectors": connectors,
    });
    farik_roles::parse_fixture_kit(
        farik_core::contract::Role::SoftwareDeveloper,
        &file.to_string(),
        &[],
        &folders,
    )
    .expect("the fixture kit loads")
}

/// `dev-a`'s entry for the service `github` of `a_developer_kit(_, Some("network"))`, as the team
/// file holds it once connected.
pub(crate) fn a_kit_server() -> Value {
    json!({
        "name": "github", "source": "kit", "transport": "stdio",
        "command": "github-mcp", "args": ["stdio"], "credential_keys": ["API_KEY"],
        "tools": { "search": "network", "create_issue": "external_effect", "delete_repo": "denied" }
    })
}

/// A project the tools run on.
pub(crate) struct TestProject {
    pub(crate) repo: TempRepo,
    pub(crate) deps: Arc<ToolDeps>,
    kits: Arc<std::sync::Mutex<Vec<farik_roles::Kit>>>,
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
        let kits: Arc<std::sync::Mutex<Vec<farik_roles::Kit>>> = Arc::default();
        let held = Arc::clone(&kits);
        let deps = Arc::new(ToolDeps {
            log,
            projections,
            files,
            transitions,
            git: repo.adapter(),
            clock,
            ids,
            kits: Arc::new(move |role| {
                let swapped = held
                    .lock()
                    .ok()
                    .and_then(|kits| kits.iter().find(|kit| kit.role == role).cloned());
                swapped.map_or_else(|| farik_roles::load_kit(role), Ok)
            }),
        });
        Self { repo, deps, kits }
    }

    /// Swaps in `kit` as its role's kit, for this project alone and from now on.
    pub(crate) fn set_kit(&self, kit: farik_roles::Kit) {
        if let Ok(mut kits) = self.kits.lock() {
            kits.retain(|held| held.role != kit.role);
            kits.push(kit);
        }
    }

    /// A session of `agent` on `task`, with no executor.
    pub(crate) fn context(&self, agent: &str, task: Option<&str>) -> ToolContext {
        ToolContext {
            agent_id: agent.to_string(),
            task_id: task.map(|task| task.parse().expect("a task id")),
            session_id: "session-1".to_string(),
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            executor: None,
            tiers: tiers_of(&self.deps, agent),
            connectors: Vec::new(),
            preview: None,
            deps: Arc::clone(&self.deps),
            daemon: std::sync::Weak::new(),
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

    /// The branch of `task`, the one its contract's file names (5.14).
    pub(crate) fn branch(&self, task: &str) -> String {
        task_branch(
            &self
                .deps
                .files
                .read_contract(&task.parse().expect("a task id"))
                .expect("the file reads"),
        )
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

    /// Sprint `sprint` open with `budget_usd` and holding `tasks`, as starting and planning it
    /// would leave it: its file, each task's contract naming it, `sprint.started` by the human, and
    /// `sprint.planned` by the Product Manager when it holds a task.
    pub(crate) fn open_sprint(&self, sprint: &str, budget_usd: Option<f64>, tasks: &[&str]) {
        let files = &self.deps.files;
        let mut wire = an_open_sprint_wire();
        wire["id"] = json!(sprint);
        wire["task_ids"] = json!(tasks);
        if let Some(usd) = budget_usd {
            wire["budget_usd"] = json!(usd);
        }
        files
            .write_sprint(&validate_sprint(&wire).expect("the fixture is a sprint"))
            .expect("the sprint is written");
        for task in tasks {
            let id: TaskId = task.parse().expect("a task id");
            let mut contract = files.read_contract(&id).expect("the contract reads");
            contract.sprint = Some(sprint.to_string());
            files
                .write_contract(&contract)
                .expect("the contract is written");
        }
        self.record(
            "",
            "sprint.started",
            &json!({ "sprint_id": sprint, "budget_usd": budget_usd, "started_by": "human" }),
        );
        if !tasks.is_empty() {
            self.record(
                "",
                "sprint.planned",
                &json!({ "sprint_id": sprint, "task_ids": tasks, "planned_by": "pm" }),
            );
        }
    }

    /// A `task.transitioned` of `task` from `from` into `to`, by the governor, with `extra`
    /// merged over it (`assignee`, `reviewer`, and so on).
    pub(crate) fn moved(&self, task: &str, from: &str, to: &str, extra: &Value) -> FarikEvent {
        self.moved_at(at(), task, from, to, extra)
    }

    /// `moved`, recorded at `recorded_at`.
    pub(crate) fn moved_at(
        &self,
        recorded_at: DateTime<Utc>,
        task: &str,
        from: &str,
        to: &str,
        extra: &Value,
    ) -> FarikEvent {
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
        self.record_at(recorded_at, task, "task.transitioned", &body)
    }

    /// Kai's proposal of marketing plan `plan` on `task`, as the tool records it: the protocol's
    /// fixture plan between `starts_on` and `ends_on` (`YYYY-MM-DD`), with no campaign and no post.
    pub(crate) fn plan_proposed(
        &self,
        task: &str,
        plan: &str,
        starts_on: &str,
        ends_on: &str,
    ) -> FarikEvent {
        let mut body =
            farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
        body["plan"] = json!(plan);
        body["starts_on"] = json!(starts_on);
        body["ends_on"] = json!(ends_on);
        body["campaigns"] = json!([]);
        body["posts"] = json!([]);
        body["budget"] = json!({ "total": "2000", "google_ads": "0" });
        self.record_by(Some("kai"), at(), task, "marketing_plan.proposed", &body)
    }

    /// The owner's approval of `plan` on `task`, with `note` (empty for none): no agent, no
    /// session.
    pub(crate) fn plan_approved(&self, task: &str, plan: &str, note: &str) -> FarikEvent {
        self.record(
            task,
            "marketing_plan.approved",
            &json!({ "plan": plan, "note": note }),
        )
    }

    /// A `cost.recorded` of `usd` dollars for `purpose` by `agent` in session `session` on `day`
    /// (`YYYY-MM-DD`, UTC), with `tokens` input and output, against `task` when one is named.
    pub(crate) fn spent(
        &self,
        agent: &str,
        task: Option<&str>,
        session: &str,
        day: &str,
        (usd, tokens): (f64, u64),
    ) -> FarikEvent {
        let mut wire = json!({
            "seq": 1,
            "recorded_at": format!("{day}T10:00:00Z"),
            "team_id": "farik",
            "project_id": "farik",
            "agent_id": agent,
            "session_id": session,
            "kind": "cost.recorded",
            "body": {
                "purpose": "implement",
                "model_id": "claude-sonnet-5",
                "usage": {
                    "input_tokens": tokens,
                    "output_tokens": tokens / 10,
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

    /// Appends one event about `task` (or none) and projects it, as a command does.
    pub(crate) fn record(&self, task: &str, kind: &str, body: &Value) -> FarikEvent {
        self.record_at(at(), task, kind, body)
    }

    /// `record`, recorded at `recorded_at`.
    pub(crate) fn record_at(
        &self,
        recorded_at: DateTime<Utc>,
        task: &str,
        kind: &str,
        body: &Value,
    ) -> FarikEvent {
        self.record_by(None, recorded_at, task, kind, body)
    }

    /// `record_at`, by `agent` when there is one.
    pub(crate) fn record_by(
        &self,
        agent: Option<&str>,
        recorded_at: DateTime<Utc>,
        task: &str,
        kind: &str,
        body: &Value,
    ) -> FarikEvent {
        let mut wire = json!({
            "seq": 1,
            "recorded_at": recorded_at.to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "kind": kind,
            "body": body,
        });
        if !task.is_empty() {
            wire["task_id"] = json!(task);
        }
        if let Some(agent) = agent {
            wire["agent_id"] = json!(agent);
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

/// `agent`'s tiers as the team file says now, which a session starting now is given; none for an
/// agent the team does not have.
pub(crate) fn tiers_of(deps: &ToolDeps, agent: &str) -> Vec<PermissionTier> {
    let team = deps.files.read_team().expect("the team reads");
    team.agents
        .iter()
        .find(|one| one.id.as_str() == agent)
        .map(|one| one.tiers(&team.permissions()))
        .unwrap_or_default()
}

/// Runs one call to the end on a runtime of its own.
pub(crate) fn run(context: &ToolContext, name: &str, input: Value) -> Result<Value, ToolError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime is made")
        .block_on(call_tool(context, name, input))
}
