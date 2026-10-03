//! What waits on the human (`docs/SPEC.md` 5.7 and 5.16): the questions nobody answered, the
//! connector calls waiting to be allowed, the plans awaiting approval, the escalations, the results waiting on the human's acceptance, and
//! the tasks waiting to be integrated by hand. One list, which the command line and the browser
//! both read.

use std::str::FromStr;

use farik_core::contract::{Role, TaskId, TaskKind, TaskStatus};
use farik_core::governor::done::result_awaits_human;
use farik_core::governor::permissions::ApprovalKey;
use farik_core::team::{Integration, Team};
use farik_protocol::event::{EventBody, EventKind, FarikEvent, TaskStatusWire};

use crate::files::ProjectFiles;
use crate::{EventLog, EventQuery, Projections, StoreError, TaskProjection};

/// What kind of thing waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingKind {
    /// A plan to approve.
    Approval,
    /// A finished result to accept.
    Acceptance,
    /// An agent's question.
    Question,
    /// Any other escalation.
    Help,
    /// An accepted task to add to the project by hand.
    Integration,
    /// A connector's call to allow or refuse (ADR 0031).
    ToolApproval,
}

impl WaitingKind {
    /// The wire's word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Acceptance => "acceptance",
            Self::Question => "question",
            Self::Help => "help",
            Self::Integration => "integration",
            Self::ToolApproval => "tool_approval",
        }
    }
}

/// One thing that waits on the human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// The task it is about.
    pub task_id: TaskId,
    /// What waits.
    pub kind: WaitingKind,
    /// The agent it waits with, when there is one.
    pub agent_id: Option<String>,
    /// The task's title.
    pub title: String,
    /// What waits, in a sentence the person reads.
    pub line: String,
    /// A question's id: the seq of its `question.asked`.
    pub question_id: Option<u64>,
    /// An escalation's reason, as the log words it.
    pub reason: Option<String>,
    /// A connector call's ask.
    pub approval: Option<ToolAsk>,
}

/// A connector call that waits for the human, as its `tool_approval.requested` recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAsk {
    /// The event's seq, the approval's id.
    pub approval: u64,
    /// The connector's server.
    pub server: String,
    /// The bare tool name.
    pub tool: String,
    /// The call's whole input, as compact JSON.
    pub input: String,
}

/// A grant of the human's that one call may still use (ADR 0031): `tool_approval.granted` was the
/// first decision on the approval, no `tool.called` used it, and no session of the asking agent
/// about the task that started after the grant has ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGrant {
    /// The seq of the `tool_approval.requested`.
    pub approval: u64,
    /// The seq of its `tool_approval.granted`: only a session started after it may use it.
    pub granted_at: u64,
    /// What the call must match.
    pub key: ApprovalKey,
}

/// The grants still open among one task's events, which must hold its `tool_approval.` events,
/// its `tool.called`, and its `session.started` and `session.ended`. A decision recorded with an
/// agent or a session on its envelope was not the human's, and decides nothing.
#[must_use]
pub fn open_grants(events: &[FarikEvent]) -> Vec<OpenGrant> {
    events
        .iter()
        .filter_map(|asked| {
            let EventBody::ToolApprovalRequested(body) = &asked.body else {
                return None;
            };
            let approval = asked.envelope.seq;
            let ids = &asked.envelope.ids;
            let granted = decision_on(events, approval)?;
            let EventBody::ToolApprovalGranted(_) = granted.body else {
                return None;
            };
            let granted_at = granted.envelope.seq;
            let agent_id = ids.agent_id.clone()?;
            let task_id = ids.task_id.clone()?;
            let used = events.iter().any(|event| {
                matches!(&event.body, EventBody::ToolCalled(called)
                    if called.approval.map(std::num::NonZeroU64::get) == Some(approval))
            });
            // A session of the asking agent that started after the grant, and has ended since.
            let lapsed = events.iter().any(|started| {
                started.envelope.seq > granted_at
                    && matches!(started.body, EventBody::SessionStarted(_))
                    && started.envelope.ids.agent_id.as_deref() == Some(agent_id.as_str())
                    && events.iter().any(|ended| {
                        ended.envelope.seq > started.envelope.seq
                            && matches!(ended.body, EventBody::SessionEnded(_))
                            && ended.envelope.ids.session_id.is_some()
                            && ended.envelope.ids.session_id == started.envelope.ids.session_id
                    })
            });
            (!used && !lapsed).then(|| OpenGrant {
                approval,
                granted_at,
                key: ApprovalKey {
                    agent_id,
                    task_id,
                    server: body.server.to_string(),
                    tool: body.tool.to_string(),
                    input_sha256: body.input_sha256.to_string(),
                },
            })
        })
        .collect()
}

