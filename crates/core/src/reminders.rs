//! What needs the owner each day (`docs/SPEC.md` 4.4, F12; ADR 0059): the rules that turn the facts
//! the store and the teams hold into the items of the daily digest. Pure: it reads nothing and runs
//! nothing.

use chrono::{DateTime, Duration, FixedOffset, Offset as _, Utc};

/// The least whole days since a session's last activity for it to count as forgotten.
pub const FORGOTTEN_FROM_DAYS: i64 = 1;
/// The most whole days since a session's last activity for it to count as forgotten.
pub const FORGOTTEN_UNTIL_DAYS: i64 = 14;
/// How many days a draft pull request is quiet before it is a reason.
pub const DRAFT_QUIET_DAYS: i64 = 3;
/// How many days after it ended a recurring session cron is still told of.
pub const ENDED_FOR_DAYS: i64 = 7;

/// Where an item is listed, in the order the groups are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Sessions the owner may have forgotten.
    Forgotten,
    /// Session crons that end soon.
    Ending,
    /// Scheduled runs and deployments.
    Schedules,
    /// What waits on the owner in a team.
    Teams,
    /// Connectors that need signing in again.
    Services,
    /// Today's runs, collapsed.
    Runs,
}

impl Group {
    /// The group's value on the wire.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Forgotten => "forgotten",
            Self::Ending => "ending",
            Self::Schedules => "schedules",
            Self::Teams => "teams",
            Self::Services => "services",
            Self::Runs => "runs",
        }
    }
}

/// A session of the owner's, with what was left undone in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFact {
    /// The session's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// The project's name.
    pub project: String,
    /// The project's folder.
    pub project_root: String,
    /// The folder the session ran in.
    pub folder: String,
    /// The branch it was on.
    pub branch: Option<String>,
    /// When it last did anything.
    pub last_at: DateTime<Utc>,
    /// The items of its last to-do list that are not completed.
    pub open_todos: u32,
    /// Whether its folder has changes that are not committed.
    pub uncommitted: bool,
    /// How many commits its branch has that are not pushed.
    pub unpushed: u32,
    /// How many days a draft pull request for its work has been quiet, when there is one.
    pub draft_pull_quiet_days: Option<i64>,
    /// Whether a background job of the session waits for input.
    pub waiting_job: bool,
}

/// A cron a session set, which Claude Code ends after a week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronFact {
    /// The cron's id.
    pub id: String,
    /// What it is called: its schedule, for its prompt is never read.
    pub label: String,
    /// The session that set it.
    pub session_id: String,
    /// The project's name.
    pub project: String,
    /// The project's folder.
    pub project_root: Option<String>,
    /// The folder the session ran in.
    pub folder: Option<String>,
    /// When Claude Code ends it.
    pub expires_at: DateTime<Utc>,
    /// Whether it repeats.
    pub recurring: bool,
}

/// A scheduled workflow of a repository on GitHub, with what GitHub said of its runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFact {
    /// `owner/name`.
    pub full_name: String,
    /// The workflow's file.
    pub workflow: String,
    /// What the job is called.
    pub name: String,
    /// The project's name.
    pub project: String,
    /// The project's folder.
    pub project_root: Option<String>,
    /// How long between two runs, when the schedule says.
    pub interval_secs: Option<i64>,
    /// The workflow's state, such as `active` or `disabled_inactivity`.
    pub state: Option<String>,
    /// The last scheduled run's conclusion.
    pub last_conclusion: Option<String>,
    /// When the last scheduled run started.
    pub last_run_at: Option<DateTime<Utc>>,
}

/// The latest deployment of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployFact {
    /// `owner/name`.
    pub full_name: String,
    /// The project's name.
    pub project: String,
    /// The project's folder.
    pub project_root: Option<String>,
    /// The environment deployed to.
    pub environment: String,
    /// The latest state of the deployment, such as `success` or `failure`.
    pub state: String,
    /// When it was made.
    pub created_at: DateTime<Utc>,
}

/// What a team has waited on the owner for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamFact {
    /// The team's name in an address.
    pub slug: String,
    /// The team's name.
    pub name: String,
    /// The oldest thing that waits: who asks, what about, since when.
    pub oldest_waiting: Option<WaitingFact>,
    /// The escalations the aged rule found.
    pub aged: Vec<AgedFact>,
}

/// One thing that waits on the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingFact {
    /// The agent that waits.
    pub agent: String,
    /// What it is about.
    pub title: String,
    /// Since when.
    pub since: DateTime<Utc>,
}

