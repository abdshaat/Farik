//! The marketing plan the owner approves (`docs/SPEC.md` section 6.5, ADR 0042): the checks a
//! proposal passes before it is recorded, and which approved plan is the active one. Pure: the
//! caller passes in today's date and the plans' records.

use std::fmt;

use chrono::NaiveDate;

/// An amount of money in hundredths of the plan's currency, so that sums and comparisons never
/// meet a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(pub u64);

/// Reads a decimal string such as `120`, `120.5` or `120.50` as hundredths: no sign, no
/// separator, at most eight digits before the point and two after, and no leading zero. `None`
/// for anything else.
#[must_use]
pub fn parse_amount(text: &str) -> Option<Amount> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || whole.len() > 8 || (whole.len() > 1 && whole.starts_with('0')) {
        return None;
    }
    let hundredths = match fraction {
        None => 0,
        Some(fraction) if digits(fraction) && fraction.len() <= 2 => {
            let number: u64 = fraction.parse().ok()?;
            if fraction.len() == 1 {
                number * 10
            } else {
                number
            }
        }
        Some(_) => return None,
    };
    Some(Amount(whole.parse::<u64>().ok()? * 100 + hundredths))
}

/// A network a post slot names (the eleven of Buffer's that Farik plans for).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PostChannel {
    /// Instagram.
    Instagram,
    /// X.
    X,
    /// Facebook.
    Facebook,
    /// `LinkedIn`.
    Linkedin,
    /// Threads.
    Threads,
    /// Bluesky.
    Bluesky,
    /// `TikTok`.
    Tiktok,
    /// Pinterest.
    Pinterest,
    /// `YouTube`.
    Youtube,
    /// Google Business Profile.
    GoogleBusiness,
    /// Mastodon.
    Mastodon,
}

/// One paid campaign of a plan, on Google Ads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCampaign {
    /// Its key, unique across the plan's campaigns and posts.
    pub key: String,
    /// Its name.
    pub name: String,
    /// What it is for.
    pub goal: String,
    /// What it may spend.
    pub budget: Amount,
    /// Its first day.
    pub starts_on: NaiveDate,
    /// Its last day.
    pub ends_on: NaiveDate,
}

/// One post slot of a plan: a channel, a day and a topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostSlot {
    /// Its key, unique across the plan's campaigns and posts.
    pub key: String,
    /// The network.
    pub channel: PostChannel,
    /// The day.
    pub on: NaiveDate,
    /// What the post is about.
    pub topic: String,
}

/// A marketing plan as the agent proposes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanProposal {
    /// Its title.
    pub title: String,
    /// Two or three plain sentences for the owner.
    pub summary: String,
    /// The plan in full.
    pub text: String,
    /// Its first day.
    pub starts_on: NaiveDate,
    /// Its last day.
    pub ends_on: NaiveDate,
    /// The currency of every amount, as an ISO 4217 code.
    pub currency: String,
    /// The most the plan spends in all.
    pub total: Amount,
    /// The part of the total for Google Ads.
    pub google_ads: Amount,
    /// The paid campaigns.
    pub campaigns: Vec<PlanCampaign>,
    /// The post slots.
    pub posts: Vec<PostSlot>,
    /// How success is measured.
    pub measures: Vec<String>,
    /// The Google Ads account the campaigns run in.
    pub google_ads_account: Option<String>,
    /// The approved plan this one supersedes.
    pub replaces: Option<String>,
}

/// One fault of a proposal, with the field it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalRefusal {
    /// The refusal's wire code.
    pub code: &'static str,
    /// The field, such as `summary` or `campaigns[1].budget`.
    pub field: String,
    /// What is wrong and what would fix it, in plain words.
    pub message: String,
}

impl fmt::Display for ProposalRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}: {}", self.code, self.field, self.message)
    }
}

/// The most days a plan may run, its first and last included.
const MOST_DAYS: i64 = 92;
/// The most campaigns a plan may hold.
const MOST_CAMPAIGNS: usize = 10;
/// The most post slots a plan may hold.
const MOST_POSTS: usize = 200;
/// The most measures a plan may name.
const MOST_MEASURES: usize = 10;
/// The longest key a campaign or a post slot may have.
const LONGEST_KEY: usize = 40;