/// The first decision the human recorded on `approval`, granted or refused.
#[must_use]
pub fn decision_on(events: &[FarikEvent], approval: u64) -> Option<&FarikEvent> {
    events.iter().find(|event| {
        let ids = &event.envelope.ids;
        ids.agent_id.is_none()
            && ids.session_id.is_none()
            && match &event.body {
                EventBody::ToolApprovalGranted(body) | EventBody::ToolApprovalRefused(body) => {
                    body.approval.get() == approval
                }
                _ => false,
            }
    })
}

/// The kinds that say what an agent asked the human, and what the human answered.
const ASKED: [EventKind; 6] = [
    EventKind::QuestionAsked,
    EventKind::QuestionAnswered,
    EventKind::EscalationRaised,
    EventKind::ToolApprovalRequested,
    EventKind::ToolApprovalGranted,
    EventKind::ToolApprovalRefused,
];

/// Everything that waits on the human, in groups (questions, connector calls, approvals, other
/// escalations, acceptances, integrations), each by task id.
///
/// # Errors
///
/// What the store refused.
pub fn waiting(
    projections: &Projections,
    log: &EventLog,
    files: &ProjectFiles,
    team: &Team,
) -> Result<Vec<Waiting>, StoreError> {
    projections.catch_up()?;
    let board = projections.board()?;
    let history = log.read(&EventQuery {
        kinds: ASKED.to_vec(),
        ..EventQuery::default()
    })?;
    let item = |row: &TaskProjection, kind, agent_id: Option<&str>, line: String| Waiting {
        task_id: row.task_id.clone(),
        kind,
        agent_id: agent_id.map(str::to_string),
        title: row.title.clone(),
        line,
        question_id: None,
        reason: None,
        approval: None,
    };
    let mut waiting = unanswered(&board, &history, &item);
    waiting.extend(undecided(&board, &history, team, &item));
    let product_manager = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
        .map(|agent| agent.id.to_string());
    for row in board.iter().filter(|row| row.awaiting_approval) {
        let pm = product_manager.as_deref();
        let line = format!(
            "{} wrote a plan for you to approve",
            name_of(team, pm.unwrap_or("the Product Manager"))
        );
        waiting.push(item(row, WaitingKind::Approval, pm, line));
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Escalated && !row.awaiting_approval)
    {
        let reason = history
            .iter()
            .rev()
            .filter(|event| event.envelope.ids.task_id.as_ref() == Some(&row.task_id))
            .find_map(|event| match &event.body {
                EventBody::EscalationRaised(body) => Some(body.reason.to_string()),
                _ => None,
            });
        let agent = row.assignee_id.as_deref().or(product_manager.as_deref());
        let line = format!(
            "{} needs your help: {}",
            name_of(team, agent.unwrap_or("the team")),
            reason
                .as_deref()
                .map_or("it did not say why", reason_in_words)
        );
        waiting.push(Waiting {
            reason,
            ..item(row, WaitingKind::Help, agent, line)
        });
    }
    for row in board
        .iter()
        .filter(|row| row.status == TaskStatus::Verifying)
    {
        let mut contract = files
            .read_contract(&row.task_id)
            .map_err(|error| StoreError::Io {
                detail: error.to_string(),
            })?;
        contract.status = row.status;
        if !result_awaits_human(&contract) {
            continue;
        }
        // A task's result reaches the human once its reviewer passed it; an epic's is theirs.
        if contract.kind != TaskKind::Epic
            && !review_passed(&log.read(&EventQuery {
                task_id: Some(row.task_id.clone()),
                ..EventQuery::default()
            })?)
        {
            continue;
        }
        let assignee = row.assignee_id.as_deref();
        let finished = name_of(team, assignee.unwrap_or("the team"));
        let line = match row.reviewer_id.as_deref() {
            Some(reviewer) => format!(
                "{finished} finished it and {} reviewed it",
                name_of(team, reviewer)
            ),
            None => format!("{finished} finished it"),
        };
        waiting.push(item(row, WaitingKind::Acceptance, assignee, line));
    }
    if team.policy.integration == Integration::Manual {
        for row in board.iter().filter(|row| row.awaiting_integration) {
            waiting.push(item(
                row,
                WaitingKind::Integration,
                row.assignee_id.as_deref(),
                "Accepted, waiting for you to add it".to_string(),
            ));
        }
    }
    Ok(waiting)
}

