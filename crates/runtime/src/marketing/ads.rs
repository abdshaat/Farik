//! What Today and a marketing plan's page say of the plan's ads and their budget (ADR 0042, step
//! 08g): the rows that ask the owner what to do when a budget was reached, or that tell them ads
//! may be running that Farik could not pause or cannot watch, and the plan's spend and pauses.

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use farik_core::contract::{TaskId, TaskStatus};
use farik_core::marketing::{
    CapScope, Lineage, PlanRecord, active_plan, is_carried, to_pause_for_end,
};
use farik_store::StoreError;
use farik_store::marketing::{
    BudgetReached, CampaignPaused, MarketingPlan, PausedWhy, budgets_reached, campaigns_paused,
    created_campaigns, marketing_plans, raises,
};
use serde_json::{Map, Value, json};

use crate::daemon::{DaemonState, SpendRead, ads_calls::lineage_of};
use crate::marketing::known_spend;
use crate::tools::ToolDeps;

/// The task of a request to raise `plan`'s budget that is open: not accepted and not cancelled.
///
/// # Errors
///
/// What the log or the board refused.
pub(crate) fn open_raise(deps: &ToolDeps, plan: &str) -> Result<Option<TaskId>, StoreError> {
    let board = deps.projections.board()?;
    Ok(raises(&deps.log)?
        .into_iter()
        .filter(|raise| raise.plan == plan)
        .find(|raise| {
            board.iter().any(|row| {
                row.task_id == raise.task_id
                    && !matches!(row.status, TaskStatus::Accepted | TaskStatus::Cancelled)
            })
        })
        .map(|raise| raise.task_id))
}

/// Whether a campaign Farik made under `plan` is left to pause: not carried by the active plan and
/// not recorded paused for its end. The command that ends a plan, handled in a process that drives
/// nothing, says so, since the watch pauses it when a process does.
///
/// # Errors
///
/// What the log refused.
pub fn ads_left_to_pause(deps: &ToolDeps, plan: &str) -> Result<bool, StoreError> {
    let plans = marketing_plans(&deps.log)?;
    let records: Vec<PlanRecord> = plans.iter().map(|plan| plan.record.clone()).collect();
    let active = active_plan(&records, deps.clock.now().date_naive())
        .and_then(|record| plans.iter().find(|each| each.record.id == record.id));
    let lineage = active.map(|each| lineage_of(&plans, &each.record.id));
    let view = active
        .zip(lineage.as_deref())
        .map(|(each, lineage)| Lineage {
            id: each.record.id.as_str(),
            plan: &each.proposal,
            lineage,
        });
    let for_end: Vec<String> = campaigns_paused(&deps.log)?
        .into_iter()
        .filter(|pause| pause.why == PausedWhy::PlanEnded)
        .map(|pause| pause.campaign)
        .collect();
    let made = created_campaigns(&deps.log)?;
    Ok(to_pause_for_end(view.as_ref(), &made, &for_end)
        .iter()
        .any(|each| each.plan == plan))
}

/// `words` as a sentence: a full stop after them unless they end in one, or in a quotation.
fn sentence(words: &str) -> String {
    let words = words.trim();
    if words.ends_with(['.', '!', '?', '”']) {
        words.to_string()
    } else {
        format!("{words}.")
    }
}

