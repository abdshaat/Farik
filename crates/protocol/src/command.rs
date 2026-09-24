//! The commands the daemon accepts: `docs/schemas/command.schema.json` as Rust types, and the
//! reader that turns an untrusted value into one.

use std::str::FromStr;
use std::sync::LazyLock;

use farik_core::contract::{TaskStatus, validate_contract};
use farik_core::team::AgentStatus;
use jsonschema::Validator;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use farik_core::contract::{TaskContract, TaskId, ValidationError};

pub use crate::generated::command::CommandName;
use crate::generated::command::{
    AgentUpdateBody, EmptyBody, EscalationResolveBody, FarikCommand as CommandWire,
    HumanAcceptBody, HumanAcceptBodySubject, MessagePostBody, QuestionAnswerBody,
    RequestTriageBody, RequestTriageBodySize, SessionStopBody, SprintStartBody, TaskCreateBody,
    TaskIdBody, TaskTransitionBody,
};

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/command.schema.json");

/// The embedded command schema, parsed.
fn schema() -> Value {
    serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded command schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    )
}

/// Every violation `validator` finds in `input`, at its path.
fn check(validator: &Validator, input: &Value) -> Result<(), Vec<ValidationError>> {
    let errors: Vec<ValidationError> = validator
        .iter_errors(input)
        .map(|error| ValidationError {
            path: pointer(&error.instance_path().to_string()),
            message: error.to_string(),
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema = schema();
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

/// What the human accepts with `HumanAccept` (`docs/SPEC.md` sections 5.4 and 5.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptSubject {
    /// The contract awaiting approval.
    Contract,
    /// The result awaiting the human's acceptance.
    Result,
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
    /// Move a task as the human, from any status but `escalated`.
    TaskTransition {
        /// The task to move.
        task_id: TaskId,
        /// Where to.
        to: TaskStatus,
        /// Why, in the human's words.
        reason: String,
    },
    /// Approve a contract awaiting approval, or accept a result that waits for the human.
    HumanAccept {
        /// The task.
        task_id: TaskId,
        /// Whether the contract or the result is accepted.
        subject: AcceptSubject,
        /// The human's words, which an epic's result needs.
        message: Option<String>,
    },
    /// Resolve an escalation by moving the task out of `escalated`.
    EscalationResolve {
        /// The escalated task.
        task_id: TaskId,
        /// Where it goes.
        to: TaskStatus,
        /// What the next session about it is told.
        message: String,
    },
    /// Answer a question an agent asked.
    QuestionAnswer {
        /// The sequence number of its `question.asked`.
        question_id: u64,
        /// The answer.
        answer: String,
    },
    /// Take a contract for the human.
    ContractLock {
        /// The task whose contract is taken.
        task_id: TaskId,
    },
    /// Give a contract back to the team.
    ContractUnlock {
        /// The task whose contract is given back.
        task_id: TaskId,
    },
    /// Integrate an accepted task now.
    TaskIntegrate {
        /// The accepted task.
        task_id: TaskId,
    },
    /// Change an agent's status.
    AgentUpdate {
        /// The agent.
        agent_id: String,
        /// Its new status.
        status: AgentStatus,
    },
    /// Stop a running session.
    SessionStop {
        /// The session.
        session_id: String,
    },
    /// Stop the run between ticks.
    RunStop,
    /// Start a sprint.
    SprintStart {
        /// What it may spend, or nothing for no budget of its own.
        budget_usd: Option<f64>,
    },
    /// End the open sprint.
    SprintEnd,
    /// Say something in the team's channel.
    MessagePost {
        /// What the human says.
        text: String,
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
    check(&VALIDATOR, input)?;
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
            Ok(Command::RequestTriage {
                task_id: task_id_of(body.task_id.as_str())?,
                size: match body.size {
                    RequestTriageBodySize::Large => RequestSize::Large,
                    RequestTriageBodySize::Small => RequestSize::Small,
                },
                reason: body.reason,
            })
        }
        name => human_command(name, &input["body"]),
    }
}

/// The human's commands, each read from its own body shape.
fn human_command(name: CommandName, body: &Value) -> Result<Command, Vec<ValidationError>> {
    match name {
        CommandName::TaskCreate | CommandName::RequestTriage => unreachable!(
            "command_from_value reads task_create and request_triage before it asks this"
        ),
        CommandName::TaskTransition => {
            let body: TaskTransitionBody = read_body(body, name)?;
            Ok(Command::TaskTransition {
                task_id: task_id_of(body.task_id.as_str())?,
                to: status_of(&body.to.to_string())?,
                reason: body.reason,
            })
        }
        CommandName::HumanAccept => {
            let body: HumanAcceptBody = read_body(body, name)?;
            Ok(Command::HumanAccept {
                task_id: task_id_of(body.task_id.as_str())?,
                subject: match body.subject {
                    HumanAcceptBodySubject::Contract => AcceptSubject::Contract,
                    HumanAcceptBodySubject::Result => AcceptSubject::Result,
                },
                message: body.message,
            })
        }
        CommandName::EscalationResolve => {
            let body: EscalationResolveBody = read_body(body, name)?;
            Ok(Command::EscalationResolve {
                task_id: task_id_of(body.task_id.as_str())?,
                to: status_of(&body.to.to_string())?,
                message: body.message,
            })
        }
        CommandName::QuestionAnswer => {
            let body: QuestionAnswerBody = read_body(body, name)?;
            Ok(Command::QuestionAnswer {
                question_id: body.question_id.get(),
                answer: body.answer,
            })
        }
        CommandName::ContractLock | CommandName::ContractUnlock | CommandName::TaskIntegrate => {
            let body: TaskIdBody = read_body(body, name)?;
            let task_id = task_id_of(body.task_id.as_str())?;
            Ok(match name {
                CommandName::ContractLock => Command::ContractLock { task_id },
                CommandName::ContractUnlock => Command::ContractUnlock { task_id },
                _ => Command::TaskIntegrate { task_id },
            })
        }
        CommandName::AgentUpdate => {
            let body: AgentUpdateBody = read_body(body, name)?;
            let status = body.status.to_string();
            Ok(Command::AgentUpdate {
                agent_id: body.agent_id.to_string(),
                status: AgentStatus::from_str(&status).map_err(|_| {
                    vec![ValidationError {
                        path: "/body/status".to_string(),
                        message: format!("{status} is not an agent's status"),
                    }]
                })?,
            })
        }
        CommandName::SessionStop => {
            let body: SessionStopBody = read_body(body, name)?;
            Ok(Command::SessionStop {
                session_id: body.session_id.to_string(),
            })
        }
        CommandName::RunStop | CommandName::SprintEnd => {
            let _: EmptyBody = read_body(body, name)?;
            Ok(if name == CommandName::RunStop {
                Command::RunStop
            } else {
                Command::SprintEnd
            })
        }
        CommandName::SprintStart => {
            let body: SprintStartBody = read_body(body, name)?;
            Ok(Command::SprintStart {
                budget_usd: body.budget_usd,
            })
        }
        CommandName::MessagePost => {
            let body: MessagePostBody = read_body(body, name)?;
            Ok(Command::MessagePost { text: body.text })
        }
    }
}

/// Writes a command as the wire `command_from_value` reads it back from: the inverse of the reader,
/// so that a command built in one process reaches another's daemon exactly as it was built. A
/// `human_accept` with no message is written without one.
///
/// # Panics
///
/// Never: a contract's `Serialize` is derived from its schema with string keys.
#[must_use]
pub fn command_to_value(command: &Command) -> Value {
    let (name, body) = match command {
        Command::TaskCreate { contract } => (
            CommandName::TaskCreate,
            json!({
                "contract": serde_json::to_value(contract.as_ref()).expect(
                    "a contract's Serialize is derived from its schema with string keys, so it \
                     cannot fail"
                )
            }),
        ),
        Command::RequestTriage {
            task_id,
            size,
            reason,
        } => (
            CommandName::RequestTriage,
            json!({
                "task_id": task_id.as_str(),
                "size": match size {
                    RequestSize::Large => "large",
                    RequestSize::Small => "small",
                },
                "reason": reason,
            }),
        ),
        Command::TaskTransition {
            task_id,
            to,
            reason,
        } => (
            CommandName::TaskTransition,
            json!({ "task_id": task_id.as_str(), "to": to.to_string(), "reason": reason }),
        ),
        Command::HumanAccept {
            task_id,
            subject,
            message,
        } => {
            let mut body = json!({
                "task_id": task_id.as_str(),
                "subject": match subject {
                    AcceptSubject::Contract => "contract",
                    AcceptSubject::Result => "result",
                },
            });
            if let Some(message) = message {
                body["message"] = json!(message);
            }
            (CommandName::HumanAccept, body)
        }
        Command::EscalationResolve {
            task_id,
            to,
            message,
        } => (
            CommandName::EscalationResolve,
            json!({ "task_id": task_id.as_str(), "to": to.to_string(), "message": message }),
        ),
        Command::QuestionAnswer {
            question_id,
            answer,
        } => (
            CommandName::QuestionAnswer,
            json!({ "question_id": question_id, "answer": answer }),
        ),
        Command::ContractLock { task_id } => (
            CommandName::ContractLock,
            json!({ "task_id": task_id.as_str() }),
        ),
        Command::ContractUnlock { task_id } => (
            CommandName::ContractUnlock,
            json!({ "task_id": task_id.as_str() }),
        ),
        Command::TaskIntegrate { task_id } => (
            CommandName::TaskIntegrate,
            json!({ "task_id": task_id.as_str() }),
        ),
        Command::AgentUpdate { agent_id, status } => (
            CommandName::AgentUpdate,
            json!({ "agent_id": agent_id, "status": status.to_string() }),
        ),
        Command::SessionStop { session_id } => (
            CommandName::SessionStop,
            json!({ "session_id": session_id }),
        ),
        Command::RunStop => (CommandName::RunStop, json!({})),
        Command::SprintStart { budget_usd } => (
            CommandName::SprintStart,
            json!({ "budget_usd": budget_usd }),
        ),
        Command::SprintEnd => (CommandName::SprintEnd, json!({})),
        Command::MessagePost { text } => (CommandName::MessagePost, json!({ "text": text })),
    };
    json!({ "command": name.to_string(), "body": body })
}

/// Why a command did nothing, as the reply names it (`$defs/commandReply`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    /// The command cannot be handled as given; a schema refusal is one.
    Invalid,
    /// It cannot be done to what it names as things stand.
    Refused,
    /// What it names is not there.
    NotFound,
    /// The store, a file, git, or the orchestrator failed.
    Failed,
}

