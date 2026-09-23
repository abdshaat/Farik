//! Requests and epics (`docs/SPEC.md` sections 5.2, 5.16, and ADR 0013): the rules that take a
//! request from `draft` through triage and refining to `ready`.

use farik_core::contract::{Role, TaskContract, TaskKind, TaskStatus};
use farik_core::governor::transition::TransitionRequest;
use farik_core::governor::transition_table::TransitionActor;
use farik_core::team::{Agent, Team};
use farik_protocol::event::{ContractEvaluatedBodyGate, EventBody, EventKind, FarikEvent};
use farik_store::{EventQuery, TaskProjection};

use super::messages::{refine_message, triage_message};
use super::rules::{Room, acted, refused_since_entering, room};
use super::session::{SessionAsk, run_session};
use super::{OrchestratorDeps, OrchestratorError, TickReport};
use crate::session::SessionPurpose;
use crate::transitions::{TransitionAsk, TransitionOutcome, refusal_details};

/// The Product Manager: the first active agent of that role in team-file order.
pub(super) fn product_manager(team: &Team) -> Option<&Agent> {
    team.active_agents()
        .find(|agent| Role::from(agent.role) == Role::ProductManager)
}

/// Rule 10: a `draft`. Untriaged, it gets the Product Manager's triage session; triaged, it is
/// moved to `refining` on the Product Manager's behalf, with no session, unless that move was
/// refused since the task became a draft.
pub(super) async fn draft(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    if !row.triaged {
        let contract = deps.tools.files.read_contract(&row.task_id)?;
        if room(deps, team, &contract, day_spent)? != Room::Free {
            return Ok(None);
        }
        let end = run_session(
            deps,
            team,
            SessionAsk {
                agent: pm,
                contract: &contract,
                purpose: SessionPurpose::Triage,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: false,
                initial_prompt: triage_message(&contract),
            },
        )
        .await?;
        return Ok(Some(acted(row, pm, "triage", &end)));
    }
    if refused_since_entering(deps, &row.task_id, TaskStatus::Draft, TaskStatus::Refining)? {
        return Ok(None);
    }
    let outcome = deps.tools.transitions.request(
        &TransitionRequest {
            task_id: row.task_id.clone(),
            to: TaskStatus::Refining,
            actor: TransitionActor::ProductManager,
            agent_id: Some(pm.id.to_string()),
        },
        &TransitionAsk::default(),
        team,
    )?;
    Ok(match outcome {
        TransitionOutcome::Moved(_) => Some(TickReport::Acted {
            task_id: row.task_id.clone(),
            what: format!("started refining it for {}", pm.id.as_str()),
        }),
        TransitionOutcome::Refused(_) => None,
    })
}

/// Rule 9: a contract `refining`. A contract written since refining began and not judged since,
/// or one filed whole (a breakdown's child, or a contract the human holds) and not judged since
/// refining began, is judged: `escalated` asked as the governor first, whose rows open on three
/// readiness failures or on a passing contract the human must approve, then `ready`. Otherwise the
/// Product Manager gets a refine session.
pub(super) async fn refining(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
    day_spent: &mut bool,
) -> Result<Option<TickReport>, OrchestratorError> {
    let Some(pm) = product_manager(team) else {
        return Ok(None);
    };
    let contract = deps.tools.files.read_contract(&row.task_id)?;
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(row.task_id.clone()),
        ..EventQuery::default()
    })?;
    let began = refining_began(&history);
    if is_to_be_judged(&contract, &history, began) {
        return judge(deps, team, row).map(Some);
    }
    if room(deps, team, &contract, day_spent)? != Room::Free {
        return Ok(None);
    }
    let asked = history
        .iter()
        .any(|event| event.envelope.seq > began && event.body.kind() == EventKind::QuestionAsked);
    let failures = last_readiness_failures(&history, began);
    let end = run_session(
        deps,
        team,
        SessionAsk {
            agent: pm,
            contract: &contract,
            purpose: SessionPurpose::Refine,
            cwd: deps.tools.files.root().to_path_buf(),
            executor: None,
            read_only: false,
            initial_prompt: refine_message(&contract, !asked, &failures),
        },
    )
    .await?;
    Ok(Some(acted(row, pm, "refine", &end)))
}

