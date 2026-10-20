//! What the log says about autopilot (ADR 0082): whether it is on, what the team flagged and has
//! not been moved since, the questions it asked itself, how much of its limits it spent on a task,
//! and what it did on its own. As `merging_allowed` is (ADR 0073), the switch is this computer's
//! log alone, so no agent, clone or hand edit turns it on.

use std::collections::{BTreeMap, BTreeSet};

use catervas_core::autopilot::{DONE_DAYS, DecideChoice, QuestionAbout, Spent};
use catervas_core::contract::{Role, TaskId};
use catervas_core::team::Team;
use catervas_protocol::event::{CatervasEvent, EventBody, EventKind};
use catervas_protocol::generated::event::{
    DataPipelineDecidedBy, SellerMessageSentBodySentBy, TeamDecidedBodyChoice,
    TeamQuestionAskedBodyAbout,
};
use chrono::{DateTime, Duration, Utc};

use crate::merging::merging_allowed;
use crate::waiting::name_of;
use crate::{EventLog, EventQuery, StoreError};

/// The most acts "Done on its own" lists.
const MOST_ACTS_LISTED: usize = 100;

/// How many characters of an answer a line of "Done on its own" keeps as its reason.
const ANSWER_REASON_CHARS: usize = 200;

/// Whether autopilot is on: the newest `autopilot.changed` in this project's log says so.
///
/// # Errors
///
/// What the log refused.
pub fn autopilot_on(log: &EventLog) -> Result<bool, StoreError> {
    let newest = log.read(&EventQuery {
        kinds: vec![EventKind::AutopilotChanged],
        limit: Some(1),
        newest_first: true,
        ..EventQuery::default()
    })?;
    Ok(newest
        .first()
        .is_some_and(|event| matches!(&event.body, EventBody::AutopilotChanged(body) if body.on)))
}

/// Whether Catervas merges its pull requests on its own: the owner permitted it (ADR 0073), or
/// autopilot is on (ADR 0082 decision 7).
///
/// # Errors
///
/// What the log refused.
pub fn merges_on_its_own(log: &EventLog, team: &Team) -> Result<bool, StoreError> {
    Ok(merging_allowed(log, team)? || autopilot_on(log)?)
}

/// Whether the team decides what waits on the owner: autopilot is on, and the team has an active
/// Product Manager or is one agent alone. With neither, nothing changes and the owner decides.
///
/// # Errors
///
/// What the log refused.
pub fn team_decides(log: &EventLog, team: &Team) -> Result<bool, StoreError> {
    Ok((team.has_active(Role::ProductManager) || team.is_solo()) && autopilot_on(log)?)
}

/// Something only the owner can give, which the team flagged and went on without (decision 4).
/// Exactly one of `task_id` and `folder_change` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    /// The flagged task.
    pub task_id: Option<TaskId>,
    /// The flagged folder change, which has no task.
    pub folder_change: Option<u64>,
    /// What the owner has to give.
    pub needs: String,
    /// The agent that flagged it, or `catervas`.
    pub flagged_by: String,
    /// When it was flagged.
    pub at: DateTime<Utc>,
}

fn read(log: &EventLog, kinds: Vec<EventKind>) -> Result<Vec<CatervasEvent>, StoreError> {
    log.read(&EventQuery {
        kinds,
        ..EventQuery::default()
    })
}

