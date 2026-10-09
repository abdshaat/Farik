//! The Product Manager decides a data pipeline request (`docs/SPEC.md` 6.10, ADR 0039): one
//! session about no task for the oldest request nobody decided or passed on, three tries at most,
//! then Farik passes the request to the owner.

use farik_core::contract::Role;
use farik_core::team::Team;
use farik_protocol::event::{DataPipelineEscalatedBody, EventBody};
use farik_store::pipelines::{PipelineRecord, PipelineState, data_pipelines};

use super::messages::decide_data_pipeline_message;
use super::rules::{Waiting, asleep, day_is_spent, ran};
use super::session::{SessionAsk, run_session};
use super::verify::append_stamped;
use super::{OrchestratorDeps, OrchestratorError, TickReport, TickRules, TickScope};
use crate::procurement::PIPELINES;
use crate::session::SessionPurpose;

/// The one tool the Product Manager's decision session is given; a session given it alone closes
/// with `PIPELINE_DECISION_INSTRUCTION`.
pub(super) const DECIDE_PIPELINE_TOOL: &str = "farik_decide_data_pipeline";

/// How many decision sessions a request is given before Farik passes it to the owner.
const MOST_TRIES: usize = 3;

/// What Farik says when it passes a request on because the Product Manager did not decide it.
const DID_NOT_DECIDE: &str = "The Product Manager did not decide";

/// The data pipeline rule, between the budget and channel rules and rule 3: each open request
/// whose three decision sessions ended with no word is passed to the owner by Farik (an
/// `escalated` that names no agent and no session), and the oldest one left gets one `verify`
/// session of the active Product Manager, about no task, given `farik_decide_data_pipeline` alone.
/// It is about no one task, so it runs only under `All` in a tick scoped to none. Nothing happens
/// with no active Product Manager, and the session waits on a spent day or a sleeping manager, as
/// a design plan's decision does.
///
/// # Errors
///
/// What the log or the session refused.
pub(super) async fn decide_pipelines(
    deps: &OrchestratorDeps,
    scope: &TickScope,
    team: &Team,
    waiting: &mut Waiting,
) -> Result<Option<TickReport>, OrchestratorError> {
    if scope.task_id.is_some() || scope.rules != TickRules::All {
        return Ok(None);
    }
    let Some(manager) = team
        .active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
    else {
        return Ok(None);
    };
    let tools = &deps.tools;
    let open: Vec<PipelineRecord> = data_pipelines(&tools.log)?
        .into_iter()
        .filter(|record| record.state == PipelineState::Open)
        .collect();
    let mut next = None;
    for record in open {
        if record.tries.len() >= MOST_TRIES {
            pass_to_the_owner(deps, record.pipeline)?;
        } else if next.is_none() {
            next = Some(record);
        }
    }
    let Some(record) = next else {
        return Ok(None);
    };
    if day_is_spent(deps, team, Role::ProductManager, &mut waiting.day_spent)?
        | asleep(deps, manager, &mut waiting.slept)?
    {
        return Ok(None);
    }
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: manager,
            contract: None,
            purpose: SessionPurpose::Verify,
            cwd: tools.files.root().to_path_buf(),
            executor: None,
            read_only: true,
            only_tool: Some(DECIDE_PIPELINE_TOOL),
            tools: None,
            in_reply_to: None,
            thread: None,
            pipeline: Some(record.pipeline),
            initial_prompt: decide_data_pipeline_message(&record),
        },
    )
    .await?;
    Ok(Some(TickReport::Acted {
        task_id: record.task_id.clone(),
        what: ran(manager, "data pipeline decision", &end),
    }))
}