fn time(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// The campaign `key` of `plan` by its name, else by the key.
fn name_of(plan: &MarketingPlan, key: &str) -> String {
    plan.proposal
        .campaigns
        .iter()
        .find(|each| each.key == key)
        .map_or_else(|| key.to_string(), |each| each.name.clone())
}

/// The fields every row about `plan` has.
fn base(plan: &MarketingPlan, kind: &str, title: &str, line: &str) -> Value {
    json!({
        "task_id": plan.task_id,
        "kind": kind,
        "agent_id": plan.agent_id,
        "title": format!("{title}: {}", plan.proposal.title),
        "line": line,
        "plan": plan.record.id,
        "plan_title": plan.proposal.title,
        "ends_on": plan.proposal.ends_on.to_string(),
        "currency": plan.proposal.currency,
    })
}

/// The row for a plan whose ads Farik could not pause.
fn running_row(plan: &MarketingPlan, why: &str) -> Value {
    let line = format!(
        "Farik could not pause its ads: {} They keep running at Google until {} or their \
         budget there. Pause them in Google Ads.",
        sentence(why),
        plan.proposal.ends_on
    );
    let mut row = base(plan, "marketing_ads_running", "Ads still running", &line);
    row["reason"] = json!(why);
    row
}

/// The row for the active plan whose spend Farik cannot read.
fn unread_row(plan: &MarketingPlan, read: &SpendRead, why: &str) -> Value {
    let line = format!(
        "Farik can't read its ad spend: {} Any of its ads still running keep running at \
         Google until {} or their budget there; pause them in Google Ads.",
        sentence(why),
        plan.proposal.ends_on
    );
    let mut row = base(
        plan,
        "marketing_spend_unread",
        "Can't read the ad spend",
        &line,
    );
    row["google_ads"] = json!(plan.proposal.google_ads.to_string());
    row["reason"] = json!(why);
    if let Some((spend, at)) = &read.spend {
        row["spent"] = json!(spend.total.to_string());
        row["read_at"] = json!(time(*at));
    }
    row
}

/// What the rows are made from.
struct Facts<'a> {
    deps: &'a ToolDeps,
    plans: Vec<MarketingPlan>,
    reached: Vec<BudgetReached>,
    paused: Vec<CampaignPaused>,
    made: Vec<farik_core::marketing::CreatedCampaign>,
    reads: BTreeMap<String, SpendRead>,
    state: &'a DaemonState,
}

impl Facts<'_> {
    /// The campaigns of `cap` the plan `plan` carries that no pause after it is recorded for.
    fn unpaused(&self, plan: &MarketingPlan, cap: &BudgetReached) -> Vec<String> {
        let lineage = lineage_of(&self.plans, &plan.record.id);
        let active = Lineage {
            id: plan.record.id.as_str(),
            plan: &plan.proposal,
            lineage: &lineage,
        };
        self.made
            .iter()
            .filter(|each| is_carried(&active, each))
            .filter(|each| match cap.scope {
                CapScope::Plan => true,
                CapScope::Campaign => Some(&each.key) == cap.key.as_ref(),
            })
            .map(|each| each.campaign.clone())
            .filter(|campaign| {
                let later_cap = self
                    .reached
                    .iter()
                    .any(|later| later.seq > cap.seq && later.paused.contains(campaign));
                let later_pause = self
                    .paused
                    .iter()
                    .any(|later| later.seq > cap.seq && &later.campaign == campaign);
                !cap.paused.contains(campaign) && !later_cap && !later_pause
            })
            .collect()
    }

    /// The row for the active plan whose ads reached a budget.
    fn budget_row(
        &self,
        plan: &MarketingPlan,
        (caps, newest): (&[&BudgetReached], &BudgetReached),
        unstopped: Option<&str>,
    ) -> Result<Value, StoreError> {
        let id = plan.record.id.as_str();
        // A cap whose pause was refused and whose campaigns are not recorded paused since.
        let stuck = caps
            .iter()
            .rev()
            .find(|cap| cap.failed.is_some() && !self.unpaused(plan, cap).is_empty());
        let (cap, trouble): (&BudgetReached, Option<&str>) = match (stuck, unstopped) {
            (Some(cap), _) => (cap, cap.failed.as_deref()),
            (None, why) => (newest, why),
        };
        let (subject, them) = match (&cap.scope, &cap.key) {
            (CapScope::Campaign, Some(key)) => (
                format!("Its campaign {} reached its budget", name_of(plan, key)),
                "it",
            ),
            _ => ("Its ads reached their budget".to_string(), "them"),
        };
        let amounts = format!("{} of {} {}", cap.spent, cap.budget, plan.proposal.currency);
        let line = match trouble {
            None => format!("{subject}: {amounts}. Farik paused {them}."),
            Some(why) => format!(
                "{subject}: {amounts}. Farik could not pause {them}: {} Farik tries again every \
                 15 minutes; pause {them} in Google Ads.",
                sentence(why)
            ),
        };
        let mut row = base(plan, "marketing_budget", "Ads budget reached", &line);
        let spend = known_spend(self.state, self.deps, id)?;
        row["google_ads"] = json!(plan.proposal.google_ads.to_string());
        row["spent"] = json!(spend.total.to_string());
        if let Some((_, at)) = self.reads.get(id).and_then(|read| read.spend.as_ref()) {
            row["read_at"] = json!(time(*at));
        }
        if let Some(why) = trouble {
            row["reason"] = json!(why);
        }
        let described = |cap: &BudgetReached| {
            let mut each = json!({
                "scope": cap.scope.as_str(),
                "spent": cap.spent.to_string(),
                "budget": cap.budget.to_string(),
            });
            if let (CapScope::Campaign, Some(key)) = (&cap.scope, &cap.key) {
                each["key"] = json!(key);
                each["name"] = json!(name_of(plan, key));
            }
            each
        };
        // The budget the line is about, which a refused pause can keep from being the newest.
        row["cap"] = described(cap);
        row["caps"] = json!(caps.iter().map(|cap| described(cap)).collect::<Vec<_>>());
        row["campaigns"] = json!(
            plan.proposal
                .campaigns
                .iter()
                .map(|each| json!({
                    "key": each.key,
                    "name": each.name,
                    "budget": each.budget.to_string(),
                    "spent": spend
                        .by_key
                        .get(&each.key)
                        .map_or_else(|| "0.00".to_string(), ToString::to_string),
                }))
                .collect::<Vec<_>>()
        );
        if let Some(task) = open_raise(self.deps, id)? {
            row["raising"] = json!(task);
        }
        Ok(row)
    }
}

