//! The Marketing Specialist's tools (ADR 0042): `catervas_propose_marketing_plan`, which ends its
//! session with a plan for the owner to approve. It acts on the task's worktree; a module of its
//! own keeps the files it writes under `docs/marketing/` together.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use catervas_core::contract::{Role, TaskId};
use catervas_core::marketing::{
    Amount, PlanCampaign, PlanProposal, PostChannel, PostSlot, ProposalRefusal, check_proposal,
    parse_amount,
};
use catervas_protocol::event::{EventBody, MarketingPlanProposedBody};
use catervas_store::marketing::{MarketingPlan, marketing_plans};
use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::git::worktree;
use super::refusal::Refusal;
use super::{Call, TOOLS, ToolError, failed};
use crate::session::SessionPurpose;

/// `catervas_propose_marketing_plan`'s input.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposeMarketingPlanInput {
    /// The plan's name, 3 to 100 characters.
    title: String,
    /// Two or three plain sentences for the owner, 20 to 600 characters: what the plan does, what
    /// it costs and what they get. The owner reads this first.
    summary: String,
    /// The plan in full, 200 to 16,000 characters. Catervas also writes it to
    /// docs/marketing/plans/MP-<n>.md.
    text: String,
    /// The first day, as YYYY-MM-DD in UTC: yesterday's date or later.
    starts_on: String,
    /// The last day, as YYYY-MM-DD in UTC: at most 92 days from the first, both counted.
    ends_on: String,
    /// The currency of every amount, as a three-letter code in capitals, such as USD.
    currency: String,
    /// What the plan may spend.
    budget: PlanBudgetInput,
    /// The paid search campaigns on Google Ads, 0 to 10. Leave out when no ad account is
    /// connected.
    #[serde(default)]
    campaigns: Vec<PlanCampaignInput>,
    /// The post slots, 0 to 200.
    #[serde(default)]
    posts: Vec<PlanPostInput>,
    /// How success is measured, 1 to 10 measures of 3 to 200 characters.
    measures: Vec<String>,
    /// The Google Ads account the campaigns run in, as 123-456-7890. Give it when the plan has
    /// campaigns, and only then.
    google_ads_account: Option<String>,
    /// The id of an approved plan this one supersedes, such as MP-1. It is in the same currency as
    /// this plan: a plan in another currency replaces nothing.
    replaces: Option<String>,
}

/// What a plan may spend.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanBudgetInput {
    /// The most the plan spends in all, as a decimal string such as 2000 or 2000.50.
    total: String,
    /// The part of the total for Google Ads, as a decimal string, at most the total.
    google_ads: String,
}

/// One paid campaign of a plan.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanCampaignInput {
    /// Lower-case words joined by hyphens, at most 40 characters, unique across the campaigns and
    /// posts of the plan.
    key: String,
    /// Always `google_ads`.
    channel: String,
    /// The campaign's name, 1 to 100 characters.
    name: String,
    /// What it is for, 1 to 300 characters.
    goal: String,
    /// What it advertises: the product, service or offer, 3 to 200 characters. The owner reads it
    /// beside the campaign's price before approving the plan.
    advertises: String,
    /// What it may spend, as a decimal string above 0; the campaigns add up to at most the Google
    /// Ads budget.
    budget: String,
    /// Its first day, YYYY-MM-DD, inside the plan's dates.
    starts_on: String,
    /// Its last day, YYYY-MM-DD, inside the plan's dates.
    ends_on: String,
}

/// One post slot of a plan.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanPostInput {
    /// Lower-case words joined by hyphens, at most 40 characters, unique across the campaigns and
    /// posts of the plan.
    key: String,
    /// One of instagram, x, facebook, linkedin, threads, bluesky, tiktok, pinterest, youtube,
    /// `google_business`, mastodon.
    channel: String,
    /// The day, YYYY-MM-DD, inside the plan's dates.
    on: String,
    /// What the post is about, 1 to 200 characters.
    topic: String,
}

/// Where the marketing plans' texts live in a worktree and in the project root.
const PLANS_FOLDER: &str = "docs/marketing/plans";

/// The path of plan `MP-<number>`'s text, relative to a worktree. Number 0 stands for the number
/// not taken yet: the path of the folder's check.
pub(super) fn plan_file(number: u32) -> String {
    format!("{PLANS_FOLDER}/MP-{number}.md")
}

