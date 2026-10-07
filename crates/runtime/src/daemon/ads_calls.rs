//! Google Ads calls in the daemon (`docs/SPEC.md` 6.7, ADR 0038, ADR 0042). `farik connector
//! google-ads` is a shim: it lists the ten tools and forwards each call here, to
//! `POST /connector/call`, with the ticket the launch route gave it. The daemon holds the grant,
//! checks the plan against the log, calls Google, and records what it created, so no token ever
//! leaves it and a plan the owner ends takes effect on the next call.
//!
//! Google Ads calls never go through `call_as` (`own_calls.rs`): the daemon is the server, so this
//! module calls [`GoogleAds`] with the session's agent's grant. Its fixed operations are its list:
//! the ten tools, the route's ad-group and spend reads, and step 08g's spend read and pause.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as Days, NaiveDate};
use farik_core::contract::Role;
use farik_core::governor::permissions::ConnectorTag;
use farik_core::marketing::{
    AdsPlanView, AdsWrite, Amount, BudgetKind, CreatedCampaign, HeldAtGoogle, PlanProposal,
    PlanRecord, ZERO_DECIMAL, active_plan, campaign_budget, check_ads_write, first_day,
};
use farik_core::team::{CustomServer, custom_server};
use farik_protocol::event::{
    EventBody, EventIds, MarketingCampaignCreatedBody, MarketingCampaignCreatedBodyBudgetKind,
};
use farik_store::marketing::{created_campaigns_on, marketing_plans};
use serde_json::{Value, json};

use super::hooks::append;
use super::{DaemonState, Fresh, Ticketed, matches_kit, refreshed_entry};
use crate::claude::Secret;
use crate::connectors::SecretAt;
use crate::google_ads::{
    AdGroupInput, AdInput, BudgetInput, CUSTOMER_CURRENCY_QUERY, CampaignInput, GoogleAds,
    GoogleAdsError, KeywordsInput, NewCampaign, READ_TOOLS, Status, StatusInput, WRITE_TOOLS,
    ad_group_campaign_query, ad_group_operations, ad_operations, budget_operations,
    campaign_operations, customer_of, dashed, held_by_campaign, keyword_ideas, keyword_operations,
    list_accounts, negative_keyword_operations, report, resource_customer, spend_by_campaign,
    spend_query, status_operations,
};
use crate::tools::ToolDeps;

/// The name of Farik's Google Ads connector, in a kit and in a team file.
pub(crate) const GOOGLE_ADS: &str = "google-ads";

/// How long a sign-in must stay good for a call to be made on it: the route makes up to four calls
/// to Google of 25 seconds each.
const VALID_FOR: Duration = Duration::from_secs(120);
/// How long a refresh may take before the call goes on with the sign-in as it is, while it holds.
const REFRESH_WAIT: Duration = Duration::from_secs(10);

/// A refusal, `<code>: <words>`, as the route answers it.
type Refusal = String;

/// Google's refusal or fault, or an input that is not valid, in the route's words.
fn said(error: GoogleAdsError) -> Refusal {
    match error {
        GoogleAdsError::Input(words) => format!("google_ads_input: {words}"),
        GoogleAdsError::Google(words) | GoogleAdsError::NotAllowed(words) => {
            format!("google_ads_refused: {words}")
        }
        GoogleAdsError::Failed(words) => format!("google_ads_failed: {words}"),
    }
}

/// What a call may use once the session, the entry and the tool have been checked.
struct Access {
    /// The agent's `google-ads` entry, which is exactly the kit's.
    definition: CustomServer,
    /// Where its grant is kept.
    at: SecretAt,
    /// Whether the tool is one of the seven writes.
    write: bool,
}

/// Runs one tool of the connector, for the session `ticket` names and as its agent, or says why it
/// does not. In order: the ticket (`unauthorized`) and its session (`session_stopped`); the
/// session's connector (`connector_not_in_session`); the agent's entry being the kit's
/// (`google_ads_not_kit`); the tool being tagged, not denied, and for a write marked as the plan's
/// (`tool_not_tagged`, `tool_denied`, `tool_not_plan_marked`); for a write, the active plan
/// (`no_active_marketing_plan`); the agent's grant (`sign_in_again`); the input
/// (`google_ads_input`); for a write, what the plan covers (`not_in_marketing_plan`); then Google.
///
/// # Errors
///
/// The refusal, as `<code>: <words>`.
pub(crate) async fn ads_call(
    state: &Arc<DaemonState>,
    ticket: &str,
    tool: &str,
    arguments: Value,
) -> Result<Value, String> {
    let held = state
        .ticketed(ticket)
        .ok_or_else(|| "unauthorized: no live session holds that ticket".to_string())?;
    if let Some(reason) = &held.stop_reason {
        return Err(format!("session_stopped: {reason}"));
    }
    let deps = state
        .deps()
        .ok_or_else(|| format!("no_project: {}", super::NO_PROJECT))?
        .clone();
    let access = access_of(state, &deps, &held, tool)?;
    let ads = GoogleAds::new(state.google_ads_api()).map_err(said)?;
    if !access.write {
        let token = grant_of(state, &access).await?;
        return match tool {
            "list_accounts" => list_accounts(&ads, &token).await.map_err(said),
            "report" => report(&ads, &token, &arguments).await.map_err(said),
            _ => keyword_ideas(&ads, &token, &arguments).await.map_err(said),
        };
    }
    // One write at a time, from reading the plan and what was made through Google's answer and
    // the record of what it made: two creates for one key would both pass the check otherwise.
    let _writing = state.ads_writes().lock().await;
    let today = deps.clock.now().date_naive();
    let plan = read_plan(&deps, today)?;
    let token = grant_of(state, &access).await?;
    let created = created_campaigns_on(&deps.log)
        .map_err(|error| format!("marketing_campaign_unreadable: {error}"))?;
    let writing = Writing {
        deps: &deps,
        held: &held,
        ads: &ads,
        token: &token,
        plan,
        created,
        today,
    };
    writing.run(tool, &arguments).await
}

/// The session's connector, the agent's entry and the tool, checked.
fn access_of(
    state: &Arc<DaemonState>,
    deps: &ToolDeps,
    held: &Ticketed,
    tool: &str,
) -> Result<Access, Refusal> {
    let not_in_session = || {
        format!(
            "connector_not_in_session: the session {} was not given {GOOGLE_ADS}",
            held.session_id
        )
    };
    let connector = (held.server == GOOGLE_ADS)
        .then(|| {
            held.connectors
                .iter()
                .find(|given| given.server == GOOGLE_ADS)
        })
        .flatten()
        .ok_or_else(not_in_session)?;
    let team = deps
        .files
        .read_team()
        .map_err(|error| format!("team_unreadable: {error}"))?;
    let agent = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == held.agent_id)
        .ok_or_else(not_in_session)?;
    // A custom entry naming the same command, whatever its tags, is not the kit's and reaches
    // nothing: only the kit's own entry is trusted to be Farik's server (ADR 0038).
    let not_kit = || {
        format!(
            "google_ads_not_kit: {GOOGLE_ADS} is not exactly as the kit has it; connect it again \
             from the agent's page"
        )
    };
    let definition = agent
        .mcp_servers
        .iter()
        .flatten()
        .filter(|entry| entry.name.as_str() == GOOGLE_ADS)
        .find_map(custom_server)
        .ok_or_else(not_kit)?;
    let kit =
        (deps.kits)(Role::from(agent.role)).map_err(|error| format!("kit_unreadable: {error}"))?;
    if !matches_kit(&kit, &definition) {
        return Err(not_kit());
    }
    let write = WRITE_TOOLS.contains(&tool);
    match connector.tools.get(tool) {
        None => {
            return Err(format!(
                "tool_not_tagged: {tool} is not in {GOOGLE_ADS}'s pinned list of tools"
            ));
        }
        Some(ConnectorTag::Denied) => {
            return Err(format!(
                "tool_denied: {tool} is tagged denied, and no session may call it"
            ));
        }
        Some(tag) if write => {
            if *tag != ConnectorTag::ExternalEffect || !connector.plan_tools.contains(tool) {
                return Err(format!(
                    "tool_not_plan_marked: {tool} is not marked as approved by a marketing plan, so \
                     it does not run"
                ));
            }
        }
        Some(tag) => {
            if *tag != ConnectorTag::Network || !READ_TOOLS.contains(&tool) {
                return Err(format!(
                    "tool_not_tagged: {tool} is not one of {GOOGLE_ADS}'s reads"
                ));
            }
        }
    }
    let at = state
        .secret_at(deps.files.root(), &held.agent_id, GOOGLE_ADS)
        .map_err(|error| format!("secret_store_unavailable: {error}"))?;
    Ok(Access {
        definition,
        at,
        write,
    })
}

