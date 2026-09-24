//! When each session started and how it ended, in the log (`docs/SPEC.md` section 8.5).

use std::num::NonZeroU64;

use farik_core::team::Effort;
use farik_protocol::clock::Clock;
use farik_protocol::event::{
    EventBody, EventIds, SessionEndedBody, SessionEndedBodyReason, SessionStartedBody,
    SessionStartedBodyEffort, SessionStartedBodyPurpose, Thread, new_event,
};
use farik_store::{EventLog, StoreError};

use crate::session::{EndReason, SessionPurpose, SessionSpec};

/// Records `session.started` for `spec`: its purpose, model, and effort, on an envelope naming
/// the spec's session, agent, and task, and `ids`' team and project. `in_reply_to` is, for a
/// conversation, the seq of the latest message it was shown, which answers the mentions up to it;
/// `thread` is a ceremony's, which says which ceremony it was.
///
/// # Errors
///
/// `InvalidEvent` when the event cannot be stamped (a blank id or model); `Io` or `Sqlite` when
/// the log does not take it.
pub fn record_session_started(
    log: &EventLog,
    spec: &SessionSpec,
    in_reply_to: Option<u64>,
    thread: Option<Thread>,
    ids: &EventIds,
    clock: &dyn Clock,
) -> Result<(), StoreError> {
    let body = SessionStartedBody {
        purpose: purpose_wire(spec.purpose),
        model: spec
            .model
            .parse()
            .map_err(|error| StoreError::InvalidEvent {
                detail: format!("session.started names no model: {error}"),
            })?,
        effort: effort_wire(spec.effort),
        in_reply_to: in_reply_to.and_then(NonZeroU64::new),
        thread,
    };
    let ids = EventIds {
        task_id: spec.task_id.clone(),
        agent_id: Some(spec.agent_id.clone()),
        session_id: Some(spec.session_id.clone()),
        ..ids.clone()
    };
    append(log, EventBody::SessionStarted(body), ids, clock)
}

/// Records `session.ended` for `session_id`: why, and what the program said. The envelope names
/// the session, and the agent and task `ids` names.
///
/// # Errors
///
/// `InvalidEvent` when the event cannot be stamped; `Io` or `Sqlite` when the log does not take
/// it.
pub fn record_session_ended(
    log: &EventLog,
    session_id: &str,
    reason: EndReason,
    detail: &str,
    ids: &EventIds,
    clock: &dyn Clock,
) -> Result<(), StoreError> {
    let body = SessionEndedBody {
        reason: match reason {
            EndReason::Completed => SessionEndedBodyReason::Completed,
            EndReason::Aborted => SessionEndedBodyReason::Aborted,
            EndReason::Limit => SessionEndedBodyReason::Limit,
            EndReason::Error => SessionEndedBodyReason::Error,
            EndReason::ProviderLimit => SessionEndedBodyReason::ProviderLimit,
        },
        detail: detail.to_string(),
    };
    let ids = EventIds {
        session_id: Some(session_id.to_string()),
        ..ids.clone()
    };
    append(log, EventBody::SessionEnded(body), ids, clock)
}

fn append(
    log: &EventLog,
    body: EventBody,
    ids: EventIds,
    clock: &dyn Clock,
) -> Result<(), StoreError> {
    let event = new_event(body, clock.now(), ids).map_err(|error| StoreError::InvalidEvent {
        detail: format!("the event cannot be stamped: {error:?}"),
    })?;
    log.append(&event).map(|_| ())
}

fn purpose_wire(purpose: SessionPurpose) -> SessionStartedBodyPurpose {
    match purpose {
        SessionPurpose::Triage => SessionStartedBodyPurpose::Triage,
        SessionPurpose::Refine => SessionStartedBodyPurpose::Refine,
        SessionPurpose::Plan => SessionStartedBodyPurpose::Plan,
        SessionPurpose::Implement => SessionStartedBodyPurpose::Implement,
        SessionPurpose::Verify => SessionStartedBodyPurpose::Verify,
        SessionPurpose::Ceremony => SessionStartedBodyPurpose::Ceremony,
        SessionPurpose::Conversation => SessionStartedBodyPurpose::Conversation,
    }
}

