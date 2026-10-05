//! Permission tiers (`docs/SPEC.md` section 5.6): what a tool call may do given the agent's
//! grants, the task's allowed paths, and the team's protected paths; and what a command may be
//! (`farik_exec`, ADR 0004 and spec 5.12).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::paths::{GlobError, PathRefusal, check_allowed_paths, check_protected_paths};
use super::team_rules::TeamRules;
use crate::contract::{Role, TaskId};
use crate::team::canonical_json;

/// A capability tier of `docs/SPEC.md` section 5.6, attached to a role and overridable per
/// agent. Serialised in `snake_case`, as on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionTier {
    /// Read files in the project and `.farik/`, and search.
    Read,
    /// Write files under the task contract's `allowed_paths`.
    WriteWorkspace,
    /// Run commands inside the sandbox.
    Execute,
    /// Outbound HTTP from the sandbox, web search.
    Network,
    /// Commit on a task branch.
    GitLocal,
    /// Push, open pull requests.
    GitRemote,
    /// Anything that changes state outside the sandbox; each use needs the human's approval
    /// unless the tool is pre-authorised.
    ExternalEffect,
}

/// The tiers a role holds by default (`docs/SPEC.md` section 5.6). Everyone reads; the human is
/// not an agent and gets the same read-only default.
#[must_use]
pub fn default_tiers(role: Role) -> &'static [PermissionTier] {
    use PermissionTier as T;
    match role {
        Role::SoftwareDeveloper | Role::UiUxDesigner => {
            &[T::Read, T::WriteWorkspace, T::Execute, T::GitLocal]
        }
        Role::Architect => &[
            T::Read,
            T::WriteWorkspace,
            T::Execute,
            T::Network,
            T::GitLocal,
        ],
        Role::ProductManager | Role::FinanceSpecialist => &[T::Read, T::Network],
        Role::MarketingSpecialist => &[T::Read, T::Network, T::WriteWorkspace, T::GitLocal],
        Role::ScrumMaster | Role::Human => &[T::Read],
    }
}

/// A tool as the runtime describes it to the governor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// The tool's name, as the agent calls it.
    pub name: String,
    /// The tier the tool needs.
    pub tier: PermissionTier,
}

/// One tool call, as the `PreToolUse` hook reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallRequest {
    /// The tool being called.
    pub tool: ToolDescriptor,
    /// The paths the call touches, relative to the project root.
    pub paths: Vec<String>,
    /// A hash of the call's input, which identifies one approved external effect.
    pub input_hash: String,
}

/// What an agent may do: its tiers and the external tools the user pre-authorised for it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentGrants {
    /// The tiers the agent holds, from its role's defaults and the user's overrides.
    pub tiers: BTreeSet<PermissionTier>,
    /// External-effect tools that need no per-call approval.
    pub preauthorized_external_tools: BTreeSet<String>,
}

/// One external-effect call the human approved, by tool name and input hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedCall {
    /// The tool's name.
    pub tool: String,
    /// The hash of the approved input.
    pub input_hash: String,
}

/// What the governor knows about the task and the team when a tool call arrives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolCallContext {
    /// The task contract's `allowed_paths`.
    pub allowed_paths: Vec<String>,
    /// The team's `protected_paths` (spec 5.12).
    pub protected_paths: Vec<String>,
    /// The external-effect calls the human has approved.
    pub approved_calls: Vec<ApprovedCall>,
}

/// Why a tool call is refused. The first reason found, in the order tier, protected paths,
/// allowed paths, approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRefusal {
    /// The agent does not hold the tool's tier.
    TierNotGranted {
        /// The tier the tool needs.
        tier: PermissionTier,
    },
    /// A `write_workspace` call touches a path outside the contract's `allowed_paths`.
    PathOutsideAllowed {
        /// The first such path.
        path: String,
    },
    /// The call touches a protected path, whatever its tier; an absolute, empty, or climbing
    /// path is refused under this reason too, because the path check refuses it outright.
    PathProtected {
        /// The first such path.
        path: String,
    },
    /// A `write_workspace` call named no path, so nothing could be checked; the hook must extract
    /// the paths a write touches.
    PathsMissing,
    /// A glob in the allowed or protected paths does not compile; nothing was checked.
    InvalidGlob {
        /// The pattern as written.
        pattern: String,
        /// What is wrong with it.
        detail: String,
    },
    /// An external effect that the human has not approved for this input and that is not
    /// pre-authorised.
    RequiresHumanApproval {
        /// The tool's name.
        tool: String,
    },
    /// A UI/UX Designer's `write_workspace`, `execute`, `git_local` or `git_remote` call on a task
    /// whose design plan the Product Manager has not approved.
    DesignPlanNotApproved,
}

/// A connector tool's tag (`docs/SPEC.md` section 5.6, role-kits' vocabulary), in `snake_case` as
/// in a connector's definition and on `tool.called` and `tool.denied`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorTag {
    /// Reaches only the preview and changes nothing outside the sandbox.
    Network,
    /// Changes state outside the sandbox; runs once for each call the human allows.
    ExternalEffect,
    /// Never offered and always refused.
    Denied,
}

