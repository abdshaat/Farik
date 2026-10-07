//! The fixture of the Google Ads tests: Kai connected to Google Ads as the kit has it and signed
//! in, a session holding the connector and its ticket, and Google Ads' API as a fixture. The
//! route's tests and the spend watch's share it.

use std::collections::BTreeMap;
use std::sync::Arc;

use farik_core::budget::DEFAULT_SESSION_LIMITS;
use farik_core::contract::Role;
use farik_core::governor::permissions::{PermissionTier, SessionConnector};
use farik_core::team::CustomServer;
use farik_protocol::event::EventKind;
use serde_json::{Value, json};

use super::{GOOGLE_ADS, ads_call};
use crate::connectors::{MemoryConnectorSecrets, SecretAt};
use crate::daemon::own_calls::fixtures::keep_a_sign_in;
use crate::daemon::{DaemonState, SessionRegistration, plan_tools_of};
use crate::google_ads::WRITE_TOOLS;
use crate::google_ads_fixture::{Fixture as Google, Seen};
use crate::oauth_fixture::Fixture as OAuth;
use crate::orchestrator::fixtures::Harness;
use crate::session::SessionPurpose;
use crate::tools::fixtures::with_the_marketing_specialist;

pub(crate) const ACCOUNT: &str = "123-456-7890";
pub(crate) const SESSION: &str = "session-ads";

