//! Whether the human has paused the team (`docs/SPEC.md` section 3): the log's last `team.paused`
//! or `team.resumed` says, so that a pause holds across a restart and every process agrees.

use farik_protocol::event::{EventBody, EventKind};
use farik_store::{EventLog, EventQuery, StoreError};

/// Whether the log's last `team.paused` or `team.resumed` is a `team.paused`.
///
/// # Errors
///
/// When the log cannot be read.
pub fn paused(log: &EventLog) -> Result<bool, StoreError> {
    let events = log.read(&EventQuery {
        kinds: vec![EventKind::TeamPaused, EventKind::TeamResumed],
        ..EventQuery::default()
    })?;
    Ok(matches!(
        events.last().map(|event| &event.body),
        Some(EventBody::TeamPaused(_))
    ))
}

#[cfg(test)]
mod tests {
    use farik_protocol::event::{EventBody, EventIds, new_event};
    use farik_store::{EventLog, IN_MEMORY, open_event_log};
    use serde_json::json;
    use std::path::Path;

    use super::paused;

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
