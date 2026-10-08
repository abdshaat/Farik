//! The watch on a marketing plan's Google Ads spend (`docs/SPEC.md` 6.7, ADR 0042, step 08g): a
//! task of its own beside the ticks, with no model, that reads each active plan's spend every 15
//! minutes, pauses the campaigns that reached a budget, and pauses the campaigns of a plan that
//! ended. A tick waits for a running session to end, so none of this is a rule of the tick.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use farik_core::contract::Role;
use farik_core::marketing::{
    Amount, Cap, CapScope, CreatedCampaign, Lineage, PlanRecord, PlanSpend, active_plan,
    caps_reached, is_carried, to_pause_for_end,
};
use farik_core::team::{Agent, AgentStatus};
use farik_protocol::event::{EventBody, new_event};
use farik_store::marketing::{
    MarketingPlan, PausedWhy, budgets_reached, campaigns_paused, created_campaigns_on,
    marketing_plans, paused_for_end,
};
use serde_json::json;

use super::{Orchestrator, OrchestratorError, RECHECK};
use crate::daemon::SpendRead;
use crate::daemon::ads_calls::{
    GOOGLE_ADS, cut_reason, lineage_of, pause_campaigns, read_spend, spent_by_key,
};

/// How long a plan's spend, or a pause that was refused, waits before it is tried again. A failed
/// try counts: a retry every minute would spend Farik Cloud's shared quota (ADR 0044) on failures
/// that rarely clear within a minute.
const TRY_EVERY: Duration = Duration::minutes(15);

