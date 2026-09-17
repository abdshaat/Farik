# Phase 1, step 04: Permission tiers

Status: done
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; steps do not get their own)
Spec: `docs/SPEC.md` section 5.6 (permission tiers, their defaults per role, protected paths on every tool call, per-call human approval of external effects), 5.12 (`forbidden_commands`), ADR 0004 (`farik_exec` refuses git; git is a Farik tool with its own tiers), F5
Depends on: phase 0 (merged in #4); step 02 of this phase (committed as 21fe00a, 87a3561, 4a1ac90: `TeamRules`); step 03 of this phase (committed as 220b576, 3355d35: the path checks)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code review session, 2026-09-16, before execution (first pass READY with four author-level items, taken in 44ea492; second pass READY under all three rules at 44ea492; recorded on pull request #5)

## Goal

`farik-core` can decide a tool call the way `docs/SPEC.md` section 5.6 says: the agent must hold the tool's tier, no call may touch a protected path whatever its tier, a write must stay within the contract's allowed paths, and an external effect runs only when the user pre-authorised the tool or approved this exact input. It can also decide whether `farik_exec` may run a command: not git, which is a Farik tool of its own, and nothing the team forbade. The tier enum serialises as the wire names, and every role has the spec table's defaults. Phase 3's hook handler calls these two functions on every `PreToolUse` and every `farik_exec`.

## Decisions

All in `docs/plans/project-plan.md`, phase 1, restated here only where this step needs the exact value.

- The order of the tool-call checks is tier, protected paths, allowed paths, approval, and the first refusal wins, so that an agent without a tier hears about the tier rather than about a path. Rejected: reporting every refusal, because a hook answers allow or deny with one reason.
- Protected paths are checked for every tool, `read` included (spec 5.6). Allowed paths are checked for `write_workspace` tools only: a `read` tool may read the whole project, and `execute`, `network`, git, and external-effect tools carry no paths. Rejected: checking allowed paths on reads, because the spec ties `allowed_paths` to writes and a developer must read what it does not change.
- A refusal from the path checks carries the first violating path; `PathRefusal::Glob` becomes `ToolRefusal::InvalidGlob`, the variant the project plan's step 04 entry gained in the step 03 plan.
- An external effect needs the tier and then either the tool pre-authorised for the agent or this call approved by the human, identified by tool name and input hash (project plan, every-phase decisions). A tier the agent does not hold refuses first.
- Default tiers are the spec 5.6 table: everyone `read`; Developer and Architect `write_workspace` and `execute`; Marketing, Architect, and Product Manager `network`; Developer `git_local`; nobody `git_remote` or `external_effect`; the Scrum Master reads only. The human is not an agent and gets the same read-only default, because `Role` has a `human` variant and every match must answer. Rejected: giving the human every tier, because the runtime never evaluates a tool call for the human.
- `farik_exec` refuses a command when any segment of it (split on `&&`, `||`, `;`, `|`, and newlines, without parsing quotes, so `echo "a; git push"` is refused on the safe side) runs git. A word is read bare (quotes and shell punctuation stripped, a path reduced to its last segment, a `.exe` suffix dropped, a redirection not a command name), and a segment runs git when its first bare word is `git` or when its first bare word is a `NAME=value` assignment or one of eleven named wrappers and `git` appears anywhere later in the segment. ADR 0004 says "first word is git"; splitting on the shell's separators closes `ls && git push`, and the assignment and wrapper rule closes `A=1 sudo -u root git push`, which the first-word reading missed because the wrapper's own flag became the first word. The cost is that `sudo apt install git` is refused too, which is the safe direction for a permission boundary. A call hidden in a subshell, a variable, or a script stays the accepted residual ADR 0004 records, pinned by a test so that the gap is known. `docs/SPEC.md` 5.6 names the wrappers and the residual exactly, because the rule is user-visible (hard rule 8).
- Forbidden commands are ECMAScript regular expressions matched through `regress` 0.12.0, which `farik-core` already depends on as the generated types' pattern engine, against the whole command and against each trimmed, non-blank segment, so that an anchor such as `^curl` applies per segment as the git rule does and a pattern that matches the empty string does not depend on whether the command has separators; every pattern is compiled before anything is matched, so that a pattern which does not compile is reported whatever else the command would have matched; the first matching pattern is named. ECMAScript has no inline flags, so `(?i)` is not a pattern a team can write, which `docs/SPEC.md` 5.12's row says. `regress` backtracks, so a pathological pattern costs the team that wrote it, the same exposure the schema validator already has. `docs/SPEC.md` 5.12's row says so. A pattern that does not compile refuses the command with `CommandRefusal::InvalidPattern { pattern, detail }`, the same stance as an invalid glob in step 03; the project plan's step 04 entry gains the variant in this plan's commit. Rejected: the `regex` crate, because a second engine for the same job is a dependency added for nothing; a team writes its patterns in the dialect the schema already validates.
- `PermissionTier` derives `Serialize` and `Deserialize` with `rename_all = "snake_case"` and `Ord`, because grants are sets and the wire names are the spec's. Rejected: a generated type, because no schema owns the tier yet (`team.schema.json` is phase 2 step 05 and will reference these names).
- A `write_workspace` call that names no path is refused with `ToolRefusal::PathsMissing`, because a write the hook could not extract paths from would otherwise pass unchecked; the project plan's step 04 entry gains the variant in this plan's revision. `PathProtected` also covers an absolute, empty, or climbing path, which the path check refuses outright; its doc says so. Protected paths cannot be enforced on what a command reads inside the container; that is phase 3's concern and is noted for it.
- `AgentGrants` and `ToolCallContext` derive `Default` so that a test can name only what it sets; an empty `AgentGrants` holds no tier.
- Tests build requests through two helpers and a developer's default grants; every code block below is the file after `cargo fmt --all`.
- Revised after the step review (pull request #5): the wrapper and assignment rule above, the bare reading of a word, blank segments left out of pattern matching, every pattern compiled first, and two tests that pin an approval to its own tool and its own input.

## Design

One task: the `permissions` module with `PermissionTier`, `default_tiers`, the four request and context types, `ToolRefusal`, `evaluate_tool_call`, `CommandRefusal`, `evaluate_command`, and twenty-one tests; one sentence in spec 5.6 and one row in spec 5.12.

Out of scope: the hook that calls these (phase 3), reading grants from `team.yaml` (phase 2), the cost of a call (step 05), git operations themselves (phase 2's git adapter).

## Architecture notes

Touches `crates/core` only: one new child of `governor`. Consumes `governor::paths::{check_allowed_paths, check_protected_paths, PathRefusal, GlobError}` (step 03), `governor::team_rules::TeamRules` (step 02), `contract::Role` (phase 0), and `regress` and `serde`, already dependencies of `farik-core`. Adds no dependency.

## Global constraints

- `farik-core` does no I/O; `cargo xtask core-io` passes.
- Every public item carries a doc comment; clippy pedantic with `-D warnings` passes; no `expect` or `unwrap` outside tests.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
crates/core/src/governor.rs                         modifies: declares permissions
crates/core/src/governor/permissions.rs             creates: PermissionTier, default_tiers, ToolDescriptor, ToolCallRequest, AgentGrants, ApprovedCall, ToolCallContext, ToolRefusal, evaluate_tool_call, CommandRefusal, evaluate_command, twenty-one tests
docs/SPEC.md                                        modifies: 5.6 says how farik_exec finds git in a command; 5.12's forbidden_commands row says how patterns match
docs/plans/project-plan.md                          modifies: phase 1 step 04 interface: CommandRefusal gains InvalidPattern, ToolRefusal gains PathsMissing (in the plan's own commits)
docs/plans/phase-1-harness/step-04-permission-tiers.md   modifies: checkboxes ticked
```

## Tasks

### Task 1: Tiers, the tool-call check, and the command check

Files: created `crates/core/src/governor/permissions.rs`; modified `crates/core/src/governor.rs`, `docs/SPEC.md`

Consumes: `governor::paths::{check_allowed_paths, check_protected_paths, PathRefusal, GlobError}` from step 03; `governor::team_rules::TeamRules` from step 02; `contract::Role` from `main`
Produces: `governor::permissions::{PermissionTier, default_tiers, ToolDescriptor, ToolCallRequest, AgentGrants, ApprovedCall, ToolCallContext, ToolRefusal, evaluate_tool_call, CommandRefusal, evaluate_command}`

- [x] Confirm the baseline on the branch head:

  ```
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Declare the module. `crates/core/src/governor.rs` in full:

  ```rust
  //! The governor: every rule of `docs/SPEC.md` section 5 as pure functions over values passed
  //! in. It never reads the world and never mutates; the runtime applies what it decides.

  /// Allowed and protected paths (`docs/SPEC.md` sections 5.4, 5.6, 5.12).
  pub mod paths;
  /// Permission tiers and the tool-call and command checks (`docs/SPEC.md` section 5.6, ADR 0004).
  pub mod permissions;
  /// The Definition of Ready of `docs/SPEC.md` section 5.3 as one function over a contract and a
  /// context.
  pub mod readiness;
  /// The lifecycle's statuses and which of them are terminal.
  pub mod task_status;
  /// Team rules of `docs/SPEC.md` section 5.12 and their defaults.
  pub mod team_rules;
  /// The transition table of `docs/SPEC.md` section 5.2 as data, with lookups.
  pub mod transition_table;
  ```
- [x] Write the failing tests. `crates/core/src/governor/permissions.rs` holds only this:

  ```rust
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
  ```
- [x] Run them and confirm they fail because the items are missing:

  ```
  cargo test --package farik-core governor::permissions
  # expected, among the output:
  # error[E0432]: unresolved imports `super::AgentGrants`, `super::ApprovedCall`, `super::CommandRefusal`, `super::PermissionTier`, `super::ToolCallContext`, `super::ToolCallRequest`, `super::ToolDescriptor`, `super::ToolRefusal`, `super::default_tiers`, `super::evaluate_command`, `super::evaluate_tool_call`
  # error: could not compile `farik-core` (lib test) due to 1 previous error
  ```

- [x] Write the implementation above the tests. `crates/core/src/governor/permissions.rs` in full:

  ```rust
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
  ```
- [x] In `docs/SPEC.md` section 5.6, replace the sentence

  ```
  The user's setup screen asks about `execute` and `git_remote` explicitly because those are the two that can hurt.
  ```

  with

  ```
  The user's setup screen asks about `execute` and `git_remote` explicitly because those are the two that can hurt. `farik_exec` refuses a command when any segment of it (split on `&&`, `||`, `;`, `|`, and newlines, without parsing quotes) runs git, because git is a Farik tool with its own tiers (ADR 0004). A segment runs git when its first word is `git`, or when its first word is a `NAME=value` assignment or one of the wrappers `env`, `sudo`, `doas`, `command`, `exec`, `nohup`, `time`, `timeout`, `nice`, `setsid`, `xargs` and `git` appears anywhere later in the segment, which catches `sudo -u root git push` at the cost of refusing `sudo apt install git`. A word is read bare: quotes and shell punctuation are stripped, a path keeps only its last segment, a `.exe` suffix is dropped, and a redirection is not a command name. A call hidden in a subshell, a variable, or a script (`$(git push)`, `GIT=git; $GIT push`, `sh -c "git push"`) is the residual ADR 0004 accepts.
  ```

- [x] In `docs/SPEC.md` section 5.12, replace the table row

  ```
  | `forbidden_commands` | regular expressions | `farik_exec` refuses a command that matches one |
  ```

  with

  ```
  | `forbidden_commands` | regular expressions | `farik_exec` refuses a command that matches one: ECMAScript patterns, which have no inline flags such as `(?i)`, matched against the whole command and against each non-blank segment (split on `&&`, `||`, `;`, `\|`, and newlines), and a pattern that does not compile refuses every command |
  ```

- [x] Format, run the tests and the full check; confirm green:

  ```
  cargo fmt --all
  cargo test --package farik-core governor::permissions
  # expected, among the output:
  # test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 69 filtered out; finished in ...
  cargo xtask check
  # expected, among the output, then exit code 0:
  # test result: ok. 90 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
  # test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
  # xtask check: ok
  ```

- [x] Commit: `feat(core): evaluate tool calls and commands against permission tiers`

## Verification

```
cargo xtask check
# expected, among the output, then exit code 0:
# test result: ok. 90 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (farik-core)
# test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ...   (xtask)
# xtask check: ok
```

```
cargo xtask core-io
# expected: no output, exit code 0.
```

```
git log --oneline -1
# expected: feat(core): evaluate tool calls and commands against permission tiers
```

## Open questions

none