impl ReplyKind {
    /// How the kind is spelled on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::Refused => "refused",
            Self::NotFound => "not_found",
            Self::Failed => "failed",
        }
    }
}

/// What the daemon answered a command with: what it did, or why it did nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandReply {
    /// The command was done.
    Done {
        /// One sentence saying what happened.
        said: String,
        /// The sequence numbers of the events it appended.
        events: Vec<u64>,
    },
    /// The command did nothing.
    Error {
        /// Why, as a kind.
        kind: ReplyKind,
        /// Why, in words.
        detail: String,
    },
}

static REPLY_VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema = schema();
    let reply = json!({
        "$schema": schema["$schema"],
        "$ref": "#/$defs/commandReply",
        "$defs": schema["$defs"],
    });
    jsonschema::options()
        .should_validate_formats(true)
        .build(&reply)
        .expect(
            "the reply definition compiles: it is the command schema's own $defs, which the \
             command validator already compiled",
        )
});

/// Writes a reply as the wire `$defs/commandReply` describes.
#[must_use]
pub fn reply_to_value(reply: &CommandReply) -> Value {
    match reply {
        CommandReply::Done { said, events } => json!({ "said": said, "events": events }),
        CommandReply::Error { kind, detail } => {
            json!({ "error": { "kind": kind.as_str(), "detail": detail } })
        }
    }
}

