//! The marketing plans the log holds (`docs/SPEC.md` 6.5, ADR 0042): each proposal, the owner's
//! decision on it, and its end, folded from the four `marketing_plan.` kinds; and the posts, folded
//! from the six `social_post.` kinds.

use chrono::{DateTime, FixedOffset, Utc};
use farik_core::contract::TaskId;
use farik_core::marketing::{
    EndReason, PlanCampaign, PlanProposal, PlanRecord, PostChannel, PostDetails, PostSlot,
    parse_amount,
};
use farik_protocol::event::{
    EventBody, EventKind, FarikEvent, MarketingPlanEndedBodyWhy, MarketingPlanProposedBody,
    SocialPostMediaKind, SocialPostMissedBodyWhy, SocialPostScheduledBodyApprovedBy,
    SocialPostStoppedBodyBy,
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
    /// What the owner said when they ended it, when they said anything: as they wrote it.
    pub end_note: Option<String>,
    /// The plan that took its place, when a newer plan did.
    pub replaced_by: Option<String>,
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
                    plan.end_note = body.note.clone().filter(|note| !note.trim().is_empty());
                    plan.replaced_by = body
                        .replaced_by
                        .as_ref()
                        .map(|newer| newer.as_str().to_string());
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
        end_note: None,
        replaced_by: None,
    })
}

/// Where a post stands (ADR 0042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostState {
    /// Written outside the plan, waiting for the owner.
    Requested,
    /// Going out: in the plan, or allowed by the owner, and not yet with Buffer.
    Scheduled,
    /// With Buffer, which posts it at its time.
    Sent,
    /// Not going out: the owner stopped it or did not allow it, or its plan ended.
    Stopped,
    /// Not handed over in time.
    Missed,
    /// Buffer did not take it.
    Failed,
}

impl PostState {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Scheduled => "scheduled",
            Self::Sent => "sent",
            Self::Stopped => "stopped",
            Self::Missed => "missed",
            Self::Failed => "failed",
        }
    }
}

/// One picture or clip of a post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostMedia {
    /// The `https` address it is fetched from.
    pub url: String,
    /// Whether it is a clip, not a picture.
    pub video: bool,
}

/// One post as the log tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialPost {
    /// Its number: the sequence number of the event that made it.
    pub post: u64,
    /// The agent that wrote it, whose Buffer connection hands it over.
    pub agent_id: String,
    /// The task it was written in.
    pub task_id: TaskId,
    /// The network.
    pub channel: PostChannel,
    /// Buffer's id for the channel.
    pub buffer_channel: String,
    /// What it says.
    pub text: String,
    /// Its pictures and clips, in order.
    pub media: Vec<PostMedia>,
    /// What `YouTube` or Pinterest need besides the text.
    pub details: Option<PostDetails>,
    /// When it goes out, with the offset it was written in.
    pub at: DateTime<FixedOffset>,
    /// The plan it is in, when it is in one.
    pub plan: Option<String>,
    /// The slot of that plan it fills.
    pub slot: Option<String>,
    /// Who approved it, `plan` or `owner`, once it is scheduled.
    pub approved_by: Option<String>,
    /// When the owner allowed it, when it was a request.
    pub decided_at: Option<DateTime<Utc>>,
    /// Buffer's id for it, once sent.
    pub buffer_post: Option<String>,
    /// Where it stands.
    pub state: PostState,
    /// When it reached that state.
    pub state_at: DateTime<Utc>,
    /// The sequence number of the event that put it in that state.
    pub state_seq: u64,
    /// Who stopped it, `owner`, `declined` or `plan_ended`, once stopped.
    pub stopped_by: Option<String>,
    /// Whether Farik took it back from Buffer when it was stopped.
    pub taken_back: bool,
    /// What the owner said when they did not allow it or stopped it, as they wrote it.
    pub note: Option<String>,
    /// Why it was missed.
    pub missed_why: Option<String>,
    /// Why it failed: Farik's sentence, then Buffer's words.
    pub reason: Option<String>,
}

/// A number the schema says is a positive integer, however the generator holds it.
fn number_of<T: serde::Serialize>(number: &T) -> Option<u64> {
    serde_json::to_value(number).ok()?.as_u64()
}

