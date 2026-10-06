//! The marketing plan the owner approves (`docs/SPEC.md` section 6.5, ADR 0042): the checks a
//! proposal passes before it is recorded, and which approved plan is the active one. Pure: the
//! caller passes in today's date and the plans' records.

use std::fmt;

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};

/// An amount of money in hundredths of the plan's currency, so that sums and comparisons never
/// meet a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(pub u64);

impl fmt::Display for Amount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

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

impl PostChannel {
    /// The eleven, in the order the plan lists them.
    pub const ALL: [Self; 11] = [
        Self::Instagram,
        Self::X,
        Self::Facebook,
        Self::Linkedin,
        Self::Threads,
        Self::Bluesky,
        Self::Tiktok,
        Self::Pinterest,
        Self::Youtube,
        Self::GoogleBusiness,
        Self::Mastodon,
    ];

    /// The wire's word for it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instagram => "instagram",
            Self::X => "x",
            Self::Facebook => "facebook",
            Self::Linkedin => "linkedin",
            Self::Threads => "threads",
            Self::Bluesky => "bluesky",
            Self::Tiktok => "tiktok",
            Self::Pinterest => "pinterest",
            Self::Youtube => "youtube",
            Self::GoogleBusiness => "google_business",
            Self::Mastodon => "mastodon",
        }
    }

    /// The channel a wire word names, when it names one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|channel| channel.as_str() == text)
    }
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

impl EndReason {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replaced => "replaced",
            Self::ByOwner => "by_owner",
            Self::Expired => "expired",
        }
    }
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

/// The most characters a post's text may have on `channel`, counted in Unicode scalar values: the
/// network's own limit (ADR 0042).
#[must_use]
pub const fn text_limit(channel: PostChannel) -> usize {
    match channel {
        PostChannel::X => 280,
        PostChannel::Bluesky => 300,
        PostChannel::Threads | PostChannel::Mastodon | PostChannel::Pinterest => 500,
        PostChannel::GoogleBusiness => 1_500,
        PostChannel::Instagram | PostChannel::Tiktok => 2_200,
        PostChannel::Linkedin => 3_000,
        PostChannel::Youtube => 5_000,
        PostChannel::Facebook => 63_206,
    }
}

/// Whether `text` may be a post on `channel`: at least one character, at most
/// [`text_limit`] of them, counted in Unicode scalar values, and no NUL.
#[must_use]
pub fn text_fits(channel: PostChannel, text: &str) -> bool {
    let characters = text.chars().count();
    (1..=text_limit(channel)).contains(&characters) && !text.contains('\0')
}

/// The network's name as the owner reads it: "Instagram", "X", "`LinkedIn`", "Google Business".
#[must_use]
pub const fn network_name(channel: PostChannel) -> &'static str {
    match channel {
        PostChannel::Instagram => "Instagram",
        PostChannel::X => "X",
        PostChannel::Facebook => "Facebook",
        PostChannel::Linkedin => "LinkedIn",
        PostChannel::Threads => "Threads",
        PostChannel::Bluesky => "Bluesky",
        PostChannel::Tiktok => "TikTok",
        PostChannel::Pinterest => "Pinterest",
        PostChannel::Youtube => "YouTube",
        PostChannel::GoogleBusiness => "Google Business",
        PostChannel::Mastodon => "Mastodon",
    }
}

/// The fifteen `YouTube` video categories Buffer takes, as the strings its `categoryId` holds.
pub const YOUTUBE_CATEGORIES: [&str; 15] = [
    "1", "2", "10", "15", "17", "19", "20", "22", "23", "24", "25", "26", "27", "28", "29",
];

/// What a post on `YouTube` or Pinterest needs besides its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostDetails {
    /// A `YouTube` video's title (1 to 100 characters) and category (one of [`YOUTUBE_CATEGORIES`]).
    Youtube {
        /// The title.
        title: String,
        /// The category's id.
        category_id: String,
    },
    /// The Pinterest board the pin goes to, by Buffer's id for it.
    Pinterest {
        /// The board's service id.
        board: String,
    },
}

/// The fewest hours between now and a post in the plan.
const LEAD_HOURS: i64 = 3;

/// A post that claims a slot of the active plan.
#[derive(Debug, Clone, Copy)]
pub struct SlotCheck<'a> {
    /// The active plan, as proposed.
    pub plan: &'a PlanProposal,
    /// The key of the slot the post claims.
    pub slot: &'a str,
    /// The channel the post is for.
    pub channel: PostChannel,
    /// When the post goes out, with the offset the agent wrote it in.
    pub at: DateTime<FixedOffset>,
    /// Now.
    pub now: DateTime<Utc>,
    /// The keys of the plan's slots another post holds, scheduled or sent.
    pub used: &'a [String],
}

/// Why a post does not fit the slot it claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRefusal {
    /// The plan has no such slot for this channel.
    NotAPlanSlot,
    /// Another post holds the slot.
    SlotUsed,
    /// The post is not on the slot's day, in the offset it was written in.
    PostOffItsDay,
    /// The post is less than three hours away.
    PostTooSoon,
}