/// The flags that still stand, oldest first. A task's flag stands until the task's next
/// `task.transitioned`, `escalation.resolved`, `task.integrated`, `human.accepted`, or
/// `team.decided` other than `flag`; a folder change's, until its next `folder_change.opened` or
/// `folder_change.integrated`, or a later `team.decided` of `retry_folder_change` for it.
///
/// # Errors
///
/// What the log refused.
pub fn flags(log: &EventLog) -> Result<Vec<Flag>, StoreError> {
    let events = read(
        log,
        vec![
            EventKind::TaskFlagged,
            EventKind::TaskTransitioned,
            EventKind::EscalationResolved,
            EventKind::TaskIntegrated,
            EventKind::HumanAccepted,
            EventKind::TeamDecided,
            EventKind::FolderChangeOpened,
            EventKind::FolderChangeIntegrated,
        ],
    )?;
    let mut tasks: BTreeMap<String, (u64, Flag)> = BTreeMap::new();
    let mut changes: BTreeMap<u64, (u64, Flag)> = BTreeMap::new();
    for event in &events {
        let seq = event.envelope.seq;
        let task = event.envelope.ids.task_id.as_ref();
        match &event.body {
            EventBody::TaskFlagged(body) => {
                if let Some(task) = task {
                    let flag = Flag {
                        task_id: Some(task.clone()),
                        folder_change: None,
                        needs: body.needs.to_string(),
                        flagged_by: body.flagged_by.clone(),
                        at: event.envelope.recorded_at,
                    };
                    tasks.insert(task.to_string(), (seq, flag));
                }
            }
            EventBody::TaskTransitioned(_)
            | EventBody::EscalationResolved(_)
            | EventBody::TaskIntegrated(_)
            | EventBody::HumanAccepted(_) => {
                if let Some(task) = task {
                    tasks.remove(task.as_str());
                }
            }
            EventBody::TeamDecided(body) => {
                let flagging = body.choice == TeamDecidedBodyChoice::Flag;
                if let Some(change) = &body.folder_change {
                    let change = change.get();
                    if flagging {
                        let flag = Flag {
                            task_id: None,
                            folder_change: Some(change),
                            needs: body.needs.as_ref().map_or_else(
                                || body.reason.to_string(),
                                |needs| needs.as_str().to_string(),
                            ),
                            flagged_by: body.decided_by.clone(),
                            at: event.envelope.recorded_at,
                        };
                        changes.insert(change, (seq, flag));
                    } else if body.choice == TeamDecidedBodyChoice::RetryFolderChange {
                        changes.remove(&change);
                    }
                } else if let (Some(task), false) = (task, flagging) {
                    tasks.remove(task.as_str());
                }
            }
            EventBody::FolderChangeOpened(body) => {
                changes.remove(&body.change.get());
            }
            EventBody::FolderChangeIntegrated(body) => {
                changes.remove(&body.change.get());
            }
            _ => {}
        }
    }
    let mut standing: Vec<(u64, Flag)> = tasks.into_values().chain(changes.into_values()).collect();
    standing.sort_by_key(|(seq, _)| *seq);
    Ok(standing.into_iter().map(|(_, flag)| flag).collect())
}

/// A question the team asked itself that nobody has answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamQuestion {
    /// The question's id, the seq of its `team.question_asked`.
    pub seq: u64,
    /// The task it holds.
    pub task_id: Option<TaskId>,
    /// What is asked.
    pub question: String,
    /// The agent that asked.
    pub asked_by: String,
    /// Whom it is for.
    pub about: QuestionAbout,
    /// When it was asked.
    pub at: DateTime<Utc>,
}

fn about_of(about: TeamQuestionAskedBodyAbout) -> QuestionAbout {
    match about {
        TeamQuestionAskedBodyAbout::Product => QuestionAbout::Product,
        TeamQuestionAskedBodyAbout::Technical => QuestionAbout::Technical,
    }
}

/// The team's questions with no answer yet, from the team or from the owner, oldest first.
///
/// # Errors
///
/// What the log refused.
pub fn open_team_questions(log: &EventLog) -> Result<Vec<TeamQuestion>, StoreError> {
    let events = read(
        log,
        vec![
            EventKind::TeamQuestionAsked,
            EventKind::TeamQuestionAnswered,
            EventKind::QuestionAnswered,
        ],
    )?;
    let answered: BTreeSet<u64> = events
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::TeamQuestionAnswered(body) => Some(body.question_seq.get()),
            EventBody::QuestionAnswered(body) => Some(body.question_id.get()),
            _ => None,
        })
        .collect();
    Ok(events
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::TeamQuestionAsked(body) if !answered.contains(&event.envelope.seq) => {
                Some(TeamQuestion {
                    seq: event.envelope.seq,
                    task_id: event.envelope.ids.task_id.clone(),
                    question: body.question.to_string(),
                    asked_by: body.asked_by.clone(),
                    about: about_of(body.about),
                    at: event.envelope.recorded_at,
                })
            }
            _ => None,
        })
        .collect())
}

/// What the team has used of its limits on `task_id`: the `more_tries` and `more_room` it chose,
/// and the integrations it reopened.
///
/// # Errors
///
/// What the log refused.
pub fn team_spent(log: &EventLog, task_id: &TaskId) -> Result<Spent, StoreError> {
    let decisions = log.read(&EventQuery {
        task_id: Some(task_id.clone()),
        kinds: vec![EventKind::TeamDecided],
        ..EventQuery::default()
    })?;
    let mut spent = Spent::default();
    for event in &decisions {
        if let EventBody::TeamDecided(body) = &event.body {
            match body.choice {
                TeamDecidedBodyChoice::MoreTries | TeamDecidedBodyChoice::MoreRoom => {
                    spent.grants += 1;
                }
                TeamDecidedBodyChoice::ReopenIntegration => spent.reopens += 1,
                _ => {}
            }
        }
    }
    Ok(spent)
}

