//! The team (`docs/SPEC.md` sections 3, 5.12, 5.14 and 5.16): `docs/schemas/team.schema.json` as
//! Rust types, the validator that turns an untrusted JSON value into one, and the three rules the
//! schema cannot say.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::contract::{named, pointer, repeated_ids, with_integers_normalised};
use crate::governor::permissions::{PermissionTier, default_tiers};
use crate::governor::team_rules::TeamRules;

pub use crate::contract::{Role, ValidationError};
pub use crate::generated::team::{
    Agent, AgentId, AgentStatus, Budgets as TeamBudgets, FarikTeam as Team, Model as AgentModel,
    ModelEffort as Effort, PermissionTier as PermissionTierWire, Policy as TeamPolicy,
    PolicyHumanAcceptsContracts as HumanAcceptsContracts, PolicyIntegration as Integration,
    Role as RoleWire, Rules as RulesWire, SessionLimits as SessionLimitsWire,
};

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/team.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded team schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded team schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// The two roles a team cannot work without, and what each of them is for.
///
/// `docs/SPEC.md` section 3 and F1: a team with nobody to write a contract has no way to start, and
/// a team with nobody to do the work has no way to finish. Every other role is the human's choice.
const REQUIRED_ROLES: [(RoleWire, &str); 2] = [
    (RoleWire::ProductManager, "write a contract"),
    (RoleWire::SoftwareDeveloper, "do the work"),
];

/// Checks a value against `docs/schemas/team.schema.json` and, when it conforms, returns the typed
/// team.
///
/// Four rules are this function's rather than the schema's. Three of them are there because
/// `typify` cannot generate a
/// usable type from an array that carries a `contains` — it writes an empty enum for the array and
/// the whole team becomes unbuildable. So the schema says two to seven agents and this says the
/// rest: ids are unique, and an active Product Manager and an active Software Developer are there.
/// The fourth is there because the schema has no way to say it either: an agent may not have `read`
/// taken away, since everyone reads (5.6) and an agent that cannot read is one every tool call is
/// refused for.
///
/// They are checked after the schema passes, on the typed value, and every one of them is reported
/// rather than only the first.
///
/// # Errors
///
/// Every schema violation, each at its own JSON pointer; or, when the schema passes, one error per
/// repeated agent id, one per agent that revoked `read`, and one per missing required role; or,
/// when the schema passes and the typed team cannot be built, one error at the root.
pub fn validate_team(input: &Value) -> Result<Team, Vec<ValidationError>> {
    let errors: Vec<ValidationError> = VALIDATOR
        .iter_errors(input)
        .map(|error| ValidationError {
            path: pointer(&error.instance_path().to_string()),
            message: error.to_string(),
        })
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let team =
        serde_json::from_value::<Team>(with_integers_normalised(input)).map_err(|error| {
            vec![ValidationError {
                path: "/".to_string(),
                message: format!(
                    "the schema passed but the typed team could not be built: {error}"
                ),
            }]
        })?;
    let mut errors = Vec::new();
    let repeated = repeated_ids(team.agents.iter().map(|agent| agent.id.as_str()));
    if !repeated.is_empty() {
        errors.push(ValidationError {
            path: "/agents".to_string(),
            message: format!(
                "an agent id names one agent, and {} more than one",
                named(&repeated)
            ),
        });
    }
    for (index, agent) in team.agents.iter().enumerate() {
        if agent
            .revokes
            .iter()
            .flatten()
            .any(|tier| *tier == PermissionTierWire::Read)
        {
            errors.push(ValidationError {
                path: format!("/agents/{index}/revokes"),
                message: format!(
                    "{} cannot have read taken away: everyone reads (docs/SPEC.md section 5.6), \
                     and an agent that cannot read is one every tool call is refused for. A team \
                     pauses an agent instead.",
                    agent.id.as_str()
                ),
            });
        }
    }
    for (role, what) in REQUIRED_ROLES {
        if !team.has_active(Role::from(role)) {
            errors.push(ValidationError {
                path: "/agents".to_string(),
                message: format!(
                    "a team needs an active {role} to {what}; this one has none (docs/SPEC.md \
                     section 3)"
                ),
            });
        }
    }
    if errors.is_empty() {
        Ok(team)
    } else {
        Err(errors)
    }
}

