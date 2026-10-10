//! Builders for test events, usable by every crate's tests.

use serde_json::{Value, json};

use crate::event::{EventKind, NewEvent, event_from_value};

/// A schema-valid wire event of one kind, with that kind's body, and no optional envelope field
/// beyond the `task_id` that a contract-scoped kind may not be recorded without.
#[must_use]
pub fn an_event_wire(kind: EventKind) -> Value {
    let mut event = json!({
        "seq": 1,
        "recorded_at": "2026-09-17T10:00:00Z",
        "team_id": "catervas",
        "project_id": "catervas",
        "kind": kind.to_string(),
        "body": a_body_wire(kind)
    });
    if crate::event::is_about_one_contract(kind) {
        event["task_id"] = json!("FRK-1");
    }
    event
}

/// A schema-valid event of one kind, ready to append: what `new_event` would have produced, built
/// from `an_event_wire` so that a test of the store and a test of the protocol cannot drift apart.
///
/// # Panics
///
/// When `an_event_wire` stops being schema-valid, which is the fixture's own bug.
#[must_use]
pub fn a_new_event(kind: EventKind) -> NewEvent {
    let event = event_from_value(&an_event_wire(kind)).expect("the fixture is schema-valid");
    NewEvent {
        recorded_at: event.envelope.recorded_at,
        ids: event.envelope.ids,
        body: event.body,
    }
}

/// The same event with every optional envelope field present.
#[must_use]
pub fn a_full_event_wire(kind: EventKind) -> Value {
    let mut event = an_event_wire(kind);
    event["task_id"] = json!("FRK-1");
    event["agent_id"] = json!("maya-chen");
    event["session_id"] = json!("session-1");
    event
}

