//! What the analytics read from a team's own log and repositories (`docs/SPEC.md` 4.4, 8.4;
//! ADR 0059): what its agents spent, how long its tasks take, what it finished, what was denied
//! and what has been blocked too long, and the commits on its integration branch.

use std::collections::BTreeMap;
use std::path::Path;

use catervas_protocol::event::{EventBody, EventKind};
use catervas_store::analytics::Window;
use catervas_store::git::Git;
use catervas_store::{EventQuery, Projections, StoreError};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Offset as _, Utc};

/// A task blocked for longer than this many days is a blocker.
pub const BLOCKED_DAYS: i64 = 3;

/// The address Catervas's own commits are authored under.
pub const CATERVAS_EMAIL: &str = "catervas@localhost";

/// A task that has been blocked too long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedTask {
    /// The task's id.
    pub task_id: String,
    /// Its title.
    pub title: String,
    /// Whole days it has been blocked.
    pub days: i64,
}

/// What a team's own log says of a period.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TeamActivity {
    /// What the agents' sessions cost, in US dollars.
    pub agents_usd: f64,
    /// The sessions that started.
    pub sessions: u64,
    /// Seconds the agents' sessions ran.
    pub active_secs: u64,
    /// The pull requests the team opened.
    pub pull_requests: u64,
    /// The tool calls refused.
    pub denials: u64,
    /// The mean days from a task's first `in_progress` to `accepted`, for tasks accepted in the
    /// period; none when none were.
    pub cycle_days: Option<f64>,
    /// The tasks accepted in the period.
    pub throughput: u64,
    /// The sessions that started on each local day.
    pub sessions_per_day: BTreeMap<NaiveDate, u64>,
    /// The tasks blocked for more than three days at `now`, longest first.
    pub blocked: Vec<BlockedTask>,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a span of seconds is far below 2^53, where an f64 stops being exact"
)]
fn days_of(secs: i64) -> f64 {
    secs as f64 / 86_400.0
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a count of tasks is far below 2^53, where an f64 stops being exact"
)]
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Measures the team whose log `projections` read, over `window`, with blockers judged at `now`.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn measure_team(
    projections: &Projections,
    window: &Window,
    now: DateTime<Utc>,
) -> Result<TeamActivity, StoreError> {
    let offset = FixedOffset::east_opt(window.utc_offset_secs).unwrap_or_else(|| Utc.fix());
    let events = projections.log().read(&EventQuery {
        kinds: vec![
            EventKind::CostRecorded,
            EventKind::SessionStarted,
            EventKind::SessionEnded,
            EventKind::TaskTransitioned,
            EventKind::ToolDenied,
            EventKind::PullRequestOpened,
        ],
        ..EventQuery::default()
    })?;
    let inside = |at: DateTime<Utc>| at >= window.from && at < window.until;
    let mut activity = TeamActivity::default();
    // The first start of each session and, per task, the first `in_progress`, the last move and
    // when it was made, and when it was accepted.
    let mut started: BTreeMap<String, DateTime<Utc>> = BTreeMap::new();
    let mut first_work: BTreeMap<String, DateTime<Utc>> = BTreeMap::new();
    let mut last_move: BTreeMap<String, (String, DateTime<Utc>)> = BTreeMap::new();
    let mut cycles: Vec<f64> = Vec::new();
    for event in &events {
        let at = event.envelope.recorded_at;
        let session = event.envelope.ids.session_id.as_deref();
        let task = event
            .envelope
            .ids
            .task_id
            .as_ref()
            .map(|id| id.as_str().to_string());
        match &event.body {
            EventBody::CostRecorded(body) if inside(at) => activity.agents_usd += body.cost_usd,
            EventBody::SessionStarted(_) => {
                if let Some(session) = session {
                    started.entry(session.to_string()).or_insert(at);
                }
                if inside(at) {
                    activity.sessions += 1;
                    *activity
                        .sessions_per_day
                        .entry(at.with_timezone(&offset).date_naive())
                        .or_default() += 1;
                }
            }
            EventBody::SessionEnded(_) => {
                if let Some(start) = session.and_then(|session| started.get(session))
                    && inside(*start)
                {
                    activity.active_secs += (at - *start).num_seconds().max(0).unsigned_abs();
                }
            }
            EventBody::ToolDenied(_) if inside(at) => activity.denials += 1,
            EventBody::PullRequestOpened(_) if inside(at) => activity.pull_requests += 1,
            EventBody::TaskTransitioned(body) => {
                let Some(task) = task else { continue };
                let to = body.to.to_string();
                if to == "in_progress" {
                    first_work.entry(task.clone()).or_insert(at);
                }
                if to == "accepted" && inside(at) {
                    activity.throughput += 1;
                    if let Some(start) = first_work.get(&task) {
                        cycles.push(days_of((at - *start).num_seconds()));
                    }
                }
                last_move.insert(task, (to, at));
            }
            _ => {}
        }
    }
    if !cycles.is_empty() {
        activity.cycle_days = Some(mean(&cycles));
    }
    let titles: BTreeMap<String, String> = projections
        .board()?
        .into_iter()
        .map(|row| (row.task_id.as_str().to_string(), row.title))
        .collect();
    for (task, (to, since)) in &last_move {
        if to == "blocked" && now - *since > Duration::days(BLOCKED_DAYS) {
            activity.blocked.push(BlockedTask {
                task_id: task.clone(),
                title: titles.get(task).cloned().unwrap_or_default(),
                days: (now - *since).num_days(),
            });
        }
    }
    activity
        .blocked
        .sort_by(|a, b| b.days.cmp(&a.days).then_with(|| a.task_id.cmp(&b.task_id)));
    Ok(activity)
}