/// The agent's access token, its sign-in refreshed first when it will not last the call.
async fn grant_of(state: &Arc<DaemonState>, access: &Access) -> Result<Secret, Refusal> {
    let entry = refreshed_entry(
        state,
        &access.at,
        &access.definition,
        VALID_FOR,
        REFRESH_WAIT,
        true,
    )
    .await
    .map_err(|fresh| match fresh {
        Fresh::Lapsed => {
            format!("sign_in_again: Google ended Farik's sign-in to {GOOGLE_ADS}; sign in again")
        }
        Fresh::NotConfirmed => format!(
            "connector_not_confirmed: {GOOGLE_ADS} is not as it was connected on this computer; \
             connect it again"
        ),
        Fresh::Failed(why) => format!("sign_in_failed: {why}"),
        Fresh::Store(why) => format!("secret_store_unavailable: {why}"),
    })?;
    entry.oauth.map(|grant| grant.access_token).ok_or_else(|| {
        format!(
            "connector_not_confirmed: {GOOGLE_ADS} is not as it was connected on this computer; \
             connect it again"
        )
    })
}

/// The active plan, its proposal, and its lineage: itself and every plan it replaces, `replaces`
/// followed through the whole chain.
struct ActivePlan {
    id: String,
    proposal: PlanProposal,
    lineage: Vec<String>,
}

/// The plan active on `today`, from the log, or the refusal that none is.
fn read_plan(deps: &ToolDeps, today: NaiveDate) -> Result<ActivePlan, Refusal> {
    let plans = marketing_plans(&deps.log)
        .map_err(|error| format!("marketing_plan_unreadable: {error}"))?;
    let records: Vec<PlanRecord> = plans.iter().map(|plan| plan.record.clone()).collect();
    let none = || {
        "no_active_marketing_plan: Google Ads changes run only inside a marketing plan the owner \
         approved"
            .to_string()
    };
    let active = active_plan(&records, today).ok_or_else(none)?;
    let proposal = |id: &str| {
        plans
            .iter()
            .find(|plan| plan.record.id == id)
            .map(|plan| plan.proposal.clone())
    };
    let mut lineage = vec![active.id.clone()];
    let mut next = proposal(&active.id).and_then(|plan| plan.replaces);
    while let Some(id) = next {
        if lineage.contains(&id) {
            break;
        }
        next = proposal(&id).and_then(|plan| plan.replaces);
        lineage.push(id);
    }
    Ok(ActivePlan {
        id: active.id.clone(),
        proposal: proposal(&active.id).ok_or_else(none)?,
        lineage,
    })
}

/// What one read of the lineage's campaigns in an ad account answers: what each plan campaign's
/// key has spent, and what Google holds for each campaign.
#[derive(Default)]
struct Spend {
    /// By the plan campaign's key, in hundredths.
    by_key: BTreeMap<String, Amount>,
    /// By the campaign's resource name.
    held: BTreeMap<String, HeldAtGoogle>,
}

/// A write, with what it is checked against, read under the writes' lock.
struct Writing<'a> {
    deps: &'a ToolDeps,
    held: &'a Ticketed,
    ads: &'a GoogleAds,
    token: &'a Secret,
    plan: ActivePlan,
    created: Vec<(CreatedCampaign, NaiveDate)>,
    today: NaiveDate,
}