impl Team {
    /// The rules the governor applies, with what the team left out filled in from what
    /// `farik-core` ships (`docs/SPEC.md` section 5.12).
    ///
    /// The shipped protected paths are kept whatever the team writes, and the team's are added to
    /// them: 5.12 says rules only narrow what a tier allows, and a team that could delete `.env`
    /// from the list would be widening one.
    #[must_use]
    pub fn rules(&self) -> TeamRules {
        let shipped = TeamRules::default();
        let mut protected_paths = shipped.protected_paths;
        for path in &self.rules.protected_paths {
            let path = path.to_string();
            if !protected_paths.contains(&path) {
                protected_paths.push(path);
            }
        }
        TeamRules {
            protected_paths,
            allowed_paths_ceiling: self
                .rules
                .allowed_paths_ceiling
                .iter()
                .map(|glob| glob.to_string())
                .collect(),
            required_criteria: self
                .rules
                .required_criteria
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            require_new_tests: self
                .rules
                .require_new_tests
                .unwrap_or(shipped.require_new_tests),
            max_task_budget_usd: self
                .rules
                .max_task_budget_usd
                .or(shipped.max_task_budget_usd),
            forbidden_commands: self
                .rules
                .forbidden_commands
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
            document_paths: self
                .rules
                .document_paths
                .iter()
                .map(|glob| glob.to_string())
                .collect(),
        }
    }

    /// The agents that take work. A paused agent keeps what it holds and is given nothing new; a
    /// retired one is kept only so that its past events still name someone (D18).
    pub fn active_agents(&self) -> impl Iterator<Item = &Agent> {
        self.agents
            .iter()
            .filter(|agent| agent.status == AgentStatus::Active)
    }

    /// Whether an active agent holds this role.
    ///
    /// `Role::Human` is never one of them: the human is not an agent, which is why no agent may
    /// carry that role in the first place.
    #[must_use]
    pub fn has_active(&self, role: Role) -> bool {
        self.active_agents()
            .any(|agent| Role::from(agent.role) == role)
    }
}

impl Agent {
    /// Every permission tier this agent holds: its role's defaults, widened by what it was granted
    /// and narrowed by what was taken away.
    ///
    /// `docs/SPEC.md` section 5.6 says a role's tiers are the user's to override, and an override
    /// that could only widen would leave no way to say that this Developer does not run commands.
    /// Taking away wins over granting, because a tier in both lists is a person's mistake and the
    /// narrower reading of a mistake is the safer one. The order is the role's own first, then what
    /// a grant added, so that a list read back reads as the role plus the exceptions.
    #[must_use]
    pub fn tiers(&self) -> Vec<PermissionTier> {
        let mut tiers = default_tiers(Role::from(self.role)).to_vec();
        for granted in self
            .grants
            .iter()
            .flatten()
            .copied()
            .map(PermissionTier::from)
        {
            if !tiers.contains(&granted) {
                tiers.push(granted);
            }
        }
        let revoked: Vec<PermissionTier> = self
            .revokes
            .iter()
            .flatten()
            .copied()
            .map(PermissionTier::from)
            .collect();
        tiers.retain(|tier| !revoked.contains(tier));
        tiers
    }
}

/// The team schema's roles are the contract schema's minus `human`: a contract may name the human
/// as a reviewer, and no agent is one. This is the mapping `docs/standards/code.md` allows one of
/// per crate, kept at the crate's edge.
impl From<RoleWire> for Role {
    fn from(wire: RoleWire) -> Self {
        match wire {
            RoleWire::ProductManager => Self::ProductManager,
            RoleWire::ScrumMaster => Self::ScrumMaster,
            RoleWire::Architect => Self::Architect,
            RoleWire::SoftwareDeveloper => Self::SoftwareDeveloper,
            RoleWire::MarketingSpecialist => Self::MarketingSpecialist,
        }
    }
}