/// How many times the team tried folder change `change` again.
///
/// # Errors
///
/// What the log refused.
pub fn folder_change_spent(log: &EventLog, change: u64) -> Result<Spent, StoreError> {
    let decisions = read(log, vec![EventKind::TeamDecided])?;
    let retries = decisions
        .iter()
        .filter(|event| {
            matches!(&event.body, EventBody::TeamDecided(body)
                if body.choice == TeamDecidedBodyChoice::RetryFolderChange
                    && body.folder_change.as_ref().is_some_and(|number| number.get() == change))
        })
        .count();
    Ok(Spent {
        grants: 0,
        reopens: u32::try_from(retries).unwrap_or(u32::MAX),
    })
}

/// One thing the team did on its own, for Today's "Done on its own" (decision 10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnItsOwn {
    /// When it happened.
    pub at: DateTime<Utc>,
    /// What was done, in a sentence.
    pub line: String,
    /// The agent that did it, when one did.
    pub agent_id: Option<String>,
    /// The task it was about.
    pub task_id: Option<TaskId>,
    /// The agent's reason, when it gave one.
    pub reason: Option<String>,
}

/// A choice in words, to follow "chose".
fn choice_in_words(choice: &str) -> &str {
    match DecideChoice::from_wire(choice) {
        Some(DecideChoice::MoreTries) => "two more tries",
        Some(DecideChoice::MoreRoom) => "more room",
        Some(DecideChoice::BackToPlan) => "going back to the plan",
        Some(DecideChoice::TryReadinessAgain) => "a new readiness check",
        Some(DecideChoice::ReopenIntegration) => "integrating again",
        Some(DecideChoice::AnswerBlock) => "answering the block",
        Some(DecideChoice::Cancel) => "cancelling the task",
        Some(DecideChoice::ApprovePlan) => "approving the plan",
        Some(DecideChoice::AcceptResult) => "accepting the result",
        Some(DecideChoice::CarryOn) => "carrying on",
        Some(DecideChoice::RetryFolderChange) => "trying the folder change again",
        Some(DecideChoice::Flag) => "leaving it for the owner",
        None => choice,
    }
}

fn cut(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

fn non_empty(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_string())
}

