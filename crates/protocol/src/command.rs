//! The commands the daemon accepts: `docs/schemas/command.schema.json` as Rust types, and the
//! reader that turns an untrusted value into one.

use std::str::FromStr;
use std::sync::LazyLock;

use catervas_core::contract::{TaskStatus, validate_contract};
use catervas_core::team::AgentStatus;
use jsonschema::Validator;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use catervas_core::contract::{TaskContract, TaskId, ValidationError};

pub use crate::generated::command::CommandName;
use crate::generated::command::{
    AgentUpdateBody, CatervasCommand as CommandWire, ChatMessagePostBody, ConnectorConnectBody,
    ConnectorDisconnectBody, DataPipelineDecideBody, DataPipelineDecideBodyDecision, EmptyBody,
    EscalationResolveBody, FolderChangeIntegrateBody, HumanAcceptBody, HumanAcceptBodySubject,
    HumanSendBackBody, HumanSendBackBodySubject, MarketingPlanDecideBody,
    MarketingPlanDecideBodyDecision, MarketingPlanEndBody, MessagePostBody,
    PurchaseOrderDecideBody, PurchaseOrderDecideBodyDecision, PurchaseOrderSendBody,
    PurchaseOrderStepBody, PurchaseOrderUpdateBody, QuestionAnswerBody, RenewalDismissBody,
    RequestTriageBody, RequestTriageBodySize, SellerMessageDiscardBody, SellerMessageSendBody,
    SellerReplyDismissBody, SessionStopBody, SiteAddBody, SiteDecideBody, SiteRemoveBody,
    SkillConfirmBody, SkillLevel, SkillRemoveBody, SkillSaveBody, SocialPostDecideBody,
    SocialPostDecideBodyDecision, SocialPostStopBody, SprintStartBody, TaskCreateBody, TaskIdBody,
    TaskTransitionBody, ToolDecisionBody,
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

/// Whose skill a skill command is about (ADR 0034).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillScope {
    /// The team's.
    Team,
    /// One agent's, by id.
    Agent(String),
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
        /// More attempts for an `iterations` escalation, counting the one this starts (ADR 0024).
        extra_tries: Option<u8>,
    },
    /// Send a contract awaiting approval back to `refining`, or a result that waits for the human
    /// back to its assignee (ADR 0024).
    HumanSendBack {
        /// The task.
        task_id: TaskId,
        /// Whether the contract or the result is sent back.
        subject: AcceptSubject,
        /// What is wrong, in the human's words.
        message: String,
        /// The criteria the result fails, when the human names any.
        failed_criteria: Vec<String>,
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
    /// Pause the whole team: no rule runs until it is resumed.
    TeamPause,
    /// Resume a paused team.
    TeamResume,
    /// Say something in the team's channel.
    MessagePost {
        /// What the human says.
        text: String,
    },
    /// Say something in the human's one-to-one chat with one agent.
    ChatMessagePost {
        /// The agent whose chat it is.
        agent_id: String,
        /// What the human says, its line breaks kept.
        text: String,
    },
    /// Give an agent a custom MCP server whose keys are already kept on this machine, or replace
    /// the one of its name (ADR 0030).
    ConnectorConnect {
        /// The agent.
        agent: String,
        /// The `mcp_servers` entry, as `team.schema.json` shapes it: names and templates, no
        /// value.
        server: serde_json::Map<String, Value>,
        /// The hash of the entry kept beside its keys.
        spec_sha256: String,
        /// Who the agent signed in with, when the connector is signed in to (ADR 0033).
        issuer: Option<String>,
    },
    /// Take a custom MCP server away from an agent.
    ConnectorDisconnect {
        /// The agent.
        agent: String,
        /// The server's name.
        server: String,
    },
    /// Allow once the connector call an agent asked about (ADR 0031).
    ToolApprove {
        /// The seq of its `tool_approval.requested`.
        approval: u64,
        /// What the human says to the agent.
        note: Option<String>,
    },
    /// Refuse the connector call an agent asked about.
    ToolRefuse {
        /// The seq of its `tool_approval.requested`.
        approval: u64,
        /// What the human says to the agent.
        note: Option<String>,
    },
    /// Add a skill for the team or one agent, or replace the one of its name (ADR 0034).
    SkillSave {
        /// Whose.
        scope: SkillScope,
        /// Each file of the skill's folder, by path, as text.
        files: std::collections::BTreeMap<String, String>,
        /// Whether a skill of a shipped skill's name may replace it.
        replace_shipped: bool,
    },
    /// Remove a skill.
    SkillRemove {
        /// Whose.
        scope: SkillScope,
        /// The skill's name.
        name: String,
    },
    /// Confirm on this computer the skill as its folder is now.
    SkillConfirm {
        /// Whose.
        scope: SkillScope,
        /// The skill's name.
        name: String,
        /// The hash of the folder the person read.
        sha256: String,
        /// Whether a skill of a shipped skill's name may replace it.
        replace_shipped: bool,
    },
    /// Approve a marketing plan, or send it back with a reason (ADR 0042).
    MarketingPlanDecide {
        /// The plan's id, `MP-<n>`.
        plan: String,
        /// Whether the owner approves it (false: sends it back).
        approve: bool,
        /// What the owner says to the agent: the reason for a plan sent back.
        note: Option<String>,
    },
    /// End an approved marketing plan.
    MarketingPlanEnd {
        /// The plan's id, `MP-<n>`.
        plan: String,
        /// Why, when the owner says.
        note: Option<String>,
    },
    /// Stop a post that is going out or is with Buffer (ADR 0042).
    SocialPostStop {
        /// The post's number.
        post: u64,
    },
    /// Allow a site the Procurement Specialist asked to read, or not allow it (ADR 0039).
    SiteDecide {
        /// The request's number, the seq of its `site.requested`.
        request: u64,
        /// Whether the owner allows the site (false: does not).
        allow: bool,
        /// What the owner says to the agent.
        note: Option<String>,
    },
    /// Allow a site no agent asked for, or turn one of Catervas's back on (ADR 0039).
    SiteAdd {
        /// A name like `shop.com`, or the address of any page on the site.
        site: String,
    },
    /// Take a site away from the Procurement Specialist: one the owner allowed, or one of Catervas's
    /// (ADR 0039).
    SiteRemove {
        /// The site, as a name or as the address of a page on it.
        host: String,
    },
    /// Approve a purchase order the Procurement Specialist suggested, or reject it (ADR 0039).
    PurchaseOrderDecide {
        /// The order's number, the n of PO-n.
        order: u64,
        /// Whether the owner approves it (false: rejects it).
        approve: bool,
        /// What the owner says to the agent.
        note: Option<String>,
    },
    /// Send message `message` to a seller, with the subject and body the owner saw (ADR 0039).
    SellerMessageSend {
        /// The message's number.
        message: u64,
        /// The subject, as the owner saw it and may have edited it.
        subject: String,
        /// The body, as the owner saw it and may have edited it, without the signature and the
        /// line Catervas adds.
        body: String,
    },
    /// Discard message `message` to a seller: nothing is sent (ADR 0039).
    SellerMessageDiscard {
        /// The message's number.
        message: u64,
    },
    /// Dismiss reply `reply` from a seller on Today; it stays kept (ADR 0039).
    SellerReplyDismiss {
        /// The reply's number.
        reply: u64,
    },
    /// Add folder change `change` to the project now, whatever escalations it carries
    /// (`catervas integrate folder-<n>`).
    FolderChangeIntegrate {
        /// The change's number, the n of `docs/folder-<n>`.
        change: u64,
    },
    /// Approve an order and email it, with its workbook, to its seller in one press, which records
    /// it placed: the owner pays the seller outside Catervas (ADR 0039).
    PurchaseOrderSend {
        /// The order's number, the n of PO-n.
        order: u64,
        /// The order's message, the one that waits with it.
        message: u64,
        /// The subject, as the owner saw it and may have edited it.
        subject: String,
        /// The body, as the owner saw it and may have edited it.
        body: String,
        /// What the owner says to the agent.
        note: Option<String>,
    },
    /// Mark an approved order placed: the owner placed it and paid for it themselves (ADR 0039).
    PurchaseOrderPlace {
        /// The order's number.
        order: u64,
        /// The day it was placed; today when absent.
        placed_on: Option<chrono::NaiveDate>,
        /// What the owner paid, as a number such as `1450`, when they know.
        paid: Option<String>,
        /// The currency of `paid`; the order's when absent.
        currency: Option<String>,
    },
    /// Mark a placed order received (ADR 0039).
    PurchaseOrderReceive {
        /// The order's number.
        order: u64,
        /// The day it came; today when absent.
        received_on: Option<chrono::NaiveDate>,
        /// What the owner paid, when they say so here.
        paid: Option<String>,
        /// The currency of `paid`.
        currency: Option<String>,
        /// The day a subscription or anything paid for again renews.
        renews_on: Option<chrono::NaiveDate>,
    },
    /// Close a placed order that will not come: the seller cancelled or refunded it, or it was
    /// lost (ADR 0039).
    PurchaseOrderClose {
        /// The order's number.
        order: u64,
        /// What the owner says to the agent.
        note: Option<String>,
    },
    /// Correct the follow-up status of a placed order (ADR 0039).
    PurchaseOrderUpdate {
        /// The order's number.
        order: u64,
        /// One of `preparing`, `shipped`, `delayed` and `problem`.
        status: String,
        /// What the owner knows.
        note: Option<String>,
        /// The day the seller expects the order.
        expected_on: Option<chrono::NaiveDate>,
    },
    /// Dismiss a renewal coming up (ADR 0039).
    RenewalDismiss {
        /// The renewal's number, the seq of its `renewal.flagged`.
        renewal: u64,
    },
    /// Approve a data pipeline request the Product Manager passed to the owner, or decline it
    /// (ADR 0039).
    DataPipelineDecide {
        /// The request's number.
        pipeline: u64,
        /// Whether the owner approves it (false: declines it).
        approve: bool,
        /// What the owner says to the agent.
        note: Option<String>,
    },
    /// Allow a post written outside the plan, or not allow it (ADR 0042).
    SocialPostDecide {
        /// The post's number.
        post: u64,
        /// Whether the owner allows it (false: does not).
        post_it: bool,
        /// What the owner says to the agent when they do not allow it.
        note: Option<String>,
    },
}

