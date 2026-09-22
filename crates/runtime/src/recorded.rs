//! Sessions replayed from transcripts the Claude Code program once printed, so that everything
//! above the runtime is tested without the program, the network, or money.

pub mod fixtures;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::mpsc::{Receiver, channel};

use crate::session::{
    EndReason, RuntimeAdapter, RuntimeError, SessionEvent, SessionHandle, SessionSpec,
};
use crate::stream::StreamParser;

/// The lines of one recorded `stream-json` session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    lines: Vec<String>,
}

impl Transcript {
    /// A transcript from JSON-lines text; blank lines are passed over.
    #[must_use]
    pub fn from_jsonl(text: &str) -> Transcript {
        Transcript {
            lines: text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// Its lines, in the order the program printed them.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

/// Replays transcripts through `RuntimeAdapter`, one per session started or resumed, in the
/// order given, and remembers what it was asked.
#[derive(Debug, Default)]
pub struct RecordedAdapter {
    transcripts: Mutex<VecDeque<Transcript>>,
    started: Mutex<Vec<SessionSpec>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl RecordedAdapter {
    /// An adapter that will play `transcripts` in order.
    #[must_use]
    pub fn new(transcripts: Vec<Transcript>) -> RecordedAdapter {
        RecordedAdapter {
            transcripts: Mutex::new(transcripts.into()),
            ..RecordedAdapter::default()
        }
    }

    /// Every spec a session was started with, in order.
    #[must_use]
    pub fn started(&self) -> Vec<SessionSpec> {
        locked(&self.started).clone()
    }

    /// Every text sent to a session, including the prompt of each resume, in order.
    #[must_use]
    pub fn sent(&self) -> Vec<String> {
        locked(&self.sent).clone()
    }

    fn play(&self, session_id: &str) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let transcript =
            locked(&self.transcripts)
                .pop_front()
                .ok_or_else(|| RuntimeError::Spawn {
                    detail: "the recorded adapter has no transcript left to play".to_string(),
                })?;
        let mut parser = StreamParser::default();
        let mut events = Vec::new();
        for line in transcript.lines() {
            events.extend(parser.parse_line(line)?);
        }
        Ok(Box::new(RecordedSession {
            session_id: session_id.to_string(),
            receiver: filled(events),
            aborted: AtomicBool::new(false),
            sent: Arc::clone(&self.sent),
        }))
    }
}

impl RuntimeAdapter for RecordedAdapter {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let handle = self.play(&spec.session_id)?;
        locked(&self.started).push(spec);
        Ok(handle)
    }

    fn resume(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let handle = self.play(session_id)?;
        locked(&self.sent).push(prompt.to_string());
        Ok(handle)
    }
}

/// A recorded session: every event is in its channel before the caller reads the first.
struct RecordedSession {
    session_id: String,
    receiver: Receiver<SessionEvent>,
    aborted: AtomicBool,
    sent: Arc<Mutex<Vec<String>>>,
}

impl SessionHandle for RecordedSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn events(&mut self) -> &mut Receiver<SessionEvent> {
        // `abort` has only `&self`; the swap waits for the next `&mut self`, which is this call.
        if self.aborted.swap(false, Ordering::SeqCst) {
            self.receiver = filled(vec![SessionEvent::Ended {
                reason: EndReason::Aborted,
                detail: "aborted".to_string(),
            }]);
        }
        &mut self.receiver
    }

    fn send(&self, text: &str) -> Result<(), RuntimeError> {
        locked(&self.sent).push(text.to_string());
        Ok(())
    }

    fn abort(&self) -> Result<(), RuntimeError> {
        self.aborted.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// A channel holding `events` whose sender is already gone, so it closes once they are read.
fn filled(events: Vec<SessionEvent>) -> Receiver<SessionEvent> {
    let (sender, receiver) = channel(events.len().max(1));
    for event in events {
        // The channel was sized to hold every event, and its receiver is alive until returned.
        let _ = sender.try_send(event);
    }
    receiver
}

/// A poisoned lock here only means a test panicked while holding it; what it guards is still
/// whole, because every write is a single push or pop.
fn locked<Value>(mutex: &Mutex<Value>) -> MutexGuard<'_, Value> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::error::TryRecvError;

    use super::fixtures::{a_session_spec, reads_a_file, write_denied};
    use super::{RecordedAdapter, Transcript};
    use crate::session::{EndReason, RuntimeAdapter, RuntimeError, SessionEvent, SessionHandle};
    use crate::stream::StreamParser;

    fn drain(handle: &mut dyn SessionHandle) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        loop {
            match handle.events().try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Disconnected) => return events,
                Err(TryRecvError::Empty) => panic!("a recorded session never waits"),
            }
        }
    }

    #[test]
    fn replays_a_transcript_as_session_events() {
        let mut parser = StreamParser::default();
        let expected: Vec<SessionEvent> = reads_a_file()
            .lines()
            .flat_map(|line| parser.parse_line(line).expect("a recorded line parses"))
            .collect();
        let adapter = RecordedAdapter::new(vec![reads_a_file()]);
        let mut handle = adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        assert_eq!(drain(handle.as_mut()), expected);
    }

    #[test]
    fn plays_transcripts_in_order_across_start_and_resume() {
        let adapter = RecordedAdapter::new(vec![reads_a_file(), write_denied()]);
        let mut first = adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        assert!(
            drain(first.as_mut()).iter().any(
                |event| matches!(event, SessionEvent::ToolCalled { tool, .. } if tool == "Read")
            )
        );
        let mut second = adapter
            .resume("s-1", "continue")
            .expect("a second transcript is left");
        assert!(drain(second.as_mut()).iter().any(
            |event| matches!(event, SessionEvent::ToolDenied { tool, .. } if tool == "Write")
        ));
    }

    #[test]
    fn answers_spawn_when_no_transcript_is_left() {
        let adapter = RecordedAdapter::new(vec![reads_a_file()]);
        adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        assert!(matches!(
            adapter.start_session(a_session_spec()),
            Err(RuntimeError::Spawn { .. })
        ));
    }

    #[test]
    fn remembers_the_specs_it_started_and_the_texts_it_was_sent() {
        let adapter = RecordedAdapter::new(vec![reads_a_file()]);
        let spec = a_session_spec();
        let handle = adapter
            .start_session(spec.clone())
            .expect("a transcript is left");
        handle.send("go on").expect("a recorded session takes text");
        assert_eq!(adapter.started(), vec![spec]);
        assert_eq!(adapter.sent(), vec!["go on".to_string()]);
    }

    #[test]
    fn ends_an_aborted_session_with_aborted() {
        let adapter = RecordedAdapter::new(vec![reads_a_file()]);
        let mut handle = adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        handle.abort().expect("a recorded session can be stopped");
        assert_eq!(
            drain(handle.as_mut()),
            vec![SessionEvent::Ended {
                reason: EndReason::Aborted,
                detail: "aborted".to_string(),
            }]
        );
    }

    #[test]
    fn hands_out_the_session_id_of_the_spec() {
        let adapter = RecordedAdapter::new(vec![reads_a_file(), write_denied()]);
        let spec = a_session_spec();
        let handle = adapter
            .start_session(spec.clone())
            .expect("a transcript is left");
        assert_eq!(handle.session_id(), spec.session_id);
        let resumed = adapter
            .resume("s-9", "continue")
            .expect("a second transcript is left");
        assert_eq!(resumed.session_id(), "s-9");
        assert_eq!(adapter.sent(), vec!["continue".to_string()]);
    }

    #[test]
    fn refuses_to_replay_a_transcript_that_does_not_parse() {
        let adapter = RecordedAdapter::new(vec![Transcript::from_jsonl("not json\n")]);
        assert!(matches!(
            adapter.start_session(a_session_spec()),
            Err(RuntimeError::Protocol { .. })
        ));
    }

    #[test]
    fn passes_over_blank_lines_in_a_transcript() {
        let transcript = Transcript::from_jsonl("{\"type\":\"a\"}\n\n   \n{\"type\":\"b\"}\n");
        assert_eq!(
            transcript.lines().collect::<Vec<_>>(),
            vec!["{\"type\":\"a\"}", "{\"type\":\"b\"}"]
        );
    }
}
