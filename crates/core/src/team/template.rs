//! Team templates (ADR 0026 C, `docs/SPEC.md` section 3): a team saved on the machine to use in
//! any project, held to `docs/schemas/team-template.schema.json`. A template keeps what makes the
//! team the user's (names, roles, personas, pictures, models, and the four answers setup asks)
//! and leaves out everything that belongs to one project.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use jsonschema::Validator;
use jsonschema::error::ValidationErrorKind;
use serde_json::{Value, json};

use super::{AgentStatus, Team, ValidationError, defaults, validate_team};
use crate::contract::{pointer, with_integers_normalised};

pub use crate::generated::team_template::{TeamTemplate, TemplateAgent};

const SCHEMA_JSON: &str = include_str!("../../../../docs/schemas/team-template.schema.json");

/// Where an uploaded picture lives in a project; a template leaves one out, since nothing copies
/// it to another project yet.
const UPLOADED: &str = ".farik/team/avatars/";

/// The longest slug, as long as an agent id may be.
const SLUG_MAX: usize = 64;

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded template schema is valid JSON: it is the file in docs/schemas/ \
         that typify generated this crate's types from at compile time",
    );
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the embedded template schema compiles: it is JSON Schema 2020-12 with no external references")
});

/// A template's slug, which names its file: the CLI's project slug rule (ASCII letters and digits
/// lower-cased, every run of anything else one `-`, trimmed), at most 64 characters. `None` when
/// nothing is left, which is a name no template may have.
#[must_use]
pub fn template_slug(name: &str) -> Option<String> {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(SLUG_MAX);
    let slug = slug.trim_end_matches('-');
    (!slug.is_empty()).then(|| slug.to_string())
}