/// Checks a value against `docs/schemas/command.schema.json` and, when it conforms, returns the
/// typed command. The contract inside `task_create` goes through
/// `catervas_core::contract::validate_contract`, so that a contract arriving inside a command is held
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
#[allow(clippy::too_many_lines, reason = "one arm per command")]
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
                // The schema holds it to 1 to 5, so it always fits.
                extra_tries: body
                    .extra_tries
                    .and_then(|tries| u8::try_from(tries.get()).ok()),
            })
        }
        CommandName::HumanSendBack => send_back(read_body(body, name)?),
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
        CommandName::RunStop
        | CommandName::SprintEnd
        | CommandName::TeamPause
        | CommandName::TeamResume => empty_command(name, body),
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
        CommandName::ChatMessagePost => {
            let body: ChatMessagePostBody = read_body(body, name)?;
            Ok(Command::ChatMessagePost {
                agent_id: body.agent_id,
                text: body.text,
            })
        }
        CommandName::ConnectorConnect | CommandName::ConnectorDisconnect => {
            connector_command(name, body)
        }
        CommandName::ToolApprove | CommandName::ToolRefuse => decision_command(name, body),
        CommandName::SkillSave | CommandName::SkillRemove | CommandName::SkillConfirm => {
            skill_command(name, body)
        }
        CommandName::MarketingPlanDecide | CommandName::MarketingPlanEnd => {
            marketing_plan_command(name, body)
        }
        CommandName::SocialPostStop => {
            let body: SocialPostStopBody = read_body(body, name)?;
            Ok(Command::SocialPostStop {
                post: body.post.get(),
            })
        }
        CommandName::SiteDecide => {
            let body: SiteDecideBody = read_body(body, name)?;
            Ok(Command::SiteDecide {
                request: body.request.get(),
                allow: body.allow,
                note: body.note.map(|note| note.as_str().to_string()),
            })
        }
        CommandName::SiteAdd => {
            let body: SiteAddBody = read_body(body, name)?;
            Ok(Command::SiteAdd {
                site: body.site.to_string(),
            })
        }
        CommandName::SiteRemove => {
            let body: SiteRemoveBody = read_body(body, name)?;
            Ok(Command::SiteRemove {
                host: body.host.to_string(),
            })
        }
        CommandName::PurchaseOrderDecide => {
            let body: PurchaseOrderDecideBody = read_body(body, name)?;
            Ok(Command::PurchaseOrderDecide {
                order: body.order.get(),
                approve: body.decision == PurchaseOrderDecideBodyDecision::Approve,
                note: body.note.map(|note| note.as_str().to_string()),
            })
        }
        CommandName::SellerMessageSend => {
            let body: SellerMessageSendBody = read_body(body, name)?;
            Ok(Command::SellerMessageSend {
                message: body.message.get(),
                subject: body.subject.as_str().to_string(),
                body: body.body.as_str().to_string(),
            })
        }
        CommandName::SellerMessageDiscard => {
            let body: SellerMessageDiscardBody = read_body(body, name)?;
            Ok(Command::SellerMessageDiscard {
                message: body.message.get(),
            })
        }
        CommandName::SellerReplyDismiss => {
            let body: SellerReplyDismissBody = read_body(body, name)?;
            Ok(Command::SellerReplyDismiss {
                reply: body.reply.get(),
            })
        }
        CommandName::FolderChangeIntegrate => {
            let body: FolderChangeIntegrateBody = read_body(body, name)?;
            Ok(Command::FolderChangeIntegrate {
                change: body.change.get(),
            })
        }
        CommandName::PurchaseOrderSend => {
            let body: PurchaseOrderSendBody = read_body(body, name)?;
            Ok(Command::PurchaseOrderSend {
                order: body.order.get(),
                message: body.message.get(),
                subject: body.subject.as_str().to_string(),
                body: body.body.as_str().to_string(),
                note: body.note.map(|note| note.as_str().to_string()),
            })
        }
        CommandName::PurchaseOrderPlace => {
            only(body, name, &["order", "placed_on", "paid", "currency"])?;
            let body: PurchaseOrderStepBody = read_body(body, name)?;
            Ok(Command::PurchaseOrderPlace {
                order: body.order.get(),
                placed_on: body.placed_on,
                paid: body.paid.map(|paid| paid.to_string()),
                currency: body.currency.map(|currency| currency.to_string()),
            })
        }
        CommandName::PurchaseOrderReceive => {
            only(
                body,
                name,
                &["order", "received_on", "paid", "currency", "renews_on"],
            )?;
            let body: PurchaseOrderStepBody = read_body(body, name)?;
            Ok(Command::PurchaseOrderReceive {
                order: body.order.get(),
                received_on: body.received_on,
                paid: body.paid.map(|paid| paid.to_string()),
                currency: body.currency.map(|currency| currency.to_string()),
                renews_on: body.renews_on,
            })
        }
        CommandName::PurchaseOrderClose => {
            only(body, name, &["order", "note"])?;
            let body: PurchaseOrderStepBody = read_body(body, name)?;
            Ok(Command::PurchaseOrderClose {
                order: body.order.get(),
                note: body.note.map(|note| note.as_str().to_string()),
            })
        }
        CommandName::PurchaseOrderUpdate => {
            let body: PurchaseOrderUpdateBody = read_body(body, name)?;
            Ok(Command::PurchaseOrderUpdate {
                order: body.order.get(),
                status: body.status.to_string(),
                note: body.note.map(|note| note.as_str().to_string()),
                expected_on: body.expected_on,
            })
        }
        CommandName::RenewalDismiss => {
            let body: RenewalDismissBody = read_body(body, name)?;
            Ok(Command::RenewalDismiss {
                renewal: body.renewal.get(),
            })
        }
        CommandName::DataPipelineDecide => {
            let body: DataPipelineDecideBody = read_body(body, name)?;
            Ok(Command::DataPipelineDecide {
                pipeline: body.pipeline.get(),
                approve: body.decision == DataPipelineDecideBodyDecision::Approve,
                note: body.note.map(|note| note.as_str().to_string()),
            })
        }
        CommandName::SocialPostDecide => {
            let body: SocialPostDecideBody = read_body(body, name)?;
            Ok(Command::SocialPostDecide {
                post: body.post.get(),
                post_it: body.decision == SocialPostDecideBodyDecision::Post,
                note: body.note.map(|note| note.as_str().to_string()),
            })
        }
    }
}

