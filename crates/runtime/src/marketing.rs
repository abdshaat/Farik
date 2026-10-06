//! The owner's decisions on marketing plans (`docs/SPEC.md` 6.5, ADR 0042): approving or sending
//! one back, ending an approved one, and the ends that dates bring. The check that a plan may be
//! decided and the write of the decision are one step, as for a connector call's approval: the
//! lock `PLANS` is held by a decision, by the owner's end and by the dated ends alike, so that two
//! of them never act on one reading of the plans.

use std::collections::BTreeSet;
use std::fmt::Display;
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use farik_core::marketing::{EndReason, PlanRecord, PostDetails, plans_to_end};
use farik_protocol::event::{EventBody, EventIds, FarikEvent, new_event};
use farik_store::marketing::{
    MarketingPlan, PlanState, PostState, SocialPost, marketing_plans, social_posts,
};
use serde_json::{Map, Value, json};

use crate::daemon::own_calls::call_as;
use crate::orchestrator::hand_over::{hand_over_time, is_too_late};
use crate::orchestrator::{CommandError, CommandReport, OrchestratorDeps};
use crate::tools::ToolDeps;

/// Held by whoever decides a plan or ends one. One lock for every plan: decisions are rare.
static PLANS: Mutex<()> = Mutex::new(());

/// The posts Farik is handing to Buffer this moment, each as the project's log (by where it is in
/// memory: one process may hold several projects, as a test run does) and the post's number. A post
/// in it is claimed: a stop of it is refused, and the end of its plan leaves it alone. Taken only
/// while `PLANS` is held.
static HANDING: Mutex<BTreeSet<(usize, u64)>> = Mutex::new(BTreeSet::new());

/// Which project `tools` is, as `HANDING` tells projects apart.
fn project_of(tools: &ToolDeps) -> usize {
    std::sync::Arc::as_ptr(&tools.log) as usize
}

/// A post claimed for the hand-over, which stops being claimed when this is dropped, whatever
/// happened to it.
pub(crate) struct Claimed(usize, u64);

impl Drop for Claimed {
    fn drop(&mut self) {
        HANDING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&(self.0, self.1));
    }
}

/// Claims post `post` of the project `tools` for the hand-over, which the caller has just found
/// scheduled, unclaimed and to be handed over, under `PLANS` (`_held`).
pub(crate) fn claim_post(_held: &PlansHeld, tools: &ToolDeps, post: u64) -> Claimed {
    let project = project_of(tools);
    HANDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((project, post));
    Claimed(project, post)
}

/// Whether post `post` of the project `tools` is being handed to Buffer this moment.
pub(crate) fn is_being_handed_over(_held: &PlansHeld, tools: &ToolDeps, post: u64) -> bool {
    HANDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&(project_of(tools), post))
}

/// The proof that `PLANS` is held, which `record_plan_end` asks for.
pub(crate) struct PlansHeld(
    #[allow(dead_code, reason = "held for its drop")] MutexGuard<'static, ()>,
);

/// Takes `PLANS`, waiting for whoever holds it.
pub(crate) fn hold_plans() -> PlansHeld {
    PlansHeld(PLANS.lock().unwrap_or_else(PoisonError::into_inner))
}

fn failed(error: impl Display) -> CommandError {
    CommandError::Failed {
        detail: error.to_string(),
    }
}

fn refused(reason: String) -> CommandError {
    CommandError::Refused { reason }
}

/// The plan `plan`, or `unknown_marketing_plan`.
fn find<'a>(plans: &'a [MarketingPlan], plan: &str) -> Result<&'a MarketingPlan, CommandError> {
    plans
        .iter()
        .find(|found| found.record.id == plan)
        .ok_or_else(|| {
            refused(format!(
                "unknown_marketing_plan: {plan} is no marketing plan"
            ))
        })
}

/// Records `body` as the owner's or Farik's own act: the task on the envelope when it names one,
/// no agent, no session.
pub(crate) fn append(
    tools: &ToolDeps,
    task: Option<farik_core::contract::TaskId>,
    body: EventBody,
) -> Result<FarikEvent, String> {
    let ids = EventIds {
        task_id: task,
        ..tools.ids.clone()
    };
    let event = new_event(body, tools.clock.now(), ids)
        .map_err(|error| format!("the event cannot be recorded: {error:?}"))?;
    let appended = tools
        .log
        .append(&event)
        .map_err(|error| error.to_string())?;
    tools
        .projections
        .apply(&appended)
        .map_err(|error| error.to_string())?;
    Ok(appended)
}

/// What the owner said, left as they wrote it; nothing when it is blank.
fn owner_words(note: Option<String>) -> Option<String> {
    note.filter(|note| !note.trim().is_empty())
}

