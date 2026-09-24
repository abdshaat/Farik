//! The five harness metrics of `docs/SPEC.md` F17, over a project recorded on disk.
//!
//! These need somewhere to write and nothing else, so they run in the default `cargo xtask check`.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use farik_core::contract::fixtures::a_contract_wire;
use farik_core::contract::validate_contract;
use farik_protocol::event::fixtures::{a_contract_summary_wire, an_event_wire};
use farik_protocol::event::{CostRecordedBodyPurpose, EventKind, NewEvent, event_from_value};
use farik_store::files::FilesError;
use farik_store::files::fixtures::TempProject;
use farik_store::metrics::{HarnessMetrics, MetricsError};
use farik_store::{EventLog, Projections, open_event_log, open_projections};
use serde_json::{Value, json};

/// A project with a log on disk and the projections of it, both empty.
struct Recorded {
    project: TempProject,
    log: Arc<EventLog>,
    projections: Projections,
}

impl Recorded {
    fn new(name: &str) -> Self {
        let project = TempProject::new(name);
        let at = Utc
            .with_ymd_and_hms(2026, 9, 17, 9, 0, 0)
            .single()
            .expect("a real hour");
        let log = Arc::new(
            open_event_log(&project.root.join(".farik/local/farik.db"), at).expect("the log opens"),
        );
        let projections = open_projections(Arc::clone(&log)).expect("the projections open");
        Self {
            project,
            log,
            projections,
        }
    }

    /// Appends one event of `kind` with `body`, about `task` when there is one, and projects it.
    fn record(&self, kind: EventKind, task: Option<&str>, body: Value) {
        let mut wire = an_event_wire(kind);
        match task {
            Some(task) => wire["task_id"] = json!(task),
            None => {
                wire.as_object_mut()
                    .expect("an event is an object")
                    .remove("task_id");
            }
        }
        wire["body"] = body;
        self.append(&wire);
    }

