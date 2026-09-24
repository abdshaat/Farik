//! Farik's events: `docs/schemas/event.schema.json` as Rust types, the reader that turns an
//! untrusted value into one, and the writer that turns one back.

use std::str::FromStr;
use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use farik_core::contract::{TaskId, ValidationError};

pub use crate::generated::event::{
    AgentSleptBody, AgentUpdatedBody, AgentUpdatedBodyStatus, BudgetExhaustedBody,
    BudgetExhaustedBodyConsequence, BudgetExhaustedBodyScope, ContractEvaluatedBody,
    ContractEvaluatedBodyGate, ContractJudgedBody, ContractLockedBody, ContractSummary,
    ContractSummaryKind, ContractSummaryParent, ContractSummaryRisk, ContractSummaryStatus,
    ContractUnlockedBody, ContractWrittenBody, CostRecordedBody, CostRecordedBodyModelId,
    CostRecordedBodyPurpose, CriteriaUpdatedBody, CriterionRecordedBody,
    CriterionRecordedBodyRunBy, DecisionWrittenBody, DriftDetectedBody, DriftDetectedBodyDrift,
    EscalationAgedBody, EscalationRaisedBody, EscalationRaisedBodyReason, EscalationResolvedBody,
    EventKind, HumanAcceptedBody, HumanAcceptedBodySubject, MemoryWrittenBody, MessagePostedBody,
    NoteWrittenBody, NoteWrittenBodyKind, ProductDocWrittenBody, ProjectScannedBody,
    PullRequestOpenedBody, QuestionAnsweredBody, QuestionAskedBody, RequestTriagedBody,
    RequestTriagedBodySize, RetroAppendedBody, ReviewRecordedBody, SessionEndedBody,
    SessionEndedBodyReason, SessionStartedBody, SessionStartedBodyEffort, SessionStartedBodyModel,
    SessionStartedBodyPurpose, SprintEndedBody, SprintEndedBodyEndedBy, SprintPlannedBody,
    SprintStartedBody, TaskCreatedBody, TaskIntegratedBody, TaskIntegratedBodyIntegratedBy,
    TaskTransitionedBody, TaskTransitionedBodyEffectsItem, TeamUpdatedBody, TokenUsage,
    ToolCalledBody, ToolDeniedBody, ToolReturnedBody, TransitionRefusedBody,
    TransitionRefusedBodyRefusal,
};
/// The generated names of the vocabularies the governor's events repeat, renamed at the edge so
/// that they cannot be mistaken for `farik-core`'s own types of the same name.
pub use crate::generated::event::{
    BlockerWire, GateId as GateWire, RejectionWire, TaskStatus as TaskStatusWire,
    TransitionActor as TransitionActorWire,
};
/// The channel's vocabularies, named for what they are rather than for the body they sit in.
pub use crate::generated::event::{MessagePostedBodyKind as MessageKind, Thread};

use crate::generated::event::FarikEvent as EventWire;

/// Builders for test events, usable by every crate's tests.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/event.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded event schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded event schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// One validator per kind, each holding that kind's body schema alone. The event schema types
/// `body` as a choice of forty-one shapes, so it can only say that a body matched none of them; these
/// say what is wrong with the one shape the event's `kind` asked for.
static BODY_VALIDATORS: LazyLock<Vec<Validator>> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded event schema is valid JSON: the whole-event validator above parses the \
         same text",
    );
    let defs = schema
        .get("$defs")
        .cloned()
        .expect("the embedded event schema has $defs: every body shape is defined there");
    EVERY_KIND
        .iter()
        .map(|kind| {
            let body = serde_json::json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "$ref": format!("#/$defs/{}", body_def_name(*kind)),
                "$defs": defs,
            });
            jsonschema::options()
                .should_validate_formats(true)
                .build(&body)
                .expect(
                    "a body schema compiles: it is one $ref into the $defs of a schema the \
                     generator already parsed",
                )
        })
        .collect()
});

/// The name `docs/schemas/event.schema.json` gives one kind's body shape.
fn body_def_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::TaskCreated => "taskCreatedBody",
        EventKind::RequestTriaged => "requestTriagedBody",
        EventKind::ContractWritten => "contractWrittenBody",
        EventKind::ContractLocked => "contractLockedBody",
        EventKind::ContractUnlocked => "contractUnlockedBody",
        EventKind::DriftDetected => "driftDetectedBody",
        EventKind::ProjectScanned => "projectScannedBody",
        EventKind::TeamUpdated => "teamUpdatedBody",
        EventKind::CriteriaUpdated => "criteriaUpdatedBody",
        EventKind::CostRecorded => "costRecordedBody",
        EventKind::BudgetExhausted => "budgetExhaustedBody",
        EventKind::TaskTransitioned => "taskTransitionedBody",
        EventKind::TransitionRefused => "transitionRefusedBody",
        EventKind::EscalationRaised => "escalationRaisedBody",
        EventKind::ContractEvaluated => "contractEvaluatedBody",
        EventKind::ContractJudged => "contractJudgedBody",
        EventKind::CriterionRecorded => "criterionRecordedBody",
        EventKind::NoteWritten => "noteWrittenBody",
        EventKind::ReviewRecorded => "reviewRecordedBody",
        EventKind::QuestionAsked => "questionAskedBody",
        EventKind::ProductDocWritten => "productDocWrittenBody",
        EventKind::ToolCalled => "toolCalledBody",
        EventKind::ToolDenied => "toolDeniedBody",
        EventKind::ToolReturned => "toolReturnedBody",
        EventKind::SessionStarted => "sessionStartedBody",
        EventKind::SessionEnded => "sessionEndedBody",
        EventKind::TaskIntegrated => "taskIntegratedBody",
        EventKind::PullRequestOpened => "pullRequestOpenedBody",
        EventKind::QuestionAnswered => "questionAnsweredBody",
        EventKind::HumanAccepted => "humanAcceptedBody",
        EventKind::EscalationResolved => "escalationResolvedBody",
        EventKind::AgentUpdated => "agentUpdatedBody",
        EventKind::SprintStarted => "sprintStartedBody",
        EventKind::SprintPlanned => "sprintPlannedBody",
        EventKind::SprintEnded => "sprintEndedBody",
        EventKind::AgentSlept => "agentSleptBody",
        EventKind::MessagePosted => "messagePostedBody",
        EventKind::RetroAppended => "retroAppendedBody",
        EventKind::EscalationAged => "escalationAgedBody",
        EventKind::MemoryWritten => "memoryWrittenBody",
        EventKind::DecisionWritten => "decisionWrittenBody",
    }
}

