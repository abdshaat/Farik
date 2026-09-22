//! The commands the daemon accepts: `docs/schemas/command.schema.json` as Rust types, and the
//! reader that turns an untrusted value into one.

use std::str::FromStr;
use std::sync::LazyLock;

use farik_core::contract::validate_contract;
use jsonschema::Validator;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use farik_core::contract::{TaskContract, TaskId, ValidationError};

pub use crate::generated::command::CommandName;
use crate::generated::command::{
    FarikCommand as CommandWire, RequestTriageBody, RequestTriageBodySize, TaskCreateBody,
};

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/command.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded command schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded command schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// How big triage found a request (`docs/SPEC.md` section 5.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSize {
    /// Large: the request becomes an epic.
    Large,
    /// Small: the request becomes one standalone task.
    Small,
}

/// A request for the daemon to change something.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// File a contract as a draft request. The contract is boxed because it is an order of
    /// magnitude larger than every other command's arguments, and an enum is as large as its
    /// largest variant.
    TaskCreate {
        /// The contract, already held to every rule `validate_contract` applies.
        contract: Box<TaskContract>,
    },
    /// Record triage's decision, or the human's overrule of it.
    RequestTriage {
        /// The request being sized.
        task_id: TaskId,
        /// How big it is.
        size: RequestSize,
        /// Why, in the triager's words.
        reason: String,
    },
}

/// Checks a value against `docs/schemas/command.schema.json` and, when it conforms, returns the
/// typed command. The contract inside `task_create` goes through
/// `farik_core::contract::validate_contract`, so that a contract arriving inside a command is held
/// to exactly the rules one arriving alone is, the repeated id rules among them.
///
/// # Errors
///
/// Every schema violation, in the schema's order rather than the input's key order; one error at
/// `/body` when the body does not belong to the command; every violation the contract's own
/// validator reports, at its path under `/body/contract`; or one error at the root when the schema
/// passes but the typed command cannot be built.
pub fn command_from_value(input: &Value) -> Result<Command, Vec<ValidationError>> {
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
    let wire = serde_json::from_value::<CommandWire>(input.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!("the schema passed but the typed command could not be built: {error}"),
        }]
    })?;
    match wire.command {
        CommandName::TaskCreate => {
            let body: TaskCreateBody = read_body(&input["body"], CommandName::TaskCreate)?;
            let contract = validate_contract(&Value::Object(body.contract)).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| ValidationError {
                        path: under_contract(&error.path),
                        message: error.message,
                    })
                    .collect::<Vec<ValidationError>>()
            })?;
            Ok(Command::TaskCreate {
                contract: Box::new(contract),
            })
        }
        CommandName::RequestTriage => {
            let body: RequestTriageBody = read_body(&input["body"], CommandName::RequestTriage)?;
            let task_id = TaskId::from_str(body.task_id.as_str()).map_err(|error| {
                vec![ValidationError {
                    path: "/body/task_id".to_string(),
                    message: error.to_string(),
                }]
            })?;
            Ok(Command::RequestTriage {
                task_id,
                size: match body.size {
                    RequestTriageBodySize::Large => RequestSize::Large,
                    RequestTriageBodySize::Small => RequestSize::Small,
                },
                reason: body.reason,
            })
        }
    }
}

fn read_body<Body: DeserializeOwned>(
    body: &Value,
    command: CommandName,
) -> Result<Body, Vec<ValidationError>> {
    serde_json::from_value::<Body>(body.clone()).map_err(|error| {
        vec![ValidationError {
            path: "/body".to_string(),
            message: format!("a {command} command does not carry this body: {error}"),
        }]
    })
}

/// A contract's own error path, moved under the command that carried it. The validator reports the
/// contract's root as `/`, which under a command is the contract itself.
fn under_contract(path: &str) -> String {
    if path == "/" {
        "/body/contract".to_string()
    } else {
        format!("/body/contract{path}")
    }
}

fn pointer(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use farik_core::contract::fixtures::a_contract_wire;
    use serde_json::{Value, json};

    use super::{Command, RequestSize, ValidationError, command_from_value};

    fn a_task_create_wire() -> Value {
        json!({ "command": "task_create", "body": { "contract": a_contract_wire() } })
    }

    fn a_request_triage_wire() -> Value {
        json!({
            "command": "request_triage",
            "body": { "task_id": "FRK-1", "size": "large", "reason": "Three deliverables." }
        })
    }

    fn refusal(input: &Value) -> Vec<ValidationError> {
        command_from_value(input).expect_err("expected a refusal")
    }

    #[test]
    fn reads_a_task_create_command_through_the_contract_validator() {
        let Command::TaskCreate { contract } =
            command_from_value(&a_task_create_wire()).expect("valid")
        else {
            panic!("a task_create command carries a contract");
        };
        assert_eq!(contract.id.to_string(), "FRK-1");
        // The validator applied the schema's defaults, which is the proof it was the one used.
        assert_eq!(contract.budget.max_sessions.get(), 5);
    }

    #[test]
    fn reads_a_request_triage_command() {
        let Command::RequestTriage {
            task_id,
            size,
            reason,
        } = command_from_value(&a_request_triage_wire()).expect("valid")
        else {
            panic!("a request_triage command carries a size");
        };
        assert_eq!(task_id.to_string(), "FRK-1");
        assert_eq!(size, RequestSize::Large);
        assert_eq!(reason, "Three deliverables.");
    }

    #[test]
    fn refuses_a_contract_the_contract_schema_refuses_and_says_where() {
        // The repeated criterion id rule of phase 1 lives in validate_contract; a contract that
        // arrives inside a command is held to it too, and its path is reported under the body.
        let mut input = a_task_create_wire();
        let twin = input["body"]["contract"]["exit_criteria"][0].clone();
        input["body"]["contract"]["exit_criteria"] = json!([twin.clone(), twin]);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/contract/exit_criteria");
        assert!(
            errors[0].message.starts_with("the id C1 names"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn reports_a_contract_refused_at_its_root_under_the_body() {
        let mut input = a_task_create_wire();
        input["body"]["contract"]["owner"] = json!("someone");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/contract");
    }

    #[test]
    fn refuses_a_body_that_belongs_to_another_command() {
        let mut input = a_task_create_wire();
        input["body"] = a_request_triage_wire()["body"].clone();
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body");
        assert!(
            errors[0]
                .message
                .starts_with("a task_create command does not carry this body"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn refuses_a_command_it_does_not_know() {
        let mut input = a_task_create_wire();
        input["command"] = json!("task_delete");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/command");
    }

    #[test]
    fn refuses_a_value_that_is_not_a_command() {
        let errors = refusal(&json!(["task_create"]));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/");
    }
}