/// What the team did on its own in the last `DONE_DAYS` days, newest first, at most 100: the
/// questions it answered, the decisions it took, the paid calls it let run, the marketing plans it
/// approved, the data sources its Product Manager decided, the seller messages it sent, and the
/// pull requests merged while autopilot was on.
///
/// # Errors
///
/// What the log refused.
#[allow(clippy::too_many_lines, reason = "one arm per kind of act")]
pub fn done_on_its_own(
    log: &EventLog,
    team: &Team,
    now: DateTime<Utc>,
) -> Result<Vec<OnItsOwn>, StoreError> {
    let events = read(
        log,
        vec![
            EventKind::AutopilotChanged,
            EventKind::TeamQuestionAsked,
            EventKind::TeamQuestionAnswered,
            EventKind::TeamDecided,
            EventKind::ToolApproved,
            EventKind::MarketingPlanApproved,
            EventKind::DataPipelineRequested,
            EventKind::DataPipelineApproved,
            EventKind::DataPipelineDeclined,
            EventKind::SellerMessageDrafted,
            EventKind::SellerMessageSent,
            EventKind::PullRequestMerged,
        ],
    )?;
    let since = now - Duration::days(DONE_DAYS);
    let mut on = false;
    let mut askers: BTreeMap<u64, String> = BTreeMap::new();
    let mut sources: BTreeMap<u64, String> = BTreeMap::new();
    let mut messages: BTreeMap<u64, (String, String)> = BTreeMap::new();
    let mut acts: Vec<OnItsOwn> = Vec::new();
    for event in &events {
        let ids = &event.envelope.ids;
        let act = |line: String, agent_id: Option<String>, reason: Option<String>| OnItsOwn {
            at: event.envelope.recorded_at,
            line,
            agent_id,
            task_id: ids.task_id.clone(),
            reason,
        };
        let named = |id: &Option<String>, otherwise: &str| {
            id.as_deref()
                .map_or_else(|| otherwise.to_string(), |id| name_of(team, id))
        };
        match &event.body {
            EventBody::AutopilotChanged(body) => on = body.on,
            EventBody::TeamQuestionAsked(body) => {
                askers.insert(event.envelope.seq, body.asked_by.clone());
            }
            EventBody::DataPipelineRequested(body) => {
                sources.insert(event.envelope.seq, body.name.to_string());
            }
            EventBody::SellerMessageDrafted(body) => {
                messages.insert(
                    body.message.get(),
                    (body.seller.to_string(), body.subject.to_string()),
                );
            }
            EventBody::TeamQuestionAnswered(body) => {
                let asker = askers
                    .get(&body.question_seq.get())
                    .map_or_else(|| "a teammate".to_string(), |asker| name_of(team, asker));
                acts.push(act(
                    format!(
                        "{} answered {asker}'s question",
                        name_of(team, &body.answered_by)
                    ),
                    Some(body.answered_by.clone()),
                    non_empty(&cut(&body.answer, ANSWER_REASON_CHARS)),
                ));
            }
            EventBody::TeamDecided(body) => {
                let about = match (&ids.task_id, &body.folder_change) {
                    (Some(task), _) => task.to_string(),
                    (None, Some(change)) => format!("folder change {}", change.get()),
                    (None, None) => "the team".to_string(),
                };
                acts.push(act(
                    format!(
                        "{} chose {} for {about}",
                        name_of(team, &body.decided_by),
                        choice_in_words(&body.choice.to_string())
                    ),
                    Some(body.decided_by.clone()),
                    Some(body.reason.to_string()),
                ));
            }
            EventBody::ToolApproved(body) => acts.push(act(
                format!(
                    "{} used {} of {}",
                    named(&ids.agent_id, "An agent"),
                    body.tool.as_str(),
                    body.server.as_str()
                ),
                ids.agent_id.clone(),
                None,
            )),
            EventBody::MarketingPlanApproved(body) if body.by.is_some() => acts.push(act(
                format!("Catervas approved marketing plan {}", *body.plan),
                None,
                non_empty(&body.note),
            )),
            EventBody::DataPipelineApproved(_) | EventBody::DataPipelineDeclined(_)
                if ids.session_id.is_some() =>
            {
                let (pipeline, by, reason, verb) = match &event.body {
                    EventBody::DataPipelineApproved(body) => (
                        body.pipeline.get(),
                        body.by,
                        body.reason.as_str(),
                        "approved",
                    ),
                    EventBody::DataPipelineDeclined(body) => (
                        body.pipeline.get(),
                        body.by,
                        body.reason.as_str(),
                        "declined",
                    ),
                    _ => continue,
                };
                if by == DataPipelineDecidedBy::ProductManager {
                    let source = sources
                        .get(&pipeline)
                        .map_or_else(|| format!("request {pipeline}"), Clone::clone);
                    acts.push(act(
                        format!(
                            "{} {verb} the data source {source}",
                            named(&ids.agent_id, "The Product Manager")
                        ),
                        ids.agent_id.clone(),
                        non_empty(reason),
                    ));
                }
            }
            EventBody::SellerMessageSent(body)
                if body.sent_by == Some(SellerMessageSentBodySentBy::Autopilot) =>
            {
                let (seller, subject) = messages
                    .get(&body.message.get())
                    .cloned()
                    .unwrap_or_else(|| ("a seller".to_string(), "a message".to_string()));
                acts.push(act(
                    format!("Catervas sent {seller} \u{201c}{subject}\u{201d}"),
                    None,
                    None,
                ));
            }
            EventBody::PullRequestMerged(body) if on => {
                acts.push(act(
                    format!("Catervas merged {}", body.url.as_str()),
                    None,
                    None,
                ));
            }
            _ => {}
        }
    }
    acts.retain(|act| act.at >= since);
    acts.reverse();
    acts.truncate(MOST_ACTS_LISTED);
    Ok(acts)
}

#[cfg(test)]
mod tests {
    use catervas_core::autopilot::{QuestionAbout, Spent};
    use catervas_core::generated::team::AgentStatus;
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{Value, json};

    use super::{
        Flag, TeamQuestion, autopilot_on, done_on_its_own, flags, folder_change_spent,
        merges_on_its_own, open_team_questions, team_decides, team_spent,
    };
    use crate::folder_docs::folder_docs;
    use crate::waiting::fixtures::{Board, a_team, at};

    fn on(board: &Board, when: DateTime<Utc>, on: bool) {
        board.put(when, None, None, "autopilot.changed", json!({ "on": on }));
    }