/// The commits of a period on a team's integration branches, by whom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommitSplit {
    /// Commits the owner made.
    pub owner: u32,
    /// Commits Catervas's agents made.
    pub team: u32,
}

/// The commits in `window` on `branch` of the repository at `root`, split by whether
/// `CATERVAS_EMAIL` authored them. A folder that is not a repository, or has no such branch, counts
/// none.
#[must_use]
pub fn commits_of(root: &Path, branch: &str, window: &Window) -> CommitSplit {
    let git = Git::open(root.to_path_buf());
    let count = |by| {
        git.commits_in_period(branch, window.from, window.until, by)
            .unwrap_or(0)
    };
    let team = count(Some(CATERVAS_EMAIL));
    CommitSplit {
        owner: count(None).saturating_sub(team),
        team,
    }
}

/// Events for a test that reads a team's log.
#[cfg(test)]
pub(crate) mod fixtures {
    use catervas_protocol::event::{
        EventKind, NewEvent, event_from_value, fixtures::an_event_wire,
    };
    use catervas_store::Projections;
    use chrono::{DateTime, Utc};
    use serde_json::{Value, json};

    /// Appends an event of `kind` at `when`, the fixture's own wire with the fields of `extra`
    /// laid over it (`body` merged field by field), and projects it.
    pub(crate) fn append(
        projections: &Projections,
        kind: EventKind,
        when: DateTime<Utc>,
        extra: &Value,
    ) {
        let mut wire = an_event_wire(kind);
        wire["recorded_at"] = json!(when.to_rfc3339());
        for (key, value) in extra.as_object().expect("an object") {
            if key == "body" {
                for (field, value) in value.as_object().expect("a body") {
                    wire["body"][field] = value.clone();
                }
            } else {
                wire[key] = value.clone();
            }
        }
        let event = event_from_value(&wire).expect("a valid event");
        let appended = projections
            .log()
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        projections.apply(&appended).expect("projects");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::fixtures::append;
    use catervas_store::{IN_MEMORY, open_event_log, open_projections};
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .single()
            .expect("a valid time")
    }

    fn week() -> Window {
        Window {
            from: at(19, 0, 0),
            until: at(26, 0, 0),
            utc_offset_secs: 0,
        }
    }

    fn task(projections: &Projections, id: &str, title: &str) {
        append(
            projections,
            EventKind::TaskCreated,
            at(10, 9, 0),
            &json!({ "task_id": id, "body": { "summary": { "kind": "task", "title": title,
                                                           "status": "draft", "risk": "low" } } }),
        );
    }

