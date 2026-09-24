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

/// The longest the machine's timer is waited on before the clock is read again. The timer does
/// not run while the machine is suspended and the clock does, so a wait read once would wake late
/// by the time suspended; read every chunk, it wakes at most one chunk late.
const CHUNK: std::time::Duration = std::time::Duration::from_secs(1);

/// A sleeper on the machine's timer: it waits until the clock says `until`, reading the clock
/// again at least every `CHUNK`, and not at all when that has passed.
pub struct TokioSleeper {
    /// The clock the run reads.
    pub clock: Arc<dyn Clock + Send + Sync>,
}

impl Sleeper for TokioSleeper {
    fn sleep_until(&self, until: DateTime<Utc>) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            // A time past is an error from `to_std`, and ends the wait as a zero one does.
            while let Ok(left) = (until - self.clock.now()).to_std()
                && !left.is_zero()
            {
                tokio::time::sleep(left.min(CHUNK)).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use chrono::{DateTime, Duration, Utc};
    use farik_protocol::clock::{Clock, MovableClock};
    use farik_protocol::event::{AgentSleptBody, EventBody, EventIds, NewEvent};
    use farik_store::{EventLog, IN_MEMORY, open_event_log};

    use super::{Sleeper, TokioSleeper, asleep_until};
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

    /// The machine's clock.
    struct MachineClock;

    impl Clock for MachineClock {
        fn now(&self) -> DateTime<Utc> {
            Utc::now()
        }
    }

    /// `sleeper`'s wait until `until`, and how long it took, failing the test rather than
    /// hanging when it takes more than three seconds.
    async fn timed(sleeper: &TokioSleeper, until: DateTime<Utc>) -> std::time::Duration {
        let started = std::time::Instant::now();
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            sleeper.sleep_until(until),
        )
        .await
        .expect("the wait ends");
        started.elapsed()
    }

    #[tokio::test]
    async fn waits_on_the_machines_timer_until_the_time() {
        let sleeper = TokioSleeper {
            clock: Arc::new(MachineClock),
        };

        let took = timed(&sleeper, Utc::now() + Duration::milliseconds(300)).await;

        assert!(took >= std::time::Duration::from_millis(250), "{took:?}");
    }

    #[tokio::test]
    async fn does_not_wait_for_a_time_past() {
        let sleeper = TokioSleeper {
            clock: Arc::new(MachineClock),
        };

        let took = timed(&sleeper, Utc::now() - Duration::seconds(1)).await;

        assert!(took < std::time::Duration::from_millis(100), "{took:?}");
    }

    #[tokio::test]
    async fn wakes_when_the_clock_jumps_past_the_time() {
        // A machine that suspends stops the timer and not the clock: after it resumes, the clock
        // is past the time while the timer still has most of the wait to go.
        let clock = Arc::new(MovableClock::new(at()));
        let sleeper = TokioSleeper {
            clock: Arc::clone(&clock) as Arc<dyn Clock + Send + Sync>,
        };
        let until = at() + Duration::hours(1);
        let resumed = async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            clock.set(until);
        };

        let (took, ()) = tokio::join!(timed(&sleeper, until), resumed);

        assert!(took < std::time::Duration::from_secs(3), "{took:?}");
    }
}
