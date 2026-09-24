//! Farik's roles as data (`docs/SPEC.md` section 6): each shipped role's mandate, what it
//! produces, what it may not do, its default model and effort, its system prompt, and its skills,
//! embedded in the binary and read through one loader. This crate performs no I/O at run time.

use std::fmt;
use std::sync::LazyLock;

use farik_core::contract::Role;
use farik_core::governor::permissions::{PermissionTier, default_tiers};
use farik_core::team::Effort;
use jsonschema::Validator;
use serde::Deserialize;
use serde_json::Value;

use crate::generated::role::{FarikRole, FarikRoleModelEffort};

/// Types generated from `docs/schemas/role.schema.json`.
pub mod generated;
/// Which role reviews a task (D7).
mod reviewer;

pub use reviewer::{REVIEWER_ROLE_FOR, default_reviewer_role};

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/role.schema.json");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded role schema is valid JSON: it is the file in docs/schemas/ that typify \
         generated this crate's types from at compile time",
    );
    jsonschema::validator_for(&schema).expect(
        "the embedded role schema compiles: it is JSON Schema 2020-12 with no external \
         references, and the generator already parsed it",
    )
});

/// One skill a role ships with, in the Agent Skills format: the frontmatter's `name` and
/// `description`, and the text after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The skill's name, equal to its directory's.
    pub name: String,
    /// When the skill applies, as its frontmatter says.
    pub description: String,
    /// The procedure: everything after the frontmatter's closing line.
    pub body: String,
}

/// A role as Farik ships it (`docs/SPEC.md` section 6).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleDefinition {
    /// The role.
    pub id: Role,
    /// What the role is for.
    pub mandate: String,
    /// What the role's work leaves behind.
    pub produces: Vec<String>,
    /// What the role may not do.
    pub forbidden: Vec<String>,
    /// The role's permission tiers: the governor's defaults, which are the only copy of them.
    pub default_tiers: Vec<PermissionTier>,
    /// The model the role runs on unless the agent names its own in the team file.
    pub model: String,
    /// How hard the model thinks unless the agent says otherwise.
    pub effort: Effort,
    /// The role's system prompt, `system.md`.
    pub system_prompt: String,
    /// The role's skills, in the order `role.yaml` names them.
    pub skills: Vec<Skill>,
}

/// Why a role could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleError {
    /// Farik ships no definition of this role.
    NotFound {
        /// The role asked for, as its wire id.
        role_id: String,
    },
    /// The role's files do not describe a role.
    Invalid {
        /// The role, as its wire id.
        role_id: String,
        /// What is wrong, in words a person editing the file can act on.
        detail: String,
    },
}

impl fmt::Display for RoleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { role_id } => {
                write!(formatter, "Farik ships no definition of the role {role_id}")
            }
            Self::Invalid { role_id, detail } => {
                write!(formatter, "the role {role_id} is not valid: {detail}")
            }
        }
    }
}

impl std::error::Error for RoleError {}

/// The definition Farik ships for a role.
///
/// # Errors
///
/// `NotFound` for `Human`, which no agent is: every agent role ships. `Invalid` when a shipped
/// file breaks its schema, which a test over every shipped role rules out.
pub fn load_role(role: Role) -> Result<RoleDefinition, RoleError> {
    match role {
        Role::ProductManager => parse_role(
            role,
            include_str!("../roles/product_manager/role.yaml"),
            include_str!("../roles/product_manager/system.md"),
            &[(
                "writing-task-contracts",
                include_str!("../roles/product_manager/skills/writing-task-contracts/SKILL.md"),
            )],
        ),
        Role::SoftwareDeveloper => parse_role(
            role,
            include_str!("../roles/software_developer/role.yaml"),
            include_str!("../roles/software_developer/system.md"),
            &[(
                "implementing-a-contract",
                include_str!("../roles/software_developer/skills/implementing-a-contract/SKILL.md"),
            )],
        ),
        Role::ScrumMaster => parse_role(
            role,
            include_str!("../roles/scrum_master/role.yaml"),
            include_str!("../roles/scrum_master/system.md"),
            &[(
                "keeping-work-flowing",
                include_str!("../roles/scrum_master/skills/keeping-work-flowing/SKILL.md"),
            )],
        ),
        Role::Architect => parse_role(
            role,
            include_str!("../roles/architect/role.yaml"),
            include_str!("../roles/architect/system.md"),
            &[(
                "reviewing-for-design",
                include_str!("../roles/architect/skills/reviewing-for-design/SKILL.md"),
            )],
        ),
        Role::MarketingSpecialist => parse_role(
            role,
            include_str!("../roles/marketing_specialist/role.yaml"),
            include_str!("../roles/marketing_specialist/system.md"),
            &[(
                "marketing-what-ships",
                include_str!("../roles/marketing_specialist/skills/marketing-what-ships/SKILL.md"),
            )],
        ),
        Role::Human => Err(RoleError::NotFound {
            role_id: role.to_string(),
        }),
    }
}

