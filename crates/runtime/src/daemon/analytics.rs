//! Analytics behind the hub (`docs/SPEC.md` 4.4, F17; ADR 0059): usage per team split between the
//! owner's own Claude Code sessions and the team's agents, workflow, and blockers, for every team
//! side by side (`analytics.get`, on the hub) and for one (`analytics.mine`, on the team). Nothing
//! is kept but a 60-second cache; everything is read from what the index, the logs and git hold.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use catervas_core::pricing::prices::PRICE_TABLE;
use catervas_core::reminders::Group;
use catervas_store::analytics::{OwnerAnalytics, OwnerGroup, Window};
use catervas_store::git::{Git, integration_branch};
use catervas_store::machine::{MACHINE_DB, open_machine_index};
use catervas_store::waiting::waiting;
use chrono::{
    DateTime, Datelike as _, Duration, FixedOffset, NaiveDate, NaiveTime, Offset as _,
    TimeZone as _, Utc,
};
use serde_json::{Value, json};

use super::DaemonState;
use super::finding::{FindingHost, host_of};
use super::hub::{Hub, hub_of};
use super::reminders::{Digest, digest_of, roots_of, team_fact};
use super::web::{Failure, INTERNAL_ERROR, REFUSED};
use crate::analytics::{CommitSplit, TeamActivity, commits_of, measure_team};
use crate::github;

/// The methods of analytics: the hub answers the first and a team the second
/// (docs/schemas/rpc.schema.json).
pub(super) const METHODS: [&str; 2] = ["analytics.get", "analytics.mine"];

/// How long an answer is kept.
pub const CACHE_SECS: i64 = 60;

/// The most examples a blocker row names.
const EXAMPLES: usize = 3;

/// The period an answer covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Period {
    /// Monday to Sunday, the week of now, in the computer's local time.
    Week,
    /// The calendar month of now.
    Month,
}

impl Period {
    /// The period a wire word names.
    #[must_use]
    pub fn from_wire(word: &str) -> Option<Self> {
        match word {
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            _ => None,
        }
    }

    /// The period's wire word.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

/// The answers kept, by scope (`all` or a team's name in an address) and period, with when each
/// was worked out.
type Kept = HashMap<(String, Period), (DateTime<Utc>, Value)>;

/// What the hub keeps of its answers.
#[derive(Default)]
pub struct Analytics {
    cache: Mutex<Kept>,
    computed: AtomicUsize,
}

impl Analytics {
    /// How many answers were worked out, not served from the cache.
    #[must_use]
    pub fn computed(&self) -> usize {
        self.computed.load(Ordering::SeqCst)
    }

