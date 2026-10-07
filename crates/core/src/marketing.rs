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
    /// What it advertises: the product, service or offer, 3 to 200 characters. Empty for a plan
    /// proposed before the field existed (ADR 0042).
    pub advertises: String,
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
        faults.length(
            "marketing_plan_campaign",
            "campaign's description of what it advertises",
            &field("advertises"),
            &campaign.advertises,
            3,
            200,
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

/// The currencies with no minor unit (ISO 4217's exponent 0): Google takes a budget in them only
/// in whole units.
pub const ZERO_DECIMAL: &[&str] = &[
    "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG", "RWF", "UGX", "UYI", "VND",
    "VUV", "XAF", "XOF", "XPF",
];

/// Whether a campaign's budget is for its whole run, or a daily one (ADR 0042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetKind {
    /// One total for the campaign's run, which Google never bills past.
    Total,
    /// A daily amount, which bounds Google's own charging while Farik is not running.
    Daily,
}

impl BudgetKind {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::Daily => "daily",
        }
    }
}

/// A Google Ads campaign Farik made for a plan campaign, as `marketing_campaign.created` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedCampaign {
    /// The plan it was made under, `MP-<n>`.
    pub plan: String,
    /// The plan campaign's key.
    pub key: String,
    /// Its resource name at Google, `customers/<id>/campaigns/<id>`.
    pub campaign: String,
    /// Its budget's resource name at Google.
    pub budget: String,
    /// Whether the budget is a total or a daily one.
    pub kind: BudgetKind,
}

/// The first day a plan campaign made on `today` can start: its own first day, or two days ahead
/// when that is not later, so that no time zone makes it the past: in a zone east of UTC the
/// account's own date can be a day ahead of the UTC date, so tomorrow's UTC date can be its today,
/// and a start at 00:00:00 that day is already past. Every day is a UTC date.
#[must_use]
pub fn first_day(campaign: &PlanCampaign, today: NaiveDate) -> NaiveDate {
    campaign.starts_on.max(today + Duration::days(2))
}

/// The budget Google keeps for a plan campaign made on `today`, and its amount (`docs/SPEC.md`
/// 6.7). The campaign's run is from its first day (`first_day`) to its last, both included. A run
/// of 3 to 90 days takes a total budget for the run, the plan campaign's budget less `spent`:
/// Google never bills past one. The daemon asks with nothing spent, since a key has one campaign
/// across a plan's whole lineage, so none of its budget was spent before the campaign is made. Any other run takes a daily budget,
/// what is left divided by the days of the run and rounded down to the hundredth, which bounds
/// Google's own charging while Farik is not running. In a currency of `ZERO_DECIMAL` the amount
/// is whole units, rounded down.
#[must_use]
pub fn campaign_budget(
    campaign: &PlanCampaign,
    currency: &str,
    spent: Amount,
    today: NaiveDate,
) -> (BudgetKind, Amount) {
    let run = (campaign.ends_on - first_day(campaign, today)).num_days() + 1;
    let days = u64::try_from(run).unwrap_or(0).max(1);
    let left = campaign.budget.0.saturating_sub(spent.0);
    let (kind, hundredths) = if (3..=90).contains(&days) {
        (BudgetKind::Total, left)
    } else {
        (BudgetKind::Daily, left / days)
    };
    let whole = ZERO_DECIMAL.contains(&currency);
    (
        kind,
        Amount(if whole {
            hundredths / 100 * 100
        } else {
            hundredths
        }),
    )
}

/// Whether the price of a campaign is fixed before the owner approves it (ADR 0042, amended
/// 2026-10-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceKind {
    /// A total budget, which Google never bills past.
    Fixed,
    /// A daily budget, which may spend past its cap for about an hour, since Google reports cost
    /// that late.
    NotFixed,
}

impl PriceKind {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::NotFixed => "not_fixed",
        }
    }
}

/// The price kind of a plan campaign made on `day`: `campaign_budget`'s kind, `Fixed` for a total
/// budget and `NotFixed` for a daily one.
#[must_use]
pub fn price_kind(campaign: &PlanCampaign, currency: &str, day: NaiveDate) -> PriceKind {
    match campaign_budget(campaign, currency, Amount(0), day).0 {
        BudgetKind::Total => PriceKind::Fixed,
        BudgetKind::Daily => PriceKind::NotFixed,
    }
}

/// What Google holds for a campaign when it is enabled, read from Google for the call: its
/// budget's amount as its kind keeps it (a total budget's total, a daily one's daily amount) and
/// the days it runs, first and last. A replacing plan that lowers a key's budget or shortens its dates leaves
/// these as the creating plan made them, so enabling is checked against them. `None` is a figure
/// Google's answer did not give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeldAtGoogle {
    /// The budget's amount.
    pub amount: Option<Amount>,
    /// The first day, a date in the ad account's time zone.
    pub starts_on: Option<NaiveDate>,
    /// The last day, a date in the ad account's time zone.
    pub ends_on: Option<NaiveDate>,
}

