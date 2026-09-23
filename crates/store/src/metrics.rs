//! The harness metrics of `docs/SPEC.md` F17, computed from the projections and the contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use farik_core::contract::{TaskId, Verification};
use farik_protocol::event::CostRecordedBodyPurpose;

use crate::error::StoreError;
use crate::files::{FilesError, ProjectFiles};
use crate::projections::Projections;

/// What an accepted task cost, in dollars: the whole and each purpose's share of it.
#[derive(Debug, Clone, PartialEq)]
pub struct CostSplit {
    /// Every `cost.recorded`, divided by the accepted tasks.
    pub total: f64,
    /// The same division per session purpose, with every purpose present.
    pub by_purpose: BTreeMap<CostRecordedBodyPurpose, f64>,
}

/// The five numbers F17 tracks, over the whole project.
#[derive(Debug, Clone, PartialEq)]
pub struct HarnessMetrics {
    /// Board rows of kind `task` in `accepted`: the denominator every rate shares.
    pub accepted_tasks: u32,
    /// Accepted tasks that entered `verifying` once and `rejected` never, over accepted tasks.
    pub first_pass_acceptance_rate: Option<f64>,
    /// Interventions on every row, over accepted tasks.
    pub interventions_per_accepted_task: Option<f64>,
    /// Every cost recorded, over accepted tasks.
    pub cost_per_accepted_task_usd: Option<CostSplit>,
    /// Exit criteria Farik runs, over every exit criterion of the accepted rows' contracts.
    pub mechanically_verified_criteria_share: Option<f64>,
    /// The ISO weeks in which at least one session recorded a cost.
    pub active_weeks: u32,
}

/// Why the metrics could not be computed.
#[derive(Debug)]
pub enum MetricsError {
    /// The projections could not be read.
    Store(StoreError),
    /// An accepted row's contract could not be read.
    Files(FilesError),
}

impl fmt::Display for MetricsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "{error}"),
            Self::Files(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for MetricsError {}

impl From<StoreError> for MetricsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<rusqlite::Error> for MetricsError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Store(StoreError::from(error))
    }
}

/// Every session purpose the wire names, in the schema's order, so that a split always has all
/// seven and they add up to its total.
const PURPOSES: [CostRecordedBodyPurpose; 7] = [
    CostRecordedBodyPurpose::Triage,
    CostRecordedBodyPurpose::Refine,
    CostRecordedBodyPurpose::Plan,
    CostRecordedBodyPurpose::Implement,
    CostRecordedBodyPurpose::Verify,
    CostRecordedBodyPurpose::Ceremony,
    CostRecordedBodyPurpose::Conversation,
];

/// What the board says, counted in one query.
struct BoardCounts {
    accepted_tasks: u32,
    first_pass: u32,
    interventions: u32,
    accepted_rows: Vec<TaskId>,
}

impl Projections {
    /// The five harness metrics of F17, over the whole project: from the board's per-task counts,
    /// the cost records, and the contracts of the accepted rows, whose criteria the log does not
    /// name. A rate is `None` while no task is accepted, because a rate with no denominator is
    /// not zero.
    ///
    /// # Errors
    ///
    /// `Store` when a projection cannot be read; `Files` when an accepted row's contract cannot,
    /// which fails the whole call rather than leaving the row out of the count.
    pub fn metrics(&self, files: &ProjectFiles) -> Result<HarnessMetrics, MetricsError> {
        let counts = self.board_counts()?;
        let (spent, days) = self.spending()?;
        // The contracts are read with the connection released: they are files, not rows.
        let (mut mechanical, mut criteria) = (0_u32, 0_u32);
        for task_id in &counts.accepted_rows {
            let contract = files.read_contract(task_id).map_err(MetricsError::Files)?;
            for criterion in &contract.exit_criteria {
                criteria += 1;
                if matches!(
                    Verification::from(&criterion.verification).method(),
                    "command" | "test" | "artifact"
                ) {
                    mechanical += 1;
                }
            }
        }
        let per_task = |value: f64| {
            (counts.accepted_tasks > 0).then(|| value / f64::from(counts.accepted_tasks))
        };
        let total: f64 = spent.values().sum();
        Ok(HarnessMetrics {
            accepted_tasks: counts.accepted_tasks,
            first_pass_acceptance_rate: per_task(f64::from(counts.first_pass)),
            interventions_per_accepted_task: per_task(f64::from(counts.interventions)),
            cost_per_accepted_task_usd: per_task(total).map(|total| CostSplit {
                total,
                by_purpose: PURPOSES
                    .iter()
                    .map(|purpose| {
                        let usd = spent.get(purpose).copied().unwrap_or(0.0);
                        (*purpose, usd / f64::from(counts.accepted_tasks))
                    })
                    .collect(),
            }),
            mechanically_verified_criteria_share: (counts.accepted_tasks > 0 && criteria > 0)
                .then(|| f64::from(mechanical) / f64::from(criteria)),
            active_weeks: active_weeks(&days)?,
        })
    }