/// An escalation that `escalation.aged` recorded and nobody answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgedFact {
    /// The task's id.
    pub task_id: String,
    /// The task's title.
    pub title: String,
    /// When the aged rule found it.
    pub since: DateTime<Utc>,
}

/// A run that happened today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFact {
    /// A key that is the same for the same run.
    pub key: String,
    /// What ran.
    pub name: String,
    /// The project's name.
    pub project: String,
    /// When it started.
    pub at: DateTime<Utc>,
}

/// How the owner answered an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkKind {
    /// Hidden until a time.
    Snooze {
        /// When it shows again.
        until: DateTime<Utc>,
    },
    /// Hidden while its fingerprint holds.
    Dismiss {
        /// The fingerprint it had.
        fingerprint: String,
    },
}

/// The owner's answer to the item `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// The item's key.
    pub key: String,
    /// What the owner did.
    pub kind: MarkKind,
}

/// Everything the rules read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReminderInput {
    /// The owner's sessions.
    pub sessions: Vec<SessionFact>,
    /// The crons sessions set.
    pub crons: Vec<CronFact>,
    /// The scheduled workflows of repositories on GitHub.
    pub jobs: Vec<JobFact>,
    /// The latest deployments.
    pub deploys: Vec<DeployFact>,
    /// What each team waits on the owner for.
    pub teams: Vec<TeamFact>,
    /// The connectors Claude Code says need signing in again, by the name Claude Code gives them.
    pub services: Vec<String>,
    /// Today's runs.
    pub runs_today: Vec<RunFact>,
    /// What the owner snoozed and dismissed.
    pub marks: Vec<Mark>,
    /// Whether GitHub is connected, so that its answers about runs are known.
    pub github_connected: bool,
    /// The computer's offset from UTC, in seconds, for the times the items say.
    pub utc_offset_secs: i32,
}

/// Which page of a repository on GitHub an item opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubPage {
    /// Its Actions page.
    Actions,
    /// Its deployments.
    Deployments,
}

impl GithubPage {
    /// The page's path under the repository.
    #[must_use]
    pub fn path(self) -> &'static str {
        match self {
            Self::Actions => "actions",
            Self::Deployments => "deployments",
        }
    }
}

/// What an item is about, for the actions that need more than its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A forgotten session.
    Session {
        /// Its id.
        session_id: String,
        /// The folder it ran in.
        folder: String,
        /// Its branch.
        branch: Option<String>,
        /// Whole days since it last did anything.
        days: i64,
        /// What was left, in words.
        reason: String,
    },
    /// A session cron.
    Cron {
        /// The session that set it.
        session_id: String,
        /// The folder it ran in.
        folder: Option<String>,
    },
    /// A repository on GitHub.
    Github {
        /// `owner/name`.
        full_name: String,
        /// The page to open.
        page: GithubPage,
    },
    /// The owner has not connected GitHub.
    ConnectGithub,
    /// A team that waits.
    Team {
        /// The team's name in an address.
        slug: String,
    },
    /// A connector to sign in to again.
    Service,
    /// A run that happened.
    Run,
}

/// One item of the digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    /// `<rule>:<thing key>`, the same for the same thing from one day to the next.
    pub key: String,
    /// Where it is listed.
    pub group: Group,
    /// What it says.
    pub title: String,
    /// The line under it.
    pub detail: String,
    /// A few words for the notice.
    pub short: String,
    /// A hash of the fields that make the item what it is; a dismissal holds while it is the same.
    pub fingerprint: String,
    /// How soon it matters: 0 first.
    pub urgency: u8,
    /// The project's name, when it has one.
    pub project: Option<String>,
    /// The project's folder, when it has one.
    pub project_root: Option<String>,
    /// What it is about.
    pub target: Target,
}

/// A hash of `parts`, the same in every build: FNV-1a over the parts, each ended by a byte that no
/// text holds.
#[must_use]
pub fn fingerprint(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0xff)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// How many items a cron field lists: `1,3,5` is three, anything else one.
fn listed(field: &str) -> i64 {
    i64::try_from(field.split(',').count()).unwrap_or(1).max(1)
}