impl Writing<'_> {
    async fn run(&self, tool: &str, arguments: &Value) -> Result<Value, Refusal> {
        match tool {
            "create_search_campaign" => self.create(arguments).await,
            "add_ad_group" => {
                let input = AdGroupInput::parse(arguments).map_err(said)?;
                let account = account_of(&input.campaign)?;
                self.covers(
                    &AdsWrite::UnderCampaign {
                        account: account.clone(),
                        campaign: input.campaign.clone(),
                    },
                    &BTreeMap::new(),
                )?;
                let made = self.mutate(&account, ad_group_operations(&input)).await?;
                Ok(json!({ "ad_group": made.first() }))
            }
            "add_keywords" => {
                let input = KeywordsInput::parse(arguments, "ad_group").map_err(said)?;
                let account = account_of(&input.parent)?;
                let campaign = self.campaign_of(&account, &input.parent).await?;
                self.covers(
                    &AdsWrite::UnderCampaign {
                        account: account.clone(),
                        campaign,
                    },
                    &BTreeMap::new(),
                )?;
                self.mutate(&account, keyword_operations(&input)).await?;
                Ok(json!({ "added": input.keywords.len() }))
            }
            "add_negative_keywords" => {
                let input = KeywordsInput::parse(arguments, "campaign").map_err(said)?;
                let account = account_of(&input.parent)?;
                self.covers(
                    &AdsWrite::UnderCampaign {
                        account: account.clone(),
                        campaign: input.parent.clone(),
                    },
                    &BTreeMap::new(),
                )?;
                self.mutate(&account, negative_keyword_operations(&input))
                    .await?;
                Ok(json!({ "added": input.keywords.len() }))
            }
            "add_responsive_search_ad" => {
                let input = AdInput::parse(arguments).map_err(said)?;
                let account = account_of(&input.ad_group)?;
                let campaign = self.campaign_of(&account, &input.ad_group).await?;
                self.covers(
                    &AdsWrite::UnderCampaign {
                        account: account.clone(),
                        campaign,
                    },
                    &BTreeMap::new(),
                )?;
                let made = self.mutate(&account, ad_operations(&input)).await?;
                Ok(json!({ "ad": made.first() }))
            }
            "set_campaign_budget" => self.set_budget(arguments).await,
            _ => self.set_status(arguments).await,
        }
    }

    /// Whether the active plan covers `write`, as the refusal `not_in_marketing_plan` says.
    fn covers(&self, write: &AdsWrite, spent: &BTreeMap<String, Amount>) -> Result<(), Refusal> {
        let created: Vec<CreatedCampaign> =
            self.created.iter().map(|(made, _)| made.clone()).collect();
        check_ads_write(
            &AdsPlanView {
                plan_id: &self.plan.id,
                plan: &self.plan.proposal,
                lineage: &self.plan.lineage,
                created: &created,
                spent,
                today: self.today,
            },
            write,
        )
        .map_err(|why| format!("not_in_marketing_plan: {why}"))
    }

    /// One `googleAds:mutate`, its refusal in the route's words, sent only while the plan the
    /// write was checked against is still the active one: it is read again just before, since
    /// the owner may have ended or replaced it during the reads that came first.
    async fn mutate(&self, account: &str, operations: Vec<Value>) -> Result<Vec<String>, Refusal> {
        let still = read_plan(self.deps, self.deps.clock.now().date_naive())?;
        if still.id != self.plan.id {
            return Err(format!(
                "no_active_marketing_plan: the active plan is {} now, no longer {}, which this \
                 change was checked against, so it was not sent",
                still.id, self.plan.id
            ));
        }
        self.ads
            .mutate(self.token, account, operations)
            .await
            .map_err(said)
    }

    /// The campaign an ad group belongs to, read from Google.
    async fn campaign_of(&self, account: &str, ad_group: &str) -> Result<String, Refusal> {
        let query = ad_group_campaign_query(ad_group).map_err(said)?;
        let rows = self
            .ads
            .search(self.token, account, &query)
            .await
            .map_err(said)?;
        rows.first()
            .and_then(|row| row["adGroup"]["campaign"].as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                "not_in_marketing_plan: that ad group is not in the plan's ad account".to_string()
            })
    }

    /// What each plan campaign's key has spent so far, in all the campaigns of the lineage in
    /// `account`: one `Search` of their cost from the day before the first was made to tomorrow,
    /// rounded up from micros to hundredths; and what budget and end Google holds for each
    /// campaign, which the same rows say.
    async fn spend(&self, account: &str) -> Result<Spend, Refusal> {
        let customer = customer_of(account).map_err(said)?;
        let ours: Vec<&(CreatedCampaign, NaiveDate)> = self
            .created
            .iter()
            .filter(|(made, _)| {
                self.plan.lineage.contains(&made.plan)
                    && resource_customer(&made.campaign, "campaigns").as_deref() == Some(&customer)
            })
            .collect();
        let Some(first) = ours.iter().map(|(_, day)| *day).min() else {
            return Ok(Spend::default());
        };
        let names: Vec<String> = ours.iter().map(|(made, _)| made.campaign.clone()).collect();
        let unread = |why: GoogleAdsError| {
            format!(
                "not_in_marketing_plan: Farik could not read what the plan's ads have spent, so it \
                 cannot check the budget: {why}"
            )
        };
        let query = spend_query(&names, first - Days::days(1), self.today + Days::days(1))
            .map_err(unread)?;
        let rows = self
            .ads
            .search(self.token, account, &query)
            .await
            .map_err(unread)?;
        let micros = spend_by_campaign(&rows);
        let mut spent: BTreeMap<String, u64> = BTreeMap::new();
        let mut held = BTreeMap::new();
        let rows_held = held_by_campaign(&rows);
        for (made, _) in ours {
            *spent.entry(made.key.clone()).or_insert(0) +=
                micros.get(&made.campaign).copied().unwrap_or(0);
            // A total budget's amount is its total, a daily one's its daily amount.
            let row = rows_held.get(&made.campaign).copied().unwrap_or_default();
            let amount = match made.kind {
                BudgetKind::Total => row.total_micros,
                BudgetKind::Daily => row.daily_micros,
            };
            held.insert(
                made.campaign.clone(),
                HeldAtGoogle {
                    amount: amount.map(|micros| Amount(micros.div_ceil(10_000))),
                    starts_on: row.starts_on,
                    ends_on: row.ends_on,
                },
            );
        }
        Ok(Spend {
            by_key: spent
                .into_iter()
                .map(|(key, cost)| (key, Amount(cost.div_ceil(10_000))))
                .collect(),
            held,
        })
    }

    /// Refuses unless the ad account bills in the plan's currency. The plan's figures are in it,
    /// so a budget, or the running of a campaign that holds one, is for an ad account that bills
    /// in it, whatever plan came before: a replacing plan in another currency would otherwise
    /// send its amounts to Google as the account's.
    async fn bills_in_the_plans_currency(&self, account: &str) -> Result<(), Refusal> {
        let plan_currency = &self.plan.proposal.currency;
        let rows = self
            .ads
            .search(self.token, account, CUSTOMER_CURRENCY_QUERY)
            .await
            .map_err(said)?;
        let account_currency = rows
            .first()
            .and_then(|row| row["customer"]["currencyCode"].as_str())
            .unwrap_or_default();
        if account_currency == plan_currency {
            return Ok(());
        }
        Err(format!(
            "not_in_marketing_plan: the plan is in {plan_currency}, but the ad account {account} \
             bills in {}",
            if account_currency.is_empty() {
                "a currency Farik could not read"
            } else {
                account_currency
            }
        ))
    }

    /// The record of a campaign Farik made for the lineage.
    fn made(&self, campaign: &str) -> Result<&CreatedCampaign, Refusal> {
        self.created
            .iter()
            .map(|(made, _)| made)
            .find(|made| made.campaign == campaign && self.plan.lineage.contains(&made.plan))
            .ok_or_else(|| {
                "not_in_marketing_plan: that campaign was not made for the active plan".to_string()
            })
    }

    async fn create(&self, arguments: &Value) -> Result<Value, Refusal> {
        let input = CampaignInput::parse(arguments).map_err(said)?;
        self.covers(
            &AdsWrite::Create {
                account: input.account.clone(),
                plan_campaign: input.plan_campaign.clone(),
            },
            &BTreeMap::new(),
        )?;
        let planned = self
            .plan
            .proposal
            .campaigns
            .iter()
            .find(|planned| planned.key == input.plan_campaign)
            .ok_or_else(|| {
                format!(
                    "not_in_marketing_plan: the active plan has no campaign {}",
                    input.plan_campaign
                )
            })?;
        self.bills_in_the_plans_currency(&input.account).await?;
        let plan_currency = &self.plan.proposal.currency;
        let (kind, amount) = campaign_budget(planned, plan_currency, Amount(0), self.today);
        let start = first_day(planned, self.today);
        let made = self
            .mutate(
                &input.account,
                campaign_operations(&NewCampaign {
                    input: &input,
                    plan: &self.plan.id,
                    start,
                    ends_on: planned.ends_on,
                    budget: (kind, amount),
                }),
            )
            .await?;
        let (Some(budget), Some(campaign)) = (made.first(), made.get(1)) else {
            return Err(
                "google_ads_failed: Google's answer does not name the campaign".to_string(),
            );
        };
        self.record(&input, (budget, campaign), (kind, amount))?;
        Ok(json!({
            "campaign": campaign, "budget": budget, "budget_kind": kind.as_str(),
            "amount": amount.to_string(), "status": "paused",
            "starts_on": start.to_string(), "ends_on": planned.ends_on.to_string(),
        }))
    }

    /// Records `marketing_campaign.created`, the session's agent, session and task on its
    /// envelope, or says that Google made the campaign and Farik could not keep its record.
    fn record(
        &self,
        input: &CampaignInput,
        (budget, campaign): (&String, &String),
        (kind, amount): (BudgetKind, Amount),
    ) -> Result<(), Refusal> {
        let unrecorded = |why: String| {
            format!(
                "record_failed: Google made {campaign}, paused, but Farik could not record it \
                 ({why}); find it in Google Ads and tell the owner"
            )
        };
        let body = MarketingCampaignCreatedBody {
            plan: self
                .plan
                .id
                .clone()
                .try_into()
                .map_err(|_| unrecorded("plan".to_string()))?,
            key: input
                .plan_campaign
                .clone()
                .try_into()
                .map_err(|_| unrecorded("key".to_string()))?,
            account: input
                .account
                .clone()
                .try_into()
                .map_err(|_| unrecorded("account".to_string()))?,
            campaign: campaign
                .clone()
                .try_into()
                .map_err(|_| unrecorded("campaign".to_string()))?,
            budget: budget
                .clone()
                .try_into()
                .map_err(|_| unrecorded("budget".to_string()))?,
            budget_kind: match kind {
                BudgetKind::Total => MarketingCampaignCreatedBodyBudgetKind::Total,
                BudgetKind::Daily => MarketingCampaignCreatedBodyBudgetKind::Daily,
            },
            amount: amount
                .to_string()
                .try_into()
                .map_err(|_| unrecorded("amount".to_string()))?,
        };
        let ids = EventIds {
            task_id: self.held.task_id.clone(),
            agent_id: Some(self.held.agent_id.clone()),
            session_id: Some(self.held.session_id.clone()),
            ..self.deps.ids.clone()
        };
        append(self.deps, ids, EventBody::MarketingCampaignCreated(body))
            .map(|_| ())
            .map_err(unrecorded)
    }

    async fn set_budget(&self, arguments: &Value) -> Result<Value, Refusal> {
        let input = BudgetInput::parse(arguments).map_err(said)?;
        let account = account_of(&input.campaign)?;
        // A currency with no minor unit takes whole units: what is sent is what is checked.
        let amount = if ZERO_DECIMAL.contains(&self.plan.proposal.currency.as_str()) {
            Amount(input.amount.0 / 100 * 100)
        } else {
            input.amount
        };
        if amount.0 == 0 {
            return Err(said(GoogleAdsError::Input(
                "amount is under one whole unit of a currency that has no minor unit".to_string(),
            )));
        }
        self.bills_in_the_plans_currency(&account).await?;
        let spent = self.spend(&account).await?;
        self.covers(
            &AdsWrite::Budget {
                account: account.clone(),
                campaign: input.campaign.clone(),
                amount,
            },
            &spent.by_key,
        )?;
        let made = self.made(&input.campaign)?;
        self.mutate(&account, budget_operations(&made.budget, made.kind, amount))
            .await?;
        Ok(json!({
            "campaign": input.campaign, "budget": made.budget,
            "budget_kind": made.kind.as_str(), "amount": amount.to_string(),
        }))
    }

    async fn set_status(&self, arguments: &Value) -> Result<Value, Refusal> {
        let input = StatusInput::parse(arguments).map_err(said)?;
        let account = account_of(&input.campaign)?;
        // Enabling needs what the ads have spent; pausing needs nothing, so it always runs.
        match input.status {
            Status::Enabled => {
                self.bills_in_the_plans_currency(&account).await?;
                let spent = self.spend(&account).await?;
                let held = spent.held.get(&input.campaign).copied().unwrap_or_default();
                self.covers(
                    &AdsWrite::Enable {
                        account: account.clone(),
                        campaign: input.campaign.clone(),
                        held,
                    },
                    &spent.by_key,
                )?;
            }
            Status::Paused => self.covers(
                &AdsWrite::Pause {
                    account: account.clone(),
                    campaign: input.campaign.clone(),
                },
                &BTreeMap::new(),
            )?,
        }
        self.mutate(&account, status_operations(&input.campaign, input.status))
            .await?;
        Ok(json!({
            "campaign": input.campaign,
            "status": if input.status == Status::Enabled { "enabled" } else { "paused" },
        }))
    }
}