/// The tool's name.
const PROPOSE_TOOL: &str = "catervas_propose_marketing_plan";

/// Held from taking a plan's number to recording it, so that two sessions in one daemon never take
/// the same number. A second daemon on the same project would need the files' own check, which
/// `write_new` makes.
static NUMBERING: Mutex<()> = Mutex::new(());

fn refused(code: &'static str, detail: impl Into<String>) -> ToolError {
    Refusal::MarketingPlan {
        code,
        detail: detail.into(),
    }
    .into()
}

/// A fault of the plan that Catervas, not the checks of `catervas_core`, found.
fn fault(code: &'static str, field: &str, message: impl Into<String>) -> ProposalRefusal {
    ProposalRefusal {
        code,
        field: field.to_string(),
        message: message.into(),
    }
}

/// `catervas_propose_marketing_plan`: checks the plan, numbers it, writes its text to the task's
/// worktree and records `marketing_plan.proposed`, from the Marketing Specialist's implement
/// session about a task. The task then waits on the owner.
pub(crate) fn propose_plan(
    call: &Call<'_>,
    input: &ProposeMarketingPlanInput,
) -> Result<Value, ToolError> {
    let task = match &call.context.task_id {
        Some(task)
            if call.role() == Role::MarketingSpecialist
                && call.context.purpose == SessionPurpose::Implement =>
        {
            task
        }
        _ => {
            return Err(refused(
                "marketing_plan_refused",
                "only the Marketing Specialist proposes a marketing plan, in its implement \
                 session of a task",
            ));
        }
    };
    let deps = call.deps();
    let _held = NUMBERING.lock().unwrap_or_else(PoisonError::into_inner);
    let plans = marketing_plans(&deps.log).map_err(failed)?;
    if let Some(waiting) = waiting_on(&plans, task) {
        return Err(waiting_refusal(&waiting.record.id));
    }
    let proposal = read_input(input).map_err(faults)?;
    let mut found = check_proposal(&proposal, deps.clock.now().date_naive())
        .err()
        .unwrap_or_default();
    if let Some(replaced) = &proposal.replaces {
        let in_force = plans.iter().find(|plan| {
            &plan.record.id == replaced
                && plan.record.approved_seq.is_some()
                && plan.record.ended.is_none()
        });
        match in_force {
            None => found.push(fault(
                "marketing_plan_unknown",
                "replaces",
                format!("{replaced} is not an approved plan that is still in force"),
            )),
            // The campaigns Catervas made for the old plan hold its amounts in its currency, which
            // the new plan's figures would be read as (ADR 0042, SPEC 6.7): a plan in another
            // currency is a new plan, started when the old one has ended.
            Some(plan) if plan.proposal.currency != proposal.currency => found.push(fault(
                "marketing_plan_currency",
                "currency",
                format!(
                    "{replaced} is in {}, and a plan replaces another only in the same currency, \
                     not in {}; to change currency, wait for {replaced} to end and propose a plan \
                     that replaces nothing",
                    plan.proposal.currency, proposal.currency
                ),
            )),
            Some(_) => {}
        }
    }
    if !found.is_empty() {
        return Err(faults(found));
    }

    let worktree = worktree(call, task);
    let number = 1 + highest_number(
        &plans,
        [
            &deps.files.root().join(PLANS_FOLDER),
            &worktree.join(PLANS_FOLDER),
        ],
    );
    let id = format!("MP-{number}");
    let path = plan_file(number);
    let tool = TOOLS
        .iter()
        .find(|tool| tool.name == PROPOSE_TOOL)
        .ok_or_else(|| ToolError::Failed {
            detail: format!("{PROPOSE_TOOL} is not listed"),
        })?;
    call.permit(tool, vec![path.clone()])?;
    if !worktree.is_dir() {
        return Err(failed(format!(
            "{} has no worktree to write {path} in",
            task.as_str()
        )));
    }
    let title = input.title.split_whitespace().collect::<Vec<_>>().join(" ");
    let file = worktree.join(&path);
    write_new(
        &file,
        &format!("# {id}: {title}\n\n{}\n", input.text.trim_end()),
    )?;
    let recorded = call.append(
        Some(task),
        EventBody::MarketingPlanProposed(body_of(call, &id, input, &proposal)?),
    );
    if let Err(error) = recorded {
        // The number was never recorded; the next try may take it.
        let _ = std::fs::remove_file(&file);
        return Err(error);
    }
    Ok(json!({
        "plan": id,
        "next": "end your turn: the owner's decision starts the next session",
    }))
}