/// The shortest time between two runs that `cron` says, in seconds; none when it cannot be read.
/// A rough reading: it looks at the minute and hour fields for a step or a star, and otherwise at
/// the day fields.
#[must_use]
pub fn cron_interval_secs(cron: &str) -> Option<i64> {
    let fields: Vec<&str> = cron.split_whitespace().collect();
    let [minute, hour, day, _month, weekday] = fields[..] else {
        return None;
    };
    if !fields.iter().all(|field| {
        field
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | '/' | ',' | '-' | '?'))
    }) {
        return None;
    }
    let step = |field: &str| -> Option<i64> {
        field
            .strip_prefix("*/")
            .and_then(|n| n.parse::<i64>().ok())
            .filter(|n| *n > 0)
    };
    if minute == "*" {
        return Some(60);
    }
    if let Some(n) = step(minute) {
        return Some(n * 60);
    }
    if hour == "*" {
        return Some(3600 / listed(minute));
    }
    if let Some(n) = step(hour) {
        return Some(n * 3600);
    }
    if day != "*" && day != "?" {
        return Some(30 * 86_400 / listed(day));
    }
    if weekday != "*" && weekday != "?" {
        return Some(7 * 86_400 / listed(weekday));
    }
    Some(86_400)
}

/// What the notice says under its count: the first two items by urgency.
#[must_use]
pub fn headline(items: &[Reminder]) -> String {
    let mut shown: Vec<&Reminder> = items
        .iter()
        .filter(|item| item.group != Group::Runs)
        .collect();
    shown.sort_by_key(|item| (item.urgency, item.group));
    let words: Vec<&str> = shown
        .iter()
        .take(2)
        .map(|item| item.short.as_str())
        .collect();
    match words[..] {
        [only] => format!("{only}."),
        [first, second] => format!("{first}, and {second}."),
        _ => String::new(),
    }
}

fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// "4 days ago".
fn days_ago(days: i64) -> String {
    format!("{} ago", plural(days, "day", "days"))
}

/// `at` as the owner reads it: "07:00 today", "yesterday 18:40", or "Oct 12 07:00".
fn when(at: DateTime<Utc>, now: DateTime<Utc>, offset: FixedOffset) -> String {
    let (at, now) = (at.with_timezone(&offset), now.with_timezone(&offset));
    let clock = at.format("%H:%M");
    if at.date_naive() == now.date_naive() {
        format!("{clock} today")
    } else if at.date_naive().succ_opt() == Some(now.date_naive()) {
        format!("yesterday {clock}")
    } else {
        format!("{} {clock}", at.format("%b %-d"))
    }
}

/// Why a session counts as forgotten, in words; empty when it does not.
fn reasons(session: &SessionFact) -> Vec<String> {
    let mut reasons = Vec::new();
    if session.open_todos > 0 {
        reasons.push(format!(
            "{} still open",
            plural(i64::from(session.open_todos), "to-do", "to-dos")
        ));
    }
    if session.uncommitted {
        reasons.push("changes not committed".to_string());
    }
    if session.unpushed > 0 {
        reasons.push("changes not pushed".to_string());
    }
    if session
        .draft_pull_quiet_days
        .is_some_and(|days| days >= DRAFT_QUIET_DAYS)
    {
        reasons.push("its pull request is still a draft".to_string());
    }
    if session.waiting_job {
        reasons.push("a background job is waiting for you".to_string());
    }
    reasons
}

fn forgotten(input: &ReminderInput, now: DateTime<Utc>, out: &mut Vec<Reminder>) {
    for session in &input.sessions {
        let days = (now - session.last_at).num_days();
        if !(FORGOTTEN_FROM_DAYS..=FORGOTTEN_UNTIL_DAYS).contains(&days) {
            continue;
        }
        let reasons = reasons(session);
        if reasons.is_empty() {
            continue;
        }
        let reason = reasons.join(", ");
        let last_at = session.last_at.to_rfc3339();
        out.push(Reminder {
            key: format!("forgotten:{}", session.id),
            group: Group::Forgotten,
            title: format!("{} · {}", session.title, session.project),
            detail: format!("{} · {reason}", days_ago(days)),
            short: format!("A session in {} was left unfinished", session.project),
            fingerprint: fingerprint(&[
                &last_at,
                &session.open_todos.to_string(),
                &session.uncommitted.to_string(),
                &session.unpushed.to_string(),
                &session.draft_pull_quiet_days.is_some().to_string(),
                &session.waiting_job.to_string(),
            ]),
            urgency: 4,
            project: Some(session.project.clone()),
            project_root: Some(session.project_root.clone()),
            target: Target::Session {
                session_id: session.id.clone(),
                folder: session.folder.clone(),
                branch: session.branch.clone(),
                days,
                reason,
            },
        });
    }
}

