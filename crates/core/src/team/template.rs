//! Team templates (ADR 0026 C, `docs/SPEC.md` section 3): a team saved on the machine to use in
//! any project, held to `docs/schemas/team-template.schema.json`. A template keeps what makes the
//! team the user's (names, roles, personas, pictures, models, and the four answers setup asks)
//! and leaves out everything that belongs to one project.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use jsonschema::Validator;
use jsonschema::error::ValidationErrorKind;
use serde_json::{Value, json};

use super::{Agent, AgentStatus, Team, ValidationError, defaults, validate_team};
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
/// preauthorized tools, connectors or an uploaded picture; its permissions, plan check,
/// integration and whether it plans in sprints written out; its daily limit. The name is trimmed.
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
            "plan_in_sprints": team.plans_in_sprints(),
        },
        "budgets": budgets,
    }))
}

/// What applying a template to a team makes: the team, and which agents (by id) it kept, retired,
/// removed and added. `errors` are `validate_team`'s for the result, empty when it is a team.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateApplied {
    /// The team after applying.
    pub team: Team,
    /// Project agents that matched a template agent by id and role.
    pub kept: Vec<String>,
    /// Project agents that had worked and were not kept.
    pub retired: Vec<String>,
    /// Project agents that never worked and were not kept: gone from the file.
    pub removed: Vec<String>,
    /// Template agents that joined, by the id each got.
    pub added: Vec<String>,
    /// `validate_team`'s errors for `team`.
    pub errors: Vec<ValidationError>,
}

/// Applies a template to a team by step 06's switching rules (`docs/SPEC.md` 4.4). Pure: the
/// caller writes the result and records what follows from it.
///
/// - A template agent whose id and role match a project agent that is not retired keeps that
///   agent: its name is the template's, and its persona, picture and model too where the template
///   has them; its status, grants, revokes and connectors stay. A paused match stays paused.
/// - Every other agent that is not retired is retired when it has worked (an id in `worked`, the
///   agents some event names) and removed from the file when it has not.
/// - Every template agent not kept joins, active, with the persona, picture, model and connectors
///   of the `suggested` agent of its role (`team.propose`'s) for each the template leaves out; its
///   id takes `-2`, `-3`… when an agent left in the file holds it.
/// - The team takes the template's permissions, plan check, integration and daily limit (none
///   clears it), and whether it plans in sprints when the template says (none keeps the
///   project's); everything else is the project's. Agents already retired are left as they are.
///
/// The result is held to `validate_team`, whose errors come back in `errors` beside the lists:
/// a kept match that is paused can leave a required role with no active agent.
///
/// # Panics
///
/// Never: a template agent's id, name, persona and picture are held to the same constraints as a
/// team agent's (the template schema copies them), so each moves into an `Agent` as it is.
#[must_use]
pub fn apply_template(
    current: &Team,
    template: &TeamTemplate,
    worked: &BTreeSet<String>,
    suggested: &[Agent],
) -> TemplateApplied {
    let mut team = current.clone();
    let (mut kept, mut retired, mut removed, mut added) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    team.agents.retain_mut(|agent| {
        let id = agent.id.to_string();
        if agent.status == AgentStatus::Retired {
            return true;
        }
        if let Some(from) = template
            .agents
            .iter()
            .find(|from| from.id.as_str() == id && from.role == agent.role)
        {
            take(agent, from);
            kept.push(id);
            true
        } else if worked.contains(&id) {
            agent.status = AgentStatus::Retired;
            retired.push(id);
            true
        } else {
            removed.push(id);
            false
        }
    });
    for from in template
        .agents
        .iter()
        .filter(|from| !kept.iter().any(|id| id == from.id.as_str()))
    {
        let id = free_id(&team, from.id.as_str());
        let mut agent: Agent = moved(&json!({
            "id": id,
            "display_name": from.display_name,
            "role": from.role,
            "status": "active",
        }));
        if let Some(start) = suggested.iter().find(|start| start.role == from.role) {
            agent.persona.clone_from(&start.persona);
            agent.avatar.clone_from(&start.avatar);
            agent.model.clone_from(&start.model);
            agent.mcp_servers.clone_from(&start.mcp_servers);
        }
        take(&mut agent, from);
        added.push(id);
        team.agents.push(agent);
    }
    team.policy.permissions = Some(template.policy.permissions.clone());
    team.policy.judgment = Some(template.policy.judgment.clone());
    team.policy.integration = template.policy.integration;
    if let Some(plan_in_sprints) = template.policy.plan_in_sprints {
        team.policy.plan_in_sprints = Some(plan_in_sprints);
    }
    team.budgets.daily_usd = template.budgets.daily_usd;
    let errors = serde_json::to_value(&team)
        .map_err(|error| {
            vec![ValidationError {
                path: "/".to_string(),
                message: format!("the team could not be written out: {error}"),
            }]
        })
        .and_then(|wire| validate_team(&wire))
        .err()
        .unwrap_or_default();
    TemplateApplied {
        team,
        kept,
        retired,
        removed,
        added,
        errors,
    }
}