/// Asks the governor to judge a contract: `escalated` first, because asking for `ready` on a
/// passing epic fails on its missing approval and would count as a readiness failure, then `ready`.
fn judge(
    deps: &OrchestratorDeps,
    team: &Team,
    row: &TaskProjection,
) -> Result<TickReport, OrchestratorError> {
    let ask = |to: TaskStatus| {
        deps.tools.transitions.request(
            &TransitionRequest {
                task_id: row.task_id.clone(),
                to,
                actor: TransitionActor::Governor,
                agent_id: None,
            },
            &TransitionAsk::default(),
            team,
        )
    };
    let what = match ask(TaskStatus::Escalated)? {
        TransitionOutcome::Moved(_) => "escalated its contract to the human".to_string(),
        TransitionOutcome::Refused(_) => match ask(TaskStatus::Ready)? {
            TransitionOutcome::Moved(_) => "judged its contract ready".to_string(),
            TransitionOutcome::Refused(refusal) => format!(
                "judged its contract, which is not ready: {}",
                refusal_details(&refusal).join("; ")
            ),
        },
    };
    Ok(TickReport::Acted {
        task_id: row.task_id.clone(),
        what,
    })
}

/// Where refining last began: the later of the task's last move into `refining` and its last
/// triage, or 0.
fn refining_began(history: &[FarikEvent]) -> u64 {
    history
        .iter()
        .filter(|event| match &event.body {
            EventBody::TaskTransitioned(body) => body.to.to_string() == "refining",
            EventBody::RequestTriaged(_) => true,
            _ => false,
        })
        .map(|event| event.envelope.seq)
        .max()
        .unwrap_or(0)
}

/// Whether the contract is judged now: written since refining began with no refusal by the
/// governor since that write, or, with no such write, filed whole and not refused since refining
/// began. A raw request is not judged: its brief would fail and spend one of the three attempts.
fn is_to_be_judged(contract: &TaskContract, history: &[FarikEvent], began: u64) -> bool {
    let refused_after = |after: u64| {
        history.iter().any(|event| {
            event.envelope.seq > after
                && matches!(&event.body, EventBody::TransitionRefused(body)
                    if body.from.to_string() == "refining" && body.requested_by == "governor")
        })
    };
    let written = history
        .iter()
        .rev()
        .find(|event| event.envelope.seq > began && event.body.kind() == EventKind::ContractWritten)
        .map(|event| event.envelope.seq);
    match written {
        Some(written) => !refused_after(written),
        None => (contract.parent.is_some() || contract.locked) && !refused_after(began),
    }
}