/// The same mapping for the permission tiers an agent is granted (`docs/SPEC.md` section 5.6).
impl From<PermissionTierWire> for PermissionTier {
    fn from(wire: PermissionTierWire) -> Self {
        match wire {
            PermissionTierWire::Read => Self::Read,
            PermissionTierWire::WriteWorkspace => Self::WriteWorkspace,
            PermissionTierWire::Execute => Self::Execute,
            PermissionTierWire::Network => Self::Network,
            PermissionTierWire::GitLocal => Self::GitLocal,
            PermissionTierWire::GitRemote => Self::GitRemote,
            PermissionTierWire::ExternalEffect => Self::ExternalEffect,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::fixtures::{a_full_team_wire, a_team_wire, an_agent_wire};
    use super::{
        AgentStatus, HumanAcceptsContracts, Integration, PermissionTier, PermissionTierWire, Role,
        RoleWire, Team, validate_team,
    };
    use crate::governor::team_rules::{DEFAULT_DOCUMENT_PATHS, DEFAULT_PROTECTED_PATHS};

    fn team(wire: &Value) -> Team {
        validate_team(wire).expect("the fixture is a team")
    }

    /// Dollars, compared the way `pricing`'s tests compare them.
    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    /// Every refusal as a pointer and its message, which is what a caller shows a person.
    fn refusals(wire: &Value) -> Vec<(String, String)> {
        validate_team(wire)
            .expect_err("this wire team is refused")
            .into_iter()
            .map(|error| (error.path, error.message))
            .collect()
    }

    fn paths(wire: &Value) -> Vec<String> {
        refusals(wire).into_iter().map(|(path, _)| path).collect()
    }

    #[test]
    fn reads_a_team_with_only_what_it_must_have() {
        let mut wire = a_team_wire();
        wire["budgets"]
            .as_object_mut()
            .expect("the fixture's budgets are an object")
            .remove("daily_usd");
        let team = team(&wire);
        assert_eq!(team.name.as_str(), "Farik");
        assert_eq!(team.agents.len(), 2);
        assert!(
            team.budgets.daily_usd.is_none(),
            "no daily budget is a day with no dollar limit (ADR 0015)"
        );
        assert!(team.budgets.session.is_none(), "the role's limits stand");
        assert_eq!(
            team.policy.human_accepts_contracts,
            HumanAcceptsContracts::HighRisk
        );
        assert_eq!(team.policy.integration, Integration::Manual);
        assert!(
            team.policy.integration_branch.is_none(),
            "the repository's default branch stands (5.14)"
        );
    }

    #[test]
    fn reads_a_team_with_every_field_it_may_have() {
        let team = team(&a_full_team_wire());
        let ada = &team.agents[0];
        assert_eq!(
            ada.persona.as_deref().map(String::as_str),
            Some("Asks the question nobody asked.")
        );
        assert_eq!(
            ada.model.as_ref().expect("a model").id.as_str(),
            "claude-opus-5"
        );
        assert_eq!(
            ada.preauthorized_external_tools
                .iter()
                .map(|tool| tool.to_string())
                .collect::<Vec<_>>(),
            ["mcp__linear__create_issue"]
        );
        let session = team.budgets.session.as_ref().expect("session limits");
        assert_eq!(
            session.max_input_tokens.map(std::num::NonZeroU64::get),
            Some(200_000)
        );
        assert_eq!(
            team.policy
                .integration_branch
                .as_ref()
                .map(|branch| branch.to_string()),
            Some("trunk".to_string())
        );
        assert_eq!(team.policy.integration, Integration::AutoMerge);
    }

    #[test]
    fn refuses_the_old_local_merge_spelling() {
        let mut wire = a_team_wire();
        wire["policy"]["integration"] = json!("local_merge");
        assert_eq!(paths(&wire), ["/policy/integration"]);
    }

    #[test]
    fn refuses_a_team_that_is_too_small_or_too_large() {
        let mut one = a_team_wire();
        one["agents"] = json!([an_agent_wire("ada", "product_manager")]);
        assert_eq!(paths(&one), ["/agents"], "a team is two agents at least");

        let mut eight = a_team_wire();
        eight["agents"] = Value::Array(
            (0..8)
                .map(|n| an_agent_wire(&format!("agent-{n}"), "software_developer"))
                .collect(),
        );
        assert_eq!(paths(&eight), ["/agents"], "and seven at most");
    }

    #[test]
    fn refuses_a_field_no_schema_knows() {
        let mut wire = a_team_wire();
        wire["mascot"] = json!("a penguin");
        assert_eq!(paths(&wire), ["/"]);
    }

    #[test]
    fn refuses_an_agent_id_that_is_not_a_slug() {
        for id in ["Ada", "ada lovelace", "ada_lovelace", "-ada", ""] {
            let mut wire = a_team_wire();
            wire["agents"][0]["id"] = json!(id);
            assert_eq!(paths(&wire), ["/agents/0/id"], "{id:?}");
        }
    }

    #[test]
    fn refuses_a_role_no_agent_can_hold() {
        // The contract schema's roles include `human`, because a contract may name the human as a
        // reviewer. No agent is a human, so the team schema's roles are the other five.
        let mut wire = a_team_wire();
        wire["agents"][0]["role"] = json!("human");
        assert_eq!(paths(&wire), ["/agents/0/role"]);
    }

    #[test]
    fn names_every_agent_id_that_appears_twice() {
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("ada", "product_manager"),
            an_agent_wire("ada", "software_developer"),
            an_agent_wire("linus", "architect"),
            an_agent_wire("linus", "scrum_master"),
        ]);
        let refusals = refusals(&wire);
        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert_eq!(refusals[0].0, "/agents");
        assert_eq!(
            refusals[0].1,
            "an agent id names one agent, and the ids ada, linus name more than one"
        );
    }

