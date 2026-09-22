//! Filing a request (F3, `docs/SPEC.md` section 5.16): one piece of code for the command line and
//! for the agents' `farik_create_task`, so that a request is filed exactly one way whoever files it.
//! Beside it, the JSON the board, the rules, and the criterion library are shown as, for the same
//! reason: `farik board --json` and `farik_read_board` say one thing.

use std::fmt;

use chrono::{DateTime, Utc};
use farik_core::contract::{TaskContract, TaskId};
use farik_core::criteria::{CriteriaLibrary, TemplateVerification};
use farik_core::governor::gates::{
    FIELDS_FIXED_AT_CREATION, FIELDS_ONLY_THE_HUMAN_WRITES, FIELDS_THE_GOVERNOR_WRITES,
    FIELDS_THE_STORE_OWNS,
};
use farik_core::governor::team_rules::TeamRules;
use farik_protocol::command::{Command, command_from_value};
use farik_protocol::event::{
    ContractSummary, EventBody, EventIds, RequestTriagedBody, RequestTriagedBodySize,
    TaskCreatedBody, new_event,
};
use serde_json::{Value, json};

use crate::files::{FilesError, ProjectFiles};
use crate::{EventLog, StoreError, TaskProjection};

/// Why a request was not filed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    /// The request is not one Farik files, in words a person or an agent can act on.
    Refused {
        /// What is wrong with it.
        reason: String,
    },
    /// The contract file could not be written.
    Files(FilesError),
    /// The log refused.
    Store(StoreError),
}

impl fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { reason } => write!(formatter, "{reason}"),
            Self::Files(error) => write!(formatter, "{error}"),
            Self::Store(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for RequestError {}

impl From<FilesError> for RequestError {
    fn from(error: FilesError) -> Self {
        Self::Files(error)
    }
}

impl From<StoreError> for RequestError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// The fields of `wire` its author may not write: the store's, the governor's, the human's lock,
/// and the two the triage and the epic's assignee decide (`docs/SPEC.md` section 5.11). Refused
/// rather than overwritten: a filing that quietly replaced what somebody wrote would make the file
/// and the contract two different things.
#[must_use]
pub fn fields_not_the_authors(wire: &Value) -> Vec<&'static str> {
    let Some(object) = wire.as_object() else {
        return Vec::new();
    };
    FIELDS_THE_STORE_OWNS
        .iter()
        .chain(&FIELDS_THE_GOVERNOR_WRITES)
        .chain(&FIELDS_ONLY_THE_HUMAN_WRITES)
        .chain(&FIELDS_FIXED_AT_CREATION)
        .copied()
        .filter(|field| object.contains_key(*field))
        .collect()
}

