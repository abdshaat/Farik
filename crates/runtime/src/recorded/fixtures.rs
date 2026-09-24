//! Transcripts recorded from `claude` 2.1.280 with `--model haiku` on 2026-09-22, trimmed of
//! hook lines, thinking signatures, and machine paths; and transcripts of Farik tool calls,
//! hand-written in the same shapes, which no recording can hold because the answers depend on the
//! project a test builds. For tests, in this crate and in others.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use farik_core::budget::DEFAULT_SESSION_LIMITS;
use farik_core::pricing::Usage;
use farik_core::team::Effort;
use serde_json::json;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use super::{FARIK_PREFIX, ToolRunner, Transcript};
#[cfg(unix)]
use crate::daemon::{DaemonState, HookRequest, decide_pre_tool_use};
use crate::session::{
    EndReason, RuntimeAdapter, RuntimeError, SessionEvent, SessionHandle, SessionPurpose,
    SessionSpec,
};
#[cfg(unix)]
use crate::tools::call_tool;

/// An implement session for `maya-chen` in `/workspace`, with the team's default limits.
#[must_use]
pub fn a_session_spec() -> SessionSpec {
    SessionSpec {
        session_id: "0b7e4f0e-5a3c-4c55-9d6f-2f1a8c3e9b10".to_string(),
        agent_id: "maya-chen".to_string(),
        task_id: None,
        purpose: SessionPurpose::Implement,
        system_prompt: "You are a Software Developer.".to_string(),
        model: "claude-haiku-4-5-20251001".to_string(),
        effort: Effort::High,
        farik_tools: vec!["farik_read_task".to_string()],
        builtin_tools: ["Glob", "Grep", "Read", "ToolSearch"]
            .map(str::to_string)
            .to_vec(),
        mcp_servers: Vec::new(),
        cwd: PathBuf::from("/workspace"),
        limits: DEFAULT_SESSION_LIMITS,
        initial_prompt: "Read note.txt.".to_string(),
    }
}

/// A Read of `/workspace/note.txt`, its result, the text `hello fixture`, and a successful end.
#[must_use]
pub fn reads_a_file() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/reads_a_file.jsonl"))
}

/// A Write refused by a safety check, the model's reply, and a successful end.
#[must_use]
pub fn write_denied() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/write_denied.jsonl"))
}

/// A Read call, then the end a one-turn limit forces.
#[must_use]
pub fn hits_the_turn_limit() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/hits_the_turn_limit.jsonl"))
}

/// A Read, then a Write the `PreToolUse` hook denied with `farik says no`, and a successful end.
#[must_use]
pub fn hook_denies_a_write() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/hook_denies_a_write.jsonl"))
}

// No capture of a refused session exists yet (2.1.280's show only `allowed`); the next four are
// hand-written in the captured lines' shapes, one per way a provider's limit can be told.

/// A rate-limit event `rejected` until 1790119200, then a result that is an error.
#[must_use]
pub fn provider_limit_rejected() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/provider_limit_rejected.jsonl"))
}

/// A result that is an error with `api_error_status` 429 and no rate-limit event.
#[must_use]
pub fn provider_limit_429() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/provider_limit_429.jsonl"))
}

/// A result that is an error saying `Claude AI usage limit reached`.
#[must_use]
pub fn provider_limit_text() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/provider_limit_text.jsonl"))
}

/// An `allowed` rate-limit event, then a `success` result that is an error: a 500.
#[must_use]
pub fn success_with_is_error() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/success_with_is_error.jsonl"))
}

/// A call of `mcp__farik__farik_read_board` with no input, its answer, and a successful end.
/// Hand-written in the shapes above; with `RecordedAdapter::with_tools` the answer is the runner's.
#[must_use]
pub fn replays_farik_read_board() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/replays_farik_read_board.jsonl"))
}

/// The Product Manager's plan session: `farik_assign_task` of FRK-1 to `dev-a`, reviewed by
/// `dev-b`, and a successful end. Hand-written.
#[must_use]
pub fn plan_assigns_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_assigns_frk_1.jsonl"))
}

/// `dev-a`'s implement session of FRK-1, done: `touch done.txt` through `farik_exec`, a commit of
/// it, C1 recorded as passed, a completion note, and `verifying` asked for. Hand-written.
#[must_use]
pub fn implement_finishes_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/implement_finishes_frk_1.jsonl"))
}

/// `dev-a`'s implement session of FRK-1, stopped early: `touch done.txt`, a commit of it, and a
/// progress note saying C1 has not run. Hand-written.
#[must_use]
pub fn implement_stops_early() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/implement_stops_early.jsonl"))
}

/// `dev-a`'s implement session of FRK-1 saying so in the channel: one `farik_post_message`, and a
/// successful end. Hand-written.
#[must_use]
pub fn implement_reacts_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/implement_reacts_frk_1.jsonl"))
}

/// `dev-b`'s verify session of FRK-1: a review note, and nothing else. Hand-written.
#[must_use]
pub fn review_writes_note() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/review_writes_note.jsonl"))
}