/// Approves the plan `plan` or sends it back, as the owner (`marketing_plan_decide`): refused
/// `unknown_marketing_plan`, `marketing_plan_decided` for a plan decided before,
/// `marketing_plan_expired` for approving one whose last day is past, and
/// `marketing_plan_reason_needed` for sending one back without a reason. An approval also ends,
/// in the same step, every approved plan the new one replaces at once.
pub(crate) fn decide_plan(
    tools: &ToolDeps,
    plan: &str,
    approve: bool,
    note: Option<String>,
) -> Result<CommandReport, CommandError> {
    let held = hold_plans();
    let plans = marketing_plans(&tools.log).map_err(failed)?;
    let found = find(&plans, plan)?;
    if found.decided.is_some() {
        return Err(refused(format!(
            "marketing_plan_decided: {plan} was decided already"
        )));
    }
    let today = tools.clock.now().date_naive();
    if approve && found.record.ends_on < today {
        return Err(refused(format!(
            "marketing_plan_expired: {plan} ended on {}, so it cannot be approved",
            found.record.ends_on
        )));
    }
    let note = owner_words(note);
    let task = Some(found.task_id.clone());
    let (body, report) = if approve {
        let body = json!({ "plan": plan, "note": note.unwrap_or_default() });
        (
            serde_json::from_value(body).map(EventBody::MarketingPlanApproved),
            format!("approved marketing plan {plan}"),
        )
    } else {
        let Some(reason) = note else {
            return Err(refused(format!(
                "marketing_plan_reason_needed: say why {plan} is sent back; the agent reads it"
            )));
        };
        (
            serde_json::from_value(json!({ "plan": plan, "reason": reason }))
                .map(EventBody::MarketingPlanReturned),
            format!("sent back marketing plan {plan}"),
        )
    };
    let decision = append(tools, task, body.map_err(failed)?).map_err(failed)?;
    let mut events = vec![decision.envelope.seq];
    let mut said = report;
    if approve {
        // The plans this one replaces at once, in the same step: the lock is still held.
        let records = records_of(tools)?;
        for (older, _, _) in plans_to_end(&records, today)
            .into_iter()
            .filter(|(_, why, by)| *why == EndReason::Replaced && by.as_deref() == Some(plan))
        {
            let ended =
                record_plan_end(&held, tools, &older, EndReason::Replaced, Some(plan), None)
                    .map_err(failed)?;
            events.extend(ended.iter().map(|event| event.envelope.seq));
            said.push_str(", which replaces ");
            said.push_str(&older);
        }
    }
    Ok(CommandReport { said, events })
}

/// What the log says of every plan, for the rules that decide which is active.
fn records_of(tools: &ToolDeps) -> Result<Vec<PlanRecord>, CommandError> {
    Ok(marketing_plans(&tools.log)
        .map_err(failed)?
        .into_iter()
        .map(|plan| plan.record)
        .collect())
}

/// Ends the approved plan `plan`, as the owner (`marketing_plan_end`): refused
/// `unknown_marketing_plan`, `marketing_plan_not_approved` for one never approved and
/// `marketing_plan_ended` for one that has ended.
pub(crate) fn end_plan(
    tools: &ToolDeps,
    plan: &str,
    note: Option<String>,
) -> Result<CommandReport, CommandError> {
    let held = hold_plans();
    let plans = marketing_plans(&tools.log).map_err(failed)?;
    let found = find(&plans, plan)?;
    if found.record.ended.is_some() {
        return Err(refused(format!("marketing_plan_ended: {plan} has ended")));
    }
    if found.record.approved_seq.is_none() {
        return Err(refused(format!(
            "marketing_plan_not_approved: {plan} was never approved, so there is nothing to end"
        )));
    }
    let ended = record_plan_end(
        &held,
        tools,
        plan,
        EndReason::ByOwner,
        None,
        owner_words(note),
    )
    .map_err(failed)?;
    Ok(CommandReport {
        said: format!("ended marketing plan {plan}"),
        events: ended.iter().map(|event| event.envelope.seq).collect(),
    })
}

/// Records the end of the approved plan `plan` for `why` (a replaced one names `replaced_by`),
/// unless it has ended already or was never approved, which records nothing. Every end goes
/// through here, the owner's and the dates', so that it is read again under the lock `held`
/// proves is held: two ends of one plan are never both recorded.
///
/// # Errors
///
/// What the log or the board refused, in words.
pub(crate) fn record_plan_end(
    held: &PlansHeld,
    tools: &ToolDeps,
    plan: &str,
    why: EndReason,
    replaced_by: Option<&str>,
    note: Option<String>,
) -> Result<Vec<FarikEvent>, String> {
    let plans = marketing_plans(&tools.log).map_err(|error| error.to_string())?;
    let due = plans
        .iter()
        .find(|found| found.record.id == plan)
        .is_some_and(|found| found.record.approved_seq.is_some() && found.record.ended.is_none());
    if !due {
        return Ok(Vec::new());
    }
    let mut body = json!({
        "plan": plan,
        "why": why.as_str(),
    });
    for (name, value) in [
        ("replaced_by", replaced_by.map(str::to_string)),
        ("note", note),
    ] {
        if let Some(value) = value {
            body[name] = Value::String(value);
        }
    }
    let body = serde_json::from_value(body).map_err(|error| error.to_string())?;
    let mut events = vec![append(tools, None, EventBody::MarketingPlanEnded(body))?];
    events.extend(stop_posts(held, tools, plan, why, replaced_by)?);
    Ok(events)
}

