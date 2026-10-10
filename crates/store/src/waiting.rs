//! What waits on the human (`docs/SPEC.md` 5.7 and 5.16): the questions nobody answered, the
//! connector calls waiting to be allowed, the plans awaiting approval, the escalations, the results waiting on the human's acceptance, and
//! the tasks waiting to be integrated by hand. One list, which the command line and the browser
//! both read.

use std::str::FromStr;

use catervas_core::contract::{Role, TaskId, TaskKind, TaskStatus};
use catervas_core::governor::done::result_awaits_human;
use catervas_core::governor::permissions::ApprovalKey;
use catervas_core::governor::sites::site_of;
use catervas_core::marketing::{PostChannel, network_name};
use catervas_core::pipeline::PipelineCost;
use catervas_core::team::{Integration, Team};
use catervas_protocol::event::{
    CatervasEvent, EventBody, EventKind, SellerMessagePurpose, TaskStatusWire,
};
use chrono::{DateTime, FixedOffset};

use crate::files::ProjectFiles;
use crate::marketing::{PostMedia, PostState, marketing_plans, social_posts};
use crate::pipelines::{PipelineState, cost_of, data_pipelines};
use crate::purchase_orders::{OrderState, expires_at, purchase_orders};
use crate::seller_mail::{MessageState, seller_mail};
use crate::sites::site_requests;
use crate::{EventLog, EventQuery, Projections, StoreError, TaskProjection};

/// What kind of thing waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingKind {
    /// A plan to approve.
    Approval,
    /// A finished result to accept.
    Acceptance,
    /// An agent's question.
    Question,
    /// Any other escalation.
    Help,
    /// An accepted task to add to the project by hand.
    Integration,
    /// A connector's call to allow or refuse (ADR 0031).
    ToolApproval,
    /// A marketing plan the Marketing Specialist proposed, to approve or send back (ADR 0042).
    MarketingPlan,
    /// A post outside the plan, which the owner allows or does not (ADR 0042).
    SocialPost,
    /// A site the Procurement Specialist asked to read, which the owner allows or does not
    /// (ADR 0039).
    SiteRequest,
    /// A purchase order the Procurement Specialist set up, which the owner approves or rejects
    /// and places themselves (ADR 0039). It holds no task.
    PurchaseOrder,
    /// A data pipeline request the Product Manager passed to the owner, or did not decide, which
    /// the owner approves or declines (ADR 0039). It holds no task.
    DataPipeline,
}

impl WaitingKind {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Acceptance => "acceptance",
            Self::Question => "question",
            Self::Help => "help",
            Self::Integration => "integration",
            Self::ToolApproval => "tool_approval",
            Self::MarketingPlan => "marketing_plan",
            Self::SocialPost => "social_post",
            Self::SiteRequest => "site_request",
            Self::PurchaseOrder => "purchase_order",
            Self::DataPipeline => "data_pipeline",
        }
    }
}

/// One thing that waits on the human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The task it is about.
    pub task_id: TaskId,
    /// What waits.
    pub kind: WaitingKind,
    /// The agent it waits with, when there is one.
    pub agent_id: Option<String>,
    /// The task's title.
    pub title: String,
    /// What waits, in a sentence the person reads.
    pub line: String,
    /// A question's id: the seq of its `question.asked`.
    pub question_id: Option<u64>,
    /// An escalation's reason, as the log words it.
    pub reason: Option<String>,
    /// A connector call's ask.
    pub approval: Option<ToolAsk>,
    /// A marketing plan's ask.
    pub plan: Option<PlanAsk>,
    /// A post's ask.
    pub post: Option<PostAsk>,
    /// A site's ask.
    pub site: Option<SiteAsk>,
    /// A purchase order's ask.
    pub order: Option<OrderAsk>,
    /// A data pipeline request's ask.
    pub pipeline: Option<PipelineAsk>,
}

/// A data pipeline request that waits for the owner, as its `data_pipeline.requested` recorded
/// it. Every text field but `host` is the agent's own words, which are untrusted; `reason` is the
/// Product Manager's, and absent when Catervas passed the request on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineAsk {
    /// The request's number, the seq of its `data_pipeline.requested`.
    pub pipeline: u64,
    /// The source's name.
    pub name: String,
    /// What it would give the agent.
    pub what: String,
    /// The source's own page, exactly as the agent wrote it.
    pub url: String,
    /// The site that page is on, in its ASCII form.
    pub host: String,
    /// Why the agent asked.
    pub why: String,
    /// What the agent says it costs.
    pub cost: PipelineCost,
    /// Whether the agent says it needs an account.
    pub needs_account: bool,
    /// Whether the agent says it sends the project's data out.
    pub sends_project_data: bool,
    /// Why the Product Manager passed it on, when it did.
    pub reason: Option<String>,
    /// When the agent asked.
    pub at: DateTime<chrono::Utc>,
}

/// One line of a purchase order that waits, as the agent wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAskLine {
    /// What is bought. The agent's words.
    pub item: String,
    /// How many.
    pub quantity: u32,
    /// What one is counted in, possibly empty. The agent's words.
    pub unit: String,
    /// The price of one, with two decimals.
    pub unit_price: String,
    /// Quantity times price, with two decimals.
    pub line_total: String,
}

/// The message that goes with a purchase order that waits, when the Procurement Specialist drafted
/// one (step 10f): sent with the order when the owner presses Approve and send. Its text is in a
/// file the runtime reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderSend {
    /// The message's number.
    pub message: u64,
    /// The seller's address. The agent's words.
    pub to: String,
    /// The subject. The agent's words.
    pub subject: String,
}

/// A purchase order that waits for the owner, as its `purchase_order.drafted` recorded it. Every
/// text field is the agent's own words, which are untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAsk {
    /// The order's number, the n of PO-n.
    pub order: u64,
    /// The seller.
    pub seller: String,
    /// How to reach the seller.
    pub seller_contact: String,
    /// The lines, in the order the agent wrote them.
    pub lines: Vec<OrderAskLine>,
    /// The currency of every amount.
    pub currency: String,
    /// `once`, `month` or `year`.
    pub period: String,
    /// The lines' exact sum, with two decimals.
    pub total: String,
    /// Delivery, as the agent wrote it.
    pub delivery: String,
    /// Terms, as the agent wrote them.
    pub terms: String,
    /// The seller's page, exactly as the agent wrote it, or empty.
    pub url: String,
    /// The comparison the order rests on, `evaluations/<name>.md`.
    pub evaluation: String,
    /// Why this seller, in the agent's words.
    pub why: String,
    /// When it was drafted.
    pub at: DateTime<chrono::Utc>,
    /// When Catervas closes it by itself if nobody decides.
    pub expires_at: DateTime<chrono::Utc>,
    /// The message that goes with it, while one waits.
    pub send: Option<OrderSend>,
}