/// The body one kind carries, schema-valid and with no optional field.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per kind of event")]
pub fn a_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::TaskCreated | EventKind::ContractWritten => a_summary_body_wire(kind),
        EventKind::RequestTriaged => json!({
            "size": "small",
            "reason": "One deliverable and one reviewer.",
            "triaged_by": "sam-ortiz"
        }),
        EventKind::ContractLocked | EventKind::ContractUnlocked => a_hold_body_wire(kind),
        EventKind::DriftDetected | EventKind::ProjectScanned => a_project_body_wire(kind),
        EventKind::TeamUpdated => json!({
            "team_name": "Catervas",
            "agent_ids": ["maya-chen", "sam-ortiz"],
            "updated_by": "human"
        }),
        EventKind::CriteriaUpdated => json!({
            "criterion_names": ["the check passes"],
            "updated_by": "human"
        }),
        EventKind::CostRecorded => json!({
            "purpose": "implement",
            "model_id": "claude-sonnet-4-5",
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 100,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0
            },
            "cost_usd": 0.5,
            "unpriced": false
        }),
        EventKind::BudgetExhausted => json!({ "scope": "day_usd", "consequence": "pause_team" }),
        EventKind::TaskTransitioned => json!({
            "from": "ready",
            "to": "assigned",
            "actor": "product_manager",
            "requested_by": "maya-chen",
            "gate": "assignment",
            "effects": [],
            "assignee": "dev-a",
            "reviewer": "dev-b",
            "iteration": 0
        }),
        EventKind::TransitionRefused => json!({
            "from": "ready",
            "to": "assigned",
            "actor": "product_manager",
            "requested_by": "maya-chen",
            "refusal": "gate_failed",
            "details": ["the reviewer is the assignee"]
        }),
        EventKind::EscalationRaised | EventKind::EscalationAged => an_escalation_body_wire(kind),
        EventKind::MemoryWritten | EventKind::DecisionWritten => a_kept_body_wire(kind),
        EventKind::ContractEvaluated => json!({
            "gate": "definition_of_ready",
            "passed": false,
            "failures": ["the contract has no exit criteria"]
        }),
        EventKind::ContractJudged => a_judgment_body_wire(),
        EventKind::CriterionRecorded => json!({
            "criterion_id": "C1",
            "passed": true,
            "evidence": "cargo xtask check: ok",
            "run_by": "assignee",
            "recorded_by": "dev-a"
        }),
        EventKind::NoteWritten => json!({
            "kind": "completion",
            "text": "The login page is done and its tests pass.",
            "written_by": "dev-a"
        }),
        EventKind::ReviewRecorded | EventKind::ProductDocWritten => a_record_body_wire(kind),
        EventKind::ToolCalled => a_tool_body_wire("input", "{\"file_path\":\"src/lib.rs\"}"),
        EventKind::ToolDenied => a_tool_body_wire("reason", "tool_not_allowed: Bash has no tier"),
        EventKind::ToolReturned => a_tool_body_wire("output", "{\"type\":\"text\"}"),
        EventKind::SessionStarted | EventKind::SessionEnded => a_session_body_wire(kind),
        EventKind::TaskIntegrated | EventKind::PullRequestOpened => an_integration_body_wire(kind),
        EventKind::QuestionAsked
        | EventKind::QuestionAnswered
        | EventKind::HumanAccepted
        | EventKind::EscalationResolved
        | EventKind::MessagePosted
        | EventKind::ChatMessagePosted => a_human_body_wire(kind),
        EventKind::AgentUpdated | EventKind::AgentSlept => an_agent_body_wire(kind),
        EventKind::TeamPaused | EventKind::TeamResumed => json!({ "by": "human" }),
        EventKind::DesignPlanProposed
        | EventKind::DesignPlanApproved
        | EventKind::DesignPlanReturned
        | EventKind::DesignReviewRecorded
        | EventKind::PreviewPrepared
        | EventKind::PreviewStarted
        | EventKind::PreviewStopped
        | EventKind::PageChecked => a_design_body_wire(kind),
        EventKind::ConnectorConnected
        | EventKind::ConnectorDisconnected
        | EventKind::ToolApprovalRequested
        | EventKind::ToolApprovalGranted
        | EventKind::ToolApprovalRefused => a_connector_body_wire(kind),
        EventKind::SkillAdded
        | EventKind::SkillChanged
        | EventKind::SkillRemoved
        | EventKind::SkillConfirmed => a_skill_body_wire(kind),
        EventKind::SprintStarted
        | EventKind::SprintPlanned
        | EventKind::SprintEnded
        | EventKind::RetroAppended => a_sprint_body_wire(kind),
        EventKind::MarketingPlanProposed
        | EventKind::MarketingPlanApproved
        | EventKind::MarketingPlanReturned
        | EventKind::MarketingPlanEnded => a_marketing_plan_body_wire(kind),
        EventKind::SocialPostScheduled
        | EventKind::SocialPostRequested
        | EventKind::SocialPostSent
        | EventKind::SocialPostStopped
        | EventKind::SocialPostMissed
        | EventKind::SocialPostFailed => a_social_post_body_wire(kind),
        EventKind::SiteRequested => json!({
            "host": "shop.example",
            "url": "https://www.shop.example/boxes?size=12x9x6",
            "why": "It sells the corrugated boxes the task asks about."
        }),
        EventKind::SiteApproved => json!({
            "host": "shop.example",
            "request": 7,
            "note": "Go on, and keep the quotes."
        }),
        EventKind::SiteDeclined => json!({
            "request": 7,
            "host": "shop.example",
            "note": "We do not buy from them."
        }),
        EventKind::SiteRemoved => json!({ "host": "shop.example" }),
        EventKind::PurchaseOrderDrafted => json!({
            "order": 1,
            "seller": "Acme Auto Parts",
            "seller_contact": "sales@acme.example",
            "lines": [{
                "item": "Baby car mirror",
                "quantity": 3,
                "unit": "piece",
                "unit_price": "19.99",
                "line_total": "59.97"
            }],
            "currency": "USD",
            "period": "once",
            "total": "59.97",
            "delivery": "Ships in 3 days",
            "terms": "Net 30",
            "url": "https://www.acme.example/mirrors",
            "evaluation": "evaluations/baby-car-mirrors.md",
            "why": "It is the cheapest seller that ships to us with a safety mark."
        }),
        EventKind::PurchaseOrderApproved
        | EventKind::PurchaseOrderRejected
        | EventKind::PurchaseOrderClosed => json!({ "order": 1, "note": "" }),
        EventKind::PurchaseOrderPlaced => json!({ "order": 1, "placed_on": "2026-10-08" }),
        EventKind::PurchaseOrderUpdated => json!({
            "order": 1,
            "status": "shipped",
            "note": "Left the seller's depot."
        }),
        EventKind::PurchaseOrderReceived => json!({ "order": 1, "received_on": "2026-10-15" }),
        EventKind::PurchaseOrderExpired => json!({ "order": 1 }),
        EventKind::RenewalFlagged => json!({
            "vendor": "Vercel",
            "renews_on": "2026-11-30",
            "decide_by": "2026-10-31"
        }),
        EventKind::RenewalDismissed => json!({ "renewal": 7 }),
        EventKind::RenewalChecked => json!({ "due": 1, "unreadable": 0 }),
        EventKind::DataPipelineRequested => json!({
            "name": "Firecrawl",
            "what": "Prices as clean text from the seller pages the task has to compare.",
            "source_url": "https://www.firecrawl.dev/pricing",
            "why": "Three sellers hide their prices behind scripts that the plain page fetch cannot read.",
            "cost": "paid",
            "needs_account": true,
            "sends_project_data": false
        }),
        EventKind::DataPipelineEscalated => json!({
            "pipeline": 7,
            "reason": "It costs money and the owner decides what the team spends."
        }),
        EventKind::DataPipelineApproved => json!({
            "pipeline": 7,
            "by": "human",
            "reason": "",
            "request": "FRK-9"
        }),
        EventKind::DataPipelineDeclined => json!({
            "pipeline": 7,
            "by": "product_manager",
            "reason": "The plain pages answer the question, so use them."
        }),
        EventKind::MailboxConnected => json!({
            "purpose": "procurement",
            "address": "buying@bakery.test"
        }),
        EventKind::MailboxDisconnected => json!({ "purpose": "procurement" }),
        EventKind::SellerMessageDrafted => json!({
            "message": 3,
            "seller": "Pie Box Pros",
            "to": "sales@pieboxpros.test",
            "subject": "Quote for 500 printed pie boxes",
            "purpose": "quote_request",
            "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c"
        }),
        EventKind::SellerMessageSent => json!({
            "message": 3,
            "message_id": "0b9d6f7e-1c2a-4f3b-8a5d-6e7f8091a2b3@bakery.test",
            "sha256": "9f2b0c1d5e7a4b3c8d6e1f0a2b4c6d8e0f1a3b5c7d9e1f2a4b6c8d0e2f4a6b8c",
            "edited": false
        }),
        EventKind::SellerMessageFailed => json!({
            "message": 3,
            "why": "the mailbox did not accept its sign-in; connect it again"
        }),
        EventKind::SellerMessageDiscarded => json!({ "message": 3 }),
        EventKind::SellerReplyReceived => json!({
            "reply": 1,
            "message": 3,
            "from": "Dana Reyes <sales@pieboxpros.test>",
            "subject": "Re: Quote for 500 printed pie boxes",
            "attachments": [
                { "name": "quote.pdf", "kept": true, "media_type": "application/pdf", "bytes": 48213 }
            ]
        }),
        EventKind::SellerReplyDismissed => json!({ "reply": 1 }),
        EventKind::MarketingCampaignCreated => json!({
            "plan": "MP-1",
            "key": "search-launch",
            "account": "123-456-7890",
            "campaign": "customers/1234567890/campaigns/11",
            "budget": "customers/1234567890/campaignBudgets/12",
            "budget_kind": "total",
            "amount": "800.00",
        }),
        EventKind::MarketingBudgetReached => json!({
            "plan": "MP-1",
            "scope": "campaign",
            "key": "search-launch",
            "spent": "800.00",
            "budget": "800.00",
            "currency": "USD",
            "paused": ["customers/1234567890/campaigns/11"],
        }),
        EventKind::MarketingCampaignPaused => json!({
            "plan": "MP-1",
            "key": "search-launch",
            "campaign": "customers/1234567890/campaigns/11",
            "why": "plan_ended",
        }),
    }
}

