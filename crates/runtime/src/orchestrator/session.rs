//! One session, start to end, the same way for every purpose: prompt, registration, start, the
//! costs it reports, and its end.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use farik_core::budget::{BudgetScope, SessionLedger, add_usage};
use farik_core::contract::{Role, TaskContract};
use farik_core::governor::permissions::PermissionTier;
use farik_core::pricing::Usage;
use farik_core::team::{Agent, Effort, Team};
use farik_protocol::event::{
    AgentSleptBody, EventBody, EventIds, EventKind, NoteWrittenBody, NoteWrittenBodyKind,
};
use farik_roles::load_role;
use farik_store::EventQuery;

use super::messages::human_message;
use super::verify::{append, append_stamped};
use super::{OrchestratorDeps, OrchestratorError, TRIAGE_MODEL};
use crate::claude::allowed_builtins;
use crate::cost::{CostError, CostSource, budget_state, record_exhaustion, record_session_cost};
use crate::daemon::SessionRegistration;
use crate::exec::Executor;
use crate::prompt::{
    JUDGMENT_INSTRUCTION, PromptInput, SPRINT_PLAN_INSTRUCTION, assemble_system_prompt,
};
use crate::session::{
    EndReason, SessionEvent, SessionHandle, SessionPurpose, SessionSpec, session_model,
};
use crate::sessions::{record_session_ended, record_session_started};
use crate::tools::{FarikTool, tool_descriptors};

/// The one tool a triage session is given.
pub(super) const TRIAGE_TOOL: &str = "farik_triage_request";

/// The one tool the Scrum Master's judgment session is given; a session given it alone closes
/// with `JUDGMENT_INSTRUCTION`.
pub(super) const JUDGMENT_TOOL: &str = "farik_record_judgment";

/// The one tool a sprint's planning session is given; a session given it alone closes with
/// `SPRINT_PLAN_INSTRUCTION`.
pub(super) const SPRINT_PLAN_TOOL: &str = "farik_plan_sprint";

/// What a rule asks a session for.
pub(super) struct SessionAsk<'a> {
    /// The agent the session is.
    pub(super) agent: &'a Agent,
    /// The task it is about, when it is about one: a sprint's planning session is about none.
    pub(super) contract: Option<&'a TaskContract>,
    /// Why it runs.
    pub(super) purpose: SessionPurpose,
    /// Where it works.
    pub(super) cwd: PathBuf,
    /// Where its commands run, when it runs any.
    pub(super) executor: Option<Arc<dyn Executor>>,
    /// Whether it gets the read tier's built-ins alone, whatever the agent's tiers, and no Farik
    /// tool that runs a command or writes to git: a verify session reads the work and does not
    /// change it.
    pub(super) read_only: bool,
    /// The one Farik tool it is given, when it is given one alone and no built-in tool: triage's
    /// `farik_triage_request`, the judgment's `farik_record_judgment`, a sprint's planning
    /// `farik_plan_sprint`.
    pub(super) only_tool: Option<&'static str>,
    /// Its first message.
    pub(super) initial_prompt: String,
}

/// How a session ended.
pub(super) struct SessionEnd {
    /// The session.
    pub(super) session_id: String,
    /// Why.
    pub(super) reason: EndReason,
    /// What the program said.
    pub(super) detail: String,
    /// When the model provider said its limit resets, for a session that ended at it.
    pub(super) resets_at: Option<DateTime<Utc>>,
    /// The budgets the session's reported usage crossed, in the order it crossed them.
    pub(super) crossed: Vec<BudgetScope>,
}

/// Runs one session to its end: registered with the daemon for as long as it runs, its start,
/// every cost it reports, and its end recorded.
pub(super) async fn run_session(
    deps: &OrchestratorDeps,
    team: &Team,
    ask: SessionAsk<'_>,
) -> Result<SessionEnd, OrchestratorError> {
    let spec = session_spec(deps, team, &ask)?;
    let role = Role::from(ask.agent.role);
    deps.daemon.register_session(SessionRegistration {
        session_id: spec.session_id.clone(),
        agent_id: spec.agent_id.clone(),
        task_id: spec.task_id.clone(),
        cwd: spec.cwd.clone(),
        executor: ask.executor,
        limits: spec.limits,
        farik_tools: spec.farik_tools.clone(),
    });
    let ended = drive(deps, team, role, ask.contract, &spec).await;
    deps.daemon.end_session(&spec.session_id);
    let end = ended?;
    // The sleep first: an agent not put to sleep is started again into its provider's refusal.
    if end.reason == EndReason::ProviderLimit {
        sleep(deps, ask.agent, &end)?;
    }
    if let Some(contract) = ask.contract
        && ask.purpose == SessionPurpose::Implement
    {
        leave_note(deps, contract, ask.agent, &end)?;
    }
    Ok(end)
}

