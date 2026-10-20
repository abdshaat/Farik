//! The owner's goal (`docs/SPEC.md` 5.18, ADR 0075): what the log says about it, folded into its
//! status. Pure: the caller reads the goal events and passes them in order, each with the time it
//! was recorded.

use chrono::{DateTime, Utc};

use crate::contract::TaskStatus;
use crate::governor::task_status::is_terminal;

/// How many rounds that filed work, with no progress between, a goal runs before it asks the owner
/// to check in. It bounds a loop as a session's tool-call limit does; it is not a budget.
pub const GOAL_ROUNDS_BEFORE_CHECK_IN: usize = 5;

/// How many requests one goal session may file.
pub const GOAL_WORK_PER_SESSION: usize = 5;

/// How the owner ended a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOutcome {
    /// The owner said the goal was reached.
    Reached,
    /// The owner let the goal go.
    Dropped,
}

/// Where a reading of the goal's number came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingSource {
    /// An agent read it from the goal's connected service.
    Connector,
    /// The owner told Catervas.
    Owner,
}

/// One goal event, as the fold reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum GoalRecord {
    /// The owner set a goal, or edited the standing one (the same `goal` again).
    Set {
        /// The goal's id, `goal-<n>`.
        goal: String,
        /// The owner's words.
        words: String,
        /// The number to reach.
        target: Option<f64>,
        /// What the number counts.
        unit: Option<String>,
        /// The connector the number is read from.
        server: Option<String>,
    },
    /// The owner paused the goal.
    Paused {
        /// The goal's id.
        goal: String,
    },
    /// The owner resumed the goal, or told a stalled one to keep going.
    Resumed {
        /// The goal's id.
        goal: String,
    },
    /// The owner ended the goal.
    Ended {
        /// The goal's id.
        goal: String,
        /// How it ended.
        outcome: GoalOutcome,
    },
    /// The goal's number was read.
    Measured {
        /// The goal's id.
        goal: String,
        /// The number.
        value: f64,
        /// Where it came from.
        source: ReadingSource,
        /// The connector it was read from, for a connector reading.
        server: Option<String>,
        /// The agent that read it, for a connector reading.
        agent: Option<String>,
    },
    /// A planning round ended.
    Planned {
        /// The goal's id.
        goal: String,
        /// How many requests the round filed.
        filed: usize,
    },
}

/// One reading of the goal's number.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// The number.
    pub value: f64,
    /// Where it came from.
    pub source: ReadingSource,
    /// The connector it was read from.
    pub server: Option<String>,
    /// The agent that read it.
    pub agent: Option<String>,
    /// When it was recorded.
    pub at: DateTime<Utc>,
}

/// The standing goal, as the log leaves it.
#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    /// Its id, `goal-<n>`.
    pub id: String,
    /// The owner's words.
    pub words: String,
    /// The number to reach.
    pub target: Option<f64>,
    /// What the number counts.
    pub unit: Option<String>,
    /// The connector the number is read from.
    pub server: Option<String>,
    /// Whether the owner paused it.
    pub paused: bool,
    /// Every reading, oldest first.
    pub readings: Vec<Reading>,
    /// The planning rounds that filed work.
    pub rounds: usize,
    /// The rounds that filed work since the last progress.
    pub rounds_since_progress: usize,
    /// Whether the last round filed nothing and nothing new has come in since.
    pub nothing_planned: bool,
}

/// Why a goal waits for the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallReason {
    /// The last round filed nothing.
    NothingPlanned,
    /// Rounds of work, and the number has not moved.
    NoProgress,
}

/// Where a goal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    /// The team works towards it.
    Active,
    /// The owner paused it.
    Paused,
    /// The count has reached the target; the owner decides whether the goal is reached.
    Met,
    /// The team waits for the owner.
    Stalled(StallReason),
}

/// A goal the owner ended.
#[derive(Debug, Clone, PartialEq)]
pub struct EndedGoal {
    /// Its id.
    pub id: String,
    /// The owner's words.
    pub words: String,
    /// How it ended.
    pub outcome: GoalOutcome,
    /// When.
    pub at: DateTime<Utc>,
}

