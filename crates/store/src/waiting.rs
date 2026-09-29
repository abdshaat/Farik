//! What waits on the human (`docs/SPEC.md` 5.7 and 5.16): the questions nobody answered, the
//! plans awaiting approval, the escalations, the results waiting on the human's acceptance, and
//! the tasks waiting to be integrated by hand. One list, which the command line and the browser
//! both read.

use farik_core::contract::{Role, TaskId, TaskStatus};
use farik_core::governor::done::result_awaits_human;
use farik_core::team::{Integration, Team};
use farik_protocol::event::{EventBody, EventKind, FarikEvent};

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
}

/// Everything that waits on the human, in groups (questions, approvals, other escalations,
/// acceptances, integrations), each by task id.
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
        kinds: vec![
            EventKind::QuestionAsked,
            EventKind::QuestionAnswered,
            EventKind::EscalationRaised,
        ],
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
    };
    let mut waiting = unanswered(&board, &history, &item);
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