/// A connector a session was given: its server's name, the one origin its calls may name when it
/// is confined to the preview, and each tool's tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConnector {
    /// The server's name, as in `mcp__<server>__<tool>`.
    pub server: String,
    /// The preview's origin, `http://localhost:<port>`, for a connector confined to the preview;
    /// `None` for a user's server, whose `network` tools may name any `url`.
    pub origin: Option<String>,
    /// Every tool the connector's pinned list tags.
    pub tools: std::collections::BTreeMap<String, ConnectorTag>,
    /// For each `external_effect` tool with one, how many calls the agent makes each period
    /// without asking (ADR 0037), by the bare tool name.
    pub allowances: std::collections::BTreeMap<String, u32>,
}

/// The most calls an allowance may be: what the schema and the daemon hold it to.
pub const MAX_ALLOWANCE: u32 = 1000;

/// What let a connector call run (`evaluate_connector_call`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectorPass {
    /// The tool's tag.
    pub tag: ConnectorTag,
    /// The seq of the human's grant this call uses up, when a grant allowed it.
    pub approval: Option<u64>,
    /// The allowance the call ran inside, when that allowed it and no grant did.
    pub allowance: Option<u32>,
}

/// The largest input, in bytes of its compact JSON, that the human is asked to allow: anything
/// longer is too long to show, so it is refused without asking.
pub const MAX_APPROVAL_INPUT: usize = 64 * 1024;

/// The sha256, in lower-case hex, of `canonical_json(input)`: what binds a human's grant to the
/// one input they saw, whatever order its keys arrive in.
#[must_use]
pub fn input_sha256(input: &serde_json::Value) -> String {
    crate::team::sha256_hex(&canonical_json(input))
}

/// What a human's grant of one `external_effect` call is bound to (ADR 0031): the asking agent,
/// the task, the server, the tool and the input's `input_sha256`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalKey {
    /// The agent that asked.
    pub agent_id: String,
    /// The task it asked about.
    pub task_id: TaskId,
    /// The connector's server.
    pub server: String,
    /// The bare tool name.
    pub tool: String,
    /// `input_sha256` of the call's input.
    pub input_sha256: String,
}

/// Why a connector call is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorRefusal {
    /// The session was given no such connector.
    ConnectorNotInSession,
    /// The connector's pinned list does not tag the tool.
    ToolNotTagged,
    /// The tool is tagged `denied`.
    ToolDenied,
    /// The tool is tagged `external_effect` and the human has not allowed this call: it waits.
    ApprovalNeeded,
    /// The tool is tagged `external_effect` and its input is over `MAX_APPROVAL_INPUT`, too long
    /// to show the human, so it is refused without asking.
    InputTooLarge,
    /// A `url` field names something other than the preview.
    UrlOutsidePreview {
        /// The field's value: the string, or the JSON of a value that is not one.
        url: String,
    },
}

/// Decides one connector call (`docs/SPEC.md` sections 5.6 and 8.6): the session must have the
/// connector, the tool must be tagged `network` or `external_effect`, and, for a connector with an
/// origin, every field named `url`, at any depth of the input, must be a string naming the
/// preview: its origin exactly, or followed by `/`, `?` or `#`. An `external_effect` call also
/// needs `granted`, the seq of the human's open grant for exactly this call, which the caller
/// looks up by `ApprovalKey`; it is returned beside the tag, so the call's record can use it up.
/// Failing that, a tool with an allowance runs while `used`, the agent's calls to it this period
/// before this one, is under it (ADR 0037): the grant is for exactly this call, so it comes
/// first. A `network` call ignores both. The tag governs whatever the agent's tiers.
/// `tool` is the bare tool name, without `mcp__<server>__`.
///
/// # Errors
///
/// `ConnectorNotInSession`, then `ToolNotTagged`, then `ToolDenied` for a `denied` tool, then
/// `UrlOutsidePreview` with the first `url` found outside the origin, then, for an
/// `external_effect` tool, `InputTooLarge` for an input over `MAX_APPROVAL_INPUT`, grant or not,
/// and `ApprovalNeeded` without a grant or an allowance with a place left.
pub fn evaluate_connector_call(
    tool: &str,
    input: &serde_json::Value,
    connector: Option<&SessionConnector>,
    granted: Option<u64>,
    used: u32,
) -> Result<ConnectorPass, ConnectorRefusal> {
    let connector = connector.ok_or(ConnectorRefusal::ConnectorNotInSession)?;
    let tag = match connector.tools.get(tool) {
        None => return Err(ConnectorRefusal::ToolNotTagged),
        Some(ConnectorTag::Denied) => return Err(ConnectorRefusal::ToolDenied),
        Some(tag) => *tag,
    };
    if let Some(origin) = &connector.origin {
        check_urls(input, origin)?;
    }
    let pass = |approval, allowance| ConnectorPass {
        tag,
        approval,
        allowance,
    };
    if tag == ConnectorTag::Network {
        return Ok(pass(None, None));
    }
    if canonical_json(input).len() > MAX_APPROVAL_INPUT {
        return Err(ConnectorRefusal::InputTooLarge);
    }
    if let Some(approval) = granted {
        return Ok(pass(Some(approval), None));
    }
    connector
        .allowances
        .get(tool)
        .filter(|allowed| used < **allowed)
        .map(|allowed| pass(None, Some(*allowed)))
        .ok_or(ConnectorRefusal::ApprovalNeeded)
}

