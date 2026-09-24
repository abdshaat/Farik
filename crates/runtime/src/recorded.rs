//! Sessions replayed from transcripts the Claude Code program once printed, so that everything
//! above the runtime is tested without the program, the network, or money.

pub mod fixtures;

use std::collections::VecDeque;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::locked;
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

/// Answers a replayed call of one of Farik's tools: given the session's id, the tool's full name
/// (`mcp__farik__<name>`), and its input, the answer the session is to see.
pub type ToolRunner =
    Arc<dyn Fn(String, String, Value) -> Pin<Box<dyn Future<Output = Value> + Send>> + Send + Sync>;

/// The prefix Claude Code gives the tools of Farik's own MCP server.
const FARIK_PREFIX: &str = "mcp__farik__";

/// Replays transcripts through `RuntimeAdapter`, one per session started or resumed, in the
/// order given, and remembers what it was asked. With a `ToolRunner`, a replayed call of a Farik
/// tool is really made, and its answer replaces the recorded one.
#[derive(Default)]
pub struct RecordedAdapter {
    transcripts: Mutex<VecDeque<Transcript>>,
    started: Mutex<Vec<SessionSpec>>,
    runner: Option<ToolRunner>,
}

impl fmt::Debug for RecordedAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordedAdapter")
            .field("transcripts", &self.transcripts)
            .field("started", &self.started)
            .field("runner", &self.runner.as_ref().map(|_| "a tool runner"))
            .finish()
    }
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

    /// An adapter that will play `transcripts` in order, calling `runner` for every replayed call
    /// of a Farik tool and giving the session its answer in place of the recorded one. Each
    /// replay runs in a spawned `tokio` task, so a session must be started inside a runtime.
    #[must_use]
    pub fn with_tools(transcripts: Vec<Transcript>, runner: ToolRunner) -> RecordedAdapter {
        RecordedAdapter {
            runner: Some(runner),
            ..RecordedAdapter::new(transcripts)
        }
    }

    /// Every spec a session was started with, in order.
    #[must_use]
    pub fn started(&self) -> Vec<SessionSpec> {
        locked(&self.started).clone()
    }

    /// How many transcripts are still to be played.
    #[must_use]
    pub fn transcripts_left(&self) -> usize {
        locked(&self.transcripts).len()
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
        let receiver = match &self.runner {
            None => filled(events),
            Some(runner) => {
                let runtime =
                    tokio::runtime::Handle::try_current().map_err(|_| RuntimeError::Spawn {
                        detail: "a replay that calls tools runs inside a tokio runtime".to_string(),
                    })?;
                let (sender, receiver) = channel(events.len().max(1));
                runtime.spawn(replay(
                    events,
                    session_id.to_string(),
                    Arc::clone(runner),
                    sender,
                ));
                receiver
            }
        };
        Ok(Box::new(RecordedSession {
            session_id: session_id.to_string(),
            receiver,
            aborted: AtomicBool::new(false),
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
        _prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        self.play(session_id)
    }
}

/// A recorded session: every event is in its channel before the caller reads the first.
struct RecordedSession {
    session_id: String,
    receiver: Receiver<SessionEvent>,
    aborted: AtomicBool,
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

    fn send(&self, _text: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn abort(&self) -> Result<(), RuntimeError> {
        self.aborted.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Sends `events` in order, calling `runner` after each call of a Farik tool and putting its
/// answer, as compact JSON, in the output of the next return of that tool. It stops when the
/// receiver is gone, which is what an abort leaves.
async fn replay(
    events: Vec<SessionEvent>,
    session_id: String,
    runner: ToolRunner,
    sender: Sender<SessionEvent>,
) {
    let mut answers: Vec<(String, String)> = Vec::new();
    for event in events {
        let event = match event {
            SessionEvent::ToolReturned { tool, output } => {
                match answers.iter().position(|(named, _)| *named == tool) {
                    Some(at) => SessionEvent::ToolReturned {
                        output: answers.remove(at).1,
                        tool,
                    },
                    None => SessionEvent::ToolReturned { tool, output },
                }
            }
            other => other,
        };
        let call = match &event {
            SessionEvent::ToolCalled { tool, input } if tool.starts_with(FARIK_PREFIX) => {
                Some((tool.clone(), input.clone()))
            }
            _ => None,
        };
        if sender.send(event).await.is_err() {
            return;
        }
        if let Some((tool, input)) = call {
            let answer = runner(session_id.clone(), tool.clone(), input).await;
            answers.push((tool, answer.to_string()));
        }
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use tokio::sync::mpsc::error::TryRecvError;

    use super::fixtures::{a_session_spec, reads_a_file, replays_farik_read_board, write_denied};
    use super::{RecordedAdapter, ToolRunner, Transcript};
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
    fn counts_the_transcripts_left_to_play() {
        let adapter = RecordedAdapter::new(vec![reads_a_file(), write_denied()]);
        assert_eq!(adapter.transcripts_left(), 2);
        adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        assert_eq!(adapter.transcripts_left(), 1);
    }

    #[test]
    fn remembers_the_specs_it_started() {
        let adapter = RecordedAdapter::new(vec![reads_a_file()]);
        let spec = a_session_spec();
        let handle = adapter
            .start_session(spec.clone())
            .expect("a transcript is left");
        handle.send("go on").expect("a recorded session takes text");
        assert_eq!(adapter.started(), vec![spec]);
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
    }

    #[test]
    fn refuses_to_replay_a_transcript_that_does_not_parse() {
        let adapter = RecordedAdapter::new(vec![Transcript::from_jsonl("not json\n")]);
        assert!(matches!(
            adapter.start_session(a_session_spec()),
            Err(RuntimeError::Protocol { .. })
        ));
    }

    /// The calls a runner was given: the session, the tool, and the input.
    type Calls = Arc<Mutex<Vec<(String, String, Value)>>>;

    /// A runner that answers `{"answered": true}` and remembers every call.
    fn recording_runner() -> (ToolRunner, Calls) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&calls);
        let runner: ToolRunner = Arc::new(move |session_id, tool, input| {
            seen.lock()
                .expect("no test panics holding it")
                .push((session_id, tool, input));
            Box::pin(async { json!({ "answered": true }) })
        });
        (runner, calls)
    }

    async fn read_to_the_end(handle: &mut dyn SessionHandle) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        while let Some(event) = handle.events().recv().await {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn answers_a_replayed_farik_tool_call_with_the_runner() {
        let (runner, calls) = recording_runner();
        let adapter = RecordedAdapter::with_tools(vec![replays_farik_read_board()], runner);
        let spec = a_session_spec();
        let mut handle = adapter
            .start_session(spec.clone())
            .expect("a transcript is left");
        let events = read_to_the_end(handle.as_mut()).await;
        let tool = "mcp__farik__farik_read_board".to_string();
        assert_eq!(
            *calls.lock().expect("no test panics holding it"),
            vec![(spec.session_id.clone(), tool.clone(), json!({}))]
        );
        let mut parser = StreamParser::default();
        let expected: Vec<SessionEvent> = replays_farik_read_board()
            .lines()
            .flat_map(|line| parser.parse_line(line).expect("a recorded line parses"))
            .map(|event| match event {
                SessionEvent::ToolReturned { tool, .. } => SessionEvent::ToolReturned {
                    tool,
                    output: "{\"answered\":true}".to_string(),
                },
                other => other,
            })
            .collect();
        assert_eq!(events, expected);
        assert!(
            matches!(&events[0], SessionEvent::ToolCalled { tool: called, .. } if *called == tool)
        );
    }

    #[tokio::test]
    async fn leaves_other_tools_to_the_transcript() {
        let (runner, calls) = recording_runner();
        let adapter = RecordedAdapter::with_tools(vec![reads_a_file()], runner);
        let mut handle = adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        let events = read_to_the_end(handle.as_mut()).await;
        assert!(calls.lock().expect("no test panics holding it").is_empty());
        let mut parser = StreamParser::default();
        let expected: Vec<SessionEvent> = reads_a_file()
            .lines()
            .flat_map(|line| parser.parse_line(line).expect("a recorded line parses"))
            .collect();
        assert_eq!(events, expected);
    }

    #[test]
    fn replays_without_tools_as_before() {
        let mut parser = StreamParser::default();
        let expected: Vec<SessionEvent> = replays_farik_read_board()
            .lines()
            .flat_map(|line| parser.parse_line(line).expect("a recorded line parses"))
            .collect();
        let adapter = RecordedAdapter::new(vec![replays_farik_read_board()]);
        let mut handle = adapter
            .start_session(a_session_spec())
            .expect("a transcript is left");
        assert_eq!(drain(handle.as_mut()), expected);
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