/// How long an agent sleeps when its model provider said no time its limit resets, or a time
/// already past.
const SLEEP_WITHOUT_A_RESET: chrono::Duration = chrono::Duration::hours(1);

/// Puts the agent of a session its model provider refused to sleep (5.5): `agent.slept` with the
/// agent on its envelope, until the time the provider said its limit resets when that is later
/// than now, else `SLEEP_WITHOUT_A_RESET` from now. The team file is not written: sleep is not a
/// status.
fn sleep(
    deps: &OrchestratorDeps,
    agent: &Agent,
    end: &SessionEnd,
) -> Result<(), OrchestratorError> {
    let tools = &deps.tools;
    let now = tools.clock.now();
    let until = end
        .resets_at
        .filter(|resets_at| *resets_at > now)
        .unwrap_or(now + SLEEP_WITHOUT_A_RESET);
    append_stamped(
        tools,
        EventIds {
            agent_id: Some(agent.id.to_string()),
            session_id: Some(end.session_id.clone()),
            ..tools.ids.clone()
        },
        EventBody::AgentSlept(AgentSleptBody {
            until,
            detail: end.detail.clone(),
        }),
    )
}

/// Who writes the note a session that stopped at its own limit leaves.
const FARIK: &str = "farik";

/// Leaves one progress note on the task of an implement session that ended at a limit, at its
/// model provider's limit, or past one of its own budgets, naming each and quoting the agent's own
/// last progress note of the session: the next implement session is shown the task's last note
/// alone (`resume()`), which is this one, so the agent's words are kept in it. Other purposes are
/// asked again from their own first message, and get none.
fn leave_note(
    deps: &OrchestratorDeps,
    contract: &TaskContract,
    agent: &Agent,
    end: &SessionEnd,
) -> Result<(), OrchestratorError> {
    let mut causes: Vec<&str> = Vec::new();
    match end.reason {
        EndReason::Limit => causes.push("a limit"),
        EndReason::ProviderLimit => causes.push("its model provider's usage limit"),
        EndReason::Completed | EndReason::Aborted | EndReason::Error => {}
    }
    for scope in &end.crossed {
        causes.push(match scope {
            BudgetScope::SessionTokens => "the session's input or output tokens",
            BudgetScope::SessionWallClock => "the session's wall clock",
            BudgetScope::SessionToolCalls => "the session's tool calls",
            BudgetScope::TaskUsd
            | BudgetScope::TaskSessions
            | BudgetScope::SprintUsd
            | BudgetScope::DayUsd => continue,
        });
    }
    if causes.is_empty() {
        return Ok(());
    }
    let tools = &deps.tools;
    let history = tools.log.read(&EventQuery {
        task_id: Some(contract.id.clone()),
        kinds: vec![EventKind::SessionStarted, EventKind::NoteWritten],
        ..EventQuery::default()
    })?;
    let since = history
        .iter()
        .rposition(|event| {
            matches!(event.body, EventBody::SessionStarted(_))
                && event.envelope.ids.session_id.as_deref() == Some(end.session_id.as_str())
        })
        .map_or(history.len(), |at| at + 1);
    let own = history[since..]
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            EventBody::NoteWritten(body)
                if body.kind == NoteWrittenBodyKind::Progress
                    && body.written_by == agent.id.as_str() =>
            {
                Some(body.text.as_str())
            }
            _ => None,
        });
    let quoted = own.map_or_else(String::new, |text| {
        format!(" Your last note in it: {}.", text.trim_end_matches('.'))
    });
    let text = format!(
        "This session stopped at {}: {}.{quoted} Resume from the last commit and this note.",
        causes.join(" and "),
        end.detail.trim_end_matches('.')
    );
    append(
        tools,
        &contract.id,
        None,
        Some(end.session_id.clone()),
        EventBody::NoteWritten(NoteWrittenBody {
            kind: NoteWrittenBodyKind::Progress,
            text,
            written_by: FARIK.to_string(),
        }),
    )
}