/// Checks a proposal as the tool does before recording it, and gives every fault at once. A plan
/// may start yesterday, so that an owner west of UTC is not refused their own today.
///
/// # Errors
///
/// Every fault, in the order of the proposal's fields.
pub fn check_proposal(
    proposal: &PlanProposal,
    today: NaiveDate,
) -> Result<(), Vec<ProposalRefusal>> {
    let mut faults = Faults::default();
    faults.text("title", "title", &proposal.title, 3, 100);
    faults.text("summary", "summary", &proposal.summary, 20, 600);
    faults.text("text", "text", &proposal.text, 200, 16_000);
    check_dates(proposal, today, &mut faults);
    if !is_a_currency(&proposal.currency) {
        faults.push(
            "marketing_plan_currency",
            "currency",
            "the currency is a three-letter code in capitals, such as USD or EUR".to_string(),
        );
    }
    check_budget(proposal, &mut faults);
    let mut keys = Vec::new();
    check_campaigns(proposal, &mut keys, &mut faults);
    check_posts(proposal, &mut keys, &mut faults);
    check_measures(proposal, &mut faults);
    check_account(proposal, &mut faults);
    faults.into_result()
}

/// The faults found so far.
#[derive(Default)]
struct Faults(Vec<ProposalRefusal>);

impl Faults {
    fn push(&mut self, code: &'static str, field: &str, message: String) {
        self.0.push(ProposalRefusal {
            code,
            field: field.to_string(),
            message,
        });
    }

    /// A text of `min` to `max` characters once its edges are trimmed, as `name` in the message.
    fn text(&mut self, name: &str, field: &str, text: &str, min: usize, max: usize) {
        self.length("marketing_plan_text", name, field, text, min, max);
    }

    fn length(
        &mut self,
        code: &'static str,
        name: &str,
        field: &str,
        text: &str,
        min: usize,
        max: usize,
    ) {
        let length = text.trim().chars().count();
        if !(min..=max).contains(&length) {
            self.push(
                code,
                field,
                format!("the {name} is {length} characters; it must be {min} to {max}"),
            );
        }
    }

    fn into_result(self) -> Result<(), Vec<ProposalRefusal>> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0)
        }
    }
}

fn check_dates(proposal: &PlanProposal, today: NaiveDate, faults: &mut Faults) {
    let yesterday = today.pred_opt().unwrap_or(today);
    if proposal.starts_on < yesterday {
        faults.push(
            "marketing_plan_dates",
            "starts_on",
            format!(
                "the plan starts on {}; it may start yesterday ({yesterday}) or later",
                proposal.starts_on
            ),
        );
    }
    let days = proposal
        .ends_on
        .signed_duration_since(proposal.starts_on)
        .num_days()
        + 1;
    if days < 1 {
        faults.push(
            "marketing_plan_dates",
            "ends_on",
            format!(
                "the plan ends on {}, before it starts on {}",
                proposal.ends_on, proposal.starts_on
            ),
        );
    } else if days > MOST_DAYS {
        faults.push(
            "marketing_plan_dates",
            "ends_on",
            format!("the plan runs {days} days; it may run {MOST_DAYS} at most, both ends counted"),
        );
    }
}

