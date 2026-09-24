//! Farik's own tools (`docs/SPEC.md` sections 5.6 and 8.2, ADR 0004): what an agent calls to read
//! the board, write a contract, ask for a transition, run a command, or use git. Each is checked
//! against the agent's tier and against the governance rule that owns it, and each leaves an event.
//! They are plain Rust: the MCP server that exposes them is the daemon's.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, LazyLock};

use farik_core::contract::{Role, TaskContract, TaskId};
use farik_core::governor::permissions::{
    AgentGrants, PermissionTier, ToolCallContext, ToolCallRequest, ToolDescriptor,
    evaluate_tool_call,
};
use farik_core::team::{Agent, AgentStatus, Team};
use farik_protocol::clock::Clock;
use farik_protocol::event::{EventBody, EventIds, FarikEvent, new_event};
use farik_store::files::ProjectFiles;
use farik_store::{EventLog, Git, Projections, TaskProjection};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::exec::Executor;
use crate::session::SessionPurpose;
use crate::transitions::Transitions;

mod channel;
mod contracts;
mod exec;
#[cfg(test)]
pub(crate) mod fixtures;
mod git;
mod reading;
pub(crate) mod refusal;
mod work;

use refusal::Refusal;

/// Why a tool call did not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// The tool does not exist, or its input does not fit its schema.
    InvalidInput {
        /// What is wrong with the input, in serde's words where serde said it.
        detail: String,
    },
    /// A rule refused the call. The reason starts with the refusal's kind in `snake_case`, then
    /// `: `, then its details, so that a test and an agent can both read which kind it was.
    Refused {
        /// The kind, then what it says.
        reason: String,
    },
    /// Farik could not carry the call out: the store, the files, git, or an executor failed.
    Failed {
        /// What failed.
        detail: String,
    },
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { detail } => {
                write!(formatter, "the input is not one this tool takes: {detail}")
            }
            Self::Refused { reason } => write!(formatter, "refused: {reason}"),
            Self::Failed { detail } => write!(formatter, "the tool failed: {detail}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// What every tool call of one project works with.
pub struct ToolDeps {
    /// The log every tool appends to.
    pub log: Arc<EventLog>,
    /// The board, kept up to date with every append.
    pub projections: Arc<Projections>,
    /// The files under `.farik/`.
    pub files: Arc<ProjectFiles>,
    /// The governor's door, for every transition a tool asks for.
    pub transitions: Arc<Transitions>,
    /// The repository.
    pub git: Git,
    /// The time every event is stamped with.
    pub clock: Arc<dyn Clock + Send + Sync>,
    /// The team and project every event belongs to; the other ids are the call's own.
    pub ids: EventIds,
}

/// The session a call comes from.
pub struct ToolContext {
    /// The calling agent. Its role and tiers are read from `.farik/team.yaml` on every call, so a
    /// pause, a grant, or a revoke changes the next call rather than the next session.
    pub agent_id: String,
    /// The task the session works on, when it works on one.
    pub task_id: Option<TaskId>,
    /// The session, stamped on every event a call appends.
    pub session_id: String,
    /// Why the session runs, which decides what kind of message it posts.
    pub purpose: SessionPurpose,
    /// Where the task's commands run, when it has somewhere.
    pub executor: Option<Arc<dyn Executor>>,
    /// The project's store, files, and repository.
    pub deps: Arc<ToolDeps>,
}

/// One tool as an agent is shown it.
#[derive(Debug, Clone, PartialEq)]
pub struct FarikTool {
    /// The name an agent calls it by.
    pub name: &'static str,
    /// The tier it needs.
    pub tier: PermissionTier,
    /// What it does, for the agent.
    pub description: &'static str,
    /// Its input's JSON Schema.
    pub input_schema: Value,
}

fn tool<Input: JsonSchema>(
    name: &'static str,
    tier: PermissionTier,
    description: &'static str,
) -> FarikTool {
    let mut input_schema = Value::from(schemars::schema_for!(Input));
    // The dialect, the struct's name and its doc line ("`farik_x`'s input.") tell an agent
    // nothing, and every session is shown every schema.
    if let Some(root) = input_schema.as_object_mut() {
        for noise in ["$schema", "title", "description"] {
            root.remove(noise);
        }
    }
    FarikTool {
        name,
        tier,
        description,
        input_schema,
    }
}

static TOOLS: LazyLock<Vec<FarikTool>> = LazyLock::new(|| {
    use PermissionTier::{Execute, GitLocal, GitRemote, Read};
    vec![
        tool::<reading::ReadTaskInput>(
            "farik_read_task",
            Read,
            "Read a task's contract and its row on the board.",
        ),
        tool::<NoInput>(
            "farik_read_board",
            Read,
            "Read the board: every task, its status, and whether it is triaged.",
        ),
        tool::<NoInput>(
            "farik_read_rules",
            Read,
            "Read the team's rules: protected paths, the allowed-paths ceiling, required criteria, the budget cap, and forbidden commands.",
        ),
        tool::<NoInput>(
            "farik_read_criteria",
            Read,
            "Read the criterion library a contract refers to by name.",
        ),
        tool::<contracts::TriageInput>(
            "farik_triage_request",
            Read,
            "Size this session's request as large (an epic) or small (a task), with a reason.",
        ),
        tool::<contracts::RecordJudgmentInput>(
            "farik_record_judgment",
            Read,
            "Record your judgment of this session's contract: whether the task fits its budget and whether its criteria would detect the failure its intent worries about, with the reason.",
        ),
        tool::<contracts::WriteContractInput>(
            "farik_write_contract",
            Read,
            "Write fields of this session's contract, and add exit criteria from the library by name.",
        ),
        tool::<contracts::CreateTaskInput>(
            "farik_create_task",
            Read,
            "File a new draft request, or with parent a task of the epic you are breaking down.",
        ),
        tool::<work::RequestTransitionInput>(
            "farik_request_transition",
            Read,
            "Ask the governor to move this session's task to another status.",
        ),
        tool::<work::AssignTaskInput>(
            "farik_assign_task",
            Read,
            "Assign a ready task to an agent, with the agent that reviews it.",
        ),
        tool::<contracts::PlanSprintInput>(
            "farik_plan_sprint",
            Read,
            "Plan the open sprint: put ready tasks and approved epics in it, within its budget.",
        ),
        tool::<work::DeclareBlockedInput>(
            "farik_declare_blocked",
            Read,
            "Block this session's task: what is in the way and what is needed.",
        ),
        tool::<work::RecordCriterionInput>(
            "farik_record_criterion_result",
            Read,
            "Record an exit criterion's result, with its evidence, as the agent that ran it.",
        ),
        tool::<work::WriteNoteInput>(
            "farik_write_note",
            Read,
            "Write a completion, review, or progress note about this session's task.",
        ),
        tool::<work::AskHumanInput>(
            "farik_ask_human",
            Read,
            "Ask the human a question; end your turn after asking.",
        ),
        tool::<work::WriteProductDocInput>(
            "farik_write_product_doc",
            Read,
            "Write a product document under .farik/product/ for the approved epic of this session.",
        ),
        tool::<channel::PostMessageInput>(
            "farik_post_message",
            Read,
            "Say something in the team's channel. One or two sentences: what happened and what is next, with no instruction to anyone.",
        ),
        tool::<exec::ExecInput>(
            "farik_exec",
            Execute,
            "Run a shell command in the task's sandbox. This is your shell; git is not run here.",
        ),
        tool::<NoInput>(
            "farik_git_status",
            GitLocal,
            "Show what is changed in the task's worktree.",
        ),
        tool::<NoInput>(
            "farik_git_diff",
            GitLocal,
            "Show the task branch's changes since the integration branch, as a patch.",
        ),
        tool::<git::CommitInput>(
            "farik_git_commit",
            GitLocal,
            "Commit the named paths of the task's worktree on the task branch.",
        ),
        tool::<NoInput>(
            "farik_git_push",
            GitRemote,
            "Push the task branch to origin.",
        ),
    ]
});

/// An input with nothing in it.
#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoInput {}

/// Every Farik tool, with its tier and its input's schema.
#[must_use]
pub fn tool_descriptors() -> Vec<FarikTool> {
    TOOLS.clone()
}

/// Runs one tool for the session `context` names. The agent must be an active agent of the team,
/// hold the tool's tier with the paths the call touches allowed (`evaluate_tool_call`, asked here
/// as well as in the hook because the endpoint is reachable by anything holding the daemon's
/// token), and pass the rule the tool itself answers to.
///
/// # Errors
///
/// `InvalidInput` for a tool that does not exist or an input that does not fit; `Refused` with the
/// refusal's kind first; `Failed` when the store, the files, git, or the executor fail.
pub async fn call_tool(
    context: &ToolContext,
    name: &str,
    input: Value,
) -> Result<Value, ToolError> {
    let tool =
        TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| ToolError::InvalidInput {
                detail: format!("there is no Farik tool named {name}"),
            })?;
    let team = context.deps.files.read_team().map_err(failed)?;
    let agent = match team
        .agents
        .iter()
        .find(|agent| agent.id.as_str() == context.agent_id)
    {
        Some(agent) if agent.status == AgentStatus::Active => agent.clone(),
        found => {
            return Err(Refusal::AgentNotActive {
                agent_id: context.agent_id.clone(),
                status: found.map(|agent| agent.status),
            }
            .into());
        }
    };
    let call = Call {
        context,
        team,
        agent,
    };
    call.permit(tool, paths_of(name, &input))?;
    match name {
        "farik_read_task" => reading::read_task(&call, &parse(input)?),
        "farik_read_board" => nothing_in(input).and_then(|()| reading::read_board(&call)),
        "farik_read_rules" => nothing_in(input).map(|()| reading::read_rules(&call)),
        "farik_read_criteria" => nothing_in(input).and_then(|()| reading::read_criteria(&call)),
        "farik_triage_request" => contracts::triage(&call, &parse(input)?),
        "farik_record_judgment" => contracts::record_judgment(&call, &parse(input)?),
        "farik_write_contract" => contracts::write_contract(&call, parse(input)?),
        "farik_create_task" => contracts::create_task(&call, parse(input)?),
        "farik_request_transition" => work::request_transition(&call, parse(input)?),
        "farik_assign_task" => work::assign_task(&call, parse(input)?),
        "farik_plan_sprint" => contracts::plan_sprint(&call, &parse(input)?),
        "farik_declare_blocked" => work::declare_blocked(&call, parse(input)?),
        "farik_record_criterion_result" => work::record_criterion(&call, parse(input)?),
        "farik_write_note" => work::write_note(&call, parse(input)?),
        "farik_ask_human" => work::ask_human(&call, parse(input)?),
        "farik_write_product_doc" => work::write_product_doc(&call, parse(input)?),
        "farik_post_message" => channel::post_message(&call, parse(input)?),
        "farik_exec" => exec::exec(&call, parse(input)?).await,
        "farik_git_status" => nothing_in(input).and_then(|()| git::status(&call)),
        "farik_git_diff" => nothing_in(input).and_then(|()| git::diff(&call)),
        "farik_git_commit" => git::commit(&call, &parse(input)?),
        "farik_git_push" => nothing_in(input).and_then(|()| git::push(&call)),
        _ => Err(ToolError::Failed {
            detail: format!("{name} is listed and has no handler"),
        }),
    }
}