/// The plan of `task` that waits for the owner, when there is one.
fn waiting_on<'a>(plans: &'a [MarketingPlan], task: &TaskId) -> Option<&'a MarketingPlan> {
    plans
        .iter()
        .find(|plan| &plan.task_id == task && plan.decided.is_none())
}

fn waiting_refusal(plan: &str) -> ToolError {
    refused(
        "marketing_plan_waiting",
        format!("{plan} waits for the owner; end your turn"),
    )
}

/// Refuses, as `marketing_plan_waiting`, the task's own plan while it waits for the owner: its
/// assignee does not hand the task in meanwhile (ADR 0042).
pub(super) fn refuse_while_waiting(call: &Call<'_>, task: &TaskId) -> Result<(), ToolError> {
    let plans = marketing_plans(&call.deps().log).map_err(failed)?;
    match waiting_on(&plans, task) {
        Some(waiting) => Err(waiting_refusal(&waiting.record.id)),
        None => Ok(()),
    }
}

fn faults(faults: Vec<ProposalRefusal>) -> ToolError {
    Refusal::MarketingPlanFaults { faults }.into()
}

/// Reads the input's dates, amounts and channels, or says which it cannot read. A proposal whose
/// figures cannot be read is not checked further: its other faults would be guesses.
fn read_input(input: &ProposeMarketingPlanInput) -> Result<PlanProposal, Vec<ProposalRefusal>> {
    let mut unreadable = Vec::new();
    let starts_on = date(
        &input.starts_on,
        "marketing_plan_dates",
        "starts_on",
        &mut unreadable,
    );
    let ends_on = date(
        &input.ends_on,
        "marketing_plan_dates",
        "ends_on",
        &mut unreadable,
    );
    let campaigns = read_campaigns(&input.campaigns, &mut unreadable);
    let posts = read_posts(&input.posts, &mut unreadable);
    let total = amount(
        &input.budget.total,
        "marketing_plan_budget",
        "budget.total",
        &mut unreadable,
    );
    let google_ads = amount(
        &input.budget.google_ads,
        "marketing_plan_budget",
        "budget.google_ads",
        &mut unreadable,
    );
    match (starts_on, ends_on, total, google_ads) {
        (Some(starts_on), Some(ends_on), Some(total), Some(google_ads))
            if unreadable.is_empty() =>
        {
            Ok(PlanProposal {
                title: input.title.clone(),
                summary: input.summary.clone(),
                text: input.text.clone(),
                starts_on,
                ends_on,
                currency: input.currency.clone(),
                total,
                google_ads,
                campaigns,
                posts,
                measures: input.measures.clone(),
                google_ads_account: input.google_ads_account.clone(),
                replaces: input.replaces.clone(),
            })
        }
        _ => Err(unreadable),
    }
}

/// The campaigns whose dates and budget can be read, saying which cannot.
fn read_campaigns(
    given: &[PlanCampaignInput],
    unreadable: &mut Vec<ProposalRefusal>,
) -> Vec<PlanCampaign> {
    let mut campaigns = Vec::new();
    for (index, campaign) in given.iter().enumerate() {
        let field = |name: &str| format!("campaigns[{index}].{name}");
        let first = date(
            &campaign.starts_on,
            "marketing_plan_campaign",
            &field("starts_on"),
            unreadable,
        );
        let last = date(
            &campaign.ends_on,
            "marketing_plan_campaign",
            &field("ends_on"),
            unreadable,
        );
        let budget = amount(
            &campaign.budget,
            "marketing_plan_campaign",
            &field("budget"),
            unreadable,
        );
        if campaign.channel != "google_ads" {
            unreadable.push(fault(
                "marketing_plan_campaign",
                &field("channel"),
                format!(
                    "{:?} is not a channel for a campaign; it is google_ads",
                    campaign.channel
                ),
            ));
        }
        if let (Some(starts_on), Some(ends_on), Some(budget)) = (first, last, budget) {
            campaigns.push(PlanCampaign {
                key: campaign.key.clone(),
                name: campaign.name.clone(),
                goal: campaign.goal.clone(),
                advertises: campaign.advertises.clone(),
                budget,
                starts_on,
                ends_on,
            });
        }
    }
    campaigns
}