/// The details of a post, read from the body's own JSON: a `YouTube` video's title and category,
/// or a Pinterest board.
fn details_of<T: serde::Serialize>(details: Option<&T>) -> Option<PostDetails> {
    let value = serde_json::to_value(details).ok()?;
    let text = |name: &str| value.get(name)?.as_str().map(str::to_string);
    match (text("board"), text("title"), text("category_id")) {
        (Some(board), _, _) => Some(PostDetails::Pinterest { board }),
        (None, Some(title), Some(category_id)) => Some(PostDetails::Youtube { title, category_id }),
        _ => None,
    }
}

/// The post a `requested` or an agent's `scheduled` event makes, or `None` for an event whose
/// fields cannot be read.
fn written(
    event: &FarikEvent,
    channel: &impl ToString,
    fields: (&str, &str, &[(String, bool)], &str),
    details: Option<PostDetails>,
    state: PostState,
) -> Option<SocialPost> {
    let (buffer_channel, text, media, at) = fields;
    let ids = &event.envelope.ids;
    Some(SocialPost {
        post: event.envelope.seq,
        agent_id: ids.agent_id.clone()?,
        task_id: ids.task_id.clone()?,
        channel: PostChannel::parse(&channel.to_string())?,
        buffer_channel: buffer_channel.to_string(),
        text: text.to_string(),
        media: media
            .iter()
            .map(|(url, video)| PostMedia {
                url: url.clone(),
                video: *video,
            })
            .collect(),
        details,
        at: DateTime::parse_from_rfc3339(at).ok()?,
        plan: None,
        slot: None,
        approved_by: None,
        decided_at: None,
        buffer_post: None,
        state,
        state_at: event.envelope.recorded_at,
        state_seq: event.envelope.seq,
        stopped_by: None,
        taken_back: false,
        note: None,
        missed_why: None,
        reason: None,
    })
}

/// A body's media as the pairs `written` takes.
fn media_pairs(media: &[farik_protocol::event::SocialPostMedia]) -> Vec<(String, bool)> {
    media
        .iter()
        .map(|item| {
            (
                item.url.as_str().to_string(),
                item.kind == SocialPostMediaKind::Video,
            )
        })
        .collect()
}