    /// The answer kept for `scope` and `period` if it is under a minute old at `now`, else the one
    /// `work` makes, which is then kept.
    fn kept_or(
        &self,
        scope: &str,
        period: Period,
        now: DateTime<Utc>,
        work: impl FnOnce() -> Result<Value, Failure>,
    ) -> Result<Value, Failure> {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (scope.to_string(), period);
        if let Some((at, value)) = cache.get(&key)
            && now - *at < Duration::seconds(CACHE_SECS)
            && now >= *at
        {
            return Ok(value.clone());
        }
        let value = work()?;
        self.computed.fetch_add(1, Ordering::SeqCst);
        cache.insert(key, (now, value.clone()));
        Ok(value)
    }
}

fn internal(why: &str) -> Failure {
    Failure::new(INTERNAL_ERROR, why)
}

/// The window of `period` at `now` on a clock `offset_secs` from UTC, and its local days.
fn window_of(period: Period, now: DateTime<Utc>, offset_secs: i32) -> (Window, Vec<NaiveDate>) {
    let offset = FixedOffset::east_opt(offset_secs).unwrap_or_else(|| Utc.fix());
    let today = now.with_timezone(&offset).date_naive();
    let first = match period {
        Period::Week => today - Duration::days(i64::from(today.weekday().num_days_from_monday())),
        Period::Month => today.with_day(1).unwrap_or(today),
    };
    let next = match period {
        Period::Week => first + Duration::days(7),
        Period::Month => {
            let (year, month) = if first.month() == 12 {
                (first.year() + 1, 1)
            } else {
                (first.year(), first.month() + 1)
            };
            NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(first + Duration::days(31))
        }
    };
    let instant = |day: NaiveDate| {
        offset
            .from_local_datetime(&day.and_time(NaiveTime::MIN))
            .single()
            .map_or(now, |local| local.with_timezone(&Utc))
    };
    let days = (0..(next - first).num_days())
        .map(|n| first + Duration::days(n))
        .collect();
    (
        Window {
            from: instant(first),
            until: instant(next),
            utc_offset_secs: offset_secs,
        },
        days,
    )
}

/// A team this answer is about.
struct Part {
    slug: String,
    name: String,
    roots: Vec<PathBuf>,
    state: Arc<DaemonState>,
}

fn part_of(slug: &str, state: &Arc<DaemonState>) -> Option<Part> {
    let roots = roots_of(slug, state)?;
    Some(Part {
        slug: roots.slug,
        name: roots.name,
        roots: roots.roots,
        state: Arc::clone(state),
    })
}

/// How long ago, in the words the page uses.
fn age(now: DateTime<Utc>, since: DateTime<Utc>) -> String {
    match (now - since).num_days() {
        ..=0 => "today".to_string(),
        1 => "1 day".to_string(),
        days => format!("{days} days"),
    }
}

/// The job's name in the short words of a schedule item.
fn job_name(key: &str, short: &str) -> String {
    let name = if key.starts_with("schedules:failed:") {
        short.strip_suffix(" failed")
    } else if key.starts_with("schedules:silent:") {
        short.strip_suffix(" has gone quiet")
    } else {
        short.strip_prefix("GitHub turned off ")
    };
    name.unwrap_or(short).to_string()
}

fn blocker(kind: &str, count: usize, examples: &[String]) -> Option<Value> {
    (count > 0).then(|| {
        json!({
            "kind": kind,
            "count": count,
            "examples": examples.iter().take(EXAMPLES).collect::<Vec<_>>(),
        })
    })
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a count of seconds is far below 2^53, where an f64 stops being exact"
)]
fn hours(secs: u64) -> f64 {
    ((secs as f64) / 360.0).round() / 10.0
}

/// The pull requests of the period: GitHub's count when it is connected and knows the repositories
/// and answers, else the teams' own `pull_request.opened` events, else none.
fn pull_requests(
    host: &Arc<dyn FindingHost>,
    window: &Window,
    repos: &[String],
    teams: &[TeamActivity],
) -> Option<u64> {
    if !repos.is_empty()
        && let Some((token, _)) = github::load_connection(host.secrets().as_ref())
            .ok()
            .flatten()
        && let Ok(count) = github::count_pull_requests(
            &host.github().api,
            &token,
            repos,
            window.from,
            window.until,
        )
    {
        return Some(count);
    }
    let own: u64 = teams.iter().map(|team| team.pull_requests).sum();
    (own > 0).then_some(own)
}

/// One team's numbers and what its log and repositories say of it.
struct Measured {
    activity: TeamActivity,
    commits: CommitSplit,
    waiting: usize,
    waiting_since: Option<DateTime<Utc>>,
}

fn measure(part: &Part, window: &Window, now: DateTime<Utc>) -> Result<Measured, Failure> {
    let deps = part
        .state
        .deps()
        .ok_or_else(|| internal("this team has no log"))?;
    deps.projections
        .catch_up()
        .map_err(|error| internal(&error.to_string()))?;
    let activity = measure_team(&deps.projections, window, now)
        .map_err(|error| internal(&error.to_string()))?;
    let team = deps
        .files
        .read_team()
        .map_err(|error| internal(&error.to_string()))?;
    let mut commits = CommitSplit::default();
    for root in &part.roots {
        let git = Git::open(root.clone());
        let Ok(branch) = integration_branch(&team, &git) else {
            continue;
        };
        let split = commits_of(root, &branch, window);
        commits.owner += split.owner;
        commits.team += split.team;
    }
    let listed = waiting(&deps.projections, &deps.log, &deps.files, &team).unwrap_or_default();
    Ok(Measured {
        activity,
        commits,
        waiting: listed.len(),
        waiting_since: team_fact(&part.slug, &part.state)
            .and_then(|fact| fact.oldest_waiting)
            .map(|waiting| waiting.since),
    })
}