/// Everything the log says about goals.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalBook {
    /// The goal that stands, active or paused.
    pub standing: Option<Goal>,
    /// The goal ended last.
    pub last_ended: Option<EndedGoal>,
    /// The id the next goal takes.
    pub next_id: String,
}

impl Goal {
    /// The newest reading.
    #[must_use]
    pub fn latest(&self) -> Option<&Reading> {
        self.readings.last()
    }

    /// Where the goal stands: the first that holds of paused, met, nothing planned, no progress.
    #[must_use]
    pub fn status(&self) -> GoalStatus {
        if self.paused {
            return GoalStatus::Paused;
        }
        if let (Some(target), Some(latest)) = (self.target, self.latest())
            && latest.value >= target
        {
            return GoalStatus::Met;
        }
        if self.nothing_planned {
            return GoalStatus::Stalled(StallReason::NothingPlanned);
        }
        if self.rounds_since_progress >= GOAL_ROUNDS_BEFORE_CHECK_IN {
            return GoalStatus::Stalled(StallReason::NoProgress);
        }
        GoalStatus::Active
    }
}

/// The number in a goal's id, `goal-<n>`.
fn number_of(goal: &str) -> usize {
    goal.strip_prefix("goal-")
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(0)
}

/// The id a record is about.
fn id_of(record: &GoalRecord) -> &str {
    match record {
        GoalRecord::Set { goal, .. }
        | GoalRecord::Paused { goal }
        | GoalRecord::Resumed { goal }
        | GoalRecord::Ended { goal, .. }
        | GoalRecord::Measured { goal, .. }
        | GoalRecord::Planned { goal, .. } => goal,
    }
}

/// Folds goal events, oldest first, into the standing goal, the last one ended, and the next id.
/// Records of any goal but the standing one change nothing, though their ids are counted.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per kind of record")]
pub fn fold_goals(records: &[(DateTime<Utc>, GoalRecord)]) -> GoalBook {
    let mut standing: Option<Goal> = None;
    let mut last_ended = None;
    let mut highest = 0;
    for (at, record) in records {
        highest = highest.max(number_of(id_of(record)));
        if let GoalRecord::Set {
            goal,
            words,
            target,
            unit,
            server,
        } = record
        {
            match standing.as_mut() {
                None => {
                    standing = Some(Goal {
                        id: goal.clone(),
                        words: words.clone(),
                        target: *target,
                        unit: unit.clone(),
                        server: server.clone(),
                        paused: false,
                        readings: Vec::new(),
                        rounds: 0,
                        rounds_since_progress: 0,
                        nothing_planned: false,
                    });
                }
                Some(current) if current.id == *goal => {
                    // An edit keeps the readings and the rounds, and is the owner's word.
                    current.words.clone_from(words);
                    current.target = *target;
                    current.unit.clone_from(unit);
                    current.server.clone_from(server);
                    current.rounds_since_progress = 0;
                    current.nothing_planned = false;
                }
                Some(_) => {}
            }
            continue;
        }
        let Some(current) = standing
            .as_mut()
            .filter(|current| current.id == id_of(record))
        else {
            continue;
        };
        match record {
            GoalRecord::Set { .. } => {}
            GoalRecord::Paused { .. } => current.paused = true,
            GoalRecord::Resumed { .. } => {
                current.paused = false;
                current.rounds_since_progress = 0;
                current.nothing_planned = false;
            }
            GoalRecord::Ended { outcome, .. } => {
                last_ended = Some(EndedGoal {
                    id: current.id.clone(),
                    words: current.words.clone(),
                    outcome: *outcome,
                    at: *at,
                });
                standing = None;
            }
            GoalRecord::Measured {
                value,
                source,
                server,
                agent,
                ..
            } => {
                let rises = current
                    .readings
                    .iter()
                    .all(|earlier| *value > earlier.value);
                if *source == ReadingSource::Owner || rises {
                    current.rounds_since_progress = 0;
                }
                current.nothing_planned = false;
                current.readings.push(Reading {
                    value: *value,
                    source: *source,
                    server: server.clone(),
                    agent: agent.clone(),
                    at: *at,
                });
            }
            GoalRecord::Planned { filed, .. } => {
                if *filed > 0 {
                    current.rounds += 1;
                    current.rounds_since_progress += 1;
                    current.nothing_planned = false;
                } else {
                    current.nothing_planned = true;
                }
            }
        }
    }
    GoalBook {
        standing,
        last_ended,
        next_id: format!("goal-{}", highest + 1),
    }
}