fn ending(input: &ReminderInput, now: DateTime<Utc>, out: &mut Vec<Reminder>) {
    for cron in &input.crons {
        let soon = cron.expires_at > now && cron.expires_at <= now + Duration::hours(24);
        let ended = cron.expires_at <= now
            && now - cron.expires_at <= Duration::days(ENDED_FOR_DAYS)
            && cron.recurring;
        if !soon && !ended {
            continue;
        }
        let said = if soon { "ends tomorrow" } else { "ended" };
        out.push(Reminder {
            key: format!("ending:{}", cron.id),
            group: Group::Ending,
            title: format!("“{}” {said}", cron.label),
            detail: cron.project.clone(),
            short: format!("A schedule {said}"),
            fingerprint: fingerprint(&[&cron.expires_at.to_rfc3339()]),
            urgency: 3,
            project: Some(cron.project.clone()),
            project_root: cron.project_root.clone(),
            target: Target::Cron {
                session_id: cron.session_id.clone(),
                folder: cron.folder.clone(),
            },
        });
    }
}

fn schedules(
    input: &ReminderInput,
    now: DateTime<Utc>,
    offset: FixedOffset,
    out: &mut Vec<Reminder>,
) {
    if !input.github_connected {
        if !input.jobs.is_empty() {
            let names: Vec<&str> = input
                .jobs
                .iter()
                .take(3)
                .map(|job| job.name.as_str())
                .collect();
            out.push(Reminder {
                key: "schedules:connect_github".to_string(),
                group: Group::Schedules,
                title: "Connect GitHub to see its runs.".to_string(),
                detail: names.join(", "),
                short: "Some schedules cannot be checked without GitHub".to_string(),
                fingerprint: fingerprint(&[&input.jobs.len().to_string()]),
                urgency: 5,
                project: None,
                project_root: None,
                target: Target::ConnectGithub,
            });
        }
        return;
    }
    for job in &input.jobs {
        let id = format!("{}:{}", job.full_name, job.workflow);
        let github = |page| Target::Github {
            full_name: job.full_name.clone(),
            page,
        };
        let run_at = job
            .last_run_at
            .map(|at| at.to_rfc3339())
            .unwrap_or_default();
        let conclusion = job.last_conclusion.clone().unwrap_or_default();
        let item = |kind: &str, title: String, detail: String, short: String| Reminder {
            key: format!("schedules:{kind}:{id}"),
            group: Group::Schedules,
            title,
            detail,
            short,
            fingerprint: fingerprint(&[kind, &run_at, &conclusion]),
            urgency: 0,
            project: Some(job.project.clone()),
            project_root: job.project_root.clone(),
            target: github(GithubPage::Actions),
        };
        if job.state.as_deref() == Some("disabled_inactivity") {
            out.push(item(
                "off",
                format!("GitHub turned off {}", job.name),
                job.project.clone(),
                format!("GitHub turned off {}", job.name),
            ));
            continue;
        }
        if job.last_conclusion.as_deref() == Some("failure") {
            let at = job
                .last_run_at
                .map_or_else(String::new, |at| format!(" · {}", when(at, now, offset)));
            out.push(item(
                "failed",
                format!("{} failed", job.name),
                format!("{}{at}", job.project),
                format!("{} failed", job.name),
            ));
            continue;
        }
        if let (Some(interval), Some(last)) = (job.interval_secs, job.last_run_at)
            && now - last > Duration::seconds(interval.saturating_mul(2))
        {
            out.push(item(
                "silent",
                format!("{} hasn't run since {}", job.name, when(last, now, offset)),
                job.project.clone(),
                format!("{} has gone quiet", job.name),
            ));
        }
    }
    deploys(input, now, offset, out);
}

fn deploys(
    input: &ReminderInput,
    now: DateTime<Utc>,
    offset: FixedOffset,
    out: &mut Vec<Reminder>,
) {
    for deploy in &input.deploys {
        if !matches!(deploy.state.as_str(), "failure" | "error") {
            continue;
        }
        out.push(Reminder {
            key: format!("schedules:deploy:{}", deploy.full_name),
            group: Group::Schedules,
            title: "Last deploy failed".to_string(),
            detail: format!(
                "{} · {} · {}",
                deploy.project,
                deploy.environment,
                when(deploy.created_at, now, offset)
            ),
            short: format!("{}'s last deploy failed", deploy.project),
            fingerprint: fingerprint(&[&deploy.created_at.to_rfc3339(), &deploy.state]),
            urgency: 0,
            project: Some(deploy.project.clone()),
            project_root: deploy.project_root.clone(),
            target: Target::Github {
                full_name: deploy.full_name.clone(),
                page: GithubPage::Deployments,
            },
        });
    }
}