/// Turns a role's files into its definition: `role.yaml` held to its schema, then each skill it
/// names found among `skills` (name, `SKILL.md` text) and its frontmatter read.
fn parse_role(
    role: Role,
    yaml: &str,
    system: &str,
    skills: &[(&str, &str)],
) -> Result<RoleDefinition, RoleError> {
    let invalid = |detail: String| RoleError::Invalid {
        role_id: role.to_string(),
        detail,
    };
    let value: Value = yaml_value(yaml, "role.yaml").map_err(invalid)?;
    let errors: Vec<String> = VALIDATOR
        .iter_errors(&value)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    if !errors.is_empty() {
        return Err(invalid(format!(
            "role.yaml breaks docs/schemas/role.schema.json: {}",
            errors.join("; ")
        )));
    }
    let file: FarikRole = serde_json::from_value(value).map_err(|error| {
        invalid(format!(
            "the schema passed but the typed role could not be built: {error}"
        ))
    })?;
    if file.id.to_string() != role.to_string() {
        return Err(invalid(format!(
            "role.yaml says it is {}, and it is the file of {role}",
            file.id
        )));
    }
    let skills = file
        .skills
        .iter()
        .map(|name| {
            let name = name.to_string();
            let text = skills
                .iter()
                .find(|(shipped, _)| *shipped == name)
                .map(|(_, text)| *text)
                .ok_or_else(|| {
                    invalid(format!(
                        "role.yaml names the skill {name}, which has no SKILL.md"
                    ))
                })?;
            parse_skill(&name, text).map_err(invalid)
        })
        .collect::<Result<Vec<Skill>, RoleError>>()?;
    Ok(RoleDefinition {
        id: role,
        mandate: file.mandate.to_string(),
        produces: file.produces.into_iter().map(String::from).collect(),
        forbidden: file.forbidden.into_iter().map(String::from).collect(),
        default_tiers: default_tiers(role).to_vec(),
        model: file.model.id.to_string(),
        effort: match file.model.effort {
            FarikRoleModelEffort::Low => Effort::Low,
            FarikRoleModelEffort::Medium => Effort::Medium,
            FarikRoleModelEffort::High => Effort::High,
        },
        system_prompt: system.to_string(),
        skills,
    })
}

/// A skill's frontmatter, in the Agent Skills format.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frontmatter {
    name: String,
    description: String,
}

/// Reads a `SKILL.md`: a `---` line, YAML up to the next `---` line, and the body after it.
fn parse_skill(name: &str, text: &str) -> Result<Skill, String> {
    let file = format!("skills/{name}/SKILL.md");
    let rest = text
        .strip_prefix("---\n")
        .ok_or_else(|| format!("{file} does not open with a --- line and its frontmatter"))?;
    let (front, body) = rest
        .split_once("\n---\n")
        .ok_or_else(|| format!("{file} has no --- line closing its frontmatter"))?;
    let front: Frontmatter = serde_saphyr::from_str_with_options(front, yaml_options())
        .map_err(|error| format!("{file} has a frontmatter that is not a skill's: {error}"))?;
    if front.name != name {
        return Err(format!(
            "{file} calls itself {}, and a skill's name is its directory's",
            front.name
        ));
    }
    Ok(Skill {
        name: front.name,
        description: front.description,
        body: body.to_string(),
    })
}

/// The YAML dialect every file Farik reads is held to (ADR 0007): `true` spelled `true`. A copy of
/// `farik_store::files`' options, since this crate cannot depend on the store; keep the two equal.
fn yaml_options() -> serde_saphyr::Options {
    let mut options = serde_saphyr::Options::default();
    options.strict_booleans = true;
    options
}

