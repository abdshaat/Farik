//! `farik task create`: file a YAML contract as a draft request (F3, `docs/SPEC.md` section 5.16).

use std::path::Path;

use chrono::{DateTime, Utc};
use farik_core::contract::TaskContract;
use farik_core::governor::gates::{
    FIELDS_FIXED_AT_CREATION, FIELDS_ONLY_THE_HUMAN_WRITES, FIELDS_THE_GOVERNOR_WRITES,
    FIELDS_THE_STORE_OWNS,
};
use farik_protocol::command::{Command, command_from_value};
use farik_protocol::event::EventBody;
use farik_protocol::generated::event::TaskCreatedBody;
use serde_json::{Value, json};

use crate::project::Project;
use crate::{HUMAN, Report};

/// The fields a person writing a request does not fill in: the store's, the governor's, the human's
/// lock, and the two the triage and the epic's assignee decide (`docs/SPEC.md` section 5.11).
///
/// Refused rather than overwritten: a command that quietly replaced what somebody wrote would make
/// the file and the contract two different things.
///
/// `parent` is the one of these a person may legitimately write: 5.16 item 3 lets the human create a
/// task under an epic. The gate for that is `check_child_creation`, and it takes a `ParentEpic` whose
/// `assignee_id` this command cannot fill: no projection carries one until phase 3 step 03. For a
/// human actor the gate reads only the epic's status, so a blank id would pass today — and a
/// governance predicate asked with a field invented at the call site is worse than one not asked yet,
/// because the branch that field feeds is the branch that says who may write the task at all. So the
/// refusal says which rule refuses `parent` and what will open it, and the project plan records it.
fn not_the_authors() -> Vec<&'static str> {
    let mut fields: Vec<&'static str> = Vec::new();
    fields.extend(FIELDS_THE_STORE_OWNS);
    fields.extend(FIELDS_THE_GOVERNOR_WRITES);
    fields.extend(FIELDS_ONLY_THE_HUMAN_WRITES);
    fields.extend(FIELDS_FIXED_AT_CREATION);
    fields
}

/// Reads the contract in `file`, gives it the next id, and files it as a `draft` request.
///
/// The contract goes through `command_from_value` rather than through `validate_contract` alone, so
/// that a contract filed from a terminal is held to exactly the rules one arriving from an agent is.
///
/// `file` is taken as the person typed it: absolute as it is, relative to `cwd`, which is where the
/// command was run.
///
/// # Errors
///
/// A sentence saying the file could not be read, that it is not YAML, that it carries a field it is
/// not the author's to write, every rule the contract breaks, or what could not be written.
pub fn create(
    project: &Project,
    cwd: &Path,
    file: &Path,
    now: DateTime<Utc>,
) -> Result<Report, String> {
    // A path a person typed is relative to where they typed it, and `cwd` is where that was: the
    // project is the whole repository, so the directory the command was run in is not where the
    // project is, and reading the file through the process's own current directory would make the
    // library answer differently from the binary.
    let file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };
    let text = std::fs::read_to_string(&file)
        .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
    let mut wire = farik_store::files::yaml_value(&text, &file.display().to_string())
        .map_err(|error| error.to_string())?;
    let object = wire.as_object_mut().ok_or_else(|| {
        format!(
            "{} is not a contract: a contract is a mapping",
            file.display()
        )
    })?;

    let written: Vec<&'static str> = not_the_authors()
        .into_iter()
        .filter(|field| object.contains_key(*field))
        .collect();
    if !written.is_empty() {
        return Err(format!(
            "{} sets {}, which a request does not: Farik assigns the id and the stamps, the \
             governor writes the lifecycle, the lock is yours to take with farik contract lock, \
             farik triage decides whether this is an epic, and a task's parent is set by the epic's \
             assignee when it breaks the epic down (5.16)",
            file.display(),
            written.join(", ")
        ));
    }

    // Past every contract the repository already holds as well as the counter: the log is
    // machine-local and the contracts are committed (8.4), so on a fresh clone the counter alone
    // would hand out FRK-1 again. `list_contracts` is in number order, so the last is the highest.
    let taken = project
        .files
        .list_contracts()
        .map_err(|error| error.to_string())?
        .last()
        .and_then(|id| id.as_str().trim_start_matches("FRK-").parse::<u64>().ok())
        .unwrap_or(0);
    let task_id = project
        .log
        .next_task_id_above(taken)
        .map_err(|error| error.to_string())?;
    let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    object.insert("id".to_string(), json!(task_id.to_string()));
    object.insert("status".to_string(), json!("draft"));
    object.insert("created_by".to_string(), json!(HUMAN));
    object.insert("created_at".to_string(), json!(stamp));
    object.insert("updated_at".to_string(), json!(stamp));

    let command = command_from_value(&json!({
        "command": "task_create",
        "body": { "contract": wire }
    }))
    .map_err(|errors| {
        format!(
            "{} is not a contract Farik can file: {}",
            file.display(),
            errors
                .iter()
                .map(|error| format!("{} {}", error.path, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    let Command::TaskCreate { contract } = command else {
        return Err("the command line built a command the reader did not read back".to_string());
    };

    project
        .files
        .create_contract(&contract)
        .map_err(|error| error.to_string())?;
    let event = project.event(
        EventBody::TaskCreated(TaskCreatedBody {
            created_by: HUMAN.to_string(),
            summary: summary_of(&contract),
        }),
        now,
        Some(contract.id.clone()),
    )?;
    let seq = project.append(&event)?;

    Ok(Report {
        lines: vec![
            format!(
                "{} filed as a draft request: {}",
                contract.id.as_str(),
                contract.title.as_str()
            ),
            "farik triage says whether it is large or small; nothing starts before that \
             (5.16)"
                .to_string(),
        ],
        json: json!({
            "task_id": contract.id.to_string(),
            "title": contract.title,
            "status": "draft",
            "path": format!(".farik/contracts/{}.yaml", contract.id.as_str()),
            "events": [seq],
        }),
        json_lines: None,
    })
}

/// The fields of the contract the board shows, taken from the contract itself so that the log can
/// be replayed into projections without the files (`docs/SPEC.md` section 8.4).
fn summary_of(contract: &TaskContract) -> farik_protocol::generated::event::ContractSummary {
    let value = json!({
        "kind": contract.kind.to_string(),
        "parent": contract.parent.as_ref().map(|parent| parent.as_str().to_string()),
        "risk": contract.risk.to_string(),
        "status": contract.status.to_string(),
        "title": contract.title,
    });
    serde_json::from_value(strip_nulls(value)).expect(
        "a contract's own kind, risk, status and title are the summary's, and both vocabularies \
         come from task-contract.schema.json, which a test in farik-protocol holds to agreeing",
    )
}

/// An absent `parent` is absent rather than null: the summary's schema says `parent` is a string
/// when it is there at all.
fn strip_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, value)| !value.is_null())
                .collect(),
        ),
        other => other,
    }
}