/// `marketing_plan_decide` and `marketing_plan_end`, each read from its own body shape.
fn marketing_plan_command(
    name: CommandName,
    body: &Value,
) -> Result<Command, Vec<ValidationError>> {
    if name == CommandName::MarketingPlanDecide {
        let body: MarketingPlanDecideBody = read_body(body, name)?;
        Ok(Command::MarketingPlanDecide {
            plan: body.plan.to_string(),
            approve: body.decision == MarketingPlanDecideBodyDecision::Approve,
            note: body.note.map(|note| note.as_str().to_string()),
        })
    } else {
        let body: MarketingPlanEndBody = read_body(body, name)?;
        Ok(Command::MarketingPlanEnd {
            plan: body.plan.to_string(),
            note: body.note.map(|note| note.as_str().to_string()),
        })
    }
}

/// `level` and `agent` as a scope: an agent's level names its agent, and a team's names none,
/// both refused at `/body/agent`.
fn scope_of(level: SkillLevel, agent: Option<String>) -> Result<SkillScope, Vec<ValidationError>> {
    let wrong = |message: &str| {
        vec![ValidationError {
            path: "/body/agent".to_string(),
            message: message.to_string(),
        }]
    };
    match (level, agent) {
        (SkillLevel::Team, None) => Ok(SkillScope::Team),
        (SkillLevel::Agent, Some(agent)) => Ok(SkillScope::Agent(agent)),
        (SkillLevel::Team, Some(_)) => Err(wrong("a team skill has no agent")),
        (SkillLevel::Agent, None) => Err(wrong("an agent's skill names its agent")),
    }
}

/// `skill_save`, `skill_remove` and `skill_confirm`, each read from its own body shape.
fn skill_command(name: CommandName, body: &Value) -> Result<Command, Vec<ValidationError>> {
    match name {
        CommandName::SkillSave => {
            let body: SkillSaveBody = read_body(body, name)?;
            Ok(Command::SkillSave {
                scope: scope_of(body.level, body.agent.map(|agent| agent.to_string()))?,
                files: body.files.into_iter().collect(),
                replace_shipped: body.replace_shipped.unwrap_or(false),
            })
        }
        CommandName::SkillRemove => {
            let body: SkillRemoveBody = read_body(body, name)?;
            Ok(Command::SkillRemove {
                scope: scope_of(body.level, body.agent.map(|agent| agent.to_string()))?,
                name: body.name.to_string(),
            })
        }
        _ => {
            let body: SkillConfirmBody = read_body(body, name)?;
            Ok(Command::SkillConfirm {
                scope: scope_of(body.level, body.agent.map(|agent| agent.to_string()))?,
                name: body.name.to_string(),
                sha256: body.sha256.to_string(),
                replace_shipped: body.replace_shipped.unwrap_or(false),
            })
        }
    }
}

/// `run_stop`, `sprint_end`, `team_pause` or `team_resume`, whose body is empty.
fn empty_command(name: CommandName, body: &Value) -> Result<Command, Vec<ValidationError>> {
    let _: EmptyBody = read_body(body, name)?;
    Ok(match name {
        CommandName::RunStop => Command::RunStop,
        CommandName::SprintEnd => Command::SprintEnd,
        CommandName::TeamPause => Command::TeamPause,
        _ => Command::TeamResume,
    })
}

/// `tool_approve` or `tool_refuse`, which share one body shape.
fn decision_command(name: CommandName, body: &Value) -> Result<Command, Vec<ValidationError>> {
    let body: ToolDecisionBody = read_body(body, name)?;
    let (approval, note) = (body.approval.get(), body.note);
    Ok(if name == CommandName::ToolApprove {
        Command::ToolApprove { approval, note }
    } else {
        Command::ToolRefuse { approval, note }
    })
}

/// `connector_connect` and `connector_disconnect`, each read from its own body shape.
fn connector_command(name: CommandName, body: &Value) -> Result<Command, Vec<ValidationError>> {
    if name == CommandName::ConnectorConnect {
        let body: ConnectorConnectBody = read_body(body, name)?;
        Ok(Command::ConnectorConnect {
            agent: body.agent.to_string(),
            server: body.server,
            spec_sha256: body.spec_sha256.to_string(),
            issuer: body.issuer.map(|issuer| issuer.to_string()),
        })
    } else {
        let body: ConnectorDisconnectBody = read_body(body, name)?;
        Ok(Command::ConnectorDisconnect {
            agent: body.agent.to_string(),
            server: body.server.to_string(),
        })
    }
}

fn send_back(body: HumanSendBackBody) -> Result<Command, Vec<ValidationError>> {
    Ok(Command::HumanSendBack {
        task_id: task_id_of(body.task_id.as_str())?,
        subject: match body.subject {
            HumanSendBackBodySubject::Contract => AcceptSubject::Contract,
            HumanSendBackBodySubject::Result => AcceptSubject::Result,
        },
        message: body.message,
        failed_criteria: body.failed_criteria,
    })
}