/// `dev-b`'s verify session of FRK-1 that answers no `review` criterion: a review note, and
/// nothing else. Hand-written.
#[must_use]
pub fn review_answers_nothing() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/review_answers_nothing.jsonl"))
}

/// The Product Manager's review of the epic FRK-1: C2 recorded passed, then a review note.
/// Hand-written.
#[must_use]
pub fn review_epic_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/review_epic_frk_1.jsonl"))
}

/// The Product Manager's review of the epic FRK-1: C2 recorded failed, then a review note saying
/// why. Hand-written.
#[must_use]
pub fn review_epic_fails_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/review_epic_fails_frk_1.jsonl"))
}

/// The Product Manager's verify session of FRK-1: `accepted` asked for. Hand-written.
#[must_use]
pub fn accept_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/accept_frk_1.jsonl"))
}

/// The Product Manager's triage of FRK-1: `farik_triage_request` of size `large`, "A file and the
/// check that it exists.". Hand-written.
#[must_use]
pub fn triage_frk_1_large() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/triage_frk_1_large.jsonl"))
}

/// The Scrum Master's triage of FRK-1: `farik_triage_request` of size `small`, "One file and its
/// check: a task.". Hand-written.
#[must_use]
pub fn triage_by_sm_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/triage_by_sm_frk_1.jsonl"))
}

/// The Scrum Master's judgment of FRK-1: `farik_record_judgment` answering yes to both questions,
/// "One file in five dollars, and C1 fails while done.txt is missing.". Hand-written.
#[must_use]
pub fn judge_frk_1_passes() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/judge_frk_1_passes.jsonl"))
}

/// The Scrum Master's judgment of FRK-1: `farik_record_judgment` answering that it fits its budget
/// and that its criteria would not detect the failure, "C1 checks that done.txt exists, not what
/// it says.". Hand-written.
#[must_use]
pub fn judge_frk_1_fails() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/judge_frk_1_fails.jsonl"))
}

/// The Product Manager's refine session of FRK-1 that asks the human "Should done.txt be empty?"
/// with `farik_ask_human`, and nothing else. Hand-written.
#[must_use]
pub fn refine_asks_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/refine_asks_frk_1.jsonl"))
}

/// The Product Manager's refine session of FRK-1 as an epic: `farik_write_contract` with its intent,
/// `product_manager` as assignee role and the human as reviewer, `done.txt` its one allowed path,
/// C1 (`command`, `test -f done.txt`) and C2 (`review`), one item out of scope, risk `low`, and a
/// budget of 5 dollars and 10 sessions. Hand-written.
#[must_use]
pub fn refine_writes_epic_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/refine_writes_epic_frk_1.jsonl"))
}

/// The Product Manager's refine session of FRK-1 as a task: `farik_write_contract` restating the
/// request's fields. Hand-written.
#[must_use]
pub fn refine_writes_task_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/refine_writes_task_frk_1.jsonl"))
}

/// The Product Manager's plan session breaking epic FRK-1 down: `farik_create_task` of one task
/// under it, "Add done.txt", with the request's fields and a budget of 2 dollars. Hand-written.
#[must_use]
pub fn plan_breaks_down_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_breaks_down_frk_1.jsonl"))
}

/// The Product Manager's plan session assigning FRK-2 to `dev-a`, reviewed by `dev-b`, with
/// `farik_assign_task`. Hand-written.
#[must_use]
pub fn plan_assigns_frk_2() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_assigns_frk_2.jsonl"))
}

/// The Product Manager's plan session closing epic FRK-1 out: a completion note, "FRK-2 added
/// done.txt; nothing left out.", and `verifying` asked for. Hand-written.
#[must_use]
pub fn plan_closes_epic_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_closes_epic_frk_1.jsonl"))
}

/// The assigner's planning session of the open sprint: `farik_plan_sprint` of FRK-1, then "S1
/// holds FRK-1, within its budget.". Hand-written.
#[must_use]
pub fn plan_sprint_frk_1() -> Transcript {
    Transcript::from_jsonl(include_str!("transcripts/plan_sprint_frk_1.jsonl"))
}

/// An adapter whose every session reports `usage` at once and then either ends `completed` at
/// once or waits for `abort` and ends `aborted`, or, when its abort fails, waits for ever: the
/// shapes a recorded transcript, which reports usage only on its last line, cannot show.
pub struct UsageThenWaitAdapter {
    usage: Usage,
    completes: bool,
    abort_fails: bool,
    started: Mutex<Vec<SessionSpec>>,
    starts: Arc<AtomicU32>,
    aborts: Arc<AtomicU32>,
    waiting: Mutex<Vec<Sender<SessionEvent>>>,
}

impl UsageThenWaitAdapter {
    /// Sessions that report `usage` and wait to be aborted.
    #[must_use]
    pub fn waiting(usage: Usage) -> Self {
        Self::new(usage, false, false)
    }

    /// Sessions that report `usage` and end `completed`.
    #[must_use]
    pub fn completing(usage: Usage) -> Self {
        Self::new(usage, true, false)
    }

    /// Sessions that report `usage` and wait, and whose every abort is counted and fails.
    #[must_use]
    pub fn failing_to_abort(usage: Usage) -> Self {
        Self::new(usage, false, true)
    }