fn teams(input: &ReminderInput, now: DateTime<Utc>, out: &mut Vec<Reminder>) {
    for team in &input.teams {
        if let Some(waiting) = &team.oldest_waiting
            && now - waiting.since > Duration::days(1)
        {
            let days = (now - waiting.since).num_days().max(1);
            out.push(Reminder {
                key: format!("teams:waiting:{}", team.slug),
                group: Group::Teams,
                title: format!(
                    "{}: {} has waited {} for your answer",
                    team.name,
                    waiting.agent,
                    plural(days, "day", "days")
                ),
                detail: waiting.title.clone(),
                short: format!("{} waits for you", team.name),
                fingerprint: fingerprint(&[
                    &waiting.since.to_rfc3339(),
                    &waiting.agent,
                    &waiting.title,
                ]),
                urgency: 1,
                project: None,
                project_root: None,
                target: Target::Team {
                    slug: team.slug.clone(),
                },
            });
        }
        for aged in &team.aged {
            out.push(Reminder {
                key: format!("teams:aged:{}:{}", team.slug, aged.task_id),
                group: Group::Teams,
                title: format!(
                    "{}: “{}” has waited past the team's limit for your help",
                    team.name, aged.title
                ),
                detail: aged.task_id.clone(),
                short: format!("{} needs your help", team.name),
                fingerprint: fingerprint(&[&aged.since.to_rfc3339(), &aged.task_id]),
                urgency: 1,
                project: None,
                project_root: None,
                target: Target::Team {
                    slug: team.slug.clone(),
                },
            });
        }
    }
}

fn services(input: &ReminderInput, out: &mut Vec<Reminder>) {
    use crate::claude_services::catalog::match_service;

    for server in &input.services {
        let known = match_service(server, None, None);
        let name = known.map_or_else(|| server.clone(), |entry| entry.name.to_string());
        let detail = known.map_or("Claude connector".to_string(), |entry| {
            let wire = entry.kind.as_wire();
            let mut chars = wire.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        });
        out.push(Reminder {
            key: format!("services:{server}"),
            group: Group::Services,
            title: format!("{name} needs you to sign in again in Claude"),
            detail,
            short: format!("{name} needs you to sign in again"),
            fingerprint: fingerprint(&[server]),
            urgency: 2,
            project: None,
            project_root: None,
            target: Target::Service,
        });
    }
}

fn runs(input: &ReminderInput, now: DateTime<Utc>, offset: FixedOffset, out: &mut Vec<Reminder>) {
    for run in &input.runs_today {
        out.push(Reminder {
            key: format!("runs:{}", run.key),
            group: Group::Runs,
            title: format!("{} ran", run.name),
            detail: format!("{} · {}", run.project, when(run.at, now, offset)),
            short: String::new(),
            fingerprint: fingerprint(&[&run.at.to_rfc3339()]),
            urgency: 9,
            project: Some(run.project.clone()),
            project_root: None,
            target: Target::Run,
        });
    }
}

/// The items of the digest at `now`, in the order of their groups; what the owner snoozed or
/// dismissed is left out.
#[must_use]
pub fn reminders(input: &ReminderInput, now: DateTime<Utc>) -> Vec<Reminder> {
    let offset = FixedOffset::east_opt(input.utc_offset_secs).unwrap_or_else(|| Utc.fix());
    let mut items = Vec::new();
    forgotten(input, now, &mut items);
    ending(input, now, &mut items);
    schedules(input, now, offset, &mut items);
    teams(input, now, &mut items);
    services(input, &mut items);
    runs(input, now, offset, &mut items);
    items.retain(|item| {
        !input.marks.iter().any(|mark| {
            mark.key == item.key
                && match &mark.kind {
                    MarkKind::Snooze { until } => *until > now,
                    MarkKind::Dismiss { fingerprint } => *fingerprint == item.fingerprint,
                }
        })
    });
    items.sort_by_key(|item| item.group);
    items
}