/// A `social_post.` body: Kai's Instagram post for slot `post-1` of MP-1, scheduled in the plan,
/// or what happens to a post.
fn a_social_post_body_wire(kind: EventKind) -> Value {
    let post = || {
        json!({
            "channel": "instagram",
            "buffer_channel": "chan-1",
            "text": "We open on Wednesday.",
            "media": [{ "url": "https://cdn.example.com/open.png", "kind": "image" }],
            "at": "2026-11-04T09:00:00-05:00",
        })
    };
    match kind {
        EventKind::SocialPostScheduled => {
            let mut body = post();
            body["approved_by"] = json!("plan");
            body["plan"] = json!("MP-1");
            body["slot"] = json!("post-1");
            body
        }
        EventKind::SocialPostRequested => post(),
        EventKind::SocialPostSent => json!({ "post": 7, "buffer_post": "buf-1" }),
        EventKind::SocialPostStopped => json!({ "post": 7, "by": "owner" }),
        EventKind::SocialPostMissed => json!({ "post": 7, "why": "not_running" }),
        _ => json!({ "post": 7, "reason": "Buffer did not take it: \u{201c}no\u{201d}" }),
    }
}

/// A `marketing_plan.` body: Kai's two-week plan MP-1 with one campaign and one post, or the
/// owner's decision on it, or its end by the owner.
fn a_marketing_plan_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::MarketingPlanProposed => json!({
            "plan": "MP-1",
            "title": "Spring launch",
            "summary": "Two weeks of posts and one small search campaign.",
            "text": "x".repeat(300),
            "starts_on": "2026-11-02",
            "ends_on": "2026-11-15",
            "currency": "USD",
            "budget": { "total": "2000.00", "google_ads": "1000" },
            "campaigns": [{
                "key": "search-launch",
                "channel": "google_ads",
                "name": "Launch search",
                "goal": "Bring people to the shop",
                "budget": "800.50",
                "starts_on": "2026-11-03",
                "ends_on": "2026-11-14"
            }],
            "posts": [{
                "key": "post-1",
                "channel": "instagram",
                "on": "2026-11-04",
                "topic": "Opening day"
            }],
            "measures": ["New customers who say they found us online"],
            "proposed_by": "kai"
        }),
        EventKind::MarketingPlanApproved => json!({ "plan": "MP-1", "note": "" }),
        EventKind::MarketingPlanReturned => {
            json!({ "plan": "MP-1", "reason": "Start with half the budget." })
        }
        _ => json!({ "plan": "MP-1", "why": "by_owner" }),
    }
}

