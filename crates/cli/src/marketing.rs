//! `farik marketing plan show`: the Marketing Specialist's plans and where each stands
//! (`docs/SPEC.md` 6.5, ADR 0042). The decisions on them are commands, sent as `farik tool approve`
//! sends its own.

use chrono::{DateTime, Utc};
use farik_core::marketing::{PlanRecord, active_plan};
use farik_store::marketing::{MarketingPlan, PlanState, marketing_plans};
use serde_json::{Value, json};

use crate::Report;
use crate::project::Project;

/// The plans, newest first, or the one named, with where each stands.
///
/// # Errors
///
/// A sentence naming a plan that is not there, or saying what the store refused.
pub fn show(project: &Project, plan: Option<&str>, now: DateTime<Utc>) -> Result<Report, String> {
    let mut plans = marketing_plans(&project.log).map_err(|error| error.to_string())?;
    let records: Vec<PlanRecord> = plans.iter().map(|plan| plan.record.clone()).collect();
    let active = active_plan(&records, now.date_naive()).map(|plan| plan.id.clone());
    let state_of = |plan: &MarketingPlan| plan.state(active.as_deref());
    if let Some(id) = plan {
        let found = plans
            .iter()
            .find(|found| found.record.id == id)
            .ok_or_else(|| format!("{id} is not a marketing plan of this project"))?;
        let state = state_of(found);
        return Ok(Report {
            lines: one_lines(found, state),
            json: one_json(found, state),
            json_lines: None,
        });
    }
    plans.reverse();
    if plans.is_empty() {
        return Ok(Report {
            lines: vec!["no marketing plan yet".to_string()],
            json: json!({ "plans": [] }),
            json_lines: None,
        });
    }
    Ok(Report {
        lines: plans
            .iter()
            .map(|plan| list_line(plan, state_of(plan)))
            .collect(),
        json: json!({
            "plans": plans
                .iter()
                .map(|plan| list_json(plan, state_of(plan)))
                .collect::<Vec<_>>()
        }),
        json_lines: None,
    })
}

/// A hundredths amount as the plan wrote it, two decimals.
fn money(hundredths: u64) -> String {
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

fn list_line(plan: &MarketingPlan, state: PlanState) -> String {
    let proposal = &plan.proposal;
    format!(
        "{:<6} {:<9} {} to {}  {} {}  {}",
        plan.record.id,
        state.as_str(),
        proposal.starts_on,
        proposal.ends_on,
        proposal.currency,
        money(proposal.total.0),
        proposal.title
    )
}

/// One row of the list, as `marketing_plan.list` words it.
fn list_json(plan: &MarketingPlan, state: PlanState) -> Value {
    let proposal = &plan.proposal;
    json!({
        "plan": plan.record.id,
        "title": proposal.title,
        "state": state.as_str(),
        "starts_on": proposal.starts_on.to_string(),
        "ends_on": proposal.ends_on.to_string(),
        "currency": proposal.currency,
        "total": money(proposal.total.0),
        "agent_id": plan.agent_id,
        "task_id": plan.task_id.as_str(),
        "proposed_at": plan.proposed_at.to_rfc3339(),
    })
}

fn one_lines(plan: &MarketingPlan, state: PlanState) -> Vec<String> {
    let proposal = &plan.proposal;
    let mut lines = vec![
        format!("{} {}", plan.record.id, state.as_str()),
        format!("title: {}", proposal.title),
        format!(
            "proposed by {} on {}, {}",
            plan.agent_id,
            plan.task_id.as_str(),
            plan.proposed_at.to_rfc3339()
        ),
        format!("dates: {} to {}", proposal.starts_on, proposal.ends_on),
        format!(
            "budget: {} {} in all, {} of it on Google Ads",
            proposal.currency,
            money(proposal.total.0),
            money(proposal.google_ads.0)
        ),
        format!("summary: {}", proposal.summary),
    ];
    for campaign in &proposal.campaigns {
        lines.push(format!(
            "campaign {}: {} {} {} to {}, {}",
            campaign.key,
            campaign.name,
            money(campaign.budget.0),
            campaign.starts_on,
            campaign.ends_on,
            campaign.goal
        ));
    }
    for post in &proposal.posts {
        lines.push(format!(
            "post {}: {} on {}, {}",
            post.key,
            post.channel.as_str(),
            post.on,
            post.topic
        ));
    }
    for measure in &proposal.measures {
        lines.push(format!("measure: {measure}"));
    }
    if let Some((approved, note, at)) = &plan.decided {
        let words = note
            .as_ref()
            .map_or(String::new(), |note| format!(": {note}"));
        lines.push(format!(
            "{} {}{}",
            if *approved { "approved" } else { "sent back" },
            at.to_rfc3339(),
            words
        ));
    }
    if let (Some(why), Some(at)) = (plan.record.ended, plan.ended_at) {
        lines.push(format!("ended {} ({})", at.to_rfc3339(), end_word(why)));
    }
    lines
}

fn end_word(why: farik_core::marketing::EndReason) -> &'static str {
    use farik_core::marketing::EndReason;
    match why {
        EndReason::Replaced => "replaced",
        EndReason::ByOwner => "by_owner",
        EndReason::Expired => "expired",
    }
}

/// The plan whole, as `marketing_plan.get` words it.
fn one_json(plan: &MarketingPlan, state: PlanState) -> Value {
    let proposal = &plan.proposal;
    let decided = plan.decided.as_ref().map(|(approved, note, at)| {
        let mut decision = json!({
            "decision": if *approved { "approved" } else { "returned" },
            "at": at.to_rfc3339(),
        });
        if let Some(note) = note {
            decision[if *approved { "note" } else { "reason" }] = json!(note);
        }
        decision
    });
    let ended = plan
        .record
        .ended
        .zip(plan.ended_at)
        .map(|(why, at)| json!({ "why": end_word(why), "at": at.to_rfc3339() }));
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
            "total": money(proposal.total.0),
            "google_ads": money(proposal.google_ads.0),
        },
        "campaigns": proposal.campaigns.iter().map(|campaign| json!({
            "key": campaign.key,
            "channel": "google_ads",
            "name": campaign.name,
            "goal": campaign.goal,
            "budget": money(campaign.budget.0),
            "starts_on": campaign.starts_on.to_string(),
            "ends_on": campaign.ends_on.to_string(),
        })).collect::<Vec<_>>(),
        "posts": proposal.posts.iter().map(|post| json!({
            "key": post.key,
            "channel": post.channel.as_str(),
            "on": post.on.to_string(),
            "topic": post.topic,
        })).collect::<Vec<_>>(),
        "measures": proposal.measures,
        "google_ads_account": proposal.google_ads_account,
        "replaces": proposal.replaces,
        "agent_id": plan.agent_id,
        "task_id": plan.task_id.as_str(),
        "proposed_at": plan.proposed_at.to_rfc3339(),
        "decided": decided,
        "ended": ended,
    })
}