/// Stops the posts of the plan `plan` that its end takes with it, each as `plan_ended`: a post
/// scheduled and not yet with Buffer, when the owner ended the plan, or when a newer plan replaced
/// it and the post's slot falls on or after the newer plan's first day. Never when the plan only
/// ran out (each post was checked against a slot day inside the plan), and never a post Farik is
/// handing to Buffer this moment, which goes out as one already with Buffer, with its Stop.
fn stop_posts(
    held: &PlansHeld,
    tools: &ToolDeps,
    plan: &str,
    why: EndReason,
    replaced_by: Option<&str>,
) -> Result<Vec<FarikEvent>, String> {
    let plans = marketing_plans(&tools.log).map_err(|error| error.to_string())?;
    let slot_day = |slot: &str| {
        plans
            .iter()
            .find(|found| found.record.id == plan)
            .and_then(|found| found.proposal.posts.iter().find(|held| held.key == slot))
            .map(|found| found.on)
    };
    let newer_starts_on = replaced_by.and_then(|newer| {
        plans
            .iter()
            .find(|found| found.record.id == newer)
            .map(|found| found.record.starts_on)
    });
    let mut stopped = Vec::new();
    for post in social_posts(&tools.log).map_err(|error| error.to_string())? {
        let ends_with_the_plan = match why {
            EndReason::ByOwner => true,
            EndReason::Replaced => post
                .slot
                .as_deref()
                .and_then(slot_day)
                .zip(newer_starts_on)
                .is_some_and(|(on, starts_on)| on >= starts_on),
            EndReason::Expired => false,
        };
        if post.plan.as_deref() != Some(plan)
            || post.state != PostState::Scheduled
            || !ends_with_the_plan
            || is_being_handed_over(held, tools, post.post)
        {
            continue;
        }
        let body = serde_json::from_value(json!({ "post": post.post, "by": "plan_ended" }))
            .map_err(|error| error.to_string())?;
        stopped.push(append(tools, None, EventBody::SocialPostStopped(body))?);
    }
    Ok(stopped)
}

/// The post `post`, or `unknown_post`.
fn find_post(tools: &ToolDeps, post: u64) -> Result<SocialPost, CommandError> {
    social_posts(&tools.log)
        .map_err(failed)?
        .into_iter()
        .find(|found| found.post == post)
        .ok_or_else(|| refused(format!("unknown_post: {post} is no post")))
}

/// When a post goes out, as the owner reads it in the offset the post was written in.
fn when_words(post: &SocialPost) -> String {
    post.at.format("%a %-d %b %H:%M").to_string()
}

/// Records the owner's Stop of post `post`, `taken_back` when Farik first took it back from Buffer.
fn record_stop(
    tools: &ToolDeps,
    post: u64,
    taken_back: bool,
) -> Result<CommandReport, CommandError> {
    let mut body = json!({ "post": post, "by": "owner" });
    if taken_back {
        body["taken_back"] = json!(true);
    }
    let body = serde_json::from_value(body).map_err(failed)?;
    let stopped = append(tools, None, EventBody::SocialPostStopped(body)).map_err(failed)?;
    Ok(CommandReport {
        said: if taken_back {
            format!("stopped post {post} and took it back from Buffer")
        } else {
            format!("stopped post {post}")
        },
        events: vec![stopped.envelope.seq],
    })
}

/// What the first look at a post to stop found: it is stopped, or Buffer must be asked to take it
/// back first.
enum Beginning {
    Done(CommandReport),
    TakeBack { agent: String, buffer_post: String },
}

/// The first look at a post to stop, under the plans' lock: a post not yet with Buffer is stopped
/// here and now.
fn begin_stop(tools: &ToolDeps, post: u64) -> Result<Beginning, CommandError> {
    let held = hold_plans();
    let found = find_post(tools, post)?;
    match found.state {
        PostState::Scheduled if is_being_handed_over(&held, tools, post) => Err(refused(
            "post_being_handed_over: Farik is giving it to Buffer now; stop it again in a minute"
                .to_string(),
        )),
        PostState::Scheduled => record_stop(tools, post, false).map(Beginning::Done),
        PostState::Sent if found.at.with_timezone(&Utc) <= tools.clock.now() => Err(refused(
            format!("post_already_out: post {post} went out at its time, so it cannot be stopped"),
        )),
        PostState::Sent => Ok(Beginning::TakeBack {
            agent: found.agent_id,
            buffer_post: found.buffer_post.unwrap_or_default(),
        }),
        other => Err(refused(format!(
            "post_not_going_out: post {post} is {}, so there is nothing to stop",
            other.as_str()
        ))),
    }
}