/// One change to Google Ads the agent asks for, by what it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdsWrite {
    /// A new campaign for the plan campaign `plan_campaign`.
    Create {
        /// The ad account, as `NNN-NNN-NNNN`.
        account: String,
        /// The plan campaign's key.
        plan_campaign: String,
    },
    /// An ad group, keywords, negative keywords or an ad, under `campaign`.
    UnderCampaign {
        /// The ad account.
        account: String,
        /// The campaign's resource name.
        campaign: String,
    },
    /// A new budget for `campaign`.
    Budget {
        /// The ad account.
        account: String,
        /// The campaign's resource name.
        campaign: String,
        /// The new amount.
        amount: Amount,
    },
    /// Running `campaign`.
    Enable {
        /// The ad account.
        account: String,
        /// The campaign's resource name.
        campaign: String,
        /// What Google holds for it, read for the call.
        held: HeldAtGoogle,
    },
    /// Pausing `campaign`.
    Pause {
        /// The ad account.
        account: String,
        /// The campaign's resource name.
        campaign: String,
    },
}

/// What a write is checked against: the active plan, its lineage, what was made under it and what
/// was spent.
#[derive(Debug, Clone, Copy)]
pub struct AdsPlanView<'a> {
    /// The active plan's id.
    pub plan_id: &'a str,
    /// The active plan.
    pub plan: &'a PlanProposal,
    /// The active plan and every plan it replaces, `replaces` followed through the whole chain.
    pub lineage: &'a [String],
    /// Every campaign Farik made for any plan.
    pub created: &'a [CreatedCampaign],
    /// What each plan campaign's key has spent, in all the campaigns of the lineage.
    pub spent: &'a std::collections::BTreeMap<String, Amount>,
    /// Today's UTC date.
    pub today: NaiveDate,
    /// The UTC date the owner approved the active plan, whose price kinds the owner saw.
    pub approved_on: NaiveDate,
}

/// The ad account `write` is for.
fn account_of(write: &AdsWrite) -> &str {
    match write {
        AdsWrite::Create { account, .. }
        | AdsWrite::UnderCampaign { account, .. }
        | AdsWrite::Budget { account, .. }
        | AdsWrite::Enable { account, .. }
        | AdsWrite::Pause { account, .. } => account,
    }
}

/// Whether the active plan covers `write` (ADR 0042, `docs/SPEC.md` 6.7), the plan's lineage being
/// the active plan and every plan it replaces:
///
/// - every write: the account is the plan's `google_ads_account`;
/// - a create: the key is one of the plan's campaigns, no campaign was made for it under any plan
///   of the lineage, and its last day is not before the first day it can start;
/// - a write under a campaign, a new budget or enabling it: the campaign was made for a key the
///   active plan has, under a plan of the lineage, in the plan's account; a total budget's new
///   amount is from what the campaign spent to the plan campaign's budget, a daily one at most
///   what is left over the days left; enabling needs today within the plan campaign's dates, its
///   spend below its budget and the plan's Google Ads spend below the plan's;
/// - pausing: any campaign made under a plan of the lineage, whether or not the active plan still
///   has its key.
///
/// # Errors
///
/// The sentence that says why not.
pub fn check_ads_write(view: &AdsPlanView<'_>, write: &AdsWrite) -> Result<(), String> {
    let account = account_of(write);
    match view.plan.google_ads_account.as_deref() {
        None => return Err("the active plan names no Google Ads account".to_string()),
        Some(plan_account) if plan_account != account => {
            return Err(format!(
                "the active plan runs in Google Ads account {plan_account}, not {account}"
            ));
        }
        Some(_) => {}
    }
    match write {
        AdsWrite::Create { plan_campaign, .. } => check_create(view, plan_campaign),
        AdsWrite::UnderCampaign { campaign, .. } => {
            made_for_plan(view, campaign, account).map(|_| ())
        }
        AdsWrite::Budget {
            campaign, amount, ..
        } => check_new_budget(view, campaign, account, *amount),
        AdsWrite::Enable { campaign, held, .. } => check_enable(view, campaign, account, *held),
        AdsWrite::Pause { campaign, .. } => {
            let made = view
                .created
                .iter()
                .find(|made| made.campaign == *campaign && view.lineage.contains(&made.plan))
                .ok_or_else(|| {
                    format!(
                        "that campaign was not made for {} or a plan it replaces, so Farik \
                         leaves it alone",
                        view.plan_id
                    )
                })?;
            in_account(&made.campaign, account)
        }
    }
}

/// A create of the plan campaign `key`.
fn check_create(view: &AdsPlanView<'_>, key: &str) -> Result<(), String> {
    let campaign = view
        .plan
        .campaigns
        .iter()
        .find(|campaign| campaign.key == key)
        .ok_or_else(|| format!("the active plan has no campaign {key}"))?;
    if let Some(made) = view
        .created
        .iter()
        .find(|made| made.key == key && view.lineage.contains(&made.plan))
    {
        return Err(format!(
            "{key} has a campaign already ({}), made for {}",
            made.campaign, made.plan
        ));
    }
    let start = first_day(campaign, view.today);
    if campaign.ends_on < start {
        return Err(format!(
            "{key} ends on {}, before the first day it could start, {start}",
            campaign.ends_on
        ));
    }
    // The owner approved a fixed price: a campaign made with a daily budget would not be it.
    let currency = &view.plan.currency;
    if price_kind(campaign, currency, view.approved_on) == PriceKind::Fixed
        && price_kind(campaign, currency, view.today) == PriceKind::NotFixed
    {
        return Err(format!(
            "{key} was approved at a fixed price, which Google keeps only for a run of 3 days or \
             more; propose a new version"
        ));
    }
    Ok(())
}