/// Whether an event of this kind is about one contract, and so may not be recorded without naming
/// it. The log is append-only: an event saying that something was locked, by someone, with no id,
/// can never afterwards be attached to the contract it was about.
#[must_use]
pub fn is_about_one_contract(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::TaskCreated
            | EventKind::RequestTriaged
            | EventKind::ContractWritten
            | EventKind::ContractLocked
            | EventKind::ContractUnlocked
            | EventKind::TaskTransitioned
            | EventKind::TransitionRefused
            | EventKind::EscalationRaised
            | EventKind::ContractEvaluated
            | EventKind::ContractJudged
            | EventKind::CriterionRecorded
            | EventKind::NoteWritten
            | EventKind::ReviewRecorded
            | EventKind::ProductDocWritten
            | EventKind::TaskIntegrated
            | EventKind::PullRequestOpened
            | EventKind::HumanAccepted
            | EventKind::EscalationResolved
            | EventKind::EscalationAged
    )
}

/// The field naming who acted, for the kinds that name one, and nothing for `drift.detected`,
/// `project.scanned`, `cost.recorded`, `budget.exhausted`, `escalation.raised`,
/// `contract.evaluated`, and `pull_request.opened`, which record what Farik itself found, counted,
/// judged, or did; the move or the refusal they come with names who asked. `task.integrated` and
/// `sprint.ended` name who acted in a closed vocabulary, `governor` or `human`, which cannot be
/// blank. Nor for the three `tool.` kinds, the two `session.` kinds, and `agent.slept`, whose
/// envelope names the agent and the session; Farik observed the sleep, and nobody asked for it.
/// Nor for `escalation.aged`: the human left it waiting, and nobody acted.
fn attribution(body: &mut EventBody) -> Option<(&'static str, &mut String)> {
    match body {
        EventBody::TaskCreated(body) => Some(("created_by", &mut body.created_by)),
        EventBody::RequestTriaged(body) => Some(("triaged_by", &mut body.triaged_by)),
        EventBody::ContractWritten(body) => Some(("written_by", &mut body.written_by)),
        EventBody::ContractLocked(body) => Some(("locked_by", &mut body.locked_by)),
        EventBody::ContractUnlocked(body) => Some(("unlocked_by", &mut body.unlocked_by)),
        EventBody::TeamUpdated(body) => Some(("updated_by", &mut body.updated_by)),
        EventBody::CriteriaUpdated(body) => Some(("updated_by", &mut body.updated_by)),
        EventBody::TaskTransitioned(body) => Some(("requested_by", &mut body.requested_by)),
        EventBody::TransitionRefused(body) => Some(("requested_by", &mut body.requested_by)),
        EventBody::ContractJudged(body) => Some(("judged_by", &mut body.judged_by)),
        EventBody::CriterionRecorded(body) => Some(("recorded_by", &mut body.recorded_by)),
        EventBody::NoteWritten(body) => Some(("written_by", &mut body.written_by)),
        EventBody::ReviewRecorded(body) => Some(("reviewer", &mut body.reviewer)),
        EventBody::QuestionAsked(body) => Some(("asked_by", &mut body.asked_by)),
        EventBody::ProductDocWritten(body) => Some(("written_by", &mut body.written_by)),
        EventBody::QuestionAnswered(body) => Some(("answered_by", &mut body.answered_by)),
        EventBody::HumanAccepted(body) => Some(("accepted_by", &mut body.accepted_by)),
        EventBody::EscalationResolved(body) => Some(("resolved_by", &mut body.resolved_by)),
        EventBody::AgentUpdated(body) => Some(("updated_by", &mut body.updated_by)),
        EventBody::SprintStarted(body) => Some(("started_by", &mut body.started_by)),
        EventBody::SprintPlanned(body) => Some(("planned_by", &mut body.planned_by)),
        EventBody::MessagePosted(body) => Some(("author", &mut body.author)),
        EventBody::RetroAppended(body) => Some(("appended_by", &mut body.appended_by)),
        EventBody::MemoryWritten(body) => Some(("written_by", &mut body.written_by)),
        EventBody::DecisionWritten(body) => Some(("written_by", &mut body.written_by)),
        EventBody::DriftDetected(_)
        | EventBody::ProjectScanned(_)
        | EventBody::CostRecorded(_)
        | EventBody::BudgetExhausted(_)
        | EventBody::EscalationRaised(_)
        | EventBody::ContractEvaluated(_)
        | EventBody::ToolCalled(_)
        | EventBody::ToolDenied(_)
        | EventBody::ToolReturned(_)
        | EventBody::SessionStarted(_)
        | EventBody::SessionEnded(_)
        | EventBody::TaskIntegrated(_)
        | EventBody::SprintEnded(_)
        | EventBody::AgentSlept(_)
        | EventBody::PullRequestOpened(_)
        | EventBody::EscalationAged(_) => None,
    }
}