/// Gives the request in `wire` the next id and files it as a `draft`: the contract is written and
/// `task.created` appended, stamped with `ids` (whose `task_id` is replaced by the new one). With a
/// `parent`, it is a `task` of that epic, and since an epic's tasks are triaged by its breakdown
/// (5.16 item 3) `request.triaged { size: small }` follows, by `created_by`. Whether a child may be
/// written under that epic at all is `check_child_creation`'s question, asked by the caller.
///
/// The contract goes through `command_from_value`, so that a request filed from a terminal is held
/// to exactly the rules one arriving from an agent is.
///
/// # Errors
///
/// `Refused` when `wire` is not a mapping, sets a field that is not its author's, or breaks a
/// contract rule; `Files` or `Store` when the contract or the events cannot be written.
pub fn file_request(
    files: &ProjectFiles,
    log: &EventLog,
    mut wire: Value,
    created_by: &str,
    parent: Option<&TaskId>,
    now: DateTime<Utc>,
    ids: &EventIds,
) -> Result<TaskContract, RequestError> {
    let written = fields_not_the_authors(&wire);
    if !written.is_empty() {
        return Err(RequestError::Refused {
            reason: format!(
                "sets {}, which a request does not: Farik assigns the id and the stamps, the \
                 governor writes the lifecycle, the lock is the human's lock to take, triage \
                 decides whether this is an epic, and a task's parent is set by the epic's \
                 assignee when it breaks the epic down (5.16)",
                written.join(", ")
            ),
        });
    }
    let Some(object) = wire.as_object_mut() else {
        return Err(RequestError::Refused {
            reason: "is not a contract: a contract is a mapping".to_string(),
        });
    };

    let task_id = log.next_task_id()?;
    let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    object.insert("id".to_string(), json!(task_id.to_string()));
    object.insert("status".to_string(), json!("draft"));
    object.insert("created_by".to_string(), json!(created_by));
    object.insert("created_at".to_string(), json!(stamp));
    object.insert("updated_at".to_string(), json!(stamp));
    if let Some(parent) = parent {
        object.insert("kind".to_string(), json!("task"));
        object.insert("parent".to_string(), json!(parent.to_string()));
    }

    let command = command_from_value(&json!({
        "command": "task_create",
        "body": { "contract": wire }
    }))
    .map_err(|errors| RequestError::Refused {
        reason: format!(
            "is not a contract Farik can file: {}",
            errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    })?;
    let Command::TaskCreate { contract } = command else {
        return Err(RequestError::Refused {
            reason: "was read back as a command other than the one built from it".to_string(),
        });
    };

    files.write_contract(&contract)?;
    let ids = EventIds {
        task_id: Some(contract.id.clone()),
        ..ids.clone()
    };
    let mut bodies = vec![EventBody::TaskCreated(TaskCreatedBody {
        created_by: created_by.to_string(),
        summary: summary_of(&contract),
    })];
    if let Some(parent) = parent {
        bodies.push(EventBody::RequestTriaged(RequestTriagedBody {
            size: RequestTriagedBodySize::Small,
            reason: format!("a task of {}", parent.as_str()),
            triaged_by: created_by.to_string(),
        }));
    }
    for body in bodies {
        let event = new_event(body, now, ids.clone()).map_err(|error| RequestError::Refused {
            reason: format!("cannot be recorded: {error:?}"),
        })?;
        log.append(&event)?;
    }
    Ok(*contract)
}

/// The fields of the contract the board shows, taken from the contract itself so that the log can
/// be replayed into projections without the files (`docs/SPEC.md` section 8.4).
///
/// # Panics
///
/// Never: a contract's own kind, risk, status and title are the summary's, and both vocabularies
/// come from `task-contract.schema.json`, which a test in `farik-protocol` holds to agreeing.
#[must_use]
pub fn summary_of(contract: &TaskContract) -> ContractSummary {
    let mut value = json!({
        "kind": contract.kind.to_string(),
        "risk": contract.risk.to_string(),
        "status": contract.status.to_string(),
        "title": contract.title,
    });
    // An absent `parent` is absent rather than null: the summary's schema says `parent` is a
    // string when it is there at all.
    if let Some(parent) = &contract.parent {
        value["parent"] = json!(parent.as_str());
    }
    serde_json::from_value(value).expect(
        "a contract's own kind, risk, status and title are the summary's, and both vocabularies \
         come from task-contract.schema.json, which a test in farik-protocol holds to agreeing",
    )
}

/// One board row as JSON, as `farik board --json` prints it.
#[must_use]
pub fn board_row_json(row: &TaskProjection) -> Value {
    json!({
        "task_id": row.task_id.as_str(),
        "kind": row.kind.to_string(),
        "parent": row.parent.as_ref().map(|parent| parent.as_str().to_string()),
        "title": row.title,
        "status": row.status.to_string(),
        "risk": row.risk.to_string(),
        "triaged": row.triaged,
        "locked": row.locked,
    })
}

/// The board as JSON: `{ "tasks": [...] }`, one row each, in the order given.
#[must_use]
pub fn board_json(rows: &[TaskProjection]) -> Value {
    json!({ "tasks": rows.iter().map(board_row_json).collect::<Vec<_>>() })
}

/// The rules the governor applies as JSON, as `farik rules show --json` prints them.
#[must_use]
pub fn rules_json(rules: &TeamRules) -> Value {
    json!({
        "protected_paths": rules.protected_paths,
        "allowed_paths_ceiling": rules.allowed_paths_ceiling,
        "required_criteria": rules.required_criteria,
        "require_new_tests": rules.require_new_tests,
        "max_task_budget_usd": rules.max_task_budget_usd,
        "forbidden_commands": rules.forbidden_commands,
    })
}

/// The criterion library as JSON, as `farik criteria list --json` prints it.
#[must_use]
pub fn criteria_json(library: &CriteriaLibrary) -> Value {
    json!({
        "criteria": library
            .criteria
            .iter()
            .map(|one| json!({
                "name": one.name.as_str(),
                "text": one.text.as_str(),
                "source": one.source.as_ref().map(ToString::to_string),
                "method": criterion_method(&one.verification),
                "how": criterion_how(&one.verification),
            }))
            .collect::<Vec<_>>(),
    })
}

/// How a criterion is verified, in the one word the schema's `method` carries.
#[must_use]
pub fn criterion_method(verification: &TemplateVerification) -> &'static str {
    match verification {
        TemplateVerification::Variant0 { .. } => "command",
        TemplateVerification::Variant1 { .. } => "test",
        TemplateVerification::Variant2 { .. } => "artifact",
        TemplateVerification::Variant3 { .. } => "review",
        TemplateVerification::Variant4 { .. } => "human",
    }
}