impl Orchestrator {
    /// Watches the Google Ads spend of the marketing plans until `stop` is called: every minute
    /// (`RECHECK`, through the sleeper, so that a test's clock drives it) one wake reads the
    /// active plan's spend when its last try is 15 minutes old, pauses the campaigns that
    /// reached a budget, and pauses the campaigns an ended plan left running. It starts no
    /// session and runs while the team is paused, since stopping spend is never paused. Until
    /// the first campaign is made there is nothing to read or to pause, and it waits for that
    /// without asking the sleeper. A Google, sign-in or team-file failure is that read's `failed`
    /// and the watch goes on.
    ///
    /// # Errors
    ///
    /// A store error, which also stops the orchestrator.
    pub async fn watch_marketing_spend(&self) -> Result<(), OrchestratorError> {
        while !self.is_stopped() {
            match self.watch_once().await {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    self.stop();
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// One turn of the watch: waits for the first campaign, or wakes and waits a minute. Whether
    /// the watch goes on.
    async fn watch_once(&self) -> Result<bool, OrchestratorError> {
        // Taken before the stop is read, so that a stop between the two still ends the wait.
        let stopped = self.stops.notified();
        tokio::pin!(stopped);
        stopped.as_mut().enable();
        if self.is_stopped() {
            return Ok(false);
        }
        if created_campaigns_on(&self.deps.tools.log)?.is_empty() {
            tokio::select! {
                () = &mut stopped => return Ok(false),
                () = self.deps.daemon.campaign_made().notified() => return Ok(true),
            }
        }
        self.marketing_spend_wake().await?;
        let until = self.deps.tools.clock.now() + RECHECK;
        tokio::select! {
            () = &mut stopped => Ok(false),
            () = self.deps.sleeper.sleep_until(until) => Ok(true),
        }
    }

    /// One wake of the watch.
    pub(super) async fn marketing_spend_wake(&self) -> Result<(), OrchestratorError> {
        let tools = &self.deps.tools;
        let now = tools.clock.now();
        let plans = marketing_plans(&tools.log)?;
        let made = created_campaigns_on(&tools.log)?;
        if made.is_empty() {
            return Ok(());
        }
        let records: Vec<PlanRecord> = plans.iter().map(|plan| plan.record.clone()).collect();
        let active = active_plan(&records, now.date_naive())
            .and_then(|record| plans.iter().find(|plan| plan.record.id == record.id));
        let lineage = active.map(|plan| lineage_of(&plans, &plan.record.id));
        let watched = Watched {
            plans: &plans,
            made: &made,
            active: active.zip(lineage.as_deref()),
            now,
        };
        if let Some((plan, lineage)) = watched.active {
            self.read_the_active_plan(&watched, plan, lineage).await?;
        }
        self.pause_what_ended(&watched).await
    }

    /// Reads the active plan's spend when it is due, pauses the campaigns of each cap it reached,
    /// and pauses again any campaign of a cap already recorded that Google reports running. The
    /// writes' lock is held from the read through the pauses.
    async fn read_the_active_plan(
        &self,
        watched: &Watched<'_>,
        plan: &MarketingPlan,
        lineage: &[String],
    ) -> Result<(), OrchestratorError> {
        let (state, tools, now) = (&self.deps.daemon, &self.deps.tools, watched.now);
        let id = plan.record.id.as_str();
        let ours: Vec<&(CreatedCampaign, NaiveDate)> = watched
            .made
            .iter()
            .filter(|(made, _)| lineage.contains(&made.plan))
            .collect();
        let Some(first) = ours.iter().map(|(_, day)| *day).min() else {
            return Ok(());
        };
        let due = state
            .spend_reads()
            .get(id)
            .is_none_or(|read| now - read.attempted_at >= TRY_EVERY);
        if !due {
            return Ok(());
        }
        let _writing = state.ads_writes().lock().await;
        let campaigns: Vec<CreatedCampaign> = ours.iter().map(|(made, _)| made.clone()).collect();
        let read = match self.agents_for(Some(plan)) {
            Ok(agents) => read_spend(
                state,
                &agents,
                &campaigns,
                first - Duration::days(1),
                now.date_naive() + Duration::days(1),
            )
            .await
            .map(|micros| (agents, micros)),
            Err(why) => Err(why),
        };
        let (agents, micros) = match read {
            Ok(read) => read,
            Err(why) => {
                let mut reads = state.spend_reads();
                let entry = reads.entry(id.to_string()).or_insert_with(|| fresh(now));
                entry.attempted_at = now;
                entry.failed = Some((why, now));
                return Ok(());
            }
        };
        let by_key = spent_by_key(campaigns.iter(), &micros);
        let spend = PlanSpend {
            total: Amount(by_key.values().map(|each| each.0).sum()),
            by_key,
        };
        let recorded_before: Vec<(CapScope, Option<String>)> = budgets_reached(&tools.log)?
            .into_iter()
            .filter(|reached| reached.plan == id)
            .map(|reached| (reached.scope, reached.key))
            .collect();
        {
            let mut reads = state.spend_reads();
            let entry = reads.entry(id.to_string()).or_insert_with(|| fresh(now));
            entry.attempted_at = now;
            entry.spend = Some((spend.clone(), now));
            entry.failed = None;
        }
        let all: Vec<CreatedCampaign> = watched.made.iter().map(|(made, _)| made.clone()).collect();
        let active = Lineage {
            id,
            plan: &plan.proposal,
            lineage,
        };
        for cap in caps_reached(&active, &all, &spend, &recorded_before) {
            self.pause_at(plan, &agents, &cap).await?;
        }
        self.pause_again(plan, &agents, &active, &all, &recorded_before)
            .await
    }

    /// Pauses the campaigns of a cap that was just reached, and records it with what Google took
    /// and the first refusal's words.
    async fn pause_at(
        &self,
        plan: &MarketingPlan,
        agents: &[String],
        cap: &Cap,
    ) -> Result<(), OrchestratorError> {
        let results = pause_campaigns(&self.deps.daemon, agents, &cap.campaigns).await;
        let paused: Vec<&str> = results
            .iter()
            .filter(|(_, result)| result.is_ok())
            .map(|(campaign, _)| campaign.as_str())
            .collect();
        let failed = results
            .iter()
            .find_map(|(_, result)| result.as_ref().err())
            .map(|why| cut_reason(why));
        let mut body = json!({
            "plan": plan.record.id,
            "scope": cap.scope.as_str(),
            "spent": cap.spent.to_string(),
            "budget": cap.budget.to_string(),
            "currency": plan.proposal.currency,
            "paused": paused,
        });
        if let Some(key) = &cap.key {
            body["key"] = json!(key);
        }
        if let Some(failed) = failed {
            body["failed"] = json!(failed);
        }
        self.record(&body, EventBody::MarketingBudgetReached)
    }

    /// Reads the status of the campaigns of the caps recorded for this plan before this wake, and
    /// pauses any Google reports running: a pause that was refused, or a campaign enabled at
    /// Google by hand. A campaign the cap did not list as paused is recorded as paused once.
    async fn pause_again(
        &self,
        plan: &MarketingPlan,
        agents: &[String],
        active: &Lineage<'_>,
        made: &[CreatedCampaign],
        recorded_before: &[(CapScope, Option<String>)],
    ) -> Result<(), OrchestratorError> {
        if recorded_before.is_empty() {
            return Ok(());
        }
        let log = &self.deps.tools.log;
        let carried: Vec<&CreatedCampaign> = made
            .iter()
            .filter(|each| is_carried(active, each))
            .collect();
        let mut campaigns: Vec<&CreatedCampaign> = Vec::new();
        for (scope, key) in recorded_before {
            for each in &carried {
                let wanted = match scope {
                    CapScope::Plan => true,
                    CapScope::Campaign => Some(&each.key) == key.as_ref(),
                };
                if wanted
                    && !campaigns
                        .iter()
                        .any(|known| known.campaign == each.campaign)
                {
                    campaigns.push(each);
                }
            }
        }
        let names: Vec<String> = campaigns.iter().map(|each| each.campaign.clone()).collect();
        let results = pause_campaigns(&self.deps.daemon, agents, &names).await;
        let caps: Vec<_> = budgets_reached(log)?
            .into_iter()
            .filter(|reached| reached.plan == plan.record.id)
            .collect();
        // Only the pauses recorded since this plan's first cap are its own: a raised version
        // reaches its caps again for a campaign the plan it replaced had paused.
        let first_cap = caps.iter().map(|reached| reached.seq).min();
        let listed: Vec<String> = caps
            .into_iter()
            .flat_map(|reached| reached.paused)
            .collect();
        let noted: Vec<String> = campaigns_paused(log)?
            .into_iter()
            .filter(|pause| pause.why == PausedWhy::BudgetReached)
            .filter(|pause| first_cap.is_some_and(|first| pause.seq > first))
            .map(|pause| pause.campaign)
            .collect();
        for ((name, result), each) in results.iter().zip(&campaigns) {
            if result.is_ok() && !listed.contains(name) && !noted.contains(name) {
                self.record_paused(each, PausedWhy::BudgetReached)?;
            }
        }
        Ok(())
    }

    /// Pauses the campaigns no active plan carries, each plan's together, and records each as
    /// paused for the plan's end. A refusal is kept as the plan's `unstopped`, tried again
    /// fifteen minutes later.
    async fn pause_what_ended(&self, watched: &Watched<'_>) -> Result<(), OrchestratorError> {
        let (state, tools, now) = (&self.deps.daemon, &self.deps.tools, watched.now);
        let for_end = paused_for_end(&tools.log, GOOGLE_ADS)?;
        let all: Vec<CreatedCampaign> = watched.made.iter().map(|(made, _)| made.clone()).collect();
        let lineage = watched.active.map(|(plan, lineage)| Lineage {
            id: plan.record.id.as_str(),
            plan: &plan.proposal,
            lineage,
        });
        let mut by_plan: BTreeMap<String, Vec<CreatedCampaign>> = BTreeMap::new();
        for each in to_pause_for_end(lineage.as_ref(), &all, &for_end) {
            by_plan.entry(each.plan.clone()).or_default().push(each);
        }
        for (plan_id, campaigns) in by_plan {
            let waiting = state.spend_reads().get(&plan_id).is_some_and(|read| {
                read.unstopped.is_some() && now - read.attempted_at < TRY_EVERY
            });
            if waiting {
                continue;
            }
            let _writing = state.ads_writes().lock().await;
            let plan = watched.plans.iter().find(|plan| plan.record.id == plan_id);
            let refusal = match self.agents_for(plan) {
                Ok(agents) => {
                    let names: Vec<String> =
                        campaigns.iter().map(|each| each.campaign.clone()).collect();
                    let results = pause_campaigns(state, &agents, &names).await;
                    let mut refusal = None;
                    for ((_, result), each) in results.iter().zip(&campaigns) {
                        match result {
                            Ok(()) => self.record_paused(each, PausedWhy::PlanEnded)?,
                            Err(why) => {
                                refusal.get_or_insert_with(|| cut_reason(why));
                            }
                        }
                    }
                    refusal
                }
                Err(why) => Some(why),
            };
            let mut reads = state.spend_reads();
            let entry = reads.entry(plan_id).or_insert_with(|| fresh(now));
            entry.attempted_at = now;
            entry.unstopped = refusal;
        }
        Ok(())
    }

    /// The agents whose Google Ads connection Farik may use for `plan`: every Marketing Specialist
    /// in the team file, whatever its status, since stopping spend is never paused (ADR 0042). A
    /// paused agent keeps its sign-in. A retired agent is tried last and finds one only when it
    /// was retired outside Farik (a hand edit of the team file, or a pulled one): retiring it in
    /// Farik pauses its ads first and deletes its keys (ADR 0030), so it is passed over
    /// otherwise. The plan's proposer comes first, then the active ones, then the paused, then
    /// the retired, each in the team file's order. The reason, when the team file cannot be read.
    fn agents_for(&self, plan: Option<&MarketingPlan>) -> Result<Vec<String>, String> {
        let team = self
            .deps
            .tools
            .files
            .read_team()
            .map_err(|error| format!("the team file could not be read: {error}"))?;
        let proposer = plan.map(|plan| plan.agent_id.as_str());
        let mut marketing: Vec<&Agent> = team
            .agents
            .iter()
            .filter(|agent| Role::from(agent.role) == Role::MarketingSpecialist)
            .collect();
        // Stable, so that each group keeps the team file's order.
        marketing.sort_by_key(|agent| match agent.status {
            _ if Some(agent.id.as_str()) == proposer => 0,
            AgentStatus::Active => 1,
            AgentStatus::Paused => 2,
            AgentStatus::Retired => 3,
        });
        Ok(marketing
            .into_iter()
            .map(|agent| agent.id.to_string())
            .collect())
    }

    /// Records that Farik paused `made`, for `why`.
    fn record_paused(
        &self,
        made: &CreatedCampaign,
        why: PausedWhy,
    ) -> Result<(), OrchestratorError> {
        let body = json!({
            "plan": made.plan, "key": made.key, "campaign": made.campaign, "why": why.as_str(),
        });
        self.record(&body, EventBody::MarketingCampaignPaused)
    }

    /// Records Farik's own act: the event `body` reads as, with no task, no agent and no session.
    fn record<T: serde::de::DeserializeOwned>(
        &self,
        body: &serde_json::Value,
        wrap: impl FnOnce(T) -> EventBody,
    ) -> Result<(), OrchestratorError> {
        let tools = &self.deps.tools;
        let typed: T =
            serde_json::from_value(body.clone()).map_err(|error| OrchestratorError::Refused {
                reason: format!("marketing_event_not_recorded: {error}"),
            })?;
        let event =
            new_event(wrap(typed), tools.clock.now(), tools.ids.clone()).map_err(|error| {
                OrchestratorError::Refused {
                    reason: format!("marketing_event_not_recorded: {error:?}"),
                }
            })?;
        let appended = tools.log.append(&event)?;
        tools.projections.apply(&appended)?;
        Ok(())
    }
}

/// What one wake looks at: the plans, the campaigns made with the day each was, the active plan
/// with its lineage, and the time.
struct Watched<'a> {
    plans: &'a [MarketingPlan],
    made: &'a [(CreatedCampaign, NaiveDate)],
    active: Option<(&'a MarketingPlan, &'a [String])>,
    now: DateTime<Utc>,
}

/// A plan the watch has not tried yet.
fn fresh(now: DateTime<Utc>) -> SpendRead {
    SpendRead {
        attempted_at: now,
        spend: None,
        failed: None,
        unstopped: None,
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use farik_core::marketing::{Amount, PlanSpend};
    use farik_core::team::fixtures::an_agent_wire;
    use farik_protocol::clock::MovableClock;
    use farik_protocol::event::{EventBody, EventKind, FarikEvent};
    use farik_store::event_log::fixtures::refuse_appends_of;
    use farik_store::marketing::marketing_plans;
    use serde_json::{Value, json};

    use super::super::{Orchestrator, OrchestratorError};
    use crate::daemon::ads_calls::fixtures::{Ads, campaign_name, spend_row};
    use crate::google_ads::{spend_query, status_query};
    use crate::google_ads_fixture::Seen;
    use crate::orchestrator::fixtures::UsageThenWaitAdapter;
    use crate::sleep::Sleeper;
    use crate::tools::fixtures::{at, with_the_marketing_specialist};
    use farik_core::pricing::Usage;

    /// The ad account of the plans, ten digits.
    const CUSTOMER: &str = "1234567890";
    /// Another ad account.
    const OTHER: &str = "2345678901";

    /// Kai's Google Ads and a plan, and an orchestrator whose clock the test moves.
    struct Watching {
        ads: Ads,
        clock: Arc<MovableClock>,
        orchestrator: Arc<Orchestrator>,
    }

    impl Watching {
        async fn new(name: &str) -> Self {
            Self::over(Ads::new(name).await, Arc::new(Counting::default()))
        }

        /// The orchestrator over `ads`, waiting on `sleeper`.
        fn over(ads: Ads, sleeper: Arc<dyn Sleeper>) -> Self {
            let clock = Arc::new(MovableClock::new(at()));
            let orchestrator = Arc::new(ads.harness.orchestrator_sleeping(
                ads.harness.recorded(Vec::new()),
                Arc::clone(&clock),
                sleeper,
            ));
            Self {
                ads,
                clock,
                orchestrator,
            }
        }

        /// Moves the clock to `minutes` after the fixture's time.
        fn at(&self, minutes: i64) {
            self.clock.set(at() + chrono::Duration::minutes(minutes));
        }

        /// One wake of the watch.
        async fn wakes(&self) {
            self.orchestrator
                .marketing_spend_wake()
                .await
                .expect("the wake runs");
        }

        /// Kai made campaign `number` of `customer` for `key` of `plan`, under a budget of 500.00.
        fn made(&self, (plan, key): (&str, &str), (customer, number): (&str, u64)) {
            self.ads
                .made_campaign((plan, key), (customer, number), ("total", "500.00"));
        }

        /// What `customer`'s campaigns have cost, in whole dollars: `(number, dollars)`.
        fn costs(&self, customer: &str, costs: &[(u64, u64)]) {
            let rows: Vec<Value> = costs
                .iter()
                .map(|(number, dollars)| spend_row((customer, *number), dollars * 1_000_000, None))
                .collect();
            self.ads.google.script(|script| {
                script.customer_rows.insert(customer.to_string(), rows);
            });
        }

        /// Google's status for campaign `number` of `customer`.
        fn status(&self, (customer, number): (&str, u64), status: &str) {
            self.ads.google.script(|script| {
                script
                    .statuses
                    .insert(campaign_name(customer, number), status.to_string());
            });
        }

        /// The owner ends `plan`.
        fn ends(&self, plan: &str) {
            self.ads.harness.project.record(
                "",
                "marketing_plan.ended",
                &json!({ "plan": plan, "why": "by_owner" }),
            );
        }

        /// The campaigns Farik recorded as paused for `why`, in order, as `(plan, key, campaign)`.
        fn paused_for(&self, why: &str) -> Vec<(String, String, String)> {
            self.events(EventKind::MarketingCampaignPaused)
                .iter()
                .filter_map(|event| match &event.body {
                    EventBody::MarketingCampaignPaused(body) if body.why.to_string() == why => {
                        assert_eq!(event.envelope.ids.agent_id, None, "Farik records it");
                        assert_eq!(event.envelope.ids.session_id, None);
                        Some((
                            body.plan.as_str().to_string(),
                            body.key.as_str().to_string(),
                            body.campaign.as_str().to_string(),
                        ))
                    }
                    _ => None,
                })
                .collect()
        }

        /// The queries of every `Search` Google was sent, in order.
        fn searches(&self) -> Vec<String> {
            self.ads
                .google
                .requests_of("search")
                .iter()
                .map(|seen| seen.body["query"].as_str().unwrap_or("").to_string())
                .collect()
        }

        /// The `Search`es that read spend, and those that read a status.
        fn spend_reads(&self) -> Vec<String> {
            self.searches()
                .into_iter()
                .filter(|query| query.contains("metrics.cost_micros"))
                .collect()
        }

        fn status_reads(&self) -> Vec<String> {
            self.searches()
                .into_iter()
                .filter(|query| query.contains("campaign.status"))
                .collect()
        }

        /// The campaigns each `mutate` Google was sent paused, in order.
        fn pauses(&self) -> Vec<String> {
            self.ads
                .google
                .requests_of("mutate")
                .iter()
                .flat_map(|seen: &Seen| {
                    seen.body["mutateOperations"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                })
                .filter(|operation| operation["campaignOperation"]["update"]["status"] == "PAUSED")
                .map(|operation| {
                    operation["campaignOperation"]["update"]["resourceName"]
                        .as_str()
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        }

        fn events(&self, kind: EventKind) -> Vec<FarikEvent> {
            self.ads.harness.project.events(&[kind])
        }

        /// The body of the `index`th `marketing_budget.reached`.
        fn reached(&self, index: usize) -> serde_json::Value {
            let events = self.events(EventKind::MarketingBudgetReached);
            let event = events.get(index).expect("a budget was reached");
            assert_eq!(event.envelope.ids.agent_id, None, "Farik records it");
            assert_eq!(event.envelope.ids.session_id, None);
            assert_eq!(event.envelope.ids.task_id, None);
            let EventBody::MarketingBudgetReached(body) = &event.body else {
                panic!("a budget reached");
            };
            serde_json::to_value(body).expect("a body")
        }
    }

    /// A sleeper that counts how often it is asked and never returns.
    #[derive(Default)]
    struct Counting(AtomicU32);

    impl Sleeper for Counting {
        fn sleep_until(
            &self,
            _until: DateTime<Utc>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending())
        }
    }

    /// The query a spend read of `campaigns` in one account sends, from the 21st to the 23rd of
    /// September, 2026: the day before the first campaign was made to the day after today.
    fn spend_query_of(campaigns: &[String]) -> String {
        spend_query(
            campaigns,
            "2026-09-21".parse().expect("a date"),
            "2026-09-23".parse().expect("a date"),
        )
        .expect("a query")
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reads_every_fifteen_minutes_beside_the_ticks() {
        let adapter = Arc::new(UsageThenWaitAdapter::waiting(Usage::default()));
        let ads = Ads::new("spend-every-fifteen").await;
        let clock = Arc::new(MovableClock::new(at()));
        let orchestrator = Arc::new(ads.harness.orchestrator_sleeping(
            adapter.clone(),
            Arc::clone(&clock),
            Arc::new(Counting::default()),
        ));
        let watching = Watching {
            ads,
            clock,
            orchestrator,
        };
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 100)]);
        let expected = spend_query_of(&[campaign_name(CUSTOMER, 11)]);

        // A developer's session is running, and the tick that started it waits for it to end.
        watching.ads.harness.in_progress("FRK-2", "dev-a", "dev-b");
        let tick = {
            let orchestrator = Arc::clone(&watching.orchestrator);
            tokio::spawn(async move { orchestrator.tick().await })
        };
        let started = adapter.started_count();
        tokio::time::timeout(Duration::from_secs(10), async {
            while started.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the session starts");
        assert!(!tick.is_finished(), "the session is running");

        // Wakes at 0, 14 and 15 minutes: the first and the third read, the second does not.
        watching.wakes().await;
        assert_eq!(watching.spend_reads(), std::slice::from_ref(&expected));
        watching.at(14);
        watching.wakes().await;
        assert_eq!(watching.spend_reads().len(), 1, "14 minutes is not due");
        watching.at(15);
        watching.wakes().await;
        assert_eq!(watching.spend_reads(), [expected.clone(), expected.clone()]);
        assert!(!tick.is_finished(), "the session ran through the reads");
        adapter.complete();
        tick.await.expect("joined").expect("the tick ran");

        // A new daemon state has read nothing yet, so the next wake reads at once.
        watching.ads.state().spend_reads().clear();
        watching.at(16);
        watching.wakes().await;
        assert_eq!(watching.spend_reads().len(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn pauses_at_a_campaign_s_cap_and_records_it() {
        let watching = Watching::new("spend-campaign-cap").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        // 500.00 spent of the campaign's 500.00.
        watching.costs(CUSTOMER, &[(11, 500)]);

        watching.wakes().await;

        // One read of the cost, one of the status, and one pause, in that order.
        let sent: Vec<String> = watching
            .ads
            .google
            .requests()
            .iter()
            .map(Seen::method_name)
            .collect();
        assert_eq!(sent, ["search", "search", "mutate"]);
        let campaign = campaign_name(CUSTOMER, 11);
        assert_eq!(
            watching.status_reads(),
            [status_query(std::slice::from_ref(&campaign)).expect("a query")]
        );
        assert_eq!(watching.pauses(), std::slice::from_ref(&campaign));
        let reached = watching.reached(0);
        assert_eq!(reached["plan"], "MP-1");
        assert_eq!(reached["scope"], "campaign");
        assert_eq!(reached["key"], "search-launch");
        assert_eq!(reached["spent"], "500.00");
        assert_eq!(reached["budget"], "500.00");
        assert_eq!(reached["currency"], "USD");
        assert_eq!(reached["paused"], json!([campaign]));
        assert!(reached.get("failed").is_none(), "{reached}");

        // The next read finds it paused: nothing is recorded and no pause is sent.
        watching.at(15);
        watching.wakes().await;
        assert_eq!(watching.spend_reads().len(), 2);
        assert_eq!(watching.pauses().len(), 1);
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);
        assert!(
            watching
                .events(EventKind::MarketingCampaignPaused)
                .is_empty()
        );
    }

    /// A plan of three campaigns with 800.00 for Google Ads, each campaign made.
    fn three_campaigns(watching: &Watching) {
        watching.ads.plan_with("MP-1", None, |body| {
            body["budget"] = json!({ "total": "1000.00", "google_ads": "800.00" });
            let campaign = |key: &str, budget: &str| {
                json!({
                    "key": key, "channel": "google_ads", "name": key, "goal": "Sales",
                    "budget": budget, "starts_on": "2026-09-22", "ends_on": "2026-10-22"
                })
            };
            body["campaigns"] = json!([
                campaign("search-a", "500.00"),
                campaign("search-b", "400.00"),
                campaign("search-c", "300.00"),
            ]);
        });
        watching.made(("MP-1", "search-a"), (CUSTOMER, 11));
        watching.made(("MP-1", "search-b"), (CUSTOMER, 12));
        watching.made(("MP-1", "search-c"), (CUSTOMER, 13));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn pauses_everything_at_the_plan_s_cap() {
        let watching = Watching::new("spend-plan-cap").await;
        three_campaigns(&watching);
        // Each under its own budget (500.00, 400.00, 300.00), together at the plan's 800.00.
        watching.costs(CUSTOMER, &[(11, 300), (12, 300), (13, 200)]);
        // Google says the third is paused already.
        watching.status((CUSTOMER, 13), "PAUSED");

        watching.wakes().await;

        let (first, second, third) = (
            campaign_name(CUSTOMER, 11),
            campaign_name(CUSTOMER, 12),
            campaign_name(CUSTOMER, 13),
        );
        assert_eq!(watching.pauses(), [first.clone(), second.clone()]);
        assert_eq!(
            watching.ads.google.requests_of("mutate").len(),
            2,
            "one mutate each, on its own"
        );
        let reached = watching.reached(0);
        assert_eq!(reached["scope"], "plan");
        assert!(reached.get("key").is_none(), "{reached}");
        assert_eq!(reached["spent"], "800.00");
        assert_eq!(reached["budget"], "800.00");
        assert_eq!(reached["paused"], json!([first, second, third]));
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_refused_pause_is_recorded_and_tried_again() {
        let watching = Watching::new("spend-refused-pause").await;
        three_campaigns(&watching);
        watching.costs(CUSTOMER, &[(11, 300), (12, 300), (13, 200)]);
        watching.status((CUSTOMER, 13), "PAUSED");
        let (first, second) = (campaign_name(CUSTOMER, 11), campaign_name(CUSTOMER, 12));
        watching.ads.google.script(|script| {
            script
                .refuse_pause
                .insert(campaign_name(CUSTOMER, 12), "x".repeat(400));
        });

        watching.wakes().await;

        // Google took the first and refused the second: the cap says so, in Google's words cut at
        // 300 characters.
        let reached = watching.reached(0);
        assert_eq!(
            reached["paused"],
            json!([first, campaign_name(CUSTOMER, 13)])
        );
        let failed = reached["failed"].as_str().expect("Google's words");
        assert_eq!(failed.chars().count(), 300, "{failed}");
        assert!(failed.starts_with("Google answered “xxx"), "{failed}");
        assert!(
            watching
                .events(EventKind::MarketingCampaignPaused)
                .is_empty()
        );

        // Fifteen minutes later Google takes it: it is paused and recorded once.
        watching
            .ads
            .google
            .script(|script| script.refuse_pause.clear());
        watching.at(15);
        watching.wakes().await;
        assert_eq!(
            watching.pauses(),
            [first.clone(), second.clone(), second.clone()]
        );
        let paused = watching.events(EventKind::MarketingCampaignPaused);
        assert_eq!(paused.len(), 1);
        let EventBody::MarketingCampaignPaused(body) = &paused[0].body else {
            panic!("a campaign paused");
        };
        assert_eq!(
            (
                body.plan.as_str(),
                body.key.as_str(),
                body.campaign.as_str()
            ),
            ("MP-1", "search-b", second.as_str())
        );
        assert_eq!(body.why.to_string(), "budget_reached");
        assert_eq!(paused[0].envelope.ids.agent_id, None);
        assert_eq!(paused[0].envelope.ids.session_id, None);
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);

        // And a third read finds nothing to do.
        watching.at(30);
        watching.wakes().await;
        assert_eq!(watching.pauses().len(), 3);
        assert_eq!(watching.events(EventKind::MarketingCampaignPaused).len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_raised_plan_does_not_repause_the_old_cap() {
        let watching = Watching::new("spend-raised").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 500)]);
        watching.wakes().await;
        assert_eq!(watching.pauses().len(), 1, "the cap paused it");

        // The owner approves MP-2, which replaces MP-1 with the key's budget raised, and the
        // agent enables the campaign again.
        watching.ads.plan_with("MP-2", Some("MP-1"), |body| {
            body["campaigns"][0]["budget"] = json!("800.00");
            body["budget"] = json!({ "total": "1500.00", "google_ads": "1300.00" });
        });
        watching.ads.harness.project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-1", "why": "replaced", "replaced_by": "MP-2" }),
        );
        watching.status((CUSTOMER, 11), "ENABLED");

        // 500.00 of the raised 800.00: no cap, and the old cap is MP-1's, not MP-2's.
        watching.at(15);
        watching.wakes().await;
        assert_eq!(watching.spend_reads().len(), 2);
        assert_eq!(watching.pauses().len(), 1, "no pause was sent again");
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_campaign_missing_from_the_status_rows_is_paused() {
        let watching = Watching::new("spend-missing-status").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 500)]);
        // Google's status read has no row for it at all.
        watching.status((CUSTOMER, 11), "MISSING");

        watching.wakes().await;

        // No row is not "paused": the pause is sent, and the cap says Google took it.
        let campaign = campaign_name(CUSTOMER, 11);
        assert_eq!(watching.pauses(), std::slice::from_ref(&campaign));
        assert_eq!(watching.reached(0)["paused"], json!([campaign]));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_plan_s_cost_is_the_sum_over_every_ad_account() {
        let watching = Watching::new("spend-two-accounts").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        // MP-2 replaces it in another ad account, with 600.00 for Google Ads.
        watching.ads.plan_with("MP-2", Some("MP-1"), |body| {
            body["google_ads_account"] = json!("234-567-8901");
            body["budget"] = json!({ "total": "1000.00", "google_ads": "600.00" });
        });
        watching.ends("MP-1");
        watching.made(("MP-2", "search-long"), (OTHER, 21));
        // 250.00 of search-launch's 500.00 in the first account, 350.00 of search-long's 400.00
        // in the second: neither campaign is at its cap, and together they are at the plan's.
        watching.costs(CUSTOMER, &[(11, 250)]);
        watching.costs(OTHER, &[(21, 350)]);

        watching.wakes().await;

        let (first, second) = (campaign_name(CUSTOMER, 11), campaign_name(OTHER, 21));
        assert_eq!(
            watching.spend_reads(),
            [
                spend_query_of(std::slice::from_ref(&first)),
                spend_query_of(std::slice::from_ref(&second)),
            ],
            "one read for each ad account"
        );
        let reached = watching.reached(0);
        assert_eq!(reached["plan"], "MP-2");
        assert_eq!(reached["scope"], "plan");
        assert_eq!(reached["spent"], "600.00");
        assert_eq!(reached["budget"], "600.00");
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);
        // MP-2 carries only the campaign in its own account, which the cap pauses; MP-1's, in the
        // other account, is paused for its plan's end.
        assert_eq!(reached["paused"], json!([second]));
        assert_eq!(watching.pauses(), [second, first]);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "a refused pause tried again under one plan and then under its raised version"
    )]
    async fn a_repause_under_a_raised_version_is_recorded() {
        let watching = Watching::new("spend-repause-raised").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        let campaign = campaign_name(CUSTOMER, 11);
        let refuse = |words: Option<&str>| {
            watching.ads.google.script(|script| match words {
                Some(words) => {
                    script
                        .refuse_pause
                        .insert(campaign.clone(), words.to_string());
                }
                None => script.refuse_pause.clear(),
            });
        };

        // MP-1: the campaign reaches its 500.00, Google refuses the pause, and fifteen minutes
        // later takes it: that pause is recorded.
        watching.costs(CUSTOMER, &[(11, 500)]);
        refuse(Some("nope"));
        watching.wakes().await;
        refuse(None);
        watching.at(15);
        watching.wakes().await;
        assert_eq!(watching.paused_for("budget_reached").len(), 1);

        // The owner raises it: MP-2 replaces MP-1 with the key's budget at 800.00, and the agent
        // enables the campaign again at Google.
        watching.ads.plan_with("MP-2", Some("MP-1"), |body| {
            body["campaigns"][0]["budget"] = json!("800.00");
            body["budget"] = json!({ "total": "1500.00", "google_ads": "1300.00" });
        });
        watching.ads.harness.project.record(
            "",
            "marketing_plan.ended",
            &json!({ "plan": "MP-1", "why": "replaced", "replaced_by": "MP-2" }),
        );
        watching.status((CUSTOMER, 11), "ENABLED");

        // MP-2: it reaches its 800.00, Google refuses the pause again, and later takes it. That is
        // MP-2's own pause to record, though MP-1's was recorded for the same campaign.
        watching.costs(CUSTOMER, &[(11, 800)]);
        refuse(Some("nope"));
        watching.at(30);
        watching.wakes().await;
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 2);
        refuse(None);
        watching.at(45);
        watching.wakes().await;
        assert_eq!(
            watching.paused_for("budget_reached"),
            [
                (
                    "MP-1".to_string(),
                    "search-launch".to_string(),
                    campaign.clone()
                ),
                ("MP-1".to_string(), "search-launch".to_string(), campaign),
            ],
            "recorded once for each of the two caps"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn reads_while_the_team_is_paused() {
        let watching = Watching::new("spend-team-paused").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 500)]);
        watching
            .ads
            .harness
            .project
            .record("", "team.paused", &json!({ "by": "human" }));

        watching.wakes().await;

        assert_eq!(watching.spend_reads().len(), 1);
        assert_eq!(watching.pauses(), [campaign_name(CUSTOMER, 11)]);
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_read_holds_the_ads_lock() {
        let watching = Watching::new("spend-lock").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 100)]);