fn check_urls(value: &serde_json::Value, origin: &str) -> Result<(), ConnectorRefusal> {
    use serde_json::Value;
    match value {
        Value::Object(fields) => fields.iter().try_for_each(|(key, field)| {
            if key == "url" {
                let inside = field.as_str().is_some_and(|url| {
                    url.strip_prefix(origin)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
                });
                if !inside {
                    return Err(ConnectorRefusal::UrlOutsidePreview {
                        url: field
                            .as_str()
                            .map_or_else(|| field.to_string(), str::to_string),
                    });
                }
            }
            check_urls(field, origin)
        }),
        Value::Array(items) => items.iter().try_for_each(|item| check_urls(item, origin)),
        _ => Ok(()),
    }
}

/// The Product Manager's plan gate (ADR 0026): a UI/UX Designer changes nothing before its
/// task's latest design plan is approved. `approved` is what the log says at the call.
///
/// # Errors
///
/// `DesignPlanNotApproved` for a Designer's `write_workspace`, `execute`, `git_local`,
/// `git_remote` or `external_effect` call while the plan is not approved: nothing to push without
/// a commit, nothing to change outside before the plan, and the safe side all the same.
pub fn check_design_plan(
    role: Role,
    tier: PermissionTier,
    approved: bool,
) -> Result<(), ToolRefusal> {
    let writes = matches!(
        tier,
        PermissionTier::WriteWorkspace
            | PermissionTier::Execute
            | PermissionTier::GitLocal
            | PermissionTier::GitRemote
            | PermissionTier::ExternalEffect
    );
    if role == Role::UiUxDesigner && writes && !approved {
        return Err(ToolRefusal::DesignPlanNotApproved);
    }
    Ok(())
}

/// Decides one tool call (`docs/SPEC.md` section 5.6): the agent must hold the tool's tier;
/// no path may be protected, even for a `read` tool; a `write_workspace` call names at least one
/// path and stays within the contract's allowed paths; an `external_effect` call needs the tool
/// to be pre-authorised or this exact input approved by the human.
///
/// # Errors
///
/// The first refusal found, in that order.
pub fn evaluate_tool_call(
    request: &ToolCallRequest,
    grants: &AgentGrants,
    context: &ToolCallContext,
) -> Result<(), ToolRefusal> {
    let tier = request.tool.tier;
    if !grants.tiers.contains(&tier) {
        return Err(ToolRefusal::TierNotGranted { tier });
    }
    check_protected_paths(&request.paths, &context.protected_paths)
        .map_err(|refusal| first_path(refusal, |path| ToolRefusal::PathProtected { path }))?;
    if tier == PermissionTier::WriteWorkspace {
        if request.paths.is_empty() {
            return Err(ToolRefusal::PathsMissing);
        }
        check_allowed_paths(&request.paths, &context.allowed_paths).map_err(|refusal| {
            first_path(refusal, |path| ToolRefusal::PathOutsideAllowed { path })
        })?;
    }
    if tier == PermissionTier::ExternalEffect
        && !grants
            .preauthorized_external_tools
            .contains(&request.tool.name)
        && !context.approved_calls.iter().any(|approved| {
            approved.tool == request.tool.name && approved.input_hash == request.input_hash
        })
    {
        return Err(ToolRefusal::RequiresHumanApproval {
            tool: request.tool.name.clone(),
        });
    }
    Ok(())
}

fn first_path(refusal: PathRefusal, to_refusal: impl FnOnce(String) -> ToolRefusal) -> ToolRefusal {
    match refusal {
        PathRefusal::Violations(violations) => to_refusal(
            violations
                .into_iter()
                .next()
                .map(|violation| violation.path)
                .unwrap_or_default(),
        ),
        PathRefusal::Glob(GlobError::Invalid { pattern, detail }) => {
            ToolRefusal::InvalidGlob { pattern, detail }
        }
    }
}

/// Why a command is refused by `farik_exec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRefusal {
    /// The command matches one of the team's forbidden patterns (spec 5.12).
    ForbiddenCommand {
        /// The pattern it matched.
        pattern: String,
    },
    /// The command runs `git`, which is a Farik tool of its own (ADR 0004).
    GitViaExec,
    /// A forbidden pattern is not a valid regular expression; nothing was checked.
    InvalidPattern {
        /// The pattern as written.
        pattern: String,
        /// What is wrong with it.
        detail: String,
    },
}

/// Words that run the command that follows them without changing what it is.
const WRAPPERS: [&str; 11] = [
    "env", "sudo", "doas", "command", "exec", "nohup", "time", "timeout", "nice", "setsid", "xargs",
];

