//! `farik marketing plan show`: the Marketing Specialist's plans and where each stands
//! (`docs/SPEC.md` 6.5, ADR 0042). The decisions on them are commands, sent as `farik tool approve`
//! sends its own.

use chrono::{DateTime, Utc};
use farik_core::marketing::network_name;
use farik_runtime::marketing::{list_row, post_row, posts_going_out, states_today, whole};
use farik_store::marketing::{MarketingPlan, PlanState, marketing_plans, social_posts};
use serde_json::json;

use crate::Report;
use crate::project::Project;

/// The plans, newest first, or the one named, with where each stands.
///
/// # Errors
///
/// A sentence naming a plan that is not there, or saying what the store refused.
pub fn show(project: &Project, plan: Option<&str>, now: DateTime<Utc>) -> Result<Report, String> {
    let mut plans = marketing_plans(&project.log).map_err(|error| error.to_string())?;
    let mut states = states_today(&plans, now.date_naive());
    if let Some(id) = plan {
        let at = plans
            .iter()
            .position(|found| found.record.id == id)
            .ok_or_else(|| format!("{id} is not a marketing plan of this project"))?;
        let (found, state) = (&plans[at], states[at]);
        let posts = social_posts(&project.log).map_err(|error| error.to_string())?;
        return Ok(Report {
            lines: one_lines(found, state),
            json: whole(found, state, &posts),
            json_lines: None,
        });
    }
    plans.reverse();
    states.reverse();
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
            .zip(&states)
            .map(|(plan, state)| list_line(plan, *state))
            .collect(),
        json: json!({
            "plans": plans
                .iter()
                .zip(&states)
                .map(|(plan, state)| list_row(plan, *state))
                .collect::<Vec<_>>()
        }),
        json_lines: None,
    })
}

/// The most characters of a post's text one line of the list shows.
const TEXT_ON_A_LINE: usize = 60;

/// The posts going out, soonest first, then those that did not go out in the last day: one line
/// each, `<post> <Network> <at> <state>: <the text's first line>`, and the same rows as
/// `social_posts.list` answers with `--json`.
///
/// # Errors
///
/// A sentence saying what the store refused.
pub fn posts(project: &Project, now: DateTime<Utc>) -> Result<Report, String> {
    let all = social_posts(&project.log).map_err(|error| error.to_string())?;
    let going = posts_going_out(&all, now);
    let lines = if going.is_empty() {
        vec!["no post is going out".to_string()]
    } else {
        going
            .iter()
            .map(|post| {
                let first: String = post
                    .text
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(TEXT_ON_A_LINE)
                    .collect();
                format!(
                    "{} {} {} {}: {}",
                    post.post,
                    network_name(post.channel),
                    post_row(post)["at"].as_str().unwrap_or_default(),
                    post.state.as_str(),
                    first
                )
            })
            .collect()
    };
    Ok(Report {
        lines,
        json: json!({ "posts": going.iter().map(|post| post_row(post)).collect::<Vec<_>>() }),
        json_lines: None,
    })
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
        proposal.total,
        proposal.title
    )
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
            proposal.currency, proposal.total, proposal.google_ads
        ),
        format!("summary: {}", proposal.summary),
    ];
    for campaign in &proposal.campaigns {
        lines.push(format!(
            "campaign {}: {} {} {} to {}, {}",
            campaign.key,
            campaign.name,
            campaign.budget,
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
        lines.push(format!("ended {} ({})", at.to_rfc3339(), why.as_str()));
    }
    lines
}