impl Reminder {
    /// The request that gives this item to its team, when it can be given: a forgotten session or
    /// a failed schedule or deploy.
    #[must_use]
    pub fn request_text(&self) -> Option<String> {
        let project = self.project.as_deref().unwrap_or("the project");
        match &self.target {
            Target::Session {
                days,
                reason,
                branch,
                ..
            } => {
                let title = self
                    .title
                    .strip_suffix(&format!(" · {project}"))
                    .unwrap_or(&self.title);
                let on = branch
                    .as_ref()
                    .map_or_else(String::new, |branch| format!(", on the branch {branch}"));
                Some(format!(
                    "Finish “{title}” in {project}. The work was left {} with {reason}{on}.",
                    days_ago(*days)
                ))
            }
            Target::Github { .. } => Some(format!(
                "Look into this in {project}: {}. {}.",
                self.title, self.detail
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-22T09:00:00Z")
            .expect("a moment")
            .with_timezone(&Utc)
    }

    fn ago(days: i64, hours: i64) -> DateTime<Utc> {
        now() - Duration::days(days) - Duration::hours(hours)
    }

    fn session(id: &str, last_at: DateTime<Utc>) -> SessionFact {
        SessionFact {
            id: id.to_string(),
            title: format!("Work {id}"),
            project: "recipe-cards".to_string(),
            project_root: "/home/me/recipe-cards".to_string(),
            folder: "/home/me/recipe-cards".to_string(),
            branch: Some("fix/login-redirect".to_string()),
            last_at,
            open_todos: 0,
            uncommitted: false,
            unpushed: 0,
            draft_pull_quiet_days: None,
            waiting_job: false,
        }
    }

    fn cron(id: &str, expires_at: DateTime<Utc>, recurring: bool) -> CronFact {
        CronFact {
            id: id.to_string(),
            label: "Session cron 0 7 * * *".to_string(),
            session_id: format!("s-{id}"),
            project: "price-watch".to_string(),
            project_root: Some("/home/me/price-watch".to_string()),
            folder: Some("/home/me/price-watch".to_string()),
            expires_at,
            recurring,
        }
    }

    fn job(workflow: &str) -> JobFact {
        JobFact {
            full_name: "octo/price-watch".to_string(),
            workflow: workflow.to_string(),
            name: "Hourly price check".to_string(),
            project: "price-watch".to_string(),
            project_root: Some("/home/me/price-watch".to_string()),
            interval_secs: Some(3600),
            state: Some("active".to_string()),
            last_conclusion: Some("success".to_string()),
            last_run_at: Some(now() - Duration::minutes(30)),
        }
    }

    fn keys(items: &[Reminder]) -> Vec<&str> {
        items.iter().map(|item| item.key.as_str()).collect()
    }

    fn find<'a>(items: &'a [Reminder], key: &str) -> &'a Reminder {
        items
            .iter()
            .find(|item| item.key == key)
            .unwrap_or_else(|| panic!("no item {key} in {:?}", keys(items)))
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one fixture that fires every rule and its neighbours, then one assertion per rule"
    )]
    fn reminds_of_what_needs_the_owner() {
        let mut todos = session("todos", ago(4, 1));
        todos.open_todos = 3;
        let mut unpushed = session("unpushed", ago(2, 1));
        unpushed.unpushed = 1;
        unpushed.uncommitted = true;
        let mut draft = session("draft", ago(3, 1));
        draft.draft_pull_quiet_days = Some(3);
        let mut waiting = session("waiting", ago(1, 2));
        waiting.waiting_job = true;
        let mut old = session("old", ago(15, 1));
        old.open_todos = 2;
        let mut fresh = session("fresh", ago(0, 3));
        fresh.open_todos = 2;
        let mut quiet_draft = session("quiet", ago(5, 0));
        quiet_draft.draft_pull_quiet_days = Some(2);
        let clean = session("clean", ago(4, 0));

        let mut failed = job("price.yml");
        failed.last_conclusion = Some("failure".to_string());
        failed.last_run_at = Some(now() - Duration::hours(2));
        let mut silent = job("silent.yml");
        silent.last_run_at = Some(now() - Duration::hours(7));
        let mut off = job("backup.yml");
        off.name = "Weekly database backup".to_string();
        off.state = Some("disabled_inactivity".to_string());
        let mut within = job("within.yml");
        within.last_run_at = Some(now() - Duration::minutes(110));
        let ok = job("ok.yml");

        let input = ReminderInput {
            sessions: vec![
                todos,
                unpushed,
                draft,
                waiting,
                old,
                fresh,
                quiet_draft,
                clean,
            ],
            crons: vec![
                cron("soon", now() + Duration::hours(20), true),
                cron("later", now() + Duration::days(2), true),
                cron("gone", now() - Duration::days(2), true),
                cron("gone-once", now() - Duration::days(2), false),
                cron("long-gone", now() - Duration::days(8), true),
            ],
            jobs: vec![failed, silent, off, within, ok],
            deploys: vec![
                DeployFact {
                    full_name: "octo/tidepool-web".to_string(),
                    project: "tidepool-web".to_string(),
                    project_root: None,
                    environment: "Production".to_string(),
                    state: "failure".to_string(),
                    created_at: ago(0, 14),
                },
                DeployFact {
                    full_name: "octo/shop".to_string(),
                    project: "shop".to_string(),
                    project_root: None,
                    environment: "Production".to_string(),
                    state: "success".to_string(),
                    created_at: ago(0, 1),
                },
            ],
            teams: vec![
                TeamFact {
                    slug: "tidepool".to_string(),
                    name: "Tidepool".to_string(),
                    oldest_waiting: Some(WaitingFact {
                        agent: "Mira".to_string(),
                        title: "Which payment plans to offer".to_string(),
                        since: ago(2, 3),
                    }),
                    aged: vec![AgedFact {
                        task_id: "CTV-4".to_string(),
                        title: "Pick a host".to_string(),
                        since: ago(1, 0),
                    }],
                },
                TeamFact {
                    slug: "bakery".to_string(),
                    name: "Bakery".to_string(),
                    oldest_waiting: Some(WaitingFact {
                        agent: "Theo".to_string(),
                        title: "Which flour".to_string(),
                        since: now() - Duration::hours(20),
                    }),
                    aged: vec![],
                },
            ],
            services: vec!["zoho-books".to_string()],
            runs_today: vec![RunFact {
                key: "octo/price-watch:ok.yml".to_string(),
                name: "Hourly price check".to_string(),
                project: "price-watch".to_string(),
                at: now() - Duration::minutes(30),
            }],
            marks: vec![],
            github_connected: true,
            utc_offset_secs: 0,
        };
        let items = reminders(&input, now());

        let group = |wanted: Group| -> Vec<&str> {
            items
                .iter()
                .filter(|item| item.group == wanted)
                .map(|item| item.key.as_str())
                .collect()
        };
        assert_eq!(
            group(Group::Forgotten),
            [
                "forgotten:todos",
                "forgotten:unpushed",
                "forgotten:draft",
                "forgotten:waiting"
            ],
            "the neighbours (15 days, 0 days, a quiet draft of 2 days, a clean one) do not fire: {:?}",
            keys(&items)
        );
        let todos = find(&items, "forgotten:todos");
        assert_eq!(todos.title, "Work todos · recipe-cards");
        assert_eq!(todos.detail, "4 days ago · 3 to-dos still open");
        assert_eq!(
            find(&items, "forgotten:unpushed").detail,
            "2 days ago · changes not committed, changes not pushed"
        );
        assert_eq!(
            find(&items, "forgotten:draft").detail,
            "3 days ago · its pull request is still a draft"
        );
        assert_eq!(
            find(&items, "forgotten:waiting").detail,
            "1 day ago · a background job is waiting for you"
        );

        assert_eq!(group(Group::Ending), ["ending:soon", "ending:gone"]);
        assert_eq!(
            find(&items, "ending:soon").title,
            "“Session cron 0 7 * * *” ends tomorrow"
        );
        assert_eq!(
            find(&items, "ending:gone").title,
            "“Session cron 0 7 * * *” ended"
        );
        assert_eq!(find(&items, "ending:soon").detail, "price-watch");

        let schedules = group(Group::Schedules);
        assert_eq!(
            schedules,
            [
                "schedules:failed:octo/price-watch:price.yml",
                "schedules:silent:octo/price-watch:silent.yml",
                "schedules:off:octo/price-watch:backup.yml",
                "schedules:deploy:octo/tidepool-web",
            ],
            "a run that succeeded and one within its interval do not fire"
        );
        assert_eq!(
            find(&items, "schedules:failed:octo/price-watch:price.yml").title,
            "Hourly price check failed"
        );
        assert_eq!(
            find(&items, "schedules:failed:octo/price-watch:price.yml").detail,
            "price-watch · 07:00 today"
        );
        assert_eq!(
            find(&items, "schedules:silent:octo/price-watch:silent.yml").title,
            "Hourly price check hasn't run since 02:00 today"
        );
        assert_eq!(
            find(&items, "schedules:off:octo/price-watch:backup.yml").title,
            "GitHub turned off Weekly database backup"
        );
        let deploy = find(&items, "schedules:deploy:octo/tidepool-web");
        assert_eq!(deploy.title, "Last deploy failed");
        assert_eq!(deploy.detail, "tidepool-web · Production · yesterday 19:00");

        assert_eq!(
            group(Group::Teams),
            ["teams:waiting:tidepool", "teams:aged:tidepool:CTV-4",],
            "Bakery's oldest has waited under a day"
        );
        assert_eq!(
            find(&items, "teams:waiting:tidepool").title,
            "Tidepool: Mira has waited 2 days for your answer"
        );
        assert_eq!(
            find(&items, "teams:waiting:tidepool").detail,
            "Which payment plans to offer"
        );
        assert_eq!(group(Group::Services), ["services:zoho-books"]);
        assert_eq!(
            find(&items, "services:zoho-books").title,
            "Zoho Books needs you to sign in again in Claude"
        );
        assert_eq!(find(&items, "services:zoho-books").detail, "Bookkeeping");
        assert_eq!(group(Group::Runs), ["runs:octo/price-watch:ok.yml"]);

        // Without GitHub the schedules are not judged: one item offers to connect it.
        let blind = ReminderInput {
            github_connected: false,
            ..input
        };
        let items = reminders(&blind, now());
        let schedules: Vec<&str> = items
            .iter()
            .filter(|item| item.group == Group::Schedules)
            .map(|item| item.key.as_str())
            .collect();
        assert_eq!(schedules, ["schedules:connect_github"]);
        assert_eq!(
            find(&items, "schedules:connect_github").title,
            "Connect GitHub to see its runs."
        );
    }