/// The Farik tools a read-only session is not offered: the command runner, which has no
/// executor there, and the git writes, which only the assignee may make.
const NOT_FOR_READ_ONLY: [&str; 3] = ["farik_exec", "farik_git_commit", "farik_git_push"];

/// The spec of the session `ask` describes, its prompt assembled from the files as they are now,
/// with what the human said about its task since its last session started. A triage session runs
/// on `TRIAGE_MODEL` at low effort. A session asked with one tool is given it alone, whatever the
/// agent's tiers, and no built-in tool.
fn session_spec(
    deps: &OrchestratorDeps,
    team: &Team,
    ask: &SessionAsk<'_>,
) -> Result<SessionSpec, OrchestratorError> {
    let files = &deps.tools.files;
    let role_id = Role::from(ask.agent.role);
    let role = load_role(role_id)?;
    // 5.16 runs triage on the cheaper model, whatever the agent's own.
    let (model, effort) = if ask.purpose == SessionPurpose::Triage {
        (TRIAGE_MODEL.to_string(), Effort::Low)
    } else {
        session_model(ask.agent, &role)
    };
    // A project that was never scanned, or whose scan cannot be read, is given none.
    let project_scan = files.read_project_scan().ok();
    let memory = files.read_memory(&ask.agent.id)?;
    let criteria = files.read_criteria()?;
    let tiers: BTreeSet<PermissionTier> = ask.agent.tiers().into_iter().collect();
    let builtin_tools = if ask.only_tool.is_some() {
        Vec::new()
    } else if ask.read_only {
        allowed_builtins(&BTreeSet::from([PermissionTier::Read]))
    } else {
        allowed_builtins(&tiers)
    };
    // A verify session judges the work and does not change it (step 12): it has no executor, and
    // the git writes are refused to all but the assignee, so it is not offered them.
    let tools: Vec<FarikTool> = tool_descriptors()
        .into_iter()
        .filter(|tool| !(ask.read_only && NOT_FOR_READ_ONLY.contains(&tool.name)))
        .filter(|tool| ask.only_tool.is_none_or(|only| tool.name == only))
        .collect();
    // A session given one tool has it whatever the agent's tiers.
    let farik_tools = tools
        .iter()
        .filter(|tool| ask.only_tool.is_some() || tiers.contains(&tool.tier))
        .map(|tool| tool.name.to_string())
        .collect();
    // A session about no task has no human message, and the whole log is not read for one.
    let human = match ask.contract {
        Some(contract) => human_message(&deps.tools.log.read(&EventQuery {
            task_id: Some(contract.id.clone()),
            ..EventQuery::default()
        })?),
        None => None,
    };
    let rules = team.rules();
    let system_prompt = assemble_system_prompt(&PromptInput {
        role: &role,
        agent: ask.agent,
        project_scan: project_scan.as_deref(),
        memory: &memory,
        rules: &rules,
        criteria: &criteria,
        contract: ask.contract,
        tools: &tools,
        builtin_tools: &builtin_tools,
        purpose: ask.purpose,
        human_message: human.as_deref(),
        closing: match ask.only_tool {
            Some(JUDGMENT_TOOL) => Some(JUDGMENT_INSTRUCTION),
            Some(SPRINT_PLAN_TOOL) => Some(SPRINT_PLAN_INSTRUCTION),
            _ => None,
        },
    })?;
    let limits = budget_state(
        &deps.tools.projections,
        team,
        role_id,
        ask.contract,
        &SessionLedger::default(),
        deps.tools.clock.now(),
    )?
    .session_limits;
    Ok(SessionSpec {
        session_id: deps.session_ids.session_id(),
        agent_id: ask.agent.id.to_string(),
        task_id: ask.contract.map(|contract| contract.id.clone()),
        purpose: ask.purpose,
        system_prompt,
        model,
        effort,
        farik_tools,
        builtin_tools,
        mcp_servers: Vec::new(),
        cwd: ask.cwd.clone(),
        limits,
        initial_prompt: ask.initial_prompt.clone(),
    })
}