    #[test]
    fn refuses_a_team_with_nobody_to_write_a_contract_or_do_the_work() {
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("ada", "architect"),
            an_agent_wire("linus", "scrum_master"),
        ]);
        let messages: Vec<String> = refusals(&wire)
            .into_iter()
            .map(|(_, message)| message)
            .collect();
        assert_eq!(
            messages,
            [
                "a team needs an active product_manager to write a contract; this one has none \
                 (docs/SPEC.md section 3)",
                "a team needs an active software_developer to do the work; this one has none \
                 (docs/SPEC.md section 3)",
            ],
            "both, not the first of them"
        );
    }

    #[test]
    fn reports_a_repeated_id_and_a_missing_role_together() {
        // Two problems in one file are two refusals: a person fixing one at a time is a person
        // running the command twice.
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("ada", "product_manager"),
            an_agent_wire("ada", "architect"),
        ]);
        let messages: Vec<String> = refusals(&wire)
            .into_iter()
            .map(|(_, message)| message)
            .collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].contains("the id ada names"), "{}", messages[0]);
        assert!(
            messages[1].contains("software_developer"),
            "{}",
            messages[1]
        );
    }

    #[test]
    fn a_paused_product_manager_is_not_an_active_one() {
        // A paused agent keeps the work it holds and is given nothing new, so a team whose only
        // Product Manager is paused cannot start anything.
        let mut wire = a_team_wire();
        wire["agents"][0]["status"] = json!("paused");
        let messages: Vec<String> = refusals(&wire)
            .into_iter()
            .map(|(_, message)| message)
            .collect();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("product_manager"), "{}", messages[0]);
    }

    #[test]
    fn fills_in_every_rule_the_team_left_out() {
        let rules = team(&a_team_wire()).rules();
        assert_eq!(
            rules.protected_paths,
            DEFAULT_PROTECTED_PATHS.map(str::to_string),
            "the five farik-core ships"
        );
        assert!(rules.allowed_paths_ceiling.is_empty(), "no ceiling");
        assert!(rules.required_criteria.is_empty());
        assert!(!rules.require_new_tests);
        assert_eq!(rules.max_task_budget_usd, None, "no cap (ADR 0015)");
        assert!(rules.forbidden_commands.is_empty());
    }

    #[test]
    fn reads_the_rules_a_team_wrote() {
        let rules = team(&a_full_team_wire()).rules();
        assert_eq!(rules.allowed_paths_ceiling, ["src/**"]);
        assert_eq!(rules.required_criteria, ["test", "review"]);
        assert!(rules.require_new_tests);
        assert!(
            rules
                .max_task_budget_usd
                .is_some_and(|cap| close(cap, 12.5))
        );
        assert_eq!(rules.forbidden_commands, ["^rm -rf /"]);
    }

    #[test]
    fn defaults_the_document_paths_when_left_out() {
        // The schema's default fills a key left out; an explicit `[]` is kept, which is how a
        // team says that no task but a Developer's can be ready.
        assert_eq!(
            team(&a_team_wire()).rules().document_paths,
            DEFAULT_DOCUMENT_PATHS.map(str::to_string)
        );
        let mut wire = a_team_wire();
        wire["rules"]["document_paths"] = json!(["notes/**"]);
        assert_eq!(team(&wire).rules().document_paths, ["notes/**"]);
        wire["rules"]["document_paths"] = json!([]);
        assert!(team(&wire).rules().document_paths.is_empty());
    }

    #[test]
    fn keeps_the_protected_paths_it_ships_and_adds_the_team_s() {
        // 5.12: rules only narrow what a tier allows. A team that could drop `.env` from the list
        // would be widening one, so the shipped paths are kept whatever the team writes.
        let mut wire = a_team_wire();
        wire["rules"]["protected_paths"] = json!(["infra/**", ".env"]);
        let rules = team(&wire).rules();
        for shipped in DEFAULT_PROTECTED_PATHS {
            assert!(
                rules.protected_paths.contains(&shipped.to_string()),
                "{shipped} is kept"
            );
        }
        assert!(rules.protected_paths.contains(&"infra/**".to_string()));
        assert_eq!(
            rules
                .protected_paths
                .iter()
                .filter(|path| *path == ".env")
                .count(),
            1,
            "and a path the team repeated is still one path"
        );
    }

    #[test]
    fn counts_only_the_agents_that_take_work() {
        let mut wire = a_team_wire();
        wire["agents"] = json!([
            an_agent_wire("ada", "product_manager"),
            an_agent_wire("linus", "software_developer"),
            an_agent_wire("grace", "architect"),
            an_agent_wire("alan", "scrum_master"),
        ]);
        wire["agents"][2]["status"] = json!("paused");
        wire["agents"][3]["status"] = json!("retired");
        let team = team(&wire);
        assert_eq!(
            team.active_agents()
                .map(|agent| agent.id.to_string())
                .collect::<Vec<_>>(),
            ["ada", "linus"]
        );
        assert!(team.has_active(Role::ProductManager));
        assert!(team.has_active(Role::SoftwareDeveloper));
        assert!(!team.has_active(Role::Architect), "paused");
        assert!(!team.has_active(Role::ScrumMaster), "retired");
        assert!(!team.has_active(Role::Human), "the human is not an agent");
        assert_eq!(team.agents[2].status, AgentStatus::Paused);
    }

    #[test]
    fn a_work_in_progress_limit_of_zero_is_how_an_agent_is_paused() {
        // 5.2: a limit of zero refuses every assignment, which is how a team pauses an agent
        // without retiring it, and the governor already answers "takes no work: its limit is zero".
        // A schema that would not let a person write the number would make that answer unreachable.
        let mut wire = a_team_wire();
        wire["policy"]["wip_limit_per_agent"] = json!(0);
        assert_eq!(team(&wire).policy.wip_limit_per_agent, 0);
    }

    #[test]
    fn refuses_a_number_outside_what_a_rule_allows() {
        // Every bound here carries a spec number: 5.2's work-in-progress limit, 5.7's blocked age
        // and iteration count, 5.5's daily budget, 5.12's task cap and its list of methods. A bound
        // nothing tests is a bound the next person deletes to make something else compile.
        for (pointer, value) in [
            ("/policy/wip_limit_per_agent", json!(-1)),
            ("/policy/wip_limit_per_agent", json!(101)),
            ("/policy/blocked_limit_hours", json!(0)),
            ("/policy/blocked_limit_hours", json!(721)),
            ("/policy/max_iterations", json!(0)),
            ("/policy/max_iterations", json!(101)),
            ("/budgets/daily_usd", json!(0)),
            ("/rules/max_task_budget_usd", json!(0)),
            ("/rules/required_criteria", json!(["vibes"])),
        ] {
            let mut wire = a_full_team_wire();
            *wire
                .pointer_mut(pointer)
                .expect("the full fixture has every field") = value.clone();
            let paths = paths(&wire);
            assert!(
                !paths.is_empty() && paths.iter().all(|path| path.starts_with(pointer)),
                "{pointer} = {value}: {paths:?}"
            );
        }
    }

    #[test]
    fn reads_a_whole_number_a_person_wrote_with_a_fraction() {
        // YAML and JSON both call 2.0 a number and JSON Schema calls it an integer, so a file that
        // says `wip_limit_per_agent: 2.0` is one the schema accepts. Normalising it is what keeps
        // the typed build from refusing it afterwards at the root, with a message about serde
        // rather than about the field.
        let mut wire = a_full_team_wire();
        wire["policy"]["wip_limit_per_agent"] = json!(2.0);
        wire["policy"]["blocked_limit_hours"] = json!(24.0);
        wire["policy"]["max_iterations"] = json!(3.0);
        wire["budgets"]["session"]["max_tool_calls"] = json!(200.0);
        let team = team(&wire);
        assert_eq!(team.policy.wip_limit_per_agent, 2);
        assert_eq!(team.policy.blocked_limit_hours.get(), 24);
        assert_eq!(team.policy.max_iterations.get(), 3);
        assert_eq!(
            team.budgets
                .session
                .expect("session limits")
                .max_tool_calls
                .map(std::num::NonZeroU64::get),
            Some(200)
        );
    }

    #[test]
    fn refuses_a_number_or_a_list_the_schema_will_not_hold() {
        // Every one of these is refused by the schema, at its own pointer, rather than by the typed
        // build at the root: a person is told which field they got wrong. The session limits are
        // capped at the largest integer JSON holds exactly, which is what makes 1e19 a refusal
        // here instead of a sentence about serde and a nonzero u64.
        let long_name = "a".repeat(101);
        for (pointer, value) in [
            ("/budgets/session/max_input_tokens", json!(1.0e19)),
            ("/name", json!(long_name.clone())),
            ("/agents/0/id", json!("a".repeat(65))),
            ("/agents/0/display_name", json!(long_name.clone())),
            ("/agents/0/model/id", json!(long_name.clone())),
            ("/policy/integration_branch", json!("a".repeat(201))),
            ("/rules/protected_paths", json!(vec!["src/**"; 101])),
            ("/rules/forbidden_commands", json!(vec![""; 1])),
        ] {
            let mut wire = a_full_team_wire();
            *wire
                .pointer_mut(pointer)
                .expect("the full fixture has every field") = value.clone();
            let paths = paths(&wire);
            assert!(
                !paths.is_empty() && paths.iter().all(|path| path.starts_with(pointer)),
                "{pointer}: {paths:?}"
            );
        }
    }

    #[test]
    fn a_role_s_tiers_are_the_user_s_to_widen_and_to_narrow() {
        // 5.6: a role's tiers are overridable per agent. Ada is a Product Manager, whose defaults
        // are read and network; she is granted execute and denied network.
        let team = team(&a_full_team_wire());
        assert_eq!(
            team.agents[0].tiers(),
            [PermissionTier::Read, PermissionTier::Execute]
        );
        assert_eq!(
            team.agents[1].tiers(),
            [
                PermissionTier::Read,
                PermissionTier::WriteWorkspace,
                PermissionTier::Execute,
                PermissionTier::GitLocal,
            ],
            "and an agent that overrides nothing holds what its role holds"
        );
    }

    #[test]
    fn taking_a_tier_away_wins_over_granting_it() {
        // Both lists naming one tier is a person's mistake, and the narrower reading of a mistake
        // is the safer one.
        let mut wire = a_team_wire();
        wire["agents"][1]["grants"] = json!(["git_remote", "read"]);
        wire["agents"][1]["revokes"] = json!(["git_remote", "execute"]);
        let team = team(&wire);
        assert_eq!(
            team.agents[1].tiers(),
            [
                PermissionTier::Read,
                PermissionTier::WriteWorkspace,
                PermissionTier::GitLocal,
            ],
            "execute taken away, git_remote granted and taken away, read granted twice over"
        );
    }

    #[test]
    fn refuses_an_agent_that_cannot_read() {
        // 5.6: everyone reads. An agent with read taken away is one every tool call is refused
        // for, in every session it is ever given, and the file that did it would look fine. Pause
        // is how a team stops an agent.
        let mut wire = a_team_wire();
        wire["agents"][1]["revokes"] = json!(["execute", "read"]);
        let refusals = refusals(&wire);
        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert_eq!(refusals[0].0, "/agents/1/revokes");
        assert_eq!(
            refusals[0].1,
            "linus cannot have read taken away: everyone reads (docs/SPEC.md section 5.6), and an \
             agent that cannot read is one every tool call is refused for. A team pauses an agent \
             instead."
        );
    }

    #[test]
    fn names_every_role_an_agent_can_hold() {
        assert_eq!(
            [
                RoleWire::ProductManager,
                RoleWire::ScrumMaster,
                RoleWire::Architect,
                RoleWire::SoftwareDeveloper,
                RoleWire::MarketingSpecialist,
            ]
            .map(Role::from),
            [
                Role::ProductManager,
                Role::ScrumMaster,
                Role::Architect,
                Role::SoftwareDeveloper,
                Role::MarketingSpecialist,
            ]
        );
    }

    #[test]
    fn names_every_permission_tier_an_agent_can_be_granted() {
        assert_eq!(
            [
                PermissionTierWire::Read,
                PermissionTierWire::WriteWorkspace,
                PermissionTierWire::Execute,
                PermissionTierWire::Network,
                PermissionTierWire::GitLocal,
                PermissionTierWire::GitRemote,
                PermissionTierWire::ExternalEffect,
            ]
            .map(PermissionTier::from),
            [
                PermissionTier::Read,
                PermissionTier::WriteWorkspace,
                PermissionTier::Execute,
                PermissionTier::Network,
                PermissionTier::GitLocal,
                PermissionTier::GitRemote,
                PermissionTier::ExternalEffect,
            ]
        );
    }
}