/// A site an agent asked to read, as its `site.requested` recorded it. Its host is in ASCII; its
/// address and its reason are the agent's own words, which are untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAsk {
    /// The request's number, the seq of its `site.requested`.
    pub request: u64,
    /// The site.
    pub host: String,
    /// The first page the agent wants, exactly as it wrote it.
    pub url: String,
    /// Why, in the agent's words.
    pub why: String,
}

/// A post outside the plan that waits for the owner, as its `social_post.requested` recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostAsk {
    /// The post's number.
    pub post: u64,
    /// The network.
    pub channel: PostChannel,
    /// What it says.
    pub text: String,
    /// Its pictures and clips.
    pub media: Vec<PostMedia>,
    /// When it would go out, with the offset it was written in.
    pub at: DateTime<FixedOffset>,
}

/// A marketing plan that waits for the owner, as its `marketing_plan.proposed` recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanAsk {
    /// The plan's id, `MP-<n>`.
    pub plan: String,
    /// What the agent tells the owner first.
    pub summary: String,
    /// The most it spends in all, as a decimal string with two decimals.
    pub total: String,
    /// The currency of every amount.
    pub currency: String,
    /// Its first day.
    pub starts_on: chrono::NaiveDate,
    /// Its last day.
    pub ends_on: chrono::NaiveDate,
}

/// A connector call that waits for the human, as its `tool_approval.requested` recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAsk {
    /// The event's seq, the approval's id.
    pub approval: u64,
    /// The connector's server.
    pub server: String,
    /// The bare tool name.
    pub tool: String,
    /// The call's whole input, as compact JSON.
    pub input: String,
}

/// A grant of the human's that one call may still use (ADR 0031): `tool_approval.granted` was the
/// first decision on the approval, no `tool.called` used it, and no session of the asking agent
/// about the task that started after the grant has ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGrant {
    /// The seq of the `tool_approval.requested`.
    pub approval: u64,
    /// The seq of its `tool_approval.granted`: only a session started after it may use it.
    pub granted_at: u64,
    /// What the call must match.
    pub key: ApprovalKey,
}

/// The grants still open among one task's events, which must hold its `tool_approval.` events,
/// its `tool.called`, and its `session.started` and `session.ended`. A decision recorded with an
/// agent or a session on its envelope was not the human's, and decides nothing.
#[must_use]
pub fn open_grants(events: &[CatervasEvent]) -> Vec<OpenGrant> {
    events
        .iter()
        .filter_map(|asked| {
            let EventBody::ToolApprovalRequested(body) = &asked.body else {
                return None;
            };
            let approval = asked.envelope.seq;
            let ids = &asked.envelope.ids;
            let granted = decision_on(events, approval)?;
            let EventBody::ToolApprovalGranted(_) = granted.body else {
                return None;
            };
            let granted_at = granted.envelope.seq;
            let agent_id = ids.agent_id.clone()?;
            let task_id = ids.task_id.clone()?;
            let used = events.iter().any(|event| {
                matches!(&event.body, EventBody::ToolCalled(called)
                    if called.approval.map(std::num::NonZeroU64::get) == Some(approval))
            });
            // A session of the asking agent that started after the grant, and has ended since.
            let lapsed = events.iter().any(|started| {
                started.envelope.seq > granted_at
                    && matches!(started.body, EventBody::SessionStarted(_))
                    && started.envelope.ids.agent_id.as_deref() == Some(agent_id.as_str())
                    && events.iter().any(|ended| {
                        ended.envelope.seq > started.envelope.seq
                            && matches!(ended.body, EventBody::SessionEnded(_))
                            && ended.envelope.ids.session_id.is_some()
                            && ended.envelope.ids.session_id == started.envelope.ids.session_id
                    })
            });
            (!used && !lapsed).then(|| OpenGrant {
                approval,
                granted_at,
                key: ApprovalKey {
                    agent_id,
                    task_id,
                    server: body.server.to_string(),
                    tool: body.tool.to_string(),
                    input_sha256: body.input_sha256.to_string(),
                },
            })
        })
        .collect()
}

/// The first decision the human recorded on `approval`, granted or refused.
#[must_use]
pub fn decision_on(events: &[CatervasEvent], approval: u64) -> Option<&CatervasEvent> {
    events.iter().find(|event| {
        let ids = &event.envelope.ids;
        ids.agent_id.is_none()
            && ids.session_id.is_none()
            && match &event.body {
                EventBody::ToolApprovalGranted(body) | EventBody::ToolApprovalRefused(body) => {
                    body.approval.get() == approval
                }
                _ => false,
            }
    })
}

/// The kinds that say what an agent asked the human, and what the human answered.
const ASKED: [EventKind; 6] = [
    EventKind::QuestionAsked,
    EventKind::QuestionAnswered,
    EventKind::EscalationRaised,
    EventKind::ToolApprovalRequested,
    EventKind::ToolApprovalGranted,
    EventKind::ToolApprovalRefused,
];

/// Everything that waits on the human, in groups (questions, connector calls, approvals, other
/// escalations, acceptances, integrations), each by task id.
///
/// # Errors
///
/// What the store refused.
#[allow(
    clippy::too_many_lines,
    reason = "one group of rows for each thing that waits"
)]
pub fn waiting(
    projections: &Projections,
    log: &EventLog,
    files: &ProjectFiles,
    team: &Team,
) -> Result<Vec<Waiting>, StoreError> {
    projections.catch_up()?;
    let board = projections.board()?;
    let history = log.read(&EventQuery {
        kinds: ASKED.to_vec(),
        ..EventQuery::default()
    })?;
    let item = |row: &TaskProjection, kind, agent_id: Option<&str>, line: String| Waiting {
        task_id: row.task_id.clone(),
        kind,
        agent_id: agent_id.map(str::to_string),
        title: row.title.clone(),
        line,
        question_id: None,
        reason: None,
        approval: None,
        plan: None,
        post: None,
        site: None,
        order: None,
        pipeline: None,
    };
    let mut waiting = unanswered(&board, &history, &item);
    waiting.extend(undecided(&board, &history, team, &item));
    waiting.extend(plans_waiting(&board, log, team, &item)?);
    waiting.extend(posts_waiting(&board, log, team, &item)?);
    waiting.extend(sites_waiting(&board, log, team, &item)?);
    waiting.extend(orders_waiting(&board, log, team, &item)?);
    waiting.extend(pipelines_waiting(&board, log, team, &item)?);
    let product_manager = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
        .map(|agent| agent.id.to_string());
    for row in board.iter().filter(|row| row.awaiting_approval) {
        let pm = product_manager.as_deref();
        let line = format!(
            "{} wrote a plan for you to approve",
            name_of(team, pm.unwrap_or("the Product Manager"))
        );
        waiting.push(item(row, WaitingKind::Approval, pm, line));
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Escalated && !row.awaiting_approval)
    {
        let reason = history
            .iter()
            .rev()
            .filter(|event| event.envelope.ids.task_id.as_ref() == Some(&row.task_id))
            .find_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason.to_string()),
                _ => None,
            });
        let agent = row.assignee_id.as_deref().or(product_manager.as_deref());
        let line = format!(
            "{} needs your help: {}",
            name_of(team, agent.unwrap_or("the team")),
            reason
                .as_deref()
                .map_or("it did not say why", reason_in_words)
        );
        waiting.push(Waiting {
            reason,
            ..item(row, WaitingKind::Help, agent, line)
        });
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Verifying)
    {
        let mut contract = files
            .read_contract(&row.task_id)
            .map_err(|error| StoreError::Io {
                detail: error.to_string(),
            })?;
        contract.status = row.status;
        if !result_awaits_human(&contract) {
            continue;
        }
        // A task's result reaches the human once its reviewer passed it; an epic's is theirs.
        if contract.kind != TaskKind::Epic
            && !review_passed(&log.read(&EventQuery {
                task_id: Some(row.task_id.clone()),
                ..EventQuery::default()
            })?)
        {
            continue;
        }
        let assignee = row.assignee_id.as_deref();
        let finished = name_of(team, assignee.unwrap_or("the team"));
        let line = match row.reviewer_id.as_deref() {
            Some(reviewer) => format!(
                "{finished} finished it and {} reviewed it",
                name_of(team, reviewer)
            ),
            None => format!("{finished} finished it"),
        };
        waiting.push(item(row, WaitingKind::Acceptance, assignee, line));
    }
    if team.policy.integration == Integration::Manual {
        for row in board.iter().filter(|row| row.awaiting_integration) {
            waiting.push(item(
                row,
                WaitingKind::Integration,
                row.assignee_id.as_deref(),
                "Accepted, waiting for you to add it".to_string(),
            ));
        }
    }
    Ok(waiting)
}