fn is_a_currency(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn check_budget(proposal: &PlanProposal, faults: &mut Faults) {
    if proposal.google_ads > proposal.total {
        faults.push(
            "marketing_plan_budget",
            "budget.google_ads",
            "the Google Ads budget is more than the plan's total budget".to_string(),
        );
    }
    let campaigns = proposal
        .campaigns
        .iter()
        .fold(0_u64, |sum, campaign| sum.saturating_add(campaign.budget.0));
    if campaigns > proposal.google_ads.0 {
        faults.push(
            "marketing_plan_budget",
            "campaigns",
            "the campaigns' budgets add up to more than the Google Ads budget".to_string(),
        );
    }
}

/// Whether `key` is lower-case letters and digits in groups joined by single hyphens, at most
/// `LONGEST_KEY` characters.
fn is_a_key(key: &str) -> bool {
    key.len() <= LONGEST_KEY
        && !key.is_empty()
        && key.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Checks one key and records it; a key already seen is a fault.
fn check_key<'a>(key: &'a str, field: &str, seen: &mut Vec<&'a str>, faults: &mut Faults) {
    if !is_a_key(key) {
        faults.push(
            "marketing_plan_key",
            field,
            format!(
                "the key {key:?} must be lower-case letters and digits joined by single hyphens, \
                 at most {LONGEST_KEY} characters"
            ),
        );
    } else if seen.contains(&key) {
        faults.push(
            "marketing_plan_key",
            field,
            format!("the key {key:?} is used twice; each campaign and post has its own"),
        );
    }
    seen.push(key);
}

fn check_campaigns<'a>(proposal: &'a PlanProposal, keys: &mut Vec<&'a str>, faults: &mut Faults) {
    if proposal.campaigns.len() > MOST_CAMPAIGNS {
        faults.push(
            "marketing_plan_campaign",
            "campaigns",
            format!(
                "the plan has {} campaigns; it may have {MOST_CAMPAIGNS} at most",
                proposal.campaigns.len()
            ),
        );
    }
    for (index, campaign) in proposal.campaigns.iter().enumerate() {
        let field = |name: &str| format!("campaigns[{index}].{name}");
        check_key(&campaign.key, &field("key"), keys, faults);
        faults.length(
            "marketing_plan_campaign",
            "campaign's name",
            &field("name"),
            &campaign.name,
            1,
            100,
        );
        faults.length(
            "marketing_plan_campaign",
            "campaign's goal",
            &field("goal"),
            &campaign.goal,
            1,
            300,
        );
        if campaign.budget.0 == 0 {
            faults.push(
                "marketing_plan_campaign",
                &field("budget"),
                "a campaign's budget is more than nothing".to_string(),
            );
        }
        let inside = |day: NaiveDate| (proposal.starts_on..=proposal.ends_on).contains(&day);
        if !inside(campaign.starts_on) {
            faults.push(
                "marketing_plan_campaign",
                &field("starts_on"),
                format!(
                    "the campaign starts on {}, outside the plan's dates",
                    campaign.starts_on
                ),
            );
        }
        if !inside(campaign.ends_on) || campaign.ends_on < campaign.starts_on {
            faults.push(
                "marketing_plan_campaign",
                &field("ends_on"),
                format!(
                    "the campaign ends on {}; that must be inside the plan's dates and not before \
                     the campaign starts",
                    campaign.ends_on
                ),
            );
        }
    }
}

fn check_posts<'a>(proposal: &'a PlanProposal, keys: &mut Vec<&'a str>, faults: &mut Faults) {
    if proposal.posts.len() > MOST_POSTS {
        faults.push(
            "marketing_plan_post",
            "posts",
            format!(
                "the plan has {} post slots; it may have {MOST_POSTS} at most",
                proposal.posts.len()
            ),
        );
    }
    for (index, post) in proposal.posts.iter().enumerate() {
        let field = |name: &str| format!("posts[{index}].{name}");
        check_key(&post.key, &field("key"), keys, faults);
        if !(proposal.starts_on..=proposal.ends_on).contains(&post.on) {
            faults.push(
                "marketing_plan_post",
                &field("on"),
                format!("the post is on {}, outside the plan's dates", post.on),
            );
        }
        faults.length(
            "marketing_plan_post",
            "post's topic",
            &field("topic"),
            &post.topic,
            1,
            200,
        );
    }
}

fn check_measures(proposal: &PlanProposal, faults: &mut Faults) {
    if !(1..=MOST_MEASURES).contains(&proposal.measures.len()) {
        faults.push(
            "marketing_plan_measures",
            "measures",
            format!(
                "the plan has {} measures; it needs 1 to {MOST_MEASURES}",
                proposal.measures.len()
            ),
        );
    }
    for (index, measure) in proposal.measures.iter().enumerate() {
        faults.length(
            "marketing_plan_measures",
            "measure",
            &format!("measures[{index}]"),
            measure,
            3,
            200,
        );
    }
}

/// Whether `account` is a Google Ads account number such as `123-456-7890`.
fn is_an_ad_account(account: &str) -> bool {
    let parts: Vec<&str> = account.split('-').collect();
    matches!(parts.as_slice(), [a, b, c] if a.len() == 3 && b.len() == 3 && c.len() == 4)
        && account
            .bytes()
            .all(|byte| byte == b'-' || byte.is_ascii_digit())
}