        // While a write to Google Ads holds the lock, no read is sent.
        let held = watching.ads.state().ads_writes().lock().await;
        let wake = {
            let orchestrator = Arc::clone(&watching.orchestrator);
            tokio::spawn(async move { orchestrator.marketing_spend_wake().await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(watching.searches().is_empty(), "{:?}", watching.searches());
        assert!(!wake.is_finished());

        // Released, the read runs.
        drop(held);
        wake.await.expect("joined").expect("the wake runs");
        assert_eq!(watching.spend_reads().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one read through four ways it can go, side by side"
    )]
    async fn falls_back_to_another_marketing_specialist_and_says_when_it_cannot_read() {
        let ads = Ads::with_team(
            "spend-fallback",
            |wire| {
                with_the_marketing_specialist(wire);
                wire["agents"][3]["display_name"] = json!("Kai");
                wire["agents"]
                    .as_array_mut()
                    .expect("agents")
                    .push(an_agent_wire("lia", "marketing_specialist"));
                wire["agents"][4]["display_name"] = json!("Lia");
            },
            |_, _| {},
        )
        .await;
        let lia = ads.connect("lia");
        let kai = ads.grant.clone();
        let watching = Watching::over(ads, Arc::new(Counting::default()));
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 100)]);
        let bearer = |seen: &Seen| {
            seen.headers
                .get("authorization")
                .cloned()
                .unwrap_or_default()
        };
        let kai_bearer = format!("Bearer {}", kai.access_token.expose());
        let lia_bearer = format!("Bearer {}", lia.access_token.expose());
        let lapse = |agent: &str, lapsed: bool| {
            use crate::connectors::ConnectorSecrets as _;
            let at = watching
                .ads
                .state()
                .secret_at(
                    watching.ads.harness.project.deps.files.root(),
                    agent,
                    "google-ads",
                )
                .expect("an address");
            let mut kept = watching
                .ads
                .store
                .load(&at)
                .expect("readable")
                .expect("kept");
            kept.oauth.as_mut().expect("a grant").lapsed = lapsed;
            watching.ads.store.save(&at, &kept).expect("kept");
        };

