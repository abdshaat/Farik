//! Farik's events: `docs/schemas/event.schema.json` as Rust types, the reader that turns an
//! untrusted value into one, and the writer that turns one back.

use std::str::FromStr;
use std::sync::LazyLock;

use chrono::{DateTime, SecondsFormat, Utc};
use jsonschema::Validator;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

pub use farik_core::contract::{TaskId, ValidationError};

pub use crate::generated::event::{
    ContractLockedBody, ContractSummary, ContractSummaryKind, ContractSummaryParent,
    ContractSummaryRisk, ContractSummaryStatus, ContractUnlockedBody, ContractWrittenBody,
    CriteriaUpdatedBody, DriftDetectedBody, DriftDetectedBodyDrift, EventKind, ProjectScannedBody,
    RequestTriagedBody, RequestTriagedBodySize, TaskCreatedBody, TeamUpdatedBody,
};

use crate::generated::event::FarikEvent as EventWire;

/// Builders for test events, usable by every crate's tests.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("generated/event.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded event schema is valid JSON: it is a copy of docs/schemas/ written by \
         cargo xtask generate and checked for freshness by cargo xtask check",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded event schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// Every kind the log holds in this phase, in the order `docs/schemas/event.schema.json` lists
/// them. The step that adds a kind adds it here.
pub const EVERY_KIND: [EventKind; 9] = [
    EventKind::TaskCreated,
    EventKind::RequestTriaged,
    EventKind::ContractWritten,
    EventKind::ContractLocked,
    EventKind::ContractUnlocked,
    EventKind::DriftDetected,
    EventKind::ProjectScanned,
    EventKind::TeamUpdated,
    EventKind::CriteriaUpdated,
];

/// Everything one event records except what happened: where it belongs and when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    /// The event's place in the log, assigned by the store on append.
    pub seq: u64,
    /// When the event was recorded, from the injected clock.
    pub recorded_at: DateTime<Utc>,
    /// The team the event belongs to.
    pub team_id: String,
    /// The project the event belongs to.
    pub project_id: String,
    /// The contract the event is about, when it is about one.
    pub task_id: Option<TaskId>,
    /// The agent whose work produced the event, when an agent did.
    pub agent_id: Option<String>,
    /// The session the event was recorded in, when it was recorded in one.
    pub session_id: Option<String>,
}

/// What happened: one variant per event kind, each holding that kind's body.
#[derive(Debug, Clone, PartialEq)]
pub enum EventBody {
    /// A request was filed as a draft contract.
    TaskCreated(TaskCreatedBody),
    /// Triage sized a request.
    RequestTriaged(RequestTriagedBody),
    /// A contract's content was written or changed.
    ContractWritten(ContractWrittenBody),
    /// A human took ownership of a contract.
    ContractLocked(ContractLockedBody),
    /// A human gave a contract back to the team.
    ContractUnlocked(ContractUnlockedBody),
    /// Reconciliation found the files and the log disagreeing.
    DriftDetected(DriftDetectedBody),
    /// The project scan read the repository back to the user.
    ProjectScanned(ProjectScannedBody),
    /// The team file was written.
    TeamUpdated(TeamUpdatedBody),
    /// The criterion library was written.
    CriteriaUpdated(CriteriaUpdatedBody),
}

impl EventBody {
    /// The kind of event this body belongs to.
    #[must_use]
    pub fn kind(&self) -> EventKind {
        match self {
            Self::TaskCreated(_) => EventKind::TaskCreated,
            Self::RequestTriaged(_) => EventKind::RequestTriaged,
            Self::ContractWritten(_) => EventKind::ContractWritten,
            Self::ContractLocked(_) => EventKind::ContractLocked,
            Self::ContractUnlocked(_) => EventKind::ContractUnlocked,
            Self::DriftDetected(_) => EventKind::DriftDetected,
            Self::ProjectScanned(_) => EventKind::ProjectScanned,
            Self::TeamUpdated(_) => EventKind::TeamUpdated,
            Self::CriteriaUpdated(_) => EventKind::CriteriaUpdated,
        }
    }
}

/// One record of the log.
#[derive(Debug, Clone, PartialEq)]
pub struct FarikEvent {
    /// Where the event belongs and when it was recorded.
    pub envelope: EventEnvelope,
    /// What happened.
    pub body: EventBody,
}