/// Whether the latest `review.recorded` since the task last entered `verifying` passed.
#[must_use]
pub fn review_passed(history: &[FarikEvent]) -> bool {
    let since =
        last_move_into(history, TaskStatus::Verifying).map_or(0, |event| event.envelope.seq);
    history
        .iter()
        .rev()
        .take_while(|event| event.envelope.seq > since)
        .find_map(|event| match &event.body {
            EventBody::ReviewRecorded(body) => Some(body.passed),
            _ => None,
        })
        .unwrap_or(false)
}

/// Whether `event` is a `task.transitioned` into `status`.
#[must_use]
pub fn is_move_into(event: &FarikEvent, status: TaskStatus) -> bool {
    matches!(&event.body, EventBody::TaskTransitioned(body) if wire_status(body.to) == Some(status))
}

/// The task's last `task.transitioned` into `status`.
#[must_use]
pub fn last_move_into(history: &[FarikEvent], status: TaskStatus) -> Option<&FarikEvent> {
    history
        .iter()
        .rev()
        .find(|event| is_move_into(event, status))
}

/// A wire status as the contract's own. The two lists are one, which a test in `farik-protocol`
/// pins, so `None` is a log no Farik wrote.
fn wire_status(status: TaskStatusWire) -> Option<TaskStatus> {
    TaskStatus::from_str(&status.to_string()).ok()
}

/// Every question nobody answered, by task.
fn unanswered(
    board: &[TaskProjection],
    history: &[FarikEvent],
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Vec<Waiting> {
    let mut waiting = Vec::new();
    for row in board {
        for event in history {
            let EventBody::QuestionAsked(body) = &event.body else {
                continue;
            };
            let seq = event.envelope.seq;
            let answered = history.iter().any(|later| {
                matches!(&later.body, EventBody::QuestionAnswered(answer)
                    if answer.question_id.get() == seq)
            });
            if answered || event.envelope.ids.task_id.as_ref() != Some(&row.task_id) {
                continue;
            }
            waiting.push(Waiting {
                question_id: Some(seq),
                ..item(
                    row,
                    WaitingKind::Question,
                    Some(&body.asked_by),
                    body.question.clone(),
                )
            });
        }
    }
    waiting
}

/// Every connector call nobody allowed or refused, by task (ADR 0031).
fn undecided(
    board: &[TaskProjection],
    history: &[FarikEvent],
    team: &Team,
    item: &impl Fn(&TaskProjection, WaitingKind, Option<&str>, String) -> Waiting,
) -> Vec<Waiting> {
    let mut waiting = Vec::new();
    for row in board {
        for asked in history {
            let EventBody::ToolApprovalRequested(body) = &asked.body else {
                continue;
            };
            let approval = asked.envelope.seq;
            if asked.envelope.ids.task_id.as_ref() != Some(&row.task_id)
                || decision_on(history, approval).is_some()
            {
                continue;
            }
            let agent = asked.envelope.ids.agent_id.as_deref();
            let line = format!(
                "{} wants to use {}",
                name_of(team, agent.unwrap_or("an agent")),
                body.server.as_str()
            );
            waiting.push(Waiting {
                approval: Some(ToolAsk {
                    approval,
                    server: body.server.to_string(),
                    tool: body.tool.to_string(),
                    input: body.input.clone(),
                }),
                ..item(row, WaitingKind::ToolApproval, agent, line)
            });
        }
    }
    waiting
}

/// An agent's display name, or `id` itself when the team has no such agent.
#[must_use]
pub fn name_of(team: &Team, id: &str) -> String {
    team.agents
        .iter()
        .find(|agent| agent.id.as_str() == id)
        .map_or_else(|| id.to_string(), |agent| agent.display_name.to_string())
}

/// Why a task escalated, as the log words it, in words a person reads.
#[must_use]
pub fn reason_in_words(reason: &str) -> &'static str {
    match reason {
        "budget" => "it reached its budget",
        "sessions" => "it used all its sessions",
        "iterations" => "it used all its tries",
        "blocker_age" => "it has been stuck too long",
        "permission" => "it needs a permission it does not have",
        "risk_gate" => "the plan is high risk",
        "approval" => "a plan waits for your approval",
        "readiness_failures" => "its plan keeps failing its checks",
        "integration" => "its work could not be added to the project",
        "explicit_request" => "it asked for you",
        _ => "it did not say why",
    }
}