/// The rows of `waiting.list` about the plans' ads, one per plan, by precedence: the active plan
/// whose ads reached a budget, else a plan whose ads Farik could not pause, else the active plan
/// whose spend Farik cannot read. A plan whose ads reached a budget and could not be paused for
/// its end or Google Ads' removal says so in the first. The active plan's come first.
///
/// # Errors
///
/// What the log or the board refused.
pub(crate) fn ads_rows(state: &DaemonState, deps: &ToolDeps) -> Result<Vec<Value>, StoreError> {
    let plans = marketing_plans(&deps.log)?;
    let records: Vec<PlanRecord> = plans.iter().map(|plan| plan.record.clone()).collect();
    let active = active_plan(&records, deps.clock.now().date_naive()).map(|plan| plan.id.clone());
    let facts = Facts {
        deps,
        reached: budgets_reached(&deps.log)?,
        paused: campaigns_paused(&deps.log)?,
        made: created_campaigns(&deps.log)?,
        reads: state.spend_reads().clone(),
        plans,
        state,
    };
    let mut rows = Vec::new();
    for plan in &facts.plans {
        let id = &plan.record.id;
        let is_active = active.as_ref() == Some(id);
        let read = facts.reads.get(id);
        let unstopped = read.and_then(|read| read.unstopped.as_deref());
        let caps: Vec<&BudgetReached> =
            facts.reached.iter().filter(|cap| &cap.plan == id).collect();
        let row = if let (true, Some(newest)) = (is_active, caps.last()) {
            Some(facts.budget_row(plan, (&caps, newest), unstopped)?)
        } else if let Some(why) = unstopped {
            Some(running_row(plan, why))
        } else if let (true, Some(read), Some((why, _))) =
            (is_active, read, read.and_then(|read| read.failed.as_ref()))
        {
            Some(unread_row(plan, read, why))
        } else {
            None
        };
        if let Some(row) = row {
            rows.push((is_active, row));
        }
    }
    // The active plan's first.
    rows.sort_by_key(|(is_active, _)| !is_active);
    Ok(rows.into_iter().map(|(_, row)| row).collect())
}