/// The end of a Stop of a sent post, once Buffer has answered (`taken`): the post is read again
/// under the plans' lock, and stopped only while it is still with Buffer, so that two Stops of it
/// record one.
fn end_take_back(tools: &ToolDeps, post: u64, taken: bool) -> Result<CommandReport, CommandError> {
    let _held = hold_plans();
    let found = find_post(tools, post)?;
    if found.state != PostState::Sent {
        return Err(refused(format!(
            "post_not_going_out: post {post} is {}, so there is nothing to stop",
            found.state.as_str()
        )));
    }
    if !taken {
        return Err(refused(format!(
            "post_not_taken_back: Buffer did not take it back; delete it in Buffer before {}",
            when_words(&found)
        )));
    }
    record_stop(tools, post, true)
}

/// Stops post `post`, as the owner (`social_post_stop`): a scheduled post that is not with Buffer
/// is stopped, one that is has Farik's own `delete_post` first and is stopped only when Buffer took
/// it back. Refused `unknown_post`; `post_not_going_out` for a post requested, stopped, missed or
/// failed; `post_being_handed_over` for one Farik is giving to Buffer this moment;
/// `post_already_out` once its time has passed; `post_not_taken_back` when Buffer would not.
pub(crate) async fn stop_post(
    deps: &OrchestratorDeps,
    post: u64,
) -> Result<CommandReport, CommandError> {
    let (agent, buffer_post) = match begin_stop(&deps.tools, post)? {
        Beginning::Done(report) => return Ok(report),
        Beginning::TakeBack { agent, buffer_post } => (agent, buffer_post),
    };
    let mut arguments = Map::new();
    arguments.insert("postId".to_string(), json!(buffer_post));
    let answer = call_as(&deps.daemon, &agent, "buffer", "delete_post", arguments).await;
    end_take_back(&deps.tools, post, answer.is_ok())
}

/// The fields of a request, as the owner's `social_post.scheduled` carries them on.
fn request_body(post: &SocialPost) -> Value {
    let mut body = json!({
        "post": post.post,
        "channel": post.channel.as_str(),
        "buffer_channel": post.buffer_channel,
        "text": post.text,
        "media": post.media.iter().map(|media| json!({
            "url": media.url,
            "kind": if media.video { "video" } else { "image" },
        })).collect::<Vec<_>>(),
        "at": post.at.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        "approved_by": "owner",
    });
    match &post.details {
        Some(PostDetails::Youtube { title, category_id }) => {
            body["details"] = json!({ "title": title, "category_id": category_id });
        }
        Some(PostDetails::Pinterest { board }) => body["details"] = json!({ "board": board }),
        None => {}
    }
    body
}

/// Allows the request `post` or does not, as the owner (`social_post_decide`): allowing it records
/// `social_post.scheduled { approved_by: owner }` on the request's task, and the hand-over sends it
/// at its time; not allowing it records `stopped { by: declined }` with the owner's words. Refused
/// `unknown_post`, `post_decided` for a post that is not waiting for the owner and
/// `post_in_the_past` once the request's time is near: allowing needs it more than five minutes
/// ahead, since Farik cannot hand it to Buffer in time, and not allowing it needs it still ahead.
pub(crate) fn decide_post(
    tools: &ToolDeps,
    post: u64,
    post_it: bool,
    note: Option<String>,
) -> Result<CommandReport, CommandError> {
    let _held = hold_plans();
    let found = find_post(tools, post)?;
    let now = tools.clock.now();
    let too_late = |why: &str| refused(format!("post_in_the_past: {why}"));
    match found.state {
        PostState::Requested => {}
        // A request nobody decided in time.
        PostState::Missed if found.approved_by.is_none() => {
            return Err(too_late(&format!("post {post} was not decided in time")));
        }
        _ => {
            return Err(refused(format!(
                "post_decided: post {post} is not waiting for your decision"
            )));
        }
    }
    let out_of_time = if post_it {
        is_too_late(&found, now)
    } else {
        found.at.with_timezone(&Utc) <= now
    };
    if out_of_time {
        return Err(too_late(&format!(
            "post {post} is for {}, too near for Farik to hand it to Buffer, or past",
            when_words(&found)
        )));
    }
    let (body, said) = if post_it {
        (
            serde_json::from_value(request_body(&found)).map(EventBody::SocialPostScheduled),
            format!("allowed post {post}"),
        )
    } else {
        let mut body = json!({ "post": post, "by": "declined" });
        if let Some(note) = owner_words(note) {
            body["note"] = json!(note);
        }
        (
            serde_json::from_value(body).map(EventBody::SocialPostStopped),
            format!("did not allow post {post}"),
        )
    };
    // The allowance is about the request's contract, so it carries the request's task; a refusal
    // to allow is about no contract.
    let task = post_it.then(|| found.task_id.clone());
    let decided = append(tools, task, body.map_err(failed)?).map_err(failed)?;
    Ok(CommandReport {
        said,
        events: vec![decided.envelope.seq],
    })
}