/// Decides whether `farik_exec` may run a command. The command is split into segments on `&&`,
/// `||`, `;`, `|`, and newlines, without parsing quotes, so `echo "a; git push"` is refused too,
/// on the safe side. Each word is read bare: quotes and shell punctuation are stripped, a path
/// keeps only its last segment, and a `.exe` suffix is dropped, so `"git"`, `(git`, `./git`,
/// `C:\tools\git`, and `git.exe` are all `git`. A segment runs git when its first bare word is
/// `git`, or when its first bare word is a `NAME=value` assignment or one of the wrappers
/// (`env`, `sudo`, `doas`, `command`, `exec`, `nohup`, `time`, `timeout`, `nice`, `setsid`,
/// `xargs`) and `git` appears anywhere later in that segment, which catches `sudo -u root git
/// push` at the cost of refusing `sudo apt install git`. Git is a Farik tool with its own tiers
/// (ADR 0004); a call hidden in a subshell, a variable, or a script (`$(git push)`, `GIT=git;
/// $GIT push`, `sh -c "git push"`) is the residual that record accepts. A forbidden pattern is an
/// ECMAScript regular expression, which has no inline flags such as `(?i)`; every pattern is
/// compiled before anything is matched, and each is matched against the whole command and against
/// each non-blank segment, so that an anchor such as `^curl` applies per segment. The engine
/// backtracks, so a pathological pattern is the team's own cost.
///
/// # Errors
///
/// `InvalidPattern` when a pattern does not compile, then `GitViaExec`, then `ForbiddenCommand`
/// with the first pattern that matches.
pub fn evaluate_command(command: &str, rules: &TeamRules) -> Result<(), CommandRefusal> {
    let patterns = compile_patterns(&rules.forbidden_commands)?;
    let segments: Vec<&str> = command
        .split(['\n', ';', '|', '&'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.iter().copied().any(runs_git) {
        return Err(CommandRefusal::GitViaExec);
    }
    for (pattern, regex) in &patterns {
        if regex.find(command).is_some()
            || segments.iter().any(|segment| regex.find(segment).is_some())
        {
            return Err(CommandRefusal::ForbiddenCommand {
                pattern: (*pattern).clone(),
            });
        }
    }
    Ok(())
}

fn compile_patterns(patterns: &[String]) -> Result<Vec<(&String, regress::Regex)>, CommandRefusal> {
    patterns
        .iter()
        .map(|pattern| {
            let regex =
                regress::Regex::new(pattern).map_err(|error| CommandRefusal::InvalidPattern {
                    pattern: pattern.clone(),
                    detail: error.text,
                })?;
            Ok((pattern, regex))
        })
        .collect()
}

fn runs_git(segment: &str) -> bool {
    let words: Vec<&str> = segment.split_whitespace().filter_map(bare_word).collect();
    let Some((first, rest)) = words.split_first() else {
        return false;
    };
    if *first == "git" {
        return true;
    }
    if is_assignment(first) || WRAPPERS.contains(first) {
        return rest.contains(&"git");
    }
    false
}

/// The word as a command name: a redirection is not one, and quotes, shell punctuation, the
/// directories of a path, and a `.exe` suffix are not part of one.
fn bare_word(word: &str) -> Option<&str> {
    if word.starts_with('>') || word.starts_with('<') {
        return None;
    }
    let bare = word
        .trim_matches(|c| matches!(c, '(' | ')' | '{' | '}' | '!' | '"' | '\'' | '`' | ';'))
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let bare = bare.strip_suffix(".exe").unwrap_or(bare);
    if bare.is_empty() { None } else { Some(bare) }
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::{
        AgentGrants, ApprovedCall, CommandRefusal, ConnectorPass, ConnectorRefusal as Refused,
        ConnectorTag, MAX_APPROVAL_INPUT, PermissionTier as T, SessionConnector, ToolCallContext,
        ToolCallRequest, ToolDescriptor, ToolRefusal, check_design_plan, default_tiers,
        evaluate_command, evaluate_connector_call, evaluate_tool_call, input_sha256,
    };
    use crate::contract::Role;
    use crate::governor::team_rules::TeamRules;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// The decision as it was before allowances: nothing used, so no allowance applies.
    fn decide(
        tool: &str,
        input: &serde_json::Value,
        connector: Option<&SessionConnector>,
        granted: Option<u64>,
    ) -> Result<(ConnectorTag, Option<u64>), Refused> {
        evaluate_connector_call(tool, input, connector, granted, 0)
            .map(|pass| (pass.tag, pass.approval))
    }

    fn playwright() -> SessionConnector {
        SessionConnector {
            server: "playwright".to_string(),
            origin: Some("http://localhost:4400".to_string()),
            tools: [
                ("browser_navigate", ConnectorTag::Network),
                ("browser_click", ConnectorTag::Network),
                ("browser_evaluate", ConnectorTag::Denied),
                ("browser_send_email", ConnectorTag::ExternalEffect),
            ]
            .into_iter()
            .map(|(tool, tag)| (tool.to_string(), tag))
            .collect(),
            allowances: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn a_network_tool_runs_with_no_origin() {
        let github = SessionConnector {
            origin: None,
            ..playwright()
        };
        for url in [
            "https://api.github.com/search?q=x",
            "http://evil.test/",
            "/x",
        ] {
            assert_eq!(
                decide(
                    "browser_navigate",
                    &json!({ "url": url, "then": [{ "url": url }] }),
                    Some(&github),
                    None
                ),
                Ok((ConnectorTag::Network, None)),
                "{url}"
            );
        }
    }

    #[test]
    fn a_preview_connector_still_checks_urls() {
        let connector = playwright();
        let navigate =
            |input: serde_json::Value| decide("browser_navigate", &input, Some(&connector), None);
        assert_eq!(
            navigate(json!({ "url": "http://localhost:4400/x" })),
            Ok((ConnectorTag::Network, None))
        );
        assert_eq!(
            navigate(json!({ "url": "http://localhost:4400" })),
            Ok((ConnectorTag::Network, None))
        );
        assert_eq!(
            navigate(json!({ "url": "http://localhost:4400?q=1#top" })),
            Ok((ConnectorTag::Network, None))
        );
        assert_eq!(
            decide(
                "browser_click",
                &json!({ "ref": "e3" }),
                Some(&connector),
                None
            ),
            Ok((ConnectorTag::Network, None)),
            "a call with no url is judged by its tag alone"
        );
        // F2: only the preview's origin, exactly, followed by nothing or by `/`, `?` or `#`.
        for url in [
            "http://localhost:44001",
            "http://localhost:440",
            "http://localhost:4400@evil.test",
            "http://localhost:4400.evil.test/",
            "http://127.0.0.1:4400",
            "https://localhost:4400",
            "HTTP://localhost:4400",
            "/x",
        ] {
            assert_eq!(
                navigate(json!({ "url": url })),
                Err(Refused::UrlOutsidePreview {
                    url: url.to_string()
                }),
                "{url}"
            );
        }
        assert_eq!(
            navigate(
                json!({ "url": "http://localhost:4400/", "then": [{ "step": { "url": "http://evil.test/" } }] })
            ),
            Err(Refused::UrlOutsidePreview {
                url: "http://evil.test/".to_string()
            }),
            "every url, at any depth"
        );
        assert_eq!(
            navigate(json!({ "url": ["http://localhost:4400/"] })),
            Err(Refused::UrlOutsidePreview {
                url: r#"["http://localhost:4400/"]"#.to_string()
            }),
            "a url that is not a string"
        );
        assert_eq!(
            decide("browser_evaluate", &json!({}), Some(&connector), None),
            Err(Refused::ToolDenied)
        );
        assert_eq!(
            decide("browser_install", &json!({}), Some(&connector), None),
            Err(Refused::ToolNotTagged)
        );
        assert_eq!(
            decide(
                "browser_navigate",
                &json!({ "url": "http://localhost:4400/" }),
                None,
                None
            ),
            Err(Refused::ConnectorNotInSession)
        );
    }

    fn github() -> SessionConnector {
        SessionConnector {
            server: "github".to_string(),
            origin: None,
            tools: [
                ("search_issues", ConnectorTag::Network),
                ("create_issue", ConnectorTag::ExternalEffect),
                ("delete_repository", ConnectorTag::Denied),
            ]
            .into_iter()
            .map(|(tool, tag)| (tool.to_string(), tag))
            .collect(),
            allowances: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn external_effect_without_a_grant_needs_approval() {
        assert_eq!(
            decide(
                "create_issue",
                &json!({ "title": "x" }),
                Some(&github()),
                None
            ),
            Err(Refused::ApprovalNeeded)
        );
    }

    #[test]
    fn external_effect_with_a_grant_runs_and_names_it() {
        assert_eq!(
            decide(
                "create_issue",
                &json!({ "title": "x" }),
                Some(&github()),
                Some(7)
            ),
            Ok((ConnectorTag::ExternalEffect, Some(7)))
        );
    }

    #[test]
    fn denied_is_refused_even_with_a_grant() {
        assert_eq!(
            decide("delete_repository", &json!({}), Some(&github()), Some(7)),
            Err(Refused::ToolDenied)
        );
    }

    #[test]
    fn a_large_input_is_refused_not_asked() {
        // `{"b":"…"}` is 8 bytes around the string.
        let sized = |bytes: usize| json!({ "b": "x".repeat(bytes - 8) });
        assert_eq!(
            sized(MAX_APPROVAL_INPUT).to_string().len(),
            MAX_APPROVAL_INPUT
        );
        for granted in [None, Some(7)] {
            assert_eq!(
                decide(
                    "create_issue",
                    &sized(MAX_APPROVAL_INPUT + 1),
                    Some(&github()),
                    granted
                ),
                Err(Refused::InputTooLarge),
                "{granted:?}"
            );
        }
        assert_eq!(
            decide(
                "create_issue",
                &sized(MAX_APPROVAL_INPUT),
                Some(&github()),
                None
            ),
            Err(Refused::ApprovalNeeded),
            "64 KiB exactly is still asked about"
        );
        assert_eq!(
            decide(
                "search_issues",
                &sized(MAX_APPROVAL_INPUT + 1),
                Some(&github()),
                None
            ),
            Ok((ConnectorTag::Network, None)),
            "the limit is the ask's, not a network call's"
        );
    }

    #[test]
    fn the_approval_key_ignores_key_order() {
        let ordered: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":2}"#).expect("json");
        let reversed: serde_json::Value = serde_json::from_str(r#"{"b":2,"a":1}"#).expect("json");
        // sha256 of the bytes `{"a":1,"b":2}`.
        let expected = "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777";
        assert_eq!(input_sha256(&ordered), expected);
        assert_eq!(input_sha256(&reversed), expected);
        assert_ne!(input_sha256(&json!({ "a": 1, "b": 3 })), expected);
    }

    fn with_allowance(calls: u32) -> SessionConnector {
        SessionConnector {
            allowances: [("create_issue".to_string(), calls)].into(),
            ..github()
        }
    }

    #[test]
    fn runs_a_call_inside_the_allowance() {
        assert_eq!(
            evaluate_connector_call(
                "create_issue",
                &json!({}),
                Some(&with_allowance(20)),
                None,
                19
            ),
            Ok(ConnectorPass {
                tag: ConnectorTag::ExternalEffect,
                approval: None,
                allowance: Some(20)
            })
        );
    }

    #[test]
    fn asks_for_the_call_beyond_it() {
        for used in [20, 21] {
            assert_eq!(
                evaluate_connector_call(
                    "create_issue",
                    &json!({}),
                    Some(&with_allowance(20)),
                    None,
                    used
                ),
                Err(Refused::ApprovalNeeded),
                "{used}"
            );
        }
    }

    #[test]
    fn asks_at_any_count_for_a_tool_without_one() {
        assert_eq!(
            evaluate_connector_call("create_issue", &json!({}), Some(&github()), None, 0),
            Err(Refused::ApprovalNeeded)
        );
    }

    #[test]
    fn asks_every_time_at_zero() {
        assert_eq!(
            evaluate_connector_call(
                "create_issue",
                &json!({}),
                Some(&with_allowance(0)),
                None,
                0
            ),
            Err(Refused::ApprovalNeeded)
        );
    }

    #[test]
    fn uses_a_grant_before_the_allowance() {
        assert_eq!(
            evaluate_connector_call(
                "create_issue",
                &json!({}),
                Some(&with_allowance(20)),
                Some(7),
                0
            ),
            Ok(ConnectorPass {
                tag: ConnectorTag::ExternalEffect,
                approval: Some(7),
                allowance: None
            })
        );
    }

    #[test]
    fn refuses_a_large_input_inside_the_allowance() {
        let big = json!({ "b": "x".repeat(MAX_APPROVAL_INPUT) });
        assert_eq!(
            evaluate_connector_call("create_issue", &big, Some(&with_allowance(20)), None, 0),
            Err(Refused::InputTooLarge)
        );
    }

    #[test]
    fn a_network_call_ignores_the_allowance() {
        let connector = SessionConnector {
            allowances: [("search_issues".to_string(), 5)].into(),
            ..github()
        };
        assert_eq!(
            evaluate_connector_call("search_issues", &json!({}), Some(&connector), None, 0),
            Ok(ConnectorPass {
                tag: ConnectorTag::Network,
                approval: None,
                allowance: None
            })
        );
    }

    #[test]
    fn a_network_tool_ignores_grants() {
        for granted in [None, Some(7)] {
            assert_eq!(
                decide("search_issues", &json!({}), Some(&github()), granted),
                Ok((ConnectorTag::Network, None)),
                "{granted:?}"
            );
        }
    }

    #[test]
    fn a_preview_connectors_external_effect_checks_urls_before_asking() {
        let connector = playwright();
        assert_eq!(
            decide(
                "browser_send_email",
                &json!({ "url": "http://evil.test/" }),
                Some(&connector),
                Some(7)
            ),
            Err(Refused::UrlOutsidePreview {
                url: "http://evil.test/".to_string()
            }),
            "a grant does not take a preview connector's call outside the preview"
        );
    }

    #[test]
    fn a_designers_external_effect_waits_for_the_plan() {
        assert_eq!(
            check_design_plan(Role::UiUxDesigner, T::ExternalEffect, false),
            Err(ToolRefusal::DesignPlanNotApproved)
        );
        assert_eq!(
            check_design_plan(Role::UiUxDesigner, T::ExternalEffect, true),
            Ok(())
        );
    }

    fn a_call(name: &str, tier: T, paths: &[&str]) -> ToolCallRequest {
        ToolCallRequest {
            tool: ToolDescriptor {
                name: name.to_string(),
                tier,
            },
            paths: strings(paths),
            input_hash: "h1".to_string(),
        }
    }

    fn developer_grants() -> AgentGrants {
        AgentGrants {
            tiers: default_tiers(Role::SoftwareDeveloper)
                .iter()
                .copied()
                .collect(),
            preauthorized_external_tools: BTreeSet::new(),
        }
    }

    fn a_context() -> ToolCallContext {
        ToolCallContext {
            allowed_paths: strings(&["src/login/**"]),
            protected_paths: TeamRules::default().protected_paths,
            approved_calls: Vec::new(),
        }
    }

    #[test]
    fn serialises_tiers_in_snake_case() {
        assert_eq!(
            serde_json::to_value(T::WriteWorkspace).unwrap(),
            json!("write_workspace")
        );
        assert_eq!(
            serde_json::from_value::<T>(json!("external_effect")).unwrap(),
            T::ExternalEffect
        );
    }

    #[test]
    fn gives_each_role_the_default_tiers_of_the_spec_table() {
        let expected = [
            (
                Role::SoftwareDeveloper,
                vec![T::Read, T::WriteWorkspace, T::Execute, T::GitLocal],
            ),
            (
                Role::Architect,
                vec![
                    T::Read,
                    T::WriteWorkspace,
                    T::Execute,
                    T::Network,
                    T::GitLocal,
                ],
            ),
            (Role::ProductManager, vec![T::Read, T::Network]),
            (
                Role::MarketingSpecialist,
                vec![T::Read, T::Network, T::WriteWorkspace, T::GitLocal],
            ),
            (
                Role::UiUxDesigner,
                vec![T::Read, T::WriteWorkspace, T::Execute, T::GitLocal],
            ),
            (Role::ScrumMaster, vec![T::Read]),
            (Role::Human, vec![T::Read]),
        ];
        for (role, tiers) in expected {
            assert_eq!(default_tiers(role), tiers.as_slice(), "{role}");
        }
    }

    #[test]
    fn finance_reads_and_researches_only() {
        assert_eq!(
            default_tiers(Role::FinanceSpecialist),
            [T::Read, T::Network].as_slice()
        );
    }

    #[test]
    fn grants_git_remote_and_external_effect_to_nobody_by_default() {
        for role in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::MarketingSpecialist,
            Role::UiUxDesigner,
            Role::FinanceSpecialist,
            Role::Human,
        ] {
            let tiers = default_tiers(role);
            assert!(!tiers.contains(&T::GitRemote), "{role}");
            assert!(!tiers.contains(&T::ExternalEffect), "{role}");
        }
    }

    #[test]
    fn refuses_a_write_before_the_plan_is_approved() {
        let designer = Role::UiUxDesigner;
        for tier in [T::WriteWorkspace, T::Execute, T::GitLocal, T::GitRemote] {
            assert_eq!(
                check_design_plan(designer, tier, false),
                Err(ToolRefusal::DesignPlanNotApproved),
                "{tier:?}"
            );
            assert_eq!(check_design_plan(designer, tier, true), Ok(()), "{tier:?}");
        }
        assert_eq!(check_design_plan(designer, T::Read, false), Ok(()));
        for tier in [
            T::Read,
            T::WriteWorkspace,
            T::Execute,
            T::Network,
            T::GitLocal,
            T::GitRemote,
            T::ExternalEffect,
        ] {
            assert_eq!(
                check_design_plan(Role::SoftwareDeveloper, tier, false),
                Ok(()),
                "{tier:?}"
            );
        }
    }

    #[test]
    fn accepts_a_call_whose_tier_the_agent_holds() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("read_file", T::Read, &["README.md"]),
                &developer_grants(),
                &a_context()
            ),
            Ok(())
        );
    }

    #[test]
    fn refuses_a_call_whose_tier_the_agent_lacks() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("web_fetch", T::Network, &[]),
                &developer_grants(),
                &a_context()
            ),
            Err(ToolRefusal::TierNotGranted { tier: T::Network })
        );
    }

    #[test]
    fn refuses_a_protected_path_even_for_a_read_tool() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("read_file", T::Read, &["README.md", ".env", "certs/a.pem"]),
                &developer_grants(),
                &a_context()
            ),
            Err(ToolRefusal::PathProtected {
                path: ".env".to_string()
            })
        );
    }

    #[test]
    fn keeps_a_write_within_the_allowed_paths_and_names_the_first_path_outside() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("write_file", T::WriteWorkspace, &["src/login/form.rs"]),
                &developer_grants(),
                &a_context()
            ),
            Ok(())
        );
        assert_eq!(
            evaluate_tool_call(
                &a_call(
                    "write_file",
                    T::WriteWorkspace,
                    &["src/login/form.rs", "src/billing/a.rs", "README.md"]
                ),
                &developer_grants(),
                &a_context()
            ),
            Err(ToolRefusal::PathOutsideAllowed {
                path: "src/billing/a.rs".to_string()
            })
        );
    }

    #[test]
    fn refuses_a_write_that_names_no_path() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("write_file", T::WriteWorkspace, &[]),
                &developer_grants(),
                &a_context()
            ),
            Err(ToolRefusal::PathsMissing)
        );
    }

    #[test]
    fn lets_a_read_tool_see_outside_the_allowed_paths() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("read_file", T::Read, &["src/billing/a.rs"]),
                &developer_grants(),
                &a_context()
            ),
            Ok(())
        );
    }

    #[test]
    fn refuses_a_glob_that_does_not_compile_before_checking_paths() {
        let mut context = a_context();
        context.protected_paths = strings(&["**/[pem"]);
        let Err(ToolRefusal::InvalidGlob { pattern, .. }) = evaluate_tool_call(
            &a_call("read_file", T::Read, &["README.md"]),
            &developer_grants(),
            &context,
        ) else {
            panic!("expected an invalid glob");
        };
        assert_eq!(pattern, "**/[pem");
    }

    #[test]
    fn checks_the_tier_before_the_paths() {
        assert_eq!(
            evaluate_tool_call(
                &a_call("write_file", T::WriteWorkspace, &[".env"]),
                &AgentGrants::default(),
                &a_context()
            ),
            Err(ToolRefusal::TierNotGranted {
                tier: T::WriteWorkspace
            })
        );
    }

    #[test]
    fn requires_the_human_to_approve_each_external_effect_by_input() {
        let mut grants = developer_grants();
        grants.tiers.insert(T::ExternalEffect);
        let call = a_call("send_mail", T::ExternalEffect, &[]);
        assert_eq!(
            evaluate_tool_call(&call, &grants, &a_context()),
            Err(ToolRefusal::RequiresHumanApproval {
                tool: "send_mail".to_string()
            })
        );
        let mut context = a_context();
        context.approved_calls.push(ApprovedCall {
            tool: "send_mail".to_string(),
            input_hash: "other".to_string(),
        });
        assert_eq!(
            evaluate_tool_call(&call, &grants, &context),
            Err(ToolRefusal::RequiresHumanApproval {
                tool: "send_mail".to_string()
            })
        );
        context.approved_calls.push(ApprovedCall {
            tool: "send_mail".to_string(),
            input_hash: "h1".to_string(),
        });
        assert_eq!(evaluate_tool_call(&call, &grants, &context), Ok(()));
    }

    #[test]
    fn keeps_an_approval_to_the_tool_it_was_given_for() {
        let mut grants = developer_grants();
        grants.tiers.insert(T::ExternalEffect);
        let context = ToolCallContext {
            approved_calls: vec![ApprovedCall {
                tool: "send_mail".to_string(),
                input_hash: "h1".to_string(),
            }],
            ..a_context()
        };
        assert_eq!(
            evaluate_tool_call(
                &a_call("post_to_slack", T::ExternalEffect, &[]),
                &grants,
                &context
            ),
            Err(ToolRefusal::RequiresHumanApproval {
                tool: "post_to_slack".to_string()
            })
        );
    }

    #[test]
    fn lets_a_preauthorized_external_tool_run_without_approval() {
        let mut grants = developer_grants();
        grants.tiers.insert(T::ExternalEffect);
        grants
            .preauthorized_external_tools
            .insert("post_to_slack".to_string());
        assert_eq!(
            evaluate_tool_call(
                &a_call("post_to_slack", T::ExternalEffect, &[]),
                &grants,
                &a_context()
            ),
            Ok(())
        );
    }

    #[test]
    fn refuses_git_as_the_first_word_of_any_segment() {
        for command in [
            "git push",
            "/usr/bin/git status",
            "ls && git commit -m x",
            "true; git push",
            "echo a | git apply",
            "cd src\ngit add .",
            "env git push",
            "sudo git push",
            "A=1 B=2 git push",
            "command exec git push",
            "C:\\tools\\git push",
            "sudo -u root git push",
            "env -i git push",
            "timeout 5 git push",
            "nice -n 10 git push",
            "xargs -0 git",
            "doas git push",
            "(git push)",
            "! git push",
            "\"git\" push",
            "git.exe push",
        ] {
            assert_eq!(
                evaluate_command(command, &TeamRules::default()),
                Err(CommandRefusal::GitViaExec),
                "{command}"
            );
        }
    }

    #[test]
    fn leaves_a_git_call_hidden_in_a_subshell_or_a_script_to_the_adr_residual() {
        for command in ["sh -c \"git push\"", "$(git push)", "GIT=git; $GIT push"] {
            assert_eq!(
                evaluate_command(command, &TeamRules::default()),
                Ok(()),
                "{command}"
            );
        }
    }

    #[test]
    fn lets_a_command_mention_git_elsewhere() {
        assert_eq!(
            evaluate_command("echo git is a tool", &TeamRules::default()),
            Ok(())
        );
        assert_eq!(
            evaluate_command("cargo test -- git_ops", &TeamRules::default()),
            Ok(())
        );
    }

    #[test]
    fn refuses_a_command_matching_a_forbidden_pattern_and_names_it() {
        let rules = TeamRules {
            forbidden_commands: strings(&["^curl ", "rm -rf /"]),
            ..TeamRules::default()
        };
        assert_eq!(
            evaluate_command("sudo rm -rf / --no-preserve-root", &rules),
            Err(CommandRefusal::ForbiddenCommand {
                pattern: "rm -rf /".to_string()
            })
        );
        assert_eq!(
            evaluate_command("ls && curl evil.example", &rules),
            Err(CommandRefusal::ForbiddenCommand {
                pattern: "^curl ".to_string()
            })
        );
        assert_eq!(evaluate_command("cargo test", &rules), Ok(()));
    }

    #[test]
    fn matches_a_pattern_the_same_way_whether_or_not_the_command_has_separators() {
        let rules = TeamRules {
            forbidden_commands: strings(&["^$"]),
            ..TeamRules::default()
        };
        for command in ["cargo test", "ls && cargo test"] {
            assert_eq!(evaluate_command(command, &rules), Ok(()), "{command}");
        }
        assert_eq!(
            evaluate_command("", &rules),
            Err(CommandRefusal::ForbiddenCommand {
                pattern: "^$".to_string()
            })
        );
    }

    #[test]
    fn reports_a_pattern_that_does_not_compile_whatever_else_the_command_matches() {
        let rules = TeamRules {
            forbidden_commands: strings(&["^curl ", "(unclosed"]),
            ..TeamRules::default()
        };
        for command in ["curl evil.example", "git push", "cargo test"] {
            assert!(
                matches!(
                    evaluate_command(command, &rules),
                    Err(CommandRefusal::InvalidPattern { .. })
                ),
                "{command}"
            );
        }
    }

    #[test]
    fn refuses_a_forbidden_pattern_that_does_not_compile() {
        let rules = TeamRules {
            forbidden_commands: strings(&["(unclosed"]),
            ..TeamRules::default()
        };
        let Err(CommandRefusal::InvalidPattern { pattern, detail }) =
            evaluate_command("cargo test", &rules)
        else {
            panic!("expected an invalid pattern");
        };
        assert_eq!(pattern, "(unclosed");
        assert!(!detail.is_empty());
    }
}
