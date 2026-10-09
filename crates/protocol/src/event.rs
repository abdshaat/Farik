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
    BudgetExhaustedBodyConsequence, BudgetExhaustedBodyScope, ChatMessagePostedBody, CheckTheme,
    CheckWidth, ConnectorConnectedBody, ConnectorDisconnectedBody,
    ConnectorTag as ConnectorTagWire, ContractEvaluatedBody, ContractEvaluatedBodyGate,
    ContractJudgedBody, ContractLockedBody, ContractSummary, ContractSummaryKind,
    ContractSummaryParent, ContractSummaryRisk, ContractSummaryStatus, ContractUnlockedBody,
    ContractWrittenBody, CostRecordedBody, CostRecordedBodyModelId, CostRecordedBodyPurpose,
    CriteriaUpdatedBody, CriterionRecordedBody, CriterionRecordedBodyRunBy, DecisionWrittenBody,
    DesignPlanProposedBody, DesignReviewCheck, DesignReviewRecordedBody, DriftDetectedBody,
    DriftDetectedBodyDrift, EscalationAgedBody, EscalationRaisedBody, EscalationRaisedBodyReason,
    EscalationResolvedBody, EventKind, HumanAcceptedBody, HumanAcceptedBodySubject, JudgmentAnswer,
    MarketingBudgetReachedBody, MarketingBudgetReachedBodyScope, MarketingCampaignCreatedBody,
    MarketingCampaignCreatedBodyBudgetKind, MarketingCampaignPausedBody,
    MarketingCampaignPausedBodyWhy, MarketingPlanApprovedBody, MarketingPlanBudget,
    MarketingPlanCampaign, MarketingPlanCampaignChannel, MarketingPlanEndedBody,
    MarketingPlanEndedBodyWhy, MarketingPlanPost, MarketingPlanPostChannel,
    MarketingPlanProposedBody, MarketingPlanReturnedBody, MemoryWrittenBody, MessagePostedBody,
    NoteWrittenBody, NoteWrittenBodyKind, PageCheckedBody, PreviewPreparedBody, PreviewStartedBody,
    ProductDocWrittenBody, ProjectScannedBody, ProposedRequest, PullRequestOpenedBody,
    QuestionAnsweredBody, QuestionAskedBody, QuestionChoice, ReasonBody, RequestTriagedBody,
    RequestTriagedBodySize, RetroAppendedBody, ReviewRecordedBody, SessionEndedBody,
    SessionEndedBodyReason, SessionStartedBody, SessionStartedBodyEffort, SessionStartedBodyModel,
    SessionStartedBodyPurpose, SkillPinnedBody, SkillRemovedBody, SprintEndedBody,
    SprintEndedBodyEndedBy, SprintPlannedBody, SprintStartedBody, TaskCreatedBody,
    TaskIntegratedBody, TaskIntegratedBodyIntegratedBy, TaskTransitionedBody,
    TaskTransitionedBodyEffectsItem, TeamPausedBody, TeamPausedBodyBy, TeamPausedBodyReason,
    TeamUpdatedBody, TokenUsage, ToolApprovalDecidedBody, ToolApprovalRequestedBody,
    ToolCalledBody, ToolDeniedBody, ToolReturnedBody, TransitionRefusedBody,
    TransitionRefusedBodyRefusal, Violation,
};
/// The generated names of the vocabularies the governor's events repeat, renamed at the edge so
/// that they cannot be mistaken for `farik-core`'s own types of the same name.
pub use crate::generated::event::{
    BlockerWire, GateId as GateWire, RejectionWire, TaskStatus as TaskStatusWire,
    TransitionActor as TransitionActorWire,
};
/// The bodies of the four `data_pipeline.` kinds, with the vocabularies they repeat.
pub use crate::generated::event::{
    DataPipelineApprovedBody, DataPipelineCost, DataPipelineDecidedBy, DataPipelineDeclinedBody,
    DataPipelineEscalatedBody, DataPipelineNumber, DataPipelineRequestedBody,
};
/// The bodies of the eight mailbox and seller mail kinds (`mailbox.`, `seller_message.`,
/// `seller_reply.`), with the vocabularies they repeat.
pub use crate::generated::event::{
    MailboxConnectedBody, MailboxDisconnectedBody, MailboxPurpose, SellerMessageDiscardedBody,
    SellerMessageDraftedBody, SellerMessageDraftedBodyPurpose as SellerMessagePurpose,
    SellerMessageFailedBody, SellerMessageSentBody, SellerReplyAttachment,
    SellerReplyAttachmentMediaType, SellerReplyDismissedBody, SellerReplyReceivedBody,
};
/// The channel's vocabularies, named for what they are rather than for the body they sit in.
pub use crate::generated::event::{MessagePostedBodyKind as MessageKind, Thread};
/// The bodies of the eight `purchase_order.` kinds, with the vocabularies they repeat: an
/// approval, a rejection and a closing share one.
pub use crate::generated::event::{
    PurchaseOrderDecisionBody, PurchaseOrderDraftedBody, PurchaseOrderDraftedBodyPeriod,
    PurchaseOrderExpiredBody, PurchaseOrderLine, PurchaseOrderPlacedBody,
    PurchaseOrderReceivedBody, PurchaseOrderStatus, PurchaseOrderUpdatedBody,
};
/// The bodies of the three `renewal.` kinds.
pub use crate::generated::event::{RenewalCheckedBody, RenewalDismissedBody, RenewalFlaggedBody};
/// The bodies of the four `site.` kinds: an approval, a decline and a removal share one.
pub use crate::generated::event::{SiteDecisionBody, SiteRequestedBody};
/// The bodies of the six `social_post.` kinds, with the vocabularies they repeat.
pub use crate::generated::event::{
    SocialPostChannel, SocialPostDetails, SocialPostFailedBody, SocialPostMedia,
    SocialPostMediaKind, SocialPostMissedBody, SocialPostMissedBodyWhy, SocialPostRequestedBody,
    SocialPostScheduledBody, SocialPostScheduledBodyApprovedBy, SocialPostSentBody,
    SocialPostStoppedBody, SocialPostStoppedBodyBy,
};

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
/// `body` as a choice of forty-two shapes, so it can only say that a body matched none of them; these
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
#[allow(clippy::too_many_lines, reason = "one arm per kind of event")]
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
        EventKind::TeamPaused | EventKind::TeamResumed => "teamPausedBody",
        EventKind::DesignPlanProposed => "designPlanProposedBody",
        EventKind::DesignPlanApproved
        | EventKind::DesignPlanReturned
        | EventKind::PreviewStopped => "reasonBody",
        EventKind::DesignReviewRecorded => "designReviewRecordedBody",
        EventKind::PreviewPrepared => "previewPreparedBody",
        EventKind::PreviewStarted => "previewStartedBody",
        EventKind::PageChecked => "pageCheckedBody",
        EventKind::ChatMessagePosted => "chatMessagePostedBody",
        EventKind::ConnectorConnected => "connectorConnectedBody",
        EventKind::ConnectorDisconnected => "connectorDisconnectedBody",
        EventKind::ToolApprovalRequested => "toolApprovalRequestedBody",
        EventKind::ToolApprovalGranted | EventKind::ToolApprovalRefused => {
            "toolApprovalDecidedBody"
        }
        EventKind::SkillAdded | EventKind::SkillChanged | EventKind::SkillConfirmed => {
            "skillPinnedBody"
        }
        EventKind::SkillRemoved => "skillRemovedBody",
        EventKind::MarketingPlanProposed => "marketingPlanProposedBody",
        EventKind::MarketingPlanApproved => "marketingPlanApprovedBody",
        EventKind::MarketingPlanReturned => "marketingPlanReturnedBody",
        EventKind::MarketingPlanEnded => "marketingPlanEndedBody",
        EventKind::SocialPostScheduled => "socialPostScheduledBody",
        EventKind::SocialPostRequested => "socialPostRequestedBody",
        EventKind::SocialPostSent => "socialPostSentBody",
        EventKind::SocialPostStopped => "socialPostStoppedBody",
        EventKind::SocialPostMissed => "socialPostMissedBody",
        EventKind::SocialPostFailed => "socialPostFailedBody",
        EventKind::MarketingCampaignCreated => "marketingCampaignCreatedBody",
        EventKind::MarketingBudgetReached => "marketingBudgetReachedBody",
        EventKind::MarketingCampaignPaused => "marketingCampaignPausedBody",
        EventKind::SiteRequested => "siteRequestedBody",
        EventKind::SiteApproved | EventKind::SiteDeclined | EventKind::SiteRemoved => {
            "siteDecisionBody"
        }
        EventKind::PurchaseOrderDrafted => "purchaseOrderDraftedBody",
        EventKind::PurchaseOrderApproved
        | EventKind::PurchaseOrderRejected
        | EventKind::PurchaseOrderClosed => "purchaseOrderDecisionBody",
        EventKind::PurchaseOrderPlaced => "purchaseOrderPlacedBody",
        EventKind::PurchaseOrderUpdated => "purchaseOrderUpdatedBody",
        EventKind::PurchaseOrderReceived => "purchaseOrderReceivedBody",
        EventKind::PurchaseOrderExpired => "purchaseOrderExpiredBody",
        EventKind::RenewalFlagged => "renewalFlaggedBody",
        EventKind::RenewalDismissed => "renewalDismissedBody",
        EventKind::RenewalChecked => "renewalCheckedBody",
        EventKind::DataPipelineRequested => "dataPipelineRequestedBody",
        EventKind::DataPipelineEscalated => "dataPipelineEscalatedBody",
        EventKind::DataPipelineApproved => "dataPipelineApprovedBody",
        EventKind::DataPipelineDeclined => "dataPipelineDeclinedBody",
        EventKind::MailboxConnected => "mailboxConnectedBody",
        EventKind::MailboxDisconnected => "mailboxDisconnectedBody",
        EventKind::SellerMessageDrafted => "sellerMessageDraftedBody",
        EventKind::SellerMessageSent => "sellerMessageSentBody",
        EventKind::SellerMessageFailed => "sellerMessageFailedBody",
        EventKind::SellerMessageDiscarded => "sellerMessageDiscardedBody",
        EventKind::SellerReplyReceived => "sellerReplyReceivedBody",
        EventKind::SellerReplyDismissed => "sellerReplyDismissedBody",
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
            | EventKind::DesignPlanProposed
            | EventKind::DesignPlanApproved
            | EventKind::DesignPlanReturned
            | EventKind::DesignReviewRecorded
            | EventKind::PreviewPrepared
            | EventKind::PreviewStarted
            | EventKind::PreviewStopped
            | EventKind::PageChecked
            | EventKind::ToolApprovalRequested
            | EventKind::ToolApprovalGranted
            | EventKind::ToolApprovalRefused
            | EventKind::MarketingPlanProposed
            | EventKind::MarketingPlanApproved
            | EventKind::MarketingPlanReturned
            | EventKind::SocialPostScheduled
            | EventKind::SocialPostRequested
            | EventKind::SiteRequested
            | EventKind::SiteDeclined
            | EventKind::PurchaseOrderDrafted
            | EventKind::PurchaseOrderApproved
            | EventKind::PurchaseOrderRejected
            | EventKind::PurchaseOrderPlaced
            | EventKind::PurchaseOrderUpdated
            | EventKind::PurchaseOrderReceived
            | EventKind::PurchaseOrderClosed
            | EventKind::PurchaseOrderExpired
            | EventKind::DataPipelineRequested
            | EventKind::SellerMessageDrafted
    )
}

