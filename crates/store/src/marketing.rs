//! The marketing plans the log holds (`docs/SPEC.md` 6.5, ADR 0042): each proposal, the owner's
//! decision on it, and its end, folded from the four `marketing_plan.` kinds.

use chrono::{DateTime, Utc};
use farik_core::contract::TaskId;
use farik_core::marketing::{
    EndReason, PlanCampaign, PlanProposal, PlanRecord, PostChannel, PostSlot, parse_amount,
};
use farik_protocol::event::{
    EventBody, EventKind, FarikEvent, MarketingPlanEndedBodyWhy, MarketingPlanProposedBody,
};

use crate::{EventLog, EventQuery, StoreError};

/// One marketing plan as the log tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingPlan {
    /// What decides whether the plan is active: its id, dates and standing.
    pub record: PlanRecord,
    /// What was proposed.
    pub proposal: PlanProposal,
    /// The agent that proposed it.
    pub agent_id: String,
    /// The task the proposal waited on the owner for.
    pub task_id: TaskId,
    /// When it was proposed.
    pub proposed_at: DateTime<Utc>,
    /// The owner's decision: whether they approved it, what they said (the note, or for a
    /// plan sent back the reason; none when they said nothing), and when.
    pub decided: Option<(bool, Option<String>, DateTime<Utc>)>,
    /// When it ended, when it did.
    pub ended_at: Option<DateTime<Utc>>,
}

/// Where a plan stands (ADR 0042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    /// Proposed and waiting for the owner.
    Proposed,
    /// Sent back by the owner.
    Returned,
    /// Approved and not the active plan: not started yet, or another plan is active.
    Approved,
    /// Approved, and the plan in force today.
    Active,
    /// Ended.
    Ended,
}

impl PlanState {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Returned => "returned",
            Self::Approved => "approved",
            Self::Active => "active",
            Self::Ended => "ended",
        }
    }
}

impl MarketingPlan {
    /// Where this plan stands, given the id of the plan that is active today, when one is
    /// (`farik_core::marketing::active_plan`).
    #[must_use]
    pub fn state(&self, active: Option<&str>) -> PlanState {
        if self.record.ended.is_some() {
            PlanState::Ended
        } else if self.record.returned {
            PlanState::Returned
        } else if self.record.approved_seq.is_none() {
            PlanState::Proposed
        } else if active == Some(self.record.id.as_str()) {
            PlanState::Active
        } else {
            PlanState::Approved
        }
    }
}

/// Every marketing plan the log holds, oldest first. A decision or an end counts only when its
/// envelope names no agent and no session, since only the owner and Farik decide or end a plan;
/// the first decision on a plan is the only one, and so is the first end.
///
/// # Errors
///
/// What the log refused, or `InvalidEvent` for a plan event whose figures cannot be read.
pub fn marketing_plans(log: &EventLog) -> Result<Vec<MarketingPlan>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::MarketingPlanProposed,
            EventKind::MarketingPlanApproved,
            EventKind::MarketingPlanReturned,
            EventKind::MarketingPlanEnded,
        ],
        ..EventQuery::default()
    })?;
    let mut plans: Vec<MarketingPlan> = Vec::new();
    for event in &events {
        let ids = &event.envelope.ids;
        let at = event.envelope.recorded_at;
        let seq = event.envelope.seq;
        // Only the owner decides and only the owner or Farik ends a plan: an event an agent's
        // session recorded is no decision.
        let is_not_an_agents = ids.agent_id.is_none() && ids.session_id.is_none();
        match &event.body {
            EventBody::MarketingPlanProposed(body) => {
                if !plans
                    .iter()
                    .any(|plan| plan.record.id == body.plan.as_str())
                {
                    plans.push(proposed(event, body)?);
                }
            }
            EventBody::MarketingPlanApproved(body) if is_not_an_agents => {
                if let Some(plan) = undecided(&mut plans, body.plan.as_str()) {
                    plan.record.approved_seq = Some(seq);
                    let note = Some(body.note.clone()).filter(|note| !note.is_empty());
                    plan.decided = Some((true, note, at));
                }
            }
            EventBody::MarketingPlanReturned(body) if is_not_an_agents => {
                if let Some(plan) = undecided(&mut plans, body.plan.as_str()) {
                    plan.record.returned = true;
                    plan.decided = Some((false, Some(body.reason.clone()), at));
                }
            }
            EventBody::MarketingPlanEnded(body) if is_not_an_agents => {
                if let Some(plan) = plans
                    .iter_mut()
                    .find(|plan| plan.record.id == body.plan.as_str())
                    .filter(|plan| plan.record.ended.is_none())
                {
                    plan.record.ended = Some(match body.why {
                        MarketingPlanEndedBodyWhy::Replaced => EndReason::Replaced,
                        MarketingPlanEndedBodyWhy::ByOwner => EndReason::ByOwner,
                        MarketingPlanEndedBodyWhy::Expired => EndReason::Expired,
                    });
                    plan.ended_at = Some(at);
                }
            }
            _ => {}
        }
    }
    Ok(plans)
}