/// The paths a call touches, for the permission check: `.farik/product/<path>` for a product
/// document and the named paths of a commit; nothing for every other tool. Read leniently, since
/// the input is parsed strictly afterwards.
fn paths_of(name: &str, input: &Value) -> Vec<String> {
    match name {
        "farik_write_product_doc" => input
            .get("path")
            .and_then(Value::as_str)
            .map(|path| vec![format!(".farik/product/{path}")])
            .unwrap_or_default(),
        "farik_git_commit" => input
            .get("paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn parse<Input: DeserializeOwned>(input: Value) -> Result<Input, ToolError> {
    serde_json::from_value(input).map_err(|error| ToolError::InvalidInput {
        detail: error.to_string(),
    })
}

/// Holds an input that should be empty to being empty.
fn nothing_in(input: Value) -> Result<(), ToolError> {
    parse::<NoInput>(input).map(|_| ())
}

fn failed(error: impl fmt::Display) -> ToolError {
    ToolError::Failed {
        detail: error.to_string(),
    }
}

/// One call in flight: the session, and the team and agent as the team file says they are now.
pub(crate) struct Call<'a> {
    context: &'a ToolContext,
    team: Team,
    agent: Agent,
}

impl Call<'_> {
    fn deps(&self) -> &ToolDeps {
        &self.context.deps
    }

    fn agent_id(&self) -> &str {
        self.agent.id.as_str()
    }

    fn role(&self) -> Role {
        Role::from(self.agent.role)
    }

    /// The session's task, or `no_task`.
    fn task(&self) -> Result<&TaskId, ToolError> {
        self.context
            .task_id
            .as_ref()
            .ok_or_else(|| Refusal::NoTask.into())
    }

    /// The contract of `task`, with the status, people, and iteration the board gives it: the log
    /// decides where a task is (8.4).
    fn contract(&self, task: &TaskId) -> Result<(TaskContract, TaskProjection), ToolError> {
        let row = self.row(task)?;
        let mut contract = self.deps().files.read_contract(task).map_err(failed)?;
        contract.status = row.status;
        contract.assignee.clone_from(&row.assignee_id);
        contract.reviewer.clone_from(&row.reviewer_id);
        contract.iteration = row.iteration.into();
        Ok((contract, row))
    }

    /// The board's row of `task`, or `no_such_task`.
    fn row(&self, task: &TaskId) -> Result<TaskProjection, ToolError> {
        self.deps()
            .projections
            .task(task)
            .map_err(failed)?
            .ok_or_else(|| {
                Refusal::NoSuchTask {
                    task_id: task.to_string(),
                }
                .into()
            })
    }

    /// The tier check and the path checks of 5.6 for this tool and these paths.
    fn permit(&self, tool: &FarikTool, paths: Vec<String>) -> Result<(), ToolError> {
        let allowed_paths = match &self.context.task_id {
            Some(task) => self
                .deps()
                .files
                .read_contract(task)
                .map_err(failed)?
                .allowed_paths
                .iter()
                .map(ToString::to_string)
                .collect(),
            None => Vec::new(),
        };
        evaluate_tool_call(
            &ToolCallRequest {
                tool: ToolDescriptor {
                    name: tool.name.to_string(),
                    tier: tool.tier,
                },
                paths,
                input_hash: String::new(),
            },
            &AgentGrants {
                tiers: self.agent.tiers().into_iter().collect::<BTreeSet<_>>(),
                preauthorized_external_tools: BTreeSet::new(),
            },
            &ToolCallContext {
                allowed_paths,
                protected_paths: self.team.rules().protected_paths,
                approved_calls: Vec::new(),
            },
        )
        .map_err(|refusal| Refusal::Tool(refusal).into())
    }

    /// The ids an event of this call is stamped with: the agent, the session, and `task`.
    fn ids(&self, task: Option<&TaskId>) -> EventIds {
        EventIds {
            task_id: task.cloned(),
            agent_id: Some(self.agent_id().to_string()),
            session_id: Some(self.context.session_id.clone()),
            ..self.deps().ids.clone()
        }
    }

    /// Appends one event, stamped with the agent, the session, and `task` when it is about one,
    /// and projects it.
    fn append(&self, task: Option<&TaskId>, body: EventBody) -> Result<FarikEvent, ToolError> {
        let deps = self.deps();
        let event = new_event(body, deps.clock.now(), self.ids(task)).map_err(|error| {
            ToolError::Failed {
                detail: format!("the event cannot be stamped: {error:?}"),
            }
        })?;
        let appended = deps.log.append(&event).map_err(failed)?;
        deps.projections.apply(&appended).map_err(failed)?;
        Ok(appended)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{TestProject, a_team_of_three};
    use super::{ToolError, tool_descriptors};
    use farik_core::governor::permissions::PermissionTier;

    #[test]
    fn lists_every_tool_with_its_tier() {
        let tools = tool_descriptors();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name).collect();
        let expected = [
            "farik_read_task",
            "farik_read_board",
            "farik_read_rules",
            "farik_read_criteria",
            "farik_triage_request",
            "farik_record_judgment",
            "farik_write_contract",
            "farik_create_task",
            "farik_request_transition",
            "farik_assign_task",
            "farik_plan_sprint",
            "farik_declare_blocked",
            "farik_record_criterion_result",
            "farik_write_note",
            "farik_ask_human",
            "farik_write_product_doc",
            "farik_post_message",
            "farik_exec",
            "farik_git_status",
            "farik_git_diff",
            "farik_git_commit",
            "farik_git_push",
        ];
        assert_eq!(names, expected);
        let tier = |name: &str| {
            tools
                .iter()
                .find(|tool| tool.name == name)
                .map(|tool| tool.tier)
        };
        assert_eq!(tier("farik_exec"), Some(PermissionTier::Execute));
        assert_eq!(tier("farik_git_status"), Some(PermissionTier::GitLocal));
        assert_eq!(tier("farik_git_diff"), Some(PermissionTier::GitLocal));
        assert_eq!(tier("farik_git_commit"), Some(PermissionTier::GitLocal));
        assert_eq!(tier("farik_git_push"), Some(PermissionTier::GitRemote));
        for tool in &tools[..17] {
            assert_eq!(tool.tier, PermissionTier::Read, "{}", tool.name);
        }
        for tool in &tools {
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
            for noise in ["$schema", "title", "description"] {
                assert!(
                    tool.input_schema.get(noise).is_none(),
                    "{} shows every session its {noise}",
                    tool.name
                );
            }
        }
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_tool_the_agent_has_no_tier_for() {
        let project = TestProject::new("tools-tier", &a_team_of_three(|_| {}));
        project.filed("FRK-1", "in_progress", "task", None);
        let marker = project.repo.path.join("ran");
        let refused = project
            .call(
                "pm",
                Some("FRK-1"),
                "farik_exec",
                json!({ "command": format!("touch {}", marker.display()) }),
            )
            .expect_err("the Product Manager does not execute");
        assert_eq!(
            refused,
            ToolError::Refused {
                reason: "tier_not_granted: execute".to_string()
            }
        );
        assert!(!marker.exists(), "nothing ran");
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_a_paused_agent() {
        let project = TestProject::new(
            "tools-paused",
            &a_team_of_three(|wire| wire["agents"][1]["status"] = json!("paused")),
        );
        let refused = project
            .call("dev-a", None, "farik_read_board", json!({}))
            .expect_err("a paused agent takes no work");
        assert!(
            matches!(&refused, ToolError::Refused { reason } if reason.starts_with("agent_not_active")),
            "{refused:?}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_input_that_does_not_fit() {
        let project = TestProject::new("tools-input", &a_team_of_three(|_| {}));
        let refused = project
            .call("pm", None, "farik_read_task", json!({ "task_id": 7 }))
            .expect_err("a task id is a string");
        assert!(
            matches!(refused, ToolError::InvalidInput { .. }),
            "{refused:?}"
        );
    }

    #[test]
    #[ignore = "needs the git program: cargo xtask check --integration"]
    fn refuses_an_unknown_tool() {
        let project = TestProject::new("tools-unknown", &a_team_of_three(|_| {}));
        let refused = project
            .call("pm", None, "farik_nothing", json!({}))
            .expect_err("there is no such tool");
        assert!(
            matches!(&refused, ToolError::InvalidInput { detail } if detail.contains("farik_nothing")),
            "{refused:?}"
        );
    }
}