/// Every kind the log holds in this phase, in the order `docs/schemas/event.schema.json` lists
/// them. The step that adds a kind adds it here.
pub const EVERY_KIND: [EventKind; 41] = [
    EventKind::TaskCreated,
    EventKind::RequestTriaged,
    EventKind::ContractWritten,
    EventKind::ContractLocked,
    EventKind::ContractUnlocked,
    EventKind::DriftDetected,
    EventKind::ProjectScanned,
    EventKind::TeamUpdated,
    EventKind::CriteriaUpdated,
    EventKind::CostRecorded,
    EventKind::BudgetExhausted,
    EventKind::TaskTransitioned,
    EventKind::TransitionRefused,
    EventKind::EscalationRaised,
    EventKind::ContractEvaluated,
    EventKind::ContractJudged,
    EventKind::CriterionRecorded,
    EventKind::NoteWritten,
    EventKind::ReviewRecorded,
    EventKind::QuestionAsked,
    EventKind::ProductDocWritten,
    EventKind::ToolCalled,
    EventKind::ToolDenied,
    EventKind::ToolReturned,
    EventKind::SessionStarted,
    EventKind::SessionEnded,
    EventKind::TaskIntegrated,
    EventKind::PullRequestOpened,
    EventKind::QuestionAnswered,
    EventKind::HumanAccepted,
    EventKind::EscalationResolved,
    EventKind::AgentUpdated,
    EventKind::SprintStarted,
    EventKind::SprintPlanned,
    EventKind::SprintEnded,
    EventKind::AgentSlept,
    EventKind::MessagePosted,
    EventKind::RetroAppended,
    EventKind::EscalationAged,
    EventKind::MemoryWritten,
    EventKind::DecisionWritten,
];

/// The ids an event is stamped with: which team and project it belongs to, and the contract, agent
/// and session it is about when it is about one.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct EventIds {
    /// The team the event belongs to. Blank is refused.
    pub team_id: String,
    /// The project the event belongs to. Blank is refused.
    pub project_id: String,
    /// The contract the event is about, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The agent whose work produced the event, when an agent did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// The session the event was recorded in, when it was recorded in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Everything one event records except what happened: where it belongs and when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EventEnvelope {
    /// The event's place in the log, assigned by the store on append.
    pub seq: u64,
    /// When the event was recorded, from the injected clock.
    pub recorded_at: DateTime<Utc>,
    /// The ids the event is stamped with.
    #[serde(flatten)]
    pub ids: EventIds,
}