/// Writes a command as the wire `command_from_value` reads it back from: the inverse of the reader,
/// so that a command built in one process reaches another's daemon exactly as it was built. A
/// `human_accept` with no message is written without one.
///
/// # Panics
///
/// Never: a contract's `Serialize` is derived from its schema with string keys.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per command")]
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
        } => (
            CommandName::HumanAccept,
            with_optional(
                json!({ "task_id": task_id.as_str(), "subject": subject_wire(*subject) }),
                "message",
                message.as_ref().map(|message| json!(message)),
            ),
        ),
        Command::EscalationResolve {
            task_id,
            to,
            message,
            extra_tries,
        } => resolve_wire(task_id, *to, message, *extra_tries),
        Command::HumanSendBack {
            task_id,
            subject,
            message,
            failed_criteria,
        } => send_back_wire(task_id, *subject, message, failed_criteria),
        Command::QuestionAnswer {
            question_id,
            answer,
        } => (
            CommandName::QuestionAnswer,
            json!({ "question_id": question_id, "answer": answer }),
        ),
        Command::ContractLock { task_id } => task_wire(CommandName::ContractLock, task_id),
        Command::ContractUnlock { task_id } => task_wire(CommandName::ContractUnlock, task_id),
        Command::TaskIntegrate { task_id } => task_wire(CommandName::TaskIntegrate, task_id),
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
        Command::TeamPause => (CommandName::TeamPause, json!({})),
        Command::TeamResume => (CommandName::TeamResume, json!({})),
        Command::MessagePost { text } => (CommandName::MessagePost, json!({ "text": text })),
        Command::ChatMessagePost { agent_id, text } => (
            CommandName::ChatMessagePost,
            json!({ "agent_id": agent_id, "text": text }),
        ),
        Command::ConnectorConnect { .. } | Command::ConnectorDisconnect { .. } => {
            connector_wire(command)
        }
        Command::ToolApprove { approval, note } => {
            decision_wire(CommandName::ToolApprove, *approval, note.as_ref())
        }
        Command::ToolRefuse { approval, note } => {
            decision_wire(CommandName::ToolRefuse, *approval, note.as_ref())
        }
        Command::SkillSave { .. } | Command::SkillRemove { .. } | Command::SkillConfirm { .. } => {
            skill_wire(command)
        }
        Command::MarketingPlanDecide {
            plan,
            approve,
            note,
        } => (
            CommandName::MarketingPlanDecide,
            with_optional(
                json!({ "plan": plan, "decision": if *approve { "approve" } else { "return" } }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::MarketingPlanEnd { plan, note } => (
            CommandName::MarketingPlanEnd,
            with_optional(
                json!({ "plan": plan }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::SocialPostStop { post } => (CommandName::SocialPostStop, json!({ "post": post })),
        Command::DataPipelineDecide {
            pipeline,
            approve,
            note,
        } => (
            CommandName::DataPipelineDecide,
            with_optional(
                json!({
                    "pipeline": pipeline,
                    "decision": if *approve { "approve" } else { "decline" },
                }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::PurchaseOrderDecide {
            order,
            approve,
            note,
        } => (
            CommandName::PurchaseOrderDecide,
            with_optional(
                json!({ "order": order, "decision": if *approve { "approve" } else { "reject" } }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::SellerMessageSend {
            message,
            subject,
            body,
        } => (
            CommandName::SellerMessageSend,
            json!({ "message": message, "subject": subject, "body": body }),
        ),
        Command::SellerMessageDiscard { message } => (
            CommandName::SellerMessageDiscard,
            json!({ "message": message }),
        ),
        Command::FolderChangeIntegrate { change } => (
            CommandName::FolderChangeIntegrate,
            json!({ "change": change }),
        ),
        Command::SellerReplyDismiss { reply } => {
            (CommandName::SellerReplyDismiss, json!({ "reply": reply }))
        }
        Command::PurchaseOrderSend {
            order,
            message,
            subject,
            body,
            note,
        } => (
            CommandName::PurchaseOrderSend,
            with_optional(
                json!({ "order": order, "message": message, "subject": subject, "body": body }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::PurchaseOrderPlace {
            order,
            placed_on,
            paid,
            currency,
        } => (
            CommandName::PurchaseOrderPlace,
            with_optional(
                with_optional(
                    with_optional(
                        json!({ "order": order }),
                        "placed_on",
                        placed_on.map(|day| json!(day.to_string())),
                    ),
                    "paid",
                    paid.as_ref().map(|paid| json!(paid)),
                ),
                "currency",
                currency.as_ref().map(|currency| json!(currency)),
            ),
        ),
        Command::PurchaseOrderReceive {
            order,
            received_on,
            paid,
            currency,
            renews_on,
        } => (
            CommandName::PurchaseOrderReceive,
            with_optional(
                with_optional(
                    with_optional(
                        with_optional(
                            json!({ "order": order }),
                            "received_on",
                            received_on.map(|day| json!(day.to_string())),
                        ),
                        "paid",
                        paid.as_ref().map(|paid| json!(paid)),
                    ),
                    "currency",
                    currency.as_ref().map(|currency| json!(currency)),
                ),
                "renews_on",
                renews_on.map(|day| json!(day.to_string())),
            ),
        ),
        Command::RenewalDismiss { renewal } => {
            (CommandName::RenewalDismiss, json!({ "renewal": renewal }))
        }
        Command::PurchaseOrderClose { order, note } => (
            CommandName::PurchaseOrderClose,
            with_optional(
                json!({ "order": order }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::PurchaseOrderUpdate {
            order,
            status,
            note,
            expected_on,
        } => (
            CommandName::PurchaseOrderUpdate,
            with_optional(
                with_optional(
                    json!({ "order": order, "status": status }),
                    "note",
                    note.as_ref().map(|note| json!(note)),
                ),
                "expected_on",
                expected_on.map(|day| json!(day.to_string())),
            ),
        ),
        Command::SiteDecide {
            request,
            allow,
            note,
        } => (
            CommandName::SiteDecide,
            with_optional(
                json!({ "request": request, "allow": allow }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
        Command::SiteAdd { site } => (CommandName::SiteAdd, json!({ "site": site })),
        Command::SiteRemove { host } => (CommandName::SiteRemove, json!({ "host": host })),
        Command::SocialPostDecide {
            post,
            post_it,
            note,
        } => (
            CommandName::SocialPostDecide,
            with_optional(
                json!({ "post": post, "decision": if *post_it { "post" } else { "dont_post" } }),
                "note",
                note.as_ref().map(|note| json!(note)),
            ),
        ),
    };
    json!({ "command": name.to_string(), "body": body })
}

/// A command whose body is its task's id alone.
fn task_wire(name: CommandName, task_id: &TaskId) -> (CommandName, Value) {
    (name, json!({ "task_id": task_id.as_str() }))
}

/// `tool_approve` or `tool_refuse` as its name and body, `note` absent when there is none.
fn decision_wire(name: CommandName, approval: u64, note: Option<&String>) -> (CommandName, Value) {
    let mut body = json!({ "approval": approval });
    if let Some(note) = note {
        body["note"] = json!(note);
    }
    (name, body)
}

/// A skill command's body so far: its level and, for an agent's, its agent.
fn scope_wire(scope: &SkillScope) -> Value {
    match scope {
        SkillScope::Team => json!({ "level": "team" }),
        SkillScope::Agent(agent) => json!({ "level": "agent", "agent": agent }),
    }
}

/// `skill_save`, `skill_remove` or `skill_confirm` as its name and body, `replace_shipped` written
/// only when it is `true`.
fn skill_wire(command: &Command) -> (CommandName, Value) {
    match command {
        Command::SkillSave {
            scope,
            files,
            replace_shipped,
        } => {
            let mut body = scope_wire(scope);
            body["files"] = json!(files);
            (
                CommandName::SkillSave,
                with_flag(body, "replace_shipped", *replace_shipped),
            )
        }
        Command::SkillRemove { scope, name } => {
            let mut body = scope_wire(scope);
            body["name"] = json!(name);
            (CommandName::SkillRemove, body)
        }
        Command::SkillConfirm {
            scope,
            name,
            sha256,
            replace_shipped,
        } => {
            let mut body = scope_wire(scope);
            body["name"] = json!(name);
            body["sha256"] = json!(sha256);
            (
                CommandName::SkillConfirm,
                with_flag(body, "replace_shipped", *replace_shipped),
            )
        }
        _ => unreachable!("command_to_value asks this of the three skill commands only"),
    }
}

/// `body` with `key` set to `true` when `set`, and left out otherwise.
fn with_flag(mut body: Value, key: &str, set: bool) -> Value {
    if set {
        body[key] = json!(true);
    }
    body
}

/// `connector_connect` or `connector_disconnect` as its name and body.
fn connector_wire(command: &Command) -> (CommandName, Value) {
    match command {
        Command::ConnectorConnect {
            agent,
            server,
            spec_sha256,
            issuer,
        } => {
            let mut body = json!({ "agent": agent, "server": server, "spec_sha256": spec_sha256 });
            if let Some(issuer) = issuer {
                body["issuer"] = json!(issuer);
            }
            (CommandName::ConnectorConnect, body)
        }
        Command::ConnectorDisconnect { agent, server } => (
            CommandName::ConnectorDisconnect,
            json!({ "agent": agent, "server": server }),
        ),
        _ => unreachable!("command_to_value asks this of the two connector commands only"),
    }
}

fn resolve_wire(
    task_id: &TaskId,
    to: TaskStatus,
    message: &str,
    extra_tries: Option<u8>,
) -> (CommandName, Value) {
    (
        CommandName::EscalationResolve,
        with_optional(
            json!({ "task_id": task_id.as_str(), "to": to.to_string(), "message": message }),
            "extra_tries",
            extra_tries.map(|tries| json!(tries)),
        ),
    )
}

fn send_back_wire(
    task_id: &TaskId,
    subject: AcceptSubject,
    message: &str,
    failed_criteria: &[String],
) -> (CommandName, Value) {
    (
        CommandName::HumanSendBack,
        json!({
            "task_id": task_id.as_str(),
            "subject": subject_wire(subject),
            "message": message,
            "failed_criteria": failed_criteria,
        }),
    )
}

/// `body` with `key` set to `value` when there is one: the schema has no null for an optional field.
/// Refuses a body that holds a field `allowed` does not name, at the field's own path: the
/// commands that share a body each take some of its fields.
fn only(body: &Value, command: CommandName, allowed: &[&str]) -> Result<(), Vec<ValidationError>> {
    let stray: Vec<ValidationError> = body
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !allowed.contains(&key.as_str()))
        .map(|(key, _)| ValidationError {
            path: format!("/body/{key}"),
            message: format!("a {command} command does not take {key}"),
        })
        .collect();
    if stray.is_empty() { Ok(()) } else { Err(stray) }
}

fn with_optional(mut body: Value, key: &str, value: Option<Value>) -> Value {
    if let Some(value) = value {
        body[key] = value;
    }
    body
}

fn subject_wire(subject: AcceptSubject) -> &'static str {
    match subject {
        AcceptSubject::Contract => "contract",
        AcceptSubject::Result => "result",
    }
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
    use catervas_core::contract::fixtures::{a_contract_wire, a_full_contract_wire};
    use serde_json::{Value, json};

    use catervas_core::contract::TaskStatus;
    use catervas_core::team::AgentStatus;

    use std::collections::BTreeMap;

    use super::{
        AcceptSubject, Command, CommandReply, ReplyKind, RequestSize, SkillScope, ValidationError,
        command_from_value, command_to_value, reply_from_value, reply_to_value,
    };

    fn a_task_create_wire() -> Value {
        json!({ "command": "task_create", "body": { "contract": a_contract_wire() } })
    }

    fn a_request_triage_wire() -> Value {
        json!({
            "command": "request_triage",
            "body": { "task_id": "CTV-1", "size": "large", "reason": "Three deliverables." }
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
        assert_eq!(contract.id.to_string(), "CTV-1");
        // The validator applied the schema's defaults, which is the proof it was the one used.
        assert_eq!(contract.budget.max_sessions.get(), 14);
    }

    #[test]
    fn a_connector_connect_names_who_signed_the_agent_in_only_when_it_did() {
        let hash = "a".repeat(64);
        for issuer in [None, Some("https://auth.example")] {
            let mut body = json!({
                "agent": "dev-a", "server": { "name": "notion" }, "spec_sha256": hash
            });
            if let Some(issuer) = issuer {
                body["issuer"] = json!(issuer);
            }
            let wire = json!({ "command": "connector_connect", "body": body });
            let command = command_from_value(&wire).expect("valid");
            let Command::ConnectorConnect { issuer: read, .. } = &command else {
                panic!("a connector_connect");
            };
            assert_eq!(read.as_deref(), issuer);
            assert_eq!(command_to_value(&command), wire);
        }
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
        assert_eq!(task_id.to_string(), "CTV-1");
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

    fn ctv(number: u32) -> super::TaskId {
        format!("CTV-{number}").parse().expect("a task id")
    }

    #[test]
    fn reads_every_human_command() {
        assert_eq!(
            read(
                "task_transition",
                &json!({ "task_id": "CTV-3", "to": "blocked", "reason": "Key missing." })
            ),
            Command::TaskTransition {
                task_id: ctv(3),
                to: TaskStatus::Blocked,
                reason: "Key missing.".to_string()
            }
        );
        assert_eq!(
            read(
                "human_accept",
                &json!({ "task_id": "CTV-3", "subject": "result", "message": "Looks right." })
            ),
            Command::HumanAccept {
                task_id: ctv(3),
                subject: AcceptSubject::Result,
                message: Some("Looks right.".to_string())
            }
        );
        assert_eq!(
            read(
                "human_accept",
                &json!({ "task_id": "CTV-3", "subject": "contract" })
            ),
            Command::HumanAccept {
                task_id: ctv(3),
                subject: AcceptSubject::Contract,
                message: None
            }
        );
        assert_eq!(
            read(
                "escalation_resolve",
                &json!({ "task_id": "CTV-3", "to": "refining", "message": "Split it by page." })
            ),
            Command::EscalationResolve {
                task_id: ctv(3),
                to: TaskStatus::Refining,
                message: "Split it by page.".to_string(),
                extra_tries: None
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
        for (name, command) in [
            ("contract_lock", Command::ContractLock { task_id: ctv(3) }),
            (
                "contract_unlock",
                Command::ContractUnlock { task_id: ctv(3) },
            ),
            ("task_integrate", Command::TaskIntegrate { task_id: ctv(3) }),
        ] {
            assert_eq!(read(name, &json!({ "task_id": "CTV-3" })), command);
        }
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
            read("message_post", &json!({ "text": "@dev-a how is CTV-1?" })),
            Command::MessagePost {
                text: "@dev-a how is CTV-1?".to_string()
            }
        );
    }

    #[test]
    fn reads_and_writes_team_pause_and_team_resume() {
        for (name, command) in [
            ("team_pause", Command::TeamPause),
            ("team_resume", Command::TeamResume),
        ] {
            assert_eq!(read(name, &json!({})), command);
            assert_eq!(
                command_to_value(&command),
                json!({ "command": name, "body": {} })
            );
            let errors = refusal(&json!({ "command": name, "body": { "why": "lunch" } }));
            assert!(!errors.is_empty(), "{name} takes no field");
        }
    }

    #[test]
    fn refuses_a_body_that_is_another_commands() {
        let errors = refusal(&json!({
            "command": "question_answer",
            "body": { "task_id": "CTV-3" }
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
            "body": { "task_id": "CTV-3", "subject": "approve" }
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
                    "body": { "task_id": "CTV-3", "to": "blocked", "reason": "Key missing." } }),
            json!({ "command": "human_accept",
                    "body": { "task_id": "CTV-3", "subject": "result", "message": "Looks right." } }),
            json!({ "command": "human_accept", "body": { "task_id": "CTV-3", "subject": "contract" } }),
            json!({ "command": "escalation_resolve",
                    "body": { "task_id": "CTV-3", "to": "refining", "message": "Split it by page." } }),
            json!({ "command": "escalation_resolve",
                    "body": { "task_id": "CTV-3", "to": "in_progress", "message": "Go.", "extra_tries": 2 } }),
            json!({ "command": "human_send_back",
                    "body": { "task_id": "CTV-3", "subject": "result", "message": "Too small.", "failed_criteria": ["C1"] } }),
            json!({ "command": "question_answer", "body": { "question_id": 12, "answer": "Yes." } }),
            json!({ "command": "contract_lock", "body": { "task_id": "CTV-3" } }),
            json!({ "command": "contract_unlock", "body": { "task_id": "CTV-3" } }),
            json!({ "command": "task_integrate", "body": { "task_id": "CTV-3" } }),
            json!({ "command": "agent_update", "body": { "agent_id": "dev-a", "status": "paused" } }),
            json!({ "command": "session_stop", "body": { "session_id": "session-7" } }),
            json!({ "command": "run_stop", "body": {} }),
            json!({ "command": "sprint_start", "body": { "budget_usd": 20.0 } }),
            json!({ "command": "sprint_start", "body": { "budget_usd": null } }),
            json!({ "command": "sprint_end", "body": {} }),
            json!({ "command": "message_post", "body": { "text": "hello @dev-a" } }),
            json!({ "command": "chat_message_post", "body": { "agent_id": "mira", "text": "Apple Pay?\nOr not." } }),
        ];
        for wire in wires {
            let command = command_from_value(&wire).expect("the wire reads");
            assert_eq!(command_to_value(&command), wire);
        }
    }

    #[test]
    fn reads_and_writes_the_marketing_plan_commands() {
        for (name, body, command) in [
            (
                "marketing_plan_decide",
                json!({ "plan": "MP-1", "decision": "approve" }),
                Command::MarketingPlanDecide {
                    plan: "MP-1".to_string(),
                    approve: true,
                    note: None,
                },
            ),
            (
                "marketing_plan_decide",
                json!({ "plan": "MP-2", "decision": "approve", "note": "Start small" }),
                Command::MarketingPlanDecide {
                    plan: "MP-2".to_string(),
                    approve: true,
                    note: Some("Start small".to_string()),
                },
            ),
            (
                "marketing_plan_decide",
                json!({ "plan": "MP-3", "decision": "return", "note": "Halve the budget." }),
                Command::MarketingPlanDecide {
                    plan: "MP-3".to_string(),
                    approve: false,
                    note: Some("Halve the budget.".to_string()),
                },
            ),
            (
                "marketing_plan_end",
                json!({ "plan": "MP-1" }),
                Command::MarketingPlanEnd {
                    plan: "MP-1".to_string(),
                    note: None,
                },
            ),
            (
                "marketing_plan_end",
                json!({ "plan": "MP-1", "note": "Changed course." }),
                Command::MarketingPlanEnd {
                    plan: "MP-1".to_string(),
                    note: Some("Changed course.".to_string()),
                },
            ),
        ] {
            assert_eq!(read(name, &body), command, "{name}");
            assert_eq!(
                command_to_value(&command),
                json!({ "command": name, "body": body }),
                "{name}"
            );
        }
        for (name, body) in [
            (
                "marketing_plan_decide",
                json!({ "plan": "plan-1", "decision": "approve" }),
            ),
            (
                "marketing_plan_decide",
                json!({ "plan": "MP-1", "decision": "maybe" }),
            ),
            ("marketing_plan_decide", json!({ "plan": "MP-1" })),
            (
                "marketing_plan_decide",
                json!({ "plan": "MP-1", "decision": "return", "note": "x".repeat(601) }),
            ),
            (
                "marketing_plan_end",
                json!({ "plan": "MP-1", "note": "x".repeat(601) }),
            ),
            (
                "marketing_plan_end",
                json!({ "plan": "MP-1", "decision": "approve" }),
            ),
            ("marketing_plan_end", json!({})),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert!(!errors.is_empty(), "{name} {body}");
        }
        let a_note_of_600 = "x".repeat(600);
        read(
            "marketing_plan_decide",
            &json!({ "plan": "MP-1", "decision": "return", "note": a_note_of_600 }),
        );
    }

    #[test]
    fn reads_and_writes_the_social_post_commands() {
        for (name, body, command) in [
            (
                "social_post_stop",
                json!({ "post": 42 }),
                Command::SocialPostStop { post: 42 },
            ),
            (
                "social_post_decide",
                json!({ "post": 7, "decision": "post" }),
                Command::SocialPostDecide {
                    post: 7,
                    post_it: true,
                    note: None,
                },
            ),
            (
                "social_post_decide",
                json!({ "post": 8, "decision": "dont_post", "note": "Not this week" }),
                Command::SocialPostDecide {
                    post: 8,
                    post_it: false,
                    note: Some("Not this week".to_string()),
                },
            ),
        ] {
            assert_eq!(read(name, &body), command, "{name}");
            assert_eq!(
                command_to_value(&command),
                json!({ "command": name, "body": body }),
                "{name}"
            );
        }
        for (name, body) in [
            ("social_post_stop", json!({})),
            ("social_post_stop", json!({ "post": 0 })),
            ("social_post_stop", json!({ "post": "42" })),
            ("social_post_stop", json!({ "post": 1, "note": "no" })),
            ("social_post_decide", json!({ "post": 1 })),
            (
                "social_post_decide",
                json!({ "post": 1, "decision": "maybe" }),
            ),
            (
                "social_post_decide",
                json!({ "post": 1, "decision": "post", "note": "x".repeat(601) }),
            ),
            (
                "social_post_decide",
                json!({ "plan": "MP-1", "decision": "post" }),
            ),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert!(!errors.is_empty(), "{name} {body}");
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one row for each command and each body it refuses"
    )]
    fn reads_and_writes_the_seller_message_commands() {
        for (name, body, command) in [
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "Quote for 500 boxes", "body": "Hello,\n\nThanks." }),
                Command::SellerMessageSend {
                    message: 3,
                    subject: "Quote for 500 boxes".to_string(),
                    body: "Hello,\n\nThanks.".to_string(),
                },
            ),
            (
                "seller_message_discard",
                json!({ "message": 4 }),
                Command::SellerMessageDiscard { message: 4 },
            ),
            (
                "seller_reply_dismiss",
                json!({ "reply": 2 }),
                Command::SellerReplyDismiss { reply: 2 },
            ),
            (
                "purchase_order_send",
                json!({ "order": 12, "message": 5, "subject": "Order PO-12", "body": "Attached." }),
                Command::PurchaseOrderSend {
                    order: 12,
                    message: 5,
                    subject: "Order PO-12".to_string(),
                    body: "Attached.".to_string(),
                    note: None,
                },
            ),
            (
                "purchase_order_send",
                json!({ "order": 12, "message": 5, "subject": "Order PO-12", "body": "Attached.",
                        "note": "Go ahead." }),
                Command::PurchaseOrderSend {
                    order: 12,
                    message: 5,
                    subject: "Order PO-12".to_string(),
                    body: "Attached.".to_string(),
                    note: Some("Go ahead.".to_string()),
                },
            ),
        ] {
            assert_eq!(read(name, &body), command, "{name}");
            assert_eq!(
                command_to_value(&command),
                json!({ "command": name, "body": body }),
                "{name}"
            );
        }
        for (name, body) in [
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "S" }),
            ),
            ("seller_message_send", json!({ "message": 3, "body": "B" })),
            (
                "seller_message_send",
                json!({ "subject": "S", "body": "B" }),
            ),
            (
                "seller_message_send",
                json!({ "message": 0, "subject": "S", "body": "B" }),
            ),
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "", "body": "B" }),
            ),
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "S\nT", "body": "B" }),
            ),
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "x".repeat(201), "body": "B" }),
            ),
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "S", "body": "" }),
            ),
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "S", "body": "x".repeat(8001) }),
            ),
            (
                "seller_message_send",
                json!({ "message": 3, "subject": "S", "body": "B", "note": "n" }),
            ),
            ("seller_message_discard", json!({})),
            ("seller_reply_dismiss", json!({})),
            ("seller_reply_dismiss", json!({ "reply": 0 })),
            ("seller_reply_dismiss", json!({ "reply": 1, "message": 2 })),
            ("seller_message_discard", json!({ "message": 0 })),
            (
                "seller_message_discard",
                json!({ "message": 1, "reply": 2 }),
            ),
            (
                "purchase_order_send",
                json!({ "order": 12, "subject": "S", "body": "B" }),
            ),
            (
                "purchase_order_send",
                json!({ "message": 5, "subject": "S", "body": "B" }),
            ),
            (
                "purchase_order_send",
                json!({ "order": 12, "message": 5, "subject": "S" }),
            ),
            (
                "purchase_order_send",
                json!({ "order": 12, "message": 5, "subject": "S", "body": "B", "note": "x".repeat(601) }),
            ),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert!(!errors.is_empty(), "{name} {body}");
        }
    }

    #[test]
    fn reads_and_writes_the_site_commands() {
        for (name, body, command) in [
            (
                "site_decide",
                json!({ "request": 7, "allow": true }),
                Command::SiteDecide {
                    request: 7,
                    allow: true,
                    note: None,
                },
            ),
            (
                "site_decide",
                json!({ "request": 8, "allow": false, "note": "Not that shop." }),
                Command::SiteDecide {
                    request: 8,
                    allow: false,
                    note: Some("Not that shop.".to_string()),
                },
            ),
            (
                "site_add",
                json!({ "site": "https://www.shop.example/boxes" }),
                Command::SiteAdd {
                    site: "https://www.shop.example/boxes".to_string(),
                },
            ),
            (
                "site_remove",
                json!({ "host": "grainger.com" }),
                Command::SiteRemove {
                    host: "grainger.com".to_string(),
                },
            ),
        ] {
            assert_eq!(read(name, &body), command, "{name}");
            assert_eq!(
                command_to_value(&command),
                json!({ "command": name, "body": body }),
                "{name}"
            );
        }
        for (name, body) in [
            ("site_decide", json!({ "request": 1 })),
            ("site_decide", json!({ "allow": true })),
            ("site_decide", json!({ "request": 0, "allow": true })),
            ("site_decide", json!({ "request": "1", "allow": true })),
            ("site_decide", json!({ "request": 1, "allow": "yes" })),
            (
                "site_decide",
                json!({ "request": 1, "allow": true, "note": "x".repeat(601) }),
            ),
            ("site_add", json!({})),
            ("site_add", json!({ "site": 7 })),
            (
                "site_add",
                json!({ "site": "shop.example", "host": "shop.example" }),
            ),
            ("site_remove", json!({})),
            ("site_remove", json!({ "site": "shop.example" })),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert!(!errors.is_empty(), "{name} {body}");
        }
        read(
            "site_decide",
            &json!({ "request": 1, "allow": false, "note": "x".repeat(600) }),
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one row for each command and each body it refuses"
    )]
    fn reads_and_writes_the_purchase_order_commands() {
        let day = |text: &str| chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a date");
        for (name, body, command) in [
            (
                "purchase_order_decide",
                json!({ "order": 3, "decision": "approve" }),
                Command::PurchaseOrderDecide {
                    order: 3,
                    approve: true,
                    note: None,
                },
            ),
            (
                "purchase_order_decide",
                json!({ "order": 4, "decision": "reject", "note": "Too dear." }),
                Command::PurchaseOrderDecide {
                    order: 4,
                    approve: false,
                    note: Some("Too dear.".to_string()),
                },
            ),
            (
                "purchase_order_place",
                json!({ "order": 3 }),
                Command::PurchaseOrderPlace {
                    order: 3,
                    placed_on: None,
                    paid: None,
                    currency: None,
                },
            ),
            (
                "purchase_order_place",
                json!({ "order": 3, "placed_on": "2026-10-08", "paid": "1450", "currency": "EUR" }),
                Command::PurchaseOrderPlace {
                    order: 3,
                    placed_on: Some(day("2026-10-08")),
                    paid: Some("1450".to_string()),
                    currency: Some("EUR".to_string()),
                },
            ),
            (
                "purchase_order_receive",
                json!({
                    "order": 3, "received_on": "2026-10-15", "paid": "1450.00",
                    "currency": "EUR", "renews_on": "2027-10-15"
                }),
                Command::PurchaseOrderReceive {
                    order: 3,
                    received_on: Some(day("2026-10-15")),
                    paid: Some("1450.00".to_string()),
                    currency: Some("EUR".to_string()),
                    renews_on: Some(day("2027-10-15")),
                },
            ),
            (
                "purchase_order_receive",
                json!({ "order": 3 }),
                Command::PurchaseOrderReceive {
                    order: 3,
                    received_on: None,
                    paid: None,
                    currency: None,
                    renews_on: None,
                },
            ),
            (
                "purchase_order_close",
                json!({ "order": 3, "note": "It was lost." }),
                Command::PurchaseOrderClose {
                    order: 3,
                    note: Some("It was lost.".to_string()),
                },
            ),
            (
                "purchase_order_update",
                json!({
                    "order": 3, "status": "delayed", "note": "Short of flour.",
                    "expected_on": "2026-10-20"
                }),
                Command::PurchaseOrderUpdate {
                    order: 3,
                    status: "delayed".to_string(),
                    note: Some("Short of flour.".to_string()),
                    expected_on: Some(day("2026-10-20")),
                },
            ),
        ] {
            assert_eq!(read(name, &body), command, "{name}");
            assert_eq!(
                command_to_value(&command),
                json!({ "command": name, "body": body }),
                "{name}"
            );
        }
        for (name, body) in [
            ("purchase_order_decide", json!({ "order": 1 })),
            ("purchase_order_decide", json!({ "decision": "approve" })),
            (
                "purchase_order_decide",
                json!({ "order": 0, "decision": "approve" }),
            ),
            (
                "purchase_order_decide",
                json!({ "order": 1, "decision": "maybe" }),
            ),
            (
                "purchase_order_decide",
                json!({ "order": 1, "decision": "approve", "note": "x".repeat(601) }),
            ),
            (
                "purchase_order_decide",
                json!({ "order": "1", "decision": "approve" }),
            ),
            (
                "purchase_order_place",
                json!({ "order": 1, "placed_on": "tomorrow" }),
            ),
            ("purchase_order_place", json!({ "order": 1, "paid": 5 })),
            (
                "purchase_order_place",
                json!({ "order": 1, "received_on": "2026-10-08" }),
            ),
            (
                "purchase_order_receive",
                json!({ "order": 1, "renews_on": "next year" }),
            ),
            (
                "purchase_order_receive",
                json!({ "order": 1, "placed_on": "2026-10-08" }),
            ),
            ("purchase_order_close", json!({ "note": "gone" })),
            ("purchase_order_close", json!({ "order": 1, "paid": "5" })),
            (
                "purchase_order_close",
                json!({ "order": 1, "currency": "EUR" }),
            ),
            (
                "purchase_order_close",
                json!({ "order": 1, "renews_on": "2027-01-01" }),
            ),
            (
                "purchase_order_place",
                json!({ "order": 1, "note": "placed" }),
            ),
            (
                "purchase_order_place",
                json!({ "order": 1, "renews_on": "2027-01-01" }),
            ),
            (
                "purchase_order_receive",
                json!({ "order": 1, "note": "came" }),
            ),
            (
                "purchase_order_close",
                json!({ "order": 1, "note": "x".repeat(601) }),
            ),
            ("purchase_order_update", json!({ "order": 1 })),
            (
                "purchase_order_update",
                json!({ "order": 1, "status": "shipped", "expected_on": "soon" }),
            ),
            ("purchase_order_update", json!({ "status": "shipped" })),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert!(!errors.is_empty(), "{name} {body}");
        }
        read(
            "purchase_order_close",
            &json!({ "order": 1, "note": "x".repeat(600) }),
        );
        // A status is a word the owner's command takes whatever it is: the rules of the four are
        // the daemon's, and refuse `placed` as `purchase_order_status_invalid`, not here.
        read(
            "purchase_order_update",
            &json!({ "order": 1, "status": "placed" }),
        );
    }

    #[test]
    fn reads_and_writes_the_data_pipeline_decide_command() {
        for (body, command) in [
            (
                json!({ "pipeline": 4, "decision": "approve" }),
                Command::DataPipelineDecide {
                    pipeline: 4,
                    approve: true,
                    note: None,
                },
            ),
            (
                json!({ "pipeline": 5, "decision": "decline", "note": "Use the plain pages." }),
                Command::DataPipelineDecide {
                    pipeline: 5,
                    approve: false,
                    note: Some("Use the plain pages.".to_string()),
                },
            ),
        ] {
            assert_eq!(read("data_pipeline_decide", &body), command);
            assert_eq!(
                command_to_value(&command),
                json!({ "command": "data_pipeline_decide", "body": body })
            );
        }
        for body in [
            json!({ "pipeline": 1 }),
            json!({ "decision": "approve" }),
            json!({ "pipeline": 0, "decision": "approve" }),
            json!({ "pipeline": "1", "decision": "approve" }),
            // The owner approves or declines: passing a request on is the Product Manager's.
            json!({ "pipeline": 1, "decision": "escalate" }),
            json!({ "pipeline": 1, "decision": "reject" }),
            json!({ "pipeline": 1, "decision": "approve", "note": "x".repeat(601) }),
            json!({ "pipeline": 1, "decision": "approve", "reason": "x" }),
        ] {
            let errors = refusal(&json!({ "command": "data_pipeline_decide", "body": body }));
            assert!(!errors.is_empty(), "{body}");
        }
        read(
            "data_pipeline_decide",
            &json!({ "pipeline": 1, "decision": "approve", "note": "x".repeat(600) }),
        );
    }

    #[test]
    fn reads_folder_change_integrate() {
        let command = Command::FolderChangeIntegrate { change: 3 };
        assert_eq!(
            read("folder_change_integrate", &json!({ "change": 3 })),
            command
        );
        assert_eq!(
            command_to_value(&command),
            json!({ "command": "folder_change_integrate", "body": { "change": 3 } })
        );
        for body in [
            json!({}),
            json!({ "change": 0 }),
            json!({ "change": "3" }),
            json!({ "change": 3, "task_id": "CTV-1" }),
        ] {
            let errors = refusal(&json!({ "command": "folder_change_integrate", "body": body }));
            assert!(!errors.is_empty(), "{body}");
        }
    }

    #[test]
    fn reads_and_writes_the_renewal_dismiss_command() {
        let command = Command::RenewalDismiss { renewal: 7 };
        assert_eq!(read("renewal_dismiss", &json!({ "renewal": 7 })), command);
        assert_eq!(
            command_to_value(&command),
            json!({ "command": "renewal_dismiss", "body": { "renewal": 7 } })
        );
        for body in [
            json!({}),
            json!({ "renewal": 0 }),
            json!({ "renewal": "7" }),
            json!({ "renewal": 7, "note": "x" }),
        ] {
            let errors = refusal(&json!({ "command": "renewal_dismiss", "body": body }));
            assert!(!errors.is_empty(), "{body}");
        }
    }

    #[test]
    fn reads_and_writes_the_three_skill_commands() {
        let files = BTreeMap::from([
            ("SKILL.md".to_string(), "---\nname: a-b\n---\n".to_string()),
            ("references/a.md".to_string(), "details".to_string()),
        ]);
        let hash = "ab".repeat(32);
        for (name, body, command) in [
            (
                "skill_save",
                json!({ "level": "team", "files": files }),
                Command::SkillSave {
                    scope: SkillScope::Team,
                    files: files.clone(),
                    replace_shipped: false,
                },
            ),
            (
                "skill_save",
                json!({ "level": "agent", "agent": "dev-a", "files": files, "replace_shipped": true }),
                Command::SkillSave {
                    scope: SkillScope::Agent("dev-a".to_string()),
                    files: files.clone(),
                    replace_shipped: true,
                },
            ),
            (
                "skill_remove",
                json!({ "level": "agent", "agent": "dev-a", "name": "a-b" }),
                Command::SkillRemove {
                    scope: SkillScope::Agent("dev-a".to_string()),
                    name: "a-b".to_string(),
                },
            ),
            (
                "skill_remove",
                json!({ "level": "team", "name": "a-b" }),
                Command::SkillRemove {
                    scope: SkillScope::Team,
                    name: "a-b".to_string(),
                },
            ),
            (
                "skill_confirm",
                json!({ "level": "team", "name": "a-b", "sha256": hash }),
                Command::SkillConfirm {
                    scope: SkillScope::Team,
                    name: "a-b".to_string(),
                    sha256: hash.clone(),
                    replace_shipped: false,
                },
            ),
            (
                "skill_confirm",
                json!({ "level": "agent", "agent": "dev-a", "name": "a-b", "sha256": hash, "replace_shipped": true }),
                Command::SkillConfirm {
                    scope: SkillScope::Agent("dev-a".to_string()),
                    name: "a-b".to_string(),
                    sha256: hash.clone(),
                    replace_shipped: true,
                },
            ),
        ] {
            let wire = json!({ "command": name, "body": body });
            assert_eq!(command_from_value(&wire), Ok(command.clone()), "{wire}");
            assert_eq!(
                command_to_value(&command),
                wire,
                "written back as it was read"
            );
        }
    }

    #[test]
    fn refuses_a_skill_command_that_names_its_level_wrongly() {
        let files = json!({ "SKILL.md": "x" });
        let hash = "ab".repeat(32);
        // An agent level needs its agent, and a team level has none.
        for (name, body) in [
            ("skill_save", json!({ "level": "agent", "files": files })),
            (
                "skill_save",
                json!({ "level": "team", "agent": "dev-a", "files": files }),
            ),
            ("skill_remove", json!({ "level": "agent", "name": "a-b" })),
            (
                "skill_remove",
                json!({ "level": "team", "agent": "dev-a", "name": "a-b" }),
            ),
            (
                "skill_confirm",
                json!({ "level": "agent", "name": "a-b", "sha256": hash }),
            ),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert_eq!(errors.len(), 1, "{name} {body}: {errors:?}");
            assert_eq!(errors[0].path, "/body/agent", "{name} {body}");
        }
        // A hash is 64 lower-case hex digits; a name is a skill's; a body is another command's.
        for (name, body) in [
            (
                "skill_confirm",
                json!({ "level": "team", "name": "a-b", "sha256": "abc" }),
            ),
            ("skill_remove", json!({ "level": "team", "name": "A_B" })),
            ("skill_remove", json!({ "level": "team", "files": files })),
            ("skill_save", json!({ "level": "team", "name": "a-b" })),
        ] {
            let errors = refusal(&json!({ "command": name, "body": body }));
            assert!(!errors.is_empty(), "{name} {body}");
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