    #[test]
    fn snoozes_and_dismisses_until_it_changes() {
        let mut failed = job("price.yml");
        failed.last_conclusion = Some("failure".to_string());
        failed.last_run_at = Some(now() - Duration::hours(2));
        let mut todos = session("todos", ago(4, 1));
        todos.open_todos = 3;
        let input = |marks: Vec<Mark>, failed: JobFact| ReminderInput {
            sessions: vec![todos.clone()],
            jobs: vec![failed],
            marks,
            github_connected: true,
            ..ReminderInput::default()
        };
        let failed_key = "schedules:failed:octo/price-watch:price.yml";
        let all = reminders(&input(vec![], failed.clone()), now());
        assert_eq!(keys(&all), ["forgotten:todos", failed_key]);

        // Snoozed: hidden until its time, shown after it.
        let until = now() + Duration::days(1);
        let snoozed = vec![Mark {
            key: "forgotten:todos".to_string(),
            kind: MarkKind::Snooze { until },
        }];
        let hidden = reminders(&input(snoozed.clone(), failed.clone()), now());
        assert_eq!(keys(&hidden), [failed_key]);
        let after = reminders(
            &input(snoozed, failed.clone()),
            until + Duration::minutes(1),
        );
        assert!(
            keys(&after).contains(&"forgotten:todos"),
            "{:?}",
            keys(&after)
        );

        // Dismissed: hidden while the fingerprint holds, shown when a field changes.
        let fingerprint = find(&all, failed_key).fingerprint.clone();
        assert!(!fingerprint.is_empty());
        let dismissed = vec![Mark {
            key: failed_key.to_string(),
            kind: MarkKind::Dismiss { fingerprint },
        }];
        let hidden = reminders(&input(dismissed.clone(), failed.clone()), now());
        assert_eq!(keys(&hidden), ["forgotten:todos"]);
        let mut failed_again = failed;
        failed_again.last_run_at = Some(now() - Duration::minutes(5));
        let back = reminders(&input(dismissed, failed_again), now());
        assert_eq!(keys(&back), ["forgotten:todos", failed_key]);
    }

    #[test]
    fn reads_how_long_a_cron_waits_between_runs() {
        assert_eq!(cron_interval_secs("0 * * * *"), Some(3600));
        assert_eq!(cron_interval_secs("*/15 * * * *"), Some(900));
        assert_eq!(cron_interval_secs("30 7 * * *"), Some(86_400));
        assert_eq!(cron_interval_secs("0 6 * * 1"), Some(604_800));
        assert_eq!(cron_interval_secs("0 6 1 * *"), Some(2_592_000));
        assert_eq!(cron_interval_secs("not a schedule"), None);
    }
}