/// The record of `campaign`, made under a plan of the lineage for a key the active plan has, and
/// the plan campaign of that key.
fn made_for_plan<'a>(
    view: &AdsPlanView<'a>,
    campaign: &str,
    account: &str,
) -> Result<(&'a CreatedCampaign, &'a PlanCampaign), String> {
    let found = view
        .created
        .iter()
        .filter(|made| made.campaign == campaign && view.lineage.contains(&made.plan))
        .find_map(|made| {
            view.plan
                .campaigns
                .iter()
                .find(|planned| planned.key == made.key)
                .map(|planned| (made, planned))
        })
        .ok_or_else(|| {
            format!(
                "that campaign was not made for the active plan {}, so Farik leaves it alone",
                view.plan_id
            )
        })?;
    in_account(campaign, account)?;
    Ok(found)
}

/// Whether the campaign's resource name is in the ad account `account` (`NNN-NNN-NNNN`).
fn in_account(campaign: &str, account: &str) -> Result<(), String> {
    let customer = campaign
        .strip_prefix("customers/")
        .and_then(|rest| rest.split('/').next());
    if customer == Some(account.replace('-', "").as_str()) {
        Ok(())
    } else {
        Err(format!(
            "that campaign is in another Google Ads account than the plan's, {account}"
        ))
    }
}

/// What a plan campaign's key has spent so far.
fn spent_by(view: &AdsPlanView<'_>, key: &str) -> Amount {
    view.spent.get(key).copied().unwrap_or(Amount(0))
}

/// A new budget for `campaign`.
fn check_new_budget(
    view: &AdsPlanView<'_>,
    campaign: &str,
    account: &str,
    amount: Amount,
) -> Result<(), String> {
    let (made, planned) = made_for_plan(view, campaign, account)?;
    let key = &made.key;
    let spent = spent_by(view, key);
    match made.kind {
        BudgetKind::Total => {
            if amount > planned.budget {
                return Err(format!(
                    "a total budget of {amount} is more than {key}'s {} in the plan",
                    planned.budget
                ));
            }
            if amount < spent {
                return Err(format!(
                    "a total budget of {amount} is less than the {spent} {key} has spent"
                ));
            }
            Ok(())
        }
        BudgetKind::Daily => {
            let from = planned.starts_on.max(view.today);
            let days = u64::try_from((planned.ends_on - from).num_days() + 1).unwrap_or(0);
            if days == 0 {
                return Err(format!("{key} ended on {}", planned.ends_on));
            }
            let left = Amount(planned.budget.0.saturating_sub(spent.0));
            let most = Amount(left.0 / days);
            if amount > most {
                return Err(format!(
                    "a daily budget of {amount} is more than the {most} a day that {key}'s {left} \
                     left allows over {days} days"
                ));
            }
            Ok(())
        }
    }
}

