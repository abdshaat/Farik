//! One session, start to end, the same way for every purpose: prompt, registration, start, the
//! costs it reports, and its end.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use farik_core::budget::{BudgetScope, SessionLedger, add_usage};
use farik_core::contract::{Role, TaskContract};
use farik_core::governor::permissions::PermissionTier;
use farik_core::pricing::Usage;
use farik_core::team::{Agent, Team};
use farik_protocol::event::EventIds;
use farik_roles::load_role;

use super::{OrchestratorDeps, OrchestratorError};
use crate::claude::allowed_builtins;
use crate::cost::{CostSource, budget_state, record_exhaustion, record_session_cost};
use crate::daemon::SessionRegistration;
use crate::exec::Executor;
use crate::prompt::{PromptInput, assemble_system_prompt};
use crate::session::{EndReason, SessionEvent, SessionPurpose, SessionSpec};
use crate::sessions::{record_session_ended, record_session_started};
use crate::tools::tool_descriptors;

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
    /// Whether it gets the read tier's built-ins alone, whatever the agent's tiers: a verify
    /// session reads the work and does not change it.
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

/// The spec of the session `ask` describes, its prompt assembled from the files as they are now.
fn session_spec(
    deps: &OrchestratorDeps,
    team: &Team,
    ask: &SessionAsk<'_>,
) -> Result<SessionSpec, OrchestratorError> {
    let files = &deps.tools.files;
    let role_id = Role::from(ask.agent.role);
    let role = load_role(role_id)?;
    let (model, effort) = match &ask.agent.model {
        Some(model) => (model.id.to_string(), model.effort.unwrap_or(role.effort)),
        None => (role.model.clone(), role.effort),
    };
    // A project that was never scanned, or whose scan cannot be read, is given none.
    let project_scan = files.read_project_scan().ok();
    let memory = files.read_memory(&ask.agent.id)?;
    let criteria = files.read_criteria()?;
    let tiers: BTreeSet<PermissionTier> = ask.agent.tiers().into_iter().collect();
    let builtin_tools = if ask.read_only {
        allowed_builtins(&BTreeSet::from([PermissionTier::Read]))
    } else {
        allowed_builtins(&tiers)
    };
    let tools = tool_descriptors();
    let farik_tools = tools
        .iter()
        .filter(|tool| tiers.contains(&tool.tier))
        .map(|tool| tool.name.to_string())
        .collect();
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
        human_message: None,
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
/// to cut.
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
    let mut handle = match deps.adapter.start_session(spec.clone()) {
        Ok(handle) => handle,
        Err(error) => {
            cost(&Usage::default())?;
            record_session_ended(
                &tools.log,
                &spec.session_id,
                EndReason::Error,
                &error.to_string(),
                &ids,
                clock,
            )?;
            return Err(error.into());
        }
    };
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
    let mut costed = false;
    let (reason, detail) = loop {
        match handle.events().recv().await {
            Some(SessionEvent::UsageReported(usage)) => {
                let before = state(&ledger)?;
                let cost_usd = cost(&usage)?;
                costed = true;
                ledger = SessionLedger {
                    tool_calls: deps
                        .daemon
                        .tool_calls(&spec.session_id)
                        .unwrap_or(ledger.tool_calls),
                    wall_clock: (clock.now() - started_at).to_std().unwrap_or_default(),
                    ..add_usage(&ledger, &usage, cost_usd)
                };
                let after = state(&ledger)?;
                let crossed = record_exhaustion(
                    &tools.log,
                    &tools.projections,
                    &before,
                    &after,
                    &ids,
                    clock,
                )?;
                if crossed
                    .iter()
                    .any(|exhausted| exhausted.scope != BudgetScope::TaskSessions)
                {
                    handle.abort()?;
                }
            }
            Some(SessionEvent::Ended { reason, detail }) => break (reason, detail),
            Some(_) => {}
            None => {
                break (
                    EndReason::Error,
                    "the session's events stopped without an end".to_string(),
                );
            }
        }
    };
    if !costed {
        cost(&Usage::default())?;
    }
    record_session_ended(&tools.log, &spec.session_id, reason, &detail, &ids, clock)?;
    Ok((reason, detail))
}