/// The answer for `parts` over `period`: the whole computer's when `whole`, else the one team's.
#[allow(
    clippy::too_many_lines,
    reason = "one pass that reads each source once and lays the answer out"
)]
fn compute(
    host: &Arc<dyn FindingHost>,
    parts: &[Part],
    digest: &Digest,
    period: Period,
    whole: bool,
) -> Result<Value, Failure> {
    let now = host.now();
    let (window, days) = window_of(period, now, host.utc_offset_secs(now));
    let groups: Vec<OwnerGroup> = parts
        .iter()
        .map(|part| OwnerGroup {
            key: part.slug.clone(),
            roots: part
                .roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect(),
        })
        .collect();
    let path = host.state_dir().join(MACHINE_DB);
    let (owner, conversations, repos) = if path.exists() {
        let index = open_machine_index(&path, now).map_err(|error| internal(&error.to_string()))?;
        let owner = index
            .owner_analytics(&groups, &window, &PRICE_TABLE)
            .map_err(|error| internal(&error.to_string()))?;
        let conversations = index
            .claude_ai_conversations(&window)
            .map_err(|error| internal(&error.to_string()))?;
        let mut repos: Vec<String> = Vec::new();
        for part in parts {
            for root in &part.roots {
                if let Ok(Some(full_name)) = index.github_repo_of(&root.to_string_lossy())
                    && !repos.contains(&full_name)
                {
                    repos.push(full_name);
                }
            }
        }
        (owner, conversations, repos)
    } else {
        let empty = OwnerAnalytics {
            groups: groups
                .iter()
                .map(|group| {
                    (
                        group.key.clone(),
                        catervas_store::analytics::OwnerActivity::default(),
                    )
                })
                .collect(),
            ..OwnerAnalytics::default()
        };
        (empty, 0, Vec::new())
    };
    let measured: Vec<Measured> = parts
        .iter()
        .map(|part| measure(part, &window, now))
        .collect::<Result<_, _>>()?;
    let activities: Vec<TeamActivity> = measured.iter().map(|m| m.activity.clone()).collect();

    // What the owner did: the whole computer's, or the team's.
    let own_work = if whole {
        &owner.total
    } else {
        owner.groups.first().map_or(&owner.total, |(_, own)| own)
    };
    let sessions = own_work.sessions + activities.iter().map(|team| team.sessions).sum::<u64>();
    let active_secs =
        own_work.active_secs + activities.iter().map(|team| team.active_secs).sum::<u64>();
    let commits = measured
        .iter()
        .fold(CommitSplit::default(), |sum, m| CommitSplit {
            owner: sum.owner + m.commits.owner,
            team: sum.team + m.commits.team,
        });
    let per_day: Vec<Value> = days
        .iter()
        .map(|day| {
            let own = own_work.sessions_per_day.get(day).copied().unwrap_or(0);
            let teams: u64 = activities
                .iter()
                .map(|team| team.sessions_per_day.get(day).copied().unwrap_or(0))
                .sum();
            json!({ "day": day.to_string(), "sessions": own + teams })
        })
        .collect();
    let mut workflow = json!({
        "sessions": sessions,
        "active_hours": hours(active_secs),
        "commits": { "owner": commits.owner, "team": commits.team },
        "used_most": own_work.tools.iter()
            .map(|tool| json!({ "label": tool.label, "calls": tool.calls }))
            .collect::<Vec<_>>(),
        "per_day": per_day,
    });
    if let Some(count) = pull_requests(host, &window, &repos, &activities) {
        workflow["pull_requests"] = json!(count);
    }

    // The blockers: the digest's items, and the teams' own logs.
    let mine_slug = (!whole)
        .then(|| parts.first().map(|part| part.slug.as_str()))
        .flatten();
    let items: Vec<_> = digest
        .entries
        .iter()
        .filter(|entry| whole || entry.team.as_deref() == mine_slug)
        .map(|entry| &entry.reminder)
        .collect();
    let in_group = |group: Group| items.iter().filter(move |item| item.group == group);
    let schedules: Vec<String> = in_group(Group::Schedules)
        .filter(|item| {
            ["schedules:failed:", "schedules:silent:", "schedules:off:"]
                .iter()
                .any(|prefix| item.key.starts_with(prefix))
        })
        .map(|item| job_name(&item.key, &item.short))
        .collect();
    let deploys: Vec<String> = in_group(Group::Schedules)
        .filter(|item| item.key.starts_with("schedules:deploy:"))
        .map(|item| item.project.clone().unwrap_or_else(|| item.title.clone()))
        .collect();
    let forgotten: Vec<String> = in_group(Group::Forgotten)
        .map(|item| item.title.clone())
        .collect();
    let (mut waiting_examples, mut waiting_count) = (Vec::new(), 0);
    let (mut blocked_examples, mut blocked_count) = (Vec::new(), 0);
    let (mut denied_examples, mut denied_count) = (Vec::new(), 0);
    for (part, found) in parts.iter().zip(&measured) {
        if found.waiting > 0 {
            waiting_count += found.waiting;
            waiting_examples.push(match found.waiting_since {
                Some(since) => format!("{}, {}", part.name, age(now, since)),
                None => part.name.clone(),
            });
        }
        for task in &found.activity.blocked {
            blocked_count += 1;
            blocked_examples.push(format!("{}: “{}”", part.name, task.title));
        }
        if found.activity.denials > 0 {
            denied_count += found.activity.denials;
            denied_examples.push(format!("{}: {}", part.name, found.activity.denials));
        }
    }
    let blockers: Vec<Value> = [
        blocker("waiting_on_you", waiting_count, &waiting_examples),
        blocker("schedules", schedules.len(), &schedules),
        blocker("deploys", deploys.len(), &deploys),
        blocker("blocked", blocked_count, &blocked_examples),
        blocker("forgotten", forgotten.len(), &forgotten),
        blocker(
            "denials",
            usize::try_from(denied_count).unwrap_or(usize::MAX),
            &denied_examples,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    let usage = |part: &Part, own: &catervas_store::analytics::OwnerActivity, found: &Measured| {
        json!({
            "slug": part.slug,
            "name": part.name,
            "owner_usd": own.usd,
            "agents_usd": found.activity.agents_usd,
            "unpriced_replies": own.unpriced_replies,
        })
    };
    if whole {
        let teams: Vec<Value> = parts
            .iter()
            .zip(&owner.groups)
            .zip(&measured)
            .map(|((part, (_, own)), found)| usage(part, own, found))
            .collect();
        return Ok(json!({
            "period": period.as_wire(),
            "teams": teams,
            "watched_usd": owner.watched.usd,
            "claude_ai_conversations": conversations,
            "workflow": workflow,
            "blockers": blockers,
        }));
    }
    let (Some(part), Some((_, own)), Some(found)) =
        (parts.first(), owner.groups.first(), measured.first())
    else {
        return Err(internal("this team has nothing to measure"));
    };
    Ok(json!({
        "period": period.as_wire(),
        "team": usage(part, own, found),
        "workflow": workflow,
        "blockers": blockers,
        "cycle_days": found.activity.cycle_days,
        "throughput": found.activity.throughput,
    }))
}

/// `analytics.get` for `period`, with the billing word the account kind gives.
pub(super) fn get(hub: &Hub, period: Period, billing: &str) -> Result<Value, Failure> {
    let host = host_of(hub)?;
    let mut answer = hub.analytics.kept_or("all", period, host.now(), || {
        let parts: Vec<Part> = hub
            .driven()
            .iter()
            .filter_map(|(slug, state)| part_of(slug.as_str(), state))
            .collect();
        let digest = digest_of(hub)?;
        compute(host, &parts, &digest, period, true)
    })?;
    answer["billing"] = json!(billing);
    Ok(answer)
}

/// `analytics.mine` for the team `state` drives.
pub(super) fn mine(
    state: &Arc<DaemonState>,
    period: Period,
    billing: &str,
) -> Result<Value, Failure> {
    let (Some((slug, analytics, finding)), Some(reminders)) =
        (state.analytics_handle(), state.reminders())
    else {
        return Err(Failure::new(
            REFUSED,
            "catervas answers this on a team's page, once the hub has added the team",
        ));
    };
    let host = finding
        .host()
        .ok_or_else(|| internal("this catervas has nothing to look through your history with"))?;
    let mut answer = analytics.kept_or(slug, period, host.now(), || {
        let part = part_of(slug, state).ok_or_else(|| internal("this team has no log"))?;
        let digest = reminders.current().map_err(|why| internal(&why))?;
        compute(host, &[part], &digest, period, false)
    })?;
    answer["billing"] = json!(billing);
    Ok(answer)
}

/// `call` of analytics, whose params the schema already passed. The account says whether the
/// amounts are what the use would cost on the API (a subscription) or what was spent.
pub(super) async fn call(
    state: &Arc<DaemonState>,
    method: &str,
    params: &Value,
) -> Result<Value, Failure> {
    use catervas_core::claude_account::AccountKind;

    let period = params["period"]
        .as_str()
        .and_then(Period::from_wire)
        .ok_or_else(|| internal("the period is neither week nor month"))?;
    let kind = match state.web() {
        Some(web) => web.account.status().await.kind,
        None => None,
    };
    let billing = if kind == Some(AccountKind::Subscription) {
        "api"
    } else {
        "spent"
    };
    let state = Arc::clone(state);
    let method = method.to_string();
    tokio::task::spawn_blocking(move || {
        if method == "analytics.get" {
            get(hub_of(&state)?, period, billing)
        } else {
            mine(&state, period, billing)
        }
    })
    .await
    .unwrap_or_else(|error| Err(internal(&error.to_string())))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use catervas_core::reminders::{DeployFact, JobFact, ReminderInput, SessionFact};
    use catervas_protocol::event::EventKind;
    use catervas_store::analytics::fixtures::{seed_conversation, seed_reply, seed_session};
    use catervas_store::machine::{MACHINE_DB, open_machine_index};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use serde_json::{Value, json};

    use super::{Period, get, mine};
    use crate::analytics::fixtures::append;
    use crate::daemon::finding::FindingHost;
    use crate::daemon::hub::{Hub, TeamSlug};
    use crate::daemon::reminders::{build, tests::Fake};
    use crate::orchestrator::fixtures::Harness;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .single()
            .expect("a valid time")
    }

    /// Fails unless `value` is what `definition` of the RPC schema says.
    fn conforms(value: &Value, definition: &str) {
        let schema: Value =
            serde_json::from_str(catervas_protocol::rpc::SCHEMA_JSON).expect("the schema is JSON");
        let root = json!({
            "$schema": schema["$schema"],
            "$ref": format!("#/$defs/{definition}"),
            "$defs": schema["$defs"],
        });
        let validator = jsonschema::options()
            .build(&root)
            .expect("the definition compiles");
        let errors: Vec<String> = validator
            .iter_errors(value)
            .map(|error| error.to_string())
            .collect();
        assert!(errors.is_empty(), "{definition}: {errors:?} in {value}");
    }

    fn canonical(path: &Path) -> String {
        std::fs::canonicalize(path)
            .expect("a path")
            .to_string_lossy()
            .into_owned()
    }

    /// The owner's index: Tidepool's folder holds two priced replies and one on a model with no
    /// price, Price Watch's one, and a folder no team holds one; an agent's session in Tidepool's
    /// folder and a conversation in and out of the week.
    fn seed_index(fake: &Fake, tidepool: &str, price_watch: &str) {
        let index =
            open_machine_index(&fake.state_dir().join(MACHINE_DB), fake.now()).expect("an index");
        for (id, place, root) in [
            ("sa", "owner", tidepool),
            ("sb", "owner", price_watch),
            ("sw", "owner", "/somewhere/watched"),
            ("agent", "catervas", tidepool),
        ] {
            seed_session(&index, id, place, root);
        }
        let model = "claude-opus-5-5";
        // A million input tokens cost four dollars at this model's price.
        seed_reply(&index, "a1", "sa", model, at(20, 10, 0), 1_000_000);
        seed_reply(&index, "a2", "sa", model, at(20, 10, 3), 1_000_000);
        seed_reply(
            &index,
            "a3",
            "sa",
            "claude-unheard-of",
            at(20, 11, 0),
            1_000_000,
        );
        seed_reply(&index, "b1", "sb", model, at(21, 8, 0), 500_000);
        seed_reply(&index, "w1", "sw", model, at(21, 8, 0), 1_000_000);
        seed_reply(&index, "x1", "agent", model, at(20, 10, 1), 1_000_000);
        seed_conversation(&index, "c1", at(20, 9, 0));
        seed_conversation(&index, "c2", at(5, 9, 0));
    }

    fn moved(team: &Harness, id: &str, from: &str, to: &str, when: DateTime<Utc>) {
        append(
            &team.project.deps.projections,
            EventKind::TaskTransitioned,
            when,
            &json!({ "task_id": id, "body": { "from": from, "to": to } }),
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "two teams, built once, and every part of the answer"
    )]
    async fn answers_analytics_for_every_team_and_each() {
        let fake = Fake::new("analytics");
        let (tidepool, price_watch) = (
            Harness::new("analytics-tidepool", |_| {}),
            Harness::new("analytics-price-watch", |_| {}),
        );
        let hub = Arc::new(Hub::default());
        hub.set_finder(fake.clone());
        hub.add(TeamSlug::new("tidepool"), Arc::clone(&tidepool.daemon));
        hub.add(
            TeamSlug::new("price-watch"),
            Arc::clone(&price_watch.daemon),
        );
        let (root_a, root_b) = (
            canonical(&tidepool.project.repo.path),
            canonical(&price_watch.project.repo.path),
        );
        seed_index(&fake, &root_a, &root_b);

        // Tidepool's log: what its agents spent, a session, two denials, two tasks accepted (one
        // day and 2.2 days from in progress) and one blocked for five days.
        let log = &tidepool.project.deps.projections;
        for id in ["CTV-1", "CTV-2", "CTV-3"] {
            append(
                log,
                EventKind::TaskCreated,
                at(10, 9, 0),
                &json!({ "task_id": id, "body": { "summary": { "kind": "task",
                    "title": format!("Task {id}"), "status": "draft", "risk": "low" } } }),
            );
        }
        moved(&tidepool, "CTV-1", "assigned", "in_progress", at(19, 9, 0));
        moved(&tidepool, "CTV-1", "verifying", "accepted", at(20, 9, 0));
        moved(&tidepool, "CTV-2", "assigned", "in_progress", at(18, 20, 0));
        moved(&tidepool, "CTV-2", "verifying", "accepted", at(21, 0, 48));
        moved(&tidepool, "CTV-3", "in_progress", "blocked", at(16, 9, 0));
        append(
            log,
            EventKind::CostRecorded,
            at(20, 10, 30),
            &json!({ "session_id": "s1", "agent_id": "dev-a", "body": { "cost_usd": 1.5 } }),
        );
        for when in [at(20, 11, 0), at(20, 12, 0)] {
            append(
                log,
                EventKind::ToolDenied,
                when,
                &json!({ "session_id": "s1", "agent_id": "dev-a" }),
            );
        }
        append(
            &price_watch.project.deps.projections,
            EventKind::CostRecorded,
            at(20, 10, 30),
            &json!({ "session_id": "s2", "agent_id": "dev-a", "body": { "cost_usd": 0.5 } }),
        );

        // The digest the hub keeps: a forgotten session in Tidepool, a failing job and a failed
        // deploy in Price Watch.
        let now = fake.now();
        let digest = build(
            &hub,
            &ReminderInput {
                sessions: vec![SessionFact {
                    id: "s-1".to_string(),
                    title: "Fix the login redirect".to_string(),
                    project: "tidepool".to_string(),
                    project_root: root_a.clone(),
                    folder: root_a.clone(),
                    branch: None,
                    last_at: now - Duration::days(2),
                    open_todos: 2,
                    uncommitted: false,
                    unpushed: 0,
                    draft_pull_quiet_days: None,
                    waiting_job: false,
                }],
                jobs: vec![JobFact {
                    full_name: "octo/price-watch".to_string(),
                    workflow: "price.yml".to_string(),
                    name: "Hourly price check".to_string(),
                    project: "price-watch".to_string(),
                    project_root: Some(root_b.clone()),
                    interval_secs: Some(3600),
                    state: Some("active".to_string()),
                    last_conclusion: Some("failure".to_string()),
                    last_run_at: Some(now - Duration::hours(3)),
                }],
                deploys: vec![DeployFact {
                    full_name: "octo/price-watch".to_string(),
                    project: "price-watch".to_string(),
                    project_root: Some(root_b.clone()),
                    environment: "Production".to_string(),
                    state: "failure".to_string(),
                    created_at: now - Duration::hours(20),
                }],
                github_connected: true,
                ..ReminderInput::default()
            },
            now,
        );
        hub.reminders.replace(digest);

        let hub_for_get = Arc::clone(&hub);
        let answer = tokio::task::spawn_blocking(move || get(&hub_for_get, Period::Week, "api"))
            .await
            .expect("it ran")
            .expect("analytics");
        conforms(&answer, "analyticsGetResult");
        assert_eq!(
            (answer["period"].as_str(), answer["billing"].as_str()),
            (Some("week"), Some("api"))
        );
        let teams = answer["teams"].as_array().expect("teams");
        assert_eq!(teams.len(), 2, "{answer}");
        let of = |slug: &str| teams.iter().find(|team| team["slug"] == slug).expect(slug);
        // Four dollars a priced reply, nothing for the unpriced one, and the agent's session left out.
        assert_eq!(of("tidepool")["owner_usd"], 8.0, "{answer}");
        assert_eq!(of("tidepool")["agents_usd"], 1.5);
        assert_eq!(of("tidepool")["unpriced_replies"], 1);
        assert_eq!(of("price-watch")["owner_usd"], 2.0);
        assert_eq!(of("price-watch")["agents_usd"], 0.5);
        assert_eq!(answer["watched_usd"], 4.0);
        assert_eq!(answer["claude_ai_conversations"], 1);
        // Owner sessions with a reply in the week: sa, sb and sw.
        assert_eq!(answer["workflow"]["sessions"], 3, "{answer}");
        assert_eq!(answer["workflow"]["used_most"], json!([]));
        let blocker = |kind: &str| -> Value {
            answer["blockers"]
                .as_array()
                .expect("blockers")
                .iter()
                .find(|row| row["kind"] == kind)
                .cloned()
                .unwrap_or(Value::Null)
        };
        assert_eq!(blocker("blocked")["count"], 1, "{answer}");
        assert_eq!(blocker("blocked")["examples"][0], "Catervas: “Task CTV-3”");
        assert_eq!(blocker("denials")["count"], 2);
        assert_eq!(blocker("forgotten")["count"], 1);
        assert_eq!(
            blocker("schedules")["examples"],
            json!(["Hourly price check"])
        );
        assert_eq!(blocker("deploys")["examples"], json!(["price-watch"]));

        // A second call within a minute is the first one's answer: the store is read once.
        assert_eq!(hub.analytics.computed(), 1);
        let hub_again = Arc::clone(&hub);
        let again = tokio::task::spawn_blocking(move || get(&hub_again, Period::Week, "spent"))
            .await
            .expect("it ran")
            .expect("analytics");
        assert_eq!(hub.analytics.computed(), 1, "served from the cache");
        assert_eq!(
            again["billing"], "spent",
            "the billing word is the account's, not the cache's"
        );
        assert_eq!(again["teams"], answer["teams"]);

        // A team answers its own part, with its cycle time and what it finished.
        let state = Arc::clone(&tidepool.daemon);
        let own = tokio::task::spawn_blocking(move || mine(&state, Period::Week, "api"))
            .await
            .expect("it ran")
            .expect("its part");
        conforms(&own, "analyticsMineResult");
        assert_eq!(own["team"]["slug"], "tidepool");
        assert_eq!(own["team"]["owner_usd"], 8.0);
        assert_eq!(own["throughput"], 2, "{own}");
        let cycle = own["cycle_days"].as_f64().expect("a cycle time");
        assert!((cycle - 1.6).abs() < 1e-6, "{cycle}");
        assert!(
            own["blockers"]
                .as_array()
                .expect("blockers")
                .iter()
                .all(|row| row["kind"] != "deploys"),
            "Price Watch's deploy is not Tidepool's: {own}"
        );

        for frame in [
            json!({"jsonrpc": "2.0", "id": 1, "method": "analytics.get", "params": {"period": "month"}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "analytics.mine", "params": {"period": "week"}}),
        ] {
            catervas_protocol::rpc::rpc_request_from_value(&frame)
                .unwrap_or_else(|errors| panic!("{frame} refused: {errors:?}"));
        }
    }
}