/// Starts the session, reads it to its end, and records its start, its costs, and its end. A
/// session that cannot start, or that ends without reporting usage, is recorded as costing
/// nothing, so that it counts as one of the task's sessions. Each usage report is costed and the
/// budgets it exhausts recorded; the session is aborted when one of them is not the task's
/// sessions, which crosses at the last session a task is allowed, a session to finish rather than
/// to cut. Once the session is started, whatever fails ends it: it is aborted, and its zero cost
/// and its end are recorded as far as they can be before the error is returned, so that no
/// session runs on, or goes uncounted, after the tick has given up on it.
async fn drive(
    deps: &OrchestratorDeps,
    team: &Team,
    role: Role,
    contract: Option<&TaskContract>,
    spec: &SessionSpec,
) -> Result<SessionEnd, OrchestratorError> {
    let tools = &deps.tools;
    let clock = &*tools.clock;
    let ids = EventIds {
        task_id: spec.task_id.clone(),
        agent_id: Some(spec.agent_id.clone()),
        session_id: Some(spec.session_id.clone()),
        ..tools.ids.clone()
    };
    let prices = tools.files.effective_prices()?;
    let source = CostSource {
        ids: ids.clone(),
        purpose: spec.purpose,
        model_id: &spec.model,
    };
    let cost = |usage: &Usage| {
        record_session_cost(
            &tools.log,
            &tools.projections,
            &source,
            usage,
            &prices,
            clock,
        )
    };
    record_session_started(&tools.log, spec, &tools.ids, clock)?;
    let mut costed = false;
    let mut crossed = Vec::new();
    let read = match deps.adapter.start_session(spec.clone()) {
        Ok(mut handle) => {
            let running = Running {
                deps,
                team,
                role,
                contract,
                spec,
                ids: &ids,
            };
            let read = read_to_end(&running, &mut *handle, &cost, &mut costed, &mut crossed).await;
            if read.is_err() {
                // The error is what the tick reports; an abort that fails too adds nothing to it.
                let _ = handle.abort();
            }
            read
        }
        Err(error) => Err(error.into()),
    };
    let (reason, detail) = match &read {
        Ok((reason, detail, _)) => (*reason, detail.clone()),
        Err(error) => (EndReason::Error, error.to_string()),
    };
    let zero = if costed {
        Ok(())
    } else {
        cost(&Usage::default()).map(|_| ())
    };
    let ended = record_session_ended(&tools.log, &spec.session_id, reason, &detail, &ids, clock);
    // A session that failed is reported by what failed it; its zero cost and its end are
    // recorded as far as they can be, and their own failures would only hide the first.
    let (_, _, resets_at) = read?;
    zero?;
    ended?;
    Ok(SessionEnd {
        session_id: spec.session_id.clone(),
        reason,
        detail,
        resets_at,
        crossed,
    })
}

/// A started session, and what its budgets are read against.
#[derive(Clone, Copy)]
struct Running<'a> {
    deps: &'a OrchestratorDeps,
    team: &'a Team,
    role: Role,
    contract: Option<&'a TaskContract>,
    spec: &'a SessionSpec,
    ids: &'a EventIds,
}

