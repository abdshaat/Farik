//! An agent asleep until its model provider's limit resets (`docs/SPEC.md` 5.5). Sleep is not a
//! status: the team file is not written, and the log's `agent.slept` is all there is of it.

use chrono::{DateTime, Utc};
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