/// What `marketing_plan.get` adds for `plan`: `spend`, when the watch has tried, `reached`, the
/// budgets its ads reached, and `paused`, the campaigns Farik paused on its own.
///
/// # Errors
///
/// What the log refused.
pub(crate) fn spend_and_pauses(
    state: &DaemonState,
    deps: &ToolDeps,
    plan: &MarketingPlan,
) -> Result<Map<String, Value>, StoreError> {
    let id = plan.record.id.as_str();
    let mut more = Map::new();
    let read = state.spend_reads().get(id).cloned();
    let mut spend = Map::new();
    if let Some(read) = &read {
        if let Some((seen, at)) = &read.spend {
            spend.insert("read_at".to_string(), json!(time(*at)));
            spend.insert("total".to_string(), json!(seen.total.to_string()));
            spend.insert(
                "by_key".to_string(),
                json!(
                    seen.by_key
                        .iter()
                        .map(|(key, amount)| (key.clone(), amount.to_string()))
                        .collect::<BTreeMap<_, _>>()
                ),
            );
        }
        if let Some((why, at)) = &read.failed {
            spend.insert("failed".to_string(), json!(why));
            spend.insert("failed_at".to_string(), json!(time(*at)));
        }
    }
    if !spend.is_empty() {
        more.insert("spend".to_string(), Value::Object(spend));
    }
    let reached: Vec<BudgetReached> = budgets_reached(&deps.log)?
        .into_iter()
        .filter(|cap| cap.plan == id)
        .collect();
    more.insert(
        "reached".to_string(),
        json!(
            reached
                .iter()
                .map(|cap| {
                    let mut each = json!({
                        "scope": cap.scope.as_str(),
                        "spent": cap.spent.to_string(),
                        "budget": cap.budget.to_string(),
                        "at": time(cap.at),
                    });
                    if let Some(key) = &cap.key {
                        each["key"] = json!(key);
                    }
                    if let Some(failed) = &cap.failed {
                        each["failed"] = json!(failed);
                    }
                    each
                })
                .collect::<Vec<_>>()
        ),
    );
    // Each campaign's pauses, the budget's own among them, once for a campaign and a reason.
    let made = created_campaigns(&deps.log)?;
    let key_of = |campaign: &str| {
        made.iter()
            .find(|each| each.campaign == campaign)
            .map(|each| each.key.clone())
    };
    let mut pauses: Vec<(DateTime<Utc>, u64, String, String, &'static str)> = Vec::new();
    for cap in &reached {
        for campaign in &cap.paused {
            if let Some(key) = key_of(campaign) {
                pauses.push((
                    cap.at,
                    cap.seq,
                    campaign.clone(),
                    key,
                    PausedWhy::BudgetReached.as_str(),
                ));
            }
        }
    }
    for pause in campaigns_paused(&deps.log)?
        .into_iter()
        .filter(|pause| pause.plan == id)
    {
        pauses.push((
            pause.at,
            pause.seq,
            pause.campaign,
            pause.key,
            pause.why.as_str(),
        ));
    }
    pauses.sort_by_key(|(at, seq, ..)| (*at, *seq));
    let mut seen: Vec<(String, &str)> = Vec::new();
    let mut listed = Vec::new();
    for (at, _, campaign, key, why) in pauses {
        if seen.contains(&(campaign.clone(), why)) {
            continue;
        }
        listed.push(json!({
            "key": key, "name": name_of(plan, &key), "why": why, "at": time(at),
        }));
        seen.push((campaign, why));
    }
    more.insert("paused".to_string(), json!(listed));
    Ok(more)
}