        // What the last read that worked said: 100.00 for the campaign's key, as of minute 15.
        let last_good = || {
            Some((
                PlanSpend {
                    by_key: BTreeMap::from([("search-launch".to_string(), Amount(10_000))]),
                    total: Amount(10_000),
                },
                at() + chrono::Duration::minutes(15),
            ))
        };

        // The proposer's sign-in has ended: the read is made with Lia's.
        lapse("kai", true);
        watching.wakes().await;
        let seen = watching.ads.google.requests_of("search");
        assert_eq!(
            seen.iter().map(&bearer).collect::<Vec<_>>(),
            std::slice::from_ref(&lia_bearer)
        );
        {
            let reads = watching.ads.state().spend_reads();
            let read = reads.get("MP-1").expect("the plan was read");
            assert!(read.spend.is_some() && read.failed.is_none(), "{read:?}");
        }

        // Google does not let the proposer's sign-in into the account: Lia's is tried next.
        lapse("kai", false);
        watching.ads.google.script(|script| {
            script.deny_tokens = vec![kai.access_token.expose().to_string()];
        });
        watching.at(15);
        watching.wakes().await;
        let seen = watching.ads.google.requests_of("search");
        assert_eq!(
            seen.iter().skip(1).map(&bearer).collect::<Vec<_>>(),
            [kai_bearer.clone(), lia_bearer.clone()]
        );

