//! Permission tiers (`docs/SPEC.md` section 5.6): what a tool call may do given the agent's
//! grants, the task's allowed paths, and the team's protected paths; and what a command may be
//! (`farik_exec`, ADR 0004 and spec 5.12).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::paths::{GlobError, PathRefusal, check_allowed_paths, check_protected_paths};
use super::team_rules::TeamRules;
use crate::contract::Role;

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
        Role::SoftwareDeveloper => &[T::Read, T::WriteWorkspace, T::Execute, T::GitLocal],
        Role::Architect => &[T::Read, T::WriteWorkspace, T::Execute, T::Network],
        Role::ProductManager | Role::MarketingSpecialist => &[T::Read, T::Network],
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
const WRAPPERS: [&str; 7] = ["env", "sudo", "command", "exec", "nohup", "time", "xargs"];

/// Decides whether `farik_exec` may run a command. The command is split into segments on `&&`,
/// `||`, `;`, `|`, and newlines, without parsing quotes, so `echo "a; git push"` is refused too,
/// on the safe side. A segment runs git when its first word, after any leading `NAME=value`
/// assignments and wrappers such as `env`, `sudo`, `command`, `exec`, `nohup`, `time`, or
/// `xargs`, is `git` or a path ending in `git`; git is a Farik tool with its own tiers (ADR
/// 0004), and any other spelling (a subshell, a variable, a script, `sh -c`) is the residual
/// that record accepts. A forbidden pattern is an ECMAScript regular expression matched against
/// the whole command and against each segment, so that an anchor such as `^curl` applies per
/// segment; the engine backtracks, so a pathological pattern is the team's own cost.
///
/// # Errors
///
/// `GitViaExec`, or `ForbiddenCommand` with the first pattern that matches, or
/// `InvalidPattern` when a pattern does not compile.
pub fn evaluate_command(command: &str, rules: &TeamRules) -> Result<(), CommandRefusal> {
    let segments: Vec<&str> = command.split(['\n', ';', '|', '&']).collect();
    if segments.iter().copied().any(runs_git) {
        return Err(CommandRefusal::GitViaExec);
    }
    for pattern in &rules.forbidden_commands {
        let regex =
            regress::Regex::new(pattern).map_err(|error| CommandRefusal::InvalidPattern {
                pattern: pattern.clone(),
                detail: error.text,
            })?;
        if regex.find(command).is_some()
            || segments
                .iter()
                .any(|segment| regex.find(segment.trim()).is_some())
        {
            return Err(CommandRefusal::ForbiddenCommand {
                pattern: pattern.clone(),
            });
        }
    }
    Ok(())
}

fn runs_git(segment: &str) -> bool {
    segment
        .split_whitespace()
        .find(|word| !is_assignment(word) && !WRAPPERS.contains(word))
        .is_some_and(|word| word == "git" || word.ends_with("/git") || word.ends_with("\\git"))
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
        AgentGrants, ApprovedCall, CommandRefusal, PermissionTier as T, ToolCallContext,
        ToolCallRequest, ToolDescriptor, ToolRefusal, default_tiers, evaluate_command,
        evaluate_tool_call,
    };
    use crate::contract::Role;
    use crate::governor::team_rules::TeamRules;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
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
                vec![T::Read, T::WriteWorkspace, T::Execute, T::Network],
            ),
            (Role::ProductManager, vec![T::Read, T::Network]),
            (Role::MarketingSpecialist, vec![T::Read, T::Network]),
            (Role::ScrumMaster, vec![T::Read]),
            (Role::Human, vec![T::Read]),
        ];
        for (role, tiers) in expected {
            assert_eq!(default_tiers(role), tiers.as_slice(), "{role}");
        }
    }

    #[test]
    fn grants_git_remote_and_external_effect_to_nobody_by_default() {
        for role in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::MarketingSpecialist,
            Role::Human,
        ] {
            let tiers = default_tiers(role);
            assert!(!tiers.contains(&T::GitRemote), "{role}");
            assert!(!tiers.contains(&T::ExternalEffect), "{role}");
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
        assert!(evaluate_tool_call(&call, &grants, &context).is_err());
        context.approved_calls.push(ApprovedCall {
            tool: "send_mail".to_string(),
            input_hash: "h1".to_string(),
        });
        assert_eq!(evaluate_tool_call(&call, &grants, &context), Ok(()));
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