/// Checks a value against the template schema, then holds the team it makes (every agent active,
/// over `defaults()`) to `validate_team`, so a hand-edited template gets a team's plain sentences,
/// each at its template path: the team keeps the template's agents in order and its answers where
/// a team keeps them.
///
/// A key the schema does not know is refused at its own pointer, `/rules` rather than `/`, since
/// a template is a file a person may edit by hand.
///
/// # Errors
///
/// Every schema violation at its own pointer; or, when the schema passes, `validate_team`'s.
pub fn validate_template(input: &Value) -> Result<TeamTemplate, Vec<ValidationError>> {
    let mut errors = Vec::new();
    for error in VALIDATOR.iter_errors(input) {
        let path = error.instance_path().to_string();
        match error.kind() {
            ValidationErrorKind::AdditionalProperties { unexpected } => {
                errors.extend(unexpected.iter().map(|key| ValidationError {
                    path: format!("{path}/{key}"),
                    message: error.to_string(),
                }));
            }
            _ => errors.push(ValidationError {
                path: pointer(&path),
                message: error.to_string(),
            }),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let template = serde_json::from_value::<TeamTemplate>(with_integers_normalised(input))
        .map_err(|error| {
            vec![ValidationError {
                path: "/".to_string(),
                message: format!(
                    "the schema passed but the typed template could not be built: {error}"
                ),
            }]
        })?;
    validate_team(&team_wire(&template))?;
    Ok(template)
}

/// The wire team a template makes: its agents, each active, and its answers over the defaults.
fn team_wire(template: &TeamTemplate) -> Value {
    let mut policy = defaults().policy;
    policy.permissions = Some(template.policy.permissions.clone());
    policy.judgment = Some(template.policy.judgment.clone());
    policy.integration = template.policy.integration;
    let agents: Vec<Value> = template
        .agents
        .iter()
        .map(|agent| {
            let mut wire = json!(agent);
            wire["status"] = json!("active");
            wire
        })
        .collect();
    json!({
        "name": template.name,
        "agents": agents,
        "budgets": template.budgets,
        "policy": policy,
        "rules": {}
    })
}

/// The template of a team: its agents that are not retired, without status, grants, revokes,
/// preauthorized tools, connectors or an uploaded picture; its permissions, plan check and
/// integration written out; its daily limit. The name is trimmed.
///
/// # Errors
///
/// `validate_template`'s, which for a valid team is only a name the schema refuses, at `/name`.
pub fn template_from_team(
    team: &Team,
    name: &str,
    saved_at: DateTime<Utc>,
) -> Result<TeamTemplate, Vec<ValidationError>> {
    let agents: Vec<Value> = team
        .agents
        .iter()
        .filter(|agent| agent.status != AgentStatus::Retired)
        .map(|agent| {
            let mut wire = json!({
                "id": agent.id,
                "display_name": agent.display_name,
                "role": agent.role,
                "persona": agent.persona,
                "avatar": agent.avatar.as_ref().filter(|avatar| !avatar.starts_with(UPLOADED)),
                "model": agent.model,
            });
            if let Some(fields) = wire.as_object_mut() {
                fields.retain(|_, value| !value.is_null());
            }
            wire
        })
        .collect();
    let mut budgets = json!({});
    if let Some(daily_usd) = team.budgets.daily_usd {
        budgets["daily_usd"] = json!(daily_usd);
    }
    validate_template(&json!({
        "version": 1,
        "name": name.trim(),
        "saved_at": saved_at,
        "agents": agents,
        "policy": {
            "permissions": team.permissions(),
            "judgment": team.policy.judgment.clone().unwrap_or_default(),
            "integration": team.policy.integration,
        },
        "budgets": budgets,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{DateTime, Utc};
    use serde_json::{Value, json};

    use super::{template_from_team, template_slug, validate_template};
    use crate::team::fixtures::{a_full_team_wire, a_team_wire, a_template_wire, an_agent_wire};
    use crate::team::{Team, validate_team};

    const TEAM_SCHEMA: &str = include_str!("../../../../docs/schemas/team.schema.json");
    const TEMPLATE_SCHEMA: &str =
        include_str!("../../../../docs/schemas/team-template.schema.json");

    fn at() -> DateTime<Utc> {
        "2026-09-30T12:00:00Z".parse().expect("a date-time")
    }

    fn team(wire: &Value) -> Team {
        validate_team(wire).expect("the fixture is a team")
    }

    /// The template of a wire team, as a JSON value, which is what lands in the file.
    fn template_of(wire: &Value) -> Value {
        let template =
            template_from_team(&team(wire), "My usual team", at()).expect("a valid template");
        serde_json::to_value(template).expect("a template serialises")
    }

    fn keys(value: &Value) -> BTreeSet<String> {
        value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect()
    }

    fn refusals(wire: &Value) -> Vec<(String, String)> {
        validate_template(wire)
            .expect_err("this wire template is refused")
            .into_iter()
            .map(|error| (error.path, error.message))
            .collect()
    }

    #[test]
    fn copies_the_team_schemas_definitions() {
        let team: Value = serde_json::from_str(TEAM_SCHEMA).expect("JSON");
        let template: Value = serde_json::from_str(TEMPLATE_SCHEMA).expect("JSON");
        for (copy, original) in [
            ("/$defs/role", "/$defs/role"),
            ("/$defs/model", "/$defs/model"),
            ("/$defs/permissions", "/$defs/permissions"),
            ("/$defs/judgment", "/$defs/judgment"),
            ("/$defs/integration", "/$defs/policy/properties/integration"),
        ] {
            let original = team.pointer(original).expect("the team schema has it");
            assert_eq!(template.pointer(copy), Some(original), "{copy}");
        }
    }

    #[test]
    fn holds_each_agent_and_the_four_answers() {
        let mut wire = a_team_wire();
        wire["agents"][0]["persona"] = json!("Plans first.");
        wire["agents"][0]["avatar"] = json!("product-manager");
        wire["agents"][0]["model"] = json!({ "id": "claude-sonnet-5", "effort": "low" });
        wire["agents"][1]["display_name"] = json!("Linus T.");
        wire["policy"]
            .as_object_mut()
            .expect("the fixture's policy is an object")
            .remove("judgment");
        assert!(wire["policy"].get("permissions").is_none());
        assert_eq!(
            template_of(&wire),
            json!({
                "version": 1,
                "name": "My usual team",
                "saved_at": "2026-09-30T12:00:00Z",
                "agents": [
                    {
                        "id": "ada",
                        "display_name": "ada",
                        "role": "product_manager",
                        "persona": "Plans first.",
                        "avatar": "product-manager",
                        "model": { "id": "claude-sonnet-5", "effort": "low" }
                    },
                    { "id": "linus", "display_name": "Linus T.", "role": "software_developer" }
                ],
                "policy": {
                    "permissions": { "run_commands": true, "push": false },
                    "judgment": {
                        "required": "always",
                        "questions": [
                            "Does the task fit its budget?",
                            "Would its checks notice if the work went wrong the way its intent worries about?"
                        ],
                        "judge": "auto"
                    },
                    "integration": "manual"
                },
                "budgets": { "daily_usd": 20.0 }
            })
        );
    }

    #[test]
    fn leaves_out_what_belongs_to_the_project() {
        let mut wire = a_full_team_wire();
        let mut sol = an_agent_wire("sol", "scrum_master");
        sol["status"] = json!("paused");
        let mut old = an_agent_wire("old", "architect");
        old["status"] = json!("retired");
        let agents = wire["agents"].as_array_mut().expect("agents");
        agents.extend([sol, old]);
        let template = template_of(&wire);
        assert_eq!(
            keys(&template),
            ["agents", "budgets", "name", "policy", "saved_at", "version"]
                .map(str::to_string)
                .into(),
            "no rules, preview or team name"
        );
        assert_eq!(
            keys(&template["policy"]),
            ["integration", "judgment", "permissions"]
                .map(str::to_string)
                .into(),
            "no integration branch or other policy key"
        );
        assert_eq!(
            keys(&template["budgets"]),
            ["daily_usd"].map(str::to_string).into(),
            "no session limits"
        );
        let ids: Vec<&str> = template["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|agent| agent["id"].as_str().expect("an id"))
            .collect();
        assert_eq!(
            ids,
            ["ada", "linus", "sol"],
            "the paused agent held, the retired one not"
        );
        let allowed: BTreeSet<String> =
            ["id", "display_name", "role", "persona", "avatar", "model"]
                .map(str::to_string)
                .into();
        for agent in template["agents"].as_array().expect("agents") {
            assert!(keys(agent).is_subset(&allowed), "{agent}");
        }
        assert_eq!(
            keys(&template["agents"][0]),
            allowed,
            "Ada keeps her persona, picture and model"
        );
    }

    #[test]
    fn leaves_out_an_uploaded_picture() {
        let mut wire = a_team_wire();
        wire["agents"][0]["avatar"] = json!(".farik/team/avatars/mira.png");
        wire["agents"][1]["avatar"] = json!("developer");
        let template = template_of(&wire);
        assert!(template["agents"][0].get("avatar").is_none());
        assert_eq!(template["agents"][1]["avatar"], "developer");
    }

    #[test]
    fn validates_a_saved_template() {
        let template = validate_template(&a_template_wire()).expect("a template");
        assert_eq!(template.name.as_str(), "Three of us");
        assert_eq!(template.agents.len(), 2);
        assert_eq!(template.budgets.daily_usd, Some(20.0));
        assert_eq!(
            serde_json::to_value(&template).expect("serialises")["saved_at"],
            "2026-09-30T12:00:00Z"
        );

        // What template_from_team writes is a template validate_template reads back, equal.
        let mut wire = a_full_team_wire();
        wire["agents"][0]["avatar"] = json!("product-manager");
        let saved = template_from_team(&team(&wire), "  Padded  ", at()).expect("a template");
        assert_eq!(saved.name.as_str(), "Padded", "the name is trimmed");
        let read = validate_template(&serde_json::to_value(&saved).expect("serialises"))
            .expect("read back");
        assert_eq!(read, saved);

        for name in ["", "   ", &"a".repeat(61)] {
            let refused = template_from_team(&team(&wire), name, at())
                .expect_err("a name the schema refuses");
            assert!(
                refused.iter().all(|error| error.path == "/name"),
                "{name:?}: {refused:?}"
            );
        }
    }

    #[test]
    fn refuses_a_template_without_a_developer() {
        let mut wire = a_template_wire();
        wire["agents"][1]["role"] = json!("architect");
        assert_eq!(
            refusals(&wire),
            [(
                "/agents".to_string(),
                "A team needs an active Software Developer to do the work, and this one has none."
                    .to_string()
            )]
        );
    }

    #[test]
    fn refuses_a_template_without_a_product_manager() {
        let mut wire = a_template_wire();
        wire["agents"][0]["role"] = json!("scrum_master");
        assert_eq!(
            refusals(&wire),
            [(
                "/agents".to_string(),
                "A team needs an active Product Manager to write its plans, and this one has none."
                    .to_string()
            )]
        );
    }

    #[test]
    fn refuses_eight_agents() {
        let mut wire = a_template_wire();
        let agents = wire["agents"].as_array_mut().expect("agents");
        agents.extend((0..6).map(|n| {
            json!({ "id": format!("dev-{n}"), "display_name": "Dev", "role": "software_developer" })
        }));
        let refusals = refusals(&wire);
        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert_eq!(refusals[0].0, "/agents");
        assert!(
            refusals[0].1.contains("more than 7 items"),
            "the schema's: {}",
            refusals[0].1
        );

        wire["agents"].as_array_mut().expect("agents").pop();
        validate_template(&wire).expect("seven is a template");
    }

    #[test]
    fn refuses_a_judge_the_template_does_not_hold() {
        let mut wire = a_template_wire();
        wire["policy"]["judgment"]["judge"] = json!("architect");
        assert_eq!(
            refusals(&wire),
            [(
                "/policy/judgment/judge".to_string(),
                "No active Architect can check plans. Let Farik choose, or add one.".to_string()
            )]
        );
    }

    #[test]
    fn refuses_unknown_keys() {
        let mut wire = a_template_wire();
        wire["rules"] = json!({});
        assert_eq!(
            refusals(&wire)
                .into_iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>(),
            ["/rules"]
        );

        let mut wire = a_template_wire();
        wire["agents"][1]["status"] = json!("active");
        wire["policy"]["integration_branch"] = json!("trunk");
        assert_eq!(
            refusals(&wire)
                .into_iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>(),
            ["/agents/1/status", "/policy/integration_branch"]
        );
    }

    #[test]
    fn slugs_a_template_name() {
        assert_eq!(
            template_slug("My usual team").as_deref(),
            Some("my-usual-team")
        );
        assert_eq!(template_slug("Team #2!").as_deref(), Some("team-2"));
        assert_eq!(template_slug("!!!"), None);
        assert_eq!(template_slug(&"a".repeat(100)), Some("a".repeat(64)));
        let long = template_slug(&"ab ".repeat(30)).expect("a slug");
        assert!(long.len() <= 64 && !long.ends_with('-'), "{long}");
        assert_eq!(template_slug(" -Lead"), Some("lead".to_string()));
    }
}