/// The post slots whose day and channel can be read, saying which cannot.
fn read_posts(given: &[PlanPostInput], unreadable: &mut Vec<ProposalRefusal>) -> Vec<PostSlot> {
    let mut posts = Vec::new();
    for (index, post) in given.iter().enumerate() {
        let field = |name: &str| format!("posts[{index}].{name}");
        let on = date(&post.on, "marketing_plan_post", &field("on"), unreadable);
        let channel = PostChannel::parse(&post.channel);
        if channel.is_none() {
            unreadable.push(fault(
                "marketing_plan_post",
                &field("channel"),
                format!("{:?} is not one of the eleven channels", post.channel),
            ));
        }
        if let (Some(on), Some(channel)) = (on, channel) {
            posts.push(PostSlot {
                key: post.key.clone(),
                channel,
                on,
                topic: post.topic.clone(),
            });
        }
    }
    posts
}

fn date(
    text: &str,
    code: &'static str,
    field: &str,
    unreadable: &mut Vec<ProposalRefusal>,
) -> Option<NaiveDate> {
    let read = if text.len() == 10 {
        text.parse::<NaiveDate>().ok()
    } else {
        None
    };
    if read.is_none() {
        unreadable.push(fault(
            code,
            field,
            format!("{text:?} is not a date written YYYY-MM-DD"),
        ));
    }
    read
}

fn amount(
    text: &str,
    code: &'static str,
    field: &str,
    unreadable: &mut Vec<ProposalRefusal>,
) -> Option<Amount> {
    let read = parse_amount(text);
    if read.is_none() {
        unreadable.push(fault(
            code,
            field,
            format!("{text:?} is not an amount: digits with at most two decimals, such as 1200 or 1200.50"),
        ));
    }
    read
}

/// The highest plan number the log and the plans folders hold; 0 when none.
fn highest_number(plans: &[MarketingPlan], folders: [&Path; 2]) -> u32 {
    let numbered = |name: &str| name.strip_prefix("MP-")?.parse::<u32>().ok();
    let from_log = plans.iter().filter_map(|plan| numbered(&plan.record.id));
    let from_files = folders.into_iter().flat_map(|folder| {
        std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                numbered(name.strip_suffix(".md")?)
            })
    });
    from_log.chain(from_files).max().unwrap_or(0)
}

/// Writes `text` to a file that is not there yet, making its folders.
fn write_new(path: &Path, text: &str) -> Result<(), ToolError> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(failed)?;
    }
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(refused(
                "marketing_plan_file_exists",
                format!("{} is there already", path.display()),
            ));
        }
        Err(error) => return Err(failed(error)),
    };
    file.write_all(text.as_bytes()).map_err(failed)
}