fn yaml_value(text: &str, named: &str) -> Result<Value, String> {
    serde_saphyr::from_str_with_options(text, yaml_options()).map_err(|error| {
        error
            .render_with_formatter(&serde_saphyr::UserMessageFormatter)
            .replace("<input>", named)
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use farik_core::contract::Role;
    use farik_core::governor::permissions::default_tiers;
    use farik_core::team::Effort;
    use serde_json::Value;

    use super::{RoleDefinition, RoleError, SCHEMA_JSON, load_role, parse_role, yaml_options};

    const PM_YAML: &str = include_str!("../roles/product_manager/role.yaml");
    const PM_SYSTEM: &str = include_str!("../roles/product_manager/system.md");
    const PM_SKILL: &str =
        include_str!("../roles/product_manager/skills/writing-task-contracts/SKILL.md");
    const SD_YAML: &str = include_str!("../roles/software_developer/role.yaml");
    const SD_SKILL: &str =
        include_str!("../roles/software_developer/skills/implementing-a-contract/SKILL.md");

    fn loaded(role: Role) -> RoleDefinition {
        load_role(role).expect("a shipped role loads")
    }

    fn invalid_detail(result: Result<RoleDefinition, RoleError>) -> String {
        match result {
            Err(RoleError::Invalid { role_id, detail }) => {
                assert_eq!(role_id, "product_manager");
                detail
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    fn assert_loads(role: Role, skill: &str) -> RoleDefinition {
        let definition = loaded(role);
        assert_eq!(definition.id, role);
        assert!(!definition.mandate.trim().is_empty());
        assert!(!definition.produces.is_empty());
        assert!(!definition.forbidden.is_empty());
        assert_eq!(definition.model, "claude-opus-5");
        assert_eq!(definition.effort, Effort::High);
        assert_eq!(definition.default_tiers, default_tiers(role));
        assert_eq!(definition.skills.len(), 1);
        assert_eq!(definition.skills[0].name, skill);
        assert!(!definition.skills[0].description.trim().is_empty());
        assert!(!definition.skills[0].body.trim().is_empty());
        assert!(
            !definition.skills[0]
                .body
                .contains(&format!("name: {skill}")),
            "the frontmatter is not part of the body"
        );
        definition
    }

    #[test]
    fn loads_the_product_manager() {
        let definition = assert_loads(Role::ProductManager, "writing-task-contracts");
        assert!(definition.system_prompt.contains("untrusted"));
    }

    #[test]
    fn loads_the_software_developer() {
        let definition = assert_loads(Role::SoftwareDeveloper, "implementing-a-contract");
        assert!(definition.system_prompt.contains("farik_exec"));
        assert!(definition.system_prompt.contains("untrusted"));
    }

    #[test]
    fn loads_the_scrum_master() {
        let definition = loaded(Role::ScrumMaster);
        assert_eq!(definition.id, Role::ScrumMaster);
        assert_eq!(definition.model, "claude-sonnet-5");
        assert_eq!(definition.effort, Effort::Medium);
        assert_eq!(definition.default_tiers, default_tiers(Role::ScrumMaster));
        assert_eq!(definition.skills.len(), 1);
        assert_eq!(definition.skills[0].name, "keeping-work-flowing");
        assert!(!definition.skills[0].description.trim().is_empty());
        assert!(!definition.skills[0].body.trim().is_empty());
        assert!(definition.system_prompt.contains("untrusted"));
        assert!(definition.system_prompt.contains("farik_triage_request"));
    }

    #[test]
    fn ends_the_scrum_masters_one_tool_sessions_on_their_one_tool() {
        let prompt = loaded(Role::ScrumMaster).system_prompt;
        let ending = &prompt[prompt.find("## How a session ends").expect("the section")..];
        assert!(
            ending.contains("recorded with `farik_triage_request`: end your turn"),
            "{ending}"
        );
        assert!(
            ending.contains("recorded with `farik_record_judgment`: end your turn"),
            "{ending}"
        );
        assert!(
            !ending.contains("a triage or a breakdown you finished"),
            "a triage requests no transition: {ending}"
        );
    }

    #[test]
    fn loads_the_architect() {
        let definition = loaded(Role::Architect);
        assert_eq!(definition.id, Role::Architect);
        assert_eq!(definition.model, "claude-opus-5");
        assert_eq!(definition.effort, Effort::High);
        assert_eq!(definition.default_tiers, default_tiers(Role::Architect));
        assert_eq!(definition.skills.len(), 1);
        assert_eq!(definition.skills[0].name, "reviewing-for-design");
        assert!(!definition.skills[0].description.trim().is_empty());
        assert!(!definition.skills[0].body.trim().is_empty());
        assert!(definition.system_prompt.contains("untrusted"));
        assert!(definition.system_prompt.contains("farik_write_note"));
    }

    #[test]
    fn loads_the_marketing_specialist() {
        let definition = loaded(Role::MarketingSpecialist);
        assert_eq!(definition.id, Role::MarketingSpecialist);
        assert_eq!(definition.model, "claude-sonnet-5");
        assert_eq!(definition.effort, Effort::Medium);
        assert_eq!(
            definition.default_tiers,
            default_tiers(Role::MarketingSpecialist)
        );
        assert_eq!(definition.skills.len(), 1);
        assert_eq!(definition.skills[0].name, "marketing-what-ships");
        assert!(!definition.skills[0].description.trim().is_empty());
        assert!(!definition.skills[0].body.trim().is_empty());
        assert!(definition.system_prompt.contains("untrusted"));
    }

    #[test]
    fn forbids_application_code_to_every_role_but_the_developer() {
        for role in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::MarketingSpecialist,
        ] {
            let definition = loaded(role);
            assert!(
                definition
                    .forbidden
                    .iter()
                    .any(|item| item == "write application code"),
                "{role}: {:?}",
                definition.forbidden
            );
        }
    }

    /// Reads the role directories as they are on disk rather than as `load_role` embeds them, so
    /// that a role directory nobody wired into the loader, or a skill directory a role file names
    /// and nobody wrote, is found here.
    #[test]
    fn holds_every_shipped_role_to_its_schema() {
        let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("the schema is JSON");
        let validator = jsonschema::validator_for(&schema).expect("the schema compiles");
        let roles = Path::new(env!("CARGO_MANIFEST_DIR")).join("roles");
        let mut directories: Vec<String> = std::fs::read_dir(&roles)
            .expect("the roles directory")
            .map(|entry| {
                entry
                    .expect("a directory entry")
                    .file_name()
                    .into_string()
                    .expect("a UTF-8 name")
            })
            .collect();
        directories.sort();
        assert_eq!(
            directories,
            [
                "architect",
                "marketing_specialist",
                "product_manager",
                "scrum_master",
                "software_developer",
            ]
        );
        for directory in &directories {
            let yaml = std::fs::read_to_string(roles.join(directory).join("role.yaml"))
                .expect("a role.yaml");
            let value: Value = serde_saphyr::from_str_with_options(&yaml, yaml_options())
                .expect("the role file is YAML");
            let errors: Vec<String> = validator
                .iter_errors(&value)
                .map(|error| error.to_string())
                .collect();
            assert!(errors.is_empty(), "{directory}: {errors:?}");
            assert_eq!(value["id"], Value::String(directory.clone()));
            let role: Role = serde_json::from_value(value["id"].clone()).expect("the id is a role");
            assert!(
                load_role(role).is_ok(),
                "{directory} is wired into load_role"
            );
            for skill in value["skills"].as_array().expect("a skill list") {
                let skill = skill.as_str().expect("a skill name");
                let text = std::fs::read_to_string(
                    roles
                        .join(directory)
                        .join("skills")
                        .join(skill)
                        .join("SKILL.md"),
                )
                .unwrap_or_else(|error| panic!("{directory} names {skill}: {error}"));
                let front = text
                    .strip_prefix("---\n")
                    .and_then(|rest| rest.split_once("\n---\n"))
                    .map(|(front, _)| front)
                    .expect("a frontmatter");
                let front: Value = serde_saphyr::from_str_with_options(front, yaml_options())
                    .expect("YAML frontmatter");
                assert_eq!(front["name"], Value::String(skill.to_string()));
            }
        }
    }

    #[test]
    fn refuses_a_role_that_is_not_shipped() {
        assert_eq!(
            load_role(Role::Human),
            Err(RoleError::NotFound {
                role_id: "human".to_string()
            })
        );
    }

    fn pm_with_skill(skill: &str) -> Result<RoleDefinition, RoleError> {
        parse_role(
            Role::ProductManager,
            PM_YAML,
            PM_SYSTEM,
            &[("writing-task-contracts", skill)],
        )
    }

    #[test]
    fn refuses_a_role_file_that_breaks_the_schema() {
        let yaml = format!("{PM_YAML}\nmascot: a penguin\n");
        let detail = invalid_detail(parse_role(
            Role::ProductManager,
            &yaml,
            PM_SYSTEM,
            &[("writing-task-contracts", PM_SKILL)],
        ));
        assert!(detail.contains("mascot"), "{detail}");
        assert!(detail.contains("role.schema.json"), "{detail}");
    }

    #[test]
    fn refuses_a_role_file_that_is_another_roles() {
        let detail = invalid_detail(parse_role(
            Role::ProductManager,
            SD_YAML,
            PM_SYSTEM,
            &[("implementing-a-contract", SD_SKILL)],
        ));
        assert!(
            detail.contains("role.yaml says it is software_developer"),
            "{detail}"
        );
    }

    #[test]
    fn refuses_a_named_skill_with_no_skill_file() {
        let detail = invalid_detail(parse_role(Role::ProductManager, PM_YAML, PM_SYSTEM, &[]));
        assert!(
            detail.contains("names the skill writing-task-contracts, which has no SKILL.md"),
            "{detail}"
        );
    }

    #[test]
    fn refuses_a_skill_without_frontmatter() {
        let detail = invalid_detail(pm_with_skill(
            "# Writing task contracts\n---\nNo opening line.\n",
        ));
        assert!(
            detail.contains("skills/writing-task-contracts/SKILL.md does not open with a --- line"),
            "{detail}"
        );
    }

    #[test]
    fn refuses_a_frontmatter_that_is_never_closed() {
        let detail = invalid_detail(pm_with_skill(
            "---\nname: writing-task-contracts\ndescription: Contracts.\n",
        ));
        assert!(detail.contains("no --- line closing"), "{detail}");
    }

    #[test]
    fn refuses_a_frontmatter_key_the_format_does_not_have() {
        let detail = invalid_detail(pm_with_skill(
            "---\nname: writing-task-contracts\ndescription: Contracts.\nlicense: MIT\n---\nBody.\n",
        ));
        assert!(detail.contains("not a skill's"), "{detail}");
        assert!(detail.contains("license"), "{detail}");
    }

    #[test]
    fn refuses_a_skill_named_other_than_its_directory() {
        let detail = invalid_detail(pm_with_skill(
            "---\nname: writing-contracts\ndescription: Contracts.\n---\nBody.\n",
        ));
        assert!(
            detail
                .contains("calls itself writing-contracts, and a skill's name is its directory's"),
            "{detail}"
        );
    }

    /// ADR 0007: YAML 1.1's `no` is a word, not `false`, here as in `farik-store`. The role file
    /// is read into a `Value`, where the dialect decides; the frontmatter's typed fields would take
    /// `no` as a string under either setting.
    #[test]
    fn reads_the_role_file_with_strict_booleans() {
        let yaml = PM_YAML.replace("  - release scope\n", "  - release scope\n  - no\n");
        let definition = parse_role(
            Role::ProductManager,
            &yaml,
            PM_SYSTEM,
            &[("writing-task-contracts", PM_SKILL)],
        )
        .expect("the role loads");
        assert_eq!(definition.produces.last().map(String::as_str), Some("no"));
    }

    #[test]
    fn lists_the_agent_roles_in_the_schema() {
        let contract: Value = serde_json::from_str(include_str!(
            "../../../docs/schemas/task-contract.schema.json"
        ))
        .expect("the contract schema is JSON");
        let agent_roles: Vec<Value> = contract["$defs"]["role"]["enum"]
            .as_array()
            .expect("the contract's roles")
            .iter()
            .filter(|value| {
                serde_json::from_value::<Role>((*value).clone()).expect("a Role") != Role::Human
            })
            .cloned()
            .collect();
        let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("the schema is JSON");
        assert_eq!(
            schema["properties"]["id"]["enum"],
            Value::Array(agent_roles)
        );
    }
}
