//! The team (`docs/SPEC.md` sections 3, 5.12, 5.14 and 5.16): `docs/schemas/team.schema.json` as
//! Rust types, the validator that turns an untrusted JSON value into one, and the three rules the
//! schema cannot say.

use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;

use crate::contract::{pointer, with_integers_normalised};

pub use crate::contract::{Role, ValidationError};
pub use crate::generated::team::{
    Agent, AgentStatus, Budgets as TeamBudgets, FarikTeam as Team, Model as AgentModel,
    ModelEffort as Effort, PermissionTier as PermissionTierWire, Policy as TeamPolicy,
    PolicyHumanAcceptsContracts as HumanAcceptsContracts, PolicyIntegration as Integration,
    Role as RoleWire, Rules as RulesWire, SessionLimits as SessionLimitsWire,
};

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

const SCHEMA_JSON: &str = include_str!("generated/team.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded team schema is valid JSON: it is a copy of docs/schemas/ written by \
         cargo xtask generate and checked for freshness by cargo xtask check",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect(
            "the embedded team schema compiles: it is JSON Schema 2020-12 with no external \
             references, and the generator already parsed it",
        )
});

/// Checks a value against `docs/schemas/team.schema.json` and, when it conforms, returns the typed
/// team.
///
/// # Errors
///
/// Every schema violation, each at its own JSON pointer; or, when the schema passes and the typed
/// team cannot be built, one error at the root.
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
    serde_json::from_value::<Team>(with_integers_normalised(input)).map_err(|error| {
        vec![ValidationError {
            path: "/".to_string(),
            message: format!("the schema passed but the typed team could not be built: {error}"),
        }]
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::fixtures::{a_full_team_wire, a_team_wire, an_agent_wire};
    use super::{HumanAcceptsContracts, Integration, Team, validate_team};

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
        let team = team(&a_team_wire());
        assert_eq!(team.name.as_str(), "Farik");
        assert_eq!(team.agents.len(), 2);
        assert!(close(team.budgets.daily_usd, 20.0));
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
}