/// Whether the latest `review.recorded` since the task last entered `verifying` passed.
#[must_use]
pub fn review_passed(history: &[CatervasEvent]) -> bool {
    let since =
        last_move_into(history, TaskStatus::Verifying).map_or(0, |event| event.envelope.seq);
    history
        .iter()
        .rev()
        .take_while(|event| event.envelope.seq > since)
        .find_map(|event| match &event.body {
            EventBody::ReviewRecorded(body) => Some(body.passed),
            _ => None,
        })
        .unwrap_or(false)
}

/// Whether `event` is a `task.transitioned` into `status`.
#[must_use]
pub fn is_move_into(event: &CatervasEvent, status: TaskStatus) -> bool {
    matches!(&event.body, EventBody::TaskTransitioned(body) if wire_status(body.to) == Some(status))
}

/// The task's last `task.transitioned` into `status`.
#[must_use]
pub fn last_move_into(history: &[CatervasEvent], status: TaskStatus) -> Option<&CatervasEvent> {
    history
        .iter()
        .rev()
        .find(|event| is_move_into(event, status))
}

/// A wire status as the contract's own. The two lists are one, which a test in `catervas-protocol`
/// pins, so `None` is a log no Catervas wrote.
fn wire_status(status: TaskStatusWire) -> Option<TaskStatus> {
    TaskStatus::from_str(&status.to_string()).ok()
}