/// What happened: one variant per event kind, each holding that kind's body. On the wire it is the
/// event's `kind` and `body` side by side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body")]
pub enum EventBody {
    /// A request was filed as a draft contract.
    #[serde(rename = "task.created")]
    TaskCreated(TaskCreatedBody),
    /// Triage sized a request.
    #[serde(rename = "request.triaged")]
    RequestTriaged(RequestTriagedBody),
    /// A contract's content was written or changed.
    #[serde(rename = "contract.written")]
    ContractWritten(ContractWrittenBody),
    /// A human took ownership of a contract.
    #[serde(rename = "contract.locked")]
    ContractLocked(ContractLockedBody),
    /// A human gave a contract back to the team.
    #[serde(rename = "contract.unlocked")]
    ContractUnlocked(ContractUnlockedBody),
    /// Reconciliation found the files and the log disagreeing.
    #[serde(rename = "drift.detected")]
    DriftDetected(DriftDetectedBody),
    /// The project scan read the repository back to the user.
    #[serde(rename = "project.scanned")]
    ProjectScanned(ProjectScannedBody),
    /// The team file was written.
    #[serde(rename = "team.updated")]
    TeamUpdated(TeamUpdatedBody),
    /// The criterion library was written.
    #[serde(rename = "criteria.updated")]
    CriteriaUpdated(CriteriaUpdatedBody),
    /// A session's usage report was priced.
    #[serde(rename = "cost.recorded")]
    CostRecorded(CostRecordedBody),
    /// A budget ran out.
    #[serde(rename = "budget.exhausted")]
    BudgetExhausted(BudgetExhaustedBody),
    /// The governor moved a contract.
    #[serde(rename = "task.transitioned")]
    TaskTransitioned(TaskTransitionedBody),
    /// The governor refused to move a contract.
    #[serde(rename = "transition.refused")]
    TransitionRefused(TransitionRefusedBody),
    /// A move sent a contract to the human.
    #[serde(rename = "escalation.raised")]
    EscalationRaised(EscalationRaisedBody),
    /// A contract was held to the Definition of Ready or of Done.
    #[serde(rename = "contract.evaluated")]
    ContractEvaluated(ContractEvaluatedBody),
    /// The Scrum Master judged a contract against the Definition of Ready's judgment rules.
    #[serde(rename = "contract.judged")]
    ContractJudged(ContractJudgedBody),
    /// An agent recorded an exit criterion's result.
    #[serde(rename = "criterion.recorded")]
    CriterionRecorded(CriterionRecordedBody),
    /// An agent wrote a note about the work.
    #[serde(rename = "note.written")]
    NoteWritten(NoteWrittenBody),
    /// A reviewer's verification of a contract was summed up.
    #[serde(rename = "review.recorded")]
    ReviewRecorded(ReviewRecordedBody),
    /// An agent asked the human a question.
    #[serde(rename = "question.asked")]
    QuestionAsked(QuestionAskedBody),
    /// The Product Manager wrote a product document.
    #[serde(rename = "product_doc.written")]
    ProductDocWritten(ProductDocWrittenBody),
    /// The `PreToolUse` hook allowed a tool call.
    #[serde(rename = "tool.called")]
    ToolCalled(ToolCalledBody),
    /// The `PreToolUse` hook denied a tool call.
    #[serde(rename = "tool.denied")]
    ToolDenied(ToolDeniedBody),
    /// A tool call returned, as the `PostToolUse` hook reported it.
    #[serde(rename = "tool.returned")]
    ToolReturned(ToolReturnedBody),
    /// A Claude Code session started.
    #[serde(rename = "session.started")]
    SessionStarted(SessionStartedBody),
    /// A Claude Code session ended.
    #[serde(rename = "session.ended")]
    SessionEnded(SessionEndedBody),
    /// An accepted task's branch reached the integration branch.
    #[serde(rename = "task.integrated")]
    TaskIntegrated(TaskIntegratedBody),
    /// Farik opened a pull request for an accepted task.
    #[serde(rename = "pull_request.opened")]
    PullRequestOpened(PullRequestOpenedBody),
    /// The human answered a question.
    #[serde(rename = "question.answered")]
    QuestionAnswered(QuestionAnsweredBody),
    /// The human approved a contract or accepted a result.
    #[serde(rename = "human.accepted")]
    HumanAccepted(HumanAcceptedBody),
    /// The human resolved an escalation.
    #[serde(rename = "escalation.resolved")]
    EscalationResolved(EscalationResolvedBody),
    /// An agent's status changed in the team file.
    #[serde(rename = "agent.updated")]
    AgentUpdated(AgentUpdatedBody),
    /// The human started a sprint.
    #[serde(rename = "sprint.started")]
    SprintStarted(SprintStartedBody),
    /// Tasks were planned into the open sprint.
    #[serde(rename = "sprint.planned")]
    SprintPlanned(SprintPlannedBody),
    /// A sprint ended, by itself or by the human.
    #[serde(rename = "sprint.ended")]
    SprintEnded(SprintEndedBody),
    /// An agent's model provider refused it at a usage limit; it sleeps until the limit resets.
    #[serde(rename = "agent.slept")]
    AgentSlept(AgentSleptBody),
    /// Somebody said something in the team's channel.
    #[serde(rename = "message.posted")]
    MessagePosted(MessagePostedBody),
    /// The retro ceremony recorded what the next planning should know.
    #[serde(rename = "retro.appended")]
    RetroAppended(RetroAppendedBody),
    /// The aged rule found an open escalation that has waited past the team's
    /// `escalation_age_hours` on the human.
    #[serde(rename = "escalation.aged")]
    EscalationAged(EscalationAgedBody),
    /// An agent replaced its notebook.
    #[serde(rename = "memory.written")]
    MemoryWritten(MemoryWrittenBody),
    /// The Architect or the Product Manager recorded a decision, which is never written over.
    #[serde(rename = "decision.written")]
    DecisionWritten(DecisionWrittenBody),
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
            Self::CostRecorded(_) => EventKind::CostRecorded,
            Self::BudgetExhausted(_) => EventKind::BudgetExhausted,
            Self::TaskTransitioned(_) => EventKind::TaskTransitioned,
            Self::TransitionRefused(_) => EventKind::TransitionRefused,
            Self::EscalationRaised(_) => EventKind::EscalationRaised,
            Self::ContractEvaluated(_) => EventKind::ContractEvaluated,
            Self::ContractJudged(_) => EventKind::ContractJudged,
            Self::CriterionRecorded(_) => EventKind::CriterionRecorded,
            Self::NoteWritten(_) => EventKind::NoteWritten,
            Self::ReviewRecorded(_) => EventKind::ReviewRecorded,
            Self::QuestionAsked(_) => EventKind::QuestionAsked,
            Self::ProductDocWritten(_) => EventKind::ProductDocWritten,
            Self::ToolCalled(_) => EventKind::ToolCalled,
            Self::ToolDenied(_) => EventKind::ToolDenied,
            Self::ToolReturned(_) => EventKind::ToolReturned,
            Self::SessionStarted(_) => EventKind::SessionStarted,
            Self::SessionEnded(_) => EventKind::SessionEnded,
            Self::TaskIntegrated(_) => EventKind::TaskIntegrated,
            Self::PullRequestOpened(_) => EventKind::PullRequestOpened,
            Self::QuestionAnswered(_) => EventKind::QuestionAnswered,
            Self::HumanAccepted(_) => EventKind::HumanAccepted,
            Self::EscalationResolved(_) => EventKind::EscalationResolved,
            Self::AgentUpdated(_) => EventKind::AgentUpdated,
            Self::SprintStarted(_) => EventKind::SprintStarted,
            Self::SprintPlanned(_) => EventKind::SprintPlanned,
            Self::SprintEnded(_) => EventKind::SprintEnded,
            Self::AgentSlept(_) => EventKind::AgentSlept,
            Self::MessagePosted(_) => EventKind::MessagePosted,
            Self::RetroAppended(_) => EventKind::RetroAppended,
            Self::EscalationAged(_) => EventKind::EscalationAged,
            Self::MemoryWritten(_) => EventKind::MemoryWritten,
            Self::DecisionWritten(_) => EventKind::DecisionWritten,
        }
    }
}