/// Checks a value against `docs/schemas/event.schema.json` and, when it conforms, returns the
/// typed event. Refuses anything the schema refuses; refuses a body that does not belong to the
/// event's `kind`, which the schema cannot say; and refuses a blank `team_id` or `project_id`,
/// which name nobody. Trims every id and drops an optional one left blank.
///
/// # Errors
///
/// Every schema violation, in the schema's order rather than the input's key order; one error at
/// `/body` when the body does not fit the kind; one error at `/team_id` or `/project_id` for a
/// blank id; one at `/task_id` for an id the contract's pattern refuses; or one error at the root
/// when the schema passes but the typed event cannot be built.
pub fn event_from_value(input: &Value) -> Result<FarikEvent, Vec<ValidationError>> {
    let errors: Vec<ValidationError> = VALIDATOR
        .iter_errors(input)
        .map(|error| ValidationError {
            path: pointer(&error.instance_path().to_string()),
            message: error.to_string(),
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let wire = serde_json::from_value::<EventWire>(input.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!("the schema passed but the typed event could not be built: {error}"),
        }]
    })?;
    let envelope = envelope_from_wire(&wire)?;
    let body = body_from_value(wire.kind, &input["body"])?;
    Ok(FarikEvent { envelope, body })
}

fn envelope_from_wire(wire: &EventWire) -> Result<EventEnvelope, Vec<ValidationError>> {
    let team_id = required_id(&wire.team_id, "/team_id")?;
    let project_id = required_id(&wire.project_id, "/project_id")?;
    let task_id = match &wire.task_id {
        None => None,
        Some(id) => Some(TaskId::from_str(id.as_str()).map_err(|error| {
            vec![ValidationError {
                path: "/task_id".to_string(),
                message: error.to_string(),
            }]
        })?),
    };
    Ok(EventEnvelope {
        seq: wire.seq,
        recorded_at: wire.recorded_at,
        team_id,
        project_id,
        task_id,
        agent_id: optional_id(wire.agent_id.as_deref()),
        session_id: optional_id(wire.session_id.as_deref()),
    })
}

/// The body the kind says it is, read from the value. The wire enum answers "which body is this?"
/// by shape; the kind is what the event says it is, so the body is read by kind and one that does
/// not fit is refused.
fn body_from_value(kind: EventKind, body: &Value) -> Result<EventBody, Vec<ValidationError>> {
    Ok(match kind {
        EventKind::TaskCreated => EventBody::TaskCreated(read_body(body, kind)?),
        EventKind::RequestTriaged => EventBody::RequestTriaged(read_body(body, kind)?),
        EventKind::ContractWritten => EventBody::ContractWritten(read_body(body, kind)?),
        EventKind::ContractLocked => EventBody::ContractLocked(read_body(body, kind)?),
        EventKind::ContractUnlocked => EventBody::ContractUnlocked(read_body(body, kind)?),
        EventKind::DriftDetected => EventBody::DriftDetected(read_body(body, kind)?),
        EventKind::ProjectScanned => EventBody::ProjectScanned(read_body(body, kind)?),
        EventKind::TeamUpdated => EventBody::TeamUpdated(read_body(body, kind)?),
        EventKind::CriteriaUpdated => EventBody::CriteriaUpdated(read_body(body, kind)?),
    })
}

fn read_body<Body: DeserializeOwned>(
    body: &Value,
    kind: EventKind,
) -> Result<Body, Vec<ValidationError>> {
    serde_json::from_value::<Body>(body.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/body".to_string(),
            message: format!("a {kind} event does not carry this body: {error}"),
        }]
    })
}

fn required_id(value: &str, path: &str) -> Result<String, Vec<ValidationError>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(vec![ValidationError {
            path: path.to_string(),
            message: "is blank, and an event the log cannot attribute to a team and a project \
                      cannot be read back"
                .to_string(),
        }]);
    }
    Ok(trimmed.to_string())
}

fn optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|named| !named.is_empty())
        .map(ToString::to_string)
}