        // Any other answer of Google's ends the read: Lia is not asked.
        watching.ads.google.script(|script| {
            script.deny_tokens.clear();
            script.fail_search_containing = Some(("metrics.cost_micros".to_string(), 400));
        });
        watching.at(30);
        watching.wakes().await;
        let seen = watching.ads.google.requests_of("search");
        assert_eq!(
            seen.iter().skip(3).map(&bearer).collect::<Vec<_>>(),
            [kai_bearer]
        );
        {
            let reads = watching.ads.state().spend_reads();
            let read = reads.get("MP-1").expect("the plan was read");
            assert_eq!(
                read.failed.as_ref().map(|(why, _)| why.as_str()),
                Some("Google answered “the search failed”")
            );
            assert_eq!(
                read.spend,
                last_good(),
                "the last good read is kept, with its time"
            );
        }

        // With no sign-in left, the read fails with the last reason, and the next try is fifteen
        // minutes after this one.
        watching
            .ads
            .google
            .script(|script| script.fail_search_containing = None);
        lapse("kai", true);
        lapse("lia", true);
        watching.at(45);
        watching.wakes().await;
        {
            let reads = watching.ads.state().spend_reads();
            let read = reads.get("MP-1").expect("the plan was read");
            assert_eq!(
                read.failed.as_ref().map(|(why, at)| (why.as_str(), *at)),
                Some((
                    "Lia's sign-in to Google has ended; sign Lia in again on Lia's page",
                    at() + chrono::Duration::minutes(45)
                ))
            );
            assert_eq!(
                read.spend,
                last_good(),
                "the last good read is kept beside it, with its time"
            );
        }
        let before = watching.ads.google.requests().len();
        watching.at(59);
        watching.wakes().await;
        assert_eq!(watching.ads.google.requests().len(), before, "not due yet");
        {
            // No sign-in to ask Google with means no request to count, so the try itself is
            // what is looked for: the failure still stands from 45 minutes in.
            let reads = watching.ads.state().spend_reads();
            let read = reads.get("MP-1").expect("the plan was read");
            assert_eq!(
                read.failed.as_ref().map(|(_, at_)| *at_),
                Some(at() + chrono::Duration::minutes(45)),
                "not tried again within 15 minutes of a failed try"
            );
        }
        assert!(
            watching
                .events(EventKind::MarketingBudgetReached)
                .is_empty()
        );
        assert!(
            watching
                .events(EventKind::MarketingCampaignPaused)
                .is_empty()
        );
        lapse("lia", false);
        watching.at(60);
        watching.wakes().await;
        let reads = watching.ads.state().spend_reads();
        assert!(
            reads.get("MP-1").expect("read").failed.is_none(),
            "a read worked"
        );
    }

    /// Kai, the plan's proposer, with `status` in the team file, watching.
    async fn watching_with_kai(name: &str, status: &'static str) -> Watching {
        let ads = Ads::with_team(
            name,
            move |wire| {
                with_the_marketing_specialist(wire);
                wire["agents"][3]["status"] = json!(status);
            },
            |_, _| {},
        )
        .await;
        Watching::over(ads, Arc::new(Counting::default()))
    }

    /// Kai, `status` in the team file, has a campaign at its cap: Farik still reads, pauses and
    /// records it, since a sign-in is not revoked by pausing or retiring its agent.
    async fn stops_spend_with_kai(name: &str, status: &'static str) {
        let watching = watching_with_kai(name, status).await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        // 500.00 spent of the campaign's 500.00.
        watching.costs(CUSTOMER, &[(11, 500)]);

        watching.wakes().await;

        let campaign = campaign_name(CUSTOMER, 11);
        assert_eq!(watching.spend_reads().len(), 1);
        assert_eq!(
            watching.status_reads(),
            [status_query(std::slice::from_ref(&campaign)).expect("a query")]
        );
        assert_eq!(watching.pauses(), std::slice::from_ref(&campaign));
        let reached = watching.reached(0);
        assert_eq!(reached["scope"], "campaign");
        assert_eq!(reached["paused"], json!([campaign]));
        assert_eq!(watching.events(EventKind::MarketingBudgetReached).len(), 1);
        let reads = watching.ads.state().spend_reads();
        let read = reads.get("MP-1").expect("the plan was read");
        assert!(read.failed.is_none(), "{read:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_paused_marketing_specialist_still_stops_spend() {
        stops_spend_with_kai("spend-kai-paused", "paused").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_retired_marketing_specialist_still_stops_spend() {
        stops_spend_with_kai("spend-kai-retired", "retired").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn tries_the_proposer_then_active_then_paused_then_retired() {
        let ads = Ads::with_team(
            "spend-agents-order",
            |wire| {
                with_the_marketing_specialist(wire);
                wire["agents"][3]["status"] = json!("retired");
                let agents = wire["agents"].as_array_mut().expect("agents");
                for (id, status) in [
                    ("lia", "paused"),
                    ("mo", "active"),
                    ("ned", "paused"),
                    ("oz", "retired"),
                    ("pat", "active"),
                ] {
                    let mut agent = an_agent_wire(id, "marketing_specialist");
                    agent["status"] = json!(status);
                    agents.push(agent);
                }
            },
            |_, _| {},
        )
        .await;
        let watching = Watching::over(ads, Arc::new(Counting::default()));
        watching.ads.plan("MP-1", None);
        let plans = marketing_plans(&watching.ads.harness.project.deps.log).expect("the plans");
        let plan = plans.first().expect("a plan");
        assert_eq!(plan.agent_id, "kai");

        // Kai proposed it: first, though retired. Then the active ones in the team file's order,
        // then the paused, then the retired.
        assert_eq!(
            watching.orchestrator.agents_for(Some(plan)),
            Ok(["kai", "mo", "pat", "lia", "ned", "oz"]
                .map(String::from)
                .to_vec())
        );
        // With no plan: the same order, with none first.
        assert_eq!(
            watching.orchestrator.agents_for(None),
            Ok(["mo", "pat", "lia", "ned", "kai", "oz"]
                .map(String::from)
                .to_vec())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_store_error_stops_the_watch() {
        let watching = Watching::new("spend-store-error").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 500)]);
        refuse_appends_of(
            &watching.ads.harness.project.deps.log,
            EventKind::MarketingBudgetReached,
        );

        let ended = tokio::time::timeout(
            Duration::from_secs(10),
            watching.orchestrator.watch_marketing_spend(),
        )
        .await
        .expect("the watch ends");

        assert!(
            matches!(&ended, Err(OrchestratorError::Store(error))
                if error.to_string().contains("this log refuses marketing_budget.reached")),
            "{ended:?}"
        );
        assert!(watching.orchestrator.is_stopped());
    }

    /// MP-1 ended by the owner, with five of its campaigns made: 11 and 12 and 13 in the plan's
    /// ad account, 21 in another.
    fn an_ended_plan(watching: &Watching) {
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.made(("MP-1", "search-long"), (CUSTOMER, 12));
        watching.made(("MP-1", "search-extra"), (CUSTOMER, 13));
        watching.made(("MP-1", "search-other"), (OTHER, 21));
        watching.ends("MP-1");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one end through the wake that pauses, the refusal and the retry"
    )]
    async fn ending_a_plan_pauses_its_campaigns_within_a_minute() {
        let watching = Watching::new("end-pauses").await;
        an_ended_plan(&watching);
        let (first, second, third, other) = (
            campaign_name(CUSTOMER, 11),
            campaign_name(CUSTOMER, 12),
            campaign_name(CUSTOMER, 13),
            campaign_name(OTHER, 21),
        );
        // Google reports the second paused already; the third it will not pause.
        watching.status((CUSTOMER, 12), "PAUSED");
        watching.ads.google.script(|script| {
            script
                .refuse_pause
                .insert(campaign_name(CUSTOMER, 13), "nope".to_string());
        });

        // The very next wake, a minute on, not the next read of fifteen: one status read for each
        // ad account, with no metrics, and a pause for each campaign reported running.
        watching.at(1);
        watching.wakes().await;
        assert_eq!(
            watching.status_reads(),
            [
                status_query(&[first.clone(), second.clone(), third.clone()]).expect("a query"),
                status_query(std::slice::from_ref(&other)).expect("a query"),
            ]
        );
        assert!(
            watching.spend_reads().is_empty(),
            "an ended plan's spend is not read"
        );
        assert_eq!(
            watching.pauses(),
            [first.clone(), third.clone(), other.clone()]
        );
        // Recorded for every campaign Google took or reported paused, the second included, and
        // not for the one it refused.
        let plan_ended = |campaigns: &[&str]| -> Vec<(String, String, String)> {
            let keys = [
                "search-launch",
                "search-long",
                "search-extra",
                "search-other",
            ];
            let all = [&first, &second, &third, &other];
            campaigns
                .iter()
                .map(|key| {
                    let at = keys.iter().position(|each| each == key).expect("a key");
                    ("MP-1".to_string(), (*key).to_string(), all[at].clone())
                })
                .collect()
        };
        assert_eq!(
            watching.paused_for("plan_ended"),
            plan_ended(&["search-launch", "search-long", "search-other"])
        );
        // The refusal is kept as the plan's `unstopped`, in Google's words.
        assert_eq!(
            watching
                .ads
                .state()
                .spend_reads()
                .get("MP-1")
                .and_then(|read| read.unstopped.clone())
                .as_deref(),
            Some("Google answered “nope”")
        );

        // It is not tried again before fifteen minutes have passed since the try.
        let sent = watching.ads.google.requests().len();
        watching.at(15);
        watching.wakes().await;
        assert_eq!(watching.ads.google.requests().len(), sent);

        // Fifteen minutes after it, Google takes it: the pause is recorded and the warning goes.
        watching
            .ads
            .google
            .script(|script| script.refuse_pause.clear());
        watching.at(16);
        watching.wakes().await;
        assert_eq!(
            watching.status_reads().last(),
            Some(&status_query(std::slice::from_ref(&third)).expect("a query"))
        );
        assert_eq!(watching.pauses().last(), Some(&third));
        assert_eq!(
            watching.paused_for("plan_ended"),
            plan_ended(&[
                "search-launch",
                "search-long",
                "search-other",
                "search-extra"
            ])
        );
        assert_eq!(
            watching
                .ads
                .state()
                .spend_reads()
                .get("MP-1")
                .and_then(|read| read.unstopped.clone()),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_new_version_keeps_its_carried_campaigns() {
        let watching = Watching::new("end-carried").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 100)]);
        // MP-2 replaces it with the same key in the same ad account: the campaign runs on.
        watching.ads.plan_with("MP-2", Some("MP-1"), |_| {});
        watching.ends("MP-1");
        watching.at(1);
        watching.wakes().await;
        assert!(
            watching.status_reads().is_empty(),
            "{:?}",
            watching.searches()
        );
        assert!(watching.pauses().is_empty());
        assert!(watching.paused_for("plan_ended").is_empty());

        // MP-3 replaces that in another ad account: the campaign is not its own.
        watching.ads.plan_with("MP-3", Some("MP-2"), |body| {
            body["google_ads_account"] = json!("234-567-8901");
        });
        watching.ends("MP-2");
        watching.at(2);
        watching.wakes().await;
        assert_eq!(
            watching.status_reads(),
            [status_query(&[campaign_name(CUSTOMER, 11)]).expect("a query")]
        );
        assert_eq!(watching.pauses(), [campaign_name(CUSTOMER, 11)]);
        assert_eq!(
            watching.paused_for("plan_ended"),
            [(
                "MP-1".to_string(),
                "search-launch".to_string(),
                campaign_name(CUSTOMER, 11)
            )]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn an_ended_plan_all_paused_is_not_looked_at_again() {
        let watching = Watching::new("end-once").await;
        an_ended_plan(&watching);
        watching.at(1);
        watching.wakes().await;
        assert_eq!(watching.paused_for("plan_ended").len(), 4, "each recorded");

        // Every campaign is recorded paused for the plan's end: no wake sends anything for it.
        let sent = watching.ads.google.requests().len();
        for minutes in [2, 16, 61, 600] {
            watching.at(minutes);
            watching.wakes().await;
        }
        assert_eq!(watching.ads.google.requests().len(), sent);
        assert_eq!(watching.paused_for("plan_ended").len(), 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn the_end_pause_holds_the_ads_lock() {
        let watching = Watching::new("end-lock").await;
        an_ended_plan(&watching);
        watching.at(1);

        let held = watching.ads.state().ads_writes().lock().await;
        let wake = {
            let orchestrator = Arc::clone(&watching.orchestrator);
            tokio::spawn(async move { orchestrator.marketing_spend_wake().await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(watching.searches().is_empty(), "{:?}", watching.searches());
        assert!(!wake.is_finished());

        drop(held);
        wake.await.expect("joined").expect("the wake runs");
        assert_eq!(watching.paused_for("plan_ended").len(), 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn an_ended_plan_s_ads_pause_with_its_agent_paused() {
        let watching = watching_with_kai("end-kai-paused", "paused").await;
        an_ended_plan(&watching);
        watching.at(1);

        watching.wakes().await;

        let campaigns = [
            campaign_name(CUSTOMER, 11),
            campaign_name(CUSTOMER, 12),
            campaign_name(CUSTOMER, 13),
            campaign_name(OTHER, 21),
        ];
        assert_eq!(watching.pauses(), campaigns);
        assert_eq!(watching.paused_for("plan_ended").len(), 4);
        let unstopped = watching
            .ads
            .state()
            .spend_reads()
            .get("MP-1")
            .and_then(|read| read.unstopped.clone());
        assert_eq!(unstopped, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_removal_s_pause_counts_when_the_plan_ends_until_google_ads_is_connected_again() {
        let watching = Watching::new("end-after-removal").await;
        watching.ads.plan("MP-1", None);
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        let campaign = campaign_name(CUSTOMER, 11);

        // The owner removed Kai's Google Ads: Farik paused the campaign first and recorded it, and
        // the entry and its sign-in went.
        let files = &watching.ads.harness.project.deps.files;
        let team = crate::daemon::with_server(
            &files.read_team().expect("the team"),
            "kai",
            "google-ads",
            None,
        )
        .expect("a team");
        files.write_team(&team).expect("the team is written");
        let project = &watching.ads.harness.project;
        project.record(
            "",
            "marketing_campaign.paused",
            &json!({
                "plan": "MP-1", "key": "search-launch", "campaign": campaign,
                "why": "connection_removed"
            }),
        );
        project.record(
            "",
            "connector.disconnected",
            &json!({ "agent": "kai", "server": "google-ads" }),
        );

        // The plan ends. The campaign is paused already: no pause is sent, none is recorded, and
        // no "ads still running" is kept for the owner, though no sign-in is left to ask Google.
        watching.ends("MP-1");
        watching.at(1);
        watching.wakes().await;
        assert!(watching.searches().is_empty(), "{:?}", watching.searches());
        assert!(watching.paused_for("plan_ended").is_empty());
        let unstopped = watching
            .ads
            .state()
            .spend_reads()
            .get("MP-1")
            .and_then(|read| read.unstopped.clone());
        assert_eq!(unstopped, None);

        // Google Ads is connected again, and an agent could have enabled the campaign since: it
        // is paused for the end, as before.
        watching.ads.connect("kai");
        let mut connected =
            farik_protocol::event::fixtures::a_body_wire(EventKind::ConnectorConnected);
        connected["agent"] = json!("kai");
        connected["server"] = json!("google-ads");
        project.record("", "connector.connected", &connected);
        watching.at(2);
        watching.wakes().await;
        assert_eq!(watching.pauses(), std::slice::from_ref(&campaign));
        assert_eq!(
            watching.paused_for("plan_ended"),
            [("MP-1".to_string(), "search-launch".to_string(), campaign)]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn waits_for_the_first_campaign_before_it_asks_the_sleeper() {
        let sleeper = Arc::new(Counting::default());
        let ads = Ads::new("spend-dormant").await;
        let watching = Watching::over(ads, sleeper.clone());
        watching.ads.plan("MP-1", None);
        let watch = {
            let orchestrator = Arc::clone(&watching.orchestrator);
            tokio::spawn(async move { orchestrator.watch_marketing_spend().await })
        };

        // With no campaign made there is nothing to read or to pause, and nothing to wake for.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(sleeper.0.load(Ordering::SeqCst), 0);
        assert!(watching.searches().is_empty());
        assert!(!watch.is_finished());

        // The first campaign wakes it: it reads, and then waits a minute on the sleeper.
        watching.made(("MP-1", "search-launch"), (CUSTOMER, 11));
        watching.costs(CUSTOMER, &[(11, 100)]);
        watching.ads.state().campaign_made().notify_one();
        tokio::time::timeout(Duration::from_secs(10), async {
            while sleeper.0.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the watch asks the sleeper once it has read");
        assert_eq!(watching.spend_reads().len(), 1);

        // And stop ends it.
        watching.orchestrator.stop();
        let ended = tokio::time::timeout(Duration::from_secs(10), watch)
            .await
            .expect("the watch ends")
            .expect("joined");
        assert_eq!(ended, Ok(()));
    }
}
