//! One session, start to end, the same way for every purpose: prompt, registration, start, the
//! costs it reports, and its end.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use farik_core::budget::{BudgetScope, SessionLedger, add_usage};
use farik_core::contract::{Role, TaskContract};
use farik_core::governor::permissions::PermissionTier;
use farik_core::pricing::Usage;
use farik_core::team::{Agent, Effort, Team};
use farik_protocol::event::EventIds;
use farik_roles::load_role;
use farik_store::EventQuery;

use super::messages::human_message;
use super::{OrchestratorDeps, OrchestratorError, TRIAGE_MODEL};
use crate::claude::allowed_builtins;
use crate::cost::{CostError, CostSource, budget_state, record_exhaustion, record_session_cost};
use crate::daemon::SessionRegistration;
use crate::exec::Executor;
use crate::prompt::{PromptInput, assemble_system_prompt};
use crate::session::{
    EndReason, SessionEvent, SessionHandle, SessionPurpose, SessionSpec, session_model,
};
use crate::sessions::{record_session_ended, record_session_started};
use crate::tools::{FarikTool, tool_descriptors};

/// The one tool a triage session is given.
const TRIAGE_TOOL: &str = "farik_triage_request";

/// What a rule asks a session for.
pub(super) struct SessionAsk<'a> {
    /// The agent the session is.
    pub(super) agent: &'a Agent,
    /// The task it is about.
    pub(super) contract: &'a TaskContract,
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
    let (reason, detail) = ended?;
    Ok(SessionEnd {
        session_id: spec.session_id,
        reason,
        detail,
    })
}

/// The Farik tools a read-only session is not offered: the command runner, which has no
/// executor there, and the git writes, which only the assignee may make.
const NOT_FOR_READ_ONLY: [&str; 3] = ["farik_exec", "farik_git_commit", "farik_git_push"];

/// The spec of the session `ask` describes, its prompt assembled from the files as they are now,
/// with what the human said about its task since its last session started. A triage session runs
/// on `TRIAGE_MODEL` at low effort with `farik_triage_request` alone and no built-in tool.
fn session_spec(
    deps: &OrchestratorDeps,
    team: &Team,
    ask: &SessionAsk<'_>,
) -> Result<SessionSpec, OrchestratorError> {
    let files = &deps.tools.files;
    let role_id = Role::from(ask.agent.role);
    let role = load_role(role_id)?;
    // 5.16 runs triage on the cheaper model, whatever the agent's own, with its one tool.
    let triage = ask.purpose == SessionPurpose::Triage;
    let (model, effort) = if triage {
        (TRIAGE_MODEL.to_string(), Effort::Low)
    } else {
        session_model(ask.agent, &role)
    };
    // A project that was never scanned, or whose scan cannot be read, is given none.
    let project_scan = files.read_project_scan().ok();
    let memory = files.read_memory(&ask.agent.id)?;
    let criteria = files.read_criteria()?;
    let tiers: BTreeSet<PermissionTier> = ask.agent.tiers().into_iter().collect();
    let builtin_tools = if triage {
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
        .collect();
    let farik_tools = if triage {
        vec![TRIAGE_TOOL.to_string()]
    } else {
        tools
            .iter()
            .filter(|tool| tiers.contains(&tool.tier))
            .map(|tool| tool.name.to_string())
            .collect()
    };
    let history = deps.tools.log.read(&EventQuery {
        task_id: Some(ask.contract.id.clone()),
        ..EventQuery::default()
    })?;
    let human = human_message(&history);
    let rules = team.rules();
    let system_prompt = assemble_system_prompt(&PromptInput {
        role: &role,
        agent: ask.agent,
        project_scan: project_scan.as_deref(),
        memory: &memory,
        rules: &rules,
        criteria: &criteria,
        contract: Some(ask.contract),
        tools: &tools,
        builtin_tools: &builtin_tools,
        purpose: ask.purpose,
        human_message: human.as_deref(),
    })?;
    let limits = budget_state(
        &deps.tools.projections,
        team,
        role_id,
        Some(ask.contract),
        &SessionLedger::default(),
        deps.tools.clock.now(),
    )?
    .session_limits;
    Ok(SessionSpec {
        session_id: deps.session_ids.session_id(),
        agent_id: ask.agent.id.to_string(),
        task_id: Some(ask.contract.id.clone()),
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
    contract: &TaskContract,
    spec: &SessionSpec,
) -> Result<(EndReason, String), OrchestratorError> {
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
            let read = read_to_end(&running, &mut *handle, &cost, &mut costed).await;
            if read.is_err() {
                // The error is what the tick reports; an abort that fails too adds nothing to it.
                let _ = handle.abort();
            }
            read
        }
        Err(error) => Err(error.into()),
    };
    let (reason, detail) = match &read {
        Ok((reason, detail)) => (*reason, detail.clone()),
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
    let read = read?;
    zero?;
    ended?;
    Ok(read)
}

/// A started session, and what its budgets are read against.
#[derive(Clone, Copy)]
struct Running<'a> {
    deps: &'a OrchestratorDeps,
    team: &'a Team,
    role: Role,
    contract: &'a TaskContract,
    spec: &'a SessionSpec,
    ids: &'a EventIds,
}

/// Reads the started session's events until it ends, costing each usage report, recording the
/// budgets it exhausts, and aborting the session when one of them is not the task's sessions.
/// The session is also aborted, once, as soon as the daemon has been asked to stop it: by the
/// human's `SessionStop` or pause, or by a hook that found its agent no longer active (5.2, F1).
/// `costed` says whether a usage report was costed.
async fn read_to_end(
    running: &Running<'_>,
    handle: &mut dyn SessionHandle,
    cost: &impl Fn(&Usage) -> Result<f64, CostError>,
    costed: &mut bool,
) -> Result<(EndReason, String), OrchestratorError> {
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
            Some(contract),
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
                let crossed =
                    record_exhaustion(&tools.log, &tools.projections, &before, &after, ids, clock)?;
                if crossed
                    .iter()
                    .any(|exhausted| exhausted.scope != BudgetScope::TaskSessions)
                {
                    handle.abort()?;
                }
            }
            Some(SessionEvent::Ended { reason, detail }) => return Ok((reason, detail)),
            Some(_) => {}
            None => {
                return Ok((
                    EndReason::Error,
                    "the session's events stopped without an end".to_string(),
                ));
            }
        }
    }
}