    fn moved(projections: &Projections, id: &str, from: &str, to: &str, when: DateTime<Utc>) {
        append(
            projections,
            EventKind::TaskTransitioned,
            when,
            &json!({ "task_id": id, "body": { "from": from, "to": to } }),
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one seeded log, and every number read from it"
    )]
    fn measures_a_team_s_activity() {
        let log = Arc::new(open_event_log(Path::new(IN_MEMORY), at(1, 0, 0)).expect("a log"));
        let projections = open_projections(log).expect("projections");
        for (id, title) in [
            ("CTV-1", "Add login"),
            ("CTV-2", "Fix the footer"),
            ("CTV-3", "Waits on keys"),
            ("CTV-4", "Waits on a reply"),
        ] {
            task(&projections, id, title);
        }
        // CTV-1 takes one day from in_progress to accepted, CTV-2 two days and 4.8 hours.
        moved(
            &projections,
            "CTV-1",
            "assigned",
            "in_progress",
            at(20, 9, 0),
        );
        moved(&projections, "CTV-1", "verifying", "accepted", at(21, 9, 0));
        moved(
            &projections,
            "CTV-2",
            "assigned",
            "in_progress",
            at(20, 7, 0),
        );
        moved(
            &projections,
            "CTV-2",
            "verifying",
            "accepted",
            at(22, 11, 48),
        );
        // CTV-3 has been blocked four days at `now`, CTV-4 two.
        moved(
            &projections,
            "CTV-3",
            "in_progress",
            "blocked",
            at(18, 12, 0),
        );
        moved(
            &projections,
            "CTV-4",
            "in_progress",
            "blocked",
            at(20, 12, 0),
        );
        // The agents' sessions and what they cost: one inside the week, one before it.
        let session = |kind, when, id: &str| {
            append(
                &projections,
                kind,
                when,
                &json!({ "session_id": id, "agent_id": "dev-a" }),
            );
        };
        session(EventKind::SessionStarted, at(20, 10, 0), "s1");
        session(EventKind::SessionEnded, at(20, 10, 30), "s1");
        session(EventKind::SessionStarted, at(10, 10, 0), "old");
        for (when, usd) in [
            (at(20, 10, 30), 1.25),
            (at(21, 10, 30), 0.5),
            (at(12, 9, 0), 9.0),
        ] {
            append(
                &projections,
                EventKind::CostRecorded,
                when,
                &json!({ "session_id": "s1", "agent_id": "dev-a", "body": { "cost_usd": usd } }),
            );
        }
        for when in [at(20, 11, 0), at(23, 11, 0), at(5, 11, 0)] {
            append(
                &projections,
                EventKind::ToolDenied,
                when,
                &json!({ "session_id": "s1", "agent_id": "dev-a" }),
            );
        }
        append(
            &projections,
            EventKind::PullRequestOpened,
            at(21, 12, 0),
            &json!({}),
        );

        let found = measure_team(&projections, &week(), at(22, 12, 0)).expect("measured");
        assert!(
            (found.agents_usd - 1.75).abs() < 1e-9,
            "{}",
            found.agents_usd
        );
        assert_eq!((found.sessions, found.active_secs), (1, 1800));
        assert_eq!(found.pull_requests, 1);
        assert_eq!(found.denials, 2);
        assert_eq!(found.throughput, 2);
        let cycle = found.cycle_days.expect("a cycle time");
        assert!((cycle - 1.6).abs() < 1e-9, "{cycle}");
        assert_eq!(
            found.blocked,
            [BlockedTask {
                task_id: "CTV-3".to_string(),
                title: "Waits on keys".to_string(),
                days: 4
            }]
        );
        assert_eq!(
            found
                .sessions_per_day
                .get(&NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
            Some(&1)
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn splits_commits_by_catervas_s_identity() {
        use catervas_store::git::fixtures::TempRepo;

        let repo = TempRepo::new("analytics-commits");
        let commit = |email: &str, name: &str, date: &str| {
            repo.git(&["config", "user.email", email]);
            repo.git(&["config", "user.name", name]);
            repo.write(&format!("{date}-{email}.txt"), "x\n");
            repo.commit_at(&format!("work {date}"), date);
        };
        commit("me@example.invalid", "Me", "2026-10-18T10:00:00Z");
        commit("me@example.invalid", "Me", "2026-10-20T10:00:00Z");
        commit("me@example.invalid", "Me", "2026-10-21T10:00:00Z");
        commit(CATERVAS_EMAIL, "Dev (Catervas)", "2026-10-22T10:00:00Z");
        commit(
            "Catervas@Localhost",
            "Dev (Catervas)",
            "2026-10-23T10:00:00Z",
        );
        commit(CATERVAS_EMAIL, "Dev (Catervas)", "2026-10-27T10:00:00Z");
        let branch = repo.git_output(&["rev-parse", "--abbrev-ref", "HEAD"]);
        let split = commits_of(&repo.path, &branch, &week());
        assert_eq!(split, CommitSplit { owner: 2, team: 2 });
        assert_eq!(
            commits_of(&repo.path.join("nowhere"), &branch, &week()),
            CommitSplit::default()
        );
    }
}