/// A body that writes a contract's summary: its creation by the human, or a write by
/// `maya-chen`.
fn a_summary_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::TaskCreated {
        json!({ "summary": a_contract_summary_wire(), "created_by": "human" })
    } else {
        json!({ "summary": a_contract_summary_wire(), "written_by": "maya-chen" })
    }
}

/// A record an agent keeps: its notebook, or a decision, by `maya-chen`.
fn a_kept_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::MemoryWritten {
        json!({ "text": "Use pnpm.", "written_by": "maya-chen" })
    } else {
        json!({
            "number": 1,
            "slug": "use-sqlite-for-the-log",
            "title": "Use SQLite for the log",
            "written_by": "maya-chen"
        })
    }
}

/// An escalation raised for a blocker's age, or one that has waited 25 hours on the human.
fn an_escalation_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::EscalationRaised {
        json!({ "reason": "blocker_age", "detail": "blocked_age: no key" })
    } else {
        json!({ "raised_seq": 1, "hours": 25 })
    }
}

/// A Designer's plan, or the Product Manager's decision on it; a stopped preview's reason has the
/// decision's shape.
fn a_design_plan_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::DesignPlanProposed {
        json!({ "plan": "Make the sign-in page calm.\n\nOne button leads; the header stays." })
    } else {
        json!({ "reason": "It keeps to the contract." })
    }
}