/// One marketing plan as `marketing_plan.list` words a row: the daemon's answer and the command
/// line's `--json` are one shape.
#[must_use]
pub fn list_row(plan: &MarketingPlan, state: PlanState) -> Value {
    let proposal = &plan.proposal;
    json!({
        "plan": plan.record.id,
        "title": proposal.title,
        "state": state.as_str(),
        "starts_on": proposal.starts_on.to_string(),
        "ends_on": proposal.ends_on.to_string(),
        "currency": proposal.currency,
        "total": proposal.total.to_string(),
        "agent_id": plan.agent_id,
        "task_id": plan.task_id.as_str(),
        "proposed_at": plan.proposed_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
    })
}

/// How far back Today looks for posts that did not go out.
const DID_NOT_GO_OUT_FOR: Duration = Duration::hours(24);

/// A time as the wire words it: RFC 3339, with a `Z` for UTC.
fn wire_time<Tz: chrono::TimeZone>(time: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// One post as `social_posts.list` words a row: the daemon's answer and the command line's `--json`
/// are one shape.
#[must_use]
pub fn post_row(post: &SocialPost) -> Value {
    let mut row = json!({
        "post": post.post,
        "agent_id": post.agent_id,
        "channel": post.channel.as_str(),
        "text": post.text,
        "media": post.media.iter().map(|media| json!({
            "url": media.url,
            "kind": if media.video { "video" } else { "image" },
        })).collect::<Vec<_>>(),
        "at": wire_time(&post.at),
        "hands_over_at": wire_time(&hand_over_time(post)),
        "state": post.state.as_str(),
    });
    for (name, value) in [
        ("plan", post.plan.as_deref()),
        ("slot", post.slot.as_deref()),
        ("approved_by", post.approved_by.as_deref()),
        ("missed_why", post.missed_why.as_deref()),
        ("reason", post.reason.as_deref()),
    ] {
        if let Some(value) = value {
            row[name] = json!(value);
        }
    }
    row
}

/// What Today shows of the posts, as `social_posts.list` answers it: every post scheduled or sent
/// whose time is still ahead, soonest first, then every post missed or failed in the last 24 hours,
/// the latest first.
#[must_use]
pub fn going_out(posts: &[SocialPost], now: DateTime<Utc>) -> Vec<Value> {
    let mut ahead: Vec<&SocialPost> = posts
        .iter()
        .filter(|post| {
            matches!(post.state, PostState::Scheduled | PostState::Sent)
                && post.at.with_timezone(&Utc) > now
        })
        .collect();
    ahead.sort_by_key(|post| (post.at.with_timezone(&Utc), post.post));
    let mut did_not: Vec<&SocialPost> = posts
        .iter()
        .filter(|post| {
            matches!(post.state, PostState::Missed | PostState::Failed)
                && post.state_at > now - DID_NOT_GO_OUT_FOR
        })
        .collect();
    did_not.sort_by_key(|post| std::cmp::Reverse((post.state_at, post.post)));
    ahead.into_iter().chain(did_not).map(post_row).collect()
}

/// The posts written for the plan `plan`'s slots, oldest first, as `marketing_plan.get` lists them.
fn written_posts(plan: &str, posts: &[SocialPost]) -> Vec<Value> {
    posts
        .iter()
        .filter(|post| post.plan.as_deref() == Some(plan))
        .map(|post| {
            let mut row = json!({
                "post": post.post,
                "slot": post.slot,
                "text": post.text,
                "at": wire_time(&post.at),
                "state": post.state.as_str(),
                "state_at": wire_time(&post.state_at),
            });
            for (name, value) in [
                ("stopped_by", post.stopped_by.as_deref()),
                ("missed_why", post.missed_why.as_deref()),
            ] {
                if let Some(value) = value {
                    row[name] = json!(value);
                }
            }
            row
        })
        .collect()
}

/// One marketing plan whole, as `marketing_plan.get` words it: the proposal, its state, the owner's
/// decision with their words, its end with the owner's note and the plan that replaced it, and the
/// posts written for it. `posts` in it are the plan's slots, `written_posts` what was written for
/// them, taken from `written`, every post of the project.
#[must_use]
pub fn whole(plan: &MarketingPlan, state: PlanState, written: &[SocialPost]) -> Value {
    let proposal = &plan.proposal;
    let time = |at: &chrono::DateTime<chrono::Utc>| {
        at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
    };
    let decided = plan.decided.as_ref().map(|(approved, words, at)| {
        let mut decision = json!({
            "decision": if *approved { "approved" } else { "returned" },
            "at": time(at),
        });
        if let Some(words) = words {
            decision[if *approved { "note" } else { "reason" }] = json!(words);
        }
        decision
    });
    let ended = plan.record.ended.zip(plan.ended_at).map(|(why, at)| {
        let mut end = json!({ "why": why.as_str(), "at": time(&at) });
        // The owner's words, and the plan that took over, when the end has them.
        if let Some(note) = &plan.end_note {
            end["note"] = json!(note);
        }
        if let Some(newer) = &plan.replaced_by {
            end["replaced_by"] = json!(newer);
        }
        end
    });
    json!({
        "plan": plan.record.id,
        "title": proposal.title,
        "summary": proposal.summary,
        "text": proposal.text,
        "state": state.as_str(),
        "starts_on": proposal.starts_on.to_string(),
        "ends_on": proposal.ends_on.to_string(),
        "currency": proposal.currency,
        "budget": {
            "total": proposal.total.to_string(),
            "google_ads": proposal.google_ads.to_string(),
        },
        "campaigns": proposal.campaigns.iter().map(|campaign| json!({
            "key": campaign.key,
            "channel": "google_ads",
            "name": campaign.name,
            "goal": campaign.goal,
            "budget": campaign.budget.to_string(),
            "starts_on": campaign.starts_on.to_string(),
            "ends_on": campaign.ends_on.to_string(),
        })).collect::<Vec<_>>(),
        "posts": proposal.posts.iter().map(|post| json!({
            "key": post.key,
            "channel": post.channel.as_str(),
            "on": post.on.to_string(),
            "topic": post.topic,
        })).collect::<Vec<_>>(),
        "written_posts": written_posts(&plan.record.id, written),
        "measures": proposal.measures,
        "google_ads_account": proposal.google_ads_account,
        "replaces": proposal.replaces,
        "agent_id": plan.agent_id,
        "task_id": plan.task_id.as_str(),
        "proposed_at": time(&plan.proposed_at),
        "decided": decided,
        "ended": ended,
    })
}

/// The states of `plans`, in their order, given today: the one in force is active.
#[must_use]
pub fn states_today(plans: &[MarketingPlan], today: chrono::NaiveDate) -> Vec<PlanState> {
    let records: Vec<PlanRecord> = plans.iter().map(|plan| plan.record.clone()).collect();
    let active = farik_core::marketing::active_plan(&records, today).map(|plan| plan.id.as_str());
    plans.iter().map(|plan| plan.state(active)).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use chrono::Duration as Minutes;
    use farik_protocol::command::Command;
    use farik_protocol::event::EventKind;
    use serde_json::{Value, json};

    use super::{decide_plan, end_plan, hold_plans};
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::hand_over::fixtures::{Handing, after};
    use crate::orchestrator::{CommandError, CommandReport};
    use crate::tools::fixtures::at;
    use crate::tools::media::LoopbackAllowed;

    /// How long a call that must wait for the lock is given to show that it did not.
    const LONG_ENOUGH: Duration = Duration::from_millis(100);

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn decisions_wait_for_the_plans_lock() {
        let harness = Harness::new(
            "plans-lock",
            crate::tools::fixtures::with_the_marketing_specialist,
        );
        harness.in_progress("FRK-1", "kai", "pm");
        harness
            .project
            .plan_proposed("FRK-1", "MP-1", "2026-09-22", "2026-10-20");
        let deps = Arc::clone(&harness.project.deps);
        let recorded = |kind| harness.project.events(&[kind]).len();

        // The owner's approval, called while the lock is held elsewhere, records nothing until
        // the lock is let go, and then exactly one.
        let held = hold_plans();
        let approving = {
            let deps = Arc::clone(&deps);
            thread::spawn(move || decide_plan(&deps, "MP-1", true, None))
        };
        thread::sleep(LONG_ENOUGH);
        assert_eq!(
            recorded(EventKind::MarketingPlanApproved),
            0,
            "the approval waits for the lock"
        );
        drop(held);
        approving
            .join()
            .expect("the approval's thread ends")
            .expect("the approval is made");
        assert_eq!(recorded(EventKind::MarketingPlanApproved), 1);

        // The owner's end of the plan waits the same way.
        let held = hold_plans();
        let ending = {
            let deps = Arc::clone(&deps);
            thread::spawn(move || end_plan(&deps, "MP-1", None))
        };
        thread::sleep(LONG_ENOUGH);
        assert_eq!(
            recorded(EventKind::MarketingPlanEnded),
            0,
            "the end waits for the lock"
        );
        drop(held);
        ending
            .join()
            .expect("the end's thread ends")
            .expect("the end is made");
        assert_eq!(recorded(EventKind::MarketingPlanEnded), 1);
    }

    fn refusal_of(result: Result<CommandReport, CommandError>) -> String {
        match result {
            Err(CommandError::Refused { reason }) => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The body of the one event of `kind` the log holds.
    fn the_body(handing: &Handing, kind: EventKind) -> Value {
        let events = handing.events(kind);
        assert_eq!(events.len(), 1, "{kind:?}: {events:?}");
        farik_protocol::event::event_to_value(&events[0])["body"].clone()
    }

    async fn stop(handing: &Handing, post: u64) -> Result<CommandReport, CommandError> {
        handing
            .orchestrator
            .handle(Command::SocialPostStop { post })
            .await
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stop_before_the_hand_over_records_it() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-before").await;
        let post = handing.schedules(&after(180));

        stop(&handing, post).await.expect("stopped");

        assert_eq!(
            the_body(&handing, EventKind::SocialPostStopped),
            json!({ "post": post, "by": "owner" })
        );
        let ids = &handing.events(EventKind::SocialPostStopped)[0].envelope.ids;
        assert!(
            ids.agent_id.is_none() && ids.session_id.is_none(),
            "the owner's"
        );
        assert!(
            handing.fixture.calls("delete_post").is_empty(),
            "Buffer has nothing to delete"
        );
        // A stopped post is not handed over when its hour comes.
        handing.now(at() + Minutes::minutes(130));
        handing.hands_over().await;
        assert!(handing.created().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stop_after_takes_it_back_from_buffer() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-after").await;
        let post = handing.schedules(&after(120));
        handing.happens(
            "social_post.sent",
            &json!({ "post": post, "buffer_post": "buf-1" }),
        );

        stop(&handing, post).await.expect("taken back");

        assert_eq!(
            handing.fixture.calls("delete_post"),
            [json!({ "postId": "buf-1" })
                .as_object()
                .cloned()
                .expect("an object")]
        );
        assert_eq!(
            the_body(&handing, EventKind::SocialPostStopped),
            json!({ "post": post, "by": "owner", "taken_back": true })
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_stop_buffer_refuses_records_nothing() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-refused").await;
        handing.fixture.set(|flags| {
            flags.tool_answers.insert(
                "delete_post".to_string(),
                crate::oauth_fixture::ToolAnswer::Error("No such post".to_string()),
            );
        });
        let post = handing.schedules("2026-09-22T14:00:00-05:00");
        handing.happens(
            "social_post.sent",
            &json!({ "post": post, "buffer_post": "buf-1" }),
        );
        handing.now("2026-09-22T12:00:00Z".parse().expect("a time"));

        let reason = refusal_of(stop(&handing, post).await);

        assert_eq!(
            reason,
            "post_not_taken_back: Buffer did not take it back; delete it in Buffer before Tue 22 Sep 14:00"
        );
        assert!(handing.events(EventKind::SocialPostStopped).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn too_late_to_stop() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-too-late").await;
        let post = handing.schedules(&after(120));
        handing.happens(
            "social_post.sent",
            &json!({ "post": post, "buffer_post": "buf-1" }),
        );
        // Its time has passed: it is out.
        handing.now(at() + Minutes::minutes(121));

        let reason = refusal_of(stop(&handing, post).await);

        assert!(reason.starts_with("post_already_out"), "{reason}");
        assert!(handing.fixture.calls("delete_post").is_empty());
        assert!(handing.events(EventKind::SocialPostStopped).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn stops_only_a_post_going_out() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-only-going").await;
        let reason = refusal_of(stop(&handing, 999).await);
        assert!(reason.starts_with("unknown_post"), "{reason}");

        let mut request = handing.a_post(&after(300));
        let object = request.as_object_mut().expect("an object");
        for key in ["approved_by", "plan", "slot"] {
            object.remove(key);
        }
        let requested = handing.records("social_post.requested", &request);
        let stopped = handing.schedules(&after(300));
        handing.happens(
            "social_post.stopped",
            &json!({ "post": stopped, "by": "owner" }),
        );
        let missed = handing.schedules(&after(300));
        handing.happens(
            "social_post.missed",
            &json!({ "post": missed, "why": "paused" }),
        );
        let failed = handing.schedules(&after(300));
        handing.happens(
            "social_post.failed",
            &json!({ "post": failed, "reason": "No" }),
        );
        for (post, what) in [
            (requested, "requested"),
            (stopped, "stopped"),
            (missed, "missed"),
            (failed, "failed"),
        ] {
            let reason = refusal_of(stop(&handing, post).await);
            assert!(reason.starts_with("post_not_going_out"), "{what}: {reason}");
        }
        assert_eq!(
            handing.events(EventKind::SocialPostStopped).len(),
            1,
            "only the one made above"
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_stop_during_the_hand_over_is_refused_and_the_post_is_sent() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-during").await;
        let post = handing.schedules(&after(59));
        handing.fixture.hold("tool:create_post");

        let ((), refused) = tokio::join!(handing.hands_over(), async {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            while handing.created().is_empty() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the hand-over never reached Buffer"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let refused = refusal_of(stop(&handing, post).await);
            handing.fixture.release("tool:create_post");
            refused
        });

        assert_eq!(
            refused,
            "post_being_handed_over: Farik is giving it to Buffer now; stop it again in a minute"
        );
        assert_eq!(
            the_body(&handing, EventKind::SocialPostSent),
            json!({ "post": post, "buffer_post": "buf-1" })
        );
        assert!(handing.events(EventKind::SocialPostStopped).is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn two_stops_at_once_record_one() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("stop-twice").await;
        let post = handing.schedules(&after(120));
        handing.happens(
            "social_post.sent",
            &json!({ "post": post, "buffer_post": "buf-1" }),
        );
        handing.fixture.hold("tool:delete_post");

        let (first, second, ()) = tokio::join!(stop(&handing, post), stop(&handing, post), async {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            while handing.fixture.calls("delete_post").len() < 2 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "both never reached Buffer"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            handing.fixture.release("tool:delete_post");
        });

        let results = [first, second];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let other = results
            .into_iter()
            .find_map(Result::err)
            .expect("the other");
        let CommandError::Refused { reason } = other else {
            panic!("a refusal");
        };
        assert!(reason.starts_with("post_not_going_out"), "{reason}");
        assert_eq!(
            the_body(&handing, EventKind::SocialPostStopped),
            json!({ "post": post, "by": "owner", "taken_back": true })
        );
    }

    /// A request outside the plan, `minutes` from now.
    fn requested(handing: &Handing, minutes: i64) -> (u64, Value) {
        let mut body = handing.a_post(&after(minutes));
        let object = body.as_object_mut().expect("an object");
        for key in ["approved_by", "plan", "slot"] {
            object.remove(key);
        }
        (handing.records("social_post.requested", &body), body)
    }

    async fn decide(
        handing: &Handing,
        post: u64,
        post_it: bool,
        note: Option<&str>,
    ) -> Result<CommandReport, CommandError> {
        handing
            .orchestrator
            .handle(Command::SocialPostDecide {
                post,
                post_it,
                note: note.map(str::to_string),
            })
            .await
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn post_it_schedules_the_request() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("decide-post").await;
        let (post, mut request) = requested(&handing, 30);

        decide(&handing, post, true, None).await.expect("allowed");

        request["post"] = json!(post);
        request["approved_by"] = json!("owner");
        assert_eq!(the_body(&handing, EventKind::SocialPostScheduled), request);
        let ids = &handing.events(EventKind::SocialPostScheduled)[0]
            .envelope
            .ids;
        assert_eq!(
            ids.task_id.as_ref().map(|task| task.to_string()).as_deref(),
            Some("FRK-1")
        );
        assert!(
            ids.agent_id.is_none() && ids.session_id.is_none(),
            "the owner's, with the request's task"
        );
        // Its hour has passed, so the next tick hands it over.
        handing.hands_over().await;
        assert_eq!(handing.created().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn dont_post_stops_it() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("decide-dont").await;
        let (post, _) = requested(&handing, 300);

        decide(&handing, post, false, Some("Not this week"))
            .await
            .expect("declined");

        assert_eq!(
            the_body(&handing, EventKind::SocialPostStopped),
            json!({ "post": post, "by": "declined", "note": "Not this week" })
        );
        handing.now(at() + Minutes::minutes(250));
        handing.hands_over().await;
        assert!(handing.created().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn decided_once() {
        let _here = LoopbackAllowed::new();
        let handing = Handing::new("decide-once").await;
        let (post, _) = requested(&handing, 300);
        let reason = refusal_of(decide(&handing, 999, true, None).await);
        assert!(reason.starts_with("unknown_post"), "{reason}");

        decide(&handing, post, true, None).await.expect("allowed");
        for post_it in [true, false] {
            let reason = refusal_of(decide(&handing, post, post_it, None).await);
            assert!(reason.starts_with("post_decided"), "{reason}");
        }
        assert_eq!(
            handing.events(EventKind::SocialPostScheduled).len(),
            1,
            "one allowance; the two refusals recorded nothing"
        );

        // A request whose time has passed cannot be allowed, and one that is five minutes away
        // cannot be handed to Buffer in time; either can still be declined until it is out.
        let (late, _) = requested(&handing, 10);
        let (near, _) = requested(&handing, 20);
        handing.now(at() + Minutes::minutes(11));
        let reason = refusal_of(decide(&handing, late, true, None).await);
        assert!(reason.starts_with("post_in_the_past"), "{reason}");
        let reason = refusal_of(decide(&handing, late, false, None).await);
        assert!(reason.starts_with("post_in_the_past"), "{reason}");
        handing.now(at() + Minutes::minutes(16));
        let reason = refusal_of(decide(&handing, near, true, None).await);
        assert!(reason.starts_with("post_in_the_past"), "{reason}");
        decide(&handing, near, false, None)
            .await
            .expect("declined in time");
    }
}