    fn moved(to: &str) -> Value {
        json!({ "from": "in_progress", "to": to, "actor": "assignee", "requested_by": "linus",
                "gate": "none", "effects": [], "iteration": 0 })
    }

    fn decided(choice: &str, change: Option<u64>) -> Value {
        let mut body = json!({ "choice": choice, "reason": "It is the cheapest way.",
                               "decided_by": "ada" });
        if choice == "flag" {
            body["needs"] = json!("A key from the owner.");
        }
        if let Some(change) = change {
            body["folder_change"] = json!(change);
        }
        body
    }

    #[test]
    fn autopilot_is_on_only_by_this_computers_log() {
        let board = Board::new("autopilot-on");
        assert!(
            !autopilot_on(&board.log).expect("the log reads"),
            "an empty log is off"
        );
        on(&board, at(9, 1), true);
        assert!(autopilot_on(&board.log).expect("the log reads"));
        on(&board, at(9, 2), false);
        assert!(!autopilot_on(&board.log).expect("the log reads"));
        on(&board, at(9, 3), true);

        // Under autopilot a team that never set `merge_when_checks_pass` merges on its own.
        let team = a_team();
        assert!(!team.merge_when_checks_pass());
        assert!(merges_on_its_own(&board.log, &team).expect("the log reads"));
        // The team decides while it has an active Product Manager, and not without one.
        assert!(team_decides(&board.log, &team).expect("the log reads"));
        // The file cannot lack an active Product Manager, but one can be paused after it loads.
        let mut without = a_team();
        without.agents[0].status = AgentStatus::Paused;
        assert!(!without.is_solo());
        assert!(!team_decides(&board.log, &without).expect("the log reads"));
        let off = Board::new("autopilot-off");
        assert!(!merges_on_its_own(&off.log, &team).expect("the log reads"));
        assert!(!team_decides(&off.log, &team).expect("the log reads"));
    }