/// Running `campaign`: today within its dates, its spend below its budget and the plan's below
/// the plan's, and what Google holds for it no more than the active plan gives its key: a plan
/// that replaces another and lowers a key's budget, or shortens its dates, leaves the campaign's
/// budget and end as the first plan made them, and Farik can lower the budget but not the end.
fn check_enable(
    view: &AdsPlanView<'_>,
    campaign: &str,
    account: &str,
    held: HeldAtGoogle,
) -> Result<(), String> {
    let (made, planned) = made_for_plan(view, campaign, account)?;
    let key = &made.key;
    if view.today < planned.starts_on {
        return Err(format!("{key} starts on {}", planned.starts_on));
    }
    if view.today > planned.ends_on {
        return Err(format!("{key} ended on {}", planned.ends_on));
    }
    let spent = spent_by(view, key);
    if spent >= planned.budget {
        return Err(format!("{key} has spent {spent} of its {}", planned.budget));
    }
    let all = Amount(view.spent.values().map(|each| each.0).sum());
    if all >= view.plan.google_ads {
        return Err(format!(
            "the plan's Google Ads spend is {all} of its {}",
            view.plan.google_ads
        ));
    }
    let Some(ends_on) = held.ends_on else {
        return Err(format!(
            "Farik could not read when {key} ends at Google, so it cannot check it against the plan"
        ));
    };
    if ends_on > planned.ends_on {
        return Err(format!(
            "{key} ends on {ends_on} at Google, after the {} the plan gives it, and Farik cannot \
             change a campaign's end",
            planned.ends_on
        ));
    }
    let Some(amount) = held.amount else {
        return Err(format!(
            "Farik could not read the budget of {key} at Google, so it cannot check it against \
             the plan"
        ));
    };
    match made.kind {
        BudgetKind::Total if amount > planned.budget => Err(format!(
            "{key}'s total budget at Google is {amount}, more than the {} the plan gives it; lower \
             it first",
            planned.budget
        )),
        BudgetKind::Total => Ok(()),
        BudgetKind::Daily => {
            // Google runs it from its first day, or today, to its last, both included, and bills
            // up to the daily amount each day.
            let Some(starts_on) = held.starts_on else {
                return Err(format!(
                    "Farik could not read when {key} starts at Google, so it cannot check it \
                     against the plan"
                ));
            };
            let days =
                u64::try_from((ends_on - starts_on.max(view.today)).num_days() + 1).unwrap_or(0);
            let left = Amount(planned.budget.0.saturating_sub(spent.0));
            if amount.0.saturating_mul(days) > left.0 {
                return Err(format!(
                    "{key}'s daily budget at Google is {amount}, which over the {days} days it \
                     runs is more than the {left} the plan has left for it; lower it first"
                ));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};

    use super::{
        AdsPlanView, AdsWrite, Amount, BudgetKind, CreatedCampaign, EndReason, HeldAtGoogle,
        PlanCampaign, PlanProposal, PlanRecord, PostChannel, PostSlot, PriceKind, ProposalRefusal,
        SlotCheck, SlotRefusal, ZERO_DECIMAL, active_plan, campaign_budget, check_ads_write,
        check_proposal, check_slot, network_name, parse_amount, plans_to_end, price_kind,
        text_fits, text_limit,
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
                advertises: "Handmade candles from the shop".to_string(),
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

    /// A plan campaign of `budget` hundredths between two days.
    fn a_campaign(budget: u64, starts_on: &str, ends_on: &str) -> PlanCampaign {
        PlanCampaign {
            key: "search".to_string(),
            name: "Search".to_string(),
            goal: "Sales".to_string(),
            advertises: "Handmade candles".to_string(),
            budget: amount(budget),
            starts_on: day(starts_on),
            ends_on: day(ends_on),
        }
    }

    /// The budget of a campaign that starts on `starts_on` and ends `days - 1` days later, made on
    /// 2026-11-02 with nothing spent.
    fn budget_of_a_run(days: u64, starts_on: &str) -> (BudgetKind, Amount) {
        let first = day(starts_on);
        let last = first.checked_add_days(Days::new(days - 1)).expect("a date");
        campaign_budget(
            &a_campaign(30_000, starts_on, &last.to_string()),
            "USD",
            amount(0),
            today(),
        )
    }

    #[test]
    fn a_short_or_long_campaign_takes_a_daily_budget() {
        // From two days ahead (2026-11-04) to the last day, both included: 2 and 91 days are
        // daily, 3 and 90 total.
        assert_eq!(
            budget_of_a_run(2, "2026-11-04"),
            (BudgetKind::Daily, amount(15_000))
        );
        assert_eq!(
            budget_of_a_run(3, "2026-11-04"),
            (BudgetKind::Total, amount(30_000))
        );
        assert_eq!(
            budget_of_a_run(90, "2026-11-04"),
            (BudgetKind::Total, amount(30_000))
        );
        // 300.00 over 91 days is 3.2967: rounded down to the hundredth.
        assert_eq!(
            budget_of_a_run(91, "2026-11-04"),
            (BudgetKind::Daily, amount(329))
        );
        // A one-day run is a daily budget of all of it, never a division by nothing.
        assert_eq!(
            budget_of_a_run(1, "2026-11-04"),
            (BudgetKind::Daily, amount(30_000))
        );

        // A campaign that starts today starts two days ahead, and is counted from there: four
        // days from its own first day are two from the 4th, so daily; five are three, so total.
        assert_eq!(
            budget_of_a_run(4, "2026-11-02"),
            (BudgetKind::Daily, amount(15_000))
        );
        assert_eq!(
            budget_of_a_run(5, "2026-11-02"),
            (BudgetKind::Total, amount(30_000))
        );
        // One that starts tomorrow starts two days ahead too: three days from the 3rd are two
        // from the 4th, so daily; four are three, so total.
        assert_eq!(
            budget_of_a_run(3, "2026-11-03"),
            (BudgetKind::Daily, amount(15_000))
        );
        assert_eq!(
            budget_of_a_run(4, "2026-11-03"),
            (BudgetKind::Total, amount(30_000))
        );
        // One that began before today is counted from two days ahead too: it ended on the 29th of
        // October, before that, so its run counts as one day, a daily budget of all of it.
        assert_eq!(
            budget_of_a_run(10, "2026-10-20"),
            (BudgetKind::Daily, amount(30_000))
        );

        // What earlier versions of it spent comes off, never below nothing.
        let campaign = a_campaign(30_000, "2026-11-04", "2026-12-03");
        assert_eq!(
            campaign_budget(&campaign, "USD", amount(6_000), today()),
            (BudgetKind::Total, amount(24_000))
        );
        assert_eq!(
            campaign_budget(&campaign, "USD", amount(40_000), today()),
            (BudgetKind::Total, amount(0))
        );
        let short = a_campaign(30_000, "2026-11-04", "2026-11-05");
        assert_eq!(
            campaign_budget(&short, "USD", amount(6_000), today()),
            (BudgetKind::Daily, amount(12_000))
        );

        // In a currency with no minor unit a budget is whole units, rounded down.
        assert!(ZERO_DECIMAL.contains(&"JPY") && !ZERO_DECIMAL.contains(&"USD"));
        let yen = a_campaign(100_050, "2026-11-04", "2026-12-03");
        assert_eq!(
            campaign_budget(&yen, "JPY", amount(0), today()),
            (BudgetKind::Total, amount(100_000))
        );
        let long_yen = a_campaign(100_000, "2026-11-04", "2027-02-02");
        assert_eq!(
            campaign_budget(&long_yen, "JPY", amount(0), today()),
            (BudgetKind::Daily, amount(1_000)),
            "1000.00 over 91 days is 10.98: ten yen"
        );
        assert_eq!(
            campaign_budget(&long_yen, "USD", amount(0), today()),
            (BudgetKind::Daily, amount(1_098))
        );
    }

    /// The Google Ads account of `an_ads_plan`.
    const ACCOUNT: &str = "123-456-7890";

    /// A plan whose two campaigns run from 2026-11-03 to 2026-12-02, `search-a` on 500.00 and
    /// `search-b` on 400.00 of the plan's 1000.00 for Google Ads.
    fn an_ads_plan() -> PlanProposal {
        let mut plan = a_proposal();
        plan.google_ads = amount(100_000);
        plan.campaigns = vec![
            PlanCampaign {
                key: "search-a".to_string(),
                ..a_campaign(50_000, "2026-11-03", "2026-12-02")
            },
            PlanCampaign {
                key: "search-b".to_string(),
                ..a_campaign(40_000, "2026-11-03", "2026-12-02")
            },
        ];
        plan
    }

    fn made(plan: &str, key: &str, number: u64, kind: BudgetKind) -> CreatedCampaign {
        CreatedCampaign {
            plan: plan.to_string(),
            key: key.to_string(),
            campaign: campaign(number),
            budget: format!("customers/1234567890/campaignBudgets/{number}"),
            kind,
        }
    }

    fn campaign(number: u64) -> String {
        format!("customers/1234567890/campaigns/{number}")
    }

    /// Everything a write is checked against, in one place that each case changes a little.
    struct Ads {
        plan: PlanProposal,
        lineage: Vec<String>,
        created: Vec<CreatedCampaign>,
        spent: std::collections::BTreeMap<String, Amount>,
        today: &'static str,
        /// The day the owner approved the plan.
        approved_on: &'static str,
    }

    impl Ads {
        /// MP-3 is active and replaces MP-2, which replaced MP-1; `search-a` was made under MP-1
        /// (campaign 11, a total budget), `search-b` under MP-3 (campaign 12, a daily one), and
        /// `search-old`, which MP-3 dropped, under MP-2 (campaign 13).
        fn new() -> Self {
            Self {
                plan: an_ads_plan(),
                lineage: vec!["MP-3".to_string(), "MP-2".to_string(), "MP-1".to_string()],
                created: vec![
                    made("MP-1", "search-a", 11, BudgetKind::Total),
                    made("MP-3", "search-b", 12, BudgetKind::Daily),
                    made("MP-2", "search-old", 13, BudgetKind::Total),
                    made("MP-9", "search-a", 14, BudgetKind::Total),
                ],
                spent: std::collections::BTreeMap::new(),
                today: "2026-11-10",
                approved_on: "2026-11-01",
            }
        }

        fn check(&self, write: &AdsWrite) -> Result<(), String> {
            check_ads_write(
                &AdsPlanView {
                    plan_id: "MP-3",
                    plan: &self.plan,
                    lineage: &self.lineage,
                    created: &self.created,
                    spent: &self.spent,
                    today: day(self.today),
                    approved_on: day(self.approved_on),
                },
                write,
            )
        }

        /// The sentence the write is refused with, which must hold `says`.
        fn refuses(&self, write: &AdsWrite, says: &str) {
            let why = self.check(write).expect_err("the write is refused");
            assert!(
                why.contains(says),
                "wanted a refusal saying {says:?}, got {why:?}"
            );
        }

        fn passes(&self, write: &AdsWrite) {
            if let Err(why) = self.check(write) {
                panic!("the write should pass, but {why}");
            }
        }
    }

    fn create(key: &str) -> AdsWrite {
        AdsWrite::Create {
            account: ACCOUNT.to_string(),
            plan_campaign: key.to_string(),
        }
    }

    fn under(number: u64) -> AdsWrite {
        AdsWrite::UnderCampaign {
            account: ACCOUNT.to_string(),
            campaign: campaign(number),
        }
    }

    fn budget(number: u64, hundredths: u64) -> AdsWrite {
        AdsWrite::Budget {
            account: ACCOUNT.to_string(),
            campaign: campaign(number),
            amount: amount(hundredths),
        }
    }

    /// Enabling campaign 11 (`search-a`, a total budget of 500.00 to 2026-12-02), which Google
    /// holds as the plan has it.
    fn enable(number: u64) -> AdsWrite {
        enabling(number, Some(50_000), Some("2026-12-02"))
    }

    /// Enabling a campaign that Google holds a budget of `held_amount` and an end of `held_ends`
    /// for, running from 2026-11-03, the plan's first day.
    fn enabling(number: u64, held_amount: Option<u64>, held_ends: Option<&str>) -> AdsWrite {
        enabling_from(number, held_amount, Some("2026-11-03"), held_ends)
    }

    /// As `enabling`, with the day Google starts the campaign.
    fn enabling_from(
        number: u64,
        held_amount: Option<u64>,
        held_starts: Option<&str>,
        held_ends: Option<&str>,
    ) -> AdsWrite {
        AdsWrite::Enable {
            account: ACCOUNT.to_string(),
            campaign: campaign(number),
            held: HeldAtGoogle {
                amount: held_amount.map(amount),
                starts_on: held_starts.map(day),
                ends_on: held_ends.map(day),
            },
        }
    }

    fn pause(number: u64) -> AdsWrite {
        AdsWrite::Pause {
            account: ACCOUNT.to_string(),
            campaign: campaign(number),
        }
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one case per refusal, side by side, so a missing case is plain to see"
    )]
    fn checks_each_write_against_the_plan() {
        let mut ads = Ads::new();
        // A new campaign for a key no plan of the lineage has made one for.
        ads.created.retain(|made| made.key != "search-b");
        ads.passes(&create("search-b"));

        // Another account: every kind of write.
        let elsewhere = "999-999-9999".to_string();
        for write in [
            AdsWrite::Create {
                account: elsewhere.clone(),
                plan_campaign: "search-b".to_string(),
            },
            AdsWrite::UnderCampaign {
                account: elsewhere.clone(),
                campaign: campaign(11),
            },
            AdsWrite::Budget {
                account: elsewhere.clone(),
                campaign: campaign(11),
                amount: amount(100),
            },
            AdsWrite::Enable {
                account: elsewhere.clone(),
                campaign: campaign(11),
                held: HeldAtGoogle::default(),
            },
            AdsWrite::Pause {
                account: elsewhere.clone(),
                campaign: campaign(11),
            },
        ] {
            ads.refuses(&write, "999-999-9999");
        }
        let mut no_account = Ads::new();
        no_account.plan.google_ads_account = None;
        no_account.refuses(&under(11), "no Google Ads account");

        // A key the plan lacks.
        ads.refuses(&create("search-c"), "no campaign search-c");

        // A second campaign for a key: under the active plan, under a plan two replaces back, and
        // not when the only one was made under a plan outside the lineage.
        let ads = Ads::new();
        ads.refuses(&create("search-b"), "has a campaign already");
        ads.refuses(&create("search-a"), "has a campaign already");
        let mut outside = Ads::new();
        outside
            .created
            .retain(|made| !(made.key == "search-a" && made.plan == "MP-1"));
        outside.passes(&create("search-a"));

        // A create whose last day is before the first day it could start: starting two days
        // ahead, so on the 30th it can start on the 2nd, the last day, and on the 1st it cannot.
        let mut late = Ads::new();
        late.created.clear();
        late.today = "2026-11-30";
        late.approved_on = "2026-11-30";
        late.passes(&create("search-b"));
        late.today = "2026-12-01";
        late.approved_on = "2026-12-01";
        late.refuses(&create("search-b"), "before");
        // A campaign of the plan that starts later than two days ahead still starts when it says.
        late.today = "2026-11-02";
        late.plan.campaigns[1].starts_on = day("2026-12-10");
        late.plan.campaigns[1].ends_on = day("2026-12-10");
        late.passes(&create("search-b"));
        late.plan.campaigns[1].ends_on = day("2026-12-09");
        late.refuses(&create("search-b"), "before");

        // Under a campaign: one the plan's lineage made for a key the plan has, a campaign two
        // replaces back included.
        let ads = Ads::new();
        ads.passes(&under(11));
        ads.passes(&under(12));
        // Another plan's campaign, one never recorded, and one whose key the plan dropped.
        ads.refuses(&under(14), "not made for");
        ads.refuses(&under(99), "not made for");
        ads.refuses(&under(13), "not made for");
        // A campaign in another account than the plan's, whatever was recorded.
        let mut moved = Ads::new();
        moved.created[0].campaign = "customers/5555555555/campaigns/11".to_string();
        moved.refuses(
            &AdsWrite::UnderCampaign {
                account: ACCOUNT.to_string(),
                campaign: "customers/5555555555/campaigns/11".to_string(),
            },
            "another Google Ads account",
        );

        // A total budget: from what the campaign spent to the plan campaign's budget.
        let mut ads = Ads::new();
        ads.spent.insert("search-a".to_string(), amount(10_000));
        ads.passes(&budget(11, 50_000));
        ads.passes(&budget(11, 10_000));
        ads.refuses(&budget(11, 50_001), "more than search-a's 500.00");
        ads.refuses(&budget(11, 9_999), "less than the 100.00");
        // A daily one: at most what is left over the days left. Today is the 10th: 23 days left
        // to 2026-12-02 inclusive, and 400.00 less 92.00 spent is 308.00, 13.39 a day.
        ads.spent.insert("search-b".to_string(), amount(9_200));
        ads.passes(&budget(12, 1_339));
        ads.refuses(&budget(12, 1_340), "more than the 13.39 a day");
        // Before its first day the days left are all of its run, 30 from 2026-11-03: 400.00 is
        // 13.33 a day.
        ads.spent.clear();
        ads.today = "2026-11-02";
        ads.passes(&budget(12, 1_333));
        ads.refuses(&budget(12, 1_334), "more than the 13.33 a day");
        ads.today = "2026-11-10";
        ads.spent.insert("search-b".to_string(), amount(9_200));
        // A campaign the lineage did not make, or another plan's.
        ads.refuses(&budget(14, 100), "not made for");
        ads.refuses(&budget(13, 100), "not made for");
        // A campaign whose dates are over has no daily budget.
        ads.today = "2026-12-03";
        ads.refuses(&budget(12, 1), "ended on 2026-12-02");

        // Enabling: within the dates, below the campaign's budget and below the plan's.
        let mut ads = Ads::new();
        ads.passes(&enable(11));
        ads.today = "2026-11-02";
        ads.refuses(&enable(11), "starts on 2026-11-03");
        ads.today = "2026-11-03";
        ads.passes(&enable(11));
        ads.today = "2026-12-03";
        ads.refuses(&enable(11), "ended on 2026-12-02");
        ads.today = "2026-12-02";
        ads.passes(&enable(11));
        ads.today = "2026-11-10";
        ads.spent.insert("search-a".to_string(), amount(49_999));
        ads.passes(&enable(11));
        ads.spent.insert("search-a".to_string(), amount(50_000));
        ads.refuses(&enable(11), "has spent 500.00 of its 500.00");
        ads.spent.insert("search-a".to_string(), amount(60_000));
        ads.refuses(&enable(11), "has spent 600.00 of its 500.00");
        // The plan's own limit: 1000.00 for Google Ads, spent over its campaigns.
        ads.spent.insert("search-a".to_string(), amount(49_000));
        ads.spent.insert("search-b".to_string(), amount(39_000));
        ads.spent.insert("search-old".to_string(), amount(11_999));
        ads.passes(&enable(11));
        ads.spent.insert("search-old".to_string(), amount(12_000));
        ads.refuses(
            &enable(11),
            "the plan's Google Ads spend is 1000.00 of its 1000.00",
        );
        // A campaign the lineage did not make cannot be enabled, nor one the plan dropped.
        ads.spent.clear();
        ads.refuses(&enable(14), "not made for");
        ads.refuses(&enable(13), "not made for");

        // Pausing covers any campaign of the lineage, the dropped key's included, and none of
        // another plan or none.
        let ads = Ads::new();
        ads.passes(&pause(11));
        ads.passes(&pause(12));
        ads.passes(&pause(13));
        ads.refuses(&pause(14), "not made for");
        ads.refuses(&pause(99), "not made for");
    }

    #[test]
    fn enabling_holds_what_google_keeps_to_the_plan() {
        // Today is 2026-11-10. `search-a` (campaign 11) has a total budget, 500.00 in the plan,
        // to 2026-12-02; `search-b` (campaign 12) a daily one, 400.00 in the plan, to 2026-12-02.
        let mut ads = Ads::new();
        // As the plan has them, or less.
        ads.passes(&enabling(11, Some(50_000), Some("2026-12-02")));
        ads.passes(&enabling(11, Some(1), Some("2026-11-10")));
        // A total budget past the plan campaign's, by a hundredth.
        ads.refuses(
            &enabling(11, Some(50_001), Some("2026-12-02")),
            "total budget at Google is 500.01, more than the 500.00",
        );
        // An end after the plan campaign's, by a day.
        ads.refuses(
            &enabling(11, Some(50_000), Some("2026-12-03")),
            "ends on 2026-12-03 at Google, after the 2026-12-02",
        );
        // A figure Google did not give cannot be checked.
        ads.refuses(
            &enabling(11, None, Some("2026-12-02")),
            "could not read the budget",
        );
        ads.refuses(
            &enabling(11, Some(50_000), None),
            "could not read when search-a ends",
        );

        // A daily budget, over the days Google still runs the campaign: today is the 10th, so
        // 400.00 over the 23 days from today to 2026-12-02, both included, is 17.39 a day (17.39
        // over 23 days is 399.97), and what was spent comes off.
        ads.passes(&enabling(12, Some(1_739), Some("2026-12-02")));
        ads.refuses(
            &enabling(12, Some(1_740), Some("2026-12-02")),
            "daily budget at Google is 17.40, which over the 23 days it runs is more than the \
             400.00",
        );
        ads.spent.insert("search-b".to_string(), amount(9_200));
        ads.passes(&enabling(12, Some(1_339), Some("2026-12-02")));
        ads.refuses(
            &enabling(12, Some(1_340), Some("2026-12-02")),
            "more than the 308.00",
        );
        // Before Google starts it, its days are all of its run: from the 12th, 21 days, so 19.04
        // a day of 400.00 and not 19.05.
        ads.spent.clear();
        ads.passes(&enabling_from(
            12,
            Some(1_904),
            Some("2026-11-12"),
            Some("2026-12-02"),
        ));
        ads.refuses(
            &enabling_from(12, Some(1_905), Some("2026-11-12"), Some("2026-12-02")),
            "over the 21 days it runs",
        );
        // A start Google did not give cannot be counted from.
        ads.refuses(
            &enabling_from(12, Some(1_739), None, Some("2026-12-02")),
            "could not read when search-b starts",
        );
        // Over already, at Google, runs no day: nothing to spend.
        ads.passes(&enabling_from(
            12,
            Some(99_999),
            Some("2026-11-03"),
            Some("2026-11-09"),
        ));

        // A replacing plan that lowers a key's budget leaves Google's total as it was.
        ads.spent.clear();
        ads.plan.campaigns[0].budget = amount(30_000);
        ads.refuses(
            &enabling(11, Some(50_000), Some("2026-12-02")),
            "more than the 300.00",
        );
        ads.passes(&enabling(11, Some(30_000), Some("2026-12-02")));
        // And one that shortens its dates leaves Google's end as it was.
        ads.plan.campaigns[0].ends_on = day("2026-11-20");
        ads.refuses(
            &enabling(11, Some(30_000), Some("2026-12-02")),
            "after the 2026-11-20",
        );
        ads.passes(&enabling(11, Some(30_000), Some("2026-11-20")));
    }
    #[test]
    fn a_campaign_says_what_it_advertises() {
        let field = "campaigns[0].advertises";
        // None, nothing but spaces, two characters and 201 are refused.
        for bad in ["", "   ", "ab", &"a".repeat(201)] {
            assert_eq!(
                after(|p| p.campaigns[0].advertises = bad.to_string()),
                only("marketing_plan_campaign", field),
                "{bad:?}"
            );
        }
        // Three and 200 pass, counted on the trimmed text.
        for good in ["abc", " abc ", &"a".repeat(200)] {
            assert_eq!(
                after(|p| p.campaigns[0].advertises = good.to_string()),
                [],
                "{good:?}"
            );
        }
    }

    /// The price kind of a campaign that starts on `starts_on` and runs `days` days from there,
    /// made on 2026-11-02.
    fn price_of_a_run(days: u64, starts_on: &str) -> PriceKind {
        let last = day(starts_on)
            .checked_add_days(Days::new(days - 1))
            .expect("a date");
        price_kind(
            &a_campaign(30_000, starts_on, &last.to_string()),
            "USD",
            today(),
        )
    }

    #[test]
    fn price_kind_follows_the_total_budget_rule() {
        // From two days ahead (the 4th): 3 and 90 days are a total budget, so fixed; 2 and 91 a
        // daily one, so not.
        assert_eq!(price_of_a_run(3, "2026-11-04"), PriceKind::Fixed);
        assert_eq!(price_of_a_run(90, "2026-11-04"), PriceKind::Fixed);
        assert_eq!(price_of_a_run(2, "2026-11-04"), PriceKind::NotFixed);
        assert_eq!(price_of_a_run(91, "2026-11-04"), PriceKind::NotFixed);
        // One that starts tomorrow is counted from today plus two: three days from the 3rd are
        // two from the 4th, and four are three.
        assert_eq!(price_of_a_run(3, "2026-11-03"), PriceKind::NotFixed);
        assert_eq!(price_of_a_run(4, "2026-11-03"), PriceKind::Fixed);
        // So is one that starts today.
        assert_eq!(price_of_a_run(4, "2026-11-02"), PriceKind::NotFixed);
        assert_eq!(price_of_a_run(5, "2026-11-02"), PriceKind::Fixed);
        // The price kind is the budget kind's, whatever the day it is asked on.
        let campaign = a_campaign(30_000, "2026-11-04", "2026-11-20");
        for asked in ["2026-11-02", "2026-11-18", "2026-11-19"] {
            let expected = match campaign_budget(&campaign, "USD", amount(0), day(asked)).0 {
                BudgetKind::Total => PriceKind::Fixed,
                BudgetKind::Daily => PriceKind::NotFixed,
            };
            assert_eq!(
                price_kind(&campaign, "USD", day(asked)),
                expected,
                "{asked}"
            );
        }
        assert_eq!(PriceKind::Fixed.as_str(), "fixed");
        assert_eq!(PriceKind::NotFixed.as_str(), "not_fixed");
    }

    #[test]
    fn a_fixed_price_is_never_made_daily() {
        // search-b runs 2026-11-03 to 2026-12-02. Approved on the 1st it is 30 days, fixed.
        let mut ads = Ads::new();
        ads.created.retain(|made| made.key != "search-b");
        ads.approved_on = "2026-11-01";
        ads.today = "2026-11-10";
        ads.passes(&create("search-b"));
        // On the 30th it could start on the 2nd alone: a run of one day, a daily budget, which
        // the owner did not see.
        ads.today = "2026-11-30";
        ads.refuses(&create("search-b"), "approved at a fixed price");
        ads.refuses(&create("search-b"), "propose a new version");
        // On the 29th it is two days, still a daily budget.
        ads.today = "2026-11-29";
        ads.refuses(&create("search-b"), "approved at a fixed price");
        // On the 28th it is three, a total budget again.
        ads.today = "2026-11-28";
        ads.passes(&create("search-b"));

        // A campaign the owner saw at a daily budget is created daily, as it was shown.
        let mut daily = Ads::new();
        daily.created.retain(|made| made.key != "search-b");
        daily.plan.campaigns[1].starts_on = day("2026-11-04");
        daily.plan.campaigns[1].ends_on = day("2026-11-05");
        daily.approved_on = "2026-11-02";
        daily.today = "2026-11-03";
        daily.passes(&create("search-b"));
    }
}