/// A design plan or its decision, a design review, a preview's life, or a page check.
fn a_design_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::DesignPlanProposed
        | EventKind::DesignPlanApproved
        | EventKind::DesignPlanReturned
        | EventKind::PreviewStopped => a_design_plan_body_wire(kind),
        EventKind::DesignReviewRecorded => json!({
            "pass": true,
            "reasons": "Both widths read well in both themes.",
            "checks": [{ "width": "phone", "theme": "light", "violations": [] }]
        }),
        EventKind::PreviewPrepared => {
            json!({ "tree": "4b825dc642cb6eb9a060e54bf8d69288fbee4904", "seconds": 212 })
        }
        EventKind::PageChecked => json!({
            "width": "phone",
            "theme": "light",
            "path": "/",
            "violations": [],
            "screenshot": "session-1-phone-light.png"
        }),
        // preview.started
        _ => json!({ "port": 4400 }),
    }
}

/// A drift report, or the project scan's read-back.
fn a_project_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::DriftDetected {
        json!({
            "drift": "contract_without_events",
            "detail": "FRK-1 has a contract file and no events."
        })
    } else {
        json!({
            "read_back": "A Rust workspace with one crate and a check command.",
            "detected_criteria": ["cargo xtask check"]
        })
    }
}

/// A lock or an unlock of a contract, by the human.
fn a_hold_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::ContractLocked {
        json!({ "locked_by": "human" })
    } else {
        json!({ "unlocked_by": "human" })
    }
}

/// The judge's check of a contract's plan, passing its one question.
fn a_judgment_body_wire() -> Value {
    json!({
        "judged_by": "sam-ortiz",
        "answers": [
            { "question": "Does the task fit its budget?", "pass": true, "reason": "One deliverable." }
        ],
        "reason": "One deliverable and a criterion that runs it."
    })
}

/// A review summed up, or a product document written.
fn a_record_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::ReviewRecorded {
        json!({ "reviewer": "dev-b", "criteria_run": 2, "passed": true })
    } else {
        json!({ "path": "prd.md", "written_by": "maya-chen" })
    }
}

/// A body of a question to the human or of the human's own acts: a question, an answer to question
/// 3, an acceptance of a result with its words, a resolution back to `refining`, a message
/// in the channel mentioning `dev-a`, and the human's message in dev-a's chat.
fn a_human_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::QuestionAsked => json!({
            "question": "Should a login page remember the user?",
            "asked_by": "maya-chen"
        }),
        EventKind::QuestionAnswered => {
            json!({ "question_id": 3, "answer": "Yes.", "answered_by": "human" })
        }
        EventKind::HumanAccepted => {
            json!({ "subject": "result", "accepted_by": "human", "message": "Both look right." })
        }
        EventKind::ChatMessagePosted => {
            json!({ "chat": "dev-a", "author": "human", "text": "How is FRK-1?\nNo rush." })
        }
        EventKind::MessagePosted => json!({
            "author": "human",
            "kind": "human",
            "text": "@dev-a how is FRK-1?",
            "mentions": ["dev-a"]
        }),
        _ => json!({ "to": "refining", "message": "Split it by page.", "resolved_by": "human" }),
    }
}