    #[test]
    #[allow(clippy::too_many_lines, reason = "each of the log's answers, in order")]
    fn a_flag_stands_until_the_task_moves_and_acts_are_listed_for_seven_days() {
        let board = Board::new("autopilot-flags");
        let team = a_team();
        let flag = |when, task: &str| {
            board.put(
                when,
                Some(task),
                Some("linus"),
                "task.flagged",
                json!({ "needs": "A sign-in only the owner can give.", "flagged_by": "linus" }),
            );
        };
        flag(at(9, 1), "CTV-1");
        flag(at(9, 2), "CTV-2");
        let standing = flags(&board.log).expect("the log reads");
        assert_eq!(
            standing,
            [
                Flag {
                    task_id: Some("CTV-1".parse().expect("a task id")),
                    folder_change: None,
                    needs: "A sign-in only the owner can give.".to_string(),
                    flagged_by: "linus".to_string(),
                    at: at(9, 1),
                },
                Flag {
                    task_id: Some("CTV-2".parse().expect("a task id")),
                    folder_change: None,
                    needs: "A sign-in only the owner can give.".to_string(),
                    flagged_by: "linus".to_string(),
                    at: at(9, 2),
                },
            ]
        );
        board.put(
            at(9, 3),
            Some("CTV-1"),
            Some("linus"),
            "task.transitioned",
            moved("verifying"),
        );
        let standing = flags(&board.log).expect("the log reads");
        assert_eq!(standing.len(), 1);
        assert_eq!(
            standing[0].task_id.as_ref().map(|id| id.as_str()),
            Some("CTV-2")
        );
        // Another flag of the same task does not end it; a decision other than a flag does.
        board.put(
            at(9, 4),
            Some("CTV-2"),
            Some("ada"),
            "team.decided",
            decided("flag", None),
        );
        assert_eq!(flags(&board.log).expect("the log reads").len(), 1);
        board.put(
            at(9, 5),
            Some("CTV-2"),
            Some("ada"),
            "team.decided",
            decided("cancel", None),
        );
        assert!(flags(&board.log).expect("the log reads").is_empty());

        // A folder change has no task: its flag is a decision, and it stands until the change is
        // retried, which also leaves the change not escalated.
        board.put(
            at(9, 6),
            None,
            None,
            "folder_doc.written",
            json!({ "path": "docs/catervas/delivery/cadence.md", "written_by": "sm", "change": 4,
                    "sha": "b2" }),
        );
        board.put(
            at(9, 7),
            None,
            None,
            "folder_change.escalated",
            json!({ "change": 4, "detail": "conflict" }),
        );
        board.put(
            at(9, 8),
            None,
            Some("ada"),
            "team.decided",
            decided("flag", Some(4)),
        );
        let standing = flags(&board.log).expect("the log reads");
        assert_eq!(standing.len(), 1);
        assert_eq!(standing[0].folder_change, Some(4));
        assert_eq!(standing[0].task_id, None);
        assert!(folder_docs(&board.log).expect("folds").changes[0].escalated);
        board.put(
            at(9, 9),
            None,
            Some("ada"),
            "team.decided",
            decided("retry_folder_change", Some(4)),
        );
        assert!(flags(&board.log).expect("the log reads").is_empty());
        assert!(!folder_docs(&board.log).expect("folds").changes[0].escalated);
        assert_eq!(
            folder_change_spent(&board.log, 4).expect("the log reads"),
            Spent {
                grants: 0,
                reopens: 1
            }
        );

        // What the team spent on a task: two more tries, a reopened integration, another's.
        for (minute, choice) in [
            (10, "more_tries"),
            (11, "more_tries"),
            (12, "reopen_integration"),
        ] {
            board.put(
                at(9, minute),
                Some("CTV-3"),
                Some("ada"),
                "team.decided",
                decided(choice, None),
            );
        }
        board.put(
            at(9, 13),
            Some("CTV-4"),
            Some("ada"),
            "team.decided",
            decided("more_room", None),
        );
        assert_eq!(
            team_spent(&board.log, &"CTV-3".parse().expect("a task id")).expect("the log reads"),
            Spent {
                grants: 2,
                reopens: 1
            }
        );

        // Acts: a decision six days old is listed with its reason, one eight days old is not, and
        // a merge counts only from the time autopilot was turned on.
        let day = |month, day, hour| {
            Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0)
                .single()
                .expect("a real time")
        };
        let merged = json!({ "url": "https://example.test/pull/7", "method": "merge" });
        let acts = Board::new("autopilot-acts");
        acts.put(
            day(9, 20, 9),
            Some("CTV-1"),
            Some("ada"),
            "team.decided",
            decided("cancel", None),
        );
        acts.put(
            day(9, 22, 9),
            Some("CTV-2"),
            Some("ada"),
            "team.decided",
            decided("back_to_plan", None),
        );
        acts.put(
            day(9, 25, 8),
            Some("CTV-5"),
            None,
            "pull_request.merged",
            merged.clone(),
        );
        on(&acts, day(9, 25, 9), true);
        acts.put(
            day(9, 26, 9),
            Some("CTV-6"),
            None,
            "pull_request.merged",
            merged,
        );
        let done = done_on_its_own(&acts.log, &team, day(9, 28, 12)).expect("the log reads");
        let lines: Vec<(&str, Option<&str>)> = done
            .iter()
            .map(|act| (act.line.as_str(), act.reason.as_deref()))
            .collect();
        assert_eq!(
            lines,
            [
                ("Catervas merged https://example.test/pull/7", None),
                (
                    "Ada chose going back to the plan for CTV-2",
                    Some("It is the cheapest way.")
                ),
            ]
        );
        assert!(done[0].at > done[1].at, "newest first");
    }

    #[test]
    fn lists_a_team_question_until_the_team_or_the_owner_answers_it() {
        let board = Board::new("autopilot-questions");
        let ask = |minute, task: &str| {
            board
                .put(
                    at(9, minute),
                    Some(task),
                    Some("linus"),
                    "team.question_asked",
                    json!({ "question": "Which database?", "asked_by": "linus", "about": "technical" }),
                )
                .envelope
                .seq
        };
        let (first, second, third) = (ask(1, "CTV-1"), ask(2, "CTV-2"), ask(3, "CTV-3"));
        board.put(
            at(9, 4),
            Some("CTV-1"),
            Some("grace"),
            "team.question_answered",
            json!({ "question_seq": first, "answer": "SQLite.", "answered_by": "grace" }),
        );
        board.put(
            at(9, 5),
            Some("CTV-2"),
            None,
            "question.answered",
            json!({ "question_id": second, "answer": "Yes.", "answered_by": "human" }),
        );
        assert_eq!(
            open_team_questions(&board.log).expect("the log reads"),
            [TeamQuestion {
                seq: third,
                task_id: Some("CTV-3".parse().expect("a task id")),
                question: "Which database?".to_string(),
                asked_by: "linus".to_string(),
                about: QuestionAbout::Technical,
                at: at(9, 3),
            }]
        );
    }
}