/// Checks a value against `$defs/commandReply` and, when it conforms, returns the reply.
///
/// # Errors
///
/// Every schema violation, as `command_from_value` reports them.
pub fn reply_from_value(value: &Value) -> Result<CommandReply, Vec<ValidationError>> {
    check(&REPLY_VALIDATOR, value)?;
    if let Some(error) = value.get("error") {
        let kind = match error["kind"].as_str() {
            Some("invalid") => ReplyKind::Invalid,
            Some("refused") => ReplyKind::Refused,
            Some("not_found") => ReplyKind::NotFound,
            _ => ReplyKind::Failed,
        };
        return Ok(CommandReply::Error {
            kind,
            detail: error["detail"].as_str().unwrap_or_default().to_string(),
        });
    }
    Ok(CommandReply::Done {
        said: value["said"].as_str().unwrap_or_default().to_string(),
        events: value["events"]
            .as_array()
            .map(|events| events.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
    })
}

/// A task id the schema's pattern passed, as the contract's own type.
fn task_id_of(task_id: &str) -> Result<TaskId, Vec<ValidationError>> {
    TaskId::from_str(task_id).map_err(|error| {
        vec![ValidationError {
            path: "/body/task_id".to_string(),
            message: error.to_string(),
        }]
    })
}

/// A status the schema's list passed, as the contract's own type: the two lists are the contract
/// schema's, so every value the schema lets through reads.
fn status_of(status: &str) -> Result<TaskStatus, Vec<ValidationError>> {
    TaskStatus::from_str(status).map_err(|_| {
        vec![ValidationError {
            path: "/body/to".to_string(),
            message: format!("{status} is not a status"),
        }]
    })
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
    use farik_core::contract::fixtures::{a_contract_wire, a_full_contract_wire};
    use serde_json::{Value, json};

    use farik_core::contract::TaskStatus;
    use farik_core::team::AgentStatus;

    use super::{
        AcceptSubject, Command, CommandReply, ReplyKind, RequestSize, ValidationError,
        command_from_value, command_to_value, reply_from_value, reply_to_value,
    };

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
        assert_eq!(contract.budget.max_sessions.get(), 12);
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

    fn read(command: &str, body: &Value) -> Command {
        command_from_value(&json!({ "command": command, "body": body }))
            .unwrap_or_else(|errors| panic!("{command} reads: {errors:?}"))
    }

    fn frk(number: u32) -> super::TaskId {
        format!("FRK-{number}").parse().expect("a task id")
    }

    #[test]
    fn reads_every_human_command() {
        assert_eq!(
            read(
                "task_transition",
                &json!({ "task_id": "FRK-3", "to": "blocked", "reason": "Key missing." })
            ),
            Command::TaskTransition {
                task_id: frk(3),
                to: TaskStatus::Blocked,
                reason: "Key missing.".to_string()
            }
        );
        assert_eq!(
            read(
                "human_accept",
                &json!({ "task_id": "FRK-3", "subject": "result", "message": "Looks right." })
            ),
            Command::HumanAccept {
                task_id: frk(3),
                subject: AcceptSubject::Result,
                message: Some("Looks right.".to_string())
            }
        );
        assert_eq!(
            read(
                "human_accept",
                &json!({ "task_id": "FRK-3", "subject": "contract" })
            ),
            Command::HumanAccept {
                task_id: frk(3),
                subject: AcceptSubject::Contract,
                message: None
            }
        );
        assert_eq!(
            read(
                "escalation_resolve",
                &json!({ "task_id": "FRK-3", "to": "refining", "message": "Split it by page." })
            ),
            Command::EscalationResolve {
                task_id: frk(3),
                to: TaskStatus::Refining,
                message: "Split it by page.".to_string()
            }
        );
        assert_eq!(
            read(
                "question_answer",
                &json!({ "question_id": 12, "answer": "Yes." })
            ),
            Command::QuestionAnswer {
                question_id: 12,
                answer: "Yes.".to_string()
            }
        );
        assert_eq!(
            read("contract_lock", &json!({ "task_id": "FRK-3" })),
            Command::ContractLock { task_id: frk(3) }
        );
        assert_eq!(
            read("contract_unlock", &json!({ "task_id": "FRK-3" })),
            Command::ContractUnlock { task_id: frk(3) }
        );
        assert_eq!(
            read("task_integrate", &json!({ "task_id": "FRK-3" })),
            Command::TaskIntegrate { task_id: frk(3) }
        );
        assert_eq!(
            read(
                "agent_update",
                &json!({ "agent_id": "dev-a", "status": "paused" })
            ),
            Command::AgentUpdate {
                agent_id: "dev-a".to_string(),
                status: AgentStatus::Paused
            }
        );
        assert_eq!(
            read("session_stop", &json!({ "session_id": "session-7" })),
            Command::SessionStop {
                session_id: "session-7".to_string()
            }
        );
        assert_eq!(read("run_stop", &json!({})), Command::RunStop);
        assert_eq!(
            read("sprint_start", &json!({ "budget_usd": 20.0 })),
            Command::SprintStart {
                budget_usd: Some(20.0)
            }
        );
        assert_eq!(
            read("sprint_start", &json!({ "budget_usd": null })),
            Command::SprintStart { budget_usd: None }
        );
        assert_eq!(read("sprint_end", &json!({})), Command::SprintEnd);
        assert_eq!(
            read("message_post", &json!({ "text": "@dev-a how is FRK-1?" })),
            Command::MessagePost {
                text: "@dev-a how is FRK-1?".to_string()
            }
        );
    }

    #[test]
    fn refuses_a_body_that_is_another_commands() {
        let errors = refusal(&json!({
            "command": "question_answer",
            "body": { "task_id": "FRK-3" }
        }));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].path, "/body");
        assert!(
            errors[0]
                .message
                .starts_with("a question_answer command does not carry this body"),
            "{}",
            errors[0].message
        );

        let errors = refusal(&json!({
            "command": "human_accept",
            "body": { "task_id": "FRK-3", "subject": "approve" }
        }));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].path, "/body");
    }

    #[test]
    fn writes_every_command_back_as_the_wire_it_was_read_from() {
        // The reader fills a contract's defaults, so the task_create wire is one with every
        // default written, which is what a contract read and written back looks like.
        let wires = [
            json!({ "command": "task_create", "body": { "contract": a_full_contract_wire() } }),
            a_request_triage_wire(),
            json!({ "command": "task_transition",
                    "body": { "task_id": "FRK-3", "to": "blocked", "reason": "Key missing." } }),
            json!({ "command": "human_accept",
                    "body": { "task_id": "FRK-3", "subject": "result", "message": "Looks right." } }),
            json!({ "command": "human_accept", "body": { "task_id": "FRK-3", "subject": "contract" } }),
            json!({ "command": "escalation_resolve",
                    "body": { "task_id": "FRK-3", "to": "refining", "message": "Split it by page." } }),
            json!({ "command": "question_answer", "body": { "question_id": 12, "answer": "Yes." } }),
            json!({ "command": "contract_lock", "body": { "task_id": "FRK-3" } }),
            json!({ "command": "contract_unlock", "body": { "task_id": "FRK-3" } }),
            json!({ "command": "task_integrate", "body": { "task_id": "FRK-3" } }),
            json!({ "command": "agent_update", "body": { "agent_id": "dev-a", "status": "paused" } }),
            json!({ "command": "session_stop", "body": { "session_id": "session-7" } }),
            json!({ "command": "run_stop", "body": {} }),
            json!({ "command": "sprint_start", "body": { "budget_usd": 20.0 } }),
            json!({ "command": "sprint_start", "body": { "budget_usd": null } }),
            json!({ "command": "sprint_end", "body": {} }),
            json!({ "command": "message_post", "body": { "text": "hello @dev-a" } }),
        ];
        for wire in wires {
            let command = command_from_value(&wire).expect("the wire reads");
            assert_eq!(command_to_value(&command), wire);
        }
    }

    #[test]
    fn reads_a_reply_either_way_and_refuses_one_that_is_neither() {
        let done = json!({ "said": "x", "events": [3, 4] });
        assert_eq!(
            reply_from_value(&done),
            Ok(CommandReply::Done {
                said: "x".to_string(),
                events: vec![3, 4]
            })
        );
        let error = json!({ "error": { "kind": "not_found", "detail": "question 9" } });
        assert_eq!(
            reply_from_value(&error),
            Ok(CommandReply::Error {
                kind: ReplyKind::NotFound,
                detail: "question 9".to_string()
            })
        );
        for wire in [done, error] {
            let reply = reply_from_value(&wire).expect("the reply reads");
            assert_eq!(reply_to_value(&reply), wire);
        }
        assert!(reply_from_value(&json!({ "said": "x" })).is_err());
        assert!(reply_from_value(&json!({ "error": { "kind": "lost", "detail": "" } })).is_err());
    }

    #[test]
    fn reads_every_kind_of_refusal_back_as_it_was_written() {
        for kind in [
            ReplyKind::Invalid,
            ReplyKind::Refused,
            ReplyKind::NotFound,
            ReplyKind::Failed,
        ] {
            let reply = CommandReply::Error {
                kind,
                detail: "why".to_string(),
            };
            let wire = reply_to_value(&reply);
            assert_eq!(wire["error"]["kind"], kind.as_str());
            assert_eq!(reply_from_value(&wire), Ok(reply));
        }
    }
}