    fn append(&self, wire: &Value) {
        let event = event_from_value(wire).expect("the event is schema-valid");
        let new = NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        };
        self.projections
            .apply(&self.log.append(&new).expect("appends"))
            .expect("projects");
    }

    /// Files `task` as a `kind` in `draft`, under `parent` when there is one.
    fn created(&self, task: &str, kind: &str, parent: Option<&str>) {
        let mut summary = a_contract_summary_wire();
        summary["kind"] = json!(kind);
        if let Some(parent) = parent {
            summary["parent"] = json!(parent);
        }
        self.record(
            EventKind::TaskCreated,
            Some(task),
            json!({ "summary": summary, "created_by": "human" }),
        );
    }

    /// Moves `task` from `from` into `to`, asked for by `actor`.
    fn moved(&self, task: &str, from: &str, to: &str, actor: &str) {
        self.record(
            EventKind::TaskTransitioned,
            Some(task),
            json!({
                "from": from,
                "to": to,
                "actor": actor,
                "requested_by": actor,
                "gate": "none",
                "effects": [],
                "iteration": 0
            }),
        );
    }

    /// Moves `task` through each status of `path` in turn, each move by `product_manager`.
    fn walked(&self, task: &str, path: &[&str]) {
        for pair in path.windows(2) {
            self.moved(task, pair[0], pair[1], "product_manager");
        }
    }

    fn escalated(&self, task: &str, reason: &str) {
        self.record(
            EventKind::EscalationRaised,
            Some(task),
            json!({ "reason": reason, "detail": "a gate" }),
        );
    }

    /// A `cost.recorded` of `usd` for `purpose`, about `task` when there is one, in a session of
    /// its own, at ten UTC on `day`.
    fn cost(&self, task: Option<&str>, purpose: &str, usd: f64, day: &str) {
        self.append(&self.cost_wire(task, purpose, usd, day));
    }

    /// A `cost.recorded` for a model no price table prices: no dollars, and `unpriced`.
    fn unpriced_cost(&self, task: Option<&str>, purpose: &str, day: &str) {
        let mut wire = self.cost_wire(task, purpose, 0.0, day);
        wire["body"]["unpriced"] = json!(true);
        self.append(&wire);
    }

    fn cost_wire(&self, task: Option<&str>, purpose: &str, usd: f64, day: &str) -> Value {
        let mut wire = an_event_wire(EventKind::CostRecorded);
        if let Some(task) = task {
            wire["task_id"] = json!(task);
        }
        let session = self
            .log
            .read(&farik_store::EventQuery::default())
            .expect("the log reads")
            .len();
        wire["agent_id"] = json!("dev-a");
        wire["session_id"] = json!(format!("s{session}"));
        wire["recorded_at"] = json!(format!("{day}T10:00:00Z"));
        wire["body"]["purpose"] = json!(purpose);
        wire["body"]["cost_usd"] = json!(usd);
        wire
    }

    /// Writes `task`'s contract: the fixture's, with one exit criterion per method in `methods`,
    /// numbered `C1` on.
    fn contract(&self, task: &str, methods: &[&str]) {
        let mut wire = a_contract_wire();
        wire["id"] = json!(task);
        let criteria: Vec<Value> = methods
            .iter()
            .zip(1..)
            .map(|(method, number)| {
                let verification = match *method {
                    "command" => json!({
                        "method": "command",
                        "command": "true",
                        "expect": { "exit_code": 0 }
                    }),
                    "test" => json!({ "method": "test", "command": "cargo test" }),
                    "artifact" => json!({ "method": "artifact", "path": "done.txt" }),
                    "review" => json!({ "method": "review", "rubric": ["Is it right?"] }),
                    _ => json!({ "method": "human", "question": "Is it right?" }),
                };
                json!({
                    "id": format!("C{number}"),
                    "text": format!("The {method} criterion."),
                    "satisfies": ["R1"],
                    "verification": verification
                })
            })
            .collect();
        wire["exit_criteria"] = json!(criteria);
        let contract = validate_contract(&wire).expect("the contract is valid");
        self.project
            .files()
            .write_contract(&contract)
            .expect("the contract is written");
    }

    fn metrics(&self) -> Result<HarnessMetrics, MetricsError> {
        self.projections.metrics(&self.project.files())
    }

    fn metrics_for_sprint(&self, sprint_id: &str) -> Result<HarnessMetrics, MetricsError> {
        self.projections
            .metrics_for_sprint(&self.project.files(), sprint_id)
    }

    /// Opens `sprint_id`, with `budget_usd` when there is one.
    fn sprint_started(&self, sprint_id: &str, budget_usd: Option<f64>) {
        self.record(
            EventKind::SprintStarted,
            None,
            json!({ "sprint_id": sprint_id, "budget_usd": budget_usd, "started_by": "human" }),
        );
    }

    /// Puts `task_ids` into `sprint_id`.
    fn sprint_planned(&self, sprint_id: &str, task_ids: &[&str]) {
        self.record(
            EventKind::SprintPlanned,
            None,
            json!({ "sprint_id": sprint_id, "task_ids": task_ids, "planned_by": "sam-ortiz" }),
        );
    }

    /// Closes `sprint_id`, with `left` the tasks that were still in it.
    fn sprint_ended(&self, sprint_id: &str, left: &[&str]) {
        self.record(
            EventKind::SprintEnded,
            None,
            json!({ "sprint_id": sprint_id, "ended_by": "human", "left": left }),
        );
    }
}

/// The path of a task from `draft` to `in_progress`, with no move by the human.
const TO_WORK: [&str; 5] = ["draft", "refining", "ready", "assigned", "in_progress"];

/// A task's path from `draft` through one verification to `accepted`.
fn verified_once(recorded: &Recorded, task: &str) {
    recorded.walked(task, &TO_WORK);
    recorded.moved(task, "in_progress", "verifying", "assignee");
    recorded.moved(task, "verifying", "accepted", "product_manager");
}