fn check_account(proposal: &PlanProposal, faults: &mut Faults) {
    let message = match (&proposal.google_ads_account, proposal.campaigns.is_empty()) {
        (None, false) => "a plan with campaigns names the Google Ads account they run in",
        (Some(account), false) if !is_an_ad_account(account) => {
            "the Google Ads account is a number such as 123-456-7890"
        }
        (Some(_), true) => "a plan with no campaigns names no Google Ads account",
        _ => return,
    };
    faults.push(
        "google_ads_account_needed",
        "google_ads_account",
        message.to_string(),
    );
}

/// Why an approved plan ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// A newer approved plan took its place.
    Replaced,
    /// The owner ended it.
    ByOwner,
    /// Its last day passed.
    Expired,
}

/// What the log says of one plan, for deciding which is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRecord {
    /// Its id, `MP-<n>`.
    pub id: String,
    /// Its first day.
    pub starts_on: NaiveDate,
    /// Its last day.
    pub ends_on: NaiveDate,
    /// The sequence number of the event that approved it, when it was approved.
    pub approved_seq: Option<u64>,
    /// Whether the owner sent it back.
    pub returned: bool,
    /// Why it ended, when it did.
    pub ended: Option<EndReason>,
}

/// Whether a plan is approved and still in force: neither sent back nor ended.
fn is_in_force(plan: &PlanRecord) -> bool {
    plan.approved_seq.is_some() && !plan.returned && plan.ended.is_none()
}

/// The active plan: among the approved plans not ended, the one approved last whose dates hold
/// today.
#[must_use]
pub fn active_plan(plans: &[PlanRecord], today: NaiveDate) -> Option<&PlanRecord> {
    plans
        .iter()
        .filter(|plan| is_in_force(plan) && (plan.starts_on..=plan.ends_on).contains(&today))
        .max_by_key(|plan| plan.approved_seq)
}