/// The event's body for a plan the checks passed, from the input's own words.
fn body_of(
    call: &Call<'_>,
    id: &str,
    input: &ProposeMarketingPlanInput,
    proposal: &PlanProposal,
) -> Result<MarketingPlanProposedBody, ToolError> {
    let mut body = json!({
        "plan": id,
        "title": input.title,
        "summary": input.summary,
        "text": input.text,
        "starts_on": proposal.starts_on.to_string(),
        "ends_on": proposal.ends_on.to_string(),
        "currency": input.currency,
        "budget": { "total": input.budget.total, "google_ads": input.budget.google_ads },
        "campaigns": input.campaigns.iter().zip(&proposal.campaigns).map(|(given, read)| json!({
            "key": given.key,
            "channel": given.channel,
            "name": given.name,
            "goal": given.goal,
            "advertises": given.advertises,
            "budget": given.budget,
            "starts_on": read.starts_on.to_string(),
            "ends_on": read.ends_on.to_string(),
        })).collect::<Vec<_>>(),
        "posts": input.posts.iter().zip(&proposal.posts).map(|(given, read)| json!({
            "key": given.key,
            "channel": read.channel.as_str(),
            "on": read.on.to_string(),
            "topic": given.topic,
        })).collect::<Vec<_>>(),
        "measures": input.measures,
        "proposed_by": call.agent_id(),
    });
    for (name, value) in [
        ("google_ads_account", &input.google_ads_account),
        ("replaces", &input.replaces),
    ] {
        if let Some(value) = value {
            body[name] = json!(value);
        }
    }
    serde_json::from_value(body).map_err(failed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use catervas_core::team::fixtures::an_agent_wire;
    use catervas_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use crate::tools::fixtures::{TestProject, a_team_of_three};
    use crate::tools::{ToolContext, ToolError};

    /// Kai the Marketing Specialist beside the team of three.
    fn a_marketing_project(name: &str) -> TestProject {
        TestProject::new(
            name,
            &a_team_of_three(|wire| {
                wire["agents"]
                    .as_array_mut()
                    .expect("a list of agents")
                    .push(an_agent_wire("kai", "marketing_specialist"));
            }),
        )
    }

    /// `task` in progress with Kai, allowed `docs/marketing/**`, and its worktree made.
    fn works_on(project: &TestProject, task: &str) -> PathBuf {
        project.filed_with(task, "assigned", "task", None, |wire| {
            wire["allowed_paths"] = json!(["docs/marketing/**"]);
            wire["assignee_role"] = json!("marketing_specialist");
            wire["reviewer_role"] = json!("product_manager");
        });
        project.moved(
            task,
            "assigned",
            "in_progress",
            &json!({ "assignee": "kai", "reviewer": "pm" }),
        );
        let worktree = project
            .repo
            .path
            .join(".catervas/local/worktrees")
            .join(task);
        project
            .deps
            .git
            .create_worktree(&worktree, &project.branch(task), "main")
            .expect("the task's worktree is made");
        worktree
    }

    /// A valid plan for the clock of the tool fixtures, 2026-09-22 UTC.
    fn a_plan() -> Value {
        json!({
            "title": "Spring launch",
            "summary": "Two weeks of posts and one small search campaign.",
            "text": "x".repeat(300),
            "starts_on": "2026-09-22",
            "ends_on": "2026-10-05",
            "currency": "USD",
            "budget": { "total": "2000.00", "google_ads": "1000" },
            "campaigns": [{
                "key": "search-launch",
                "channel": "google_ads",
                "name": "Launch search",
                "goal": "Bring people to the shop",
                "advertises": "Handmade candles from the shop",
                "budget": "800.50",
                "starts_on": "2026-09-23",
                "ends_on": "2026-10-04"
            }],
            "posts": [{
                "key": "post-1",
                "channel": "instagram",
                "on": "2026-09-24",
                "topic": "Opening day"
            }],
            "measures": ["New customers who say they found us online"],
            "google_ads_account": "123-456-7890"
        })
    }

    fn propose(project: &TestProject, task: &str, plan: &Value) -> Result<Value, ToolError> {
        project.call(
            "kai",
            Some(task),
            "catervas_propose_marketing_plan",
            plan.clone(),
        )
    }

    fn refused(error: ToolError) -> String {
        match error {
            ToolError::Refused { reason } => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn proposed_events(project: &TestProject) -> usize {
        project.events(&[EventKind::MarketingPlanProposed]).len()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn proposes_a_plan_and_writes_its_text() {
        let project = a_marketing_project("tools-plan-proposes");
        let worktree = works_on(&project, "CTV-1");
        assert!(
            !project
                .deps
                .projections
                .task(&"CTV-1".parse().expect("an id"))
                .expect("reads")
                .expect("a row")
                .waiting_on_human
        );

        let answer = propose(&project, "CTV-1", &a_plan()).expect("the plan is proposed");

        assert_eq!(answer["plan"], "MP-1");
        assert!(
            answer["next"]
                .as_str()
                .expect("a sentence")
                .starts_with("end your turn"),
            "{answer}"
        );
        let events = project.events(&[EventKind::MarketingPlanProposed]);
        assert_eq!(events.len(), 1);
        let EventBody::MarketingPlanProposed(body) = &events[0].body else {
            panic!("a marketing plan was proposed");
        };
        assert_eq!(body.plan.as_str(), "MP-1");
        assert_eq!(body.title, "Spring launch");
        assert_eq!(
            body.summary,
            "Two weeks of posts and one small search campaign."
        );
        assert_eq!(body.text.len(), 300);
        assert_eq!(body.starts_on.to_string(), "2026-09-22");
        assert_eq!(body.ends_on.to_string(), "2026-10-05");
        assert_eq!(body.currency.as_str(), "USD");
        assert_eq!(body.budget.total.as_str(), "2000.00");
        assert_eq!(body.budget.google_ads.as_str(), "1000");
        assert_eq!(body.campaigns.len(), 1);
        assert_eq!(body.campaigns[0].key.as_str(), "search-launch");
        assert_eq!(body.campaigns[0].budget.as_str(), "800.50");
        assert_eq!(body.posts.len(), 1);
        assert_eq!(body.posts[0].channel.to_string(), "instagram");
        assert_eq!(
            body.measures,
            ["New customers who say they found us online"]
        );
        assert_eq!(
            body.google_ads_account
                .as_ref()
                .map(|account| account.as_str()),
            Some("123-456-7890")
        );
        assert_eq!(body.replaces, None);
        assert_eq!(body.proposed_by, "kai");
        let ids = &events[0].envelope.ids;
        assert_eq!(ids.agent_id.as_deref(), Some("kai"));
        assert_eq!(ids.session_id.as_deref(), Some("session-1"));
        assert_eq!(
            ids.task_id.as_ref().map(|task| task.to_string()),
            Some("CTV-1".to_string())
        );

        let file = std::fs::read_to_string(worktree.join("docs/marketing/plans/MP-1.md"))
            .expect("the plan's text is written to the task's worktree");
        assert_eq!(
            file,
            format!("# MP-1: Spring launch\n\n{}\n", "x".repeat(300))
        );
        assert!(
            !project
                .repo
                .path
                .join("docs/marketing/plans/MP-1.md")
                .exists(),
            "the project root is not written"
        );

        let row = project
            .deps
            .projections
            .task(&"CTV-1".parse().expect("an id"))
            .expect("the board reads")
            .expect("a row");
        assert!(row.waiting_on_human, "the task waits on the owner");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn numbers_past_a_committed_plan() {
        let project = a_marketing_project("tools-plan-numbers");
        // A committed plan in the project root, with an empty log: the next is MP-5.
        let committed = project.repo.path.join("docs/marketing/plans");
        std::fs::create_dir_all(&committed).expect("a directory");
        std::fs::write(committed.join("MP-4.md"), "# MP-4\n").expect("a file");
        works_on(&project, "CTV-1");
        assert_eq!(
            propose(&project, "CTV-1", &a_plan()).expect("proposed")["plan"],
            "MP-5"
        );

        // The log's own plans count too, though no file of this task's worktree carries them.
        let other = works_on(&project, "CTV-2");
        assert_eq!(
            propose(&project, "CTV-2", &a_plan()).expect("proposed")["plan"],
            "MP-6"
        );
        assert!(other.join("docs/marketing/plans/MP-6.md").is_file());

        // And so does a file already in the task's own worktree.
        let third = works_on(&project, "CTV-3");
        std::fs::create_dir_all(third.join("docs/marketing/plans")).expect("a directory");
        std::fs::write(third.join("docs/marketing/plans/MP-9.md"), "# MP-9\n").expect("a file");
        std::fs::write(third.join("docs/marketing/plans/notes.md"), "not a plan\n")
            .expect("a file");
        assert_eq!(
            propose(&project, "CTV-3", &a_plan()).expect("proposed")["plan"],
            "MP-10"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_bad_proposal_with_every_reason() {
        let project = a_marketing_project("tools-plan-faults");
        let worktree = works_on(&project, "CTV-1");
        let before = project.event_count();
        let mut plan = a_plan();
        plan["title"] = json!("ab");
        plan["ends_on"] = json!("2026-12-31");
        plan["posts"][0]["topic"] = json!("");
        plan["budget"]["google_ads"] = json!("2500");

        let reason = refused(propose(&project, "CTV-1", &plan).expect_err("a plan with faults"));

        for code in [
            "marketing_plan_text",
            "marketing_plan_dates",
            "marketing_plan_post",
            "marketing_plan_budget",
        ] {
            assert!(reason.contains(code), "{code} is missing: {reason}");
        }
        assert!(reason.starts_with("marketing_plan_text"), "{reason}");
        for field in ["title", "ends_on", "posts[0].topic", "budget.google_ads"] {
            assert!(reason.contains(field), "{field} is missing: {reason}");
        }
        assert_eq!(project.event_count(), before, "nothing is recorded");
        assert!(
            !worktree.join("docs/marketing/plans").exists(),
            "nothing is written"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_tool_asks_what_each_campaign_advertises() {
        let project = a_marketing_project("tools-plan-advertises");
        works_on(&project, "CTV-1");

        // A campaign that does not say what it advertises is not a campaign this tool takes.
        let mut plan = a_plan();
        plan["campaigns"][0]
            .as_object_mut()
            .expect("a campaign")
            .remove("advertises");
        match propose(&project, "CTV-1", &plan).expect_err("no advertises") {
            ToolError::InvalidInput { detail } => {
                assert!(detail.contains("advertises"), "{detail}");
            }
            other => panic!("expected invalid input, got {other:?}"),
        }
        assert_eq!(proposed_events(&project), 0);

        // One that says too little is refused by the plan's checks, by its field.
        plan["campaigns"][0]["advertises"] = json!("ab");
        let reason = refused(propose(&project, "CTV-1", &plan).expect_err("too short"));
        assert!(
            reason.starts_with("marketing_plan_campaign: campaigns[0].advertises:"),
            "{reason}"
        );
        assert_eq!(proposed_events(&project), 0);

        // And what it says is recorded as the agent wrote it.
        plan["campaigns"][0]["advertises"] = json!("Handmade candles");
        propose(&project, "CTV-1", &plan).expect("proposed");
        let events = project.events(&[EventKind::MarketingPlanProposed]);
        let EventBody::MarketingPlanProposed(body) = &events[0].body else {
            panic!("a marketing plan was proposed");
        };
        assert_eq!(
            body.campaigns[0].advertises.as_deref(),
            Some("Handmade candles")
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reports_a_figure_it_cannot_read_by_its_field() {
        let project = a_marketing_project("tools-plan-figures");
        works_on(&project, "CTV-1");
        let mut plan = a_plan();
        plan["budget"]["total"] = json!("1,000");
        plan["starts_on"] = json!("next Monday");
        plan["campaigns"][0]["budget"] = json!("-5");
        plan["campaigns"][0]["channel"] = json!("tiktok_ads");
        plan["posts"][0]["channel"] = json!("telegram");

        let reason = refused(propose(&project, "CTV-1", &plan).expect_err("unreadable figures"));

        for (code, field) in [
            ("marketing_plan_budget", "budget.total"),
            ("marketing_plan_dates", "starts_on"),
            ("marketing_plan_campaign", "campaigns[0].budget"),
            ("marketing_plan_campaign", "campaigns[0].channel"),
            ("marketing_plan_post", "posts[0].channel"),
        ] {
            assert!(
                reason.contains(&format!("{code}: {field}"))
                    || reason.contains(&format!("{code}: {field}:")),
                "{code} {field}: {reason}"
            );
        }
        assert_eq!(proposed_events(&project), 0);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_plan_to_replace_that_is_not_approved() {
        let project = a_marketing_project("tools-plan-replaces");
        works_on(&project, "CTV-1");
        let mut plan = a_plan();
        plan["replaces"] = json!("MP-7");
        let reason = refused(propose(&project, "CTV-1", &plan).expect_err("no such plan"));
        assert!(reason.contains("marketing_plan_unknown"), "{reason}");
        assert!(reason.contains("replaces"), "{reason}");

        // A plan the owner approved can be replaced.
        let first = propose(&project, "CTV-1", &a_plan()).expect("proposed");
        assert_eq!(first["plan"], "MP-1");
        project.record(
            "CTV-1",
            "marketing_plan.approved",
            &json!({ "plan": "MP-1", "note": "" }),
        );
        works_on(&project, "CTV-2");
        let mut newer = a_plan();
        newer["replaces"] = json!("MP-1");
        assert_eq!(
            propose(&project, "CTV-2", &newer).expect("replaces")["plan"],
            "MP-2"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_plan_that_replaces_one_in_another_currency() {
        let project = a_marketing_project("tools-plan-currency");
        works_on(&project, "CTV-1");
        propose(&project, "CTV-1", &a_plan()).expect("a plan in dollars");
        project.record(
            "CTV-1",
            "marketing_plan.approved",
            &json!({ "plan": "MP-1", "note": "" }),
        );
        works_on(&project, "CTV-2");
        let mut yen = a_plan();
        yen["currency"] = json!("JPY");
        yen["replaces"] = json!("MP-1");

        let reason = refused(propose(&project, "CTV-2", &yen).expect_err("another currency"));

        assert!(
            reason.contains("marketing_plan_currency: currency"),
            "{reason}"
        );
        assert!(
            reason.contains("MP-1 is in USD") && reason.contains("JPY"),
            "{reason}"
        );
        assert_eq!(proposed_events(&project), 1, "only the plan in dollars");

        // The same plan in the same currency replaces it.
        yen["currency"] = json!("USD");
        assert_eq!(
            propose(&project, "CTV-2", &yen).expect("the same currency")["plan"],
            "MP-2"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_second_plan_on_the_task() {
        let project = a_marketing_project("tools-plan-waiting");
        works_on(&project, "CTV-1");
        propose(&project, "CTV-1", &a_plan()).expect("the first plan");

        let reason =
            refused(propose(&project, "CTV-1", &a_plan()).expect_err("one plan waits already"));

        assert!(
            reason.starts_with("marketing_plan_waiting: MP-1 waits for the owner; end your turn"),
            "{reason}"
        );
        assert_eq!(proposed_events(&project), 1);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_another_role_or_session() {
        let project = a_marketing_project("tools-plan-refused");
        works_on(&project, "CTV-1");
        // A Developer allowed to write there passes the tier and path checks and meets the gate.
        project.filed_with("CTV-2", "in_progress", "task", None, |wire| {
            wire["allowed_paths"] = json!(["docs/marketing/**"]);
        });
        let kai = |purpose: crate::session::SessionPurpose, task: Option<&str>| {
            let mut context: ToolContext = project.context("kai", task);
            context.purpose = purpose;
            crate::tools::fixtures::run(&context, "catervas_propose_marketing_plan", a_plan())
        };

        let developer = project.call(
            "dev-a",
            Some("CTV-2"),
            "catervas_propose_marketing_plan",
            a_plan(),
        );
        assert!(refused(developer.expect_err("a Developer")).starts_with("marketing_plan_refused"),);
        for (purpose, task) in [
            (crate::session::SessionPurpose::Verify, Some("CTV-1")),
            (crate::session::SessionPurpose::Chat, Some("CTV-1")),
        ] {
            let reason =
                refused(kai(purpose, task).expect_err("not an implement session of a task"));
            assert!(
                reason.starts_with("marketing_plan_refused"),
                "{purpose:?}: {reason}"
            );
        }
        // A session about no task has no contract to allow the plan's folder: the path check
        // refuses it before the gate is asked.
        let reason = refused(
            kai(crate::session::SessionPurpose::Implement, None).expect_err("no task, no paths"),
        );
        assert!(reason.starts_with("path_outside_allowed"), "{reason}");
        assert_eq!(proposed_events(&project), 0);
    }

    #[test]
    fn refuses_to_write_over_a_plan_that_is_there() {
        let folder =
            std::env::temp_dir().join(format!("catervas-plan-file-{}", std::process::id()));
        let file = folder.join("docs/marketing/plans/MP-3.md");
        super::write_new(&file, "# MP-3: First\n").expect("a new file is written");
        let reason =
            refused(super::write_new(&file, "# MP-3: Second\n").expect_err("the file is there"));
        assert!(reason.starts_with("marketing_plan_file_exists"), "{reason}");
        assert_eq!(
            std::fs::read_to_string(&file).expect("reads"),
            "# MP-3: First\n",
            "the plan that was there is untouched"
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_path_the_contract_does_not_allow() {
        let project = a_marketing_project("tools-plan-paths");
        project.filed_with("CTV-1", "in_progress", "task", None, |wire| {
            wire["allowed_paths"] = json!(["docs/marketing/research/**"]);
            wire["assignee_role"] = json!("marketing_specialist");
        });
        let reason = refused(
            propose(&project, "CTV-1", &a_plan()).expect_err("the plans folder is not allowed"),
        );
        assert!(reason.starts_with("path_outside_allowed"), "{reason}");
        assert_eq!(proposed_events(&project), 0);
    }
}