/// A template agent's name, and its persona, picture and model where it has them, on `agent`.
fn take(agent: &mut Agent, from: &TemplateAgent) {
    agent.display_name = moved(&json!(from.display_name));
    if let Some(persona) = &from.persona {
        agent.persona = Some(moved(&json!(persona)));
    }
    if let Some(avatar) = &from.avatar {
        agent.avatar = Some(moved(&json!(avatar)));
    }
    if let Some(model) = &from.model {
        agent.model = Some(model.clone());
    }
}

/// `id`, or the first of `id-2`, `id-3`… that no agent in `team` holds, its base cut short so
/// that it stays within an id's 64 characters.
fn free_id(team: &Team, id: &str) -> String {
    let held = |candidate: &str| {
        team.agents
            .iter()
            .any(|agent| agent.id.as_str() == candidate)
    };
    let mut candidate = id.to_string();
    let mut n = 1;
    while held(&candidate) {
        n += 1;
        let suffix = format!("-{n}");
        let base = id[..id.len().min(SLUG_MAX - suffix.len())].trim_end_matches('-');
        candidate = format!("{base}{suffix}");
    }
    candidate
}

/// A template value as the team type of the same constraints.
fn moved<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(value.clone())
        .expect("the template schema holds this value to the team schema's constraints")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{DateTime, Utc};
    use serde_json::{Value, json};

    use super::{
        TemplateApplied, apply_template, template_from_team, template_slug, validate_template,
    };
    use crate::team::fixtures::{a_full_team_wire, a_team_wire, a_template_wire, an_agent_wire};
    use crate::team::{Agent, AgentStatus, Team, validate_team};

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
                    "integration": "manual",
                    "plan_in_sprints": false
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
            ["integration", "judgment", "permissions", "plan_in_sprints"]
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

    // Applying a template (Task 3).

    const NEEDS_DEVELOPER: &str =
        "A team needs an active Software Developer to do the work, and this one has none.";

    /// The agents `team.propose` suggests, one per role: each with its role's persona, picture and
    /// model, and the Designer with its connector (step 12).
    fn suggested() -> Vec<Agent> {
        [
            "product_manager",
            "scrum_master",
            "architect",
            "software_developer",
            "ui_ux_designer",
            "marketing_specialist",
        ]
        .iter()
        .map(|role| {
            let mut wire = json!({
                "id": role.replace('_', "-"),
                "display_name": role,
                "role": role,
                "status": "active",
                "persona": format!("A {role}."),
                "avatar": format!("{role}-picture"),
                "model": { "id": format!("{role}-model"), "effort": "medium" }
            });
            if *role == "ui_ux_designer" {
                wire["mcp_servers"] = json!([{ "name": "playwright", "source": "builtin" }]);
            }
            serde_json::from_value(wire).expect("an agent")
        })
        .collect()
    }

    fn with_status(id: &str, role: &str, status: &str) -> Value {
        let mut wire = an_agent_wire(id, role);
        wire["status"] = json!(status);
        wire
    }

    fn project(agents: &[Value]) -> Value {
        let mut wire = a_team_wire();
        wire["agents"] = json!(agents);
        wire
    }

    fn joining(id: &str, name: &str, role: &str) -> Value {
        json!({ "id": id, "display_name": name, "role": role })
    }

    fn template_with(agents: &[Value]) -> Value {
        let mut wire = a_template_wire();
        wire["agents"] = json!(agents);
        wire
    }

    fn applied(project: &Value, template: &Value, worked: &[&str]) -> TemplateApplied {
        let template = validate_template(template).expect("a template");
        let worked = worked.iter().map(|id| (*id).to_string()).collect();
        apply_template(&team(project), &template, &worked, &suggested())
    }

    /// The agent `id` of the result, as it lands in `team.yaml`.
    fn agent(applied: &TemplateApplied, id: &str) -> Option<Value> {
        applied
            .team
            .agents
            .iter()
            .find(|agent| agent.id.as_str() == id)
            .map(|agent| serde_json::to_value(agent).expect("an agent serialises"))
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_string()).collect()
    }

    fn errors(applied: &TemplateApplied) -> Vec<(String, String)> {
        applied
            .errors
            .iter()
            .map(|error| (error.path.clone(), error.message.clone()))
            .collect()
    }

    #[test]
    fn keeps_an_agent_by_id_and_role() {
        let mira = json!({
            "id": "mira",
            "display_name": "Mira P.",
            "role": "product_manager",
            "status": "active",
            "persona": "Old.",
            "avatar": "old",
            "model": { "id": "claude-sonnet-5" },
            "grants": ["execute"],
            "revokes": ["network"],
            "preauthorized_external_tools": ["mcp__linear__create_issue"],
            "mcp_servers": [{ "name": "playwright", "source": "builtin" }]
        });
        let result = applied(
            &project(&[mira, an_agent_wire("theo", "software_developer")]),
            &a_template_wire(),
            &[],
        );
        assert_eq!(result.kept, ids(&["mira", "theo"]));
        assert!(result.retired.is_empty() && result.removed.is_empty() && result.added.is_empty());
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(
            agent(&result, "mira"),
            Some(json!({
                "id": "mira",
                "display_name": "Mira",
                "role": "product_manager",
                "status": "active",
                "persona": "Mira.",
                "avatar": "product-manager",
                "model": { "id": "claude-opus-5", "effort": "high" },
                "grants": ["execute"],
                "revokes": ["network"],
                "preauthorized_external_tools": ["mcp__linear__create_issue"],
                "mcp_servers": [{ "name": "playwright", "source": "builtin" }]
            }))
        );
        assert_eq!(
            agent(&result, "theo"),
            Some(
                json!({ "id": "theo", "display_name": "Theo", "role": "software_developer", "status": "active" })
            )
        );
    }

    #[test]
    fn keeps_a_paused_match_paused() {
        let result = applied(
            &project(&[
                an_agent_wire("mira", "product_manager"),
                with_status("theo", "software_developer", "paused"),
                an_agent_wire("linus", "software_developer"),
            ]),
            &template_with(&[
                joining("mira", "Mira", "product_manager"),
                joining("theo", "Theo", "software_developer"),
                joining("linus", "Linus", "software_developer"),
            ]),
            &["theo"],
        );
        assert_eq!(result.kept, ids(&["mira", "theo", "linus"]));
        assert_eq!(
            agent(&result, "theo").expect("theo")["status"],
            "paused",
            "the card changes a status, not a template"
        );
    }

    #[test]
    fn keeps_the_projects_value_where_the_template_has_none() {
        let mut mira = an_agent_wire("mira", "product_manager");
        mira["persona"] = json!("Plans.");
        mira["avatar"] = json!(".farik/team/avatars/mira.png");
        mira["model"] = json!({ "id": "claude-sonnet-5" });
        let mut kai = joining("kai", "Kai", "marketing_specialist");
        kai["persona"] = json!("Tells.");
        kai["avatar"] = json!("marketing-specialist");
        kai["model"] = json!({ "id": "claude-haiku-5" });
        let result = applied(
            &project(&[mira, an_agent_wire("theo", "software_developer")]),
            &template_with(&[
                joining("mira", "Mira", "product_manager"),
                joining("theo", "Theo", "software_developer"),
                joining("iris", "Iris", "ui_ux_designer"),
                kai,
            ]),
            &[],
        );
        let mira = agent(&result, "mira").expect("mira");
        assert_eq!(mira["persona"], "Plans.");
        assert_eq!(mira["avatar"], ".farik/team/avatars/mira.png");
        assert_eq!(mira["model"], json!({ "id": "claude-sonnet-5" }));
        assert_eq!(result.added, ids(&["iris", "kai"]));
        assert_eq!(
            agent(&result, "iris"),
            Some(json!({
                "id": "iris",
                "display_name": "Iris",
                "role": "ui_ux_designer",
                "status": "active",
                "persona": "A ui_ux_designer.",
                "avatar": "ui_ux_designer-picture",
                "model": { "id": "ui_ux_designer-model", "effort": "medium" },
                "mcp_servers": [{ "name": "playwright", "source": "builtin" }]
            })),
            "an added agent takes its role's, the Designer its connector"
        );
        assert_eq!(
            agent(&result, "kai"),
            Some(json!({
                "id": "kai",
                "display_name": "Kai",
                "role": "marketing_specialist",
                "status": "active",
                "persona": "Tells.",
                "avatar": "marketing-specialist",
                "model": { "id": "claude-haiku-5" }
            }))
        );
    }

    #[test]
    fn refuses_when_a_kept_match_is_paused() {
        let result = applied(
            &project(&[
                an_agent_wire("mira", "product_manager"),
                an_agent_wire("ada", "software_developer"),
                with_status("theo", "software_developer", "paused"),
            ]),
            &a_template_wire(),
            &["ada"],
        );
        assert_eq!(result.kept, ids(&["mira", "theo"]));
        assert_eq!(result.retired, ids(&["ada"]));
        assert_eq!(
            errors(&result),
            [("/agents".to_string(), NEEDS_DEVELOPER.to_string())]
        );
    }

    fn mira_ada_theo() -> Value {
        project(&[
            an_agent_wire("mira", "product_manager"),
            an_agent_wire("ada", "software_developer"),
            an_agent_wire("theo", "software_developer"),
        ])
    }

    fn mira_and_ada() -> Value {
        template_with(&[
            joining("mira", "Mira", "product_manager"),
            joining("ada", "Ada", "software_developer"),
        ])
    }

    #[test]
    fn retires_a_worked_agent_it_does_not_hold() {
        let result = applied(&mira_ada_theo(), &mira_and_ada(), &["theo", "mira"]);
        assert_eq!(result.kept, ids(&["mira", "ada"]), "a worked match is kept");
        assert_eq!(result.retired, ids(&["theo"]));
        assert!(result.removed.is_empty());
        assert_eq!(agent(&result, "theo").expect("theo")["status"], "retired");
        assert_eq!(agent(&result, "mira").expect("mira")["status"], "active");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn removes_an_agent_that_never_worked() {
        let result = applied(&mira_ada_theo(), &mira_and_ada(), &["mira"]);
        assert_eq!(result.removed, ids(&["theo"]));
        assert!(result.retired.is_empty());
        assert_eq!(agent(&result, "theo"), None);
        assert_eq!(
            result
                .team
                .agents
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            ["mira", "ada"]
        );
    }

    #[test]
    fn adds_the_rest_with_a_free_id() {
        // The project's theo is retired: the template's takes theo-2, or theo-3 when that is held.
        let result = applied(
            &project(&[
                an_agent_wire("mira", "product_manager"),
                with_status("theo", "software_developer", "retired"),
                an_agent_wire("linus", "software_developer"),
            ]),
            &a_template_wire(),
            &["linus"],
        );
        assert_eq!(result.added, ids(&["theo-2"]));
        assert_eq!(
            agent(&result, "theo-2"),
            Some(json!({
                "id": "theo-2",
                "display_name": "Theo",
                "role": "software_developer",
                "status": "active",
                "persona": "A software_developer.",
                "avatar": "software_developer-picture",
                "model": { "id": "software_developer-model", "effort": "medium" }
            }))
        );
        assert_eq!(agent(&result, "theo").expect("theo")["status"], "retired");
        let result = applied(
            &project(&[
                an_agent_wire("mira", "product_manager"),
                with_status("theo", "software_developer", "retired"),
                with_status("theo-2", "software_developer", "retired"),
                an_agent_wire("linus", "software_developer"),
            ]),
            &a_template_wire(),
            &[],
        );
        assert_eq!(result.added, ids(&["theo-3"]));
        assert!(result.errors.is_empty(), "{:?}", result.errors);

        // The same id with another role: the project's theo is retired, and the template's added.
        let architect = project(&[
            an_agent_wire("mira", "product_manager"),
            an_agent_wire("theo", "architect"),
            an_agent_wire("linus", "software_developer"),
        ]);
        let result = applied(&architect, &a_template_wire(), &["theo"]);
        assert_eq!(result.kept, ids(&["mira"]));
        assert_eq!(result.retired, ids(&["theo"]));
        assert_eq!(result.added, ids(&["theo-2"]));
        assert_eq!(agent(&result, "theo").expect("theo")["role"], "architect");
        assert_eq!(
            agent(&result, "theo-2").expect("theo-2")["role"],
            "software_developer"
        );
        // A free id is an id still: at most 64 characters.
        let long = "a".repeat(64);
        let result = applied(
            &project(&[
                an_agent_wire("mira", "product_manager"),
                with_status(&long, "software_developer", "retired"),
                an_agent_wire("theo", "software_developer"),
            ]),
            &template_with(&[
                joining("mira", "Mira", "product_manager"),
                joining("theo", "Theo", "software_developer"),
                joining(&long, "A", "software_developer"),
            ]),
            &[],
        );
        assert_eq!(result.added, [format!("{}-2", "a".repeat(62))]);

        // Removed, it holds no id any more: the template's Theo takes theo.
        let result = applied(&architect, &a_template_wire(), &[]);
        assert_eq!(result.removed, ids(&["theo", "linus"]));
        assert_eq!(result.added, ids(&["theo"]));
        assert_eq!(
            agent(&result, "theo").expect("theo")["role"],
            "software_developer"
        );
    }

    #[test]
    fn leaves_retired_agents_alone() {
        let mut old = with_status("old", "architect", "retired");
        old["persona"] = json!("Was here.");
        let wire = project(&[
            an_agent_wire("mira", "product_manager"),
            an_agent_wire("theo", "software_developer"),
            old.clone(),
        ]);
        let template = template_with(&[
            joining("mira", "Mira", "product_manager"),
            joining("theo", "Theo", "software_developer"),
            joining("old", "Old", "architect"),
        ]);
        for worked in [&[][..], &["old"][..]] {
            let result = applied(&wire, &template, worked);
            assert_eq!(agent(&result, "old"), Some(old.clone()), "{worked:?}");
            assert_eq!(result.kept, ids(&["mira", "theo"]));
            assert!(result.retired.is_empty() && result.removed.is_empty());
            assert_eq!(result.added, ids(&["old-2"]));
        }
    }

    #[test]
    fn takes_the_four_answers() {
        let wire = a_full_team_wire();
        let mut template = template_with(&[
            joining("ada", "Ada", "product_manager"),
            joining("linus", "Linus", "software_developer"),
        ]);
        template["policy"] = json!({
            "permissions": { "run_commands": false, "push": true },
            "judgment": { "required": "never" },
            "integration": "manual"
        });
        template["budgets"] = json!({});
        let result = applied(&wire, &template, &[]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);

        let mut expected = serde_json::to_value(team(&wire)).expect("serialises");
        let read = validate_template(&template).expect("a template");
        expected["policy"]["permissions"] = json!({ "run_commands": false, "push": true });
        expected["policy"]["judgment"] =
            serde_json::to_value(&read.policy.judgment).expect("serialises");
        expected["policy"]["integration"] = json!("manual");
        expected["budgets"]
            .as_object_mut()
            .expect("budgets")
            .remove("daily_usd");
        assert_eq!(
            serde_json::to_value(&result.team).expect("serialises"),
            expected,
            "the four answers are the template's; rules, name and every other key the project's"
        );
    }

    #[test]
    fn carries_the_policy_in_a_template() {
        let mut wire = a_team_wire();
        wire["policy"]["plan_in_sprints"] = json!(true);
        assert_eq!(template_of(&wire)["policy"]["plan_in_sprints"], json!(true));

        let without = a_template_wire();
        assert!(without["policy"].get("plan_in_sprints").is_none());
        validate_template(&without).expect("a template without the key is one");

        let mut project = a_full_team_wire();
        project["policy"]["plan_in_sprints"] = json!(true);
        let mut template = template_with(&[
            joining("ada", "Ada", "product_manager"),
            joining("linus", "Linus", "software_developer"),
        ]);
        assert!(
            applied(&project, &template, &[]).team.plans_in_sprints(),
            "the key absent keeps the project's"
        );
        template["policy"]["plan_in_sprints"] = json!(false);
        assert!(!applied(&project, &template, &[]).team.plans_in_sprints());
    }

    #[test]
    fn never_passes_the_cap() {
        let wire = project(&[
            an_agent_wire("p1", "product_manager"),
            an_agent_wire("d1", "software_developer"),
            an_agent_wire("a1", "architect"),
            an_agent_wire("a2", "scrum_master"),
            an_agent_wire("a3", "marketing_specialist"),
            an_agent_wire("a4", "ui_ux_designer"),
            an_agent_wire("a5", "software_developer"),
        ]);
        let template = template_with(&[
            joining("mira", "Mira", "product_manager"),
            joining("theo", "Theo", "software_developer"),
            joining("ada", "Ada", "architect"),
            joining("sol", "Sol", "scrum_master"),
            joining("kai", "Kai", "marketing_specialist"),
            joining("iris", "Iris", "ui_ux_designer"),
            joining("noor", "Noor", "software_developer"),
        ]);
        let result = applied(&wire, &template, &["p1", "d1", "a1", "a2"]);
        let count = |status: AgentStatus| {
            result
                .team
                .agents
                .iter()
                .filter(|agent| agent.status == status)
                .count()
        };
        assert_eq!(count(AgentStatus::Active), 7);
        assert_eq!(count(AgentStatus::Retired), 4);
        assert_eq!(result.removed, ids(&["a3", "a4", "a5"]));
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }
}