/// Every post the log holds, oldest first. An agent's session makes a post (`requested`, or
/// `scheduled` in the plan) and nothing else about it: the owner's allowance, stop and decline, and
/// Farik's hand-over, count only when the envelope names no agent and no session, as for a plan's
/// decision. A post goes through its states once: what happens to it counts only from the state it
/// may happen in, so the first outcome stands.
///
/// # Errors
///
/// What the log refused.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per kind of event, side by side"
)]
pub fn social_posts(log: &EventLog) -> Result<Vec<SocialPost>, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![
            EventKind::SocialPostScheduled,
            EventKind::SocialPostRequested,
            EventKind::SocialPostSent,
            EventKind::SocialPostStopped,
            EventKind::SocialPostMissed,
            EventKind::SocialPostFailed,
        ],
        ..EventQuery::default()
    })?;
    let mut posts: Vec<SocialPost> = Vec::new();
    for event in &events {
        let ids = &event.envelope.ids;
        let at = event.envelope.recorded_at;
        let seq = event.envelope.seq;
        // Only the owner and Farik decide, hand over and stop: an event an agent's session
        // recorded is none of those.
        let is_not_an_agents = ids.agent_id.is_none() && ids.session_id.is_none();
        let mut moves = |post: u64,
                         from: &[PostState],
                         to: PostState,
                         change: &mut dyn FnMut(&mut SocialPost)| {
            if let Some(found) = posts
                .iter_mut()
                .find(|found| found.post == post && from.contains(&found.state))
            {
                found.state = to;
                found.state_at = at;
                found.state_seq = seq;
                change(found);
            }
        };
        match &event.body {
            EventBody::SocialPostRequested(body) => {
                let media = media_pairs(&body.media);
                if let Some(post) = written(
                    event,
                    &body.channel,
                    (
                        body.buffer_channel.as_str(),
                        &body.text,
                        &media,
                        body.at.as_str(),
                    ),
                    details_of(body.details.as_ref()),
                    PostState::Requested,
                ) {
                    posts.push(post);
                }
            }
            EventBody::SocialPostScheduled(body) => {
                let owners = body.approved_by == SocialPostScheduledBodyApprovedBy::Owner;
                match (number_of(&body.post), owners) {
                    // The owner allows a request: its own words, at the owner's time.
                    (Some(request), true) if is_not_an_agents => {
                        moves(
                            request,
                            &[PostState::Requested],
                            PostState::Scheduled,
                            &mut |post| {
                                post.approved_by = Some("owner".to_string());
                                post.decided_at = Some(at);
                            },
                        );
                    }
                    // The agent schedules a post in the plan.
                    (None, false) => {
                        let media = media_pairs(&body.media);
                        if let Some(mut post) = written(
                            event,
                            &body.channel,
                            (
                                body.buffer_channel.as_str(),
                                &body.text,
                                &media,
                                body.at.as_str(),
                            ),
                            details_of(body.details.as_ref()),
                            PostState::Scheduled,
                        ) {
                            post.plan = body.plan.as_ref().map(|plan| plan.as_str().to_string());
                            post.slot = body.slot.as_ref().map(|slot| slot.as_str().to_string());
                            post.approved_by = Some("plan".to_string());
                            posts.push(post);
                        }
                    }
                    _ => {}
                }
            }
            EventBody::SocialPostSent(body) if is_not_an_agents => {
                if let Some(post) = number_of(&body.post) {
                    moves(
                        post,
                        &[PostState::Scheduled],
                        PostState::Sent,
                        &mut |found| {
                            found.buffer_post = Some(body.buffer_post.as_str().to_string());
                        },
                    );
                }
            }
            EventBody::SocialPostStopped(body) if is_not_an_agents => {
                let from: &[PostState] = match body.by {
                    SocialPostStoppedBodyBy::Declined => &[PostState::Requested],
                    SocialPostStoppedBodyBy::Owner => &[PostState::Scheduled, PostState::Sent],
                    SocialPostStoppedBodyBy::PlanEnded => &[PostState::Scheduled],
                };
                let by = match body.by {
                    SocialPostStoppedBodyBy::Declined => "declined",
                    SocialPostStoppedBodyBy::Owner => "owner",
                    SocialPostStoppedBodyBy::PlanEnded => "plan_ended",
                };
                if let Some(post) = number_of(&body.post) {
                    moves(post, from, PostState::Stopped, &mut |found| {
                        found.stopped_by = Some(by.to_string());
                        found.taken_back = body.taken_back == Some(true);
                        found.note = body.note.clone().filter(|note| !note.trim().is_empty());
                    });
                }
            }
            EventBody::SocialPostMissed(body) if is_not_an_agents => {
                let (from, why): (&[PostState], &str) = match body.why {
                    SocialPostMissedBodyWhy::Undecided => (&[PostState::Requested], "undecided"),
                    SocialPostMissedBodyWhy::NotRunning => (&[PostState::Scheduled], "not_running"),
                    SocialPostMissedBodyWhy::Paused => (&[PostState::Scheduled], "paused"),
                };
                if let Some(post) = number_of(&body.post) {
                    moves(post, from, PostState::Missed, &mut |found| {
                        found.missed_why = Some(why.to_string());
                    });
                }
            }
            EventBody::SocialPostFailed(body) if is_not_an_agents => {
                if let Some(post) = number_of(&body.post) {
                    moves(
                        post,
                        &[PostState::Scheduled],
                        PostState::Failed,
                        &mut |found| {
                            found.reason = Some(body.reason.as_str().to_string());
                        },
                    );
                }
            }
            _ => {}
        }
    }
    Ok(posts)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::{DateTime, TimeZone, Utc};
    use farik_core::marketing::{Amount, EndReason, PostChannel, PostDetails};
    use farik_protocol::event::fixtures::an_event_wire;
    use farik_protocol::event::{EventKind, NewEvent, event_from_value};
    use serde_json::{Value, json};

    use super::{PostState, marketing_plans, social_posts};
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
            wire["body"] = json!({
                "plan": "MP-3", "why": "by_owner", "note": "We close early for the refit."
            });
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
        assert_eq!(
            third.end_note.as_deref(),
            Some("We close early for the refit."),
            "the owner's words when they ended it"
        );
        assert_eq!(third.replaced_by, None);
        assert_eq!(first.end_note, None);
        assert_eq!(first.replaced_by, None);

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
            wire["body"] = json!({ "plan": "MP-2", "why": "expired", "note": "An agent's." });
            wire["agent_id"] = json!("kai");
        });
        let ignored = &marketing_plans(&log).expect("folds")[1];
        assert_eq!(ignored.record.ended, None);
        assert_eq!(ignored.end_note, None, "an agent's note is no one's end");
        append(&log, EventKind::MarketingPlanEnded, 17, |wire| {
            wire["body"] = json!({ "plan": "MP-2", "why": "replaced", "replaced_by": "MP-3" });
        });
        append(&log, EventKind::MarketingPlanEnded, 18, |wire| {
            wire["body"] = json!({ "plan": "MP-2", "why": "by_owner", "note": "Too late." });
        });
        let second = &marketing_plans(&log).expect("folds")[1];
        assert_eq!(second.record.ended, Some(EndReason::Replaced));
        assert_eq!(second.ended_at, Some(at(17)));
        assert_eq!(second.replaced_by.as_deref(), Some("MP-3"));
        assert_eq!(second.end_note, None, "the second end's note is not kept");
    }

    /// An agent's post event of `kind`: Kai's, in her session, on FRK-1, with `change` applied to
    /// its body. Answers the post's number.
    fn kai_wrote(
        log: &EventLog,
        kind: EventKind,
        hour: u32,
        change: impl FnOnce(&mut Value),
    ) -> u64 {
        append(log, kind, hour, |wire| {
            wire["agent_id"] = json!("kai");
            wire["session_id"] = json!("session-1");
            change(&mut wire["body"]);
        })
    }

    /// What happens to post `post`: Farik's or the owner's event, with no agent and no session.
    fn then(log: &EventLog, kind: EventKind, hour: u32, body: Value) -> u64 {
        append(log, kind, hour, |wire| wire["body"] = body)
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "a post in every state, folded and then read whole"
    )]
    fn the_store_folds_the_posts() {
        let log = a_log();
        // Scheduled in the plan, handed to Buffer.
        let sent = kai_wrote(&log, EventKind::SocialPostScheduled, 9, |_| {});
        let sent_seq = then(
            &log,
            EventKind::SocialPostSent,
            10,
            json!({ "post": sent, "buffer_post": "buf-1" }),
        );
        // Scheduled and stopped by the owner.
        let stopped = kai_wrote(&log, EventKind::SocialPostScheduled, 9, |body| {
            body["slot"] = json!("post-2");
        });
        then(
            &log,
            EventKind::SocialPostStopped,
            10,
            json!({ "post": stopped, "by": "owner" }),
        );
        // Requested, then allowed by the owner: the owner's event names the request's task and no
        // agent.
        let allowed = kai_wrote(&log, EventKind::SocialPostRequested, 9, |body| {
            body["text"] = json!("A post outside the plan");
        });
        append(&log, EventKind::SocialPostScheduled, 11, |wire| {
            wire["body"]["post"] = json!(allowed);
            wire["body"]["approved_by"] = json!("owner");
            wire["body"]
                .as_object_mut()
                .expect("an object")
                .remove("plan");
            wire["body"]
                .as_object_mut()
                .expect("an object")
                .remove("slot");
        });
        // Requested, then declined with a note.
        let declined = kai_wrote(&log, EventKind::SocialPostRequested, 9, |_| {});
        then(
            &log,
            EventKind::SocialPostStopped,
            12,
            json!({ "post": declined, "by": "declined", "note": "Not this week" }),
        );
        // Requested and never decided; scheduled and not sent in time; scheduled and refused.
        let undecided = kai_wrote(&log, EventKind::SocialPostRequested, 9, |_| {});
        then(
            &log,
            EventKind::SocialPostMissed,
            13,
            json!({ "post": undecided, "why": "undecided" }),
        );
        let missed = kai_wrote(&log, EventKind::SocialPostScheduled, 9, |_| {});
        then(
            &log,
            EventKind::SocialPostMissed,
            13,
            json!({ "post": missed, "why": "paused" }),
        );
        let failed = kai_wrote(&log, EventKind::SocialPostScheduled, 9, |_| {});
        then(
            &log,
            EventKind::SocialPostFailed,
            14,
            json!({ "post": failed, "reason": "Buffer said no" }),
        );
        // A YouTube post keeps its details, and a sent post the owner took back says so.
        let youtube = kai_wrote(&log, EventKind::SocialPostRequested, 9, |body| {
            body["channel"] = json!("youtube");
            body["details"] = json!({ "title": "Opening day", "category_id": "22" });
        });
        let taken = kai_wrote(&log, EventKind::SocialPostScheduled, 9, |_| {});
        then(
            &log,
            EventKind::SocialPostSent,
            10,
            json!({ "post": taken, "buffer_post": "buf-2" }),
        );
        then(
            &log,
            EventKind::SocialPostStopped,
            15,
            json!({ "post": taken, "by": "owner", "taken_back": true }),
        );

        let posts = social_posts(&log).expect("the posts fold");

        let numbers: Vec<u64> = posts.iter().map(|post| post.post).collect();
        assert_eq!(
            numbers,
            [
                sent, stopped, allowed, declined, undecided, missed, failed, youtube, taken
            ],
            "oldest first, each numbered by the event that made it"
        );
        let states: Vec<PostState> = posts.iter().map(|post| post.state).collect();
        assert_eq!(
            states,
            [
                PostState::Sent,
                PostState::Stopped,
                PostState::Scheduled,
                PostState::Stopped,
                PostState::Missed,
                PostState::Missed,
                PostState::Failed,
                PostState::Requested,
                PostState::Stopped,
            ]
        );
        let first = &posts[0];
        assert_eq!(first.agent_id, "kai");
        assert_eq!(first.task_id.as_str(), "FRK-1");
        assert_eq!(first.channel, PostChannel::Instagram);
        assert_eq!(first.buffer_channel, "chan-1");
        assert_eq!(first.text, "We open on Wednesday.");
        assert_eq!(first.media.len(), 1);
        assert_eq!(first.media[0].url, "https://cdn.example.com/open.png");
        assert!(!first.media[0].video);
        assert_eq!(first.at.to_rfc3339(), "2026-11-04T09:00:00-05:00");
        assert_eq!(first.plan.as_deref(), Some("MP-1"));
        assert_eq!(first.slot.as_deref(), Some("post-1"));
        assert_eq!(first.approved_by.as_deref(), Some("plan"));
        assert_eq!(first.buffer_post.as_deref(), Some("buf-1"));
        assert_eq!(first.state_at, at(10));
        assert_eq!(first.state_seq, sent_seq, "the event that sent it");
        assert_eq!(
            posts[7].state_seq, posts[7].post,
            "a request is in its state from its own event"
        );
        assert_eq!(first.details, None);

        let stopped = &posts[1];
        assert_eq!(stopped.stopped_by.as_deref(), Some("owner"));
        assert!(!stopped.taken_back);
        assert_eq!(stopped.state_at, at(10));

        // Allowed: the request's own words, now scheduled by the owner at the owner's time.
        let allowed = &posts[2];
        assert_eq!(allowed.text, "A post outside the plan");
        assert_eq!(allowed.approved_by.as_deref(), Some("owner"));
        assert_eq!(allowed.decided_at, Some(at(11)));
        assert_eq!(
            (allowed.plan.as_deref(), allowed.slot.as_deref()),
            (None, None)
        );
        assert_eq!(allowed.agent_id, "kai", "whose post it is");

        let declined = &posts[3];
        assert_eq!(declined.stopped_by.as_deref(), Some("declined"));
        assert_eq!(declined.note.as_deref(), Some("Not this week"));
        assert_eq!(posts[4].missed_why.as_deref(), Some("undecided"));
        assert_eq!(posts[5].missed_why.as_deref(), Some("paused"));
        assert_eq!(posts[6].reason.as_deref(), Some("Buffer said no"));
        assert_eq!(
            posts[7].details,
            Some(PostDetails::Youtube {
                title: "Opening day".to_string(),
                category_id: "22".to_string()
            })
        );
        assert_eq!(posts[7].approved_by, None, "a request has no approval");
        let taken = &posts[8];
        assert!(taken.taken_back);
        assert_eq!(taken.buffer_post.as_deref(), Some("buf-2"));
        assert_eq!(taken.state_at, at(15));
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "each forgery and each out-of-order event, side by side"
    )]
    fn counts_only_what_a_post_may_go_through() {
        let log = a_log();
        let request = kai_wrote(&log, EventKind::SocialPostRequested, 9, |_| {});
        let scheduled = kai_wrote(&log, EventKind::SocialPostScheduled, 9, |_| {});
        // An owner's allowance or stop with an agent or a session on its envelope is no one's.
        for (key, value) in [("agent_id", "kai"), ("session_id", "session-9")] {
            append(&log, EventKind::SocialPostScheduled, 10, |wire| {
                wire["body"]["post"] = json!(request);
                wire["body"]["approved_by"] = json!("owner");
                wire[key] = json!(value);
            });
            append(&log, EventKind::SocialPostStopped, 10, |wire| {
                wire["body"] = json!({ "post": scheduled, "by": "owner" });
                wire[key] = json!(value);
            });
            append(&log, EventKind::SocialPostStopped, 10, |wire| {
                wire["body"] = json!({ "post": request, "by": "declined" });
                wire[key] = json!(value);
            });
        }
        // An agent's own scheduled event cannot name a request, or call itself the owner's.
        kai_wrote(&log, EventKind::SocialPostScheduled, 10, |body| {
            body["post"] = json!(request);
            body["approved_by"] = json!("owner");
        });
        kai_wrote(&log, EventKind::SocialPostScheduled, 10, |body| {
            body["approved_by"] = json!("owner");
        });
        let posts = social_posts(&log).expect("the posts fold");
        assert_eq!(posts.len(), 2, "the agent's forgeries made no post");
        assert_eq!(posts[0].state, PostState::Requested);
        assert_eq!(posts[1].state, PostState::Scheduled);

        // The first thing that happens to a post is what happens: a stop after a failure, a sent
        // after a stop, an event for a post nobody made, and a decline of a scheduled post change
        // nothing.
        then(
            &log,
            EventKind::SocialPostFailed,
            11,
            json!({ "post": scheduled, "reason": "No" }),
        );
        then(
            &log,
            EventKind::SocialPostStopped,
            12,
            json!({ "post": scheduled, "by": "owner" }),
        );
        then(
            &log,
            EventKind::SocialPostSent,
            12,
            json!({ "post": scheduled, "buffer_post": "b" }),
        );
        then(
            &log,
            EventKind::SocialPostSent,
            12,
            json!({ "post": 999, "buffer_post": "b" }),
        );
        then(
            &log,
            EventKind::SocialPostSent,
            12,
            json!({ "post": request, "buffer_post": "b" }),
        );
        let posts = social_posts(&log).expect("the posts fold");
        assert_eq!(posts[1].state, PostState::Failed);
        assert_eq!(posts[1].state_at, at(11));
        assert_eq!(
            posts[0].state,
            PostState::Requested,
            "a request is not sent"
        );
        then(
            &log,
            EventKind::SocialPostStopped,
            13,
            json!({ "post": request, "by": "owner" }),
        );
        assert_eq!(
            social_posts(&log).expect("the posts fold")[0].state,
            PostState::Requested,
            "a request is declined, not stopped"
        );
        // And a post that is going out is stopped, not declined; Farik's plan ending is no stop of
        // a request.
        let going = kai_wrote(&log, EventKind::SocialPostScheduled, 14, |_| {});
        then(
            &log,
            EventKind::SocialPostStopped,
            14,
            json!({ "post": going, "by": "declined" }),
        );
        then(
            &log,
            EventKind::SocialPostStopped,
            14,
            json!({ "post": request, "by": "plan_ended" }),
        );
        let posts = social_posts(&log).expect("the posts fold");
        assert_eq!(
            posts[2].state,
            PostState::Scheduled,
            "a scheduled post is not declined"
        );
        assert_eq!(
            posts[0].state,
            PostState::Requested,
            "a request is not ended with a plan"
        );
    }
}