/// The field naming who acted, for the kinds that name one, and nothing for `drift.detected`,
/// `project.scanned`, `cost.recorded`, `budget.exhausted`, `escalation.raised`,
/// `contract.evaluated`, and `pull_request.opened`, which record what Farik itself found, counted,
/// judged, or did; the move or the refusal they come with names who asked. `task.integrated` and
/// `sprint.ended` name who acted in a closed vocabulary, `governor` or `human`, which cannot be
/// blank. Nor for the three `tool.` kinds, the two `session.` kinds, and `agent.slept`, whose
/// envelope names the agent and the session; Farik observed the sleep, and nobody asked for it.
/// Nor for `escalation.aged`: the human left it waiting, and nobody acted. `team.paused` and
/// `team.resumed` name the human in a closed vocabulary, which cannot be blank. The three
/// `design_plan.` kinds name no one in the body: their envelope names the agent and the session.
/// Nor do the five kinds of step 12: `design_review.recorded` and `page.checked`, whose envelope
/// names the Designer and the session, and `preview.prepared`, `preview.started` and
/// `preview.stopped`, which record what Farik itself did with the task's preview. Nor the two
/// `connector.` kinds: only the human connects a server, and `agent` names whose it is, not who
/// acted. `marketing_plan.proposed` names the Marketing Specialist in `proposed_by`; the other
/// three `marketing_plan.` kinds name no one in the body: the owner decided or ended a plan, or
/// Farik ended one by its dates, and their envelope names no agent and no session. The six
/// `social_post.` kinds name no one in the body either: the agent that wrote a post is on the
/// envelope of `scheduled` and `requested`, and the other four are the owner's or Farik's. Nor
/// do the four `site.` kinds: the agent that asked is on the envelope of `site.requested`, and the
/// other three are the owner's. Nor do the four `data_pipeline.` kinds: the agent that asked is on
/// the envelope of `data_pipeline.requested`, and `by` of an approval or a decline says whether
/// the Product Manager or the owner decided it.
#[allow(clippy::too_many_lines, reason = "one arm per kind of event")]
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
        EventBody::ChatMessagePosted(body) => Some(("author", &mut body.author)),
        EventBody::MarketingPlanProposed(body) => Some(("proposed_by", &mut body.proposed_by)),
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
        | EventBody::EscalationAged(_)
        | EventBody::TeamPaused(_)
        | EventBody::TeamResumed(_)
        | EventBody::DesignPlanProposed(_)
        | EventBody::DesignPlanApproved(_)
        | EventBody::DesignPlanReturned(_)
        | EventBody::DesignReviewRecorded(_)
        | EventBody::PreviewPrepared(_)
        | EventBody::PreviewStarted(_)
        | EventBody::PreviewStopped(_)
        | EventBody::PageChecked(_)
        | EventBody::ConnectorConnected(_)
        | EventBody::ConnectorDisconnected(_)
        | EventBody::ToolApprovalRequested(_)
        | EventBody::ToolApprovalGranted(_)
        | EventBody::ToolApprovalRefused(_)
        | EventBody::SkillAdded(_)
        | EventBody::SkillChanged(_)
        | EventBody::SkillRemoved(_)
        | EventBody::SkillConfirmed(_)
        | EventBody::MarketingPlanApproved(_)
        | EventBody::MarketingPlanReturned(_)
        | EventBody::MarketingPlanEnded(_)
        | EventBody::SocialPostScheduled(_)
        | EventBody::SocialPostRequested(_)
        | EventBody::SocialPostSent(_)
        | EventBody::SocialPostStopped(_)
        | EventBody::SocialPostMissed(_)
        | EventBody::SocialPostFailed(_)
        | EventBody::MarketingCampaignCreated(_)
        | EventBody::MarketingBudgetReached(_)
        | EventBody::MarketingCampaignPaused(_)
        | EventBody::SiteRequested(_)
        | EventBody::SiteApproved(_)
        | EventBody::SiteDeclined(_)
        | EventBody::SiteRemoved(_)
        | EventBody::PurchaseOrderDrafted(_)
        | EventBody::PurchaseOrderApproved(_)
        | EventBody::PurchaseOrderRejected(_)
        | EventBody::PurchaseOrderPlaced(_)
        | EventBody::PurchaseOrderUpdated(_)
        | EventBody::PurchaseOrderReceived(_)
        | EventBody::PurchaseOrderClosed(_)
        | EventBody::PurchaseOrderExpired(_)
        | EventBody::RenewalFlagged(_)
        | EventBody::RenewalDismissed(_)
        | EventBody::RenewalChecked(_)
        | EventBody::DataPipelineRequested(_)
        | EventBody::DataPipelineEscalated(_)
        | EventBody::DataPipelineApproved(_)
        | EventBody::DataPipelineDeclined(_)
        | EventBody::MailboxConnected(_)
        | EventBody::MailboxDisconnected(_)
        | EventBody::SellerMessageDrafted(_)
        | EventBody::SellerMessageSent(_)
        | EventBody::SellerMessageFailed(_)
        | EventBody::SellerMessageDiscarded(_)
        | EventBody::SellerReplyReceived(_)
        | EventBody::SellerReplyDismissed(_) => None,
    }
}

