//! An agent asleep until its model provider's limit resets (`docs/SPEC.md` 5.5). Sleep is not a
//! status: the team file is not written, and the log's `agent.slept` is all there is of it.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik_protocol::clock::Clock;
use farik_protocol::event::{EventBody, EventKind};
use farik_store::{EventLog, EventQuery, StoreError};

/// When the agent wakes, while it is asleep at `now`: the `until` of the last `agent.slept` for
/// it, when that is later than `now`, and nothing otherwise.
///
/// # Errors
///
/// `StoreError` when the log cannot be read.
pub fn asleep_until(
    log: &EventLog,
    agent_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, StoreError> {
    let sleeps = log.read(&EventQuery {
        agent_id: Some(agent_id.to_string()),
        kinds: vec![EventKind::AgentSlept],
        ..EventQuery::default()
    })?;
    Ok(sleeps.last().and_then(|event| match &event.body {
        EventBody::AgentSlept(body) if body.until > now => Some(body.until),
        _ => None,
    }))
}

/// What a run waits on while every agent with work is asleep: the machine's time, or a test's.
pub trait Sleeper: Send + Sync {
    /// Returns once it is `until`.
    fn sleep_until(&self, until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// A sleeper on the machine's timer: it waits `until` less the clock's now, and not at all when
/// that has passed.
pub struct TokioSleeper {
    /// The clock the run reads.
    pub clock: Arc<dyn Clock + Send + Sync>,
}

impl Sleeper for TokioSleeper {
    fn sleep_until(&self, until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let wait = (until - self.clock.now()).to_std().unwrap_or_default();
        Box::pin(tokio::time::sleep(wait))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::Duration;
    use farik_protocol::event::{AgentSleptBody, EventBody, EventIds, NewEvent};
    use farik_store::{EventLog, IN_MEMORY, open_event_log};

    use super::asleep_until;
    use crate::tools::fixtures::at;

    /// Records that `agent` sleeps until `until`.
    fn slept(log: &EventLog, agent: &str, until: chrono::DateTime<chrono::Utc>) {
        log.append(&NewEvent {
            recorded_at: at(),
            ids: EventIds {
                team_id: "farik".to_string(),
                project_id: "farik".to_string(),
                agent_id: Some(agent.to_string()),
                ..EventIds::default()
            },
            body: EventBody::AgentSlept(AgentSleptBody {
                until,
                detail: "Claude AI usage limit reached".to_string(),
            }),
        })
        .expect("appends");
    }

    #[test]
    fn answers_asleep_until_from_the_last_sleep() {
        let log = open_event_log(Path::new(IN_MEMORY), at()).expect("the log opens");
        slept(&log, "dev-a", at() + Duration::hours(3));
        slept(&log, "dev-a", at() + Duration::hours(1));

        assert_eq!(
            asleep_until(&log, "dev-a", at()).expect("the log reads"),
            Some(at() + Duration::hours(1))
        );
        assert_eq!(
            asleep_until(&log, "dev-b", at()).expect("the log reads"),
            None
        );
        assert_eq!(
            asleep_until(&log, "dev-a", at() + Duration::hours(2)).expect("the log reads"),
            None
        );
    }
}
