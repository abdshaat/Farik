//! Which agents have a session running in this process, and the turn every tick holds but while
//! its session runs (ADR 0064).
//!
//! A tick takes the turn at its start and holds it while it reads the board, asks the governor,
//! runs Catervas's own criteria, integrates, and prepares a session. It hands the turn back just
//! before the session runs (`session_runs`) and takes it back when the session has ended
//! (`session_ran`), to record the end and whatever follows in the same tick. So everything but a
//! session's own stream of model turns and tool calls is one at a time, as it was.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use catervas_core::contract::TaskId;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::OrchestratorError;
use crate::session::SessionPurpose;

/// Which agents have a session running in this process, and the task each is about, and the turn.
#[derive(Debug, Default)]
pub struct Work {
    /// Held by the tick that is doing anything but running a session.
    turn: Arc<AsyncMutex<()>>,
    /// Each agent with a session, and the task it is about.
    busy: Mutex<BTreeMap<String, Option<TaskId>>>,
    /// Each agent with a chat fork running beside its work (ADR 0064).
    chats: Mutex<BTreeSet<String>>,
    /// Each re-plan asked for, by epic and the builders it was asked for (ADR 0078).
    replans: Mutex<BTreeSet<String>>,
}

impl Work {
    /// The turn, for `Orchestrator::next` to take for a tick it starts.
    pub(super) fn turn(&self) -> Arc<AsyncMutex<()>> {
        Arc::clone(&self.turn)
    }

    fn busy(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Option<TaskId>>> {
        self.busy.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn chats(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.chats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records a re-plan asked for under `key`, answering whether it is the first: an epic is
    /// re-planned once for the same idle builders, so tasks that truly wait on each other are not
    /// asked about on every tick (ADR 0078). A new process asks once more.
    pub(crate) fn first_replan(&self, key: String) -> bool {
        self.replans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key)
    }

    /// Whether `agent_id` has a chat fork running.
    pub(crate) fn is_chatting(&self, agent_id: &str) -> bool {
        self.chats().contains(agent_id)
    }

    /// The task of `agent_id`'s running work session, `None` when it runs none or one about no
    /// task.
    pub(crate) fn work_task_of(&self, agent_id: &str) -> Option<TaskId> {
        self.busy().get(agent_id).cloned().flatten()
    }

    /// Whether `agent_id` has a work session running.
    pub(crate) fn is_busy(&self, agent_id: &str) -> bool {
        self.busy().contains_key(agent_id)
    }

    /// The tasks a session runs about.
    pub(crate) fn busy_tasks(&self) -> BTreeSet<TaskId> {
        self.busy().values().flatten().cloned().collect()
    }

    /// Records that `agent_id` has a session about `task_id`, until the claim is dropped. A chat
    /// fork is recorded apart from a work session, so that each runs beside the other.
    ///
    /// # Errors
    ///
    /// `Refused` with `agent_busy: <agent> has a session running` when `agent_id` holds a work
    /// session and `purpose` is not a chat, and `chat_busy: <agent> is answering its chat` when
    /// it holds a chat fork and `purpose` is one.
    pub(crate) fn claim(
        &self,
        agent_id: &str,
        task_id: Option<&TaskId>,
        purpose: SessionPurpose,
    ) -> Result<Claim<'_>, OrchestratorError> {
        let chat = purpose == SessionPurpose::Chat;
        if chat {
            if !self.chats().insert(agent_id.to_string()) {
                return Err(OrchestratorError::Refused {
                    reason: format!("chat_busy: {agent_id} is answering its chat"),
                });
            }
        } else {
            let mut busy = self.busy();
            if busy.contains_key(agent_id) {
                return Err(OrchestratorError::Refused {
                    reason: format!("agent_busy: {agent_id} has a session running"),
                });
            }
            busy.insert(agent_id.to_string(), task_id.cloned());
        }
        Ok(Claim {
            work: self,
            agent_id: agent_id.to_string(),
            chat,
        })
    }
}

/// An agent's session in `Work`: dropping it, also on a panic's unwind, frees the agent.
pub(crate) struct Claim<'a> {
    work: &'a Work,
    agent_id: String,
    chat: bool,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if self.chat {
            self.work.chats().remove(&self.agent_id);
        } else {
            self.work.busy().remove(&self.agent_id);
        }
    }
}

/// A session about to run, as its tick tells `Orchestrator::next`.
pub(super) struct Started {
    pub(super) agent_id: String,
    pub(super) task_id: Option<TaskId>,
    pub(super) purpose: String,
}

impl Started {
    pub(super) fn into_progress(self) -> super::Progress {
        super::Progress::Started {
            agent_id: self.agent_id,
            task_id: self.task_id,
            purpose: self.purpose,
        }
    }
}

/// What a tick spawned by `next` holds: the turn, until its session runs, and where to say so.
struct TickSlot {
    turn: Option<OwnedMutexGuard<()>>,
    started: UnboundedSender<Started>,
}

tokio::task_local! {
    static SLOT: RefCell<TickSlot>;
}

/// Runs `tick` holding `turn`; `started` hears of every session the tick starts.
pub(super) async fn holding_the_turn<T>(
    turn: OwnedMutexGuard<()>,
    started: UnboundedSender<Started>,
    tick: impl Future<Output = T>,
) -> T {
    SLOT.scope(
        RefCell::new(TickSlot {
            turn: Some(turn),
            started,
        }),
        tick,
    )
    .await
}

/// Called just before a session runs: in a tick spawned by `next`, hands the turn back and says
/// the session started. Outside one (a test's `tick()`), does nothing.
pub(super) fn session_runs(started: Started) {
    let _ = SLOT.try_with(|slot| {
        let mut slot = slot.borrow_mut();
        drop(slot.turn.take());
        let _ = slot.started.send(started);
    });
}

/// Called when a session has ended: in a tick spawned by `next`, waits for the turn and takes it
/// back, so the end, the cost and whatever follows are recorded one at a time.
pub(super) async fn session_ran(work: &Work) {
    let handed_back = SLOT
        .try_with(|slot| slot.borrow().turn.is_none())
        .unwrap_or(false);
    if handed_back {
        let guard = work.turn().lock_owned().await;
        let _ = SLOT.try_with(|slot| slot.borrow_mut().turn = Some(guard));
    }
}
