//! Picking up a run that was killed (`docs/SPEC.md` 5.15).

use std::collections::BTreeSet;

use farik_core::contract::TaskStatus;
use farik_core::pricing::Usage;
use farik_protocol::event::{EventBody, EventIds, EventKind, SessionStartedBodyPurpose};
use farik_store::EventQuery;

use super::integrate::remove_workspace;
use super::{Orchestrator, OrchestratorError, RecoveryReport};
use crate::cost::{CostSource, record_session_cost};
use crate::session::{EndReason, SessionPurpose};
use crate::sessions::record_session_ended;

/// What a session a stopped run left open is recorded as ending with.
const INTERRUPTED: &str = "interrupted: farik stopped before the session ended";

/// `Orchestrator::recover`.
pub(super) fn recover(orchestrator: &Orchestrator) -> Result<RecoveryReport, OrchestratorError> {
    let tools = &orchestrator.deps.tools;
    let clock = &*tools.clock;
    let sessions = tools.log.read(&EventQuery {
        kinds: vec![
            EventKind::SessionStarted,
            EventKind::SessionEnded,
            EventKind::CostRecorded,
        ],
        ..EventQuery::default()
    })?;
    let session_of = |kind: EventKind| -> BTreeSet<String> {
        sessions
            .iter()
            .filter(|event| event.body.kind() == kind)
            .filter_map(|event| event.envelope.ids.session_id.clone())
            .collect()
    };
    let ended = session_of(EventKind::SessionEnded);
    let costed = session_of(EventKind::CostRecorded);
    let mut report = RecoveryReport {
        sessions_interrupted: 0,
        worktrees_removed: 0,
        tasks_resumed: 0,
    };
    let mut prices = None;
    for event in &sessions {
        let (EventBody::SessionStarted(body), Some(session_id)) =
            (&event.body, &event.envelope.ids.session_id)
        else {
            continue;
        };
        if ended.contains(session_id) {
            continue;
        }
        let ids = EventIds {
            task_id: event.envelope.ids.task_id.clone(),
            agent_id: event.envelope.ids.agent_id.clone(),
            session_id: Some(session_id.clone()),
            ..tools.ids.clone()
        };
        record_session_ended(
            &tools.log,
            session_id,
            EndReason::Aborted,
            INTERRUPTED,
            &ids,
            clock,
        )?;
        if !costed.contains(session_id) {
            if prices.is_none() {
                prices = Some(tools.files.effective_prices()?);
            }
            let model = body.model.to_string();
            record_session_cost(
                &tools.log,
                &tools.projections,
                &CostSource {
                    ids,
                    purpose: purpose(body.purpose),
                    model_id: &model,
                },
                &Usage::default(),
                prices.as_ref().expect("the prices were read just above"),
                clock,
            )?;
        }
        report.sessions_interrupted += 1;
    }
    for row in tools.projections.board()? {
        match row.status {
            TaskStatus::Accepted | TaskStatus::Cancelled => {
                if remove_workspace(orchestrator, &row.task_id)? {
                    report.worktrees_removed += 1;
                }
            }
            TaskStatus::InProgress => report.tasks_resumed += 1,
            _ => {}
        }
    }
    Ok(report)
}