/// Builders for the logs the tests of what waits, what the agents do, and what moved read.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;
    use std::sync::Arc;

    use chrono::{DateTime, TimeZone, Utc};
    use farik_core::contract::validate_contract;
    use farik_core::team::{Team, validate_team};
    use farik_protocol::event::{FarikEvent, NewEvent, event_from_value};
    use serde_json::{Value, json};

    use crate::files::ProjectFiles;
    use crate::files::fixtures::TempProject;
    use crate::{EventLog, IN_MEMORY, Projections, open_event_log, open_projections};

    pub(crate) fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, hour, minute, 0)
            .single()
            .expect("a real time")
    }

    /// A team of Ada (Product Manager), Linus (Developer), and Grace (Architect), integrating by
    /// hand.
    pub(crate) fn a_team() -> Team {
        let mut wire = farik_core::team::fixtures::a_team_wire();
        wire["agents"] = json!([
            { "id": "ada", "display_name": "Ada", "role": "product_manager", "status": "active" },
            { "id": "linus", "display_name": "Linus", "role": "software_developer", "status": "active" },
            { "id": "grace", "display_name": "Grace", "role": "architect", "status": "active" },
        ]);
        validate_team(&wire).expect("the fixture is a team")
    }

    /// A project's files, log, and board, all empty but for the team.
    pub(crate) struct Board {
        pub(crate) _project: TempProject,
        pub(crate) files: ProjectFiles,
        pub(crate) log: Arc<EventLog>,
        pub(crate) projections: Projections,
    }

    impl Board {
        pub(crate) fn new(name: &str) -> Self {
            let project = TempProject::new(name);
            let files = project.files();
            files.init(&a_team()).expect(".farik/ is made");
            let log = Arc::new(
                open_event_log(Path::new(IN_MEMORY), at(9, 0)).expect("a log in memory opens"),
            );
            let projections = open_projections(Arc::clone(&log)).expect("the projections open");
            Self {
                _project: project,
                files,
                log,
                projections,
            }
        }

        /// Appends one event at `at`, about `task` and by `agent` when named, and projects it.
        #[allow(
            clippy::needless_pass_by_value,
            reason = "the tests build each body in the call"
        )]
        pub(crate) fn put(
            &self,
            when: DateTime<Utc>,
            task: Option<&str>,
            agent: Option<&str>,
            kind: &str,
            body: Value,
        ) -> FarikEvent {
            self.put_with(when, task, agent, None, kind, body)
        }

        /// `put`, in `agent`'s session `session`.
        #[allow(
            clippy::needless_pass_by_value,
            reason = "the tests build each body in the call"
        )]
        pub(crate) fn session(
            &self,
            when: DateTime<Utc>,
            task: Option<&str>,
            agent: &str,
            session: &str,
            kind: &str,
            body: Value,
        ) -> FarikEvent {
            self.put_with(when, task, Some(agent), Some(session), kind, body)
        }

        #[allow(
            clippy::needless_pass_by_value,
            reason = "the tests build each body in the call"
        )]
        fn put_with(
            &self,
            when: DateTime<Utc>,
            task: Option<&str>,
            agent: Option<&str>,
            session: Option<&str>,
            kind: &str,
            body: Value,
        ) -> FarikEvent {
            let mut wire = json!({
                "seq": 1, "recorded_at": when.to_rfc3339(),
                "team_id": "farik", "project_id": "farik",
                "kind": kind, "body": body
            });
            if let Some(task) = task {
                wire["task_id"] = json!(task);
            }
            if let Some(agent) = agent {
                wire["agent_id"] = json!(agent);
            }
            if let Some(session) = session {
                wire["session_id"] = json!(session);
            }
            let event = event_from_value(&wire).unwrap_or_else(|e| panic!("{wire}: {e:?}"));
            let appended = self
                .log
                .append(&NewEvent {
                    recorded_at: event.envelope.recorded_at,
                    ids: event.envelope.ids,
                    body: event.body,
                })
                .expect("appends");
            self.projections.apply(&appended).expect("projects");
            appended
        }

        /// Files `task`, titled `title`, in the files and the log, with `change` applied to the
        /// fixture contract.
        pub(crate) fn file(&self, task: &str, title: &str, change: impl FnOnce(&mut Value)) {
            let mut wire = farik_core::contract::fixtures::a_contract_wire();
            wire["id"] = json!(task);
            wire["title"] = json!(title);
            change(&mut wire);
            let contract = validate_contract(&wire).expect("the fixture is a contract");
            self.files
                .create_contract(&contract)
                .expect("the file is made");
            self.put(
                at(9, 0),
                Some(task),
                None,
                "task.created",
                json!({ "created_by": "human", "summary": {
                    "kind": wire.get("kind").cloned().unwrap_or(json!("task")),
                    "title": title, "status": "draft", "risk": wire["risk"]
                }}),
            );
        }

        /// Moves `task` from `from` to `to` at `when`, as `by` asked, held by `assignee` and
        /// reviewed by `reviewer` when named.
        pub(crate) fn moved(
            &self,
            when: DateTime<Utc>,
            task: &str,
            (from, to): (&str, &str),
            by: &str,
            people: (Option<&str>, Option<&str>),
        ) -> FarikEvent {
            let mut body = json!({
                "from": from, "to": to, "actor": if by == "human" { "human" } else { "assignee" },
                "requested_by": by, "gate": "none", "effects": [], "iteration": 0
            });
            if let Some(assignee) = people.0 {
                body["assignee"] = json!(assignee);
            }
            if let Some(reviewer) = people.1 {
                body["reviewer"] = json!(reviewer);
            }
            self.put(when, Some(task), None, "task.transitioned", body)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{Board, a_team, at};
    use super::{WaitingKind, waiting};

    /// A passing review of `task` by Grace.
    fn reviewed(board: &Board, task: &str) {
        board.put(
            at(9, 2),
            Some(task),
            Some("grace"),
            "review.recorded",
            json!({ "reviewer": "grace", "criteria_run": 1, "passed": true }),
        );
    }

    #[test]
    fn lists_a_result_once_its_review_passed() {
        let board = Board::new("waiting-reviewed");
        let people = (Some("linus"), Some("grace"));
        let high = |wire: &mut serde_json::Value| wire["risk"] = json!("high");
        board.file("FRK-1", "Unreviewed work", high);
        board.moved(at(9, 1), "FRK-1", ("draft", "verifying"), "linus", people);
        // A pass from before the task went back to work is not this verification's.
        board.file("FRK-2", "Reworked work", high);
        board.moved(at(9, 2), "FRK-2", ("draft", "verifying"), "linus", people);
        reviewed(&board, "FRK-2");
        board.moved(
            at(9, 3),
            "FRK-2",
            ("verifying", "in_progress"),
            "human",
            people,
        );
        board.moved(
            at(9, 4),
            "FRK-2",
            ("in_progress", "verifying"),
            "linus",
            people,
        );
        // An epic's result is the human's to review, so it waits on no reviewer.
        board.file("FRK-3", "An epic", |wire| wire["kind"] = json!("epic"));
        board.moved(
            at(9, 5),
            "FRK-3",
            ("draft", "verifying"),
            "linus",
            (Some("linus"), None),
        );

        let listed = waiting(&board.projections, &board.log, &board.files, &a_team())
            .expect("the store reads");
        let seen: Vec<(&str, &str)> = listed
            .iter()
            .map(|item| (item.task_id.as_str(), item.line.as_str()))
            .collect();
        assert_eq!(seen, vec![("FRK-3", "Linus finished it")]);

        reviewed(&board, "FRK-1");
        let listed = waiting(&board.projections, &board.log, &board.files, &a_team())
            .expect("the store reads");
        assert_eq!(listed[0].line, "Linus finished it and Grace reviewed it");
    }

    #[test]
    fn lists_what_waits_on_the_human() {
        let board = Board::new("waiting-five");
        let people = (Some("linus"), Some("grace"));
        board.file("FRK-1", "A plan", |_| {});
        board.moved(
            at(9, 1),
            "FRK-1",
            ("draft", "escalated"),
            "ada",
            (None, None),
        );
        board.put(
            at(9, 1),
            Some("FRK-1"),
            None,
            "escalation.raised",
            json!({ "reason": "approval", "detail": "waits" }),
        );
        board.file("FRK-2", "A result", |wire| {
            wire["exit_criteria"][0]["verification"] =
                json!({ "method": "human", "question": "Does it look right?" });
        });
        board.moved(at(9, 2), "FRK-2", ("draft", "verifying"), "linus", people);
        reviewed(&board, "FRK-2");
        board.file("FRK-3", "A question", |_| {});
        let asked = board.put(
            at(9, 3),
            Some("FRK-3"),
            Some("linus"),
            "question.asked",
            json!({ "question": "Which colour should the button be?", "asked_by": "linus" }),
        );
        board.file("FRK-4", "Stuck work", |_| {});
        board.moved(at(9, 4), "FRK-4", ("draft", "escalated"), "linus", people);
        board.put(
            at(9, 4),
            Some("FRK-4"),
            None,
            "escalation.raised",
            json!({ "reason": "iterations", "detail": "three tries" }),
        );
        board.file("FRK-5", "Done work", |_| {});
        board.moved(at(9, 5), "FRK-5", ("draft", "accepted"), "linus", people);
        // A low-risk result with no human criterion waits on nobody.
        board.file("FRK-6", "Plain work", |_| {});
        board.moved(at(9, 6), "FRK-6", ("draft", "verifying"), "linus", people);

        let listed = waiting(&board.projections, &board.log, &board.files, &a_team())
            .expect("the store reads");
        let seen: Vec<(&str, WaitingKind, Option<&str>, &str, &str)> = listed
            .iter()
            .map(|item| {
                (
                    item.task_id.as_str(),
                    item.kind,
                    item.agent_id.as_deref(),
                    item.title.as_str(),
                    item.line.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            vec![
                (
                    "FRK-3",
                    WaitingKind::Question,
                    Some("linus"),
                    "A question",
                    "Which colour should the button be?"
                ),
                (
                    "FRK-1",
                    WaitingKind::Approval,
                    Some("ada"),
                    "A plan",
                    "Ada wrote a plan for you to approve"
                ),
                (
                    "FRK-4",
                    WaitingKind::Help,
                    Some("linus"),
                    "Stuck work",
                    "Linus needs your help: it used all its tries"
                ),
                (
                    "FRK-2",
                    WaitingKind::Acceptance,
                    Some("linus"),
                    "A result",
                    "Linus finished it and Grace reviewed it"
                ),
                (
                    "FRK-5",
                    WaitingKind::Integration,
                    Some("linus"),
                    "Done work",
                    "Accepted, waiting for you to add it"
                ),
            ]
        );
        assert_eq!(listed[0].question_id, Some(asked.envelope.seq));
        assert_eq!(listed[2].reason.as_deref(), Some("iterations"));
    }
}