fn pointer(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

/// One event as the wire value the log holds, the inverse of `event_from_value`.
///
/// The crate writes the wire form itself, field by field, rather than deriving it: `serde_json`
/// answers with a `Result` whose error cannot happen for these types, and `docs/standards/code.md`
/// allows no `unwrap` or `expect` here, so the alternative is an impossible error on every caller
/// forever. The round-trip test is what keeps this honest.
#[must_use]
pub fn event_to_value(event: &FarikEvent) -> Value {
    let mut wire = Map::new();
    wire.insert("seq".to_string(), Value::from(event.envelope.seq));
    wire.insert(
        "recorded_at".to_string(),
        Value::String(
            event
                .envelope
                .recorded_at
                .to_rfc3339_opts(SecondsFormat::AutoSi, true),
        ),
    );
    wire.insert(
        "team_id".to_string(),
        Value::String(event.envelope.team_id.clone()),
    );
    wire.insert(
        "project_id".to_string(),
        Value::String(event.envelope.project_id.clone()),
    );
    if let Some(task_id) = &event.envelope.task_id {
        wire.insert("task_id".to_string(), Value::String(task_id.to_string()));
    }
    if let Some(agent_id) = &event.envelope.agent_id {
        wire.insert("agent_id".to_string(), Value::String(agent_id.clone()));
    }
    if let Some(session_id) = &event.envelope.session_id {
        wire.insert("session_id".to_string(), Value::String(session_id.clone()));
    }
    wire.insert(
        "kind".to_string(),
        Value::String(event.body.kind().to_string()),
    );
    wire.insert("body".to_string(), body_to_value(&event.body));
    Value::Object(wire)
}

fn body_to_value(body: &EventBody) -> Value {
    let mut wire = Map::new();
    match body {
        EventBody::TaskCreated(body) => {
            wire.insert("summary".to_string(), summary_to_value(&body.summary));
            wire.insert(
                "created_by".to_string(),
                Value::String(body.created_by.clone()),
            );
        }
        EventBody::RequestTriaged(body) => {
            wire.insert("size".to_string(), Value::String(body.size.to_string()));
            wire.insert("reason".to_string(), Value::String(body.reason.clone()));
            wire.insert(
                "triaged_by".to_string(),
                Value::String(body.triaged_by.clone()),
            );
        }
        EventBody::ContractWritten(body) => {
            wire.insert("summary".to_string(), summary_to_value(&body.summary));
            wire.insert(
                "written_by".to_string(),
                Value::String(body.written_by.clone()),
            );
        }
        EventBody::ContractLocked(body) => {
            wire.insert(
                "locked_by".to_string(),
                Value::String(body.locked_by.clone()),
            );
        }
        EventBody::ContractUnlocked(body) => {
            wire.insert(
                "unlocked_by".to_string(),
                Value::String(body.unlocked_by.clone()),
            );
        }
        EventBody::DriftDetected(body) => {
            wire.insert("drift".to_string(), Value::String(body.drift.to_string()));
            wire.insert("detail".to_string(), Value::String(body.detail.clone()));
        }
        EventBody::ProjectScanned(body) => {
            wire.insert(
                "read_back".to_string(),
                Value::String(body.read_back.clone()),
            );
            wire.insert(
                "detected_criteria".to_string(),
                strings(&body.detected_criteria),
            );
        }
        EventBody::TeamUpdated(body) => {
            wire.insert(
                "team_name".to_string(),
                Value::String(body.team_name.clone()),
            );
            wire.insert("agent_ids".to_string(), strings(&body.agent_ids));
            wire.insert(
                "updated_by".to_string(),
                Value::String(body.updated_by.clone()),
            );
        }
        EventBody::CriteriaUpdated(body) => {
            wire.insert(
                "criterion_names".to_string(),
                strings(&body.criterion_names),
            );
            wire.insert(
                "updated_by".to_string(),
                Value::String(body.updated_by.clone()),
            );
        }
    }
    Value::Object(wire)
}

fn summary_to_value(summary: &ContractSummary) -> Value {
    let mut wire = Map::new();
    wire.insert("kind".to_string(), Value::String(summary.kind.to_string()));
    if let Some(parent) = &summary.parent {
        wire.insert("parent".to_string(), Value::String(parent.to_string()));
    }
    wire.insert("title".to_string(), Value::String(summary.title.clone()));
    wire.insert(
        "status".to_string(),
        Value::String(summary.status.to_string()),
    );
    wire.insert("risk".to_string(), Value::String(summary.risk.to_string()));
    Value::Object(wire)
}

fn strings(values: &[String]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| Value::String(value.clone()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{a_body_wire, a_contract_summary_wire, a_full_event_wire, an_event_wire};
    use super::{
        EVERY_KIND, EventBody, EventKind, ValidationError, event_from_value, event_to_value,
    };

    fn refusal(input: &serde_json::Value) -> Vec<ValidationError> {
        event_from_value(input).expect_err("expected a refusal")
    }

    #[test]
    fn reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back() {
        for kind in EVERY_KIND {
            let event = event_from_value(&an_event_wire(kind)).expect("valid");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event.envelope.seq, 1);
            assert_eq!(event.envelope.team_id, "farik");
            assert!(event.envelope.task_id.is_none());
        }
    }

    #[test]
    fn reads_every_optional_field_of_the_envelope() {
        let event = event_from_value(&a_full_event_wire(EventKind::TaskCreated)).expect("valid");
        assert_eq!(
            event.envelope.task_id.as_ref().map(|id| id.to_string()),
            Some("FRK-1".to_string())
        );
        assert_eq!(event.envelope.agent_id.as_deref(), Some("maya-chen"));
        assert_eq!(event.envelope.session_id.as_deref(), Some("session-1"));
    }

    #[test]
    fn reads_the_summary_a_contract_event_carries() {
        let event = event_from_value(&an_event_wire(EventKind::TaskCreated)).expect("valid");
        let EventBody::TaskCreated(body) = event.body else {
            panic!("a task.created event carries a task.created body");
        };
        assert_eq!(body.created_by, "human");
        assert_eq!(body.summary.title, "Add a login page");
        assert_eq!(body.summary.status.to_string(), "draft");
        assert!(body.summary.parent.is_none());
    }

    #[test]
    fn refuses_a_body_that_belongs_to_another_kind() {
        // The schema cannot pair `kind` with `body`, so this is the reader's rule: without it a
        // contract.locked event could carry a task.created body into the log and every reader
        // after it would have to guess which one to believe.
        for kind in EVERY_KIND {
            let other = if kind == EventKind::TaskCreated {
                EventKind::ContractLocked
            } else {
                EventKind::TaskCreated
            };
            let mut input = an_event_wire(kind);
            input["body"] = a_body_wire(other);
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{kind}");
            assert_eq!(errors[0].path, "/body", "{kind}");
            assert!(
                errors[0]
                    .message
                    .starts_with(&format!("a {kind} event does not carry this body")),
                "{}",
                errors[0].message
            );
        }
    }

    #[test]
    fn refuses_a_kind_it_does_not_know() {
        let mut input = an_event_wire(EventKind::TaskCreated);
        input["kind"] = json!("task.exploded");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/kind");
    }

    #[test]
    fn refuses_a_value_that_is_not_an_event() {
        let errors = refusal(&json!("not an event"));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
    }

    #[test]
    fn refuses_an_unknown_property() {
        let mut input = an_event_wire(EventKind::TaskCreated);
        input["author"] = json!("someone");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
    }

    #[test]
    fn refuses_a_task_id_that_is_not_one() {
        let mut input = an_event_wire(EventKind::TaskCreated);
        input["task_id"] = json!("TASK-1");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/task_id");
    }

    #[test]
    fn refuses_a_blank_team_id_and_a_blank_project_id() {
        // The schema lets a string be empty; an event nobody can attribute to a team and a project
        // cannot be read back out of the log, so the reader refuses it here.
        for (field, path) in [("team_id", "/team_id"), ("project_id", "/project_id")] {
            let mut input = an_event_wire(EventKind::TaskCreated);
            input[field] = json!("   ");
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{field}");
            assert_eq!(errors[0].path, path);
            assert!(
                errors[0].message.starts_with("is blank"),
                "{}",
                errors[0].message
            );
        }
    }

    #[test]
    fn trims_the_ids_and_forgets_an_optional_one_that_is_blank() {
        let mut input = a_full_event_wire(EventKind::TaskCreated);
        input["team_id"] = json!("  farik  ");
        input["agent_id"] = json!("  maya-chen  ");
        input["session_id"] = json!("   ");
        let event = event_from_value(&input).expect("valid");
        assert_eq!(event.envelope.team_id, "farik");
        assert_eq!(event.envelope.agent_id.as_deref(), Some("maya-chen"));
        assert!(event.envelope.session_id.is_none());
    }

    #[test]
    fn reads_a_summary_that_names_a_parent() {
        let mut input = an_event_wire(EventKind::ContractWritten);
        let mut summary = a_contract_summary_wire();
        summary["parent"] = json!("FRK-3");
        input["body"]["summary"] = summary;
        let event = event_from_value(&input).expect("valid");
        let EventBody::ContractWritten(body) = event.body else {
            panic!("a contract.written event carries a contract.written body");
        };
        assert_eq!(
            body.summary.parent.as_ref().map(|id| id.to_string()),
            Some("FRK-3".to_string())
        );
    }

    #[test]
    fn writes_back_exactly_the_value_it_read_for_every_kind() {
        // The writer is hand-written, so this is what holds it to the schema the reader checks.
        for kind in EVERY_KIND {
            let wire = a_full_event_wire(kind);
            let event = event_from_value(&wire).expect("valid");
            assert_eq!(event_to_value(&event), wire, "{kind}");
        }
    }

    #[test]
    fn writes_a_summary_and_its_parent() {
        let mut input = an_event_wire(EventKind::ContractWritten);
        input["body"]["summary"]["parent"] = json!("FRK-3");
        let event = event_from_value(&input).expect("valid");
        assert_eq!(event_to_value(&event), input);
    }

    #[test]
    fn leaves_out_the_optional_fields_that_are_not_there() {
        let event = event_from_value(&an_event_wire(EventKind::ContractLocked)).expect("valid");
        let wire = event_to_value(&event);
        for absent in ["task_id", "agent_id", "session_id"] {
            assert!(wire.get(absent).is_none(), "{absent}");
        }
    }
}