/// A session's purpose as the log spells it, as the runtime's own.
fn purpose(wire: SessionStartedBodyPurpose) -> SessionPurpose {
    match wire {
        SessionStartedBodyPurpose::Triage => SessionPurpose::Triage,
        SessionStartedBodyPurpose::Refine => SessionPurpose::Refine,
        SessionStartedBodyPurpose::Plan => SessionPurpose::Plan,
        SessionStartedBodyPurpose::Implement => SessionPurpose::Implement,
        SessionStartedBodyPurpose::Verify => SessionPurpose::Verify,
        SessionStartedBodyPurpose::Ceremony => SessionPurpose::Ceremony,
        SessionStartedBodyPurpose::Conversation => SessionPurpose::Conversation,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use farik_protocol::event::{
        CostRecordedBodyPurpose, EventBody, EventKind, FarikEvent, SessionEndedBodyReason,
    };
    use farik_store::git::fixtures::git_output_in;
    use serde_json::json;

    use crate::orchestrator::RecoveryReport;
    use crate::orchestrator::fixtures::{CountingSandboxFactory, Harness};
    use crate::recorded::fixtures::implement_stops_early;

    /// A run killed with `dev-a`'s session on FRK-2 open, FRK-1 accepted with its worktree left,
    /// and FRK-2 in progress with a commit on its branch; answers that commit.
    fn a_killed_run(name: &str) -> (Harness, String) {
        let harness = Harness::new(name, |wire| {
            wire["policy"]["wip_limit_per_agent"] = serde_json::json!(2);
        });
        harness.accepted_with_worktree("FRK-1");
        harness.in_progress("FRK-2", "dev-a", "dev-b");
        let worktree = harness.worktree("FRK-2");
        std::fs::write(worktree.join("done.txt"), "").expect("written");
        let git = &harness.project.deps.git;
        git.commit(&worktree, "Add done.txt", &["done.txt".to_string()])
            .expect("committed");
        harness.started_session("FRK-2", "dev-a", "session-killed", "implement");
        let sha = git_output_in(&harness.project.repo.path, &["rev-parse", "farik/FRK-2"]);
        (harness, sha)
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn recovers_an_interrupted_run() {
        let (harness, _) = a_killed_run("recover-run");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.recover().expect("recovery runs");

        assert_eq!(
            report,
            RecoveryReport {
                sessions_interrupted: 1,
                worktrees_removed: 1,
                tasks_resumed: 1,
            }
        );
        let ended = harness.events(&[EventKind::SessionEnded]);
        assert_eq!(ended.len(), 1, "{ended:?}");
        assert_eq!(
            ended[0].envelope.ids.session_id.as_deref(),
            Some("session-killed")
        );
        match &ended[0].body {
            EventBody::SessionEnded(body) => {
                assert_eq!(body.reason, SessionEndedBodyReason::Aborted);
                assert!(body.detail.starts_with("interrupted"), "{}", body.detail);
            }
            other => panic!("a session.ended, got {other:?}"),
        }
        let costs = harness.events(&[EventKind::CostRecorded]);
        assert_eq!(costs.len(), 1, "{costs:?}");
        assert_eq!(
            costs[0].envelope.ids.session_id.as_deref(),
            Some("session-killed")
        );
        match &costs[0].body {
            EventBody::CostRecorded(body) => {
                assert_eq!(body.usage.input_tokens, 0);
                assert_eq!(body.usage.output_tokens, 0);
            }
            other => panic!("a cost.recorded, got {other:?}"),
        }
        assert!(!harness.worktree("FRK-1").exists());
        assert!(harness.worktree("FRK-2").exists());
    }

    /// The events of `kind` about `session`.
    fn of_session(harness: &Harness, kind: EventKind, session: &str) -> Vec<FarikEvent> {
        harness
            .events(&[kind])
            .into_iter()
            .filter(|event| event.envelope.ids.session_id.as_deref() == Some(session))
            .collect()
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_each_interrupted_session_against_its_task_agent_and_purpose() {
        let (harness, _) = a_killed_run("recover-attribution");
        harness.started_session("FRK-2", "dev-b", "session-review", "verify");
        // A session whose cost was recorded before the run stopped, and whose end was not.
        harness.started_session("FRK-2", "dev-a", "session-costed", "implement");
        harness.spent(Some("FRK-2"), "session-costed", 0.5);
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.recover().expect("recovery runs");

        assert_eq!(report.sessions_interrupted, 3);
        for (session, agent, purpose) in [
            (
                "session-killed",
                "dev-a",
                CostRecordedBodyPurpose::Implement,
            ),
            ("session-review", "dev-b", CostRecordedBodyPurpose::Verify),
        ] {
            let ended = of_session(&harness, EventKind::SessionEnded, session);
            let costs = of_session(&harness, EventKind::CostRecorded, session);
            assert_eq!(ended.len(), 1, "{session}: {ended:?}");
            assert_eq!(costs.len(), 1, "{session}: {costs:?}");
            for event in [&ended[0], &costs[0]] {
                let ids = &event.envelope.ids;
                assert_eq!(
                    ids.task_id.as_ref().map(|task| task.as_str()),
                    Some("FRK-2"),
                    "{session}"
                );
                assert_eq!(ids.agent_id.as_deref(), Some(agent), "{session}");
            }
            match &costs[0].body {
                EventBody::CostRecorded(body) => assert_eq!(body.purpose, purpose, "{session}"),
                other => panic!("a cost.recorded, got {other:?}"),
            }
        }
        assert_eq!(
            of_session(&harness, EventKind::SessionEnded, "session-costed").len(),
            1
        );
        // Its cost is not counted twice.
        assert_eq!(
            of_session(&harness, EventKind::CostRecorded, "session-costed").len(),
            1
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn removes_the_worktree_of_a_cancelled_task() {
        let harness = Harness::new("recover-cancelled", |_| {});
        harness.in_progress("FRK-1", "dev-a", "dev-b");
        harness.project.moved(
            "FRK-1",
            "in_progress",
            "cancelled",
            &json!({ "actor": "human", "requested_by": "human" }),
        );
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));

        let report = orchestrator.recover().expect("recovery runs");

        assert_eq!(report.worktrees_removed, 1);
        assert!(!harness.worktree("FRK-1").exists());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn resumes_an_in_progress_task_in_a_fresh_sandbox() {
        let (harness, sha) = a_killed_run("recover-resume");
        let adapter = harness.recorded(vec![implement_stops_early()]);
        let sandboxes = Arc::new(CountingSandboxFactory::default());
        let orchestrator = harness.orchestrator_with(adapter.clone(), sandboxes.clone());
        orchestrator.recover().expect("recovery runs");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(sandboxes.created("FRK-2"), 1);
        assert_eq!(sandboxes.created("FRK-1"), 0);
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert!(
            started[0]
                .initial_prompt
                .contains(&format!("Resuming: last commit {sha}")),
            "{}",
            started[0].initial_prompt
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn recovers_nothing_twice() {
        let (harness, _) = a_killed_run("recover-twice");
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        orchestrator.recover().expect("recovery runs");
        let before = harness.events(&[]).len();

        let report = orchestrator.recover().expect("recovery runs again");

        assert_eq!(
            report,
            RecoveryReport {
                sessions_interrupted: 0,
                worktrees_removed: 0,
                tasks_resumed: 1,
            }
        );
        assert_eq!(harness.events(&[]).len(), before);
    }
}