impl SlotRefusal {
    /// The refusal's wire code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotAPlanSlot => "not_a_plan_slot",
            Self::SlotUsed => "slot_used",
            Self::PostOffItsDay => "post_off_its_day",
            Self::PostTooSoon => "post_too_soon",
        }
    }
}

/// Whether a post fills the slot it names: a post slot of the plan for the same channel, held by no
/// other post, on the slot's day (the date of `at` in its own offset, so that no time-zone table
/// is needed), and at least three hours from now. The first fault is the answer.
///
/// # Errors
///
/// The [`SlotRefusal`] of the first check the post fails.
pub fn check_slot(check: &SlotCheck<'_>) -> Result<(), SlotRefusal> {
    let slot = check
        .plan
        .posts
        .iter()
        .find(|slot| slot.key == check.slot && slot.channel == check.channel)
        .ok_or(SlotRefusal::NotAPlanSlot)?;
    if check.used.iter().any(|used| used == check.slot) {
        return Err(SlotRefusal::SlotUsed);
    }
    if check.at.date_naive() != slot.on {
        return Err(SlotRefusal::PostOffItsDay);
    }
    if check.at.with_timezone(&Utc) < check.now + Duration::hours(LEAD_HOURS) {
        return Err(SlotRefusal::PostTooSoon);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};

    use super::{
        Amount, EndReason, PlanCampaign, PlanProposal, PlanRecord, PostChannel, PostSlot,
        ProposalRefusal, SlotCheck, SlotRefusal, active_plan, check_proposal, check_slot,
        network_name, parse_amount, plans_to_end, text_fits, text_limit,
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
    fn names_each_end_as_the_wire_does() {
        assert_eq!(
            [EndReason::Replaced, EndReason::ByOwner, EndReason::Expired].map(EndReason::as_str),
            ["replaced", "by_owner", "expired"]
        );
    }

    #[test]
    fn names_the_eleven_channels_as_the_wire_does() {
        let words: Vec<&str> = PostChannel::ALL
            .iter()
            .map(|channel| channel.as_str())
            .collect();
        assert_eq!(
            words,
            [
                "instagram",
                "x",
                "facebook",
                "linkedin",
                "threads",
                "bluesky",
                "tiktok",
                "pinterest",
                "youtube",
                "google_business",
                "mastodon",
            ]
        );
        for channel in PostChannel::ALL {
            assert_eq!(PostChannel::parse(channel.as_str()), Some(channel));
        }
        for word in [
            "",
            "Instagram",
            "twitter",
            "google_ads",
            "google-business",
            " x",
        ] {
            assert_eq!(PostChannel::parse(word), None, "{word:?}");
        }
    }

    #[test]
    fn shows_an_amount_with_two_decimals_that_reads_back() {
        for (hundredths, text) in [
            (0, "0.00"),
            (5, "0.05"),
            (50, "0.50"),
            (80_050, "800.50"),
            (200_000, "2000.00"),
            (9_999_999_999, "99999999.99"),
        ] {
            assert_eq!(Amount(hundredths).to_string(), text);
            assert_eq!(parse_amount(text), Some(Amount(hundredths)), "{text}");
        }
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

    #[test]
    fn limits_each_network_s_text() {
        let limits = [
            (PostChannel::X, 280),
            (PostChannel::Bluesky, 300),
            (PostChannel::Threads, 500),
            (PostChannel::Mastodon, 500),
            (PostChannel::Pinterest, 500),
            (PostChannel::GoogleBusiness, 1_500),
            (PostChannel::Instagram, 2_200),
            (PostChannel::Tiktok, 2_200),
            (PostChannel::Linkedin, 3_000),
            (PostChannel::Youtube, 5_000),
            (PostChannel::Facebook, 63_206),
        ];
        assert_eq!(
            limits.len(),
            PostChannel::ALL.len(),
            "every network has its limit"
        );
        for (channel, limit) in limits {
            assert_eq!(text_limit(channel), limit, "{channel:?}");
        }

        // At the limit and one past it, counted in characters, not bytes.
        for (channel, limit) in [
            (PostChannel::X, 280),
            (PostChannel::Instagram, 2_200),
            (PostChannel::Facebook, 63_206),
        ] {
            assert!(
                text_fits(channel, &"a".repeat(limit)),
                "{channel:?} at {limit}"
            );
            assert!(
                !text_fits(channel, &"a".repeat(limit + 1)),
                "{channel:?} at {}",
                limit + 1
            );
            assert!(
                text_fits(channel, &"é".repeat(limit)),
                "{channel:?}: two bytes a character still counts one"
            );
        }
        // Never empty, never a NUL.
        assert!(!text_fits(PostChannel::X, ""));
        assert!(text_fits(PostChannel::X, "a"));
        assert!(!text_fits(PostChannel::X, "a\0b"));

        let names: Vec<&str> = PostChannel::ALL.into_iter().map(network_name).collect();
        assert_eq!(
            names,
            [
                "Instagram",
                "X",
                "Facebook",
                "LinkedIn",
                "Threads",
                "Bluesky",
                "TikTok",
                "Pinterest",
                "YouTube",
                "Google Business",
                "Mastodon"
            ]
        );
    }

    fn at(text: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(text).expect("a time with an offset")
    }

    fn now(text: &str) -> DateTime<Utc> {
        text.parse().expect("a UTC time")
    }

    /// The slots checked for `channel`, on `at`, `now` and with `used`, in the plan of
    /// `a_proposal` that also holds a slot on 2026-11-03 and one for X.
    fn slot_of(
        slot: &str,
        channel: PostChannel,
        post_at: &str,
        now_text: &str,
        used: &[&str],
    ) -> Result<(), SlotRefusal> {
        let mut plan = a_proposal();
        plan.posts = vec![
            PostSlot {
                key: "post-0".to_string(),
                channel: PostChannel::Instagram,
                on: day("2026-11-03"),
                topic: "Teaser".to_string(),
            },
            PostSlot {
                key: "post-1".to_string(),
                channel: PostChannel::Instagram,
                on: day("2026-11-04"),
                topic: "Opening day".to_string(),
            },
            PostSlot {
                key: "post-x".to_string(),
                channel: PostChannel::X,
                on: day("2026-11-04"),
                topic: "Opening day".to_string(),
            },
        ];
        let used: Vec<String> = used.iter().map(ToString::to_string).collect();
        check_slot(&SlotCheck {
            plan: &plan,
            slot,
            channel,
            at: at(post_at),
            now: now(now_text),
            used: &used,
        })
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one case per refusal and per offset, side by side, so a missing case is plain to see"
    )]
    fn a_slot_is_the_plan_s_on_its_day_three_hours_ahead() {
        let instagram = PostChannel::Instagram;
        let ok = |slot, channel, post_at, now_text, used: &[&str]| {
            slot_of(slot, channel, post_at, now_text, used)
        };
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-04T12:00:00+00:00",
                "2026-11-04T08:00:00Z",
                &[]
            ),
            Ok(())
        );

        // Each refusal, with its own code.
        let not_a_slot = Err(SlotRefusal::NotAPlanSlot);
        assert_eq!(
            ok(
                "nothing",
                instagram,
                "2026-11-04T12:00:00+00:00",
                "2026-11-04T08:00:00Z",
                &[]
            ),
            not_a_slot
        );
        assert_eq!(
            ok(
                "post-x",
                instagram,
                "2026-11-04T12:00:00+00:00",
                "2026-11-04T08:00:00Z",
                &[]
            ),
            not_a_slot,
            "a slot of another channel is not this channel's"
        );
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-04T12:00:00+00:00",
                "2026-11-04T08:00:00Z",
                &["post-1"]
            ),
            Err(SlotRefusal::SlotUsed)
        );
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-05T12:00:00+00:00",
                "2026-11-04T08:00:00Z",
                &["post-0"]
            ),
            Err(SlotRefusal::PostOffItsDay)
        );
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-04T10:59:59+00:00",
                "2026-11-04T08:00:00Z",
                &[]
            ),
            Err(SlotRefusal::PostTooSoon)
        );
        // Three hours exactly is far enough.
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-04T11:00:00+00:00",
                "2026-11-04T08:00:00Z",
                &[]
            ),
            Ok(())
        );
        assert_eq!(SlotRefusal::NotAPlanSlot.code(), "not_a_plan_slot");
        assert_eq!(SlotRefusal::SlotUsed.code(), "slot_used");
        assert_eq!(SlotRefusal::PostOffItsDay.code(), "post_off_its_day");
        assert_eq!(SlotRefusal::PostTooSoon.code(), "post_too_soon");

        // The slot's day is the day in the post's own offset, so no time-zone table is needed:
        // 23:30 in New York on the 3rd is the 4th in UTC, and still the slot of the 3rd.
        assert_eq!(
            ok(
                "post-0",
                instagram,
                "2026-11-03T23:30:00-05:00",
                "2026-11-03T10:00:00Z",
                &[]
            ),
            Ok(())
        );
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-03T23:30:00-05:00",
                "2026-11-03T10:00:00Z",
                &[]
            ),
            Err(SlotRefusal::PostOffItsDay)
        );
        // 00:30 in Paris on the 4th is the 3rd in UTC, and the slot of the 4th.
        assert_eq!(
            ok(
                "post-1",
                instagram,
                "2026-11-04T00:30:00+01:00",
                "2026-11-03T10:00:00Z",
                &[]
            ),
            Ok(())
        );
        assert_eq!(
            ok(
                "post-0",
                instagram,
                "2026-11-04T00:30:00+01:00",
                "2026-11-03T10:00:00Z",
                &[]
            ),
            Err(SlotRefusal::PostOffItsDay)
        );
    }
}