/// What is actually run, read or asked, which is the part a person checks.
#[must_use]
pub fn criterion_how(verification: &TemplateVerification) -> String {
    match verification {
        TemplateVerification::Variant0 { command, .. }
        | TemplateVerification::Variant1 { command, .. } => command.clone(),
        TemplateVerification::Variant2 { path, .. } => path.clone(),
        TemplateVerification::Variant3 { rubric, .. } => rubric.join("; "),
        TemplateVerification::Variant4 { question, .. } => question.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use chrono::{DateTime, TimeZone, Utc};
    use farik_core::contract::fixtures::a_contract_wire;
    use farik_core::contract::{TaskId, TaskKind, TaskStatus};
    use farik_protocol::event::{EventBody, EventIds, EventKind, RequestTriagedBodySize};
    use serde_json::Value;

    use super::{RequestError, file_request};
    use crate::files::fixtures::{TempProject, a_team};
    use crate::{EventLog, EventQuery, IN_MEMORY, open_event_log, open_projections};

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 10, 0, 0)
            .single()
            .expect("a real time")
    }

    fn ids() -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            ..EventIds::default()
        }
    }

    /// A request as its author writes it: the fixture contract without what Farik fills in.
    fn a_request() -> Value {
        let mut wire = a_contract_wire();
        let object = wire.as_object_mut().expect("the fixture is a mapping");
        object.remove("id");
        object.remove("status");
        wire
    }

    fn a_project(name: &str) -> (TempProject, Arc<EventLog>) {
        let project = TempProject::new(name);
        project.files().init(&a_team()).expect(".farik/ is made");
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens"));
        (project, log)
    }

    fn kinds(log: &EventLog) -> Vec<EventKind> {
        log.read(&EventQuery::default())
            .expect("the log reads")
            .iter()
            .map(|event| event.body.kind())
            .collect()
    }

    #[test]
    fn files_a_request_with_the_next_id_the_log_hands_out() {
        let (project, log) = a_project("requests-next");
        let files = project.files();
        let request = file_request(&files, &log, a_request(), "human", None, at(), &ids())
            .expect("the request is filed");

        assert_eq!(request.id.as_str(), "FRK-1");
        assert_eq!(request.status, TaskStatus::Draft);
        assert_eq!(
            files.read_contract(&request.id).expect("it is on disk"),
            request
        );
        assert_eq!(kinds(&log), vec![EventKind::TaskCreated]);
    }

    #[test]
    fn files_a_child_as_a_triaged_task_of_its_epic() {
        let (project, log) = a_project("requests-child");
        let files = project.files();
        let epic = TaskId::try_from("FRK-1").expect("an id");
        let request = file_request(&files, &log, a_request(), "pm", Some(&epic), at(), &ids())
            .expect("the child is filed");

        assert_eq!(request.kind, TaskKind::Task);
        assert_eq!(request.parent.as_ref().map(|p| p.as_str()), Some("FRK-1"));
        assert_eq!(request.status, TaskStatus::Draft);
        let events = log.read(&EventQuery::default()).expect("the log reads");
        assert_eq!(
            kinds(&log),
            vec![EventKind::TaskCreated, EventKind::RequestTriaged]
        );
        let EventBody::RequestTriaged(body) = &events[1].body else {
            panic!("the second event is the triage");
        };
        assert_eq!(body.size, RequestTriagedBodySize::Small);
        assert_eq!(body.triaged_by, "pm");
        assert_eq!(
            events[1].envelope.ids.task_id.as_ref(),
            Some(&request.id),
            "the triage is about the child"
        );
        let board = open_projections(Arc::clone(&log))
            .expect("the projections open")
            .board()
            .expect("the board reads");
        assert!(board[0].triaged);
        assert_eq!(board[0].parent.as_ref(), Some(&epic));
    }

    #[test]
    fn refuses_a_request_that_sets_an_id() {
        let (project, log) = a_project("requests-id");
        let files = project.files();
        let mut wire = a_request();
        wire["id"] = serde_json::json!("FRK-9");
        let refused = file_request(&files, &log, wire, "human", None, at(), &ids())
            .expect_err("an id is Farik's to give");

        let RequestError::Refused { reason } = refused else {
            panic!("a refusal, not a failure: {refused:?}");
        };
        assert!(reason.starts_with("sets id,"), "{reason}");
        assert!(kinds(&log).is_empty(), "nothing is recorded");
        assert!(
            files.list_contracts().expect("a list").is_empty(),
            "nothing is filed"
        );
    }
}