fn effort_wire(effort: Effort) -> SessionStartedBodyEffort {
    match effort {
        Effort::Low => SessionStartedBodyEffort::Low,
        Effort::Medium => SessionStartedBodyEffort::Medium,
        Effort::High => SessionStartedBodyEffort::High,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::{DateTime, Utc};
    use farik_core::team::Effort;
    use farik_protocol::clock::FixedClock;
    use farik_protocol::event::{EventBody, EventIds, FarikEvent, SessionEndedBodyReason};
    use farik_store::{EventLog, EventQuery, IN_MEMORY, open_event_log};

    use super::{record_session_ended, record_session_started};
    use crate::recorded::fixtures::a_session_spec;
    use crate::session::{EndReason, SessionPurpose, SessionSpec};

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().expect("a fixed timestamp")
    }

    fn a_log() -> EventLog {
        open_event_log(Path::new(IN_MEMORY), at("2026-09-22T09:00:00Z"))
            .expect("a log in memory opens")
    }

    fn spec() -> SessionSpec {
        SessionSpec {
            task_id: Some("FRK-7".parse().expect("a task id")),
            ..a_session_spec()
        }
    }

    fn ids(spec: &SessionSpec) -> EventIds {
        EventIds {
            team_id: "farik".to_string(),
            project_id: "farik".to_string(),
            task_id: spec.task_id.clone(),
            agent_id: Some(spec.agent_id.clone()),
            session_id: None,
        }
    }

    fn everything(log: &EventLog) -> Vec<FarikEvent> {
        log.read(&EventQuery::default()).expect("the log reads")
    }

    #[test]
    fn records_a_session_starting_and_ending() {
        let log = a_log();
        let spec = spec();
        let clock = FixedClock::new(at("2026-09-22T10:00:00Z"));
        record_session_started(&log, &spec, None, None, &ids(&spec), &clock).expect("recorded");
        record_session_ended(
            &log,
            &spec.session_id,
            EndReason::Limit,
            "the wall clock ran out",
            &ids(&spec),
            &clock,
        )
        .expect("recorded");
        let events = everything(&log);
        assert_eq!(events.len(), 2, "{events:?}");
        for event in &events {
            assert_eq!(
                event.envelope.ids.session_id.as_deref(),
                Some(spec.session_id.as_str())
            );
            assert_eq!(event.envelope.ids.agent_id.as_deref(), Some("maya-chen"));
            assert_eq!(event.envelope.ids.task_id, spec.task_id);
            assert!(event.envelope.ids.task_id.is_some());
        }
        match &events[0].body {
            EventBody::SessionStarted(body) => {
                assert_eq!(body.purpose.to_string(), "implement");
                assert_eq!(body.model.to_string(), spec.model);
                assert_eq!(body.effort.to_string(), "high");
            }
            other => panic!("expected session.started, got {other:?}"),
        }
        match &events[1].body {
            EventBody::SessionEnded(body) => {
                assert_eq!(body.reason, SessionEndedBodyReason::Limit);
                assert_eq!(body.detail, "the wall clock ran out");
            }
            other => panic!("expected session.ended, got {other:?}"),
        }
    }

    #[test]
    fn writes_every_purpose_effort_and_reason_as_its_wire_name() {
        let log = a_log();
        let clock = FixedClock::new(at("2026-09-22T10:00:00Z"));
        let purposes = [
            (SessionPurpose::Triage, "triage"),
            (SessionPurpose::Refine, "refine"),
            (SessionPurpose::Plan, "plan"),
            (SessionPurpose::Implement, "implement"),
            (SessionPurpose::Verify, "verify"),
            (SessionPurpose::Ceremony, "ceremony"),
            (SessionPurpose::Conversation, "conversation"),
        ];
        let efforts = [
            (Effort::Low, "low"),
            (Effort::Medium, "medium"),
            (Effort::High, "high"),
        ];
        let reasons = [
            (EndReason::Completed, "completed"),
            (EndReason::Aborted, "aborted"),
            (EndReason::Limit, "limit"),
            (EndReason::Error, "error"),
        ];
        let mut expected = Vec::new();
        for (index, (purpose, purpose_wire)) in purposes.into_iter().enumerate() {
            let (effort, effort_wire) = efforts[index % efforts.len()];
            let (reason, reason_wire) = reasons[index % reasons.len()];
            let spec = SessionSpec {
                purpose,
                effort,
                ..spec()
            };
            record_session_started(&log, &spec, None, None, &ids(&spec), &clock).expect("recorded");
            record_session_ended(&log, &spec.session_id, reason, "", &ids(&spec), &clock)
                .expect("recorded");
            expected.push((purpose_wire, effort_wire, reason_wire));
        }
        let events = everything(&log);
        let written: Vec<(String, String, String)> = events
            .chunks(2)
            .map(|pair| match (&pair[0].body, &pair[1].body) {
                (EventBody::SessionStarted(started), EventBody::SessionEnded(ended)) => (
                    started.purpose.to_string(),
                    started.effort.to_string(),
                    ended.reason.to_string(),
                ),
                other => panic!("expected a start and an end, got {other:?}"),
            })
            .collect();
        let expected: Vec<(String, String, String)> = expected
            .into_iter()
            .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
            .collect();
        assert_eq!(written, expected);
    }

    #[test]
    fn records_the_providers_limit_as_the_sessions_end() {
        let log = a_log();
        let spec = spec();
        let clock = FixedClock::new(at("2026-09-22T10:00:00Z"));
        record_session_ended(
            &log,
            &spec.session_id,
            EndReason::ProviderLimit,
            "Claude AI usage limit reached",
            &ids(&spec),
            &clock,
        )
        .expect("recorded");
        match &everything(&log)[..] {
            [event] => match &event.body {
                EventBody::SessionEnded(body) => {
                    assert_eq!(body.reason.to_string(), "provider_limit");
                }
                other => panic!("expected session.ended, got {other:?}"),
            },
            other => panic!("expected one event, got {other:?}"),
        }
    }
}