/// Kai's kit: Google Ads as `farik connector google-ads`, signing in, its three reads
/// `network` and its seven writes `external_effect` marked as approved by the plan.
pub(crate) fn ads_kit() -> farik_roles::Kit {
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
pub(crate) fn register(harness: &Harness, connector: SessionConnector) -> String {
    harness.daemon.register_session(SessionRegistration {
        session_id: SESSION.to_string(),
        web: farik_core::governor::sites::WebAccess::Open,
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
pub(crate) struct Ads {
    pub(crate) harness: Harness,
    pub(crate) google: Google,
    pub(crate) oauth: OAuth,
    pub(crate) store: Arc<MemoryConnectorSecrets>,
    pub(crate) server: CustomServer,
    pub(crate) at: SecretAt,
    pub(crate) kit: farik_roles::Kit,
    pub(crate) grant: crate::sign_in::OAuthGrant,
    pub(crate) ticket: String,
}

impl Ads {
    pub(crate) async fn new(name: &str) -> Self {
        Self::with(name, |_, _| {}).await
    }

    /// As `new`, with `change` made to the team's wire of Kai's entry and to the tags the
    /// session is given.
    pub(crate) async fn with(
        name: &str,
        change: impl FnOnce(&mut Value, &mut CustomServer),
    ) -> Self {
        Self::with_team(name, with_the_marketing_specialist, change).await
    }

    /// As `with`, the team's wire changed by `team` first: `with_the_marketing_specialist`, or
    /// that and a second Marketing Specialist.
    pub(crate) async fn with_team(
        name: &str,
        team: impl FnOnce(&mut Value),
        change: impl FnOnce(&mut Value, &mut CustomServer),
    ) -> Self {
        let google = Google::start().await;
        let oauth = OAuth::start().await;
        let harness = Harness::new(name, team);
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
        let custom =
            farik_core::team::custom_server(&serde_json::from_value(entry).expect("a wire entry"))
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

    pub(crate) fn state(&self) -> &Arc<DaemonState> {
        &self.harness.daemon
    }

    pub(crate) async fn call(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        ads_call(self.state(), &self.ticket, tool, arguments).await
    }

    /// MP-`n`, proposed by Kai and approved by the owner, for the day of the fixture clock,
    /// 2026-09-22: `search-launch` of 500.00 to 2026-10-22 (a total budget, from two days
    /// ahead), and `search-long` of 400.00 to 2027-01-20 (a daily one), out of 1000.00 for
    /// Google Ads.
    pub(crate) fn plan(&self, plan: &str, replaces: Option<&str>) {
        self.plan_with(plan, replaces, |_| {});
    }

    /// As `plan`, with `change` made to the proposal's wire.
    pub(crate) fn plan_with(
        &self,
        plan: &str,
        replaces: Option<&str>,
        change: impl FnOnce(&mut Value),
    ) {
        self.plan_approved_at(plan, replaces, change, crate::tools::fixtures::at());
    }

    /// As `plan_with`, the owner having approved it at `approved_at`.
    pub(crate) fn plan_approved_at(
        &self,
        plan: &str,
        replaces: Option<&str>,
        change: impl FnOnce(&mut Value),
        approved_at: chrono::DateTime<chrono::Utc>,
    ) {
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
        self.harness.project.record_by(
            None,
            approved_at,
            "FRK-1",
            "marketing_plan.approved",
            &json!({ "plan": plan, "note": "" }),
        );
    }

    /// Connects `agent` to Google Ads as the kit has it and signs it in, as Kai is: the grant it
    /// holds.
    pub(crate) fn connect(&self, agent: &str) -> crate::sign_in::OAuthGrant {
        let files = &self.harness.project.deps.files;
        let team = files.read_team().expect("the team");
        let (entry, _) =
            crate::daemon::kit_entry(&self.kit, &team, agent, GOOGLE_ADS, &BTreeMap::new())
                .expect("the kit's service is the agent's");
        let team = crate::daemon::with_server(&team, agent, GOOGLE_ADS, Some(&entry))
            .expect("the entry is the team's");
        files.write_team(&team).expect("the team is written");
        let at = self
            .harness
            .daemon
            .secret_at(files.root(), agent, GOOGLE_ADS)
            .expect("an address");
        let custom =
            farik_core::team::custom_server(&serde_json::from_value(entry).expect("a wire entry"))
                .expect("a custom server");
        keep_a_sign_in(
            &self.store,
            (&custom, &at),
            &self.oauth,
            chrono::Duration::hours(1),
        )
    }

    /// Records that Kai made `campaign` number `number` in the ad account `customer` (ten
    /// digits) for the plan campaign `key` of `plan`, with a budget of `kind` (`total` or
    /// `daily`) worth `amount`.
    pub(crate) fn made_campaign(
        &self,
        (plan, key): (&str, &str),
        (customer, number): (&str, u64),
        (kind, amount): (&str, &str),
    ) {
        self.harness.project.record_by(
            Some("kai"),
            crate::tools::fixtures::at(),
            "FRK-1",
            "marketing_campaign.created",
            &json!({
                "plan": plan, "key": key, "account": crate::google_ads::dashed(customer),
                "campaign": campaign_name(customer, number),
                "budget": format!("customers/{customer}/campaignBudgets/{}", number + 100),
                "budget_kind": kind, "amount": amount,
            }),
        );
    }

    pub(crate) fn made(&self) -> Vec<farik_protocol::event::FarikEvent> {
        self.harness
            .project
            .events(&[EventKind::MarketingCampaignCreated])
    }

    pub(crate) fn mutates(&self) -> Vec<Seen> {
        self.google.requests_of("mutate")
    }
}

pub(crate) fn create(key: &str) -> Value {
    json!({
        "account": ACCOUNT, "plan_campaign": key, "name": "Launch",
        "bidding": "maximize_clicks", "max_cpc": "1.50",
        "locations": [2840], "languages": [1000]
    })
}

/// The refusal's code, before its colon.
pub(crate) fn code(refusal: &str) -> &str {
    refusal.split(':').next().unwrap_or(refusal)
}

/// The resource name of campaign `number` in the ad account `customer`.
pub(crate) fn campaign_name(customer: &str, number: u64) -> String {
    format!("customers/{customer}/campaigns/{number}")
}

/// The row of a spend read that says campaign `number` of `customer` has cost `micros`, and what
/// Google holds for it: its first and last day and its budget's figure, a total (`total`) or a
/// daily one (`amount`) in micros.
pub(crate) fn spend_row(
    (customer, number): (&str, u64),
    micros: u64,
    held: Option<(&str, &str, &str)>,
) -> Value {
    let mut row = json!({
        "campaign": { "resourceName": campaign_name(customer, number) },
        "metrics": { "costMicros": micros.to_string() },
    });
    if let Some((starts, ends, budget)) = held {
        row["campaign"]["startDateTime"] = json!(format!("{starts} 00:00:00"));
        row["campaign"]["endDateTime"] = json!(format!("{ends} 23:59:59"));
        row["campaignBudget"] = json!({ "amountMicros": budget, "totalAmountMicros": budget });
    }
    row
}