/// An agent's body: `dev-a` paused by the human, or asleep until three that afternoon at its
/// model provider's limit.
fn an_agent_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::AgentUpdated {
        json!({ "agent_id": "dev-a", "status": "paused", "updated_by": "human" })
    } else {
        json!({ "until": "2026-09-17T15:00:00Z", "detail": "Claude AI usage limit reached" })
    }
}

/// A sprint's body: S1 started by the human with 20 dollars, FRK-1 planned into it by the Scrum
/// Master, S1 ended by the governor with nothing left, or S1's retro appended by the Scrum Master.
fn a_sprint_body_wire(kind: EventKind) -> Value {
    match kind {
        EventKind::SprintStarted => {
            json!({ "sprint_id": "S1", "budget_usd": 20.0, "started_by": "human" })
        }
        EventKind::SprintPlanned => {
            json!({ "sprint_id": "S1", "task_ids": ["FRK-1"], "planned_by": "sam-ortiz" })
        }
        EventKind::RetroAppended => {
            json!({ "sprint_id": "S1", "text": "Keep the tasks small.", "appended_by": "sam-ortiz" })
        }
        _ => json!({ "sprint_id": "S1", "ended_by": "governor", "left": [] }),
    }
}

/// An integration body: FRK-1 merged into `main` by the governor, or its pull request 7 opened.
fn an_integration_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::TaskIntegrated {
        json!({ "sha": "4b825dc642cb6eb9a060e54bf8d69288fbee4904", "into": "main", "integrated_by": "governor" })
    } else {
        json!({ "url": "https://github.com/o/r/pull/7", "number": 7, "branch": "catervas/FRK-1" })
    }
}

/// A `session.` body: an implement session on haiku that started, or that completed.
fn a_session_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::SessionStarted {
        json!({ "purpose": "implement", "model": "claude-haiku-4-5-20251001", "effort": "high" })
    } else {
        json!({ "reason": "completed", "detail": "done" })
    }
}

/// A `tool.` body of a `Read`, with its one field of its own.
fn a_tool_body_wire(field: &str, value: &str) -> Value {
    let mut body = json!({ "tool": "Read" });
    body[field] = json!(value);
    body
}

/// A schema-valid contract summary: a task in `draft`, with no parent.
#[must_use]
pub fn a_contract_summary_wire() -> Value {
    json!({ "kind": "task", "title": "Add a login page", "status": "draft", "risk": "low" })
}

/// `dev-a`'s skill `api-style`, added, changed or confirmed with a hash, or removed.
fn a_skill_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::SkillRemoved {
        json!({ "level": "agent", "agent": "dev-a", "name": "api-style" })
    } else {
        json!({ "level": "agent", "agent": "dev-a", "name": "api-style", "sha256": "0".repeat(64) })
    }
}

/// `dev-a`'s custom server `github`, connected or taken away, or a call of it asked about or
/// decided.
fn a_connector_body_wire(kind: EventKind) -> Value {
    if !matches!(
        kind,
        EventKind::ConnectorConnected | EventKind::ConnectorDisconnected
    ) {
        return a_tool_approval_body_wire(kind);
    }
    if kind == EventKind::ConnectorConnected {
        json!({
            "agent": "dev-a",
            "server": "github",
            "transport": "stdio",
            "credential_keys": ["API_KEY"],
            "tools": { "search_issues": "network", "delete_repo": "denied" },
            "spec_sha256": "0".repeat(64)
        })
    } else {
        json!({ "agent": "dev-a", "server": "github" })
    }
}

/// A `tool_approval.` body: `create_issue` of `github` asked about, or approval 1 decided.
fn a_tool_approval_body_wire(kind: EventKind) -> Value {
    if kind == EventKind::ToolApprovalRequested {
        json!({
            "server": "github",
            "tool": "create_issue",
            "input": "{\"title\":\"x\"}",
            "input_sha256": "0".repeat(64)
        })
    } else {
        json!({ "approval": 1 })
    }
}