/// The ad account, `NNN-NNN-NNNN`, a campaign's or an ad group's resource name is in.
fn account_of(resource: &str) -> Result<String, Refusal> {
    ["campaigns", "adGroups"]
        .iter()
        .find_map(|kind| resource_customer(resource, kind))
        .map(|customer| dashed(&customer))
        .ok_or_else(|| {
            said(GoogleAdsError::Input(
                "a resource name such as customers/1234567890/campaigns/123 is needed".to_string(),
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use farik_core::budget::DEFAULT_SESSION_LIMITS;
    use farik_core::contract::Role;
    use farik_core::governor::permissions::{PermissionTier, SessionConnector};
    use farik_core::team::CustomServer;
    use farik_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use super::{GOOGLE_ADS, ads_call};
    use crate::connectors::{ConnectorSecrets as _, MemoryConnectorSecrets, SecretAt};
    use crate::daemon::own_calls::fixtures::keep_a_sign_in;
    use crate::daemon::{DaemonState, SessionRegistration, plan_tools_of};
    use crate::google_ads::WRITE_TOOLS;
    use crate::google_ads_fixture::{Fixture as Google, Seen};
    use crate::oauth_fixture::Fixture as OAuth;
    use crate::orchestrator::fixtures::Harness;
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::with_the_marketing_specialist;

    const ACCOUNT: &str = "123-456-7890";
    const SESSION: &str = "session-ads";

    /// Kai's kit: Google Ads as `farik connector google-ads`, signing in, its three reads
    /// `network` and its seven writes `external_effect` marked as approved by the plan.
    fn ads_kit() -> farik_roles::Kit {
        let mut tools = serde_json::Map::new();
        for read in crate::google_ads::READ_TOOLS {
            tools.insert(read.to_string(), json!("network"));
        }
        for write in WRITE_TOOLS {
            tools.insert(write.to_string(), json!("external_effect"));
        }
        let kit = json!({
            "role": "marketing_specialist", "skills": [],
            "connectors": [{
                "name": GOOGLE_ADS, "transport": "stdio", "command": "farik",
                "args": ["connector", "google-ads"],
                "oauth": { "scopes": ["https://www.googleapis.com/auth/adwords"] },
                "title": "Google Ads", "about": "Shows your ads on Google.",
                "why": "To run the ads in your plan.", "setup": "Sign in with Google.",
                "tools": tools, "plan_approved": WRITE_TOOLS
            }]
        });
        farik_roles::parse_fixture_kit(Role::MarketingSpecialist, &kit.to_string(), &[], &[])
            .expect("the fixture kit loads")
    }

    /// Registers Kai's session, given `connector`, and gives its Google Ads server a ticket.
    fn register(harness: &Harness, connector: SessionConnector) -> String {
        harness.daemon.register_session(SessionRegistration {
            session_id: SESSION.to_string(),
            agent_id: "kai".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
            cwd: harness.project.repo.path.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: vec![PermissionTier::Read],
            connectors: vec![connector],
            preview: None,
        });
        harness
            .daemon
            .issue_ticket(SESSION, GOOGLE_ADS)
            .expect("a ticket")
            .expect("a live session")
    }

    /// Kai, connected to Google Ads as the kit has it and signed in, a session of Kai's holding
    /// the connector and its ticket, and Google Ads' API as a fixture.
    struct Ads {
        harness: Harness,
        google: Google,
        oauth: OAuth,
        store: Arc<MemoryConnectorSecrets>,
        server: CustomServer,
        at: SecretAt,
        kit: farik_roles::Kit,
        grant: crate::sign_in::OAuthGrant,
        ticket: String,
    }

    impl Ads {
        async fn new(name: &str) -> Self {
            Self::with(name, |_, _| {}).await
        }

        /// As `new`, with `change` made to the team's wire of Kai's entry and to the tags the
        /// session is given.
        async fn with(name: &str, change: impl FnOnce(&mut Value, &mut CustomServer)) -> Self {
            let google = Google::start().await;
            let oauth = OAuth::start().await;
            let harness = Harness::new(name, with_the_marketing_specialist);
            assert!(harness.daemon.set_google_ads_api(google.address.clone()));
            let kit = ads_kit();
            harness.project.set_kit(kit.clone());
            let files = &harness.project.deps.files;
            let team = files.read_team().expect("the team");
            let (entry, mut server) =
                crate::daemon::kit_entry(&kit, &team, "kai", GOOGLE_ADS, &BTreeMap::new())
                    .expect("the kit's service is kai's");
            let mut entry = entry;
            change(&mut entry, &mut server);
            let team = crate::daemon::with_server(&team, "kai", GOOGLE_ADS, Some(&entry))
                .expect("the entry is the team's");
            files.write_team(&team).expect("the team is written");
            let store = Arc::new(MemoryConnectorSecrets::default());
            assert!(
                harness
                    .daemon
                    .set_connector_secrets(Arc::clone(&store) as _)
            );
            let at = harness
                .daemon
                .secret_at(files.root(), "kai", GOOGLE_ADS)
                .expect("an address");
            let custom = farik_core::team::custom_server(
                &serde_json::from_value(entry).expect("a wire entry"),
            )
            .expect("a custom server");
            let grant = keep_a_sign_in(&store, (&custom, &at), &oauth, chrono::Duration::hours(1));
            let ticket = register(
                &harness,
                SessionConnector {
                    server: GOOGLE_ADS.to_string(),
                    origin: None,
                    tools: server.tools.clone(),
                    allowances: BTreeMap::new(),
                    plan_tools: plan_tools_of(&kit, &server),
                },
            );
            Self {
                harness,
                google,
                oauth,
                store,
                server: custom,
                at,
                kit,
                grant,
                ticket,
            }
        }

        fn state(&self) -> &Arc<DaemonState> {
            &self.harness.daemon
        }

        async fn call(&self, tool: &str, arguments: Value) -> Result<Value, String> {
            ads_call(self.state(), &self.ticket, tool, arguments).await
        }

        /// MP-`n`, proposed by Kai and approved by the owner, for the day of the fixture clock,
        /// 2026-09-22: `search-launch` of 500.00 to 2026-10-22 (a total budget, from two days
        /// ahead), and `search-long` of 400.00 to 2027-01-20 (a daily one), out of 1000.00 for
        /// Google Ads.
        fn plan(&self, plan: &str, replaces: Option<&str>) {
            self.plan_with(plan, replaces, |_| {});
        }

        /// As `plan`, with `change` made to the proposal's wire.
        fn plan_with(&self, plan: &str, replaces: Option<&str>, change: impl FnOnce(&mut Value)) {
            let mut body =
                farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
            body["plan"] = json!(plan);
            body["starts_on"] = json!("2026-09-20");
            body["ends_on"] = json!("2027-01-31");
            body["budget"] = json!({ "total": "1000.00", "google_ads": "1000.00" });
            body["posts"] = json!([]);
            body["google_ads_account"] = json!(ACCOUNT);
            if let Some(replaces) = replaces {
                body["replaces"] = json!(replaces);
            }
            let campaign = |key: &str, budget: &str, ends_on: &str| {
                json!({
                    "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
                    "budget": budget, "starts_on": "2026-09-22", "ends_on": ends_on
                })
            };
            body["campaigns"] = json!([
                campaign("search-launch", "500.00", "2026-10-22"),
                campaign("search-long", "400.00", "2027-01-20"),
            ]);
            change(&mut body);
            self.harness.project.record_by(
                Some("kai"),
                crate::tools::fixtures::at(),
                "FRK-1",
                "marketing_plan.proposed",
                &body,
            );
            self.harness.project.plan_approved("FRK-1", plan, "");
        }

        fn made(&self) -> Vec<farik_protocol::event::FarikEvent> {
            self.harness
                .project
                .events(&[EventKind::MarketingCampaignCreated])
        }

        fn mutates(&self) -> Vec<Seen> {
            self.google.requests_of("mutate")
        }
    }

    fn create(key: &str) -> Value {
        json!({
            "account": ACCOUNT, "plan_campaign": key, "name": "Launch",
            "bidding": "maximize_clicks", "max_cpc": "1.50",
            "locations": [2840], "languages": [1000]
        })
    }

    /// The refusal's code, before its colon.
    fn code(refusal: &str) -> &str {
        refusal.split(':').next().unwrap_or(refusal)
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_write_outside_the_plan_is_refused_and_records_nothing() {
        let ads = Ads::new("ads-outside-plan").await;
        ads.plan("MP-1", None);

        // A plan campaign the plan lacks.
        let refused = ads
            .call("create_search_campaign", create("search-zzz"))
            .await
            .expect_err("refused");
        assert!(refused.starts_with("not_in_marketing_plan: "), "{refused}");
        assert!(refused.contains("no campaign search-zzz"), "{refused}");

        // A campaign Farik did not make for the plan, and one in another account.
        for campaign in [
            "customers/1234567890/campaigns/77",
            "customers/5555555555/campaigns/77",
        ] {
            let refused = ads
                .call(
                    "add_ad_group",
                    json!({ "campaign": campaign, "name": "Boots" }),
                )
                .await
                .expect_err("refused");
            assert!(
                refused.starts_with("not_in_marketing_plan: "),
                "{campaign}: {refused}"
            );
        }
        assert!(ads.mutates().is_empty(), "Google saw no change");
        assert!(ads.made().is_empty(), "nothing was recorded");
    }

    /// The resource name Farik's answer to `create_search_campaign` gives the campaign.
    fn campaign_of(answer: &Value) -> String {
        answer["campaign"].as_str().expect("a campaign").to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_write_inside_the_plan_reaches_google_and_records_the_campaign() {
        let ads = Ads::new("ads-inside-plan").await;
        ads.plan("MP-1", None);
        let answer = ads
            .call("create_search_campaign", create("search-launch"))
            .await
            .expect("made");
        let campaign = campaign_of(&answer);
        assert!(
            campaign.starts_with("customers/1234567890/campaigns/"),
            "{answer}"
        );
        assert_eq!(answer["status"], json!("paused"));

        // The ad account's currency was read, and then the campaign was sent in one request,
        // paused, with the plan's budget as a total for its 30 days, with the agent's own grant.
        let queries: Vec<Value> = ads
            .google
            .requests_of("search")
            .iter()
            .map(|seen| seen.body["query"].clone())
            .collect();
        assert_eq!(
            queries,
            [json!("SELECT customer.currency_code FROM customer")]
        );
        let sent = ads.mutates();
        assert_eq!(sent.len(), 1);
        let operations = &sent[0].body["mutateOperations"];
        assert_eq!(
            operations[0]["campaignBudgetOperation"]["create"]["totalAmountMicros"],
            json!("500000000")
        );
        let made = &operations[1]["campaignOperation"]["create"];
        assert_eq!(made["status"], json!("PAUSED"));
        assert_eq!(made["name"], json!("MP-1 search-launch: Launch"));
        assert_eq!(made["startDateTime"], json!("2026-09-24 00:00:00"));
        assert_eq!(made["endDateTime"], json!("2026-10-22 23:59:59"));
        assert_eq!(
            sent[0].headers.get("authorization").map(String::as_str),
            Some(format!("Bearer {}", ads.grant.access_token.expose()).as_str()),
            "the agent's own grant"
        );

        // It was recorded, and the store reads it back.
        let recorded = ads.made();
        assert_eq!(recorded.len(), 1);
        let EventBody::MarketingCampaignCreated(body) = &recorded[0].body else {
            panic!("a campaign was made");
        };
        assert_eq!(
            (
                body.plan.as_str(),
                body.key.as_str(),
                body.account.as_str(),
                body.campaign.as_str(),
                body.amount.as_str()
            ),
            (
                "MP-1",
                "search-launch",
                ACCOUNT,
                campaign.as_str(),
                "500.00"
            )
        );
        let read = farik_store::marketing::created_campaigns(&ads.harness.project.deps.log)
            .expect("reads");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].campaign, campaign);
        assert_eq!(read[0].kind, farik_core::marketing::BudgetKind::Total);
        assert_eq!(recorded[0].envelope.ids.agent_id.as_deref(), Some("kai"));
        assert_eq!(
            recorded[0].envelope.ids.session_id.as_deref(),
            Some(SESSION)
        );

        // A plan campaign that runs 119 days from two days ahead takes a daily budget.
        ads.call("create_search_campaign", create("search-long"))
            .await
            .expect("made");
        let sent = ads.mutates();
        assert_eq!(
            sent[1].body["mutateOperations"][0]["campaignBudgetOperation"]["create"]["amountMicros"],
            json!("3360000")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn puts_what_goes_under_a_campaign_to_google_enabled() {
        let ads = Ads::new("ads-under-campaign").await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made"),
        );
        // Under the campaign: an ad group, keywords, negative keywords and an ad, each enabled.
        let group = ads
            .call(
                "add_ad_group",
                json!({ "campaign": campaign, "name": "Boots", "cpc_bid": "0.75" }),
            )
            .await
            .expect("an ad group");
        let ad_group = group["ad_group"].as_str().expect("an ad group").to_string();
        assert!(
            ad_group.starts_with("customers/1234567890/adGroups/"),
            "{group}"
        );
        ads.google.script(|script| {
            script.rows = vec![json!({ "adGroup": { "campaign": campaign } })];
        });
        ads.call(
            "add_keywords",
            json!({ "ad_group": ad_group, "keywords": [{ "text": "red boots", "match": "phrase" }] }),
        )
        .await
        .expect("keywords");
        ads.call(
            "add_negative_keywords",
            json!({ "campaign": campaign, "keywords": [{ "text": "free", "match": "broad" }] }),
        )
        .await
        .expect("negative keywords");
        ads.call(
            "add_responsive_search_ad",
            json!({
                "ad_group": ad_group, "headlines": ["One", "Two", "Three"],
                "descriptions": ["First description.", "Second description."],
                "final_url": "https://shop.example/boots"
            }),
        )
        .await
        .expect("an ad");
        let sent = ads.mutates();
        let created = |at: usize, operation: &str| {
            sent[at].body["mutateOperations"][0][operation]["create"].clone()
        };
        assert_eq!(created(1, "adGroupOperation")["status"], json!("ENABLED"));
        assert_eq!(
            created(1, "adGroupOperation")["type"],
            json!("SEARCH_STANDARD")
        );
        assert_eq!(
            created(2, "adGroupCriterionOperation")["status"],
            json!("ENABLED")
        );
        assert_eq!(
            created(3, "campaignCriterionOperation")["negative"],
            json!(true)
        );
        assert_eq!(created(4, "adGroupAdOperation")["status"], json!("ENABLED"));
        // The ad group's campaign was read for the two writes under an ad group.
        let reads: Vec<Value> = ads
            .google
            .requests_of("search")
            .iter()
            .map(|seen| seen.body["query"].clone())
            .filter(|query| {
                query
                    .as_str()
                    .is_some_and(|text| text.contains("ad_group.campaign"))
            })
            .collect();
        assert_eq!(reads.len(), 2);
        assert_eq!(ads.made().len(), 1, "only a campaign is recorded");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn two_creates_for_one_key_make_one_campaign() {
        let ads = Ads::new("ads-two-creates").await;
        ads.plan("MP-1", None);
        ads.google
            .script(|script| script.mutate_delay = Some(Duration::from_millis(400)));
        let (first, second) = tokio::join!(
            ads.call("create_search_campaign", create("search-launch")),
            ads.call("create_search_campaign", create("search-launch"))
        );
        let (made, refused): (Vec<_>, Vec<_>) =
            [first, second].into_iter().partition(Result::is_ok);
        assert_eq!(made.len(), 1, "one made it");
        let refusal = refused[0].clone().expect_err("the other was refused");
        assert!(refusal.starts_with("not_in_marketing_plan: "), "{refusal}");
        assert_eq!(ads.mutates().len(), 1, "only one reached Google");
        assert_eq!(ads.made().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one case after another, so the spend read between them stays in view"
    )]
    async fn changes_a_budget_and_a_status_inside_the_plan() {
        let ads = Ads::new("ads-budget-status").await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made"),
        );
        // A campaign of a plan outside the lineage, in the same account, is no part of the spend.
        ads.harness.project.record_by(
            Some("kai"),
            crate::tools::fixtures::at(),
            "FRK-1",
            "marketing_campaign.created",
            &json!({
                "plan": "MP-9", "key": "search-launch", "account": ACCOUNT,
                "campaign": "customers/1234567890/campaigns/4242",
                "budget": "customers/1234567890/campaignBudgets/4243",
                "budget_kind": "total", "amount": "500.00"
            }),
        );
        // What Google answers for the campaign: what it holds, as the create made it, and what it
        // cost.
        let spend = |micros: &str| {
            json!([{
                "campaign": {
                    "resourceName": campaign,
                    "startDateTime": "2026-09-24 00:00:00",
                    "endDateTime": "2026-10-22 23:59:59"
                },
                "campaignBudget": { "totalAmountMicros": "500000000" },
                "metrics": { "costMicros": micros }
            }])
        };
        ads.google.script(|script| {
            script.rows = serde_json::from_value(spend("99999999")).expect("rows");
        });

        // 99.999999 spent is 100.00 rounded up from micros, of 500.00: a new total between the
        // two passes, past either end does not.
        ads.call(
            "set_campaign_budget",
            json!({ "campaign": campaign, "amount": "400" }),
        )
        .await
        .expect("changed");
        let sent = ads.mutates();
        let update = &sent[1].body["mutateOperations"][0]["campaignBudgetOperation"];
        assert!(
            update["update"]["resourceName"]
                .as_str()
                .is_some_and(|name| name.starts_with("customers/1234567890/campaignBudgets/"))
        );
        assert_eq!(update["update"]["totalAmountMicros"], json!("400000000"));
        assert_eq!(update["updateMask"], json!("totalAmountMicros"));
        for amount in ["500.01", "99.99"] {
            let refused = ads
                .call(
                    "set_campaign_budget",
                    json!({ "campaign": campaign, "amount": amount }),
                )
                .await
                .expect_err("refused");
            assert!(
                refused.starts_with("not_in_marketing_plan: "),
                "{amount}: {refused}"
            );
        }
        ads.call(
            "set_campaign_budget",
            json!({ "campaign": campaign, "amount": "100" }),
        )
        .await
        .expect("as much as was spent, rounded up");
        // The spend was read in one search of the campaigns of the plan's lineage, over the days
        // from the day before the first was made to tomorrow.
        let query = ads
            .google
            .requests_of("search")
            .last()
            .map(|seen| seen.body["query"].clone())
            .expect("a read");
        assert_eq!(
            query,
            json!(format!(
                "SELECT campaign.resource_name, campaign.start_date_time, \
                 campaign.end_date_time, campaign_budget.amount_micros, \
                 campaign_budget.total_amount_micros, metrics.cost_micros FROM campaign WHERE \
                 campaign.resource_name IN ('{campaign}') AND segments.date BETWEEN \
                 '2026-09-21' AND '2026-09-23'"
            ))
        );

        // Enabling and pausing.
        ads.call(
            "set_campaign_status",
            json!({ "campaign": campaign, "status": "enabled" }),
        )
        .await
        .expect("enabled");
        let sent = ads.mutates();
        let last = &sent[sent.len() - 1].body["mutateOperations"][0]["campaignOperation"];
        assert_eq!(last["update"]["status"], json!("ENABLED"));
        assert_eq!(last["updateMask"], json!("status"));
        ads.google.script(|script| {
            script.rows = serde_json::from_value(spend("500000000")).expect("rows");
        });
        let refused = ads
            .call(
                "set_campaign_status",
                json!({ "campaign": campaign, "status": "enabled" }),
            )
            .await
            .expect_err("at its budget");
        assert!(refused.starts_with("not_in_marketing_plan: "), "{refused}");
        ads.call(
            "set_campaign_status",
            json!({ "campaign": campaign, "status": "paused" }),
        )
        .await
        .expect("paused, whatever was spent");
        let sent = ads.mutates();
        assert_eq!(
            sent[sent.len() - 1].body["mutateOperations"][0]["campaignOperation"]["update"]["status"],
            json!("PAUSED")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_failed_spend_read_refuses_budget_and_enable() {
        let ads = Ads::new("ads-spend-fails").await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made"),
        );
        ads.google.script(|script| {
            script.fail_search_containing = Some(("cost_micros".to_string(), 500));
        });
        for (tool, input) in [
            (
                "set_campaign_budget",
                json!({ "campaign": campaign, "amount": "400" }),
            ),
            (
                "set_campaign_status",
                json!({ "campaign": campaign, "status": "enabled" }),
            ),
        ] {
            let refused = ads.call(tool, input).await.expect_err("refused");
            assert!(
                refused.starts_with("not_in_marketing_plan: "),
                "{tool}: {refused}"
            );
            assert!(refused.contains("could not read what"), "{tool}: {refused}");
        }
        assert_eq!(
            ads.mutates().len(),
            1,
            "no change reached Google but the create"
        );
        // Pausing needs no spend, so it still runs.
        ads.call(
            "set_campaign_status",
            json!({ "campaign": campaign, "status": "paused" }),
        )
        .await
        .expect("paused");
        assert_eq!(ads.mutates().len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reads_run_without_a_plan() {
        let ads = Ads::new("ads-reads").await;
        let accounts = ads
            .call("list_accounts", json!({}))
            .await
            .expect("accounts");
        assert_eq!(accounts["accounts"][0]["account"], json!(ACCOUNT));
        ads.call(
            "report",
            json!({ "account": ACCOUNT, "kind": "campaigns", "from": "2026-09-01", "to": "2026-09-22" }),
        )
        .await
        .expect("a report");
        ads.call(
            "keyword_ideas",
            json!({ "account": ACCOUNT, "words": ["boots"], "language": 1000, "locations": [2840] }),
        )
        .await
        .expect("ideas");
        // A write with no plan is refused before anything is read.
        let refused = ads
            .call("create_search_campaign", create("search-launch"))
            .await
            .expect_err("no plan");
        assert_eq!(code(&refused), "no_active_marketing_plan", "{refused}");
        assert!(ads.mutates().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_lapsed_sign_in_says_sign_in_again() {
        let ads = Ads::new("ads-lapsed").await;
        let mut kept = ads.store.load(&ads.at).expect("readable").expect("kept");
        if let Some(grant) = &mut kept.oauth {
            grant.lapsed = true;
        }
        ads.store.save(&ads.at, &kept).expect("kept");
        let refused = ads
            .call("list_accounts", json!({}))
            .await
            .expect_err("lapsed");
        assert_eq!(code(&refused), "sign_in_again", "{refused}");
        assert!(ads.google.requests().is_empty(), "Google was not asked");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_sign_in_about_to_end_is_refreshed_first() {
        let ads = Ads::new("ads-refresh").await;
        keep_a_sign_in(
            &ads.store,
            (&ads.server, &ads.at),
            &ads.oauth,
            chrono::Duration::seconds(60),
        );
        ads.call("list_accounts", json!({}))
            .await
            .expect("accounts");
        assert_eq!(ads.oauth.count("/token"), 1, "one refresh");
        let now = ads
            .store
            .load(&ads.at)
            .expect("readable")
            .and_then(|entry| entry.oauth)
            .expect("a grant is kept");
        let seen = ads.google.requests();
        assert!(!seen.is_empty());
        assert!(
            seen.iter().all(|request| {
                request.headers.get("authorization").map(String::as_str)
                    == Some(format!("Bearer {}", now.access_token.expose()).as_str())
            }),
            "the refreshed grant was sent: {seen:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_custom_entry_is_not_the_kit_s() {
        // The same command, arguments and sign-in as the kit's, written as a custom entry whose
        // tags make every tool `network`: it is not the kit's, and reaches nothing.
        let ads = Ads::with("ads-custom", |entry, server| {
            entry["source"] = json!("custom");
            for tool in WRITE_TOOLS {
                entry["tools"][tool] = json!("network");
                server.tools.insert(
                    tool.to_string(),
                    farik_core::governor::permissions::ConnectorTag::Network,
                );
            }
            server.kit = false;
        })
        .await;
        ads.plan("MP-1", None);
        for (tool, input) in [
            ("create_search_campaign", create("search-launch")),
            ("list_accounts", json!({})),
        ] {
            let refused = ads.call(tool, input).await.expect_err("refused");
            assert_eq!(code(&refused), "google_ads_not_kit", "{tool}: {refused}");
        }
        assert!(ads.google.requests().is_empty(), "Google saw nothing");
        assert!(ads.made().is_empty());
        let _ = &ads.kit;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn refuses_in_the_order_the_route_checks() {
        let ads = Ads::new("ads-order").await;
        // A tool the connector does not list.
        let refused = ads
            .call("delete_campaign", json!({}))
            .await
            .expect_err("refused");
        assert_eq!(code(&refused), "tool_not_tagged", "{refused}");
        // A ticket made for another server of the session reaches nothing.
        let other = ads
            .state()
            .issue_ticket(SESSION, "other")
            .expect("a ticket")
            .expect("a live session");
        let refused = ads_call(ads.state(), &other, "list_accounts", json!({}))
            .await
            .expect_err("refused");
        assert_eq!(code(&refused), "connector_not_in_session", "{refused}");
        // No plan: refused before the input is read.
        let refused = ads
            .call("create_search_campaign", json!({}))
            .await
            .expect_err("no plan");
        assert_eq!(code(&refused), "no_active_marketing_plan", "{refused}");
        // A plan: an input that is not valid is refused before Google is asked.
        ads.plan("MP-1", None);
        let refused = ads
            .call(
                "create_search_campaign",
                json!({ "plan_campaign": "search-launch" }),
            )
            .await
            .expect_err("bad input");
        assert_eq!(code(&refused), "google_ads_input", "{refused}");
        let mut manual = create("search-launch");
        manual["bidding"] = json!("manual_cpc");
        let refused = ads
            .call("create_search_campaign", manual)
            .await
            .expect_err("manual");
        assert_eq!(code(&refused), "google_ads_input", "{refused}");
        assert!(ads.google.requests().is_empty(), "Google was not asked");
        // A plan in another currency than the ad account's: refused, and nothing was made.
        ads.google.script(|script| {
            script.customers[0].1["currencyCode"] = json!("EUR");
        });
        let refused = ads
            .call("create_search_campaign", create("search-launch"))
            .await
            .expect_err("currency");
        assert!(refused.starts_with("not_in_marketing_plan: "), "{refused}");
        assert!(
            refused.contains("USD") && refused.contains("EUR"),
            "{refused}"
        );
        assert!(ads.mutates().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_campaign_of_a_plan_it_replaces_counts_under_the_new_one() {
        let ads = Ads::new("ads-lineage").await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made under MP-1"),
        );
        // MP-2 replaces MP-1 and carries the same keys: the campaign is MP-2's to change, and a
        // second one for its key is refused.
        ads.plan("MP-2", Some("MP-1"));
        ads.call(
            "set_campaign_status",
            json!({ "campaign": campaign, "status": "paused" }),
        )
        .await
        .expect("MP-2 changes MP-1's campaign");
        let refused = ads
            .call("create_search_campaign", create("search-launch"))
            .await
            .expect_err("it has one");
        assert!(refused.starts_with("not_in_marketing_plan: "), "{refused}");
        assert_eq!(ads.made().len(), 1);
    }

    /// The route's status and body for a call with `ticket` as its bearer, or none.
    async fn over_http(
        ads: &Ads,
        ticket: Option<&str>,
        body: &Value,
    ) -> (axum::http::StatusCode, String) {
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use tokio_util::sync::CancellationToken;
        use tower::ServiceExt as _;

        let mut request =
            Request::post("/connector/call").header("content-type", "application/json");
        if let Some(ticket) = ticket {
            request = request.header("authorization", format!("Bearer {ticket}"));
        }
        let answer = crate::daemon::router(
            Arc::clone(ads.state()),
            "daemon-token",
            CancellationToken::new(),
        )
        .oneshot(
            request
                .body(Body::from(body.to_string()))
                .expect("a request"),
        )
        .await
        .expect("the router answers");
        let status = answer.status();
        let bytes = to_bytes(answer.into_body(), usize::MAX)
            .await
            .expect("a body");
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_call_needs_a_live_ticket() {
        use axum::http::StatusCode;

        let ads = Ads::new("ads-ticket").await;
        let call = json!({ "tool": "list_accounts", "arguments": {} });

        // The ticket answers, as a bearer and not as the daemon's token.
        let (status, body) = over_http(&ads, Some(&ads.ticket), &call).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answer: Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(
            answer["ok"]["accounts"][0]["account"],
            json!(ACCOUNT),
            "{body}"
        );

        // None, the daemon's own token and another ticket are not tickets.
        for given in [None, Some("daemon-token"), Some(&"0".repeat(64)[..])] {
            let (status, _) = over_http(&ads, given, &call).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{given:?}");
        }
        let before = ads.google.requests().len();

        // A call that is not one is answered, not run.
        let (status, body) = over_http(&ads, Some(&ads.ticket), &json!({ "nothing": 1 })).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("google_ads_input"), "{body}");

        // A stopped session's ticket is refused in the daemon's words, and runs nothing.
        assert!(
            ads.state()
                .request_stop(SESSION, "the owner paused the team")
        );
        let (status, body) = over_http(&ads, Some(&ads.ticket), &call).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answer: Value = serde_json::from_str(&body).expect("JSON");
        assert!(
            answer["error"]
                .as_str()
                .is_some_and(|error| error.starts_with("session_stopped: ")),
            "{body}"
        );
        assert_eq!(ads.google.requests().len(), before, "Google was not asked");

        // An ended session's ticket is no ticket at all.
        ads.state().end_session(SESSION);
        let (status, _) = over_http(&ads, Some(&ads.ticket), &call).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn runs_a_tool_only_as_the_session_was_given_it() {
        use farik_core::governor::permissions::ConnectorTag;

        let ads = Ads::new("ads-tags").await;
        ads.plan("MP-1", None);
        let given = |change: &dyn Fn(&mut SessionConnector)| {
            let mut connector = SessionConnector {
                server: GOOGLE_ADS.to_string(),
                origin: None,
                tools: ads.server.tools.clone(),
                allowances: BTreeMap::new(),
                plan_tools: plan_tools_of(&ads.kit, &ads.server),
            };
            change(&mut connector);
            register(&ads.harness, connector)
        };
        // A write the session's tags deny.
        let ticket = given(&|connector| {
            connector
                .tools
                .insert("create_search_campaign".to_string(), ConnectorTag::Denied);
        });
        let refused = ads_call(
            ads.state(),
            &ticket,
            "create_search_campaign",
            create("search-launch"),
        )
        .await
        .expect_err("denied");
        assert_eq!(code(&refused), "tool_denied", "{refused}");
        // A write the kit did not mark as the plan's, and one tagged as a read.
        let ticket = given(&|connector| connector.plan_tools.clear());
        let refused = ads_call(
            ads.state(),
            &ticket,
            "create_search_campaign",
            create("search-launch"),
        )
        .await
        .expect_err("unmarked");
        assert_eq!(code(&refused), "tool_not_plan_marked", "{refused}");
        let ticket = given(&|connector| {
            connector
                .tools
                .insert("create_search_campaign".to_string(), ConnectorTag::Network);
        });
        let refused = ads_call(
            ads.state(),
            &ticket,
            "create_search_campaign",
            create("search-launch"),
        )
        .await
        .expect_err("tagged as a read");
        assert_eq!(code(&refused), "tool_not_plan_marked", "{refused}");
        // A read tagged as a change is not run as one.
        let ticket = given(&|connector| {
            connector
                .tools
                .insert("list_accounts".to_string(), ConnectorTag::ExternalEffect);
        });
        let refused = ads_call(ads.state(), &ticket, "list_accounts", json!({}))
            .await
            .expect_err("not a read as tagged");
        assert_eq!(code(&refused), "tool_not_tagged", "{refused}");
        assert!(ads.google.requests().is_empty(), "Google was not asked");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_currency_with_no_minor_unit_gets_whole_units() {
        let ads = Ads::new("ads-yen").await;
        ads.plan("MP-1", None);
        // The plan is in yen, and so is the ad account.
        ads.harness.project.record_by(
            Some("kai"),
            crate::tools::fixtures::at(),
            "FRK-1",
            "marketing_plan.proposed",
            &yen_plan("MP-2"),
        );
        ads.harness.project.plan_approved("FRK-1", "MP-2", "");
        ads.google.script(|script| {
            script.customers[0].1["currencyCode"] = json!("JPY");
        });
        ads.call("create_search_campaign", create("search-launch"))
            .await
            .expect("made");
        let sent = ads.mutates();
        // 500.50 yen over 30 days is a total of 500 yen: whole units, rounded down.
        assert_eq!(
            sent[0].body["mutateOperations"][0]["campaignBudgetOperation"]["create"]["totalAmountMicros"],
            json!("500000000")
        );
        let campaign = ads.made();
        let EventBody::MarketingCampaignCreated(made) = &campaign[0].body else {
            panic!("a campaign was made");
        };
        assert_eq!(made.amount.as_str(), "500.00");
        // A new budget is whole units too, and what is sent is what the plan was asked about.
        ads.google.script(|script| script.rows = Vec::new());
        ads.call(
            "set_campaign_budget",
            json!({ "campaign": made.campaign.as_str(), "amount": "400.99" }),
        )
        .await
        .expect("changed");
        let sent = ads.mutates();
        assert_eq!(
            sent[1].body["mutateOperations"][0]["campaignBudgetOperation"]["update"]["totalAmountMicros"],
            json!("400000000")
        );
        let refused = ads
            .call(
                "set_campaign_budget",
                json!({ "campaign": made.campaign.as_str(), "amount": "0.99" }),
            )
            .await
            .expect_err("under a yen");
        assert_eq!(code(&refused), "google_ads_input", "{refused}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_plan_in_another_currency_changes_nothing() {
        let ads = Ads::new("ads-currency").await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made, in the dollars both bill in"),
        );
        // MP-2 replaces MP-1 in yen, a plan the proposal tool refuses to propose, so it is put in
        // the log as it would be if one got there; the ad account still bills in dollars.
        let mut yen = yen_plan("MP-2");
        yen["replaces"] = json!("MP-1");
        ads.harness.project.record_by(
            Some("kai"),
            crate::tools::fixtures::at(),
            "FRK-1",
            "marketing_plan.proposed",
            &yen,
        );
        ads.harness.project.plan_approved("FRK-1", "MP-2", "");

        for (tool, input) in [
            (
                "set_campaign_budget",
                json!({ "campaign": campaign, "amount": "400" }),
            ),
            (
                "set_campaign_status",
                json!({ "campaign": campaign, "status": "enabled" }),
            ),
        ] {
            let refused = ads.call(tool, input).await.expect_err("refused");
            assert!(
                refused.starts_with("not_in_marketing_plan: "),
                "{tool}: {refused}"
            );
            assert!(
                refused.contains("the plan is in JPY") && refused.contains("bills in USD"),
                "{tool}: {refused}"
            );
        }
        assert_eq!(ads.mutates().len(), 1, "only the create reached Google");
        // Pausing moves no money, and still runs.
        ads.call(
            "set_campaign_status",
            json!({ "campaign": campaign, "status": "paused" }),
        )
        .await
        .expect("paused");
        assert_eq!(ads.mutates().len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one plan after another, so what Google holds between them stays in view"
    )]
    async fn enabling_holds_what_google_keeps_to_the_plan() {
        let ads = Ads::new("ads-held").await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made"),
        );
        let enable = |campaign: &str| {
            ads.call(
                "set_campaign_status",
                json!({ "campaign": campaign, "status": "enabled" }),
            )
        };
        // What Google answers for a campaign, which keeps what its first plan made: a budget
        // column, "totalAmountMicros" or "amountMicros", and the days it runs.
        let holds =
            |campaign: &str, (column, micros): (&str, &str), (starts, ends): (&str, &str)| {
                let rows = json!([{
                    "campaign": {
                        "resourceName": campaign,
                        "startDateTime": format!("{starts} 00:00:00"),
                        "endDateTime": format!("{ends} 23:59:59")
                    },
                    "campaignBudget": { column: micros },
                    "metrics": { "costMicros": "0" }
                }]);
                ads.google.script(|script| {
                    script.rows = serde_json::from_value(rows).expect("rows");
                });
            };
        let days = ("2026-09-24", "2026-10-22");
        holds(&campaign, ("totalAmountMicros", "500000000"), days);

        // A plan that replaces MP-1 and lowers the campaign's budget: Google still holds 500.00.
        ads.plan_with("MP-2", Some("MP-1"), |body| {
            body["campaigns"][0]["budget"] = json!("300.00");
        });
        let refused = enable(&campaign)
            .await
            .expect_err("more than the plan allows");
        assert!(refused.starts_with("not_in_marketing_plan: "), "{refused}");
        assert!(
            refused.contains("total budget at Google is 500.00, more than the 300.00"),
            "{refused}"
        );
        assert_eq!(ads.mutates().len(), 1, "only the create reached Google");
        // Lowered to the plan's, it runs.
        ads.call(
            "set_campaign_budget",
            json!({ "campaign": campaign, "amount": "300" }),
        )
        .await
        .expect("lowered");
        // A budget set by hand in Google's own screens need not be a whole hundredth: a micro over
        // is a hundredth over.
        holds(&campaign, ("totalAmountMicros", "300000001"), days);
        let refused = enable(&campaign).await.expect_err("a micro over");
        assert!(refused.contains("budget at Google is 300.01"), "{refused}");
        holds(&campaign, ("totalAmountMicros", "300000000"), days);
        enable(&campaign).await.expect("within the plan now");
        assert_eq!(ads.mutates().len(), 3);

        // A plan that replaces it and shortens the campaign: Google still ends it on the 22nd,
        // and Farik cannot change that.
        ads.plan_with("MP-3", Some("MP-2"), |body| {
            body["campaigns"][0]["budget"] = json!("300.00");
            body["campaigns"][0]["ends_on"] = json!("2026-10-10");
        });
        let refused = enable(&campaign).await.expect_err("ends too late");
        assert!(
            refused.contains("ends on 2026-10-22 at Google, after the 2026-10-10"),
            "{refused}"
        );
        // An answer that does not say what Google holds is not one to enable on.
        ads.google.script(|script| {
            script.rows = vec![json!({ "campaign": { "resourceName": campaign } })];
        });
        let refused = enable(&campaign).await.expect_err("unreadable");
        assert!(refused.contains("could not read"), "{refused}");
        assert_eq!(ads.mutates().len(), 3, "nothing more reached Google");

        // A daily budget is read from its own column and counted over the days Google runs the
        // campaign: 400.00 over the 119 days from 2026-09-24 is 3.36, which a freshly made
        // campaign must be allowed to run on, though today (the 22nd) has 121 days left in the
        // plan's dates.
        let long = campaign_of(
            &ads.call("create_search_campaign", create("search-long"))
                .await
                .expect("made, with a daily budget"),
        );
        let run = ("2026-09-24", "2027-01-20");
        holds(&long, ("amountMicros", "3360000"), run);
        enable(&long)
            .await
            .expect("a daily budget of what was left over its run");
        // A plan that lowers it to 300.00 leaves Google at 3.36 a day: 399.84 over the run.
        ads.plan_with("MP-4", Some("MP-3"), |body| {
            body["campaigns"][1]["budget"] = json!("300.00");
        });
        let refused = enable(&long)
            .await
            .expect_err("more than the plan has left");
        assert!(
            refused.contains("daily budget at Google is 3.36, which over the 119 days it runs is more than the 300.00"),
            "{refused}"
        );
    }

    /// A campaign enabled while `during` happens to the plan, which the write reads and checks
    /// before it has read the ad account's currency and what the ads have spent, each a search
    /// that waits; the refusal it is answered with, after the campaign made under MP-1.
    async fn enabled_while(name: &str, during: impl FnOnce(&Ads)) -> (Ads, String) {
        let ads = Ads::new(name).await;
        ads.plan("MP-1", None);
        let campaign = campaign_of(
            &ads.call("create_search_campaign", create("search-launch"))
                .await
                .expect("made"),
        );
        ads.google.script(|script| {
            script.search_delay = Some(Duration::from_millis(300));
            script.rows = vec![json!({
                "campaign": {
                    "resourceName": campaign,
                    "startDateTime": "2026-09-24 00:00:00",
                    "endDateTime": "2026-10-22 23:59:59"
                },
                "campaignBudget": { "totalAmountMicros": "500000000" },
                "metrics": { "costMicros": "0" }
            })];
        });
        let (enabled, ()) = tokio::join!(
            ads.call(
                "set_campaign_status",
                json!({ "campaign": campaign, "status": "enabled" })
            ),
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                during(&ads);
            }
        );
        let refused = enabled.expect_err("the plan changed, so nothing was sent");
        (ads, refused)
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_plan_ended_during_a_write_changes_nothing() {
        // The plan ends by a path that does not wait for the write, as one in another process
        // would not, while the write is still reading.
        let (ads, refused) = enabled_while("ads-plan-ends", |ads| {
            let held = crate::marketing::hold_plans();
            crate::marketing::record_plan_end(
                &held,
                &ads.harness.project.deps,
                "MP-1",
                farik_core::marketing::EndReason::ByOwner,
                None,
                None,
            )
            .expect("ended");
        })
        .await;
        assert_eq!(code(&refused), "no_active_marketing_plan", "{refused}");
        assert_eq!(ads.mutates().len(), 1, "only the create reached Google");

        // A newer plan that replaces it is no more the plan the write was checked against.
        let (ads, refused) =
            enabled_while("ads-plan-replaced", |ads| ads.plan("MP-2", Some("MP-1"))).await;
        assert_eq!(code(&refused), "no_active_marketing_plan", "{refused}");
        assert!(
            refused.contains("MP-1") && refused.contains("MP-2"),
            "{refused}"
        );
        assert_eq!(ads.mutates().len(), 1, "only the create reached Google");
    }

    /// A plan in yen with one campaign of 500.50 to 2026-10-22.
    fn yen_plan(plan: &str) -> Value {
        let mut body =
            farik_protocol::event::fixtures::a_body_wire(EventKind::MarketingPlanProposed);
        body["plan"] = json!(plan);
        body["starts_on"] = json!("2026-09-20");
        body["ends_on"] = json!("2027-01-31");
        body["currency"] = json!("JPY");
        body["budget"] = json!({ "total": "1000.00", "google_ads": "1000.00" });
        body["posts"] = json!([]);
        body["google_ads_account"] = json!(ACCOUNT);
        body["campaigns"] = json!([{
            "key": "search-launch", "channel": "google_ads", "name": "x", "goal": "Sales",
            "budget": "500.50", "starts_on": "2026-09-22", "ends_on": "2026-10-22"
        }]);
        body
    }
}