/// The project the plan's table describes: an accepted epic, four accepted tasks (one rejected
/// once), a cancelled one, an escalated one, and a cost with no task.
fn recorded_project(name: &str) -> Recorded {
    let recorded = Recorded::new(name);

    recorded.contract("FRK-1", &["command", "review"]);
    recorded.created("FRK-1", "epic", None);
    recorded.moved("FRK-1", "draft", "refining", "product_manager");
    recorded.moved("FRK-1", "refining", "escalated", "governor");
    recorded.escalated("FRK-1", "readiness_failures");
    recorded.moved("FRK-1", "escalated", "refining", "human");
    recorded.moved("FRK-1", "refining", "escalated", "governor");
    recorded.escalated("FRK-1", "approval");
    recorded.moved("FRK-1", "escalated", "ready", "human");
    recorded.record(
        EventKind::QuestionAsked,
        Some("FRK-1"),
        json!({ "question": "Which pages?", "asked_by": "pm" }),
    );
    recorded.record(
        EventKind::HumanAccepted,
        Some("FRK-1"),
        json!({ "subject": "contract", "accepted_by": "human" }),
    );
    recorded.walked("FRK-1", &["ready", "assigned", "in_progress"]);
    recorded.moved("FRK-1", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-1", "verifying", "accepted", "product_manager");
    recorded.cost(Some("FRK-1"), "triage", 0.25, "2026-09-22");
    recorded.cost(Some("FRK-1"), "refine", 0.5, "2026-09-22");
    recorded.cost(Some("FRK-1"), "plan", 0.5, "2026-09-27");

    recorded.contract("FRK-2", &["command", "test"]);
    recorded.created("FRK-2", "task", Some("FRK-1"));
    verified_once(&recorded, "FRK-2");
    recorded.cost(Some("FRK-2"), "implement", 0.5, "2026-09-28");
    recorded.cost(Some("FRK-2"), "verify", 0.25, "2026-10-06");

    recorded.contract("FRK-3", &["artifact", "review"]);
    recorded.created("FRK-3", "task", Some("FRK-1"));
    recorded.walked("FRK-3", &TO_WORK);
    recorded.moved("FRK-3", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-3", "verifying", "rejected", "reviewer");
    recorded.moved("FRK-3", "rejected", "in_progress", "governor");
    recorded.moved("FRK-3", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-3", "verifying", "accepted", "product_manager");
    recorded.cost(Some("FRK-3"), "implement", 1.0, "2026-09-28");
    recorded.cost(Some("FRK-3"), "verify", 0.5, "2026-10-06");

    recorded.contract("FRK-4", &["command", "human"]);
    recorded.created("FRK-4", "task", None);
    recorded.moved("FRK-4", "draft", "refining", "product_manager");
    recorded.moved("FRK-4", "refining", "escalated", "governor");
    recorded.escalated("FRK-4", "risk_gate");
    recorded.moved("FRK-4", "escalated", "ready", "human");
    recorded.walked("FRK-4", &["ready", "assigned", "in_progress"]);
    recorded.moved("FRK-4", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-4", "verifying", "accepted", "product_manager");
    recorded.escalated("FRK-4", "integration");
    recorded.cost(Some("FRK-4"), "implement", 1.0, "2026-09-29");
    recorded.cost(Some("FRK-4"), "verify", 0.25, "2026-10-06");

    recorded.contract("FRK-5", &["review"]);
    recorded.created("FRK-5", "task", None);
    recorded.walked("FRK-5", &TO_WORK);
    recorded.moved("FRK-5", "in_progress", "blocked", "assignee");
    recorded.moved("FRK-5", "blocked", "escalated", "governor");
    recorded.escalated("FRK-5", "blocker_age");
    recorded.moved("FRK-5", "escalated", "cancelled", "human");
    recorded.cost(Some("FRK-5"), "implement", 0.5, "2026-09-29");

    recorded.contract("FRK-6", &["command"]);
    recorded.created("FRK-6", "task", None);
    recorded.walked("FRK-6", &TO_WORK);
    recorded.moved("FRK-6", "in_progress", "blocked", "assignee");
    recorded.moved("FRK-6", "blocked", "in_progress", "human");
    recorded.moved("FRK-6", "in_progress", "escalated", "human");
    recorded.escalated("FRK-6", "explicit_request");

    recorded.contract("FRK-7", &["command", "review"]);
    recorded.created("FRK-7", "task", Some("FRK-1"));
    verified_once(&recorded, "FRK-7");
    recorded.cost(Some("FRK-7"), "refine", 0.5, "2026-09-22");

    recorded.cost(None, "conversation", 0.25, "2026-10-06");
    recorded
}

fn metrics_of(recorded: &Recorded) -> HarnessMetrics {
    recorded.metrics().expect("the metrics compute")
}

#[test]
fn measures_first_pass_acceptance() {
    let metrics = metrics_of(&recorded_project("first-pass"));
    assert_eq!(metrics.accepted_tasks, 4);
    assert_eq!(metrics.first_pass_acceptance_rate, Some(0.75));
}

#[test]
fn measures_interventions_per_accepted_task() {
    let metrics = metrics_of(&recorded_project("interventions"));
    assert_eq!(metrics.interventions_per_accepted_task, Some(1.25));
}

#[test]
fn measures_cost_per_accepted_task_by_purpose() {
    let metrics = metrics_of(&recorded_project("cost"));
    let split = metrics
        .cost_per_accepted_task_usd
        .expect("a task was accepted");
    assert!((split.total - 1.5).abs() < f64::EPSILON, "{}", split.total);
    let by_purpose: Vec<(CostRecordedBodyPurpose, f64)> = split.by_purpose.into_iter().collect();
    assert_eq!(
        by_purpose,
        vec![
            (CostRecordedBodyPurpose::Triage, 0.0625),
            (CostRecordedBodyPurpose::Refine, 0.25),
            (CostRecordedBodyPurpose::Plan, 0.125),
            (CostRecordedBodyPurpose::Implement, 0.75),
            (CostRecordedBodyPurpose::Verify, 0.25),
            (CostRecordedBodyPurpose::Ceremony, 0.0),
            (CostRecordedBodyPurpose::Conversation, 0.0625),
        ]
    );
}

#[test]
fn counts_an_unpriced_report_as_a_session_at_no_cost() {
    let recorded = Recorded::new("unpriced");
    recorded.contract("FRK-1", &["command"]);
    recorded.created("FRK-1", "task", None);
    verified_once(&recorded, "FRK-1");
    recorded.cost(Some("FRK-1"), "implement", 2.0, "2026-09-22");
    recorded.unpriced_cost(Some("FRK-1"), "verify", "2026-09-22");

    let metrics = metrics_of(&recorded);
    let split = metrics
        .cost_per_accepted_task_usd
        .expect("a task was accepted");
    assert!((split.total - 2.0).abs() < f64::EPSILON, "{}", split.total);
    assert_eq!(
        split.by_purpose.get(&CostRecordedBodyPurpose::Verify),
        Some(&0.0)
    );
    assert_eq!(metrics.active_weeks, 1);
}

#[test]
fn measures_the_share_of_mechanically_verified_criteria() {
    let metrics = metrics_of(&recorded_project("criteria"));
    assert_eq!(metrics.mechanically_verified_criteria_share, Some(0.6));
}

#[test]
fn counts_active_weeks() {
    let metrics = metrics_of(&recorded_project("weeks"));
    assert_eq!(metrics.active_weeks, 3);
}

#[test]
fn says_none_for_every_rate_before_a_task_is_accepted() {
    let recorded = Recorded::new("none-yet");
    recorded.contract("FRK-2", &["command"]);
    recorded.created("FRK-2", "epic", None);
    recorded.walked("FRK-2", &TO_WORK);
    recorded.moved("FRK-2", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-2", "verifying", "accepted", "product_manager");
    recorded.created("FRK-1", "task", None);
    recorded.walked("FRK-1", &TO_WORK);
    recorded.escalated("FRK-1", "iterations");
    recorded.cost(Some("FRK-1"), "implement", 0.5, "2026-09-22");
    assert_eq!(
        metrics_of(&recorded),
        HarnessMetrics {
            accepted_tasks: 0,
            first_pass_acceptance_rate: None,
            interventions_per_accepted_task: None,
            cost_per_accepted_task_usd: None,
            mechanically_verified_criteria_share: None,
            active_weeks: 1,
        }
    );
}

#[test]
fn does_not_count_an_acceptance_without_a_verification_as_first_pass() {
    let recorded = Recorded::new("unverified");
    recorded.contract("FRK-1", &["command"]);
    recorded.created("FRK-1", "task", None);
    recorded.walked("FRK-1", &TO_WORK);
    recorded.moved("FRK-1", "in_progress", "escalated", "governor");
    recorded.moved("FRK-1", "escalated", "accepted", "human");
    let metrics = metrics_of(&recorded);
    assert_eq!(metrics.accepted_tasks, 1);
    assert_eq!(metrics.first_pass_acceptance_rate, Some(0.0));
    assert_eq!(metrics.mechanically_verified_criteria_share, Some(1.0));
}

#[test]
fn counts_neither_a_rejection_nor_a_second_verification_as_first_pass() {
    let recorded = Recorded::new("not-first-pass");
    for task in ["FRK-1", "FRK-2", "FRK-3"] {
        recorded.contract(task, &["command"]);
        recorded.created(task, "task", None);
        recorded.walked(task, &TO_WORK);
    }
    // Verified once, rejected, and accepted by the human resolving the escalation.
    recorded.moved("FRK-1", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-1", "verifying", "rejected", "reviewer");
    recorded.moved("FRK-1", "rejected", "escalated", "governor");
    recorded.moved("FRK-1", "escalated", "accepted", "human");
    // Verified twice and never rejected.
    recorded.moved("FRK-2", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-2", "verifying", "escalated", "governor");
    recorded.moved("FRK-2", "escalated", "in_progress", "human");
    recorded.moved("FRK-2", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-2", "verifying", "accepted", "product_manager");
    // Verified once and accepted: the one first pass.
    recorded.moved("FRK-3", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-3", "verifying", "accepted", "product_manager");

    let metrics = metrics_of(&recorded);
    assert_eq!(metrics.accepted_tasks, 3);
    assert_eq!(metrics.first_pass_acceptance_rate, Some(1.0 / 3.0));
}

#[test]
fn counts_the_turn_of_a_year_as_one_week() {
    let recorded = Recorded::new("new-year");
    recorded.cost(None, "conversation", 0.25, "2026-12-31");
    recorded.cost(None, "conversation", 0.25, "2027-01-01");
    assert_eq!(metrics_of(&recorded).active_weeks, 1);
    recorded.cost(None, "conversation", 0.25, "2027-01-04");
    assert_eq!(metrics_of(&recorded).active_weeks, 2);
}

#[test]
fn measures_one_sprint() {
    let recorded = Recorded::new("one-sprint");

    recorded.contract("FRK-1", &["command"]);
    recorded.created("FRK-1", "task", None);
    recorded.walked("FRK-1", &TO_WORK);
    recorded.sprint_started("S1", None);
    recorded.sprint_planned("S1", &["FRK-1"]);
    recorded.moved("FRK-1", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-1", "verifying", "accepted", "product_manager");
    recorded.cost(Some("FRK-1"), "implement", 1.0, "2026-09-22");
    recorded.sprint_ended("S1", &[]);

    recorded.contract("FRK-2", &["command"]);
    recorded.created("FRK-2", "task", None);
    recorded.walked("FRK-2", &TO_WORK);
    recorded.sprint_started("S2", None);
    recorded.sprint_planned("S2", &["FRK-2"]);
    recorded.moved("FRK-2", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-2", "verifying", "rejected", "reviewer");
    recorded.moved("FRK-2", "rejected", "in_progress", "governor");
    recorded.moved("FRK-2", "in_progress", "verifying", "assignee");
    recorded.moved("FRK-2", "verifying", "accepted", "product_manager");
    recorded.cost(Some("FRK-2"), "implement", 2.0, "2026-09-23");
    recorded.sprint_ended("S2", &[]);

    let s1 = recorded
        .metrics_for_sprint("S1")
        .expect("S1's metrics compute");
    assert_eq!(s1.accepted_tasks, 1);
    assert_eq!(s1.first_pass_acceptance_rate, Some(1.0));
    assert_eq!(
        s1.cost_per_accepted_task_usd
            .as_ref()
            .map(|split| split.total),
        Some(1.0)
    );

    let s2 = recorded
        .metrics_for_sprint("S2")
        .expect("S2's metrics compute");
    assert_eq!(s2.accepted_tasks, 1);
    assert_eq!(s2.first_pass_acceptance_rate, Some(0.0));
    assert_eq!(
        s2.cost_per_accepted_task_usd
            .as_ref()
            .map(|split| split.total),
        Some(2.0)
    );
}

#[test]
fn refuses_metrics_over_an_accepted_contract_it_cannot_read() {
    let recorded = recorded_project("unreadable");
    std::fs::remove_file(recorded.project.root.join(".farik/contracts/FRK-2.yaml"))
        .expect("the contract file is removed");
    let refusal = recorded.metrics().expect_err("FRK-2's contract is gone");
    assert!(
        matches!(&refusal, MetricsError::Files(FilesError::NotFound { path })
            if path.ends_with("contracts/FRK-2.yaml")),
        "{refusal:?}"
    );
}