/// The failures of the last Definition of Ready judged since refining began, when it failed.
fn last_readiness_failures(history: &[FarikEvent], began: u64) -> Vec<String> {
    history
        .iter()
        .rev()
        .filter(|event| event.envelope.seq > began)
        .find_map(|event| match &event.body {
            EventBody::ContractEvaluated(body)
                if body.gate == ContractEvaluatedBodyGate::DefinitionOfReady =>
            {
                Some(if body.passed {
                    Vec::new()
                } else {
                    body.failures.clone()
                })
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Whether `row` is an epic, which rules 6 and 7 leave to the epic's own rules.
pub(super) fn is_epic(row: &TaskProjection) -> bool {
    row.kind == TaskKind::Epic
}

#[cfg(test)]
mod tests {
    use farik_core::contract::{TaskKind, TaskStatus};
    use farik_core::team::Effort;
    use farik_protocol::command::{Command, RequestSize};
    use farik_protocol::event::{
        EscalationRaisedBodyReason, EventBody, EventKind, FarikEvent, NewEvent,
        RequestTriagedBodySize, TransitionActorWire, event_from_value,
    };
    use serde_json::json;

    use crate::orchestrator::TRIAGE_MODEL;
    use crate::orchestrator::fixtures::Harness;
    use crate::orchestrator::{Orchestrator, TickReport};
    use crate::recorded::fixtures::{
        plan_assigns_frk_1, refine_asks_frk_1, refine_writes_epic_frk_1, refine_writes_task_frk_1,
        replays_farik_read_board, triage_frk_1_large,
    };
    use crate::session::SessionPurpose;
    use crate::tools::fixtures::at;

    fn task(id: &str) -> farik_core::contract::TaskId {
        id.parse().expect("a task id")
    }

    fn last(harness: &Harness, kind: EventKind) -> Option<FarikEvent> {
        harness.events(&[kind]).pop()
    }

    fn is_idle(report: &TickReport) -> bool {
        matches!(report, TickReport::Idle { .. })
    }

    /// Each move of `id`, as `from -> to`, in order.
    fn moves_of(harness: &Harness, id: &str) -> Vec<String> {
        harness
            .events(&[EventKind::TaskTransitioned])
            .iter()
            .filter(|event| event.envelope.ids.task_id == Some(task(id)))
            .map(|event| match &event.body {
                EventBody::TaskTransitioned(body) => format!("{} -> {}", body.from, body.to),
                other => panic!("a move, got {other:?}"),
            })
            .collect()
    }

    /// A request filed by the human and sized by them, `size`, as `farik triage` does.
    async fn a_sized_request(harness: &Harness, orchestrator: &Orchestrator, size: RequestSize) {
        harness.a_request("Add done.txt and its check");
        orchestrator
            .handle(Command::RequestTriage {
                task_id: task("FRK-1"),
                size,
                reason: "Sized by the human.".to_string(),
            })
            .await
            .expect("the human sizes a draft");
    }

    /// A request sized `size` and moved to `refining` by a tick.
    async fn refining(
        harness: &Harness,
        orchestrator: &Orchestrator,
        size: RequestSize,
    ) -> TickReport {
        a_sized_request(harness, orchestrator, size).await;
        let report = orchestrator.tick().await.expect("the tick runs");
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Refining);
        report
    }

    /// Calls a Farik tool as `agent` on `task`, as a session of theirs would.
    async fn call(
        harness: &Harness,
        agent: &str,
        task: Option<&str>,
        tool: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, crate::tools::ToolError> {
        crate::tools::call_tool(&harness.project.context(agent, task), tool, input).await
    }

    /// The failing write the tests seed: a budget over the team's cap of 5 dollars.
    async fn write_over_the_cap(harness: &Harness) {
        call(
            harness,
            "pm",
            Some("FRK-1"),
            "farik_write_contract",
            json!({ "fields": { "budget": { "max_cost_usd": 50 } } }),
        )
        .await
        .expect("the schema allows the write");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn triages_a_request_on_the_cheaper_model() {
        let harness = Harness::new("req-triage", |_| {});
        harness.a_request("Add done.txt and its check");
        let adapter = harness.recorded(vec![triage_frk_1_large()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        let spec = &started[0];
        assert_eq!(spec.purpose, SessionPurpose::Triage);
        assert_eq!(spec.agent_id, "pm");
        assert_eq!(spec.model, TRIAGE_MODEL);
        assert_eq!(spec.effort, Effort::Low);
        assert_eq!(spec.farik_tools, vec!["farik_triage_request".to_string()]);
        assert!(spec.builtin_tools.is_empty(), "{:?}", spec.builtin_tools);
        let triaged = last(&harness, EventKind::RequestTriaged).expect("the triage");
        let EventBody::RequestTriaged(body) = &triaged.body else {
            panic!("a triage");
        };
        assert_eq!(body.size, RequestTriagedBodySize::Large);
        assert_eq!(body.triaged_by, "pm");
        assert_eq!(harness.row("FRK-1").kind, TaskKind::Epic);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn starts_refining_a_triaged_request_without_a_session() {
        let harness = Harness::new("req-draft-to-refining", |_| {});
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        refining(&harness, &orchestrator, RequestSize::Large).await;

        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (body.from.to_string(), body.to.to_string()),
            ("draft".to_string(), "refining".to_string())
        );
        assert_eq!(body.actor, TransitionActorWire::ProductManager);
        assert_eq!(body.requested_by, "pm");
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn acts_for_no_agent_that_is_paused() {
        // A team always has an active Product Manager; the one that acts is the first active one.
        let harness = Harness::new("req-paused", |wire| {
            wire["agents"][0]["status"] = json!("paused");
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(json!({
                    "id": "pm-2",
                    "display_name": "pm-2",
                    "role": "product_manager",
                    "status": "active"
                }));
        });
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        a_sized_request(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the tick runs");
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        assert!(matches!(
            &moved.body,
            EventBody::TaskTransitioned(body) if body.requested_by == "pm-2"
        ));

        let harness = Harness::new("req-paused-assignee", |wire| {
            wire["agents"][1]["status"] = json!("paused");
        });
        harness.assigned("FRK-2", "dev-a", "dev-b");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(harness.row("FRK-2").status, TaskStatus::Assigned);
        assert!(adapter.started().is_empty());
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn asks_first_when_refining_an_epic() {
        let harness = Harness::new("req-asks-first", |_| {});
        let adapter = harness.recorded(vec![refine_asks_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Refine);
        assert!(
            started[0].initial_prompt.contains("farik_ask_human"),
            "{}",
            started[0].initial_prompt
        );
        assert!(last(&harness, EventKind::QuestionAsked).is_some());
        assert!(harness.row("FRK-1").waiting_on_human);
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn hands_the_answer_to_the_next_refine_session() {
        let harness = Harness::new("req-answer-handed", |_| {});
        let adapter = harness.recorded(vec![
            refine_asks_frk_1(),
            replays_farik_read_board(),
            replays_farik_read_board(),
        ]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the question is asked");
        let n = last(&harness, EventKind::QuestionAsked)
            .expect("the question")
            .envelope
            .seq;
        orchestrator
            .handle(Command::QuestionAnswer {
                question_id: n,
                answer: "Yes.".to_string(),
            })
            .await
            .expect("the human answers");

        orchestrator.tick().await.expect("the tick runs");
        let prompt = adapter.started()[1].system_prompt.clone();
        let question = prompt
            .find(&format!("Question {n}:"))
            .unwrap_or_else(|| panic!("the question's id: {prompt}"));
        let wrapped = prompt
            .find("<untrusted source=\"question\">")
            .expect("the question as the agent's words");
        let asked = prompt
            .find("Should done.txt be empty?")
            .expect("the question");
        let answer = prompt.find("Answer: Yes.").expect("the answer, unwrapped");
        assert!(
            question < wrapped && wrapped < asked && asked < answer,
            "{prompt}"
        );

        orchestrator.tick().await.expect("the tick runs");
        let prompt = adapter.started()[2].system_prompt.clone();
        assert_eq!(adapter.started()[2].purpose, SessionPurpose::Refine);
        assert!(!prompt.contains(&format!("Question {n}:")), "{prompt}");
        assert!(!prompt.contains("Answer: Yes."), "{prompt}");
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_written_contract_before_refining_again() {
        let harness = Harness::new("req-judges", |_| {});
        let adapter = harness.recorded(vec![refine_writes_task_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        orchestrator.tick().await.expect("the contract is written");
        assert!(last(&harness, EventKind::ContractWritten).is_some());

        orchestrator.tick().await.expect("the tick runs");

        let evaluated = last(&harness, EventKind::ContractEvaluated).expect("the judgement");
        assert!(
            matches!(&evaluated.body, EventBody::ContractEvaluated(body) if body.passed),
            "{evaluated:?}"
        );
        let moved = last(&harness, EventKind::TaskTransitioned).expect("a move");
        let EventBody::TaskTransitioned(body) = &moved.body else {
            panic!("a move");
        };
        assert_eq!(
            (
                body.from.to_string(),
                body.to.to_string(),
                body.requested_by.as_str()
            ),
            ("refining".to_string(), "ready".to_string(), "governor")
        );
        assert_eq!(adapter.started().len(), 1);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn returns_a_failing_contract_with_its_failures() {
        let harness = Harness::new("req-failing", |_| {});
        let adapter = harness.recorded(vec![replays_farik_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Small).await;
        write_over_the_cap(&harness).await;

        orchestrator.tick().await.expect("the tick runs");
        let evaluated = last(&harness, EventKind::ContractEvaluated).expect("the judgement");
        let EventBody::ContractEvaluated(body) = &evaluated.body else {
            panic!("a judgement");
        };
        assert!(!body.passed);
        assert!(
            body.failures
                .iter()
                .any(|failure| failure.contains("exceeds the team's cap of 5 USD")),
            "{:?}",
            body.failures
        );
        assert!(adapter.started().is_empty());

        orchestrator.tick().await.expect("the tick runs");
        let started = adapter.started();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].purpose, SessionPurpose::Refine);
        assert!(
            started[0]
                .initial_prompt
                .contains("exceeds the team's cap of 5 USD"),
            "{}",
            started[0].initial_prompt
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_an_epic_for_approval_once_it_passes() {
        let harness = Harness::new("req-approval", |_| {});
        let adapter = harness.recorded(vec![refine_writes_epic_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the epic is written");

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "FRK-1").last().map(String::as_str),
            Some("refining -> escalated")
        );
        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        assert!(matches!(
            &escalation.body,
            EventBody::EscalationRaised(body) if body.reason == EscalationRaisedBodyReason::Approval
        ));
        assert!(
            harness.events(&[EventKind::ContractEvaluated]).iter().all(
                |event| matches!(&event.body, EventBody::ContractEvaluated(body) if body.passed)
            )
        );
        assert!(harness.row("FRK-1").awaiting_approval);
        assert!(is_idle(&orchestrator.tick().await.expect("the tick runs")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn escalates_a_contract_that_failed_three_times() {
        let harness = Harness::new("req-three-failures", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        refining(&harness, &orchestrator, RequestSize::Small).await;
        for _ in 0..3 {
            harness.project.record(
                "FRK-1",
                "contract.evaluated",
                &json!({ "gate": "definition_of_ready", "passed": false, "failures": ["no"] }),
            );
        }
        write_over_the_cap(&harness).await;

        orchestrator.tick().await.expect("the tick runs");

        let escalation = last(&harness, EventKind::EscalationRaised).expect("the escalation");
        assert!(matches!(
            &escalation.body,
            EventBody::EscalationRaised(body)
                if body.reason == EscalationRaisedBodyReason::ReadinessFailures
        ));
        assert_eq!(harness.row("FRK-1").status, TaskStatus::Escalated);
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn judges_a_child_filed_whole_without_a_session() {
        let harness = Harness::new("req-child-whole", |_| {});
        harness
            .project
            .filed_with("FRK-1", "in_progress", "epic", None, |wire| {
                wire["assignee_role"] = json!("product_manager");
                wire["reviewer_role"] = json!("human");
                wire["allowed_paths"] = json!(["done.txt"]);
            });
        harness.project.moved(
            "FRK-1",
            "assigned",
            "in_progress",
            &json!({ "assignee": "pm" }),
        );
        // The call `plan_breaks_down_frk_1` makes, made here without its session.
        let mut contract = Harness::request_fields("Add done.txt");
        contract["budget"] = json!({ "max_cost_usd": 2 });
        call(
            &harness,
            "pm",
            None,
            "farik_create_task",
            json!({ "parent": "FRK-1", "contract": contract }),
        )
        .await
        .expect("the epic's assignee files its task");
        let adapter = harness.recorded(Vec::new());
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");
        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(
            moves_of(&harness, "FRK-2"),
            ["draft -> refining", "refining -> ready"]
        );
        assert!(
            adapter
                .started()
                .iter()
                .all(|spec| spec.task_id != Some(task("FRK-2")))
        );
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn passes_over_a_task_waiting_on_the_human() {
        let harness = Harness::new("req-waiting", |_| {});
        harness.ready("FRK-1");
        harness.project.record(
            "FRK-1",
            "question.asked",
            &json!({ "question": "Which file?", "asked_by": "pm" }),
        );
        harness.ready("FRK-2");
        let adapter = harness.recorded(vec![plan_assigns_frk_1()]);
        let orchestrator = harness.orchestrator(adapter.clone());

        orchestrator.tick().await.expect("the tick runs");

        assert_eq!(adapter.started()[0].task_id, Some(task("FRK-2")));
    }

    #[tokio::test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    async fn catches_up_with_another_process_before_each_tick() {
        let harness = Harness::new("req-catch-up", |_| {});
        let adapter = harness.recorded(vec![refine_asks_frk_1(), replays_farik_read_board()]);
        let orchestrator = harness.orchestrator(adapter.clone());
        refining(&harness, &orchestrator, RequestSize::Large).await;
        orchestrator.tick().await.expect("the question is asked");
        let n = last(&harness, EventKind::QuestionAsked)
            .expect("the question")
            .envelope
            .seq;
        let wire = json!({
            "seq": 1,
            "recorded_at": at().to_rfc3339(),
            "team_id": "farik",
            "project_id": "farik",
            "task_id": "FRK-1",
            "kind": "question.answered",
            "body": { "question_id": n, "answer": "Yes.", "answered_by": "human" },
        });
        let event = event_from_value(&wire).expect("the answer is schema-valid");
        harness
            .project
            .deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("another process appends the answer");

        orchestrator.tick().await.expect("the tick runs");

        let started = adapter.started();
        assert_eq!(started.len(), 2);
        assert_eq!(started[1].purpose, SessionPurpose::Refine);
        assert_eq!(started[1].task_id, Some(task("FRK-1")));
    }
}