    fn new(usage: Usage, completes: bool, abort_fails: bool) -> Self {
        Self {
            usage,
            completes,
            abort_fails,
            started: Mutex::new(Vec::new()),
            starts: Arc::new(AtomicU32::new(0)),
            aborts: Arc::new(AtomicU32::new(0)),
            waiting: Mutex::new(Vec::new()),
        }
    }

    /// Ends every session still waiting `completed`, as if each had finished its work.
    ///
    /// # Panics
    ///
    /// When a test panicked while holding the list.
    pub fn complete(&self) {
        for sender in self
            .waiting
            .lock()
            .expect("no test panics holding it")
            .drain(..)
        {
            let _ = sender.try_send(SessionEvent::Ended {
                reason: EndReason::Completed,
                detail: "done".to_string(),
                resets_at: None,
            });
        }
    }

    /// How many sessions this adapter has started, as a counter another task can watch.
    #[must_use]
    pub fn started_count(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.starts)
    }

    /// Every spec a session was started with, in order.
    ///
    /// # Panics
    ///
    /// When a test panicked while holding the list.
    #[must_use]
    pub fn started(&self) -> Vec<SessionSpec> {
        self.started
            .lock()
            .expect("no test panics holding it")
            .clone()
    }

    /// How many times a session of this adapter was aborted.
    #[must_use]
    pub fn aborts(&self) -> u32 {
        self.aborts.load(Ordering::SeqCst)
    }
}

impl RuntimeAdapter for UsageThenWaitAdapter {
    fn start_session(&self, spec: SessionSpec) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        let (sender, receiver) = channel(4);
        sender
            .try_send(SessionEvent::UsageReported(self.usage))
            .expect("the channel has room");
        let sender = if self.completes {
            sender
                .try_send(SessionEvent::Ended {
                    reason: EndReason::Completed,
                    detail: "done".to_string(),
                    resets_at: None,
                })
                .expect("the channel has room");
            None
        } else {
            self.waiting
                .lock()
                .expect("no test panics holding it")
                .push(sender.clone());
            Some(sender)
        };
        let handle = WaitingSession {
            session_id: spec.session_id.clone(),
            receiver,
            sender: Mutex::new(sender),
            aborts: Arc::clone(&self.aborts),
            abort_fails: self.abort_fails,
        };
        self.started
            .lock()
            .expect("no test panics holding it")
            .push(spec);
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(handle))
    }

    fn resume(
        &self,
        _session_id: &str,
        _prompt: &str,
    ) -> Result<Box<dyn SessionHandle>, RuntimeError> {
        Err(RuntimeError::Spawn {
            detail: "this adapter does not resume".to_string(),
        })
    }
}

/// A session of `UsageThenWaitAdapter`.
struct WaitingSession {
    session_id: String,
    receiver: Receiver<SessionEvent>,
    sender: Mutex<Option<Sender<SessionEvent>>>,
    aborts: Arc<AtomicU32>,
    abort_fails: bool,
}

impl SessionHandle for WaitingSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn events(&mut self) -> &mut Receiver<SessionEvent> {
        &mut self.receiver
    }

    fn send(&self, _text: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn abort(&self) -> Result<(), RuntimeError> {
        self.aborts.fetch_add(1, Ordering::SeqCst);
        if self.abort_fails {
            return Err(RuntimeError::Spawn {
                detail: "the session would not stop".to_string(),
            });
        }
        if let Some(sender) = self
            .sender
            .lock()
            .expect("no test panics holding it")
            .take()
        {
            let _ = sender.try_send(SessionEvent::Ended {
                reason: EndReason::Aborted,
                detail: "aborted".to_string(),
                resets_at: None,
            });
        }
        Ok(())
    }
}

/// Answers a replayed Farik tool call as a real session's would be: the `PreToolUse` decision
/// first, a deny answered `{"error": "<reason>"}`; then the tool, called with the session's tool
/// context as the daemon has it at that moment, an error answered `{"error": "<its words>"}`. No
/// `PostToolUse` is recorded.
#[cfg(unix)]
pub fn tool_runner(daemon: Arc<DaemonState>) -> ToolRunner {
    Arc::new(move |session_id, tool, input| {
        let daemon = Arc::clone(&daemon);
        Box::pin(async move {
            let request = HookRequest {
                session_id: session_id.clone(),
                cwd: PathBuf::new(),
                hook_event_name: "PreToolUse".to_string(),
                tool_name: tool.clone(),
                tool_input: input.clone(),
                tool_use_id: None,
                tool_response: None,
                duration_ms: None,
            };
            let decision = decide_pre_tool_use(&request, &daemon);
            if !decision.allow {
                return json!({ "error": decision.reason });
            }
            let Some(context) = daemon.tool_context(&session_id) else {
                return json!({ "error": format!("the daemon answers for no session {session_id}") });
            };
            let name = tool.strip_prefix(FARIK_PREFIX).unwrap_or(&tool);
            match call_tool(&context, name, input).await {
                Ok(answer) => answer,
                Err(error) => json!({ "error": error.to_string() }),
            }
        })
    })
}