/// The plan `id` when it has no decision yet.
fn undecided<'a>(plans: &'a mut [MarketingPlan], id: &str) -> Option<&'a mut MarketingPlan> {
    plans
        .iter_mut()
        .find(|plan| plan.record.id == id)
        .filter(|plan| plan.decided.is_none())
}

fn proposed(
    event: &FarikEvent,
    body: &MarketingPlanProposedBody,
) -> Result<MarketingPlan, StoreError> {
    let seq = event.envelope.seq;
    let unreadable = |what: &str| StoreError::InvalidEvent {
        detail: format!("event {seq} proposes a marketing plan with {what} that cannot be read"),
    };
    let amount = |text: &str| parse_amount(text).ok_or_else(|| unreadable("an amount"));
    let campaigns = body
        .campaigns
        .iter()
        .map(|campaign| {
            Ok(PlanCampaign {
                key: campaign.key.as_str().to_string(),
                name: campaign.name.clone(),
                goal: campaign.goal.clone(),
                budget: amount(campaign.budget.as_str())?,
                starts_on: campaign.starts_on,
                ends_on: campaign.ends_on,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let posts = body
        .posts
        .iter()
        .map(|post| {
            Ok(PostSlot {
                key: post.key.as_str().to_string(),
                channel: PostChannel::parse(&post.channel.to_string())
                    .ok_or_else(|| unreadable("a channel"))?,
                on: post.on,
                topic: post.topic.clone(),
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let task_id = event
        .envelope
        .ids
        .task_id
        .clone()
        .ok_or_else(|| unreadable("no task"))?;
    Ok(MarketingPlan {
        record: PlanRecord {
            id: body.plan.as_str().to_string(),
            starts_on: body.starts_on,
            ends_on: body.ends_on,
            approved_seq: None,
            returned: false,
            ended: None,
        },
        proposal: PlanProposal {
            title: body.title.clone(),
            summary: body.summary.clone(),
            text: body.text.clone(),
            starts_on: body.starts_on,
            ends_on: body.ends_on,
            currency: body.currency.as_str().to_string(),
            total: amount(body.budget.total.as_str())?,
            google_ads: amount(body.budget.google_ads.as_str())?,
            campaigns,
            posts,
            measures: body.measures.clone(),
            google_ads_account: body
                .google_ads_account
                .as_ref()
                .map(|account| account.as_str().to_string()),
            replaces: body.replaces.as_ref().map(|plan| plan.as_str().to_string()),
        },
        agent_id: body.proposed_by.clone(),
        task_id,
        proposed_at: event.envelope.recorded_at,
        decided: None,
        ended_at: None,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::{DateTime, TimeZone, Utc};
    use farik_core::marketing::{Amount, EndReason, PostChannel};
    use farik_protocol::event::fixtures::an_event_wire;
    use farik_protocol::event::{EventKind, NewEvent, event_from_value};
    use serde_json::{Value, json};

    use super::marketing_plans;
    use crate::{EventLog, IN_MEMORY, open_event_log};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, hour, 0, 0)
            .single()
            .expect("a real hour")
    }

    fn a_log() -> EventLog {
        open_event_log(Path::new(IN_MEMORY), at(8)).expect("a log in memory opens")
    }

    /// Appends the fixture event of `kind` about `task` with `change` applied to its wire.
    fn append(log: &EventLog, kind: EventKind, hour: u32, change: impl FnOnce(&mut Value)) -> u64 {
        let mut wire = an_event_wire(kind);
        wire["recorded_at"] = json!(at(hour).to_rfc3339());
        change(&mut wire);
        let event = event_from_value(&wire).expect("the fixture is schema-valid");
        log.append(&NewEvent {
            recorded_at: event.envelope.recorded_at,
            ids: event.envelope.ids,
            body: event.body,
        })
        .expect("appends")
        .envelope
        .seq
    }

    fn proposed(log: &EventLog, plan: &str, hour: u32) -> u64 {
        append(log, EventKind::MarketingPlanProposed, hour, |wire| {
            wire["body"]["plan"] = json!(plan);
            wire["agent_id"] = json!("kai");
            wire["session_id"] = json!("session-1");
        })
    }

    fn decided(log: &EventLog, kind: EventKind, plan: &str, hour: u32, body: Value) -> u64 {
        append(log, kind, hour, |wire| {
            wire["body"] = body;
            wire["body"]["plan"] = json!(plan);
        })
    }

    #[test]
    fn the_store_folds_the_plans() {
        let log = a_log();
        proposed(&log, "MP-1", 9);
        let approved = decided(
            &log,
            EventKind::MarketingPlanApproved,
            "MP-1",
            10,
            json!({ "note": "Start small" }),
        );
        proposed(&log, "MP-2", 11);
        decided(
            &log,
            EventKind::MarketingPlanReturned,
            "MP-2",
            12,
            json!({ "reason": "Halve the budget." }),
        );
        proposed(&log, "MP-3", 13);
        decided(
            &log,
            EventKind::MarketingPlanApproved,
            "MP-3",
            14,
            json!({ "note": "" }),
        );
        append(&log, EventKind::MarketingPlanEnded, 15, |wire| {
            wire["body"] = json!({ "plan": "MP-3", "why": "by_owner" });
        });
        proposed(&log, "MP-4", 16);

        let plans = marketing_plans(&log).expect("the plans fold");
        let ids: Vec<&str> = plans.iter().map(|plan| plan.record.id.as_str()).collect();
        assert_eq!(ids, ["MP-1", "MP-2", "MP-3", "MP-4"], "oldest first");

        let first = &plans[0];
        assert_eq!(first.record.approved_seq, Some(approved));
        assert!(!first.record.returned);
        assert_eq!(first.record.ended, None);
        assert_eq!(
            first.decided,
            Some((true, Some("Start small".to_string()), at(10)))
        );
        assert_eq!(first.agent_id, "kai");
        assert_eq!(first.task_id.as_str(), "FRK-1");
        assert_eq!(first.proposed_at, at(9));
        assert_eq!(first.ended_at, None);
        // Everything the proposal said, with its figures read.
        let proposal = &first.proposal;
        assert_eq!(proposal.title, "Spring launch");
        assert_eq!(proposal.total, Amount(200_000));
        assert_eq!(proposal.google_ads, Amount(100_000));
        assert_eq!(proposal.currency, "USD");
        assert_eq!(proposal.starts_on.to_string(), "2026-11-02");
        assert_eq!(proposal.ends_on.to_string(), "2026-11-15");
        assert_eq!(proposal.campaigns.len(), 1);
        assert_eq!(proposal.campaigns[0].key, "search-launch");
        assert_eq!(proposal.campaigns[0].budget, Amount(80_050));
        assert_eq!(proposal.posts.len(), 1);
        assert_eq!(proposal.posts[0].channel, PostChannel::Instagram);
        assert_eq!(proposal.posts[0].on.to_string(), "2026-11-04");
        assert_eq!(proposal.measures.len(), 1);
        assert_eq!(proposal.google_ads_account, None);
        assert_eq!(proposal.replaces, None);

        let second = &plans[1];
        assert!(second.record.returned);
        assert_eq!(second.record.approved_seq, None);
        assert_eq!(
            second.decided,
            Some((false, Some("Halve the budget.".to_string()), at(12)))
        );

        let third = &plans[2];
        assert_eq!(third.decided, Some((true, None, at(14))), "no note, none");
        assert_eq!(third.record.ended, Some(EndReason::ByOwner));
        assert_eq!(third.ended_at, Some(at(15)));

        let waiting = &plans[3];
        assert_eq!(waiting.decided, None);
        assert_eq!(waiting.record.approved_seq, None);
        assert!(!waiting.record.returned);
    }

    #[test]
    fn says_where_each_plan_stands() {
        let log = a_log();
        for (plan, hour) in [
            ("MP-1", 9),
            ("MP-2", 10),
            ("MP-3", 11),
            ("MP-4", 12),
            ("MP-5", 13),
        ] {
            proposed(&log, plan, hour);
        }
        decided(
            &log,
            EventKind::MarketingPlanReturned,
            "MP-1",
            14,
            json!({ "reason": "No." }),
        );
        for plan in ["MP-2", "MP-3", "MP-4"] {
            decided(
                &log,
                EventKind::MarketingPlanApproved,
                plan,
                15,
                json!({ "note": "" }),
            );
        }
        append(&log, EventKind::MarketingPlanEnded, 16, |wire| {
            wire["body"] = json!({ "plan": "MP-4", "why": "by_owner" });
        });
        let plans = marketing_plans(&log).expect("folds");

        let states: Vec<&str> = plans
            .iter()
            .map(|plan| plan.state(Some("MP-2")).as_str())
            .collect();

        assert_eq!(
            states,
            ["returned", "active", "approved", "ended", "proposed"]
        );
        let none_active: Vec<&str> = plans.iter().map(|plan| plan.state(None).as_str()).collect();
        assert_eq!(
            none_active,
            ["returned", "approved", "approved", "ended", "proposed"]
        );
    }

    #[test]
    fn counts_only_the_owners_first_decision_and_farik_s_first_end() {
        let log = a_log();
        proposed(&log, "MP-1", 9);
        // A decision with an agent or a session on its envelope was not the owner's.
        for (key, value) in [("agent_id", "kai"), ("session_id", "session-9")] {
            append(&log, EventKind::MarketingPlanApproved, 10, |wire| {
                wire["body"]["plan"] = json!("MP-1");
                wire[key] = json!(value);
            });
        }
        assert_eq!(
            marketing_plans(&log).expect("folds")[0].decided,
            None,
            "an agent's approval decides nothing"
        );
        // The first real decision stands; a later one, an unknown plan's, and a second end do not.
        decided(
            &log,
            EventKind::MarketingPlanReturned,
            "MP-1",
            11,
            json!({ "reason": "No." }),
        );
        decided(
            &log,
            EventKind::MarketingPlanApproved,
            "MP-1",
            12,
            json!({ "note": "Yes." }),
        );
        decided(
            &log,
            EventKind::MarketingPlanApproved,
            "MP-9",
            13,
            json!({ "note": "" }),
        );
        let plans = marketing_plans(&log).expect("folds");
        assert_eq!(plans.len(), 1);
        assert!(plans[0].record.returned);
        assert_eq!(plans[0].record.approved_seq, None);
        assert_eq!(
            plans[0].decided,
            Some((false, Some("No.".to_string()), at(11)))
        );

        // An end from an agent or a session is ignored, the first end stands.
        proposed(&log, "MP-2", 14);
        decided(
            &log,
            EventKind::MarketingPlanApproved,
            "MP-2",
            15,
            json!({ "note": "" }),
        );
        append(&log, EventKind::MarketingPlanEnded, 16, |wire| {
            wire["body"] = json!({ "plan": "MP-2", "why": "expired" });
            wire["agent_id"] = json!("kai");
        });
        assert_eq!(marketing_plans(&log).expect("folds")[1].record.ended, None);
        append(&log, EventKind::MarketingPlanEnded, 17, |wire| {
            wire["body"] = json!({ "plan": "MP-2", "why": "replaced", "replaced_by": "MP-3" });
        });
        append(&log, EventKind::MarketingPlanEnded, 18, |wire| {
            wire["body"] = json!({ "plan": "MP-2", "why": "by_owner" });
        });
        let second = &marketing_plans(&log).expect("folds")[1];
        assert_eq!(second.record.ended, Some(EndReason::Replaced));
        assert_eq!(second.ended_at, Some(at(17)));
    }
}