/// One record of the log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FarikEvent {
    /// Where the event belongs and when it was recorded.
    #[serde(flatten)]
    pub envelope: EventEnvelope,
    /// What happened.
    #[serde(flatten)]
    pub body: EventBody,
}

/// An event that has not been appended yet: everything a `FarikEvent` has except the sequence
/// number, which the store assigns on append.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    /// When the event was recorded, from the injected clock.
    pub recorded_at: DateTime<Utc>,
    /// The ids the event is stamped with.
    pub ids: EventIds,
    /// What happened.
    pub body: EventBody,
}

/// Why an event cannot be stamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    /// An id that must name something was blank once trimmed.
    BlankId {
        /// The field: `team_id`, `project_id`, or the body's own field naming who acted.
        field: String,
    },
    /// An event about one contract was stamped without naming it.
    NoContractNamed {
        /// The kind that is about one contract.
        kind: EventKind,
    },
}

/// Stamps a body with the time it was recorded and the ids it belongs to. Trims every id and drops
/// an optional one that is blank, because a blank id names nobody and the log would show an agent
/// or a session that does not exist.
///
/// # Errors
///
/// `BlankId` when `team_id`, `project_id`, or the body's own field naming who acted is blank once
/// trimmed, in that order; `NoContractNamed` when the body's kind is about one contract
/// (`is_about_one_contract`) and `ids` names none.
pub fn new_event(
    body: EventBody,
    recorded_at: DateTime<Utc>,
    ids: EventIds,
) -> Result<NewEvent, EventError> {
    let team_id = named(&ids.team_id, "team_id")?;
    let project_id = named(&ids.project_id, "project_id")?;
    let mut body = body;
    if let Some((field, actor)) = attribution(&mut body) {
        *actor = named(actor, field)?;
    }
    if is_about_one_contract(body.kind()) && ids.task_id.is_none() {
        return Err(EventError::NoContractNamed { kind: body.kind() });
    }
    Ok(NewEvent {
        recorded_at,
        ids: EventIds {
            team_id,
            project_id,
            task_id: ids.task_id,
            agent_id: optional_id(ids.agent_id.as_deref()),
            session_id: optional_id(ids.session_id.as_deref()),
        },
        body,
    })
}

fn named(value: &str, field: &str) -> Result<String, EventError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EventError::BlankId {
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
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
    let errors = schema_errors(input);
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
    if is_about_one_contract(wire.kind) && envelope.ids.task_id.is_none() {
        return Err(vec![ValidationError {
            path: "/task_id".to_string(),
            message: format!(
                "a {} event is about one contract and names none, and an append-only log cannot \
                 attach it to one afterwards",
                wire.kind
            ),
        }]);
    }
    // The wire enum answers "which body is this?" by shape; the kind is what the event says it
    // is, so the body is read by kind and one that does not fit is refused.
    let mut body = serde_json::from_value::<EventBody>(
        serde_json::json!({ "kind": wire.kind, "body": input["body"] }),
    )
    .map_err(|error| {
        vec![ValidationError {
            path: "/body".to_string(),
            message: format!("a {} event does not carry this body: {error}", wire.kind),
        }]
    })?;
    if let Some((field, actor)) = attribution(&mut body) {
        let named = actor.trim();
        if named.is_empty() {
            return Err(vec![ValidationError {
                path: format!("/body/{field}"),
                message: "is blank, and an event the log cannot attribute to whoever acted is not \
                          correctable once it is appended"
                    .to_string(),
            }]);
        }
        *actor = named.to_string();
    }
    Ok(FarikEvent { envelope, body })
}

/// The schema's own failures. A failure inside `body` is reported by the schema once, at `/body`,
/// because `body` there is a choice of forty-one shapes and the schema can only say that none matched.
/// The event's `kind` says which one it was meant to be, so such a failure is asked again of that
/// shape alone and reported where it actually is.
fn schema_errors(input: &Value) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();
    for error in VALIDATOR.iter_errors(input) {
        let path = pointer(&error.instance_path().to_string());
        match (path.as_str(), body_errors(input)) {
            ("/body", Some(inside)) => errors.extend(inside),
            _ => errors.push(ValidationError {
                path,
                message: error.to_string(),
            }),
        }
    }
    errors
}