/// Every question nobody answered, by task.
fn unanswered(
    board: &[TaskProjection],
    history: &[CatervasEvent],
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Vec<Waiting> {
    let mut waiting = Vec::new();
    for row in board {
        for event in history {
            let EventBody::QuestionAsked(body) = &event.body else {
                continue;
            };
            let seq = event.envelope.seq;
            let answered = history.iter().any(|later| {
                matches!(&later.body, EventBody::QuestionAnswered(answer)
                    if answer.question_id.get() == seq)
            });
            if answered || event.envelope.ids.task_id.as_ref() != Some(&row.task_id) {
                continue;
            }
            waiting.push(Waiting {
                question_id: Some(seq),
                ..item(
                    row,
                    WaitingKind::Question,
                    Some(&body.asked_by),
                    body.question.clone(),
                )
            });
        }
    }
    waiting
}

/// Every connector call nobody allowed or refused, by task (ADR 0031).
fn undecided(
    board: &[TaskProjection],
    history: &[CatervasEvent],
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Vec<Waiting> {
    let mut waiting = Vec::new();
    for row in board {
        for asked in history {
            let EventBody::ToolApprovalRequested(body) = &asked.body else {
                continue;
            };
            let approval = asked.envelope.seq;
            if asked.envelope.ids.task_id.as_ref() != Some(&row.task_id)
                || decision_on(history, approval).is_some()
            {
                continue;
            }
            let agent = asked.envelope.ids.agent_id.as_deref();
            let line = format!(
                "{} wants to use {}",
                name_of(team, agent.unwrap_or("an agent")),
                body.server.as_str()
            );
            waiting.push(Waiting {
                approval: Some(ToolAsk {
                    approval,
                    server: body.server.to_string(),
                    tool: body.tool.to_string(),
                    input: body.input.clone(),
                }),
                ..item(row, WaitingKind::ToolApproval, agent, line)
            });
        }
    }
    waiting
}

/// Every marketing plan nobody decided yet, oldest first (ADR 0042). Its row's title is the
/// plan's, and its line says who proposes it.
fn plans_waiting(
    board: &[TaskProjection],
    log: &EventLog,
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Result<Vec<Waiting>, StoreError> {
    let mut waiting = Vec::new();
    for plan in marketing_plans(log)?
        .into_iter()
        .filter(|plan| plan.decided.is_none())
    {
        let Some(row) = board.iter().find(|row| row.task_id == plan.task_id) else {
            continue;
        };
        let proposal = &plan.proposal;
        let line = format!(
            "{} proposes a marketing plan: {}",
            name_of(team, &plan.agent_id),
            proposal.title
        );
        waiting.push(Waiting {
            title: proposal.title.clone(),
            plan: Some(PlanAsk {
                plan: plan.record.id.clone(),
                summary: proposal.summary.clone(),
                total: proposal.total.to_string(),
                currency: proposal.currency.clone(),
                starts_on: proposal.starts_on,
                ends_on: proposal.ends_on,
            }),
            ..item(row, WaitingKind::MarketingPlan, Some(&plan.agent_id), line)
        });
    }
    Ok(waiting)
}

/// Every request to read a site nobody decided yet, oldest first (ADR 0039). Its row is about the
/// task that asked, and its line says who asks to read which site.
fn sites_waiting(
    board: &[TaskProjection],
    log: &EventLog,
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Result<Vec<Waiting>, StoreError> {
    let mut waiting = Vec::new();
    for asked in site_requests(log)?
        .into_iter()
        .filter(|asked| asked.decision.is_none())
    {
        let Some(row) = board.iter().find(|row| row.task_id == asked.task_id) else {
            continue;
        };
        let line = format!(
            "{} asks to read {}",
            name_of(team, &asked.agent_id),
            asked.host
        );
        waiting.push(Waiting {
            site: Some(SiteAsk {
                request: asked.request,
                host: asked.host.clone(),
                url: asked.url.clone(),
                why: asked.why.clone(),
            }),
            ..item(row, WaitingKind::SiteRequest, Some(&asked.agent_id), line)
        });
    }
    Ok(waiting)
}

/// Every purchase order nobody decided yet, oldest first (ADR 0039). Its row is about the task it
/// was drafted in, which it does not hold, and its line says who set up an order from whom.
fn orders_waiting(
    board: &[TaskProjection],
    log: &EventLog,
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Result<Vec<Waiting>, StoreError> {
    let mut waiting = Vec::new();
    let mail = seller_mail(log)?;
    for record in purchase_orders(log)?
        .into_iter()
        .filter(|record| record.state == OrderState::Drafted)
    {
        let Some(row) = board.iter().find(|row| row.task_id == record.task_id) else {
            continue;
        };
        let Some(expires) = expires_at(&record) else {
            continue;
        };
        let body = &record.drafted;
        let line = format!(
            "{} set up an order from {}: {} {}",
            name_of(team, &record.agent_id),
            body.seller.as_str(),
            body.total.as_str(),
            body.currency.as_str()
        );
        waiting.push(Waiting {
            order: Some(OrderAsk {
                order: record.order,
                seller: body.seller.to_string(),
                seller_contact: body.seller_contact.to_string(),
                lines: body
                    .lines
                    .iter()
                    .map(|line| OrderAskLine {
                        item: line.item.to_string(),
                        quantity: u32::try_from(line.quantity.get()).unwrap_or(u32::MAX),
                        unit: line.unit.to_string(),
                        unit_price: line.unit_price.as_str().to_string(),
                        line_total: line.line_total.as_str().to_string(),
                    })
                    .collect(),
                currency: body.currency.as_str().to_string(),
                period: body.period.to_string(),
                total: body.total.as_str().to_string(),
                delivery: body.delivery.to_string(),
                terms: body.terms.to_string(),
                url: body.url.to_string(),
                evaluation: body.evaluation.to_string(),
                why: body.why.to_string(),
                at: record.drafted_at,
                expires_at: expires,
                send: mail
                    .messages
                    .iter()
                    .find(|message| {
                        message.state == MessageState::Waiting
                            && message.drafted.purpose == SellerMessagePurpose::PurchaseOrder
                            && message.drafted.purchase_order.as_ref().map(|n| n.get())
                                == Some(record.order)
                    })
                    .map(|message| OrderSend {
                        message: message.message,
                        to: message.drafted.to.to_string(),
                        subject: message.drafted.subject.to_string(),
                    }),
            }),
            ..item(
                row,
                WaitingKind::PurchaseOrder,
                Some(&record.agent_id),
                line,
            )
        });
    }
    Ok(waiting)
}

/// Every data pipeline request the owner has to decide, oldest first (ADR 0039): the ones the
/// Product Manager passed on, or that Catervas did after its three tries. An open one waits on the
/// Product Manager, not on the owner. Its row is about the task whose agent asked, which it does
/// not hold, and its line says who asks for which source.
fn pipelines_waiting(
    board: &[TaskProjection],
    log: &EventLog,
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Result<Vec<Waiting>, StoreError> {
    let mut waiting = Vec::new();
    for record in data_pipelines(log)?
        .into_iter()
        .filter(|record| record.state == PipelineState::Escalated)
    {
        let Some(row) = board.iter().find(|row| row.task_id == record.task_id) else {
            continue;
        };
        let body = &record.requested;
        let line = format!(
            "{} asks for a data source: {}",
            name_of(team, &record.agent_id),
            body.name.as_str()
        );
        waiting.push(Waiting {
            pipeline: Some(PipelineAsk {
                pipeline: record.pipeline,
                name: body.name.to_string(),
                what: body.what.to_string(),
                url: body.source_url.to_string(),
                host: site_of(body.source_url.as_str()).unwrap_or_default(),
                why: body.why.to_string(),
                cost: cost_of(body.cost),
                needs_account: body.needs_account,
                sends_project_data: body.sends_project_data,
                reason: record
                    .escalated_reason
                    .clone()
                    .filter(|_| !record.by_catervas),
                at: record.requested_at,
            }),
            ..item(row, WaitingKind::DataPipeline, Some(&record.agent_id), line)
        });
    }
    Ok(waiting)
}

/// Every post outside the plan nobody decided or missed yet, oldest first (ADR 0042). Its row is
/// about the task the request was written in, and its line says who wants to post where.
fn posts_waiting(
    board: &[TaskProjection],
    log: &EventLog,
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Result<Vec<Waiting>, StoreError> {
    let mut waiting = Vec::new();
    for post in social_posts(log)?
        .into_iter()
        .filter(|post| post.state == PostState::Requested)
    {
        let Some(row) = board.iter().find(|row| row.task_id == post.task_id) else {
            continue;
        };
        let line = format!(
            "{} wants to post on {}",
            name_of(team, &post.agent_id),
            network_name(post.channel)
        );
        waiting.push(Waiting {
            post: Some(PostAsk {
                post: post.post,
                channel: post.channel,
                text: post.text.clone(),
                media: post.media.clone(),
                at: post.at,
            }),
            ..item(row, WaitingKind::SocialPost, Some(&post.agent_id), line)
        });
    }
    Ok(waiting)
}

/// An agent's display name, or `id` itself when the team has no such agent.
#[must_use]
pub fn name_of(team: &Team, id: &str) -> String {
    team.agents
        .iter()
        .find(|agent| agent.id.as_str() == id)
        .map_or_else(|| id.to_string(), |agent| agent.display_name.to_string())
}

/// Why a task escalated, as the log words it, in words a person reads.
#[must_use]
pub fn reason_in_words(reason: &str) -> &'static str {
    match reason {
        "budget" => "it reached its budget",
        "sessions" => "it used all its sessions",
        "iterations" => "it used all its tries",
        "blocker_age" => "it has been stuck too long",
        "permission" => "it needs a permission it does not have",
        "risk_gate" => "the plan is high risk",
        "approval" => "a plan waits for your approval",
        "readiness_failures" => "its plan keeps failing its checks",
        "integration" => "its work could not be added to the project",
        "explicit_request" => "it asked for you",
        _ => "it did not say why",
    }
}

/// Builders for the logs the tests of what waits, what the agents do, and what moved read.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;
    use std::sync::Arc;

    use catervas_core::contract::validate_contract;
    use catervas_core::team::{Team, validate_team};
    use catervas_protocol::event::{CatervasEvent, NewEvent, event_from_value};
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{Value, json};

    use crate::files::ProjectFiles;
    use crate::files::fixtures::TempProject;
    use crate::{EventLog, IN_MEMORY, Projections, open_event_log, open_projections};

    pub(crate) fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, hour, minute, 0)
            .single()
            .expect("a real time")
    }

    /// A team of Ada (Product Manager), Linus (Developer), and Grace (Architect), integrating by
    /// hand.
    pub(crate) fn a_team() -> Team {
        let mut wire = catervas_core::team::fixtures::a_team_wire();
        wire["agents"] = json!([
            { "id": "ada", "display_name": "Ada", "role": "product_manager", "status": "active" },
            { "id": "linus", "display_name": "Linus", "role": "software_developer", "status": "active" },
            { "id": "grace", "display_name": "Grace", "role": "architect", "status": "active" },
        ]);
        validate_team(&wire).expect("the fixture is a team")
    }

    /// A project's files, log, and board, all empty but for the team.
    pub(crate) struct Board {
        pub(crate) _project: TempProject,
        pub(crate) files: ProjectFiles,
        pub(crate) log: Arc<EventLog>,
        pub(crate) projections: Projections,
    }

    impl Board {
        pub(crate) fn new(name: &str) -> Self {
            let project = TempProject::new(name);
            let files = project.files();
            files.init(&a_team()).expect(".catervas/ is made");
            let log = Arc::new(
                open_event_log(Path::new(IN_MEMORY), at(9, 0)).expect("a log in memory opens"),
            );
            let projections = open_projections(Arc::clone(&log)).expect("the projections open");
            Self {
                _project: project,
                files,
                log,
                projections,
            }
        }

        /// Appends one event at `at`, about `task` and by `agent` when named, and projects it.
        #[allow(
            clippy::needless_pass_by_value,
            reason = "the tests build each body in the call"
        )]
        pub(crate) fn put(
            &self,
            when: DateTime<Utc>,
            task: Option<&str>,
            agent: Option<&str>,
            kind: &str,
            body: Value,
        ) -> CatervasEvent {
            self.put_with(when, task, agent, None, kind, body)
        }

        /// `put`, in `agent`'s session `session`.
        #[allow(
            clippy::needless_pass_by_value,
            reason = "the tests build each body in the call"
        )]
        pub(crate) fn session(
            &self,
            when: DateTime<Utc>,
            task: Option<&str>,
            agent: &str,
            session: &str,
            kind: &str,
            body: Value,
        ) -> CatervasEvent {
            self.put_with(when, task, Some(agent), Some(session), kind, body)
        }

        #[allow(
            clippy::needless_pass_by_value,
            reason = "the tests build each body in the call"
        )]
        pub(crate) fn put_with(
            &self,
            when: DateTime<Utc>,
            task: Option<&str>,
            agent: Option<&str>,
            session: Option<&str>,
            kind: &str,
            body: Value,
        ) -> CatervasEvent {
            let mut wire = json!({
                "seq": 1, "recorded_at": when.to_rfc3339(),
                "team_id": "catervas", "project_id": "catervas",
                "kind": kind, "body": body
            });
            if let Some(task) = task {
                wire["task_id"] = json!(task);
            }
            if let Some(agent) = agent {
                wire["agent_id"] = json!(agent);
            }
            if let Some(session) = session {
                wire["session_id"] = json!(session);
            }
            let event = event_from_value(&wire).unwrap_or_else(|e| panic!("{wire}: {e:?}"));
            let appended = self
                .log
                .append(&NewEvent {
                    recorded_at: event.envelope.recorded_at,
                    ids: event.envelope.ids,
                    body: event.body,
                })
                .expect("appends");
            self.projections.apply(&appended).expect("projects");
            appended
        }

        /// Files `task`, titled `title`, in the files and the log, with `change` applied to the
        /// fixture contract.
        pub(crate) fn file(&self, task: &str, title: &str, change: impl FnOnce(&mut Value)) {
            let mut wire = catervas_core::contract::fixtures::a_contract_wire();
            wire["id"] = json!(task);
            wire["title"] = json!(title);
            change(&mut wire);
            let contract = validate_contract(&wire).expect("the fixture is a contract");
            self.files
                .create_contract(&contract)
                .expect("the file is made");
            self.put(
                at(9, 0),
                Some(task),
                None,
                "task.created",
                json!({ "created_by": "human", "summary": {
                    "kind": wire.get("kind").cloned().unwrap_or(json!("task")),
                    "title": title, "status": "draft", "risk": wire["risk"]
                }}),
            );
        }

        /// Moves `task` from `from` to `to` at `when`, as `by` asked, held by `assignee` and
        /// reviewed by `reviewer` when named.
        pub(crate) fn moved(
            &self,
            when: DateTime<Utc>,
            task: &str,
            (from, to): (&str, &str),
            by: &str,
            people: (Option<&str>, Option<&str>),
        ) -> CatervasEvent {
            let mut body = json!({
                "from": from, "to": to, "actor": if by == "human" { "human" } else { "assignee" },
                "requested_by": by, "gate": "none", "effects": [], "iteration": 0
            });
            if let Some(assignee) = people.0 {
                body["assignee"] = json!(assignee);
            }
            if let Some(reviewer) = people.1 {
                body["reviewer"] = json!(reviewer);
            }
            self.put(when, Some(task), None, "task.transitioned", body)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{Board, a_team, at};
    use super::{WaitingKind, waiting};

    /// A passing review of `task` by Grace.
    fn reviewed(board: &Board, task: &str) {
        board.put(
            at(9, 2),
            Some(task),
            Some("grace"),
            "review.recorded",
            json!({ "reviewer": "grace", "criteria_run": 1, "passed": true }),
        );
    }

    #[test]
    fn lists_a_result_once_its_review_passed() {
        let board = Board::new("waiting-reviewed");
        let people = (Some("linus"), Some("grace"));
        let high = |wire: &mut serde_json::Value| wire["risk"] = json!("high");
        board.file("CTV-1", "Unreviewed work", high);
        board.moved(at(9, 1), "CTV-1", ("draft", "verifying"), "linus", people);
        // A pass from before the task went back to work is not this verification's.
        board.file("CTV-2", "Reworked work", high);
        board.moved(at(9, 2), "CTV-2", ("draft", "verifying"), "linus", people);
        reviewed(&board, "CTV-2");
        board.moved(
            at(9, 3),
            "CTV-2",
            ("verifying", "in_progress"),
            "human",
            people,
        );
        board.moved(
            at(9, 4),
            "CTV-2",
            ("in_progress", "verifying"),
            "linus",
            people,
        );
        // An epic's result is the human's to review, so it waits on no reviewer.
        board.file("CTV-3", "An epic", |wire| wire["kind"] = json!("epic"));
        board.moved(
            at(9, 5),
            "CTV-3",
            ("draft", "verifying"),
            "linus",
            (Some("linus"), None),
        );

        let listed = waiting(&board.projections, &board.log, &board.files, &a_team())
            .expect("the store reads");
        let seen: Vec<(&str, &str)> = listed
            .iter()
            .map(|item| (item.task_id.as_str(), item.line.as_str()))
            .collect();
        assert_eq!(seen, vec![("CTV-3", "Linus finished it")]);

        reviewed(&board, "CTV-1");
        let listed = waiting(&board.projections, &board.log, &board.files, &a_team())
            .expect("the store reads");
        assert_eq!(listed[0].line, "Linus finished it and Grace reviewed it");
    }

    #[test]
    fn lists_what_waits_on_the_human() {
        let board = Board::new("waiting-five");
        let people = (Some("linus"), Some("grace"));
        board.file("CTV-1", "A plan", |_| {});
        board.moved(
            at(9, 1),
            "CTV-1",
            ("draft", "escalated"),
            "ada",
            (None, None),
        );
        board.put(
            at(9, 1),
            Some("CTV-1"),
            None,
            "escalation.raised",
            json!({ "reason": "approval", "detail": "waits" }),
        );
        board.file("CTV-2", "A result", |wire| {
            wire["exit_criteria"][0]["verification"] =
                json!({ "method": "human", "question": "Does it look right?" });
        });
        board.moved(at(9, 2), "CTV-2", ("draft", "verifying"), "linus", people);
        reviewed(&board, "CTV-2");
        board.file("CTV-3", "A question", |_| {});
        let asked = board.put(
            at(9, 3),
            Some("CTV-3"),
            Some("linus"),
            "question.asked",
            json!({ "question": "Which colour should the button be?", "asked_by": "linus" }),
        );
        board.file("CTV-4", "Stuck work", |_| {});
        board.moved(at(9, 4), "CTV-4", ("draft", "escalated"), "linus", people);
        board.put(
            at(9, 4),
            Some("CTV-4"),
            None,
            "escalation.raised",
            json!({ "reason": "iterations", "detail": "three tries" }),
        );
        board.file("CTV-5", "Done work", |_| {});
        board.moved(at(9, 5), "CTV-5", ("draft", "accepted"), "linus", people);
        // A low-risk result with no human criterion waits on nobody.
        board.file("CTV-6", "Plain work", |_| {});
        board.moved(at(9, 6), "CTV-6", ("draft", "verifying"), "linus", people);

        let listed = waiting(&board.projections, &board.log, &board.files, &a_team())
            .expect("the store reads");
        let seen: Vec<(&str, WaitingKind, Option<&str>, &str, &str)> = listed
            .iter()
            .map(|item| {
                (
                    item.task_id.as_str(),
                    item.kind,
                    item.agent_id.as_deref(),
                    item.title.as_str(),
                    item.line.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            vec![
                (
                    "CTV-3",
                    WaitingKind::Question,
                    Some("linus"),
                    "A question",
                    "Which colour should the button be?"
                ),
                (
                    "CTV-1",
                    WaitingKind::Approval,
                    Some("ada"),
                    "A plan",
                    "Ada wrote a plan for you to approve"
                ),
                (
                    "CTV-4",
                    WaitingKind::Help,
                    Some("linus"),
                    "Stuck work",
                    "Linus needs your help: it used all its tries"
                ),
                (
                    "CTV-2",
                    WaitingKind::Acceptance,
                    Some("linus"),
                    "A result",
                    "Linus finished it and Grace reviewed it"
                ),
                (
                    "CTV-5",
                    WaitingKind::Integration,
                    Some("linus"),
                    "Done work",
                    "Accepted, waiting for you to add it"
                ),
            ]
        );
        assert_eq!(listed[0].question_id, Some(asked.envelope.seq));
        assert_eq!(listed[2].reason.as_deref(), Some("iterations"));
    }

    /// Ada, Linus and Kai, the Marketing Specialist.
    fn with_kai() -> catervas_core::team::Team {
        let mut wire = catervas_core::team::fixtures::a_team_wire();
        wire["agents"] = json!([
            { "id": "ada", "display_name": "Ada", "role": "product_manager", "status": "active" },
            { "id": "linus", "display_name": "Linus", "role": "software_developer", "status": "active" },
            { "id": "kai", "display_name": "Kai", "role": "marketing_specialist", "status": "active" },
        ]);
        catervas_core::team::validate_team(&wire).expect("the fixture is a team")
    }

    /// A request of Kai's for a post outside the plan, going out at `at`.
    fn requested(board: &Board, at: &str) -> u64 {
        board
            .session(
                super::fixtures::at(9, 5),
                Some("CTV-1"),
                "kai",
                "session-1",
                "social_post.requested",
                json!({
                    "channel": "instagram", "buffer_channel": "chan-1",
                    "text": "We open on Wednesday.",
                    "media": [{ "url": "https://example.com/a.png", "kind": "image" }],
                    "at": at,
                }),
            )
            .envelope
            .seq
    }

    #[test]
    fn waiting_lists_a_requested_post() {
        let board = Board::new("waiting-post");
        let team = with_kai();
        board.file("CTV-1", "Spring posts", |_| {});
        let post = requested(&board, "2026-09-28T14:00:00+02:00");
        let rows = |board: &Board| {
            waiting(&board.projections, &board.log, &board.files, &team)
                .expect("the store reads")
                .into_iter()
                .filter(|item| item.kind == WaitingKind::SocialPost)
                .collect::<Vec<_>>()
        };

        let listed = rows(&board);

        assert_eq!(listed.len(), 1);
        let row = &listed[0];
        assert_eq!(row.line, "Kai wants to post on Instagram");
        assert_eq!(row.agent_id.as_deref(), Some("kai"));
        assert_eq!(row.task_id.as_str(), "CTV-1");
        assert_eq!(WaitingKind::SocialPost.as_str(), "social_post");
        let ask = row.post.as_ref().expect("the post's ask");
        assert_eq!(ask.post, post);
        assert_eq!(
            ask.channel,
            catervas_core::marketing::PostChannel::Instagram
        );
        assert_eq!(ask.text, "We open on Wednesday.");
        assert_eq!(
            ask.media,
            [crate::marketing::PostMedia {
                url: "https://example.com/a.png".to_string(),
                video: false
            }]
        );
        assert_eq!(ask.at.to_rfc3339(), "2026-09-28T14:00:00+02:00");
        let all = crate::activity::activity(
            &board.log,
            &board.projections,
            &board.files,
            &team,
            at(12, 0),
        )
        .expect("the store reads");
        let kai = all.iter().find(|one| one.agent_id == "kai").expect("Kai");
        assert_eq!(kai.line, "Waiting on you: may Kai post on Instagram?");

        // Decided by the owner, it waits no more.
        board.put(
            at(9, 6),
            Some("CTV-1"),
            None,
            "social_post.stopped",
            json!({ "post": post, "by": "declined" }),
        );
        assert!(rows(&board).is_empty());

        // Missed, it waits no more either.
        let late = requested(&board, "2026-09-28T14:30:00Z");
        assert_eq!(rows(&board).len(), 1);
        board.put(
            at(9, 7),
            None,
            None,
            "social_post.missed",
            json!({ "post": late, "why": "undecided" }),
        );
        assert!(rows(&board).is_empty());
    }

    /// Ada, Linus and Ivo, the Procurement Specialist.
    fn with_ivo_buying() -> catervas_core::team::Team {
        let mut wire = catervas_core::team::fixtures::a_team_wire();
        wire["agents"] = json!([
            { "id": "ada", "display_name": "Ada", "role": "product_manager", "status": "active" },
            { "id": "linus", "display_name": "Linus", "role": "software_developer", "status": "active" },
            { "id": "ivo", "display_name": "Ivo", "role": "procurement_specialist", "status": "active" },
        ]);
        catervas_core::team::validate_team(&wire).expect("the fixture is a team")
    }

    #[test]
    fn waiting_lists_each_drafted_order() {
        let board = Board::new("waiting-order");
        let team = with_ivo_buying();
        board.file("CTV-1", "Price 500 boxes", |_| {});
        board.session(
            at(9, 5),
            Some("CTV-1"),
            "ivo",
            "session-1",
            "purchase_order.drafted",
            json!({
                "order": 1, "seller": "Acme", "seller_contact": "sales@acme.example",
                "lines": [
                    { "item": "Box", "quantity": 3, "unit": "piece", "unit_price": "19.99", "line_total": "59.97" },
                    { "item": "Tape", "quantity": 1, "unit": "", "unit_price": "0.01", "line_total": "0.01" }
                ],
                "currency": "USD", "period": "month", "total": "59.98", "delivery": "3 days",
                "terms": "Net 30", "url": "https://www.acme.example/shop",
                "evaluation": "evaluations/boxes.md", "why": "It is the cheapest seller that ships here."
            }),
        );
        let rows = |board: &Board| {
            waiting(&board.projections, &board.log, &board.files, &team)
                .expect("the store reads")
                .into_iter()
                .filter(|item| item.kind == WaitingKind::PurchaseOrder)
                .collect::<Vec<_>>()
        };
        let listed = rows(&board);
        assert_eq!(listed.len(), 1);
        let row = &listed[0];
        assert_eq!(row.line, "Ivo set up an order from Acme: 59.98 USD");
        assert_eq!(row.agent_id.as_deref(), Some("ivo"));
        assert_eq!(row.task_id.as_str(), "CTV-1");
        assert_eq!(row.title, "Price 500 boxes");
        assert_eq!(WaitingKind::PurchaseOrder.as_str(), "purchase_order");
        let ask = row.order.as_ref().expect("the order's ask");
        assert_eq!(ask.order, 1);
        assert_eq!(ask.seller, "Acme");
        assert_eq!(ask.seller_contact, "sales@acme.example");
        assert_eq!(ask.lines.len(), 2);
        assert_eq!(ask.lines[0].item, "Box");
        assert_eq!(ask.lines[0].line_total, "59.97");
        assert_eq!(ask.currency, "USD");
        assert_eq!(ask.period, "month");
        assert_eq!(ask.total, "59.98");
        assert_eq!(ask.delivery, "3 days");
        assert_eq!(ask.terms, "Net 30");
        assert_eq!(ask.url, "https://www.acme.example/shop");
        assert_eq!(ask.evaluation, "evaluations/boxes.md");
        assert_eq!(ask.why, "It is the cheapest seller that ships here.");
        assert_eq!(ask.at, at(9, 5));

        // The agent is waiting on the owner, and the task is not held.
        let all = crate::activity::activity(
            &board.log,
            &board.projections,
            &board.files,
            &team,
            at(12, 0),
        )
        .expect("the store reads");
        let ivo = all.iter().find(|one| one.agent_id == "ivo").expect("Ivo");
        assert_eq!(
            ivo.line,
            "Waiting on you: Ivo set up an order from Acme: 59.98 USD"
        );
        assert!(
            !board
                .projections
                .task(&"CTV-1".parse().expect("a task id"))
                .expect("reads")
                .expect("the task")
                .waiting_on_human,
            "an order holds no task"
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one row for each state a request passes through"
    )]
    fn an_escalated_request_waits_on_the_human() {
        let board = Board::new("waiting-pipeline");
        let team = with_ivo_buying();
        board.file("CTV-1", "Price 500 boxes", |_| {});
        let request = |name: &str, minute| {
            board
                .session(
                    at(9, minute),
                    Some("CTV-1"),
                    "ivo",
                    "session-ivo",
                    "data_pipeline.requested",
                    json!({
                        "name": name,
                        "what": "Reads a seller's page as text, even where prices need a browser.",
                        "source_url": "https://www.firecrawl.dev/pricing",
                        "why": "Two of the five sellers show their prices only in a full browser.",
                        "cost": "paid", "needs_account": true, "sends_project_data": false
                    }),
                )
                .envelope
                .seq
        };
        let rows = |board: &Board| {
            waiting(&board.projections, &board.log, &board.files, &team)
                .expect("the store reads")
                .into_iter()
                .filter(|item| item.kind == WaitingKind::DataPipeline)
                .collect::<Vec<_>>()
        };
        let first = request("Firecrawl", 5);
        // Open, it waits on the Product Manager, not on the owner.
        assert!(rows(&board).is_empty());

        // The Product Manager passes it on with its reason.
        board.session(
            at(9, 6),
            None,
            "ada",
            "session-ada",
            "session.started",
            json!({ "purpose": "verify", "model": "claude-opus-5-5", "effort": "high",
                    "pipeline": first }),
        );
        board.session(
            at(9, 7),
            None,
            "ada",
            "session-ada",
            "data_pipeline.escalated",
            json!({ "pipeline": first, "reason": "It needs a paid plan, so it is your call." }),
        );
        let listed = rows(&board);
        assert_eq!(listed.len(), 1);
        let row = &listed[0];
        assert_eq!(row.line, "Ivo asks for a data source: Firecrawl");
        assert_eq!(row.agent_id.as_deref(), Some("ivo"));
        assert_eq!(row.task_id.as_str(), "CTV-1");
        assert_eq!(row.title, "Price 500 boxes");
        assert_eq!(WaitingKind::DataPipeline.as_str(), "data_pipeline");
        let ask = row.pipeline.as_ref().expect("the request's ask");
        assert_eq!(ask.pipeline, first);
        assert_eq!(ask.name, "Firecrawl");
        assert_eq!(
            ask.what,
            "Reads a seller's page as text, even where prices need a browser."
        );
        assert_eq!(ask.url, "https://www.firecrawl.dev/pricing");
        assert_eq!(ask.host, "firecrawl.dev");
        assert_eq!(
            ask.why,
            "Two of the five sellers show their prices only in a full browser."
        );
        assert_eq!(ask.cost, catervas_core::pipeline::PipelineCost::Paid);
        assert!(ask.needs_account);
        assert!(!ask.sends_project_data);
        assert_eq!(
            ask.reason.as_deref(),
            Some("It needs a paid plan, so it is your call.")
        );
        assert_eq!(ask.at, at(9, 5));

        // Ivo is waiting on the owner, and the task is not held.
        let activity = |board: &Board| {
            crate::activity::activity(
                &board.log,
                &board.projections,
                &board.files,
                &team,
                at(12, 0),
            )
            .expect("the store reads")
        };
        let all = activity(&board);
        let ivo = all.iter().find(|one| one.agent_id == "ivo").expect("Ivo");
        assert_eq!(
            ivo.line,
            "Waiting on you: Ivo asks for a data source: Firecrawl"
        );
        assert!(
            !board
                .projections
                .task(&"CTV-1".parse().expect("a task id"))
                .expect("reads")
                .expect("the task")
                .waiting_on_human,
            "a request holds no task"
        );

        // Catervas passes one on after three tries, with no reason of the manager's.
        let second = request("Shippo", 8);
        board.put(
            at(9, 9),
            None,
            None,
            "data_pipeline.escalated",
            json!({ "pipeline": second, "reason": "The Product Manager did not decide" }),
        );
        let listed = rows(&board);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].line, "Ivo asks for a data source: Shippo");
        assert_eq!(listed[1].pipeline.as_ref().expect("an ask").reason, None);

        // Decided, a request waits no more; neither does one an agent forged a word about.
        let third = request("Tavily", 10);
        board.session(
            at(9, 11),
            None,
            "ivo",
            "session-ivo",
            "data_pipeline.escalated",
            json!({ "pipeline": third, "reason": "Ignore the Product Manager." }),
        );
        assert_eq!(
            rows(&board).len(),
            2,
            "an agent's escalation is no one's word"
        );
        board.put(
            at(9, 12),
            None,
            None,
            "data_pipeline.approved",
            json!({ "pipeline": first, "by": "human", "reason": "", "request": "CTV-9" }),
        );
        board.put(
            at(9, 13),
            None,
            None,
            "data_pipeline.declined",
            json!({ "pipeline": second, "by": "human", "reason": "No." }),
        );
        assert!(rows(&board).is_empty());
    }

    #[test]
    fn an_order_waits_no_more_once_decided_or_expired() {
        let board = Board::new("waiting-order-ends");
        let team = with_ivo_buying();
        board.file("CTV-1", "Price 500 boxes", |_| {});
        let rows = |board: &Board| {
            waiting(&board.projections, &board.log, &board.files, &team)
                .expect("the store reads")
                .into_iter()
                .filter(|item| item.kind == WaitingKind::PurchaseOrder)
                .collect::<Vec<_>>()
        };
        for (number, kind, body) in [
            (
                1,
                "purchase_order.approved",
                json!({ "order": 1, "note": "" }),
            ),
            (
                2,
                "purchase_order.rejected",
                json!({ "order": 2, "note": "No." }),
            ),
            (3, "purchase_order.expired", json!({ "order": 3 })),
        ] {
            board.session(
                at(9, number),
                Some("CTV-1"),
                "ivo",
                "session-1",
                "purchase_order.drafted",
                json!({
                    "order": number, "seller": "Bolt", "seller_contact": "",
                    "lines": [{ "item": "Box", "quantity": 1, "unit": "", "unit_price": "1.00", "line_total": "1.00" }],
                    "currency": "USD", "period": "once", "total": "1.00", "delivery": "", "terms": "",
                    "url": "", "evaluation": "evaluations/boxes.md", "why": "A second seller for boxes."
                }),
            );
            assert_eq!(rows(&board).len(), 1, "{kind} waits");
            board.put(at(10, number), Some("CTV-1"), None, kind, body);
            assert!(rows(&board).is_empty(), "{kind} decided");
        }
    }
    /// Ada, Linus and Kai, the Procurement Specialist.
    fn with_kai_buying() -> catervas_core::team::Team {
        let mut wire = catervas_core::team::fixtures::a_team_wire();
        wire["agents"] = json!([
            { "id": "ada", "display_name": "Ada", "role": "product_manager", "status": "active" },
            { "id": "linus", "display_name": "Linus", "role": "software_developer", "status": "active" },
            { "id": "kai", "display_name": "Kai", "role": "procurement_specialist", "status": "active" },
        ]);
        catervas_core::team::validate_team(&wire).expect("the fixture is a team")
    }

    /// A request of Kai's, in her session, to read `url`, on CTV-1.
    fn asked_to_read(board: &Board, minute: u32, host: &str, url: &str) -> u64 {
        board
            .session(
                super::fixtures::at(9, minute),
                Some("CTV-1"),
                "kai",
                "session-1",
                "site.requested",
                json!({ "host": host, "url": url, "why": "It sells the boxes." }),
            )
            .envelope
            .seq
    }

    #[test]
    fn waiting_lists_each_site_request() {
        let board = Board::new("waiting-site");
        let team = with_kai_buying();
        board.file("CTV-1", "Price 500 boxes", |_| {});
        let site = asked_to_read(&board, 6, "shop.example", "https://www.shop.example/boxes");
        let rows = |board: &Board| {
            waiting(&board.projections, &board.log, &board.files, &team)
                .expect("the store reads")
                .into_iter()
                .filter(|item| {
                    matches!(
                        item.kind,
                        WaitingKind::SocialPost | WaitingKind::SiteRequest
                    )
                })
                .collect::<Vec<_>>()
        };

        let all = crate::activity::activity(
            &board.log,
            &board.projections,
            &board.files,
            &team,
            at(12, 0),
        )
        .expect("the store reads");
        let kai = all.iter().find(|one| one.agent_id == "kai").expect("Kai");
        assert_eq!(kai.line, "Waiting on you: may Kai read shop.example?");

        requested(&board, "2026-09-28T14:00:00+02:00");
        let listed = rows(&board);

        // After the posts outside the plan.
        assert_eq!(
            listed.iter().map(|row| row.kind).collect::<Vec<_>>(),
            [WaitingKind::SocialPost, WaitingKind::SiteRequest]
        );
        let row = &listed[1];
        assert_eq!(row.line, "Kai asks to read shop.example");
        assert_eq!(row.agent_id.as_deref(), Some("kai"));
        assert_eq!(row.task_id.as_str(), "CTV-1");
        assert_eq!(WaitingKind::SiteRequest.as_str(), "site_request");
        let ask = row.site.as_ref().expect("the site's ask");
        assert_eq!(ask.request, site);
        assert_eq!(ask.host, "shop.example");
        assert_eq!(ask.url, "https://www.shop.example/boxes");
        assert_eq!(ask.why, "It sells the boxes.");

        // Decided by the owner, either way, it waits no more.
        board.put(
            at(9, 7),
            Some("CTV-1"),
            None,
            "site.approved",
            json!({ "host": "shop.example", "request": site }),
        );
        assert_eq!(rows(&board).len(), 1, "only the post is left");
        let other = asked_to_read(&board, 8, "other.example", "https://other.example/");
        assert_eq!(rows(&board).len(), 2);
        board.put(
            at(9, 9),
            Some("CTV-1"),
            None,
            "site.declined",
            json!({ "request": other, "host": "other.example", "note": "" }),
        );
        assert_eq!(rows(&board).len(), 1);

        // A decision an agent's session recorded is no decision.
        let third = asked_to_read(&board, 10, "third.example", "https://third.example/");
        board.session(
            at(9, 11),
            Some("CTV-1"),
            "kai",
            "session-1",
            "site.approved",
            json!({ "host": "third.example", "request": third }),
        );
        assert_eq!(rows(&board).len(), 2, "the request still waits");
    }
}