/// Every kind the log holds in this phase, in the order `docs/schemas/event.schema.json` lists
/// them. The step that adds a kind adds it here.
pub const EVERY_KIND: [EventKind; 101] = [
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
    EventKind::TeamPaused,
    EventKind::TeamResumed,
    EventKind::DesignPlanProposed,
    EventKind::DesignPlanApproved,
    EventKind::DesignPlanReturned,
    EventKind::DesignReviewRecorded,
    EventKind::PreviewPrepared,
    EventKind::PreviewStarted,
    EventKind::PreviewStopped,
    EventKind::PageChecked,
    EventKind::ChatMessagePosted,
    EventKind::ConnectorConnected,
    EventKind::ConnectorDisconnected,
    EventKind::ToolApprovalRequested,
    EventKind::ToolApprovalGranted,
    EventKind::ToolApprovalRefused,
    EventKind::SkillAdded,
    EventKind::SkillChanged,
    EventKind::SkillRemoved,
    EventKind::SkillConfirmed,
    EventKind::MarketingPlanProposed,
    EventKind::MarketingPlanApproved,
    EventKind::MarketingPlanReturned,
    EventKind::MarketingPlanEnded,
    EventKind::SocialPostScheduled,
    EventKind::SocialPostRequested,
    EventKind::SocialPostSent,
    EventKind::SocialPostStopped,
    EventKind::SocialPostMissed,
    EventKind::SocialPostFailed,
    EventKind::MarketingCampaignCreated,
    EventKind::MarketingBudgetReached,
    EventKind::MarketingCampaignPaused,
    EventKind::SiteRequested,
    EventKind::SiteApproved,
    EventKind::SiteDeclined,
    EventKind::SiteRemoved,
    EventKind::PurchaseOrderDrafted,
    EventKind::PurchaseOrderApproved,
    EventKind::PurchaseOrderRejected,
    EventKind::PurchaseOrderPlaced,
    EventKind::PurchaseOrderUpdated,
    EventKind::PurchaseOrderReceived,
    EventKind::PurchaseOrderClosed,
    EventKind::PurchaseOrderExpired,
    EventKind::RenewalFlagged,
    EventKind::RenewalDismissed,
    EventKind::RenewalChecked,
    EventKind::DataPipelineRequested,
    EventKind::DataPipelineEscalated,
    EventKind::DataPipelineApproved,
    EventKind::DataPipelineDeclined,
    EventKind::MailboxConnected,
    EventKind::MailboxDisconnected,
    EventKind::SellerMessageDrafted,
    EventKind::SellerMessageSent,
    EventKind::SellerMessageFailed,
    EventKind::SellerMessageDiscarded,
    EventKind::SellerReplyReceived,
    EventKind::SellerReplyDismissed,
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
    /// The team's judge checked a contract's plan against the team's questions.
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
    /// The human paused every agent of the team.
    #[serde(rename = "team.paused")]
    TeamPaused(TeamPausedBody),
    /// The human resumed the team.
    #[serde(rename = "team.resumed")]
    TeamResumed(TeamPausedBody),
    /// The UI/UX Designer proposed its plan for the task.
    #[serde(rename = "design_plan.proposed")]
    DesignPlanProposed(DesignPlanProposedBody),
    /// The Product Manager approved the task's design plan.
    #[serde(rename = "design_plan.approved")]
    DesignPlanApproved(ReasonBody),
    /// The Product Manager returned the task's design plan with a reason.
    #[serde(rename = "design_plan.returned")]
    DesignPlanReturned(ReasonBody),
    /// The UI/UX Designer recorded its design review of a Developer's UI change.
    #[serde(rename = "design_review.recorded")]
    DesignReviewRecorded(DesignReviewRecordedBody),
    /// Farik ran the preview's `prepare` for the task.
    #[serde(rename = "preview.prepared")]
    PreviewPrepared(PreviewPreparedBody),
    /// Farik started the task's preview and it answered.
    #[serde(rename = "preview.started")]
    PreviewStarted(PreviewStartedBody),
    /// Farik stopped the task's preview.
    #[serde(rename = "preview.stopped")]
    PreviewStopped(ReasonBody),
    /// `farik_check_page` checked one page at one width and theme.
    #[serde(rename = "page.checked")]
    PageChecked(PageCheckedBody),
    /// The user or an agent said something in their one-to-one chat.
    #[serde(rename = "chat_message.posted")]
    ChatMessagePosted(ChatMessagePostedBody),
    /// The human gave an agent a custom MCP server, or connected it again.
    #[serde(rename = "connector.connected")]
    ConnectorConnected(ConnectorConnectedBody),
    /// The human took a custom MCP server away from an agent.
    #[serde(rename = "connector.disconnected")]
    ConnectorDisconnected(ConnectorDisconnectedBody),
    /// A connector's `external_effect` call waits for the human; its seq is the approval's id.
    #[serde(rename = "tool_approval.requested")]
    ToolApprovalRequested(ToolApprovalRequestedBody),
    /// The human allowed one call an agent asked about.
    #[serde(rename = "tool_approval.granted")]
    ToolApprovalGranted(ToolApprovalDecidedBody),
    /// The human refused one call an agent asked about.
    #[serde(rename = "tool_approval.refused")]
    ToolApprovalRefused(ToolApprovalDecidedBody),
    /// The human added a skill for the team or one agent.
    #[serde(rename = "skill.added")]
    SkillAdded(SkillPinnedBody),
    /// The human saved a changed version of a skill.
    #[serde(rename = "skill.changed")]
    SkillChanged(SkillPinnedBody),
    /// The human removed a skill.
    #[serde(rename = "skill.removed")]
    SkillRemoved(SkillRemovedBody),
    /// The human confirmed a skill's folder on this computer.
    #[serde(rename = "skill.confirmed")]
    SkillConfirmed(SkillPinnedBody),
    /// The Marketing Specialist proposed a marketing plan.
    #[serde(rename = "marketing_plan.proposed")]
    MarketingPlanProposed(MarketingPlanProposedBody),
    /// The owner approved a marketing plan.
    #[serde(rename = "marketing_plan.approved")]
    MarketingPlanApproved(MarketingPlanApprovedBody),
    /// The owner sent a marketing plan back.
    #[serde(rename = "marketing_plan.returned")]
    MarketingPlanReturned(MarketingPlanReturnedBody),
    /// An approved marketing plan ended.
    #[serde(rename = "marketing_plan.ended")]
    MarketingPlanEnded(MarketingPlanEndedBody),
    /// A post is going out: the Marketing Specialist scheduled it in the active plan, or the
    /// owner allowed one that was requested.
    #[serde(rename = "social_post.scheduled")]
    SocialPostScheduled(SocialPostScheduledBody),
    /// The Marketing Specialist wrote a post outside the plan, which waits for the owner.
    #[serde(rename = "social_post.requested")]
    SocialPostRequested(SocialPostRequestedBody),
    /// Farik handed a post to Buffer.
    #[serde(rename = "social_post.sent")]
    SocialPostSent(SocialPostSentBody),
    /// A post will not go out: the owner stopped or did not allow it, or its plan ended.
    #[serde(rename = "social_post.stopped")]
    SocialPostStopped(SocialPostStoppedBody),
    /// A post was not handed over in time.
    #[serde(rename = "social_post.missed")]
    SocialPostMissed(SocialPostMissedBody),
    /// Buffer did not take a post.
    #[serde(rename = "social_post.failed")]
    SocialPostFailed(SocialPostFailedBody),
    /// Farik made a Google Ads campaign, paused, for a plan campaign of the active plan.
    #[serde(rename = "marketing_campaign.created")]
    MarketingCampaignCreated(MarketingCampaignCreatedBody),
    /// A budget of the active marketing plan was reached, and Farik paused the campaigns itself.
    #[serde(rename = "marketing_budget.reached")]
    MarketingBudgetReached(MarketingBudgetReachedBody),
    /// Farik paused a campaign it made, on its own.
    #[serde(rename = "marketing_campaign.paused")]
    MarketingCampaignPaused(MarketingCampaignPausedBody),
    /// The Procurement Specialist asked to read a site that is not approved.
    #[serde(rename = "site.requested")]
    SiteRequested(SiteRequestedBody),
    /// The owner allowed a site, for a request or unasked.
    #[serde(rename = "site.approved")]
    SiteApproved(SiteDecisionBody),
    /// The owner did not allow a site an agent asked for.
    #[serde(rename = "site.declined")]
    SiteDeclined(SiteDecisionBody),
    /// The owner took a site away: one they allowed, or one of Farik's.
    #[serde(rename = "site.removed")]
    SiteRemoved(SiteDecisionBody),
    /// The Procurement Specialist set up an order for the owner to approve or reject.
    #[serde(rename = "purchase_order.drafted")]
    PurchaseOrderDrafted(PurchaseOrderDraftedBody),
    /// The owner approved an order: they will place and pay for it themselves.
    #[serde(rename = "purchase_order.approved")]
    PurchaseOrderApproved(PurchaseOrderDecisionBody),
    /// The owner did not approve an order.
    #[serde(rename = "purchase_order.rejected")]
    PurchaseOrderRejected(PurchaseOrderDecisionBody),
    /// The owner placed an approved order and marked it placed.
    #[serde(rename = "purchase_order.placed")]
    PurchaseOrderPlaced(PurchaseOrderPlacedBody),
    /// A placed order's follow-up status, from its agent or the owner's correction.
    #[serde(rename = "purchase_order.updated")]
    PurchaseOrderUpdated(PurchaseOrderUpdatedBody),
    /// The owner marked a placed order received.
    #[serde(rename = "purchase_order.received")]
    PurchaseOrderReceived(PurchaseOrderReceivedBody),
    /// The owner closed a placed order that will not come.
    #[serde(rename = "purchase_order.closed")]
    PurchaseOrderClosed(PurchaseOrderDecisionBody),
    /// Farik closed an order nobody decided or placed within 30 days.
    #[serde(rename = "purchase_order.expired")]
    PurchaseOrderExpired(PurchaseOrderExpiredBody),
    /// Farik found a renewal whose decision date is two weeks off or nearer.
    #[serde(rename = "renewal.flagged")]
    RenewalFlagged(RenewalFlaggedBody),
    /// The owner dismissed a renewal.
    #[serde(rename = "renewal.dismissed")]
    RenewalDismissed(RenewalDismissedBody),
    /// Farik read the vendors register for the day.
    #[serde(rename = "renewal.checked")]
    RenewalChecked(RenewalCheckedBody),
    /// The Procurement Specialist asked for a source of data it lacks.
    #[serde(rename = "data_pipeline.requested")]
    DataPipelineRequested(DataPipelineRequestedBody),
    /// A data pipeline request goes to the owner: the Product Manager passed it on, or did not
    /// decide it.
    #[serde(rename = "data_pipeline.escalated")]
    DataPipelineEscalated(DataPipelineEscalatedBody),
    /// A data pipeline request was approved, and the team asked to set the source up.
    #[serde(rename = "data_pipeline.approved")]
    DataPipelineApproved(DataPipelineApprovedBody),
    /// A data pipeline request was declined.
    #[serde(rename = "data_pipeline.declined")]
    DataPipelineDeclined(DataPipelineDeclinedBody),
    /// The owner connected a mailbox, and Farik logged in to both of its servers.
    #[serde(rename = "mailbox.connected")]
    MailboxConnected(MailboxConnectedBody),
    /// The owner disconnected a mailbox.
    #[serde(rename = "mailbox.disconnected")]
    MailboxDisconnected(MailboxDisconnectedBody),
    /// The Procurement Specialist drafted a message to a seller.
    #[serde(rename = "seller_message.drafted")]
    SellerMessageDrafted(SellerMessageDraftedBody),
    /// Farik sent a message to a seller on the owner's press.
    #[serde(rename = "seller_message.sent")]
    SellerMessageSent(SellerMessageSentBody),
    /// A mail server refused a message the owner pressed Send on.
    #[serde(rename = "seller_message.failed")]
    SellerMessageFailed(SellerMessageFailedBody),
    /// The owner discarded a message to a seller.
    #[serde(rename = "seller_message.discarded")]
    SellerMessageDiscarded(SellerMessageDiscardedBody),
    /// Farik read a seller's reply in the procurement mailbox.
    #[serde(rename = "seller_reply.received")]
    SellerReplyReceived(SellerReplyReceivedBody),
    /// The owner dismissed a seller's reply on Today.
    #[serde(rename = "seller_reply.dismissed")]
    SellerReplyDismissed(SellerReplyDismissedBody),
}