    fn board_counts(&self) -> Result<BoardCounts, MetricsError> {
        let connection = self.connection();
        let (accepted_tasks, first_pass, interventions): (i64, i64, i64) = connection.query_row(
            "SELECT
                 COALESCE(SUM(kind = 'task' AND status = 'accepted'), 0),
                 COALESCE(SUM(kind = 'task' AND status = 'accepted'
                              AND verifications = 1 AND rejections = 0), 0),
                 COALESCE(SUM(interventions), 0)
             FROM task_projections",
            (),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let mut statement =
            connection.prepare("SELECT task_id FROM task_projections WHERE status = 'accepted'")?;
        let mut accepted_rows = Vec::new();
        for id in statement.query_map((), |row| row.get::<_, String>(0))? {
            let id = id?;
            accepted_rows.push(TaskId::from_str(&id).map_err(|_| StoreError::InvalidEvent {
                detail: format!("the board holds {id:?} as a task id"),
            })?);
        }
        Ok(BoardCounts {
            accepted_tasks: count(accepted_tasks, "accepted tasks")?,
            first_pass: count(first_pass, "first-pass acceptances")?,
            interventions: count(interventions, "interventions")?,
            accepted_rows,
        })
    }

    /// What was spent per purpose, and the distinct days anything was.
    fn spending(
        &self,
    ) -> Result<(BTreeMap<CostRecordedBodyPurpose, f64>, Vec<String>), MetricsError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT purpose, SUM(cost_usd) FROM cost_records GROUP BY purpose")?;
        let mut spent = BTreeMap::new();
        for row in statement.query_map((), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })? {
            let (purpose, usd) = row?;
            let purpose = CostRecordedBodyPurpose::from_str(&purpose).map_err(|_| {
                StoreError::InvalidEvent {
                    detail: format!("a cost record holds {purpose:?} as its purpose"),
                }
            })?;
            spent.insert(purpose, usd);
        }
        let mut statement = connection.prepare("SELECT DISTINCT day FROM cost_records")?;
        let days = statement
            .query_map((), |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((spent, days))
    }
}

/// The distinct ISO 8601 weeks the days fall in: week-numbering year and week, Monday first.
/// SQLite's `%W` is not the ISO week, so the days are read as dates here.
fn active_weeks(days: &[String]) -> Result<u32, MetricsError> {
    let mut weeks = BTreeSet::new();
    for day in days {
        let date = NaiveDate::from_str(day).map_err(|_| StoreError::InvalidEvent {
            detail: format!("a cost record holds {day:?} as its day"),
        })?;
        let week = date.iso_week();
        weeks.insert((week.year(), week.week()));
    }
    Ok(u32::try_from(weeks.len()).unwrap_or(u32::MAX))
}

fn count(value: i64, what: &str) -> Result<u32, MetricsError> {
    u32::try_from(value).map_err(|_| {
        MetricsError::Store(StoreError::InvalidEvent {
            detail: format!("the board counts {value} {what}"),
        })
    })
}