/// Records `data_pipeline.escalated` for `pipeline` as Farik, if it is still open when the lock
/// is held.
fn pass_to_the_owner(deps: &OrchestratorDeps, pipeline: u64) -> Result<(), OrchestratorError> {
    let tools = &deps.tools;
    let _held = crate::locked(&PIPELINES);
    let still_open = data_pipelines(&tools.log)?
        .iter()
        .any(|record| record.pipeline == pipeline && record.state == PipelineState::Open);
    if !still_open {
        return Ok(());
    }
    let body = DataPipelineEscalatedBody {
        pipeline: std::num::NonZeroU64::new(pipeline)
            .map(Into::into)
            .ok_or_else(|| OrchestratorError::Refused {
                reason: "pipeline_unnumbered: a request is numbered from 1".to_string(),
            })?,
        reason: DID_NOT_DECIDE
            .parse()
            .map_err(|error| OrchestratorError::Refused {
                reason: format!("pipeline_reason_invalid: {error}"),
            })?,
    };
    append_stamped(
        tools,
        tools.ids.clone(),
        EventBody::DataPipelineEscalated(body),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use farik_protocol::event::{EventBody, EventKind};
    use farik_store::CostScope;
    use serde_json::json;

    use super::decide_pipelines;
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::rules::Waiting;
    use crate::orchestrator::{TickReport, TickRules, TickScope};
    use crate::prompt::PIPELINE_DECISION_INSTRUCTION;
    use crate::recorded::fixtures::{decide_data_pipeline, ends_without_a_decision};
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::at;

    /// A reason of a good length.
    const REASON: &str = "The plain pages answer this question, so the team needs no more.";

    /// A harness with the Procurement Specialist `proc`, whose FRK-1 waits for the owner.
    fn a_harness(name: &str) -> Harness {
        let harness = Harness::new(name, |wire| {
            wire["policy"]["wip_limit_per_agent"] = json!(2);
            crate::tools::fixtures::with_the_procurement_specialist(wire);
        });
        // The task waits for the owner: no rule has anything to do for it, and a request is
        // decided whatever its task has become.
        harness.procurement_task("FRK-1", Some("escalated"));
        harness
    }

    /// `proc`'s request for `name` on FRK-1, which costs money; answers its number.
    fn requested(harness: &Harness, name: &str) -> u64 {
        harness
            .project
            .record_in(
                Some("proc"),
                Some("session-proc"),
                "FRK-1",
                "data_pipeline.requested",
                &json!({
                    "name": name,
                    "what": "Prices as clean text from the seller pages the task compares.",
                    "source_url": "https://www.firecrawl.dev/pricing",
                    "why": "Three sellers hide their prices behind scripts the plain fetch cannot read.",
                    "cost": "paid",
                    "needs_account": true,
                    "sends_project_data": false
                }),
            )
            .envelope
            .seq
    }

    /// The (sessions, dollars) the cost projection holds for FRK-1.
    fn spent_on_the_task(harness: &Harness) -> (u32, f64) {
        harness
            .project
            .deps
            .projections
            .costs(CostScope::Task)
            .expect("the costs read")
            .iter()
            .find(|cost| cost.key == "FRK-1")
            .map_or((0, 0.0), |cost| (cost.sessions, cost.usd))
    }

    fn pipelines_started(harness: &Harness) -> Vec<(Option<u64>, String)> {
        harness
            .events(&[EventKind::SessionStarted])
            .into_iter()
            .filter_map(|event| match &event.body {
                EventBody::SessionStarted(body) => Some((
                    body.pipeline.as_ref().map(|number| number.get()),
                    event.envelope.ids.session_id.clone().unwrap_or_default(),
                )),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    #[allow(
        clippy::too_many_lines,
        reason = "one row for each thing the decision session is and is not given"
    )]
    async fn an_open_request_starts_the_product_managers_decision() {
        let harness = a_harness("pipeline-decide");
        let pipeline = requested(&harness, "Firecrawl");
        let before = spent_on_the_task(&harness);
        let adapter = harness.recorded(vec![decide_data_pipeline(pipeline, "decline", REASON)]);
        let orchestrator = harness.orchestrator(adapter.clone());

        // A tick scoped to a task, or running the planning rules alone, decides nothing.
        for scope in [
            TickScope {
                task_id: Some("FRK-1".parse().expect("a task id")),
                ..TickScope::default()
            },
            TickScope {
                rules: TickRules::Planning,
                ..TickScope::default()
            },
        ] {
            orchestrator
                .tick_within(&scope)
                .await
                .expect("the tick runs");
            assert!(
                adapter
                    .started()
                    .iter()
                    .all(|spec| spec.farik_tools != ["farik_decide_data_pipeline"]),
                "{:?}",
                adapter.started()
            );
        }
        assert!(
            pipelines_started(&harness)
                .iter()
                .all(|(named, _)| named.is_none())
        );

        let report = orchestrator.tick().await.expect("the tick runs");

        let TickReport::Acted { task_id, what } = &report else {
            panic!("a request was decided: {report:?}");
        };
        assert_eq!(task_id.as_str(), "FRK-1");
        assert!(
            what.contains("pm's data pipeline decision session"),
            "{what}"
        );
        let started = adapter.started();
        let decisions: Vec<_> = started
            .iter()
            .filter(|spec| spec.farik_tools == ["farik_decide_data_pipeline"])
            .collect();
        assert_eq!(decisions.len(), 1, "{started:?}");
        let spec = decisions[0];
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.purpose, SessionPurpose::Verify);
        assert_eq!(spec.task_id, None, "a session about no task");
        assert!(spec.builtin_tools.is_empty(), "{:?}", spec.builtin_tools);
        assert!(
            spec.system_prompt.contains(PIPELINE_DECISION_INSTRUCTION),
            "{}",
            spec.system_prompt
        );
        // Every word the agent wrote is inside the untrusted notice.
        let message = &spec.initial_prompt;
        let notice = message
            .split("<untrusted")
            .nth(1)
            .expect("an untrusted block");
        for words in [
            "Firecrawl",
            "Prices as clean text from the seller pages the task compares.",
            "https://www.firecrawl.dev/pricing",
            "Three sellers hide their prices behind scripts the plain fetch cannot read.",
            "paid",
        ] {
            assert!(notice.contains(words), "{words}: {message}");
        }
        assert!(message.contains(&format!("{pipeline}")), "{message}");

        // The session names the request it decides; the task's sessions and cost are as they were,
        // and the day's cost holds the session's.
        assert_eq!(
            pipelines_started(&harness)
                .into_iter()
                .filter(|(named, _)| *named == Some(pipeline))
                .count(),
            1
        );
        assert_eq!(spent_on_the_task(&harness), before);
        let day: f64 = harness
            .project
            .deps
            .projections
            .costs(CostScope::Day)
            .expect("the costs read")
            .iter()
            .map(|cost| cost.usd)
            .sum();
        assert!(day > 0.0, "the session's dollars are the day's: {day}");
        let declined = harness.events(&[EventKind::DataPipelineDeclined]);
        assert_eq!(declined.len(), 1);
        assert_eq!(
            declined[0].envelope.ids.session_id.as_deref(),
            Some(spec.session_id.as_str())
        );

        // Decided, it is not asked again.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(
            adapter
                .started()
                .iter()
                .filter(|spec| spec.farik_tools == ["farik_decide_data_pipeline"])
                .count(),
            1
        );
    }

    /// The text the first tick of a harness gives when nothing was done, as a value to match.
    fn why_idle(report: &TickReport) -> (&str, bool) {
        match report {
            TickReport::Idle { why, until } => (why.as_str(), until.is_some()),
            other => panic!("expected an idle tick: {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_spent_day_or_a_sleeping_manager_waits() {
        // The day's dollars are spent.
        let harness = a_harness("pipeline-day");
        harness.spent(None, "s-0", 20.0);
        let pipeline = requested(&harness, "Firecrawl");
        let adapter = harness.recorded(vec![decide_data_pipeline(pipeline, "decline", REASON)]);
        let report = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        assert_eq!(
            why_idle(&report),
            ("the team's daily budget is spent", false)
        );
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());

        // The Product Manager sleeps until its model provider's limit resets.
        let harness = a_harness("pipeline-asleep");
        let pipeline = requested(&harness, "Firecrawl");
        harness.asleep("pm", at() + chrono::Duration::days(10));
        let adapter = harness.recorded(vec![decide_data_pipeline(pipeline, "decline", REASON)]);
        let report = harness
            .orchestrator(adapter.clone())
            .tick()
            .await
            .expect("the tick runs");
        let (why, wakes) = why_idle(&report);
        assert!(why.starts_with("waiting for pm, asleep until "), "{why}");
        assert!(wakes);
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
        assert!(
            harness
                .events(&[EventKind::DataPipelineDeclined])
                .is_empty()
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn three_tries_then_the_owner() {
        let harness = a_harness("pipeline-tries");
        let first = requested(&harness, "Firecrawl");
        let second = requested(&harness, "Shippo rates");
        let before = spent_on_the_task(&harness);
        let adapter = harness.recorded(vec![
            ends_without_a_decision(),
            ends_without_a_decision(),
            ends_without_a_decision(),
            decide_data_pipeline(second, "decline", REASON),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        // Three sessions end, each for the oldest request, with no word.
        for _ in 0..3 {
            orchestrator.tick().await.expect("the tick runs");
        }
        let tries = pipelines_started(&harness);
        assert_eq!(
            tries
                .iter()
                .filter(|(named, _)| *named == Some(first))
                .count(),
            3
        );
        assert!(
            harness
                .events(&[EventKind::DataPipelineEscalated])
                .is_empty()
        );

        // The fourth tick passes it to the owner instead of starting a fourth session for it, and
        // goes on to the next request in the same tick.
        let report = orchestrator.tick().await.expect("the tick runs");
        let escalated = harness.events(&[EventKind::DataPipelineEscalated]);
        assert_eq!(escalated.len(), 1);
        let EventBody::DataPipelineEscalated(body) = &escalated[0].body else {
            panic!("a data_pipeline.escalated body");
        };
        assert_eq!(body.pipeline.get(), first);
        assert_eq!(body.reason.as_str(), "The Product Manager did not decide");
        assert_eq!(escalated[0].envelope.ids.agent_id, None);
        assert_eq!(escalated[0].envelope.ids.session_id, None);
        assert_eq!(adapter.started().len(), 4, "{report:?}");
        let tries = pipelines_started(&harness);
        assert_eq!(
            tries
                .iter()
                .filter(|(named, _)| *named == Some(first))
                .count(),
            3,
            "no fourth for the first"
        );
        assert_eq!(
            tries
                .iter()
                .filter(|(named, _)| *named == Some(second))
                .count(),
            1,
            "the next request got its session in the same tick"
        );
        assert_eq!(harness.events(&[EventKind::DataPipelineDeclined]).len(), 1);
        assert_eq!(spent_on_the_task(&harness), before);

        // Passed on, it is the owner's and is not asked again.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(adapter.started().len(), 4);
        assert_eq!(harness.events(&[EventKind::DataPipelineEscalated]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn a_request_the_manager_passed_on_waits_for_the_owner() {
        let harness = a_harness("pipeline-passed-on");
        let pipeline = requested(&harness, "Firecrawl");
        let adapter = harness.recorded(vec![
            decide_data_pipeline(pipeline, "escalate", REASON),
            decide_data_pipeline(pipeline, "decline", REASON),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.events(&[EventKind::DataPipelineEscalated]).len(), 1);
        // Neither the manager's session nor Farik's rule asks again.
        orchestrator.tick().await.expect("the tick runs");
        assert_eq!(adapter.started().len(), 1, "{:?}", adapter.started());
        assert!(
            harness
                .events(&[EventKind::DataPipelineDeclined])
                .is_empty()
        );
        assert_eq!(harness.events(&[EventKind::DataPipelineEscalated]).len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn no_product_manager_means_it_waits() {
        let harness = a_harness("pipeline-no-manager");
        let first = requested(&harness, "Firecrawl");
        // Even a request with its three tries used waits: nobody is there to pass it on for.
        for session in ["t-1", "t-2", "t-3"] {
            harness.project.record_in(
                Some("pm"),
                Some(session),
                "",
                "session.started",
                &json!({
                    "purpose": "verify", "model": "claude-opus-5-5", "effort": "high",
                    "pipeline": first
                }),
            );
        }
        let events = harness.project.event_count();
        let adapter = harness.recorded(vec![decide_data_pipeline(first, "decline", REASON)]);
        let orchestrator = harness.orchestrator(adapter.clone());
        // A valid team has an active Product Manager, so the rule is asked with a team that lost
        // it between the file's reading and the rule.
        let mut team = orchestrator.deps.tools.files.read_team().expect("the team");
        for agent in &mut team.agents {
            if agent.role == farik_core::team::RoleWire::ProductManager {
                agent.status = farik_core::team::AgentStatus::Retired;
            }
        }

        let mut waiting = Waiting::default();
        let report = decide_pipelines(
            &orchestrator.deps,
            &TickScope::default(),
            &team,
            &mut waiting,
        )
        .await
        .expect("the rule runs");

        assert_eq!(report, None);
        assert!(adapter.started().is_empty(), "{:?}", adapter.started());
        let kinds = [
            EventKind::DataPipelineEscalated,
            EventKind::DataPipelineApproved,
            EventKind::DataPipelineDeclined,
        ];
        assert!(harness.events(&kinds).is_empty());
        assert_eq!(harness.project.event_count(), events, "nothing is recorded");
    }
}