/// Whether the board is quiet: every task is accepted or cancelled.
#[must_use]
pub fn board_quiet(statuses: impl IntoIterator<Item = TaskStatus>) -> bool {
    statuses.into_iter().all(is_terminal)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, 9, minute, 0)
            .single()
            .expect("a real time")
    }

    fn set(target: f64) -> GoalRecord {
        GoalRecord::Set {
            goal: "goal-1".to_string(),
            words: "Reach 10 paying customers".to_string(),
            target: Some(target),
            unit: Some("paying customers".to_string()),
            server: Some("stripe".to_string()),
        }
    }

    fn measured(value: f64, source: ReadingSource) -> GoalRecord {
        GoalRecord::Measured {
            goal: "goal-1".to_string(),
            value,
            source,
            server: Some("stripe".to_string()),
            agent: Some("finn".to_string()),
        }
    }

    fn planned(filed: usize) -> GoalRecord {
        GoalRecord::Planned {
            goal: "goal-1".to_string(),
            filed,
        }
    }

    fn timed(records: Vec<GoalRecord>) -> Vec<(DateTime<Utc>, GoalRecord)> {
        records
            .into_iter()
            .zip(0..)
            .map(|(record, minute)| (at(minute), record))
            .collect()
    }

    fn standing(records: Vec<GoalRecord>) -> Goal {
        fold_goals(&timed(records)).standing.expect("a goal stands")
    }

    #[test]
    fn folds_a_goal_through_its_life() {
        let id = || "goal-1".to_string();
        let records = timed(vec![
            set(10.0),
            measured(4.0, ReadingSource::Connector),
            set(12.0),
            GoalRecord::Paused { goal: id() },
            GoalRecord::Resumed { goal: id() },
            measured(12.0, ReadingSource::Connector),
            GoalRecord::Ended {
                goal: id(),
                outcome: GoalOutcome::Reached,
            },
        ]);

        let before = fold_goals(&records[..6]);
        let goal = before.standing.expect("a goal stands");
        assert_eq!(goal.status(), GoalStatus::Met);
        assert_eq!(goal.readings.len(), 2);
        assert_eq!(goal.target, Some(12.0));
        assert!(!goal.paused);
        assert_eq!(before.next_id, "goal-2");

        let after = fold_goals(&records);
        assert!(after.standing.is_none());
        let ended = after.last_ended.expect("one ended");
        assert_eq!(ended.id, "goal-1");
        assert_eq!(ended.outcome, GoalOutcome::Reached);
        assert_eq!(after.next_id, "goal-2");
    }

    #[test]
    fn stalls_without_progress_and_after_an_empty_round() {
        let mut records = vec![set(10.0)];
        records.extend((0..GOAL_ROUNDS_BEFORE_CHECK_IN).map(|_| planned(2)));
        let goal = standing(records.clone());
        assert_eq!(goal.rounds, GOAL_ROUNDS_BEFORE_CHECK_IN);
        assert_eq!(goal.status(), GoalStatus::Stalled(StallReason::NoProgress));

        records.push(measured(2.0, ReadingSource::Connector));
        assert_eq!(standing(records.clone()).status(), GoalStatus::Active);

        records.push(planned(0));
        assert_eq!(
            standing(records.clone()).status(),
            GoalStatus::Stalled(StallReason::NothingPlanned)
        );
        records.push(measured(2.0, ReadingSource::Connector));
        assert_eq!(standing(records).status(), GoalStatus::Active);

        assert!(!board_quiet([
            TaskStatus::Accepted,
            TaskStatus::Escalated,
            TaskStatus::Accepted
        ]));
        assert!(board_quiet([TaskStatus::Accepted, TaskStatus::Cancelled]));
    }
}
