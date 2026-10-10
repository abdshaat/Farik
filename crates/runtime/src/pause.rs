//! Whether the human has paused the team (`docs/SPEC.md` section 3): the log's last `team.paused`
//! or `team.resumed` says, so that a pause holds across a restart and every process agrees.

use catervas_protocol::event::{CatervasEvent, EventBody, EventKind, TeamPausedBodyReason};
use catervas_store::{EventLog, EventQuery, StoreError};

/// Whether the log's last `team.paused` or `team.resumed` is a `team.paused`.
///
/// # Errors
///
/// When the log cannot be read.
pub fn paused(log: &EventLog) -> Result<bool, StoreError> {
    Ok(last(log)?.is_some_and(|event| matches!(event.body, EventBody::TeamPaused(_))))
}

/// The log's last `team.paused` or `team.resumed`.
fn last(log: &EventLog) -> Result<Option<CatervasEvent>, StoreError> {
    let query = EventQuery {
        kinds: vec![EventKind::TeamPaused, EventKind::TeamResumed],
        ..EventQuery::default()
    };
    Ok(log.read(&query)?.into_iter().last())
}

/// Whether the team is paused because the model provider refused the AI account's key (5.5):
/// the log's last `team.paused` or `team.resumed` is a pause for `credential_refused`.
///
/// # Errors
///
/// When the log cannot be read.
pub fn key_refused(log: &EventLog) -> Result<bool, StoreError> {
    Ok(last(log)?.is_some_and(|event| {
        matches!(event.body, EventBody::TeamPaused(body)
            if body.reason == Some(TeamPausedBodyReason::CredentialRefused))
    }))
}

#[cfg(test)]
mod tests {
    use catervas_protocol::event::{EventBody, EventIds, new_event};
    use catervas_store::{EventLog, IN_MEMORY, open_event_log};
    use serde_json::json;
    use std::path::Path;

    use super::{key_refused, paused};

    fn a_log() -> EventLog {
        open_event_log(Path::new(IN_MEMORY), chrono::Utc::now()).expect("a log")
    }

    fn record(log: &EventLog, resumed: bool) {
        let by = serde_json::from_value(json!({ "by": "human" })).expect("a body");
        let body = if resumed {
            EventBody::TeamResumed(by)
        } else {
            EventBody::TeamPaused(by)
        };
        let ids = EventIds {
            team_id: "team".to_string(),
            project_id: "project".to_string(),
            task_id: None,
            agent_id: None,
            session_id: None,
        };
        let event = new_event(body, chrono::Utc::now(), ids).expect("an event");
        log.append(&event).expect("appended");
    }

    #[test]
    fn knows_a_pause_for_a_refused_key_until_the_next_resume() {
        let log = a_log();
        assert!(!key_refused(&log).expect("reads"));
        record(&log, false);
        assert!(!key_refused(&log).expect("reads"));
        let refused = serde_json::from_value(
            json!({ "by": "catervas", "reason": "credential_refused", "detail": "401" }),
        )
        .expect("a body");
        let ids = EventIds {
            team_id: "team".to_string(),
            project_id: "project".to_string(),
            task_id: None,
            agent_id: None,
            session_id: None,
        };
        let event =
            new_event(EventBody::TeamPaused(refused), chrono::Utc::now(), ids).expect("an event");
        log.append(&event).expect("appended");
        assert!(key_refused(&log).expect("reads"));
        record(&log, true);
        assert!(!key_refused(&log).expect("reads"));
    }

    #[test]
    fn a_new_team_is_not_paused() {
        assert!(!paused(&a_log()).expect("reads"));
    }

    #[test]
    fn the_last_of_pause_and_resume_wins() {
        let log = a_log();
        record(&log, false);
        assert!(paused(&log).expect("reads"));
        record(&log, true);
        assert!(!paused(&log).expect("reads"));
        record(&log, false);
        assert!(paused(&log).expect("reads"));
        record(&log, true);
        assert!(!paused(&log).expect("reads"));
    }
}