impl EventBody {
    /// The kind of event this body belongs to.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm per kind of event")]
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
            Self::TeamPaused(_) => EventKind::TeamPaused,
            Self::TeamResumed(_) => EventKind::TeamResumed,
            Self::DesignPlanProposed(_) => EventKind::DesignPlanProposed,
            Self::DesignPlanApproved(_) => EventKind::DesignPlanApproved,
            Self::DesignPlanReturned(_) => EventKind::DesignPlanReturned,
            Self::DesignReviewRecorded(_) => EventKind::DesignReviewRecorded,
            Self::PreviewPrepared(_) => EventKind::PreviewPrepared,
            Self::PreviewStarted(_) => EventKind::PreviewStarted,
            Self::PreviewStopped(_) => EventKind::PreviewStopped,
            Self::PageChecked(_) => EventKind::PageChecked,
            Self::ChatMessagePosted(_) => EventKind::ChatMessagePosted,
            Self::ConnectorConnected(_) => EventKind::ConnectorConnected,
            Self::ConnectorDisconnected(_) => EventKind::ConnectorDisconnected,
            Self::ToolApprovalRequested(_) => EventKind::ToolApprovalRequested,
            Self::ToolApprovalGranted(_) => EventKind::ToolApprovalGranted,
            Self::ToolApprovalRefused(_) => EventKind::ToolApprovalRefused,
            Self::SkillAdded(_) => EventKind::SkillAdded,
            Self::SkillChanged(_) => EventKind::SkillChanged,
            Self::SkillRemoved(_) => EventKind::SkillRemoved,
            Self::SkillConfirmed(_) => EventKind::SkillConfirmed,
            Self::MarketingPlanProposed(_) => EventKind::MarketingPlanProposed,
            Self::MarketingPlanApproved(_) => EventKind::MarketingPlanApproved,
            Self::MarketingPlanReturned(_) => EventKind::MarketingPlanReturned,
            Self::MarketingPlanEnded(_) => EventKind::MarketingPlanEnded,
            Self::SocialPostScheduled(_) => EventKind::SocialPostScheduled,
            Self::SocialPostRequested(_) => EventKind::SocialPostRequested,
            Self::SocialPostSent(_) => EventKind::SocialPostSent,
            Self::SocialPostStopped(_) => EventKind::SocialPostStopped,
            Self::SocialPostMissed(_) => EventKind::SocialPostMissed,
            Self::SocialPostFailed(_) => EventKind::SocialPostFailed,
            Self::MarketingCampaignCreated(_) => EventKind::MarketingCampaignCreated,
            Self::MarketingBudgetReached(_) => EventKind::MarketingBudgetReached,
            Self::MarketingCampaignPaused(_) => EventKind::MarketingCampaignPaused,
            Self::SiteRequested(_) => EventKind::SiteRequested,
            Self::SiteApproved(_) => EventKind::SiteApproved,
            Self::SiteDeclined(_) => EventKind::SiteDeclined,
            Self::SiteRemoved(_) => EventKind::SiteRemoved,
            Self::PurchaseOrderDrafted(_) => EventKind::PurchaseOrderDrafted,
            Self::PurchaseOrderApproved(_) => EventKind::PurchaseOrderApproved,
            Self::PurchaseOrderRejected(_) => EventKind::PurchaseOrderRejected,
            Self::PurchaseOrderPlaced(_) => EventKind::PurchaseOrderPlaced,
            Self::PurchaseOrderUpdated(_) => EventKind::PurchaseOrderUpdated,
            Self::PurchaseOrderReceived(_) => EventKind::PurchaseOrderReceived,
            Self::PurchaseOrderClosed(_) => EventKind::PurchaseOrderClosed,
            Self::PurchaseOrderExpired(_) => EventKind::PurchaseOrderExpired,
            Self::RenewalFlagged(_) => EventKind::RenewalFlagged,
            Self::RenewalDismissed(_) => EventKind::RenewalDismissed,
            Self::RenewalChecked(_) => EventKind::RenewalChecked,
            Self::DataPipelineRequested(_) => EventKind::DataPipelineRequested,
            Self::DataPipelineEscalated(_) => EventKind::DataPipelineEscalated,
            Self::DataPipelineApproved(_) => EventKind::DataPipelineApproved,
            Self::DataPipelineDeclined(_) => EventKind::DataPipelineDeclined,
            Self::MailboxConnected(_) => EventKind::MailboxConnected,
            Self::MailboxDisconnected(_) => EventKind::MailboxDisconnected,
            Self::SellerMessageDrafted(_) => EventKind::SellerMessageDrafted,
            Self::SellerMessageSent(_) => EventKind::SellerMessageSent,
            Self::SellerMessageFailed(_) => EventKind::SellerMessageFailed,
            Self::SellerMessageDiscarded(_) => EventKind::SellerMessageDiscarded,
            Self::SellerReplyReceived(_) => EventKind::SellerReplyReceived,
            Self::SellerReplyDismissed(_) => EventKind::SellerReplyDismissed,
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
    if let Some(fault) = site_fault(&body) {
        return Err(vec![ValidationError {
            path: "/body".to_string(),
            message: format!("a {} event does not carry this body: {fault}", wire.kind),
        }]);
    }
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

/// What is wrong with a `site.` event's body for its kind, which the schema cannot say because the
/// three kinds that decide a site share one shape: a decline names the request it declines, a
/// removal names its host and nothing else, and the owner's note goes with a request.
fn site_fault(body: &EventBody) -> Option<&'static str> {
    match body {
        EventBody::SiteDeclined(body) if body.request.is_none() => {
            Some("a decline names the request it declines")
        }
        EventBody::SiteRemoved(body) if body.request.is_some() || body.note.is_some() => {
            Some("a removal names its host alone")
        }
        EventBody::SiteApproved(body) if body.note.is_some() && body.request.is_none() => {
            Some("a note goes with the request it answers")
        }
        // The schema cannot say which purpose goes with an order: an order's message names its
        // order, a quote request names none, and a question names a placed one for a follow-up.
        EventBody::SellerMessageDrafted(body)
            if (body.purpose == SellerMessagePurpose::PurchaseOrder)
                != body.purchase_order.is_some()
                && body.purpose != SellerMessagePurpose::Question =>
        {
            Some("an order's message names its order, and no other purpose but a question does")
        }
        _ => None,
    }
}