/// Reads the started session's events until it ends, costing each usage report, recording the
/// budgets it exhausts, and aborting the session when one of them is not the task's sessions.
/// The session is also aborted, once, as soon as the daemon has been asked to stop it: by the
/// human's `SessionStop` or pause, or by a hook that found its agent no longer active (5.2, F1).
/// `costed` says whether a usage report was costed, and `crossed` gathers the budgets crossed.
async fn read_to_end(
    running: &Running<'_>,
    handle: &mut dyn SessionHandle,
    cost: &impl Fn(&Usage) -> Result<f64, CostError>,
    costed: &mut bool,
    crossed: &mut Vec<BudgetScope>,
) -> Result<(EndReason, String, Option<DateTime<Utc>>), OrchestratorError> {
    let Running {
        deps,
        team,
        role,
        contract,
        spec,
        ids,
    } = *running;
    let tools = &deps.tools;
    let clock = &*tools.clock;
    let started_at = clock.now();
    let state = |ledger: &SessionLedger| {
        budget_state(
            &tools.projections,
            team,
            role,
            contract,
            ledger,
            clock.now(),
        )
    };
    let mut ledger = SessionLedger::default();
    let mut stopped = false;
    loop {
        // Taken before the stop is read, so that a stop requested between the two still wakes
        // the wait below.
        let notified = deps.daemon.stops().notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !stopped && deps.daemon.stop_reason(&spec.session_id).is_some() {
            stopped = true;
            handle.abort()?;
        }
        let event = tokio::select! {
            event = handle.events().recv() => event,
            () = &mut notified, if !stopped => continue,
        };
        match event {
            Some(SessionEvent::UsageReported(usage)) => {
                let before = state(&ledger)?;
                let cost_usd = cost(&usage)?;
                *costed = true;
                ledger = SessionLedger {
                    tool_calls: deps
                        .daemon
                        .tool_calls(&spec.session_id)
                        .unwrap_or(ledger.tool_calls),
                    wall_clock: (clock.now() - started_at).to_std().unwrap_or_default(),
                    ..add_usage(&ledger, &usage, cost_usd)
                };
                let after = state(&ledger)?;
                let now =
                    record_exhaustion(&tools.log, &tools.projections, &before, &after, ids, clock)?;
                crossed.extend(now.iter().map(|exhausted| exhausted.scope));
                // A task's last session and in-progress work past the sprint's budget may finish
                // (5.5): what those stop is the next session and the next assignment.
                if now.iter().any(|exhausted| {
                    !matches!(
                        exhausted.scope,
                        BudgetScope::TaskSessions | BudgetScope::SprintUsd
                    )
                }) {
                    handle.abort()?;
                }
            }
            Some(SessionEvent::Ended {
                reason,
                detail,
                resets_at,
            }) => return Ok((reason, detail, resets_at)),
            Some(_) => {}
            None => {
                return Ok((
                    EndReason::Error,
                    "the session's events stopped without an end".to_string(),
                    None,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::orchestrator::fixtures::Harness;
    use crate::session::SessionPurpose;

    use super::{SPRINT_PLAN_TOOL, SessionAsk, session_spec};
    use crate::prompt::SPRINT_PLAN_INSTRUCTION;

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn gives_a_one_tool_session_that_tool_alone() {
        let harness = Harness::new("session-one-tool", |_| {});
        harness.file("FRK-1", "refining", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let deps = &orchestrator.deps;
        let team = deps.tools.files.read_team().expect("the team");
        let contract = deps
            .tools
            .files
            .read_contract(&"FRK-1".parse().expect("a task id"))
            .expect("the contract");
        let pm = team.active_agents().next().expect("an agent");

        let spec = session_spec(
            deps,
            &team,
            &SessionAsk {
                agent: pm,
                contract: Some(&contract),
                purpose: SessionPurpose::Refine,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: false,
                only_tool: Some("farik_record_judgment"),
                initial_prompt: String::new(),
            },
        )
        .expect("the spec");

        assert_eq!(spec.farik_tools, vec!["farik_record_judgment".to_string()]);
        assert!(spec.builtin_tools.is_empty(), "{:?}", spec.builtin_tools);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn closes_a_sprint_planning_session_with_its_own_instruction() {
        let harness = Harness::new("session-sprint-plan", |_| {});
        let orchestrator = harness.orchestrator(harness.recorded(Vec::new()));
        let deps = &orchestrator.deps;
        let team = deps.tools.files.read_team().expect("the team");
        let pm = team.active_agents().next().expect("an agent");

        let spec = session_spec(
            deps,
            &team,
            &SessionAsk {
                agent: pm,
                contract: None,
                purpose: SessionPurpose::Plan,
                cwd: deps.tools.files.root().to_path_buf(),
                executor: None,
                read_only: false,
                only_tool: Some(SPRINT_PLAN_TOOL),
                initial_prompt: String::new(),
            },
        )
        .expect("the spec");

        assert!(
            spec.system_prompt
                .trim_end()
                .ends_with(SPRINT_PLAN_INSTRUCTION),
            "{}",
            spec.system_prompt
        );
        // The role's own words may name it; the tools and the instruction do not.
        let from_the_tools = &spec.system_prompt[spec
            .system_prompt
            .find("## Your tools")
            .expect("a tools section")..];
        assert!(
            !from_the_tools.contains("farik_assign_task"),
            "{from_the_tools}"
        );
        assert_eq!(spec.task_id, None);
        assert_eq!(spec.farik_tools, vec![SPRINT_PLAN_TOOL.to_string()]);
    }
}