/// What the body gets wrong when held to its own kind's shape, at paths under `/body`. `None` when
/// the kind is not one this crate knows, or when that shape accepts the body after all.
fn body_errors(input: &Value) -> Option<Vec<ValidationError>> {
    let kind: EventKind = input.get("kind")?.as_str()?.parse().ok()?;
    let index = EVERY_KIND.iter().position(|known| *known == kind)?;
    let errors: Vec<ValidationError> = BODY_VALIDATORS[index]
        .iter_errors(input.get("body")?)
        .map(|error| ValidationError {
            path: format!("/body{}", error.instance_path()),
            message: error.to_string(),
        })
        .collect();
    (!errors.is_empty()).then_some(errors)
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
        ids: EventIds {
            team_id,
            project_id,
            task_id,
            agent_id: optional_id(wire.agent_id.as_deref()),
            session_id: optional_id(wire.session_id.as_deref()),
        },
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

/// One event as the canonical wire value the log holds. `event_from_value` accepts it back, but
/// it is not byte-for-byte what that function was given: a timestamp is written in UTC in one
/// spelling whatever spelling it arrived in, and every id is written trimmed.
///
/// # Panics
///
/// Never: serialising derived strings, numbers and timestamps under string keys cannot fail.
#[must_use]
pub fn event_to_value(event: &FarikEvent) -> Value {
    serde_json::to_value(event).expect(
        "an event serialises: every field is a derived string, number or timestamp under a string \
         key, which serde_json cannot refuse",
    )
}

/// One body as the canonical wire value its kind's schema describes. The store holds this rather
/// than the whole event, because the envelope's fields are the log's own columns.
///
/// # Panics
///
/// Never: serialising derived strings and lists of strings under string keys cannot fail.
#[must_use]
pub fn body_to_value(body: &EventBody) -> Value {
    let mut tagged = serde_json::to_value(body).expect(
        "a body serialises: every field is a derived string or list of strings under a string \
         key, which serde_json cannot refuse",
    );
    tagged["body"].take()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{a_body_wire, a_contract_summary_wire, a_full_event_wire, an_event_wire};
    use super::{
        EVERY_KIND, EventBody, EventError, EventIds, EventKind, ValidationError, event_from_value,
        event_to_value, new_event,
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
            assert_eq!(event.envelope.ids.team_id, "farik");
            assert_eq!(
                event.envelope.ids.task_id.is_some(),
                super::is_about_one_contract(kind),
                "{kind}"
            );
        }
    }

    #[test]
    fn reads_every_optional_field_of_the_envelope() {
        let event = event_from_value(&a_full_event_wire(EventKind::TaskCreated)).expect("valid");
        assert_eq!(
            event.envelope.ids.task_id.as_ref().map(|id| id.to_string()),
            Some("FRK-1".to_string())
        );
        assert_eq!(event.envelope.ids.agent_id.as_deref(), Some("maya-chen"));
        assert_eq!(event.envelope.ids.session_id.as_deref(), Some("session-1"));
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
            for other in EVERY_KIND {
                if other == kind {
                    continue;
                }
                let mut input = an_event_wire(kind);
                input["body"] = a_body_wire(other);
                let errors = refusal(&input);
                assert_eq!(errors.len(), 1, "{kind} carrying {other}");
                assert_eq!(errors[0].path, "/body", "{kind} carrying {other}");
                assert!(
                    errors[0]
                        .message
                        .starts_with(&format!("a {kind} event does not carry this body")),
                    "{}",
                    errors[0].message
                );
            }
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
        assert_eq!(event.envelope.ids.team_id, "farik");
        assert_eq!(event.envelope.ids.agent_id.as_deref(), Some("maya-chen"));
        assert!(event.envelope.ids.session_id.is_none());
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
        for name in [
            "question.answered",
            "human.accepted",
            "escalation.resolved",
            "agent.updated",
        ] {
            let kind: EventKind =
                serde_json::from_value(json!(name)).unwrap_or_else(|_| panic!("{name} is a kind"));
            assert!(EVERY_KIND.contains(&kind), "{name}");
        }
        let mut moved = a_full_event_wire(EventKind::TaskTransitioned);
        moved["body"]["reason"] = json!("stopped by the human");
        let event = event_from_value(&moved).expect("a move may carry the human's reason");
        assert_eq!(event_to_value(&event), moved);
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
        // A kind that is not about one contract, since one that is may not leave out `task_id`.
        let event = event_from_value(&an_event_wire(EventKind::ProjectScanned)).expect("valid");
        let wire = event_to_value(&event);
        for absent in ["task_id", "agent_id", "session_id"] {
            assert!(wire.get(absent).is_none(), "{absent}");
        }
    }

    fn some_ids() -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            task_id: None,
            agent_id: None,
            session_id: None,
        }
    }

    /// A body of a kind that is about no one contract, so that `some_ids` naming none is enough.
    fn a_body() -> EventBody {
        let event = event_from_value(&an_event_wire(EventKind::ProjectScanned)).expect("valid");
        event.body
    }

    fn at() -> chrono::DateTime<chrono::Utc> {
        let event = event_from_value(&an_event_wire(EventKind::ProjectScanned)).expect("valid");
        event.envelope.recorded_at
    }

    #[test]
    fn stamps_a_body_with_the_time_and_the_ids_it_belongs_to() {
        let new = new_event(a_body(), at(), some_ids()).expect("stamped");
        assert_eq!(new.ids.team_id, "farik");
        assert_eq!(new.ids.project_id, "farik");
        assert_eq!(new.recorded_at, at());
        assert_eq!(new.body.kind(), EventKind::ProjectScanned);
        assert!(new.ids.task_id.is_none());
    }

    #[test]
    fn stamps_every_id_trimmed_and_forgets_an_optional_one_that_is_blank() {
        let ids = EventIds {
            team_id: "  farik  ".to_string(),
            project_id: "  farik  ".to_string(),
            agent_id: Some("  maya-chen  ".to_string()),
            session_id: Some("   ".to_string()),
            ..some_ids()
        };
        let new = new_event(a_body(), at(), ids).expect("stamped");
        assert_eq!(new.ids.team_id, "farik");
        assert_eq!(new.ids.project_id, "farik");
        assert_eq!(new.ids.agent_id.as_deref(), Some("maya-chen"));
        assert!(new.ids.session_id.is_none());
    }

    #[test]
    fn refuses_to_stamp_an_event_with_a_blank_team_id_or_project_id() {
        // A blank one names nobody, and the log would hold a record that cannot be attributed or
        // read back. The reader refuses the same thing; this is the other door into the log.
        for (field, ids) in [
            (
                "team_id",
                EventIds {
                    team_id: "   ".to_string(),
                    ..some_ids()
                },
            ),
            (
                "project_id",
                EventIds {
                    project_id: String::new(),
                    ..some_ids()
                },
            ),
        ] {
            let error = new_event(a_body(), at(), ids).expect_err("expected a refusal");
            assert_eq!(
                error,
                EventError::BlankId {
                    field: field.to_string()
                }
            );
        }
    }

    #[test]
    fn reports_a_malformed_field_inside_a_body_at_its_own_path() {
        // The schema types `body` as a choice of forty-one shapes, so it reports a failure anywhere
        // inside one at `/body`, with the whole body echoed back. The kind says which shape the
        // body was meant to be, so the reader checks it again against that one alone.
        let mut input = an_event_wire(EventKind::ProjectScanned);
        input["body"]["detected_criteria"] = json!([1, 2]);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 2);
        assert_eq!(
            errors
                .iter()
                .map(|error| error.path.as_str())
                .collect::<Vec<&str>>(),
            ["/body/detected_criteria/0", "/body/detected_criteria/1"]
        );
    }

    #[test]
    fn reports_an_unknown_field_inside_a_summary_at_its_own_path() {
        let mut input = an_event_wire(EventKind::TaskCreated);
        input["body"]["summary"]["assignee"] = json!("maya-chen");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/summary");
    }

    #[test]
    fn refuses_a_contract_event_that_names_no_contract() {
        // task.created, request.triaged and the three contract events are each about one
        // contract, and the log is append-only: an event recording that something was locked by
        // someone, with no id, can never be attached to the contract it was about.
        for kind in EVERY_KIND {
            let mut input = an_event_wire(kind);
            let removed = input
                .as_object_mut()
                .expect("an event is an object")
                .remove("task_id");
            if !super::is_about_one_contract(kind) {
                assert!(removed.is_none(), "{kind}");
                continue;
            }
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{kind}");
            assert_eq!(errors[0].path, "/task_id", "{kind}");
            assert!(
                errors[0]
                    .message
                    .starts_with(&format!("a {kind} event is about one contract")),
                "{}",
                errors[0].message
            );
        }
    }

    #[test]
    fn refuses_an_event_whose_actor_is_blank() {
        // Every kind that names who acted is refused without one. The governance rules of
        // docs/SPEC.md section 5 rest on that attribution, and the log cannot correct it later.
        for (kind, field) in [
            (EventKind::TaskCreated, "created_by"),
            (EventKind::RequestTriaged, "triaged_by"),
            (EventKind::ContractWritten, "written_by"),
            (EventKind::ContractLocked, "locked_by"),
            (EventKind::ContractUnlocked, "unlocked_by"),
            (EventKind::TeamUpdated, "updated_by"),
            (EventKind::CriteriaUpdated, "updated_by"),
            (EventKind::TaskTransitioned, "requested_by"),
            (EventKind::TransitionRefused, "requested_by"),
            (EventKind::ContractJudged, "judged_by"),
            (EventKind::CriterionRecorded, "recorded_by"),
            (EventKind::NoteWritten, "written_by"),
            (EventKind::ReviewRecorded, "reviewer"),
            (EventKind::QuestionAsked, "asked_by"),
            (EventKind::ProductDocWritten, "written_by"),
        ] {
            let mut input = an_event_wire(kind);
            input["body"][field] = json!("   ");
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{kind}");
            assert_eq!(errors[0].path, format!("/body/{field}"), "{kind}");
        }
    }

    #[test]
    fn trims_the_actor_it_reads() {
        let mut input = an_event_wire(EventKind::ContractLocked);
        input["body"]["locked_by"] = json!("  human  ");
        let event = event_from_value(&input).expect("valid");
        let EventBody::ContractLocked(body) = event.body else {
            panic!("a contract.locked event carries a contract.locked body");
        };
        assert_eq!(body.locked_by, "human");
    }

    #[test]
    fn writes_the_canonical_form_of_a_timestamp_it_read() {
        // event_to_value writes the canonical wire form, not the bytes it was given: the log
        // holds one spelling of a moment, so step 02 must not assume the value it reads back is
        // byte-identical to the one it passed in.
        for (given, canonical) in [
            ("2026-09-17T12:00:00+02:00", "2026-09-17T10:00:00Z"),
            ("2026-09-17T10:00:00.000Z", "2026-09-17T10:00:00Z"),
            ("2026-09-17t10:00:00z", "2026-09-17T10:00:00Z"),
            ("2026-09-17T10:00:00.500Z", "2026-09-17T10:00:00.500Z"),
        ] {
            let mut input = an_event_wire(EventKind::ProjectScanned);
            input["recorded_at"] = json!(given);
            let event = event_from_value(&input).expect("valid");
            assert_eq!(
                event_to_value(&event)["recorded_at"],
                json!(canonical),
                "{given}"
            );
        }
    }

    #[test]
    fn refuses_to_stamp_a_contract_event_that_names_no_contract() {
        let body = event_from_value(&an_event_wire(EventKind::ContractLocked))
            .expect("valid")
            .body;
        let error = new_event(body, at(), some_ids()).expect_err("expected a refusal");
        assert_eq!(
            error,
            EventError::NoContractNamed {
                kind: EventKind::ContractLocked
            }
        );
    }

    #[test]
    fn reads_a_cost_without_unpriced_as_priced() {
        let mut input = an_event_wire(EventKind::CostRecorded);
        input["body"]
            .as_object_mut()
            .expect("the body is an object")
            .remove("unpriced");
        let event = event_from_value(&input).expect("an older log's cost reads");
        let EventBody::CostRecorded(body) = &event.body else {
            panic!("a cost.recorded, not {:?}", event.body.kind());
        };
        assert!(!body.unpriced);
    }

    #[test]
    fn keeps_an_unpriced_cost() {
        let mut input = an_event_wire(EventKind::CostRecorded);
        input["body"]["unpriced"] = json!(true);
        input["body"]["cost_usd"] = json!(0);
        let event = event_from_value(&input).expect("an unpriced cost is valid");
        let EventBody::CostRecorded(body) = &event.body else {
            panic!("a cost.recorded, not {:?}", event.body.kind());
        };
        assert!(body.unpriced);
        assert_eq!(event_to_value(&event)["body"]["unpriced"], json!(true));
    }

    #[test]
    fn records_the_new_consequence_and_reads_the_old() {
        for consequence in ["end_session_with_note", "end_session_and_block_task"] {
            let mut input = an_event_wire(EventKind::BudgetExhausted);
            input["body"] = json!({ "scope": "session_wall_clock", "consequence": consequence });
            let read = event_from_value(&input);
            assert!(read.is_ok(), "{consequence}: {read:?}");
            let event = read.expect("checked above");
            assert_eq!(
                event_to_value(&event)["body"]["consequence"],
                json!(consequence)
            );
        }
    }

    #[test]
    fn refuses_a_cost_with_a_negative_amount() {
        let mut input = an_event_wire(EventKind::CostRecorded);
        input["body"]["cost_usd"] = json!(-0.01);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.ends_with("/cost_usd"), "{}", errors[0].path);
    }

    #[test]
    fn refuses_a_cost_with_a_property_it_does_not_know() {
        let mut input = an_event_wire(EventKind::CostRecorded);
        input["body"]["discount_usd"] = json!(1.0);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body");
    }

    #[test]
    fn refuses_a_token_count_past_what_json_holds_exactly() {
        let mut input = an_event_wire(EventKind::CostRecorded);
        input["body"]["usage"]["input_tokens"] = json!(9_007_199_254_740_992_u64);
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/usage/input_tokens");
    }

    #[test]
    fn refuses_a_cost_for_an_unknown_purpose() {
        let mut input = an_event_wire(EventKind::CostRecorded);
        input["body"]["purpose"] = json!("lunch");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/purpose");
    }

    #[test]
    fn refuses_a_transition_to_a_status_that_does_not_exist() {
        let mut input = an_event_wire(EventKind::TaskTransitioned);
        input["body"]["to"] = json!("done");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/to");
    }

    #[test]
    fn refuses_an_escalation_for_an_unknown_reason() {
        let mut input = an_event_wire(EventKind::EscalationRaised);
        input["body"]["reason"] = json!("boredom");
        let errors = refusal(&input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body/reason");
    }

    #[test]
    fn refuses_a_contract_event_without_a_task() {
        for kind in [
            EventKind::TaskTransitioned,
            EventKind::TransitionRefused,
            EventKind::EscalationRaised,
            EventKind::ContractEvaluated,
            EventKind::CriterionRecorded,
            EventKind::NoteWritten,
            EventKind::ProductDocWritten,
        ] {
            let body = event_from_value(&an_event_wire(kind)).expect("valid").body;
            let error = new_event(body, at(), some_ids()).expect_err("expected a refusal");
            assert_eq!(error, EventError::NoContractNamed { kind }, "{kind}");
        }
    }

    #[test]
    fn names_the_contract_a_judgment_is_about() {
        assert!(super::is_about_one_contract(EventKind::ContractJudged));
        let body = event_from_value(&an_event_wire(EventKind::ContractJudged))
            .expect("valid")
            .body;
        let error = new_event(body, at(), some_ids()).expect_err("expected a refusal");
        assert_eq!(
            error,
            EventError::NoContractNamed {
                kind: EventKind::ContractJudged
            }
        );
    }

    #[test]
    fn stamps_a_question_asked_outside_any_task() {
        // A conversation with no task can ask the human something (5.16).
        let body = event_from_value(&an_event_wire(EventKind::QuestionAsked))
            .expect("valid")
            .body;
        assert!(new_event(body, at(), some_ids()).is_ok());
    }

    #[test]
    fn refuses_to_stamp_an_event_whose_actor_is_blank() {
        let mut input = an_event_wire(EventKind::TeamUpdated);
        input["body"]["updated_by"] = json!("human");
        let body = event_from_value(&input).expect("valid").body;
        let EventBody::TeamUpdated(mut updated) = body else {
            panic!("a team.updated event carries a team.updated body");
        };
        updated.updated_by = "   ".to_string();
        let error = new_event(EventBody::TeamUpdated(updated), at(), some_ids())
            .expect_err("expected a refusal");
        assert_eq!(
            error,
            EventError::BlankId {
                field: "updated_by".to_string()
            }
        );
    }
}