/// The schema's own failures. A failure inside `body` is reported by the schema once, at `/body`,
/// because `body` there is a choice of forty-two shapes and the schema can only say that none matched.
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
        event_to_value, is_about_one_contract, new_event,
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
                // team.paused and team.resumed share one body, so each carries the other's, and so
                // do design_plan.approved, design_plan.returned and preview.stopped, and
                // tool_approval.granted and tool_approval.refused; no others do, and a
                // fixture that made one equal must not hide it.
                let shared: [&[EventKind]; 6] = [
                    &[
                        EventKind::SkillAdded,
                        EventKind::SkillChanged,
                        EventKind::SkillConfirmed,
                    ],
                    &[EventKind::TeamPaused, EventKind::TeamResumed],
                    &[
                        EventKind::ToolApprovalGranted,
                        EventKind::ToolApprovalRefused,
                    ],
                    &[
                        EventKind::DesignPlanApproved,
                        EventKind::DesignPlanReturned,
                        EventKind::PreviewStopped,
                    ],
                    // `{ "host": ... }` alone is an added site and a removed one, and an
                    // approval and a decline of a request have the same fields.
                    &[
                        EventKind::SiteApproved,
                        EventKind::SiteDeclined,
                        EventKind::SiteRemoved,
                    ],
                    // An order's approval, rejection and closing are `{ order, note }`.
                    &[
                        EventKind::PurchaseOrderApproved,
                        EventKind::PurchaseOrderRejected,
                        EventKind::PurchaseOrderClosed,
                    ],
                ];
                if other == kind
                    || shared
                        .iter()
                        .any(|pair| pair.contains(&kind) && pair.contains(&other))
                {
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
    fn reads_the_new_events() {
        let violation = json!({
            "rule": "color-contrast",
            "impact": "serious",
            "target": "#save",
            "help": "Elements must meet minimum color contrast ratio thresholds"
        });
        for (kind, body) in [
            (
                "design_review.recorded",
                json!({
                    "pass": false,
                    "reasons": "The save button fails contrast in the dark theme.",
                    "checks": [
                        { "width": "phone", "theme": "light", "violations": [] },
                        { "width": "phone", "theme": "dark", "violations": [violation] },
                        { "width": "desktop", "theme": "light", "violations": [] },
                        { "width": "desktop", "theme": "dark", "violations": [violation] }
                    ]
                }),
            ),
            (
                "preview.prepared",
                json!({ "tree": "4b825dc642cb6eb9a060e54bf8d69288fbee4904", "seconds": 212 }),
            ),
            ("preview.started", json!({ "port": 4400 })),
            ("preview.stopped", json!({ "reason": "The session ended." })),
            (
                "page.checked",
                json!({
                    "width": "desktop",
                    "theme": "dark",
                    "path": "/settings",
                    "violations": [violation],
                    "screenshot": "session-1-desktop-dark.png"
                }),
            ),
            (
                "tool.called",
                json!({
                    "tool": "mcp__playwright__browser_navigate",
                    "input": "{\"url\":\"http://localhost:4400/\"}",
                    "server": "playwright",
                    "tag": "network"
                }),
            ),
            (
                "tool.denied",
                json!({
                    "tool": "mcp__playwright__browser_evaluate",
                    "reason": "tool_denied: browser_evaluate runs script in the page",
                    "server": "playwright",
                    "tag": "denied"
                }),
            ),
            (
                "escalation.raised",
                json!({ "reason": "preview", "detail": "npm ERR! missing script: dev" }),
            ),
        ] {
            let mut wire = an_event_wire(EventKind::NoteWritten);
            wire["kind"] = json!(kind);
            wire["body"] = body;
            let event =
                event_from_value(&wire).unwrap_or_else(|errors| panic!("{kind}: {errors:?}"));
            assert_eq!(event.body.kind().to_string(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
        }
        for (kind, body) in [
            (
                "page.checked",
                json!({ "width": "tablet", "theme": "dark", "path": "/", "violations": [], "screenshot": "s.png" }),
            ),
            (
                "page.checked",
                json!({ "width": "phone", "theme": "sepia", "path": "/", "violations": [], "screenshot": "s.png" }),
            ),
            (
                "tool.called",
                json!({ "tool": "t", "input": "{}", "server": "playwright", "tag": "loud" }),
            ),
            ("preview.started", json!({ "port": 0 })),
        ] {
            let mut wire = an_event_wire(EventKind::NoteWritten);
            wire["kind"] = json!(kind);
            wire["body"] = body;
            assert!(event_from_value(&wire).is_err(), "{kind}: {}", wire["body"]);
        }
    }

    #[test]
    fn reads_the_design_plan_events() {
        let plan = "Make the sign-in page calm.\n\nI saw two buttons fighting for attention.";
        for (kind, body) in [
            ("design_plan.proposed", json!({ "plan": plan })),
            (
                "design_plan.approved",
                json!({ "reason": "It keeps to the contract." }),
            ),
            (
                "design_plan.returned",
                json!({ "reason": "Leave the header alone." }),
            ),
        ] {
            let mut wire = an_event_wire(EventKind::NoteWritten);
            wire["kind"] = json!(kind);
            wire["body"] = body;
            let event =
                event_from_value(&wire).unwrap_or_else(|errors| panic!("{kind}: {errors:?}"));
            assert_eq!(event.body.kind().to_string(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
        }
        let mut wire = an_event_wire(EventKind::NoteWritten);
        wire["kind"] = json!("design_plan.proposed");
        wire["body"] = json!({ "reason": "a decision is not a plan" });
        assert_eq!(refusal(&wire)[0].path, "/body");
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
    fn keeps_the_files_an_acceptance_changed() {
        // A task in a private folder's move into `accepted` names the files it changed, so that
        // its page keeps them (6.6). A move with none named, and every log from before the field,
        // read it as absent; a move that changed nothing says so with an empty list, which is not
        // the same.
        let accepted = |changed: Option<serde_json::Value>| {
            let mut wire = an_event_wire(EventKind::TaskTransitioned);
            wire["body"]["from"] = json!("verifying");
            wire["body"]["to"] = json!("accepted");
            wire["body"]["effects"] = json!(["nothing_to_integrate"]);
            if let Some(changed) = changed {
                wire["body"]["changed"] = changed;
            }
            wire
        };
        let changed_of = |wire: &serde_json::Value| {
            let EventBody::TaskTransitioned(body) = event_from_value(wire).expect("valid").body
            else {
                panic!("a task.transitioned event carries a task.transitioned body");
            };
            body.changed
        };
        assert_eq!(changed_of(&accepted(None)), None);
        assert_eq!(
            changed_of(&accepted(Some(json!(["books.xlsx", "evaluations/x.md"])))),
            Some(vec![
                "books.xlsx".to_string(),
                "evaluations/x.md".to_string()
            ])
        );
        assert_eq!(changed_of(&accepted(Some(json!([])))), Some(Vec::new()));
        // Each is written back as it was read, an absent one with no key.
        for wire in [
            accepted(None),
            accepted(Some(json!(["books.xlsx"]))),
            accepted(Some(json!([]))),
        ] {
            let event = event_from_value(&wire).expect("valid");
            assert_eq!(event_to_value(&event), wire);
        }
        assert!(!refusal(&accepted(Some(json!([1])))).is_empty());
        assert!(!refusal(&accepted(Some(json!("books.xlsx")))).is_empty());
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
        // The schema types `body` as a choice of forty-two shapes, so it reports a failure anywhere
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
    fn reads_an_approved_plan_only_with_its_note() {
        // `{ "plan": "MP-1" }` alone is also a valid design plan, and the schema's choice of bodies
        // must match exactly one, so the owner's approval always carries its note, empty when they
        // added nothing.
        let mut bare = an_event_wire(EventKind::MarketingPlanApproved);
        bare["body"] = json!({ "plan": "MP-1" });
        let errors = refusal(&bare);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "/body");
        assert!(
            errors[0]
                .message
                .starts_with("a marketing_plan.approved event does not carry this body"),
            "{}",
            errors[0].message
        );
        let mut design = an_event_wire(EventKind::DesignPlanProposed);
        design["body"] = json!({ "plan": "MP-1" });
        event_from_value(&design).expect("the same body is a design plan's");

        let mut noted = an_event_wire(EventKind::MarketingPlanApproved);
        noted["body"]["note"] = json!("Start small");
        let event = event_from_value(&noted).expect("an approval with a note");
        assert_eq!(event_to_value(&event), noted);
    }

    #[test]
    fn round_trips_every_site_event() {
        let kinds = [
            EventKind::SiteRequested,
            EventKind::SiteApproved,
            EventKind::SiteDeclined,
            EventKind::SiteRemoved,
        ];
        for kind in kinds {
            assert!(EVERY_KIND.contains(&kind), "{kind} is counted");
            let wire = a_full_event_wire(kind);
            let event = event_from_value(&wire).expect("a valid site event");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
        }
        assert_eq!(EVERY_KIND.len(), 101);
    }

    #[test]
    fn round_trips_every_seller_mail_event() {
        let kinds = [
            EventKind::MailboxConnected,
            EventKind::MailboxDisconnected,
            EventKind::SellerMessageDrafted,
            EventKind::SellerMessageSent,
            EventKind::SellerMessageFailed,
            EventKind::SellerMessageDiscarded,
            EventKind::SellerReplyReceived,
            EventKind::SellerReplyDismissed,
        ];
        for kind in kinds {
            assert!(EVERY_KIND.contains(&kind), "{kind} is counted");
            let wire = a_full_event_wire(kind);
            let event = event_from_value(&wire).expect("a valid mail event");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
            // A draft is about its task; the founder's press, a failure and a reply are not
            // recorded about one.
            assert_eq!(
                is_about_one_contract(kind),
                kind == EventKind::SellerMessageDrafted,
                "{kind}"
            );
        }
        assert_eq!(EVERY_KIND.len(), 101);

        let remove = |kind: EventKind, field: &str| {
            let mut wire = an_event_wire(kind);
            wire["body"]
                .as_object_mut()
                .expect("an object")
                .remove(field);
            wire
        };
        assert!(
            !refusal(&remove(EventKind::SellerMessageSent, "sha256")).is_empty(),
            "a sent message without the file's hash"
        );
        // An order's message names its order; the schema holds it to the purpose.
        let mut order_message = an_event_wire(EventKind::SellerMessageDrafted);
        order_message["body"]["purpose"] = json!("purchase_order");
        order_message["body"]
            .as_object_mut()
            .expect("an object")
            .remove("purchase_order");
        assert!(
            !refusal(&order_message).is_empty(),
            "an order's message without its order"
        );
        order_message["body"]["purchase_order"] = json!(12);
        event_from_value(&order_message).expect("an order's message with its order");
        let mut quote = an_event_wire(EventKind::SellerMessageDrafted);
        quote["body"]["purchase_order"] = json!(12);
        assert!(
            !refusal(&quote).is_empty(),
            "a quote request naming an order"
        );
        // Only the procurement mailbox exists before phase 15's receipts.
        for kind in [EventKind::MailboxConnected, EventKind::MailboxDisconnected] {
            let mut wire = an_event_wire(kind);
            wire["body"]["purpose"] = json!("receipts");
            assert!(!refusal(&wire).is_empty(), "{kind} of purpose receipts");
        }
        for (field, value) in [
            ("seller", json!("")),
            ("seller", json!("Pie\nBox")),
            ("to", json!("")),
            ("subject", json!("x".repeat(201))),
            ("subject", json!("Two\nlines")),
            ("purpose", json!("buy")),
            ("sha256", json!("abc")),
            ("message", json!(0)),
        ] {
            let mut wire = an_event_wire(EventKind::SellerMessageDrafted);
            wire["body"][field] = value.clone();
            assert!(!refusal(&wire).is_empty(), "{field} {value}");
        }
        for (kind, field, value) in [
            (EventKind::SellerMessageSent, "edited", json!("no")),
            (
                EventKind::SellerMessageSent,
                "message_id",
                json!("has space@x.test"),
            ),
            (EventKind::SellerMessageFailed, "why", json!("")),
            (EventKind::SellerReplyReceived, "message", json!(0)),
            (EventKind::SellerReplyReceived, "from", json!("")),
            (
                EventKind::SellerReplyReceived,
                "attachments",
                json!([{ "name": "a.pdf" }]),
            ),
            (EventKind::SellerReplyDismissed, "reply", json!(0)),
        ] {
            let mut wire = an_event_wire(kind);
            wire["body"][field] = value.clone();
            assert!(!refusal(&wire).is_empty(), "{kind} {field} {value}");
        }
    }

    #[test]
    fn round_trips_every_pipeline_event() {
        let kinds = [
            EventKind::DataPipelineRequested,
            EventKind::DataPipelineEscalated,
            EventKind::DataPipelineApproved,
            EventKind::DataPipelineDeclined,
        ];
        for kind in kinds {
            assert!(EVERY_KIND.contains(&kind), "{kind} is counted");
            let wire = a_full_event_wire(kind);
            let event = event_from_value(&wire).expect("a valid data pipeline event");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
            // The request is about its task, which waits on nothing; a decision is about the
            // request, and names no task of its own.
            assert_eq!(
                is_about_one_contract(kind),
                kind == EventKind::DataPipelineRequested,
                "{kind}"
            );
        }
        assert_eq!(EVERY_KIND.len(), 101);

        // A decision session says which request it decides.
        let mut started = an_event_wire(EventKind::SessionStarted);
        started["body"]["pipeline"] = json!(4);
        let event = event_from_value(&started).expect("a session started for a pipeline");
        assert_eq!(event_to_value(&event), started);
        started["body"]["pipeline"] = json!(0);
        assert!(
            !refusal(&started).is_empty(),
            "a request is numbered from 1"
        );

        // A request is numbered by its place in the log, so it carries no number.
        let mut numbered = an_event_wire(EventKind::DataPipelineRequested);
        numbered["body"]["pipeline"] = json!(4);
        assert!(
            !refusal(&numbered).is_empty(),
            "a request carrying a number"
        );
        // An approval names the request it filed; a decline files none.
        let mut approved = an_event_wire(EventKind::DataPipelineApproved);
        approved["body"]
            .as_object_mut()
            .expect("an object")
            .remove("request");
        assert!(!refusal(&approved).is_empty(), "an approval without it");
        let mut declined = an_event_wire(EventKind::DataPipelineDeclined);
        declined["body"]["request"] = json!("FRK-9");
        assert!(!refusal(&declined).is_empty(), "a decline with one");
        // Two deciders: the Product Manager and the owner. `auto` is phase 9 step 01's.
        for kind in [
            EventKind::DataPipelineApproved,
            EventKind::DataPipelineDeclined,
        ] {
            for by in ["product_manager", "human"] {
                let mut wire = an_event_wire(kind);
                wire["body"]["by"] = json!(by);
                event_from_value(&wire).expect("a decider");
            }
            for by in ["auto", "farik", "agent"] {
                let mut wire = an_event_wire(kind);
                wire["body"]["by"] = json!(by);
                assert!(!refusal(&wire).is_empty(), "{kind} by {by}");
            }
        }
        // What a request says of the source is held to the same words the tool holds it to.
        for (field, value) in [
            ("name", json!("")),
            ("name", json!("x".repeat(101))),
            ("name", json!("Fire\ncrawl")),
            ("what", json!("x".repeat(19))),
            ("what", json!("x".repeat(601))),
            ("why", json!("x".repeat(19))),
            ("why", json!("x".repeat(601))),
            ("source_url", json!("")),
            ("source_url", json!("x".repeat(2001))),
            ("cost", json!("cheap")),
            ("needs_account", json!("yes")),
            ("sends_project_data", json!(null)),
        ] {
            let mut wire = an_event_wire(EventKind::DataPipelineRequested);
            wire["body"][field] = value.clone();
            assert!(!refusal(&wire).is_empty(), "{field} {value}");
        }
        for field in ["needs_account", "sends_project_data", "cost"] {
            let mut wire = an_event_wire(EventKind::DataPipelineRequested);
            wire["body"]
                .as_object_mut()
                .expect("an object")
                .remove(field);
            assert!(!refusal(&wire).is_empty(), "a request without {field}");
        }
        let mut escalated = an_event_wire(EventKind::DataPipelineEscalated);
        escalated["body"]["pipeline"] = json!(0);
        assert!(!refusal(&escalated).is_empty(), "escalating request 0");
    }

    #[test]
    fn round_trips_the_budget_events() {
        for kind in [
            EventKind::MarketingBudgetReached,
            EventKind::MarketingCampaignPaused,
        ] {
            assert!(EVERY_KIND.contains(&kind), "{kind} is counted");
            let wire = an_event_wire(kind);
            let event = event_from_value(&wire).expect("a valid budget event");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
            // Farik records it, about no contract: it needs no task on its envelope.
            assert!(
                !is_about_one_contract(kind),
                "{kind} is about no one contract"
            );
        }
        // A cap is of a campaign or of the plan; a pause says why in one of three words.
        let mut reached = an_event_wire(EventKind::MarketingBudgetReached);
        reached["body"]["scope"] = json!("channel");
        assert_eq!(refusal(&reached).len(), 1);
        let mut paused = an_event_wire(EventKind::MarketingCampaignPaused);
        paused["body"]["why"] = json!("the owner asked");
        assert_eq!(refusal(&paused).len(), 1);
        // A refusal of Google is kept in words, and a pause that was refused lists what was.
        let mut failed = an_event_wire(EventKind::MarketingBudgetReached);
        failed["body"]["failed"] = json!("Google answered \"quota\"");
        failed["body"]["paused"] = json!([]);
        event_from_value(&failed).expect("a refused pause");
        failed["body"]["failed"] = json!("x".repeat(301));
        assert_eq!(refusal(&failed).len(), 1, "words are cut at 300");
    }

    #[test]
    fn round_trips_every_order_and_renewal_event() {
        let orders = [
            EventKind::PurchaseOrderDrafted,
            EventKind::PurchaseOrderApproved,
            EventKind::PurchaseOrderRejected,
            EventKind::PurchaseOrderPlaced,
            EventKind::PurchaseOrderUpdated,
            EventKind::PurchaseOrderReceived,
            EventKind::PurchaseOrderClosed,
            EventKind::PurchaseOrderExpired,
        ];
        let renewals = [
            EventKind::RenewalFlagged,
            EventKind::RenewalDismissed,
            EventKind::RenewalChecked,
        ];
        for kind in orders.into_iter().chain(renewals) {
            assert!(EVERY_KIND.contains(&kind), "{kind} is counted");
            let wire = an_event_wire(kind);
            let event = event_from_value(&wire).expect("a valid order or renewal event");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event_to_value(&event), wire, "{kind}");
            // An order's step is about its task; a renewal is about none.
            assert_eq!(
                is_about_one_contract(kind),
                orders.contains(&kind),
                "{kind}"
            );
        }
        assert_eq!(EVERY_KIND.len(), 101);

        // A decision always carries its note, empty when the owner said nothing, which leaves a
        // body of the order alone to an expiry: the schema's choice of bodies must match one.
        for kind in [
            EventKind::PurchaseOrderApproved,
            EventKind::PurchaseOrderRejected,
            EventKind::PurchaseOrderClosed,
        ] {
            let mut wire = an_event_wire(kind);
            wire["body"]
                .as_object_mut()
                .expect("an object")
                .remove("note");
            let errors = refusal(&wire);
            assert!(!errors.is_empty(), "{kind} without its note");
            assert!(errors.iter().all(|error| error.path.starts_with("/body")));
        }
        let mut expired = an_event_wire(EventKind::PurchaseOrderExpired);
        expired["body"]["note"] = json!("");
        assert!(!refusal(&expired).is_empty(), "an expiry with a note");
        // The agent's follow-up is one of four statuses, never a step only the owner takes.
        for status in [
            "placed",
            "received",
            "paid",
            "confirmed",
            "delivered",
            "cancelled",
        ] {
            let mut updated = an_event_wire(EventKind::PurchaseOrderUpdated);
            updated["body"]["status"] = json!(status);
            let errors = refusal(&updated);
            assert!(!errors.is_empty(), "a status {status}");
            assert!(errors.iter().all(|error| error.path.starts_with("/body")));
        }
        for status in ["preparing", "shipped", "delayed", "problem"] {
            let mut updated = an_event_wire(EventKind::PurchaseOrderUpdated);
            updated["body"]["status"] = json!(status);
            event_from_value(&updated).expect("a follow-up status");
        }
    }

    /// The body of an order event with `changes` applied, each a JSON pointer into the body and
    /// the value to put there.
    fn changed(kind: EventKind, changes: &[(&str, serde_json::Value)]) -> serde_json::Value {
        let mut wire = an_event_wire(kind);
        for (pointer, value) in changes {
            if let Some(field) = wire["body"].pointer_mut(pointer) {
                *field = value.clone();
            } else {
                // A field the fixture leaves out: its parent is an object and its name the last part.
                let (parent, name) = pointer.rsplit_once('/').expect("a pointer into the body");
                wire["body"]
                    .pointer_mut(parent)
                    .and_then(serde_json::Value::as_object_mut)
                    .expect("an object")
                    .insert(name.to_string(), value.clone());
            }
        }
        wire
    }

    #[test]
    fn holds_a_drafted_order_to_its_own_shape() {
        let line = json!({
            "item": "Baby car mirror", "quantity": 3, "unit": "piece",
            "unit_price": "19.99", "line_total": "59.97"
        });
        let drafted = |changes: &[(&str, serde_json::Value)]| {
            changed(EventKind::PurchaseOrderDrafted, changes)
        };
        event_from_value(&drafted(&[])).expect("the fixture is an order");
        for (what, pointer, value) in [
            ("no lines", "/lines", json!([])),
            ("51 lines", "/lines", json!(vec![line.clone(); 51])),
            ("a zero quantity", "/lines/0/quantity", json!(0)),
            ("too many of them", "/lines/0/quantity", json!(1_000_001)),
            (
                "a price with three decimals",
                "/lines/0/unit_price",
                json!("19.999"),
            ),
            (
                "a total with a separator",
                "/lines/0/line_total",
                json!("1,059.97"),
            ),
            ("a seller over a line break", "/seller", json!("A\nB")),
            ("no seller", "/seller", json!("")),
            ("a lower-case currency", "/currency", json!("usd")),
            ("a weekly period", "/period", json!("week")),
            ("a total past the limit", "/total", json!("10000000.01")),
            ("a short reason", "/why", json!("too short")),
            ("a long reason", "/why", json!("w".repeat(601))),
            ("an unknown field", "/paid", json!("1.00")),
        ] {
            let errors = refusal(&drafted(&[(pointer, value)]));
            assert!(!errors.is_empty(), "{what}");
            assert!(
                errors.iter().all(|error| error.path.starts_with("/body")),
                "{what}: {errors:?}"
            );
        }
        // The edges of the limits the schema holds an order to: 19 and 20 characters of why, 100
        // and 101 of seller.
        for (field, length, allowed) in [
            ("why", 19, false),
            ("why", 20, true),
            ("why", 600, true),
            ("seller", 100, true),
            ("seller", 101, false),
        ] {
            let wire = drafted(&[(&format!("/{field}"), json!("w".repeat(length)))]);
            assert_eq!(
                event_from_value(&wire).is_ok(),
                allowed,
                "{field} of {length} characters"
            );
        }
        let mut free = line;
        free["unit"] = json!("");
        event_from_value(&drafted(&[("/lines", json!([free]))])).expect("a line may have no unit");
    }

    #[test]
    fn holds_each_step_of_an_order_and_a_renewal_to_its_own_shape() {
        // What was paid and when are an owner's, and the currency is three capitals.
        for (kind, pointer, value) in [
            (EventKind::PurchaseOrderPlaced, "/paid", json!("1,450.00")),
            (EventKind::PurchaseOrderPlaced, "/paid", json!("1450")),
            (EventKind::PurchaseOrderPlaced, "/currency", json!("usd")),
            (
                EventKind::PurchaseOrderPlaced,
                "/placed_on",
                json!("tomorrow"),
            ),
            (
                EventKind::PurchaseOrderReceived,
                "/renews_on",
                json!("next year"),
            ),
            (EventKind::PurchaseOrderUpdated, "/note", json!("a\nb")),
            (
                EventKind::PurchaseOrderUpdated,
                "/note",
                json!("n".repeat(301)),
            ),
            (
                EventKind::PurchaseOrderApproved,
                "/note",
                json!("n".repeat(601)),
            ),
            (EventKind::RenewalFlagged, "/vendor", json!("")),
            (EventKind::RenewalDismissed, "/renewal", json!(0)),
            (EventKind::RenewalChecked, "/unreadable", json!(-1)),
        ] {
            let wire = changed(kind, &[(pointer, value.clone())]);
            assert!(!refusal(&wire).is_empty(), "{kind} {pointer} {value}");
        }
        for (kind, changes) in [
            (
                EventKind::PurchaseOrderPlaced,
                vec![("/paid", json!("1450.00")), ("/currency", json!("EUR"))],
            ),
            (
                EventKind::PurchaseOrderReceived,
                vec![
                    ("/paid", json!("0.00")),
                    ("/currency", json!("EUR")),
                    ("/renews_on", json!("2027-10-08")),
                ],
            ),
            (
                EventKind::PurchaseOrderUpdated,
                vec![("/note", json!("")), ("/expected_on", json!("2026-10-20"))],
            ),
            (
                EventKind::PurchaseOrderApproved,
                vec![("/note", json!("Go ahead."))],
            ),
        ] {
            let wire = changed(kind, &changes);
            let event = event_from_value(&wire).expect("a valid step");
            assert_eq!(event_to_value(&event), wire, "{kind}");
        }
    }

    #[test]
    fn a_site_request_and_its_decline_name_their_task() {
        for kind in [EventKind::SiteRequested, EventKind::SiteDeclined] {
            let mut wire = an_event_wire(kind);
            wire.as_object_mut()
                .expect("an object")
                .remove("task_id")
                .expect("a contract-scoped kind carries its task");
            let errors = refusal(&wire);
            assert_eq!(errors.len(), 1, "{kind}");
            assert_eq!(errors[0].path, "/task_id", "{kind}");
        }
        for kind in [EventKind::SiteApproved, EventKind::SiteRemoved] {
            event_from_value(&an_event_wire(kind)).expect("an added or removed site has no task");
            event_from_value(&a_full_event_wire(kind))
                .expect("an approval answering a request has one");
        }
    }

    #[test]
    fn holds_a_site_event_to_its_own_shape() {
        let wire = |kind, body: serde_json::Value| {
            let mut wire = an_event_wire(kind);
            wire["body"] = body;
            wire
        };
        // What the schema refuses, at a path under the body.
        for (kind, body) in [
            (
                EventKind::SiteDeclined,
                json!({ "request": 7, "host": "shop.example", "note": "x".repeat(601) }),
            ),
            (
                EventKind::SiteRequested,
                json!({ "host": "shop.example", "url": "https://shop.example/", "why": "" }),
            ),
            (
                EventKind::SiteRequested,
                json!({ "host": "shop.example", "url": "https://shop.example/", "why": "a\nb" }),
            ),
            (
                EventKind::SiteRequested,
                json!({ "host": "shop.example", "url": "https://shop.example/", "why": "x".repeat(301) }),
            ),
            (
                EventKind::SiteApproved,
                json!({ "host": "shop.example", "request": 0 }),
            ),
        ] {
            let errors = refusal(&wire(kind, body.clone()));
            assert!(!errors.is_empty(), "{kind} {body}");
            assert!(
                errors.iter().all(|error| error.path.starts_with("/body")),
                "{kind} {body}: {errors:?}"
            );
        }
        // What the three kinds that share a body each refuse of it: a decline names its request,
        // a removal names its host alone, and a note goes with a request.
        for (kind, body) in [
            (EventKind::SiteDeclined, json!({ "host": "shop.example" })),
            (
                EventKind::SiteDeclined,
                json!({ "host": "shop.example", "note": "No." }),
            ),
            (
                EventKind::SiteRemoved,
                json!({ "host": "shop.example", "request": 7 }),
            ),
            (
                EventKind::SiteRemoved,
                json!({ "host": "shop.example", "note": "No." }),
            ),
            (
                EventKind::SiteApproved,
                json!({ "host": "shop.example", "note": "Go on." }),
            ),
        ] {
            let errors = refusal(&wire(kind, body.clone()));
            assert_eq!(errors.len(), 1, "{kind} {body}");
            assert_eq!(errors[0].path, "/body", "{kind} {body}");
            assert!(
                errors[0]
                    .message
                    .starts_with(&format!("a {kind} event does not carry this body")),
                "{}",
                errors[0].message
            );
        }
        // What they read.
        for (kind, body) in [
            (
                EventKind::SiteDeclined,
                json!({ "request": 7, "host": "shop.example" }),
            ),
            (
                EventKind::SiteDeclined,
                json!({ "request": 7, "host": "shop.example", "note": "No." }),
            ),
            (EventKind::SiteApproved, json!({ "host": "shop.example" })),
            (
                EventKind::SiteApproved,
                json!({ "host": "shop.example", "request": 7, "note": "Go on." }),
            ),
            (EventKind::SiteRemoved, json!({ "host": "shop.example" })),
        ] {
            let input = wire(kind, body.clone());
            let event = event_from_value(&input)
                .unwrap_or_else(|errors| panic!("{kind} {body}: {errors:?}"));
            assert_eq!(event_to_value(&event), input, "{kind} {body}");
        }
        let declined = event_from_value(&wire(
            EventKind::SiteDeclined,
            json!({ "request": 7, "host": "shop.example", "note": "No." }),
        ))
        .expect("a decline with words");
        let EventBody::SiteDeclined(body) = declined.body else {
            panic!("a site.declined event carries a site.declined body");
        };
        assert_eq!(body.request.map(std::num::NonZeroU64::get), Some(7));
        assert_eq!(
            body.note.map(|note| note.to_string()).as_deref(),
            Some("No.")
        );
    }

    #[test]
    fn reads_a_team_paused_and_resumed_by_the_human() {
        assert_eq!(EVERY_KIND.len(), 101);
        for kind in [EventKind::TeamPaused, EventKind::TeamResumed] {
            assert_eq!(a_body_wire(kind), json!({ "by": "human" }));
            let input = an_event_wire(kind);
            let event = event_from_value(&input).expect("valid");
            assert_eq!(event.body.kind(), kind);
            assert_eq!(event_to_value(&event), input);
        }
    }

    #[test]
    fn refuses_a_team_paused_by_anyone_but_the_human() {
        for kind in [EventKind::TeamPaused, EventKind::TeamResumed] {
            let mut input = an_event_wire(kind);
            input["body"]["by"] = json!("governor");
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{kind}");
            assert_eq!(errors[0].path, "/body/by", "{kind}");
        }
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

    /// `kind`'s fixture event with `change` applied to its body.
    fn post_event(
        kind: EventKind,
        change: impl FnOnce(&mut serde_json::Value),
    ) -> serde_json::Value {
        let mut input = an_event_wire(kind);
        change(&mut input["body"]);
        input
    }

    #[test]
    fn reads_the_social_post_bodies_with_their_optional_fields() {
        // The owner's own allowance of a request: `post`, no plan and no slot.
        let allowed = post_event(EventKind::SocialPostScheduled, |body| {
            body["approved_by"] = json!("owner");
            body["post"] = json!(42);
            body.as_object_mut().expect("an object").remove("plan");
            body.as_object_mut().expect("an object").remove("slot");
        });
        // A YouTube and a Pinterest post, with the details each needs.
        let youtube = post_event(EventKind::SocialPostRequested, |body| {
            body["channel"] = json!("youtube");
            body["details"] = json!({ "title": "Opening day", "category_id": "22" });
        });
        let pinterest = post_event(EventKind::SocialPostRequested, |body| {
            body["channel"] = json!("pinterest");
            body["details"] = json!({ "board": "board-1" });
        });
        let taken_back = post_event(EventKind::SocialPostStopped, |body| {
            body["taken_back"] = json!(true);
        });
        let declined = post_event(EventKind::SocialPostStopped, |body| {
            body["by"] = json!("declined");
            body["note"] = json!("Not this week");
        });
        for input in [allowed, youtube, pinterest, taken_back, declined] {
            let event = event_from_value(&input).expect("valid");
            assert_eq!(event_to_value(&event), input);
        }
        // A time keeps its offset: the slot's day is the date in it.
        let event =
            event_from_value(&an_event_wire(EventKind::SocialPostScheduled)).expect("valid");
        let EventBody::SocialPostScheduled(body) = &event.body else {
            panic!("a scheduled post");
        };
        assert_eq!(body.at.as_str(), "2026-11-04T09:00:00-05:00");
    }

    #[test]
    fn refuses_a_social_post_body_that_breaks_a_rule_of_its_kind() {
        let bad = |kind, change: &dyn Fn(&mut serde_json::Value)| {
            let input = post_event(kind, |body| change(body));
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{input}: {errors:?}");
            assert!(errors[0].path.starts_with("/body"), "{}", errors[0].path);
        };
        let scheduled = EventKind::SocialPostScheduled;
        let requested = EventKind::SocialPostRequested;
        // Who approved a post: the plan or the owner, and nothing else yet (phase 9 step 01 adds auto).
        bad(scheduled, &|body| {
            body["approved_by"] = json!("auto");
        });
        // At most four pictures or clips, each an image or a video.
        let media = |count: usize| -> serde_json::Value {
            json!((0..count)
                .map(|n| json!({ "url": format!("https://cdn.example.com/{n}.png"), "kind": "image" }))
                .collect::<Vec<_>>())
        };
        bad(scheduled, &|body| {
            body["media"] = media(5);
        });
        bad(scheduled, &|body| {
            body["media"][0]["kind"] = json!("audio");
        });
        assert!(event_from_value(&post_event(scheduled, |body| body["media"] = media(4))).is_ok());
        // A time with its offset, and Buffer's id of the channel and nothing it could be confused
        // with.
        bad(scheduled, &|body| {
            body["at"] = json!("2026-11-04T09:00:00");
        });
        bad(scheduled, &|body| {
            body["at"] = json!("tomorrow at nine");
        });
        bad(requested, &|body| {
            body["buffer_channel"] = json!("chan 1");
        });
        bad(requested, &|body| {
            body["text"] = json!("");
        });
        bad(requested, &|body| {
            body["channel"] = json!("myspace");
        });
        // The post's number counts from one.
        bad(scheduled, &|body| {
            body["post"] = json!(0);
        });
        // A request carries no approval.
        bad(requested, &|body| {
            body["approved_by"] = json!("owner");
        });
        // The details of YouTube: a title of at most 100 characters and one of Buffer's categories.
        let youtube =
            |title: String, category: &str| json!({ "title": title, "category_id": category });
        bad(requested, &|body| {
            body["details"] = youtube("t".repeat(101), "22");
        });
        bad(requested, &|body| {
            body["details"] = youtube("Title".to_string(), "3");
        });
        bad(requested, &|body| {
            body["details"] = json!({ "board": "b", "title": "t" });
        });
        bad(requested, &|body| {
            body["details"] = json!({ "board": "no spaces" });
        });
        // What happens to a post is one of the words of its kind.
        bad(EventKind::SocialPostStopped, &|body| {
            body["by"] = json!("robot");
        });
        bad(EventKind::SocialPostMissed, &|body| {
            body["why"] = json!("forgot");
        });
        bad(EventKind::SocialPostSent, &|body| {
            body["buffer_post"] = json!("");
        });
        bad(EventKind::SocialPostFailed, &|body| {
            body["reason"] = json!("");
        });
    }

    #[test]
    fn a_social_post_that_is_written_or_requested_names_its_task() {
        for kind in [
            EventKind::SocialPostScheduled,
            EventKind::SocialPostRequested,
        ] {
            assert!(super::is_about_one_contract(kind), "{kind:?}");
            let mut input = an_event_wire(kind);
            input.as_object_mut().expect("an object").remove("task_id");
            let errors = refusal(&input);
            assert_eq!(errors.len(), 1, "{kind:?}: {errors:?}");
        }
        for kind in [
            EventKind::SocialPostSent,
            EventKind::SocialPostStopped,
            EventKind::SocialPostMissed,
            EventKind::SocialPostFailed,
        ] {
            assert!(!super::is_about_one_contract(kind), "{kind:?}");
            event_from_value(&an_event_wire(kind)).expect("about no one contract");
        }
    }
}