/// The ends that are due, oldest plan first: each approved plan not ended that a newer approved
/// plan replaces, with that plan's id, or whose last day has passed. A newer plan replaces an older
/// one that has not started before it does at once, and one that started earlier on the newer
/// plan's first day, provided the newer plan starts by the older one's last day; a newer plan that
/// is itself ended replaces nothing.
#[must_use]
pub fn plans_to_end(
    plans: &[PlanRecord],
    today: NaiveDate,
) -> Vec<(String, EndReason, Option<String>)> {
    plans
        .iter()
        .filter(|plan| is_in_force(plan))
        .filter_map(|plan| {
            let replacer = plans
                .iter()
                .filter(|newer| {
                    is_in_force(newer)
                        && newer.approved_seq > plan.approved_seq
                        && newer.starts_on <= plan.ends_on
                        && (plan.starts_on >= newer.starts_on || newer.starts_on <= today)
                })
                .max_by_key(|newer| newer.approved_seq);
            if let Some(newer) = replacer {
                return Some((plan.id.clone(), EndReason::Replaced, Some(newer.id.clone())));
            }
            (plan.ends_on < today).then(|| (plan.id.clone(), EndReason::Expired, None))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{Days, NaiveDate};

    use super::{
        Amount, EndReason, PlanCampaign, PlanProposal, PlanRecord, PostChannel, PostSlot,
        ProposalRefusal, active_plan, check_proposal, parse_amount, plans_to_end,
    };

    fn day(text: &str) -> NaiveDate {
        text.parse().expect("a date")
    }

    fn amount(hundredths: u64) -> Amount {
        Amount(hundredths)
    }

    /// A valid two-week proposal that starts today (2026-11-02), with one campaign and one post.
    fn a_proposal() -> PlanProposal {
        PlanProposal {
            title: "Spring launch".to_string(),
            summary: "Two weeks of posts and one small search campaign.".to_string(),
            text: "x".repeat(300),
            starts_on: day("2026-11-02"),
            ends_on: day("2026-11-15"),
            currency: "USD".to_string(),
            total: amount(200_000),
            google_ads: amount(100_000),
            campaigns: vec![PlanCampaign {
                key: "search-launch".to_string(),
                name: "Launch search".to_string(),
                goal: "Bring people to the shop".to_string(),
                budget: amount(80_000),
                starts_on: day("2026-11-03"),
                ends_on: day("2026-11-14"),
            }],
            posts: vec![PostSlot {
                key: "post-1".to_string(),
                channel: PostChannel::Instagram,
                on: day("2026-11-04"),
                topic: "Opening day".to_string(),
            }],
            measures: vec!["New customers who say they found us online".to_string()],
            google_ads_account: Some("123-456-7890".to_string()),
            replaces: None,
        }
    }

    fn today() -> NaiveDate {
        day("2026-11-02")
    }

    fn refusals(proposal: &PlanProposal) -> Vec<(&'static str, String)> {
        match check_proposal(proposal, today()) {
            Ok(()) => Vec::new(),
            Err(all) => all
                .into_iter()
                .map(|ProposalRefusal { code, field, .. }| (code, field))
                .collect(),
        }
    }

    /// The proposal after `change`, and the refusals it draws.
    fn after(change: impl FnOnce(&mut PlanProposal)) -> Vec<(&'static str, String)> {
        let mut proposal = a_proposal();
        change(&mut proposal);
        refusals(&proposal)
    }

    fn only(code: &'static str, field: &str) -> Vec<(&'static str, String)> {
        vec![(code, field.to_string())]
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one case per refusal, side by side, so a missing case is plain to see"
    )]
    fn checks_a_proposal_field_by_field() {
        assert_eq!(refusals(&a_proposal()), []);

        // Text lengths (`marketing_plan_text`), at and past each edge.
        assert_eq!(
            after(|p| p.title = "ab".into()),
            only("marketing_plan_text", "title")
        );
        assert_eq!(after(|p| p.title = "abc".into()), []);
        assert_eq!(
            after(|p| p.title = "t".repeat(101)),
            only("marketing_plan_text", "title")
        );
        assert_eq!(after(|p| p.title = "t".repeat(100)), []);
        assert_eq!(
            after(|p| p.title = "    ".into()),
            only("marketing_plan_text", "title"),
            "blank is not a title"
        );
        assert_eq!(
            after(|p| p.summary = "s".repeat(19)),
            only("marketing_plan_text", "summary")
        );
        assert_eq!(after(|p| p.summary = "s".repeat(20)), []);
        assert_eq!(
            after(|p| p.summary = "s".repeat(601)),
            only("marketing_plan_text", "summary")
        );
        assert_eq!(after(|p| p.summary = "s".repeat(600)), []);
        assert_eq!(
            after(|p| p.text = "x".repeat(199)),
            only("marketing_plan_text", "text")
        );
        assert_eq!(after(|p| p.text = "x".repeat(200)), []);
        assert_eq!(
            after(|p| p.text = "x".repeat(16_001)),
            only("marketing_plan_text", "text")
        );
        assert_eq!(after(|p| p.text = "x".repeat(16_000)), []);

        // Dates (`marketing_plan_dates`): yesterday may start a plan, the day before may not.
        assert_eq!(after(|p| p.starts_on = day("2026-11-01")), []);
        assert_eq!(
            after(|p| p.starts_on = day("2026-10-31")),
            only("marketing_plan_dates", "starts_on")
        );
        // Without campaigns or posts to fall outside the dates, `ends_on` alone is at fault.
        let bare = |change: fn(&mut PlanProposal)| {
            after(|p| {
                p.campaigns.clear();
                p.posts.clear();
                p.google_ads = amount(0);
                p.google_ads_account = None;
                change(p);
            })
        };
        assert_eq!(
            bare(|p| p.ends_on = day("2026-11-01")),
            only("marketing_plan_dates", "ends_on"),
            "ends_on before starts_on"
        );
        assert_eq!(
            bare(|p| p.ends_on = day("2026-11-02")),
            [],
            "a one-day plan"
        );
        // Ninety-two days inclusive pass, ninety-three do not.
        let start = day("2026-11-02");
        let last = start.checked_add_days(Days::new(91)).expect("a date");
        let past = start.checked_add_days(Days::new(92)).expect("a date");
        assert_eq!(
            after(|p| {
                p.campaigns.clear();
                p.posts.clear();
                p.google_ads = amount(0);
                p.google_ads_account = None;
                p.ends_on = last;
            }),
            []
        );
        assert_eq!(
            after(|p| {
                p.campaigns.clear();
                p.posts.clear();
                p.google_ads = amount(0);
                p.google_ads_account = None;
                p.ends_on = past;
            }),
            only("marketing_plan_dates", "ends_on")
        );

        // The currency.
        for bad in ["usd", "US", "USDX", "U5D", ""] {
            assert_eq!(
                after(|p| p.currency = bad.to_string()),
                only("marketing_plan_currency", "currency"),
                "{bad}"
            );
        }

        // The budget (`marketing_plan_budget`).
        assert_eq!(
            after(|p| p.google_ads = amount(200_001)),
            only("marketing_plan_budget", "budget.google_ads")
        );
        assert_eq!(after(|p| p.google_ads = amount(200_000)), []);

        // Campaigns (`marketing_plan_campaign`).
        assert_eq!(
            after(|p| {
                let one = p.campaigns[0].clone();
                p.campaigns = (0..11)
                    .map(|n| PlanCampaign {
                        key: format!("c-{n}"),
                        budget: amount(1),
                        ..one.clone()
                    })
                    .collect();
            }),
            only("marketing_plan_campaign", "campaigns")
        );
        assert_eq!(
            after(|p| p.campaigns[0].name = String::new()),
            only("marketing_plan_campaign", "campaigns[0].name")
        );
        assert_eq!(
            after(|p| p.campaigns[0].name = "n".repeat(101)),
            only("marketing_plan_campaign", "campaigns[0].name")
        );
        assert_eq!(
            after(|p| p.campaigns[0].goal = String::new()),
            only("marketing_plan_campaign", "campaigns[0].goal")
        );
        assert_eq!(
            after(|p| p.campaigns[0].goal = "g".repeat(301)),
            only("marketing_plan_campaign", "campaigns[0].goal")
        );
        assert_eq!(
            after(|p| p.campaigns[0].budget = amount(0)),
            only("marketing_plan_campaign", "campaigns[0].budget")
        );
        assert_eq!(
            after(|p| p.campaigns[0].starts_on = day("2026-11-01")),
            only("marketing_plan_campaign", "campaigns[0].starts_on")
        );
        assert_eq!(
            after(|p| p.campaigns[0].ends_on = day("2026-11-16")),
            only("marketing_plan_campaign", "campaigns[0].ends_on")
        );
        assert_eq!(
            after(|p| {
                p.campaigns[0].starts_on = day("2026-11-10");
                p.campaigns[0].ends_on = day("2026-11-09");
            }),
            only("marketing_plan_campaign", "campaigns[0].ends_on"),
            "a campaign cannot end before it starts"
        );

        // Posts (`marketing_plan_post`).
        assert_eq!(
            after(|p| {
                let one = p.posts[0].clone();
                p.posts = (0..201)
                    .map(|n| PostSlot {
                        key: format!("p-{n}"),
                        ..one.clone()
                    })
                    .collect();
            }),
            only("marketing_plan_post", "posts")
        );
        assert_eq!(
            after(|p| p.posts[0].on = day("2026-11-16")),
            only("marketing_plan_post", "posts[0].on")
        );
        assert_eq!(
            after(|p| p.posts[0].on = day("2026-11-01")),
            only("marketing_plan_post", "posts[0].on")
        );
        assert_eq!(
            after(|p| p.posts[0].topic = String::new()),
            only("marketing_plan_post", "posts[0].topic")
        );
        assert_eq!(
            after(|p| p.posts[0].topic = "t".repeat(201)),
            only("marketing_plan_post", "posts[0].topic")
        );

        // Keys (`marketing_plan_key`): shape, length and uniqueness across campaigns and posts.
        for bad in [
            "",
            "Bad",
            "has space",
            "-lead",
            "trail-",
            "two--dashes",
            &"k".repeat(41),
        ] {
            assert_eq!(
                after(|p| p.posts[0].key = bad.to_string()),
                only("marketing_plan_key", "posts[0].key"),
                "{bad:?}"
            );
        }
        assert_eq!(after(|p| p.posts[0].key = "k".repeat(40)), []);
        assert_eq!(
            after(|p| p.posts[0].key = "search-launch".to_string()),
            only("marketing_plan_key", "posts[0].key"),
            "a post cannot reuse a campaign's key"
        );
        assert_eq!(
            after(|p| {
                let again = p.posts[0].clone();
                p.posts.push(again);
            }),
            only("marketing_plan_key", "posts[1].key")
        );

        // Measures: one to ten, three to two hundred characters each.
        assert_eq!(
            after(|p| p.measures.clear()),
            only("marketing_plan_measures", "measures")
        );
        assert_eq!(
            after(|p| p.measures = vec!["a measure".to_string(); 11]),
            only("marketing_plan_measures", "measures")
        );
        assert_eq!(
            after(|p| p.measures = vec!["a measure".to_string(); 10]),
            []
        );
        assert_eq!(
            after(|p| p.measures = vec!["ab".to_string()]),
            only("marketing_plan_measures", "measures[0]")
        );
        assert_eq!(
            after(|p| p.measures = vec!["m".repeat(201)]),
            only("marketing_plan_measures", "measures[0]")
        );

        // The ad account is needed exactly when there are campaigns.
        assert_eq!(
            after(|p| p.google_ads_account = None),
            only("google_ads_account_needed", "google_ads_account")
        );
        for bad in ["1234567890", "123-456-789", "abc-def-ghij", "123-456-78901"] {
            assert_eq!(
                after(|p| p.google_ads_account = Some(bad.to_string())),
                only("google_ads_account_needed", "google_ads_account"),
                "{bad}"
            );
        }
        assert_eq!(
            after(|p| {
                p.campaigns.clear();
                p.google_ads_account = None;
            }),
            []
        );
        assert_eq!(
            after(|p| p.campaigns.clear()),
            only("google_ads_account_needed", "google_ads_account"),
            "an account without a campaign is refused"
        );

        // Every fault is reported at once, in the order of the fields.
        assert_eq!(
            after(|p| {
                p.title = "ab".to_string();
                p.currency = "usd".to_string();
                p.posts[0].topic = String::new();
            }),
            [
                ("marketing_plan_text", "title".to_string()),
                ("marketing_plan_currency", "currency".to_string()),
                ("marketing_plan_post", "posts[0].topic".to_string()),
            ]
        );
    }

    #[test]
    fn parses_amounts_in_hundredths() {
        for (text, hundredths) in [
            ("0", 0),
            ("0.5", 50),
            ("0.05", 5),
            ("12", 1200),
            ("12.5", 1250),
            ("12.50", 1250),
            ("99999999.99", 9_999_999_999),
        ] {
            assert_eq!(parse_amount(text), Some(Amount(hundredths)), "{text}");
        }
        for text in [
            "10.999",
            "1,000",
            "-1",
            "+1",
            "",
            ".5",
            "1.",
            "01",
            "00",
            "1e3",
            " 1",
            "1 ",
            "1.2.3",
            "100000000",
            "NaN",
            "١٢",
        ] {
            assert_eq!(parse_amount(text), None, "{text:?}");
        }
    }

    #[test]
    fn campaign_budgets_fit_the_channel() {
        let two = |each: u64| {
            after(|p| {
                p.google_ads = amount(100_000);
                let mut second = p.campaigns[0].clone();
                second.key = "search-two".to_string();
                p.campaigns[0].budget = amount(each);
                second.budget = amount(each);
                p.campaigns.push(second);
            })
        };
        assert_eq!(two(60_000), only("marketing_plan_budget", "campaigns"));
        assert_eq!(two(50_000), []);
    }

    fn a_record(id: &str, starts_on: &str, ends_on: &str, approved_seq: Option<u64>) -> PlanRecord {
        PlanRecord {
            id: id.to_string(),
            starts_on: day(starts_on),
            ends_on: day(ends_on),
            approved_seq,
            returned: false,
            ended: None,
        }
    }

    fn active<'a>(plans: &'a [PlanRecord], on: &str) -> Option<&'a str> {
        active_plan(plans, day(on)).map(|plan| plan.id.as_str())
    }

    fn ends(plans: &[PlanRecord], on: &str) -> Vec<(String, EndReason, Option<String>)> {
        plans_to_end(plans, day(on))
    }

    #[test]
    fn one_plan_is_active() {
        // An approved plan within its dates is active; a proposed or a returned one never is.
        let mut plans = vec![a_record("MP-1", "2026-11-02", "2026-11-30", Some(3))];
        assert_eq!(active(&plans, "2026-11-02"), Some("MP-1"));
        assert_eq!(active(&plans, "2026-11-30"), Some("MP-1"));
        assert_eq!(active(&plans, "2026-11-01"), None, "before its first day");
        assert_eq!(active(&plans, "2026-12-01"), None, "after its last day");
        plans.push(a_record("MP-2", "2026-11-02", "2026-11-30", None));
        assert_eq!(
            active(&plans, "2026-11-10"),
            Some("MP-1"),
            "a proposed plan"
        );
        let mut returned = a_record("MP-3", "2026-11-02", "2026-11-30", Some(9));
        returned.returned = true;
        plans.push(returned);
        assert_eq!(
            active(&plans, "2026-11-10"),
            Some("MP-1"),
            "a returned plan"
        );
        assert_eq!(ends(&plans, "2026-11-10"), []);

        // A later approval starting today replaces it: it is active and the older one is named.
        let plans = vec![
            a_record("MP-1", "2026-11-02", "2026-11-30", Some(3)),
            a_record("MP-2", "2026-11-10", "2026-12-20", Some(5)),
        ];
        assert_eq!(active(&plans, "2026-11-10"), Some("MP-2"));
        assert_eq!(
            ends(&plans, "2026-11-10"),
            [(
                "MP-1".to_string(),
                EndReason::Replaced,
                Some("MP-2".to_string())
            )]
        );
        // One starting later leaves the older active until that day, then names it.
        assert_eq!(active(&plans, "2026-11-09"), Some("MP-1"));
        assert_eq!(ends(&plans, "2026-11-09"), []);
        assert_eq!(active(&plans, "2026-11-20"), Some("MP-2"));

        // A newer plan that starts on or before an older one that has not started replaces it at
        // once, whatever today is.
        let plans = vec![
            a_record("MP-1", "2026-11-16", "2026-12-15", Some(3)),
            a_record("MP-2", "2026-11-03", "2026-12-20", Some(5)),
        ];
        assert_eq!(
            ends(&plans, "2026-11-02"),
            [(
                "MP-1".to_string(),
                EndReason::Replaced,
                Some("MP-2".to_string())
            )]
        );

        // A newer plan ended before its first day replaces nothing, and the older stays active.
        let mut newer = a_record("MP-2", "2026-11-10", "2026-12-20", Some(5));
        newer.ended = Some(EndReason::ByOwner);
        let plans = vec![a_record("MP-1", "2026-11-02", "2026-11-30", Some(3)), newer];
        assert_eq!(active(&plans, "2026-11-15"), Some("MP-1"));
        assert_eq!(ends(&plans, "2026-11-15"), []);

        // Past its last day a plan is expired, and an ended plan is named no more.
        let plans = vec![a_record("MP-1", "2026-11-02", "2026-11-30", Some(3))];
        assert_eq!(
            ends(&plans, "2026-12-01"),
            [("MP-1".to_string(), EndReason::Expired, None)]
        );
        assert_eq!(ends(&plans, "2026-11-30"), []);
        let mut done = a_record("MP-1", "2026-11-02", "2026-11-30", Some(3));
        done.ended = Some(EndReason::Expired);
        assert_eq!(ends(&[done.clone()], "2026-12-05"), []);
        assert_eq!(
            active(&[done], "2026-11-10"),
            None,
            "an ended plan is not active"
        );

        // A plan that ran out before the next began is expired, not replaced.
        let plans = vec![
            a_record("MP-1", "2026-11-02", "2026-11-09", Some(3)),
            a_record("MP-2", "2026-11-20", "2026-12-20", Some(5)),
        ];
        assert_eq!(
            ends(&plans, "2026-11-21"),
            [("MP-1".to_string(), EndReason::Expired, None)]
        );
        assert_eq!(active(&plans, "2026-11-21"), Some("MP-2"));
    }
}
