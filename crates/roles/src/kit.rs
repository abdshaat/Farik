//! A role's kit (`docs/SPEC.md` 6.7, ADR 0036): the skills it carries and the services it may be
//! connected to, read from `roles/<role>/kit.yaml` and held to `docs/schemas/kit.schema.json`
//! and the refusals the loader adds. Pure: the files are embedded in the binary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;
use std::sync::LazyLock;

use farik_core::contract::Role;
use farik_core::governor::permissions::ConnectorTag;
use farik_core::team::{BUILTIN_CONNECTORS, McpServerWire, server_errors};
use jsonschema::Validator;
use serde_json::Value;

use crate::connectors::ConnectorDefinition;
use crate::generated::kit::FarikKit;
use crate::skill_check::{CheckedSkill, check_skill};
use crate::{RoleDefinition, load_role};

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/kit.schema.json");

/// A role's kit, as Farik ships it.
#[derive(Debug, Clone, PartialEq)]
pub struct Kit {
    /// The role it belongs to.
    pub role: Role,
    /// The kit's skills, checked as a user's are (ADR 0034).
    pub skills: Vec<CheckedSkill>,
    /// The services, in the file's order.
    pub connectors: Vec<KitConnector>,
}

/// One service of a kit.
#[derive(Debug, Clone, PartialEq)]
// reason: a kit holds a dozen connectors at most, and the variants are the plan's interface.
#[allow(clippy::large_enum_variant)]
pub enum KitConnector {
    /// A `stdio` or `http` service the user connects by name.
    Server {
        /// The entry as the team file takes it, `source: custom`; `kit_entry` marks it a kit's.
        entry: McpServerWire,
        /// What the user reads while connecting.
        copy: SetupCopy,
        /// The calls per sprint a user may pre-approve, by tool (step 05b).
        allowances: BTreeMap<String, KitAllowance>,
    },
    /// A server Farik runs in Docker itself.
    Container(ConnectorDefinition),
}

impl KitConnector {
    /// The connector's name.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Server { entry, .. } => entry.name.as_str(),
            Self::Container(definition) => &definition.name,
        }
    }
}

/// The words a user reads while connecting a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCopy {
    /// The service.
    pub title: String,
    /// What it is.
    pub about: String,
    /// Why the role wants it.
    pub why: String,
    /// What the user does.
    pub setup: String,
    /// Where the key is made; present when the service takes keys.
    pub key_page: Option<String>,
    /// A tool to what it does in the user's words.
    pub labels: BTreeMap<String, String>,
}

/// What a user may pre-approve for a spending tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitAllowance {
    /// The default calls per sprint.
    pub calls: u32,
    /// A plural noun, such as "images".
    pub what: String,
}

/// Why a kit could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitError {
    /// Farik ships no kit for this role.
    NotFound {
        /// The role asked for.
        role_id: String,
    },
    /// The kit's files are not a kit's.
    Invalid {
        /// The role, as its wire id.
        role_id: String,
        /// Each refusal as `<json pointer>: <code>: <message>`, joined by `; `.
        detail: String,
    },
}

impl fmt::Display for KitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { role_id } => {
                write!(formatter, "Farik ships no kit for the role {role_id}")
            }
            Self::Invalid { role_id, detail } => {
                write!(formatter, "the kit of {role_id} is not valid: {detail}")
            }
        }
    }
}

impl std::error::Error for KitError {}

/// The kit Farik ships for a role.
///
/// # Errors
///
/// `NotFound` for `Human`; `Invalid` when a shipped file is refused, which the tests over every
/// shipped kit rule out.
pub fn load_kit(role: Role) -> Result<Kit, KitError> {
    let yaml = match role {
        Role::ProductManager => include_str!("../roles/product_manager/kit.yaml"),
        Role::SoftwareDeveloper => include_str!("../roles/software_developer/kit.yaml"),
        Role::ScrumMaster => include_str!("../roles/scrum_master/kit.yaml"),
        Role::Architect => include_str!("../roles/architect/kit.yaml"),
        Role::MarketingSpecialist => include_str!("../roles/marketing_specialist/kit.yaml"),
        Role::UiUxDesigner => include_str!("../roles/ui_ux_designer/kit.yaml"),
        Role::FinanceSpecialist => include_str!("../roles/finance_specialist/kit.yaml"),
        Role::Human => {
            return Err(KitError::NotFound {
                role_id: role.to_string(),
            });
        }
    };
    // The role's own skill names come from `load_role`, never from `core_skill_names`, which is
    // built over this function.
    let role_skills: Vec<String> = load_role(role)
        .map_err(|error| KitError::Invalid {
            role_id: role.to_string(),
            detail: error.to_string(),
        })?
        .skills
        .into_iter()
        .map(|skill| skill.name)
        .collect();
    parse_kit(role, yaml, &role_skills, &embedded_skills(role))
}

/// A kit's skills as `(name, files)`, each file embedded in the binary.
type EmbeddedSkills = Vec<(&'static str, &'static [(&'static str, &'static str)])>;

/// The bare command with which a kit names Farik's own program (ADR 0038).
pub const FARIK_COMMAND: &str = "farik";

/// The names of Farik's own connectors, each started as `farik connector <name>`.
pub const FARIK_CONNECTORS: &[&str] = &["osv"];

/// Whether `command` and `args` are, exactly, `farik connector <name>` for one of Farik's own
/// connectors. Nothing else, a user's own `farik` command included, is Farik's.
#[must_use]
pub fn is_farik_connector(command: &str, args: &[String]) -> bool {
    command == FARIK_COMMAND
        && matches!(args, [connector, name]
            if connector == "connector" && FARIK_CONNECTORS.contains(&name.as_str()))
}

/// One role's embedded skills: each folder's `SKILL.md`, in the order the kit names them.
macro_rules! embedded {
    ($folder:literal: $($name:literal),+ $(,)?) => {
        vec![$((
            $name,
            &[(
                "SKILL.md",
                include_str!(concat!("../roles/", $folder, "/skills/", $name, "/SKILL.md")),
            )] as &'static [(&'static str, &'static str)],
        )),+]
    };
}

/// The skill folders a role's kit ships, in the order its `kit.yaml` names them.
fn embedded_skills(role: Role) -> EmbeddedSkills {
    match role {
        Role::ProductManager => embedded!("product_manager":
            "asking-the-right-questions",
            "writing-requirements",
            "prioritising-the-backlog",
            "scoping-a-release",
            "using-product-sources",
        ),
        Role::ScrumMaster => embedded!("scrum_master":
            "planning-a-sprint",
            "running-ceremonies",
            "writing-escalation-digests",
        ),
        Role::Architect => embedded!("architect":
            "designing-apis-and-data",
            "reviewing-dependencies",
            "security-review",
            "setting-performance-budgets",
            "using-architecture-sources",
        ),
        Role::SoftwareDeveloper => embedded!("software_developer":
            "test-driven-development",
            "debugging",
            "safe-migrations",
            "testing-per-stack",
            "answering-a-review",
            "using-docs-and-the-browser",
        ),
        Role::MarketingSpecialist => embedded!("marketing_specialist":
            "positioning-and-messaging",
            "planning-a-launch",
            "writing-for-search",
            "writing-in-the-brands-voice",
            "keeping-a-content-calendar",
            "researching-competitors",
            "measuring-campaigns",
            "making-images-and-video",
            "posting-and-email",
            "keeping-the-brand-kit",
            "writing-the-brand-persona",
            "researching-the-market",
            "writing-the-marketing-plan",
            "running-social-channels",
        ),
        _ => Vec::new(),
    }
}

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded kit schema is valid JSON: it is the file in docs/schemas/ that typify \
         generated this crate's kit types from at compile time",
    );
    jsonschema::validator_for(&schema).expect(
        "the embedded kit schema compiles: it is JSON Schema 2020-12 with no external \
         references, and the generator already parsed it",
    )
});

/// What a kit file may not say in a service's copy: the plumbing's names (ADR 0036).
const PLUMBING: [&str; 3] = ["mcp", "oauth", "token"];

/// What one connector may and must say, by transport.
struct Shape {
    allowed: &'static [&'static str],
    required: &'static [&'static str],
}

const COPY: [&str; 6] = ["title", "about", "why", "setup", "key_page", "labels"];

fn shape(transport: &str) -> Shape {
    match transport {
        "stdio" => Shape {
            allowed: &[
                "name",
                "transport",
                "command",
                "args",
                "credential_keys",
                "tools",
                "allowances",
                "title",
                "about",
                "why",
                "setup",
                "key_page",
                "labels",
            ],
            required: &["command", "title", "about", "why", "setup"],
        },
        "http" => Shape {
            allowed: &[
                "name",
                "transport",
                "url",
                "headers",
                "credential_keys",
                "oauth",
                "tools",
                "allowances",
                "title",
                "about",
                "why",
                "setup",
                "key_page",
                "labels",
            ],
            required: &["url", "title", "about", "why", "setup"],
        },
        _ => Shape {
            allowed: &["name", "transport", "image", "module_root", "args", "tools"],
            required: &["image", "module_root", "args"],
        },
    }
}

/// Turns a kit's files into the kit: `kit.yaml` held to its schema and the loader's refusals.
/// `role_skills` are the names `role.yaml` carries; `skills` are the kit's skill folders, each as
/// its name and its files (path, text).
///
/// # Errors
///
/// `Invalid`, naming every refusal as `<json pointer>: <code>: <words>`.
pub fn parse_kit(
    role: Role,
    yaml: &str,
    role_skills: &[String],
    skills: &[(&str, &[(&str, &str)])],
) -> Result<Kit, KitError> {
    parse(role, yaml, role_skills, skills, false)
}

/// [`parse_kit`] for a test's fixture kit, which starts its stand-in server with `sh`: every
/// check but the one on a `stdio` command. Nothing Farik ships goes through it.
///
/// # Errors
///
/// As [`parse_kit`].
#[doc(hidden)]
pub fn parse_fixture_kit(
    role: Role,
    yaml: &str,
    role_skills: &[String],
    skills: &[(&str, &[(&str, &str)])],
) -> Result<Kit, KitError> {
    parse(role, yaml, role_skills, skills, true)
}

fn parse(
    role: Role,
    yaml: &str,
    role_skills: &[String],
    skills: &[(&str, &[(&str, &str)])],
    any_command: bool,
) -> Result<Kit, KitError> {
    let invalid = |detail: String| KitError::Invalid {
        role_id: role.to_string(),
        detail,
    };
    let value = crate::yaml_value(yaml, "kit.yaml").map_err(invalid)?;
    let schema: Vec<String> = VALIDATOR
        .iter_errors(&value)
        .map(|error| format!("{}: schema: {error}", error.instance_path()))
        .collect();
    if !schema.is_empty() {
        return Err(invalid(schema.join("; ")));
    }
    let file: FarikKit = serde_json::from_value(value.clone()).map_err(|error| {
        invalid(format!(
            "the schema passed but the typed kit could not be built: {error}"
        ))
    })?;
    let mut refused = Refusals(Vec::new(), any_command);
    if file.role.to_string() != role.to_string() {
        refused.add(
            "/role".to_string(),
            "kit_role_mismatch",
            &format!(
                "kit.yaml says it is {}, and it is the kit of {role}",
                file.role
            ),
        );
    }
    let names: Vec<String> = file
        .skills
        .iter()
        .map(|skill| skill.as_str().to_string())
        .collect();
    let kit_skills = load_skills(&names, role_skills, skills, &mut refused);
    let mut connectors = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (index, connector) in value["connectors"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let name = connector["name"].as_str().unwrap_or_default().to_string();
        if seen.contains(&name) {
            refused.add(
                format!("/connectors/{index}/name"),
                "kit_connector_twice",
                &format!("{name} is in this kit already"),
            );
        }
        seen.push(name);
        connectors.extend(load_connector(role, index, connector, &mut refused));
    }
    if refused.0.is_empty() {
        Ok(Kit {
            role,
            skills: kit_skills,
            connectors,
        })
    } else {
        Err(invalid(refused.0.join("; ")))
    }
}

/// The refusals of one kit file, each `<json pointer>: <code>: <words>`.
struct Refusals(Vec<String>, bool);

impl Refusals {
    fn add(&mut self, pointer: impl Into<String>, code: &str, words: &str) {
        self.0.push(format!("{}: {code}: {words}", pointer.into()));
    }

    /// Refuses `checked` when it names the plumbing, whatever the field's own text is.
    fn check_words(&mut self, pointer: &str, checked: &str) {
        let lower = checked.to_lowercase();
        if let Some(word) = PLUMBING.iter().find(|word| lower.contains(**word)) {
            self.add(
                pointer.to_string(),
                "copy_word_refused",
                &format!(
                    "Farik's own words never say \"{word}\"; only a service's label quoted in \
                     setup may"
                ),
            );
        }
    }
}

/// The kit's skills: each named, not the role's, with its folder, and passing `check_skill`.
fn load_skills(
    names: &[String],
    role_skills: &[String],
    skills: &[(&str, &[(&str, &str)])],
    refused: &mut Refusals,
) -> Vec<CheckedSkill> {
    let mut checked = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let pointer = format!("/skills/{index}");
        if role_skills.contains(name) {
            refused.add(
                pointer,
                "kit_skill_in_role",
                &format!("role.yaml carries {name} already, in the prompt; a kit skill is another"),
            );
            continue;
        }
        let Some((_, files)) = skills.iter().find(|(found, _)| found == name) else {
            refused.add(
                pointer,
                "kit_skill_missing",
                &format!("there is no skills/{name}/SKILL.md beside kit.yaml"),
            );
            continue;
        };
        let files = files
            .iter()
            .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
            .collect();
        match check_skill(name, &files) {
            Ok(skill) => checked.push(skill),
            Err(refusal) => refused.add(pointer, refusal.code(), &refusal.to_string()),
        }
    }
    checked
}

/// One connector of the file: its shape by transport, then the checks of its kind. `None` when
/// it is refused.
fn load_connector(
    role: Role,
    index: usize,
    connector: &Value,
    refused: &mut Refusals,
) -> Option<KitConnector> {
    let at = |field: &str| format!("/connectors/{index}/{field}");
    let transport = connector["transport"].as_str().unwrap_or_default();
    let shape = shape(transport);
    let object = connector.as_object().cloned().unwrap_or_default();
    let mut shaped = true;
    for field in object
        .keys()
        .filter(|field| !shape.allowed.contains(&field.as_str()))
    {
        shaped = false;
        refused.add(
            at(field),
            "kit_field_not_allowed",
            &format!("a {transport} connector has no {field}"),
        );
    }
    for field in shape
        .required
        .iter()
        .filter(|field| !object.contains_key(**field))
    {
        shaped = false;
        refused.add(
            at(field),
            "kit_field_missing",
            &format!("a {transport} connector needs its {field}"),
        );
    }
    if !shaped {
        return None;
    }
    if transport == "container" {
        return Some(load_container(role, index, connector, refused));
    }
    load_server(index, connector, transport, refused)
}

/// A `container` connector: the built-in browser of the Designer's kit, its image pinned.
fn load_container(
    role: Role,
    index: usize,
    connector: &Value,
    refused: &mut Refusals,
) -> KitConnector {
    let at = |field: &str| format!("/connectors/{index}/{field}");
    let name = connector["name"].as_str().unwrap_or_default().to_string();
    let image = connector["image"].as_str().unwrap_or_default();
    if !BUILTIN_CONNECTORS.contains(&name.as_str()) || role != Role::UiUxDesigner {
        refused.add(
            at("name"),
            "container_not_builtin",
            &format!(
                "{name} is not a server Farik runs in Docker for this role; only the \
                 UI/UX Designer's playwright is"
            ),
        );
    }
    if !image.contains("@sha256:") {
        refused.add(
            at("image"),
            "image_not_pinned",
            "name the image with its @sha256: digest",
        );
    }
    KitConnector::Container(ConnectorDefinition {
        name,
        image: image.to_string(),
        args: strings(&connector["args"]),
        module_root: connector["module_root"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        tools: tool_tags(connector),
    })
}

/// A `stdio` or `http` connector: the team file's refusals, the pin, the key page, the labels, the
/// allowances and the copy's words.
fn load_server(
    index: usize,
    connector: &Value,
    transport: &str,
    refused: &mut Refusals,
) -> Option<KitConnector> {
    let at = |field: &str| format!("/connectors/{index}/{field}");
    let object = connector.as_object().cloned().unwrap_or_default();
    let tools = tool_tags(connector);
    let mut entry = serde_json::Map::new();
    entry.insert("source".to_string(), Value::from("custom"));
    for (field, item) in &object {
        if !COPY.contains(&field.as_str()) && field != "allowances" {
            entry.insert(field.clone(), item.clone());
        }
    }
    let wire: McpServerWire = match serde_json::from_value(Value::Object(entry)) {
        Ok(wire) => wire,
        Err(error) => {
            refused.add(at(""), "connector_invalid", &error.to_string());
            return None;
        }
    };
    for (field, message) in server_errors(&wire) {
        refused.add(at(&field), code_of(&message), &message);
    }
    if transport == "stdio" && !refused.1 {
        check_pinned(
            &connector["command"],
            &strings(&connector["args"]),
            &at,
            refused,
        );
    }
    let takes_keys = connector["credential_keys"]
        .as_array()
        .is_some_and(|keys| !keys.is_empty());
    if takes_keys && !object.contains_key("key_page") {
        refused.add(
            at("key_page"),
            "key_page_missing",
            "a service that takes keys says where the user makes one",
        );
    }
    let labels = string_map(&connector["labels"]);
    for tool in labels.keys().filter(|tool| !tools.contains_key(*tool)) {
        refused.add(
            at(&format!("labels/{tool}")),
            "label_not_a_tool",
            &format!("{tool} is not one of this connector's tools"),
        );
    }
    let mut allowances = BTreeMap::new();
    for (tool, allowance) in connector["allowances"].as_object().into_iter().flatten() {
        if tools.get(tool) != Some(&ConnectorTag::ExternalEffect) {
            refused.add(
                at(&format!("allowances/{tool}")),
                "allowance_not_external",
                &format!("only a tool tagged external_effect has an allowance, and {tool} is not"),
            );
        }
        let what = allowance["what"].as_str().unwrap_or_default().to_string();
        refused.check_words(&at(&format!("allowances/{tool}/what")), &what);
        let calls = allowance["calls"]
            .as_u64()
            .and_then(|calls| u32::try_from(calls).ok());
        allowances.insert(
            tool.clone(),
            KitAllowance {
                calls: calls.unwrap_or_default(),
                what,
            },
        );
    }
    for (tool, label) in &labels {
        refused.check_words(&at(&format!("labels/{tool}")), label);
    }
    let text = |field: &str| connector[field].as_str().unwrap_or_default().to_string();
    let copy = SetupCopy {
        title: text("title"),
        about: text("about"),
        why: text("why"),
        setup: text("setup"),
        key_page: connector["key_page"].as_str().map(str::to_string),
        labels,
    };
    for (field, words) in [
        ("title", &copy.title),
        ("about", &copy.about),
        ("why", &copy.why),
    ] {
        refused.check_words(&at(field), words);
    }
    refused.check_words(&at("setup"), &without_quoted(&copy.setup));
    Some(KitConnector::Server {
        entry: wire,
        copy,
        allowances,
    })
}

/// The code a team-file refusal opens with, before its colon.
fn code_of(message: &str) -> &str {
    message
        .split_once(':')
        .map_or("connector_invalid", |(code, _)| code)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn string_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, item)| Some((key.clone(), item.as_str()?.to_string())))
        .collect()
}

/// A connector's `tools`, which the schema has already held to the three tags.
fn tool_tags(connector: &Value) -> BTreeMap<String, ConnectorTag> {
    connector["tools"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(tool, tag)| {
            let tag = match tag.as_str()? {
                "network" => ConnectorTag::Network,
                "external_effect" => ConnectorTag::ExternalEffect,
                _ => ConnectorTag::Denied,
            };
            Some((tool.clone(), tag))
        })
        .collect()
}

/// A stdio connector runs only a package runner, at an exact version, or a binary Farik ships, so
/// the code Farik runs changes only with a Farik release and its pin review (ADR 0036).
fn check_pinned(
    command: &Value,
    args: &[String],
    at: &dyn Fn(&str) -> String,
    refused: &mut Refusals,
) {
    const SAYS: &str =
        "a kit starts a package only with npx, bunx, uvx or pipx run, at an exact version";
    const NAMING_FLAGS: [&str; 7] = [
        "-p",
        "--package",
        "--from",
        "--spec",
        "--with",
        "--registry",
        "--index-url",
    ];
    let command = command.as_str().unwrap_or_default();
    // Farik's own program, by its bare name and for its own connectors alone.
    if command == FARIK_COMMAND {
        if !is_farik_connector(command, args) {
            refused.add(
                at("args"),
                "package_not_pinned",
                "Farik's own program runs only as `farik connector <name>`, for one of Farik's own connectors",
            );
        }
        return;
    }
    let file = command.rsplit(['/', '\\']).next().unwrap_or(command);
    let program = file
        .strip_suffix(".cmd")
        .or_else(|| file.strip_suffix(".exe"))
        .unwrap_or(file);
    let mut refuse = |field: &str| refused.add(at(field), "package_not_pinned", SAYS);
    if !["npx", "bunx", "uvx", "pipx"].contains(&program) {
        // Farik's own binaries are the one thing a kit may start by absolute path.
        let ships =
            command.starts_with('/') && (program == "farik" || program.starts_with("farik-"));
        if !ships {
            refuse("command");
        }
        return;
    }
    let mut args = args;
    if program == "pipx" {
        match args.split_first() {
            Some((first, rest)) if first == "run" => args = rest,
            _ => return refuse("args"),
        }
    }
    let names_apart = args.iter().any(|arg| {
        NAMING_FLAGS
            .iter()
            .any(|flag| arg == flag || arg.strip_prefix(flag).is_some_and(|r| r.starts_with('=')))
    });
    let package = args.iter().find(|arg| !arg.starts_with('-'));
    if names_apart || !package.is_some_and(|arg| names_exact_version(arg)) {
        refuse("args");
    }
}

/// Whether a package argument is an exact pin: `name@1.2.3` (npm, three numbers and an optional
/// pre-release) or `name==1.2.3` (Python, two or more numbers and an optional suffix).
fn names_exact_version(arg: &str) -> bool {
    let numbers = |version: &str, least: usize| {
        let head: &str = version.split(['-', '+']).next().unwrap_or_default();
        let parts: Vec<&str> = head.split('.').collect();
        parts.len() >= least
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let word = |c: char, extra: &str| c.is_ascii_alphanumeric() || extra.contains(c);
    if let Some((name, version)) = arg.split_once("==") {
        let (base, extras) = match name.split_once('[') {
            Some((base, rest)) => match rest.strip_suffix(']') {
                Some(extras) if !extras.is_empty() => (base, extras),
                _ => return false,
            },
            None => (name, "a"),
        };
        return numbers(version, 2)
            && base
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && base.chars().all(|c| word(c, "._-"))
            && extras.chars().all(|c| word(c, ",._-"));
    }
    let Some(at) = arg.rfind('@').filter(|at| *at > 0) else {
        return false;
    };
    let name = &arg[..at];
    let unscoped = |part: &str| {
        part.chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
    };
    let named = match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, rest)| unscoped(scope) && unscoped(rest)),
        None => unscoped(name),
    };
    named && numbers(&arg[at + 1..], 3)
}

/// The text ranges of a service's own labels quoted in a kit's `setup`: after `‘` up to `’`, after
/// `“` up to `”`, after `"` up to `"`, on one line and at most 60 characters long. The ASCII
/// apostrophe never opens one; an opening mark with no closing one quotes nothing.
#[must_use]
pub fn quoted_labels(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(opener) = text[at..].chars().next() {
        let after = at + opener.len_utf8();
        let closer = match opener {
            '\u{2018}' => Some('\u{2019}'),
            '\u{201C}' => Some('\u{201D}'),
            '"' => Some('"'),
            _ => None,
        };
        if let Some(closer) = closer
            && let Some(end) = closing(&text[after..], closer).map(|end| after + end)
        {
            found.push(after..end);
            at = end + closer.len_utf8();
            continue;
        }
        at = after;
    }
    found
}

/// Where `closer` ends a label that starts `rest`: within 60 characters and before a line break.
fn closing(rest: &str, closer: char) -> Option<usize> {
    let mut inside = 0;
    for (offset, character) in rest.char_indices() {
        if character == closer {
            return Some(offset);
        }
        inside += 1;
        if character == '\n' || character == '\r' || inside > 60 {
            return None;
        }
    }
    None
}

/// `text` with each quoted label's characters taken out, a space left in its place.
fn without_quoted(text: &str) -> String {
    let mut kept = String::new();
    let mut from = 0;
    for range in quoted_labels(text) {
        kept.push_str(&text[from..range.start]);
        kept.push(' ');
        from = range.end;
    }
    kept.push_str(&text[from..]);
    kept
}

/// The name of every skill the roles and kits carry.
#[must_use]
pub fn shipped_skill_names(roles: &[RoleDefinition], kits: &[Kit]) -> BTreeSet<String> {
    let role_skills = roles
        .iter()
        .flat_map(|role| role.skills.iter().map(|skill| skill.name.clone()));
    let kit_skills = kits
        .iter()
        .flat_map(|kit| kit.skills.iter().map(|skill| skill.name.clone()));
    role_skills.chain(kit_skills).collect()
}

/// How a service's tools differ from the kit's pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinDrift {
    /// Tools the service lists and the kit does not tag.
    pub added: Vec<String>,
    /// Tools the kit tags and the service no longer lists.
    pub removed: Vec<String>,
}

/// What the service added to, and dropped from, the tools the kit pinned.
#[must_use]
pub fn pin_drift(pinned: &BTreeMap<String, ConnectorTag>, listed: &[String]) -> PinDrift {
    let listed: BTreeSet<&String> = listed.iter().collect();
    PinDrift {
        added: listed
            .iter()
            .filter(|tool| !pinned.contains_key(**tool))
            .map(|tool| (*tool).clone())
            .collect(),
        removed: pinned
            .keys()
            .filter(|tool| !listed.contains(tool))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use farik_core::contract::Role;
    use farik_core::governor::permissions::ConnectorTag;
    use serde_json::{Value, json};

    use super::SetupCopy;
    use super::{
        Kit, KitAllowance, KitConnector, KitError, load_kit, parse_kit, pin_drift, quoted_labels,
        shipped_skill_names,
    };
    use crate::{builtin_connector, core_skill_names, load_role};
    use farik_core::team::{CustomServer, CustomTransport, custom_server};

    const SHIPPED: [Role; 7] = [
        Role::ProductManager,
        Role::ScrumMaster,
        Role::Architect,
        Role::SoftwareDeveloper,
        Role::MarketingSpecialist,
        Role::UiUxDesigner,
        Role::FinanceSpecialist,
    ];

    /// A valid kit of the Product Manager with one http service.
    fn base() -> Value {
        json!({
            "role": "product_manager",
            "skills": [],
            "connectors": [{
                "name": "notion",
                "transport": "http",
                "url": "https://mcp.notion.example/mcp",
                "credential_keys": ["NOTION_KEY"],
                "headers": { "Authorization": "Bearer {NOTION_KEY}" },
                "title": "Notion",
                "about": "Your team's notes and docs.",
                "why": "Reads your product docs, so plans start from what you wrote.",
                "setup": "Make a key on Notion's page, then paste it.",
                "key_page": "https://www.notion.so/profile/integrations",
                "labels": { "search": "search pages" },
                "tools": { "search": "network", "create_page": "external_effect", "delete_page": "denied" },
                "allowances": { "create_page": { "calls": 20, "what": "pages" } }
            }]
        })
    }

    fn parse_in(role: Role, value: &Value) -> Result<Kit, KitError> {
        parse_kit(role, &value.to_string(), &[], &[])
    }

    fn parse(value: &Value) -> Result<Kit, KitError> {
        parse_in(Role::ProductManager, value)
    }

    fn detail(result: Result<Kit, KitError>) -> String {
        match result {
            Err(KitError::Invalid { detail, .. }) => detail,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    fn refused(value: &Value, pointer: &str, code: &str) {
        let detail = detail(parse(value));
        assert!(
            detail.contains(&format!("{pointer}: {code}")),
            "wanted {code} at {pointer}, got {detail}"
        );
    }

    fn set(value: &mut Value, pointer: &str, to: Value) {
        *value.pointer_mut(pointer).expect("a field of the fixture") = to;
    }

    #[test]
    fn loads_every_shipped_kit() {
        for role in SHIPPED {
            let kit = load_kit(role).expect("a shipped kit loads");
            assert_eq!(kit.role, role);
            assert_eq!(
                kit.connectors.len(),
                match role {
                    Role::UiUxDesigner | Role::SoftwareDeveloper => 1,
                    Role::ProductManager | Role::Architect => 3,
                    Role::MarketingSpecialist => 4,
                    _ => 0,
                },
                "{role}"
            );
        }
        assert!(matches!(
            load_kit(Role::Human),
            Err(KitError::NotFound { .. })
        ));
    }

    #[test]
    fn its_kit_is_empty_until_step_10() {
        let kit = load_kit(Role::FinanceSpecialist).expect("the Finance Specialist's kit");
        assert_eq!(kit.role, Role::FinanceSpecialist);
        assert!(kit.skills.is_empty(), "{:?}", kit.skills);
        assert!(kit.connectors.is_empty(), "{:?}", kit.connectors);
    }

    #[test]
    fn product_manager_kit_carries_its_skills() {
        let kit = load_kit(Role::ProductManager).expect("the Product Manager's kit");
        let names: Vec<&str> = kit.skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "asking-the-right-questions",
                "writing-requirements",
                "prioritising-the-backlog",
                "scoping-a-release",
                "using-product-sources",
            ]
        );
        for skill in &kit.skills {
            assert!(
                skill.session_files.contains_key("SKILL.md"),
                "{}",
                skill.name
            );
        }
    }

    #[test]
    fn scrum_master_kit_carries_its_skills() {
        let kit = load_kit(Role::ScrumMaster).expect("the Scrum Master's kit");
        let names: Vec<&str> = kit.skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "planning-a-sprint",
                "running-ceremonies",
                "writing-escalation-digests",
            ]
        );
        for skill in &kit.skills {
            assert!(
                skill.session_files.contains_key("SKILL.md"),
                "{}",
                skill.name
            );
        }
    }

    #[test]
    fn developer_kit_carries_its_skills() {
        let kit = load_kit(Role::SoftwareDeveloper).expect("the Developer's kit");
        let names: Vec<&str> = kit.skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "test-driven-development",
                "debugging",
                "safe-migrations",
                "testing-per-stack",
                "answering-a-review",
                "using-docs-and-the-browser",
            ]
        );
        for skill in &kit.skills {
            assert!(
                skill.session_files.contains_key("SKILL.md"),
                "{}",
                skill.name
            );
        }
    }

    #[test]
    fn architect_kit_carries_its_skills() {
        let kit = load_kit(Role::Architect).expect("the Architect's kit");
        let names: Vec<&str> = kit.skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "designing-apis-and-data",
                "reviewing-dependencies",
                "security-review",
                "setting-performance-budgets",
                "using-architecture-sources",
            ]
        );
        for skill in &kit.skills {
            assert!(
                skill.session_files.contains_key("SKILL.md"),
                "{}",
                skill.name
            );
        }
    }

    /// The kit's skills are the nine of steps 08 and 08b, then the four of step 08c: the brand kit,
    /// the brand persona, the market and the marketing plan, each written for any business; then
    /// the one of step 08d, running the social channels, which names the tool that posts.
    #[test]
    fn marketing_kit_carries_running_social_channels() {
        let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
        let names: Vec<&str> = kit.skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "positioning-and-messaging",
                "planning-a-launch",
                "writing-for-search",
                "writing-in-the-brands-voice",
                "keeping-a-content-calendar",
                "researching-competitors",
                "measuring-campaigns",
                "making-images-and-video",
                "posting-and-email",
                "keeping-the-brand-kit",
                "writing-the-brand-persona",
                "researching-the-market",
                "writing-the-marketing-plan",
                "running-social-channels",
            ]
        );
        for skill in &kit.skills {
            assert!(
                skill.session_files.contains_key("SKILL.md"),
                "{}",
                skill.name
            );
        }
        // The five new skills: when each applies, numbered sections, under 6 KB, and the skill it
        // sits beside named where the design says it does.
        for (name, description, beside) in [
            (
                "keeping-the-brand-kit",
                "Use when the task touches the business's name, logo, colours, type, pictures or voice",
                "docs/marketing/brand/brand-kit.md",
            ),
            (
                "writing-the-brand-persona",
                "Use when deciding how the brand speaks on social channels",
                "writing-in-the-brands-voice",
            ),
            (
                "researching-the-market",
                "Use before writing a marketing plan, or when the task asks who the customers are, what they search for or what a channel costs",
                "researching-competitors",
            ),
            (
                "writing-the-marketing-plan",
                "Use when the task asks for a marketing plan",
                "researching-the-market",
            ),
            (
                "running-social-channels",
                "Use when the task asks for posts on the business's social channels",
                "`farik_schedule_post`",
            ),
        ] {
            let skill = kit
                .skills
                .iter()
                .find(|skill| skill.name == name)
                .expect("a new skill");
            assert!(skill.description.starts_with(description), "{name}");
            let text = &skill.session_files["SKILL.md"];
            assert!(text.len() < 6 * 1024, "{name} is {} bytes", text.len());
            assert!(text.contains("\n## 1. "), "{name} has numbered sections");
            assert!(text.contains(beside), "{name} does not name {beside}");
        }
    }

    /// `writing-the-marketing-plan` names the tool that proposes the plan, from the commit that
    /// gives the Marketing Specialist the tool; `kit_skills_name_only_tools_farik_lists` (in the
    /// runtime) holds the name to a tool Farik lists.
    #[test]
    fn the_plan_skill_names_the_tool_that_proposes_it() {
        let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
        let skill = kit
            .skills
            .iter()
            .find(|skill| skill.name == "writing-the-marketing-plan")
            .expect("the plan skill");
        let text = &skill.session_files["SKILL.md"];
        assert!(text.contains("`farik_propose_marketing_plan`"), "{text}");
        assert!(
            !text.contains("the tool Farik gives you"),
            "the placeholder is gone"
        );
    }

    /// The brand's logo and pictures are the business's own (ADR 0042, amended 2026-10-06): the
    /// user puts them in the kit's assets folder, the kit names and describes each file, and a
    /// missing one is asked of the user with `farik_ask_human`. The skill never has a picture a
    /// creative service made copied into the kit, and never has a logo made.
    #[test]
    fn the_brand_kit_skill_leaves_the_logo_and_pictures_to_the_user() {
        let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
        let skill = kit
            .skills
            .iter()
            .find(|skill| skill.name == "keeping-the-brand-kit")
            .expect("the brand kit skill");
        let text = &skill.session_files["SKILL.md"];
        let section = text
            .split("\n## ")
            .find(|section| section.starts_with("3. "))
            .expect("the skill's third section");
        for phrase in [
            "the business's own",
            "docs/marketing/brand/assets/",
            "farik_ask_human",
            "never make a logo",
            "generated picture",
        ] {
            assert!(
                section.to_lowercase().contains(&phrase.to_lowercase()),
                "section 3 lacks \"{phrase}\":\n{section}"
            );
        }
        let lowered = text.to_lowercase();
        for gone in [
            "higgsfield",
            "recraft",
            "copied in",
            "the tool farik gives you",
        ] {
            assert!(!lowered.contains(gone), "the skill still says \"{gone}\"");
        }
        assert!(text.len() < 6 * 1024, "{} bytes", text.len());
        assert!(!text.contains(" @"), "no @ after a space");
    }

    /// A role's service's server, its copy and its tags, by name.
    fn service(role: Role, name: &str) -> (CustomServer, SetupCopy) {
        let kit = load_kit(role).expect("a shipped kit");
        for connector in kit.connectors {
            if let KitConnector::Server { entry, copy, .. } = connector {
                let server = custom_server(&entry).expect("a custom server");
                if server.name == name {
                    return (server, copy);
                }
            }
        }
        panic!("the {role} kit has no {name}");
    }

    fn pm_service(name: &str) -> (CustomServer, SetupCopy) {
        service(Role::ProductManager, name)
    }

    fn tagged(server: &CustomServer, tag: ConnectorTag) -> usize {
        server.tools.values().filter(|found| **found == tag).count()
    }

    fn network_names(server: &CustomServer) -> Vec<&str> {
        server
            .tools
            .iter()
            .filter(|(_, tag)| **tag == ConnectorTag::Network)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    fn signed_in(server: &CustomServer) -> (&str, Option<&[String]>) {
        let CustomTransport::Http { url, oauth, .. } = &server.transport else {
            panic!("{} is http", server.name);
        };
        (
            url,
            oauth.as_ref().map(|settings| settings.scopes.as_slice()),
        )
    }

    #[test]
    fn amplitude_reads_usage_and_never_writes() {
        let (server, _) = pm_service("amplitude");
        let (url, scopes) = signed_in(&server);
        assert_eq!(url, "https://mcp.amplitude.com/mcp");
        assert_eq!(
            scopes,
            Some(&["mcp:read".to_string(), "offline_access".to_string()][..])
        );
        assert!(server.credential_keys.is_empty());
        assert_eq!(server.tools["query_amplitude_data"], ConnectorTag::Network);
        for name in [
            "get_amp_user_data",
            "use_amp_dashboards",
            "create_flags",
            "get_deployments",
            "get_from_url",
        ] {
            assert_eq!(server.tools[name], ConnectorTag::Denied, "{name}");
        }
        assert_eq!(
            network_names(&server),
            [
                "get_amp_taxonomy",
                "get_amplitude_charts",
                "get_amplitude_context",
                "get_experiments",
                "get_flags",
                "get_group_types",
                "get_guide_or_survey",
                "get_transformations",
                "list_guides_surveys",
                "query_amplitude_data",
                "query_experiment",
                "query_wave_opportunities",
                "query_wave_product_areas",
                "search",
            ]
        );
        assert_eq!(tagged(&server, ConnectorTag::Network), 14);
        assert_eq!(tagged(&server, ConnectorTag::Denied), 31);
    }

    #[test]
    fn linear_reads_from_its_read_only_address() {
        let (server, _) = pm_service("linear");
        let (url, scopes) = signed_in(&server);
        assert_eq!(url, "https://mcp.linear.app/mcp/readonly");
        assert_eq!(scopes, Some(&["read".to_string()][..]));
        assert_eq!(server.tools.len(), 21);
        assert_eq!(tagged(&server, ConnectorTag::Network), 21);
    }

    #[test]
    fn notion_reads_pages_and_never_changes_them() {
        let (server, _) = pm_service("notion");
        let (url, scopes) = signed_in(&server);
        assert_eq!(url, "https://mcp.notion.com/mcp");
        assert_eq!(scopes, Some(&[][..]));
        assert_eq!(
            network_names(&server),
            [
                "notion-download-attachment",
                "notion-fetch",
                "notion-get-comments",
                "notion-get-teams",
                "notion-get-tool-access",
                "notion-get-users",
                "notion-query-data-sources",
                "notion-search",
            ]
        );
        for name in ["notion-search", "notion-fetch"] {
            assert_eq!(server.tools[name], ConnectorTag::Network, "{name}");
        }
        for name in [
            "notion-create-pages",
            "notion-update-page",
            "notion-spawn-session",
        ] {
            assert_eq!(server.tools[name], ConnectorTag::Denied, "{name}");
        }
        assert_eq!(tagged(&server, ConnectorTag::Network), 8);
        assert_eq!(tagged(&server, ConnectorTag::Denied), 28);
    }

    #[test]
    fn context7_signs_in_and_only_reads() {
        let (server, _) = service(Role::Architect, "context7");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = &server.transport
        else {
            panic!("context7 is http");
        };
        assert_eq!(url, "https://mcp.context7.com/mcp/oauth");
        assert_eq!(
            oauth.as_ref().map(|settings| settings.scopes.as_slice()),
            Some(
                &[
                    "profile".to_string(),
                    "email".to_string(),
                    "offline_access".to_string()
                ][..]
            )
        );
        assert!(headers.is_empty());
        assert!(server.credential_keys.is_empty());
        assert_eq!(network_names(&server), ["query-docs", "resolve-library-id"]);
        assert_eq!(server.tools.len(), 2);
    }

    #[test]
    fn grep_needs_no_account_and_only_reads() {
        let (server, _) = service(Role::Architect, "grep");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = &server.transport
        else {
            panic!("grep is http");
        };
        assert_eq!(url, "https://mcp.grep.app");
        assert!(oauth.is_none());
        assert!(headers.is_empty());
        assert!(server.credential_keys.is_empty());
        assert_eq!(
            server.tools,
            BTreeMap::from([("searchGitHub".to_string(), ConnectorTag::Network)])
        );
    }

    #[test]
    fn the_developers_context7_is_the_architects() {
        let (developers, developer_copy) = service(Role::SoftwareDeveloper, "context7");
        let (architects, architect_copy) = service(Role::Architect, "context7");
        assert_eq!(developers, architects);
        assert_ne!(developer_copy.why, architect_copy.why);
        let same = SetupCopy {
            why: architect_copy.why.clone(),
            ..developer_copy
        };
        assert_eq!(same, architect_copy);
    }

    #[test]
    fn the_developers_kit_only_reads() {
        let kit = load_kit(Role::SoftwareDeveloper).expect("the Developer's kit");
        let names: Vec<&str> = kit.connectors.iter().map(KitConnector::name).collect();
        assert_eq!(names, ["context7"]);
        let KitConnector::Server {
            entry,
            copy,
            allowances,
        } = &kit.connectors[0]
        else {
            panic!("context7 is a server");
        };
        let server = custom_server(entry).expect("a custom server");
        assert_eq!(tagged(&server, ConnectorTag::ExternalEffect), 0);
        assert!(allowances.is_empty());
        let network: Vec<&String> = server
            .tools
            .iter()
            .filter(|(_, tag)| **tag == ConnectorTag::Network)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(copy.labels.keys().collect::<Vec<_>>(), network);
    }

    #[test]
    fn osv_is_farik_s_own_server_and_only_reads() {
        let (server, _) = service(Role::Architect, "osv");
        let CustomTransport::Stdio { command, args } = &server.transport else {
            panic!("osv is stdio");
        };
        assert_eq!(command, "farik");
        assert_eq!(args, &["connector".to_string(), "osv".to_string()]);
        assert!(server.credential_keys.is_empty());
        assert_eq!(
            network_names(&server),
            ["get_vulnerability", "query_package", "query_packages"]
        );
        assert_eq!(server.tools.len(), 3);
        let kit = load_kit(Role::Architect).expect("the Architect's kit");
        let names: Vec<&str> = kit.connectors.iter().map(KitConnector::name).collect();
        assert_eq!(names, ["context7", "grep", "osv"]);
        for connector in &kit.connectors {
            let KitConnector::Server {
                entry, allowances, ..
            } = connector
            else {
                panic!("{} is a server", connector.name());
            };
            let server = custom_server(entry).expect("a custom server");
            assert_eq!(
                tagged(&server, ConnectorTag::ExternalEffect),
                0,
                "{}",
                server.name
            );
            assert!(allowances.is_empty(), "{}", server.name);
        }
    }

    /// A guard: it passes with no connector at all.
    #[test]
    fn every_network_tool_of_the_architect_has_a_label() {
        let kit = load_kit(Role::Architect).expect("the Architect's kit");
        for connector in kit.connectors {
            let KitConnector::Server { entry, copy, .. } = connector else {
                continue;
            };
            let server = custom_server(&entry).expect("a custom server");
            let network: Vec<&String> = server
                .tools
                .iter()
                .filter(|(_, tag)| **tag == ConnectorTag::Network)
                .map(|(name, _)| name)
                .collect();
            let labelled: Vec<&String> = copy.labels.keys().collect();
            assert_eq!(labelled, network, "{}", server.name);
        }
    }

    #[test]
    fn the_product_managers_kit_only_reads() {
        let kit = load_kit(Role::ProductManager).expect("the Product Manager's kit");
        let names: Vec<&str> = kit.connectors.iter().map(KitConnector::name).collect();
        assert_eq!(names, ["amplitude", "linear", "notion"]);
        for connector in &kit.connectors {
            let KitConnector::Server {
                entry, allowances, ..
            } = connector
            else {
                panic!("{} is a server", connector.name());
            };
            let server = custom_server(entry).expect("a custom server");
            assert_eq!(
                tagged(&server, ConnectorTag::ExternalEffect),
                0,
                "{}",
                server.name
            );
            assert!(allowances.is_empty(), "{}", server.name);
            assert!(server.credential_keys.is_empty(), "{}", server.name);
            let CustomTransport::Http { oauth, headers, .. } = &server.transport else {
                panic!("{} is http", server.name);
            };
            assert!(oauth.is_some(), "{} signs in", server.name);
            assert!(headers.is_empty(), "{}", server.name);
        }
    }

    /// A guard: it passes with no connector at all.
    #[test]
    fn every_network_tool_of_the_product_manager_has_a_label() {
        let kit = load_kit(Role::ProductManager).expect("the Product Manager's kit");
        for connector in kit.connectors {
            let KitConnector::Server { entry, copy, .. } = connector else {
                continue;
            };
            let server = custom_server(&entry).expect("a custom server");
            for (tool, tag) in &server.tools {
                if *tag == ConnectorTag::Network {
                    assert!(copy.labels.contains_key(tool), "{}: {tool}", server.name);
                }
            }
        }
    }

    /// A Marketing Specialist's service: its server, its copy and its allowances, by name.
    fn marketing_service(name: &str) -> (CustomServer, SetupCopy, BTreeMap<String, KitAllowance>) {
        let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
        for connector in kit.connectors {
            if let KitConnector::Server {
                entry,
                copy,
                allowances,
            } = connector
            {
                let server = custom_server(&entry).expect("a custom server");
                if server.name == name {
                    return (server, copy, allowances);
                }
            }
        }
        panic!("the Marketing Specialist's kit has no {name}");
    }

    /// The tools of `server` with `tag`, sorted by name.
    fn names_tagged(server: &CustomServer, tag: ConnectorTag) -> Vec<&str> {
        server
            .tools
            .iter()
            .filter(|(_, found)| **found == tag)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    fn sorted<'a>(names: &[&'a str]) -> Vec<&'a str> {
        let mut names = names.to_vec();
        names.sort_unstable();
        names
    }

    /// Higgsfield's tools that spend credits and always ask: no allowance covers them.
    const HIGGSFIELD_ASKS: [&str; 23] = [
        // A voice-over can copy a voice from a sample, and a kit cannot refuse one argument.
        "generate_audio",
        "generate_image_batch",
        "generate_video_batch",
        "generate_audio_batch",
        "generate_3d",
        "upscale_video",
        "reframe",
        "motion_control",
        "dubbing",
        "voice_change",
        "ads_studio_generate",
        "ads_studio_create_brand",
        "ads_studio_add_product",
        "ads_studio_update_product",
        "ads_studio_cancel_run",
        "ai_influencer_generate",
        "execute_preset",
        "media_import_url",
        "resolve_explainer_preset",
        "shorts_studio_create",
        "shorts_studio_create_preset",
        "video_analysis_create",
        "virality_predictor",
    ];

    /// Higgsfield's tools that only read.
    const HIGGSFIELD_READS: [&str; 35] = [
        "models_explore",
        "balance",
        "transactions",
        "get_presets",
        "show_generations",
        "show_generation_by_ids",
        "job_display",
        "jobs_wait",
        "ads_studio_quote",
        "ads_studio_list_brands",
        "ads_studio_get_brand",
        "ads_studio_get_product",
        "ads_studio_get_run",
        "ads_studio_list_products",
        "ads_studio_list_runs",
        "ai_influencer_prepare",
        "ai_influencer_read",
        "get_preset_instructions",
        "get_workflow_instructions",
        "get_workflow_bundle_file",
        "list_voices",
        "animation_actions",
        "get_explainer_presets",
        "show_medias",
        "show_marketing_studio_generations",
        "list_projects",
        "list_folders",
        "list_project_assets",
        "list_workspaces",
        "get_preferences",
        "shorts_studio_list_presets",
        "shorts_studio_list_sessions",
        "shorts_studio_status",
        "video_analysis_jobs",
        "video_analysis_status",
    ];

    /// Higgsfield's tools a Farik session is never offered.
    const HIGGSFIELD_NEVER: [&str; 51] = [
        "build_ai_influencer",
        "show_characters",
        "show_reference_elements",
        "manage_reference_elements",
        "show_plans_and_credits",
        "show_credit_reset",
        "show_marketing_studio_v2",
        "update_preferences",
        "select_workspace",
        "cancel_trial_auto_renewal",
        "create_project",
        "create_folder",
        "media_upload",
        "media_confirm",
        "media_upload_widget",
        "create_voice",
        "create_voice_from_confirmed_audio",
        "create_website",
        "deploy_website",
        "publish_website",
        "rename_website",
        "website_db",
        "website_repo_access",
        "website_secrets",
        "website_status",
        "list_websites",
        "list_website_categories",
        "sandbox_exec",
        "participate_in_contest",
        "tiktok_accounts",
        "tiktok_connect",
        "tiktok_reconnect",
        "tiktok_prepare_publish",
        "tiktok_music_trending",
        "tiktok_music_tune",
        "tiktok_publish_status",
        "apps_search",
        "apps_describe",
        "apps_invoke",
        "scene_builder_3d_create_project",
        "scene_builder_3d_get_artifact",
        "scene_builder_3d_get_blend",
        "scene_builder_3d_get_glb",
        "scene_builder_3d_get_operation",
        "scene_builder_3d_get_project",
        "scene_builder_3d_import_asset",
        "scene_builder_3d_list_projects",
        "scene_builder_3d_query_python",
        "scene_builder_3d_run_python",
        "scene_builder_3d_search_assets",
        "scene_builder_3d_show_scene",
    ];

    #[test]
    fn higgsfield_spends_only_what_it_is_allowed() {
        let (server, copy, allowances) = marketing_service("higgsfield");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = &server.transport
        else {
            panic!("higgsfield is http");
        };
        assert_eq!(url, "https://mcp.higgsfield.ai/mcp");
        assert_eq!(
            oauth.as_ref().map(|settings| settings.scopes.as_slice()),
            Some(
                &[
                    "openid".to_string(),
                    "email".to_string(),
                    "offline_access".to_string()
                ][..]
            )
        );
        assert!(headers.is_empty());
        assert!(server.credential_keys.is_empty());
        assert!(copy.key_page.is_none());
        assert_eq!(copy.title, "Higgsfield");

        let allowed = [
            ("generate_image", 20, "images"),
            ("generate_video", 6, "video requests"),
            ("upscale_image", 10, "image upscales"),
            ("remove_background", 10, "background removals"),
            ("outpaint_image", 10, "image extensions"),
        ];
        assert_eq!(allowances.len(), allowed.len());
        for (tool, calls, what) in allowed {
            assert_eq!(server.tools[tool], ConnectorTag::ExternalEffect, "{tool}");
            assert_eq!(
                allowances[tool],
                KitAllowance {
                    calls,
                    what: what.to_string()
                },
                "{tool}"
            );
        }

        for tool in HIGGSFIELD_ASKS {
            assert_eq!(server.tools[tool], ConnectorTag::ExternalEffect, "{tool}");
            assert!(!allowances.contains_key(tool), "{tool} has no allowance");
        }
        assert_eq!(tagged(&server, ConnectorTag::ExternalEffect), 28);

        assert_eq!(
            names_tagged(&server, ConnectorTag::Network),
            sorted(&HIGGSFIELD_READS)
        );

        assert_eq!(
            names_tagged(&server, ConnectorTag::Denied),
            sorted(&HIGGSFIELD_NEVER)
        );
        assert_eq!(server.tools.len(), 114);
        for tool in [
            "sandbox_exec",
            "deploy_website",
            "apps_search",
            "apps_invoke",
            "tiktok_prepare_publish",
            "create_voice_from_confirmed_audio",
            "select_workspace",
            "scene_builder_3d_run_python",
        ] {
            assert_eq!(server.tools[tool], ConnectorTag::Denied, "{tool}");
        }
        assert_eq!(
            copy.labels["generate_video"],
            "make a video or check its price"
        );
    }

    #[test]
    fn recraft_spends_only_what_it_is_allowed() {
        let (server, copy, allowances) = marketing_service("recraft");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = &server.transport
        else {
            panic!("recraft is http");
        };
        assert_eq!(url, "https://mcp.recraft.ai/mcp");
        assert_eq!(
            oauth.as_ref().map(|settings| settings.scopes.as_slice()),
            Some(
                &[
                    "openid".to_string(),
                    "email".to_string(),
                    "profile".to_string()
                ][..]
            )
        );
        assert!(headers.is_empty());
        assert!(server.credential_keys.is_empty());
        assert_eq!(copy.title, "Recraft");

        let allowed = [
            ("generate_image", 20, "images"),
            ("image_to_image", 10, "image edits"),
            ("vectorize_image", 10, "vector conversions"),
            ("remove_background", 10, "background removals"),
            ("replace_background", 10, "background swaps"),
            ("crisp_upscale", 10, "image upscales"),
        ];
        assert_eq!(allowances.len(), allowed.len());
        for (tool, calls, what) in allowed {
            assert_eq!(server.tools[tool], ConnectorTag::ExternalEffect, "{tool}");
            assert_eq!(
                allowances[tool],
                KitAllowance {
                    calls,
                    what: what.to_string()
                },
                "{tool}"
            );
        }
        for tool in ["creative_upscale", "create_style"] {
            assert_eq!(server.tools[tool], ConnectorTag::ExternalEffect, "{tool}");
            assert!(!allowances.contains_key(tool), "{tool} has no allowance");
        }
        assert_eq!(tagged(&server, ConnectorTag::ExternalEffect), 8);
        assert_eq!(names_tagged(&server, ConnectorTag::Network), ["get_user"]);
        assert_eq!(tagged(&server, ConnectorTag::Denied), 0);
        assert_eq!(server.tools.len(), 9);
    }

    /// Buffer's tools that only read.
    const BUFFER_READS: [&str; 10] = [
        "get_account",
        "list_channels",
        "get_channel",
        "list_posts",
        "get_post",
        "get_aggregated_post_metrics",
        "list_ideas",
        "list_idea_groups",
        "list_post_templates",
        "get_post_template",
    ];

    /// Buffer's tools a Farik session is never offered: a deletion cannot be taken back, ideas and
    /// templates are the account's library, and the three generic tools reach the whole API.
    const BUFFER_NEVER: [&str; 8] = [
        "delete_post",
        "create_idea",
        "create_post_template",
        "update_post_template",
        "delete_post_template",
        "introspect_schema",
        "execute_query",
        "execute_mutation",
    ];

    /// Buffer's writes are Farik's own (ADR 0042): the agent writes a post with
    /// `farik_schedule_post`, and Farik calls `create_post` itself, with the agent's connection.
    /// The agent is offered neither of Buffer's writes, and the user's yes to a plan covers them.
    #[test]
    fn buffer_posts_only_through_farik() {
        let (server, copy, allowances) = marketing_service("buffer");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = &server.transport
        else {
            panic!("buffer is http");
        };
        assert_eq!(url, "https://mcp.buffer.com/mcp");
        assert_eq!(
            oauth.as_ref().map(|settings| settings.scopes.as_slice()),
            Some(
                &[
                    "offline_access".to_string(),
                    "posts:read".to_string(),
                    "posts:write".to_string(),
                    "account:read".to_string(),
                    "insights:read".to_string(),
                    "ideas:read".to_string(),
                ][..]
            ),
            "the scopes stay: Farik's own calls post through the same sign-in"
        );
        assert!(headers.is_empty());
        assert!(server.credential_keys.is_empty());
        assert!(copy.key_page.is_none());
        assert_eq!(copy.title, "Buffer");
        assert_eq!(
            copy.why,
            "So the Marketing Specialist can read your channels and how earlier posts did. Farik \
             sends the posts in a marketing plan you approved, and asks you about any other."
        );
        assert_eq!(
            copy.setup,
            "Sign in with your Buffer account and allow Farik to read and schedule posts. Farik \
             can reach every channel your Buffer account has. A post in a plan you approved shows \
             on Today before it goes out, with a Stop button; Farik asks you about any other. To \
             remove Farik completely, also remove it in Buffer's settings."
        );

        let mut never: Vec<&str> = BUFFER_NEVER.to_vec();
        never.extend(["create_post", "edit_post"]);
        for tool in ["create_post", "edit_post"] {
            assert_eq!(server.tools[tool], ConnectorTag::Denied, "{tool}");
            assert!(!copy.labels.contains_key(tool), "{tool} has no label");
        }
        assert_eq!(tagged(&server, ConnectorTag::ExternalEffect), 0);
        assert!(allowances.is_empty());
        assert_eq!(
            names_tagged(&server, ConnectorTag::Network),
            sorted(&BUFFER_READS)
        );
        assert_eq!(names_tagged(&server, ConnectorTag::Denied), sorted(&never));
        assert_eq!(tagged(&server, ConnectorTag::Network), 10);
        assert_eq!(tagged(&server, ConnectorTag::Denied), 10);
        assert_eq!(server.tools.len(), 20);
    }

    /// Kit's drafts, series and pages that change what a subscriber or the public sees, and its
    /// two reads that Kit marks as writes: each asks, and none has an allowance.
    const KIT_ASKS: [&str; 10] = [
        "create_sequence",
        "update_sequence",
        "create_sequence_email",
        "update_sequence_email",
        "create_landing_page",
        "update_landing_page",
        "create_snippet",
        "update_snippet",
        "list_forms",
        "get_landing_page",
    ];

    /// Kit's tools that only read the campaigns, never a person.
    const KIT_READS: [&str; 31] = [
        "get_broadcast",
        "get_broadcast_schema",
        "get_creator_profile",
        "get_current_account",
        "get_email_stats",
        "get_email_template",
        "get_growth_stats",
        "get_landing_page_schema",
        "get_link_clicks_for_a_broadcast",
        "get_post",
        "get_sequence",
        "get_sequence_email",
        "get_sequence_email_schema",
        "get_snippet",
        "get_stats_for_a_broadcast",
        "get_stats_for_a_list_of_broadcasts",
        "list_broadcasts",
        "list_colors",
        "list_custom_fields",
        "list_domains",
        "list_email_templates",
        "list_landing_pages",
        "list_posts",
        "list_prompt_suggestions",
        "list_segments",
        "list_sequence_emails",
        "list_sequences",
        "list_snippets",
        "list_tags",
        "list_products",
        "get_product",
    ];

    /// Kit's tools a Farik session is never offered: every deletion, every write to subscribers,
    /// tags, fields, products, colours and webhooks, every bulk tool, and every read of a person.
    const KIT_NEVER: [&str; 40] = [
        "add_subscriber_to_form",
        "add_subscriber_to_sequence",
        "bulk_add_subscribers_to_forms",
        "bulk_create_custom_fields",
        "bulk_create_subscribers",
        "bulk_create_tags",
        "bulk_delete_tags",
        "bulk_remove_tags_from_subscribers",
        "bulk_tag_subscribers",
        "bulk_update_subscriber_custom_field_values",
        "create_custom_field",
        "create_product",
        "create_subscriber",
        "create_tag",
        "create_webhook",
        "delete_broadcast",
        "delete_custom_field",
        "delete_sequence",
        "delete_sequence_email",
        "delete_webhook",
        "filter_subscribers",
        "get_purchase",
        "get_subscriber",
        "list_purchases",
        "list_stats_for_a_subscriber",
        "list_subscribers",
        "list_subscribers_for_form",
        "list_subscribers_for_sequence",
        "list_subscribers_for_tag",
        "list_tags_for_a_subscriber",
        "list_tax_codes",
        "list_webhooks",
        "remove_tag_from_subscriber",
        "tag_subscriber",
        "unsubscribe",
        "update_colors",
        "update_custom_field",
        "update_product",
        "update_subscriber",
        "update_tag_name",
    ];

    #[test]
    fn kit_drafts_emails_and_never_reads_subscribers() {
        let (server, copy, allowances) = marketing_service("kit");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = &server.transport
        else {
            panic!("kit is http");
        };
        assert_eq!(url, "https://app.kit.com/mcp");
        assert_eq!(
            oauth.as_ref().map(|settings| settings.scopes.as_slice()),
            Some(&["public".to_string()][..])
        );
        assert!(headers.is_empty());
        assert!(server.credential_keys.is_empty());
        assert!(copy.key_page.is_none());
        assert_eq!(copy.title, "Kit");

        // A broadcast is only a draft: the human schedules and sends it from Kit.
        assert_eq!(allowances.len(), 2);
        for tool in ["create_broadcast", "update_broadcast"] {
            assert_eq!(server.tools[tool], ConnectorTag::ExternalEffect, "{tool}");
            assert_eq!(
                allowances[tool],
                KitAllowance {
                    calls: 10,
                    what: "email drafts".to_string()
                },
                "{tool}"
            );
        }
        for tool in KIT_ASKS {
            assert_eq!(server.tools[tool], ConnectorTag::ExternalEffect, "{tool}");
            assert!(!allowances.contains_key(tool), "{tool} has no allowance");
        }
        assert_eq!(tagged(&server, ConnectorTag::ExternalEffect), 12);
        assert_eq!(copy.labels["list_forms"], "list forms and sign-up pages");
        assert_eq!(copy.labels["get_landing_page"], "read a landing page");

        assert_eq!(
            names_tagged(&server, ConnectorTag::Network),
            sorted(&KIT_READS)
        );
        assert_eq!(
            names_tagged(&server, ConnectorTag::Denied),
            sorted(&KIT_NEVER)
        );
        for tool in [
            "list_subscribers",
            "get_subscriber",
            "unsubscribe",
            "delete_broadcast",
        ] {
            assert_eq!(server.tools[tool], ConnectorTag::Denied, "{tool}");
        }
        assert_eq!(server.tools.len(), 83);
    }

    /// A guard (spec 6.7): a post changes what the public sees, so no tool of Buffer carries an
    /// allowance, whatever the kit's own map says.
    #[test]
    fn buffers_posts_have_no_allowance() {
        let (server, _, allowances) = marketing_service("buffer");
        for tool in server.tools.keys() {
            assert!(
                !allowances.contains_key(tool),
                "{tool} has an allowance in the kit's own map"
            );
        }
        assert!(allowances.is_empty(), "{allowances:?}");
    }

    #[test]
    fn the_marketing_kits_services_in_order() {
        let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
        let names: Vec<&str> = kit.connectors.iter().map(KitConnector::name).collect();
        assert_eq!(names, ["higgsfield", "recraft", "buffer", "kit"]);
        for connector in &kit.connectors {
            let KitConnector::Server { entry, .. } = connector else {
                panic!("{} is a server", connector.name());
            };
            let server = custom_server(entry).expect("a custom server");
            let CustomTransport::Http { oauth, .. } = &server.transport else {
                panic!("{} is http", server.name);
            };
            assert!(oauth.is_some(), "{} signs in", server.name);
            assert!(server.credential_keys.is_empty(), "{}", server.name);
        }
    }

    /// A guard: it passes with no marketing connector at all.
    #[test]
    fn every_spending_tool_of_the_marketing_kit_has_a_label() {
        let kit = load_kit(Role::MarketingSpecialist).expect("the Marketing Specialist's kit");
        for connector in kit.connectors {
            let KitConnector::Server { entry, copy, .. } = connector else {
                continue;
            };
            let server = custom_server(&entry).expect("a custom server");
            for (tool, tag) in &server.tools {
                if *tag == ConnectorTag::ExternalEffect {
                    assert!(copy.labels.contains_key(tool), "{}: {tool}", server.name);
                }
            }
        }
    }

    #[test]
    fn holds_every_shipped_kit_to_its_schema() {
        let roles = Path::new(env!("CARGO_MANIFEST_DIR")).join("roles");
        let mut folders = 0;
        for entry in std::fs::read_dir(&roles).expect("the roles folder") {
            let folder = entry.expect("an entry").path();
            let id = folder
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .to_string();
            let text = std::fs::read_to_string(folder.join("kit.yaml"))
                .unwrap_or_else(|_| panic!("{id} has no kit.yaml"));
            let role: Role = id.parse().expect("a folder named for a role");
            parse_kit(
                role,
                &text,
                &load_role(role)
                    .expect("a role")
                    .skills
                    .iter()
                    .map(|s| s.name.clone())
                    .collect::<Vec<_>>(),
                &super::embedded_skills(role),
            )
            .unwrap_or_else(|error| panic!("{id}: {error}"));
            load_kit(role).expect("and load_kit loads it");
            folders += 1;
        }
        assert_eq!(folders, SHIPPED.len());
    }

    #[test]
    fn keeps_the_designers_browser_in_its_kit() {
        let kit = load_kit(Role::UiUxDesigner).expect("the Designer's kit");
        let Some(KitConnector::Container(browser)) = kit.connectors.first() else {
            panic!("the Designer's first connector is a container");
        };
        assert_eq!(browser.name, "playwright");
        assert_eq!(
            browser.image,
            "mcr.microsoft.com/playwright/mcp:v0.0.82@sha256:77dccc5ce9e94cb8ae7ebea87ddbb6cd54b05760c4d63c54e16accf2726b8734"
        );
        assert_eq!(browser.module_root, "/app/node_modules");
        assert_eq!(browser.args, ["--isolated"]);
        assert_eq!(browser.tools.len(), 28);
        assert_eq!(browser.tools["browser_navigate"], ConnectorTag::Network);
        assert_eq!(browser.tools["browser_evaluate"], ConnectorTag::Denied);
        assert_eq!(builtin_connector("playwright").as_ref(), Some(browser));
    }

    #[test]
    fn loads_a_service_with_its_copy_and_allowance() {
        let kit = parse(&base()).expect("the fixture loads");
        let KitConnector::Server {
            entry,
            copy,
            allowances,
        } = &kit.connectors[0]
        else {
            panic!("a server");
        };
        assert_eq!(entry.name.as_str(), "notion");
        assert_eq!(copy.title, "Notion");
        assert_eq!(copy.labels["search"], "search pages");
        assert_eq!(allowances["create_page"].calls, 20);
        assert_eq!(kit.connectors[0].name(), "notion");
    }

    const SKILL: &str = "---\nname: launch-plans\ndescription: Plans a launch.\n---\nSteps.\n";

    #[test]
    fn loads_a_kit_skill_with_its_folder() {
        let mut value = base();
        set(&mut value, "/skills", json!(["launch-plans"]));
        let files: &[(&str, &str)] = &[("SKILL.md", SKILL)];
        let kit = parse_kit(
            Role::ProductManager,
            &value.to_string(),
            &[],
            &[("launch-plans", files)],
        )
        .expect("loads");
        assert_eq!(kit.skills[0].name, "launch-plans");
    }

    #[test]
    fn refuses_a_kit_skill_with_no_folder() {
        let mut value = base();
        set(&mut value, "/skills", json!(["launch-plans"]));
        refused(&value, "/skills/0", "kit_skill_missing");
    }

    #[test]
    fn refuses_a_kit_skill_the_role_file_names() {
        let mut value = base();
        set(&mut value, "/skills", json!(["writing-task-contracts"]));
        let result = parse_kit(
            Role::ProductManager,
            &value.to_string(),
            &["writing-task-contracts".to_string()],
            &[],
        );
        assert!(detail(result).contains("/skills/0: kit_skill_in_role"));
    }

    #[test]
    fn refuses_a_kit_skill_that_runs_commands() {
        let mut value = base();
        set(&mut value, "/skills", json!(["launch-plans"]));
        let text = "---\nname: launch-plans\ndescription: Plans.\n---\nRun !`ls` now.\n";
        let files: &[(&str, &str)] = &[("SKILL.md", text)];
        let result = parse_kit(
            Role::ProductManager,
            &value.to_string(),
            &[],
            &[("launch-plans", files)],
        );
        assert!(detail(result).contains("/skills/0: skill_runs_commands"));
    }

    #[test]
    fn refuses_the_wrong_roles_kit() {
        let result = parse_in(Role::Architect, &base());
        assert!(detail(result).contains("/role: kit_role_mismatch"));
    }

    #[test]
    fn refuses_a_tool_without_a_tag() {
        let mut value = base();
        set(&mut value, "/connectors/0/tools/search", Value::Null);
        let detail = detail(parse(&value));
        assert!(detail.contains("/connectors/0/tools/search"), "{detail}");
    }

    #[test]
    fn refuses_a_tool_tagged_read() {
        let mut value = base();
        set(&mut value, "/connectors/0/tools/search", json!("read"));
        let detail = detail(parse(&value));
        assert!(detail.contains("/connectors/0/tools/search"), "{detail}");
    }

    #[test]
    fn refuses_an_allowance_on_a_tool_that_is_not_external_effect() {
        let mut value = base();
        set(
            &mut value,
            "/connectors/0/allowances",
            json!({ "search": { "calls": 5, "what": "searches" } }),
        );
        refused(
            &value,
            "/connectors/0/allowances/search",
            "allowance_not_external",
        );
    }

    #[test]
    fn refuses_what_the_team_file_would_refuse() {
        let mut value = base();
        set(
            &mut value,
            "/connectors/0/headers",
            json!({ "Authorization": "Bearer abc" }),
        );
        refused(
            &value,
            "/connectors/0/headers/Authorization",
            "header_holds_secret",
        );
    }

    #[test]
    fn refuses_a_key_without_its_page() {
        let mut value = base();
        value["connectors"][0]
            .as_object_mut()
            .expect("an object")
            .remove("key_page");
        refused(&value, "/connectors/0/key_page", "key_page_missing");
    }

    #[test]
    fn refuses_one_connector_name_twice() {
        let mut value = base();
        let twin = value["connectors"][0].clone();
        value["connectors"]
            .as_array_mut()
            .expect("an array")
            .push(twin);
        refused(&value, "/connectors/1/name", "kit_connector_twice");
    }

    #[test]
    fn refuses_a_service_with_no_copy_or_a_foreign_field() {
        let mut value = base();
        value["connectors"][0]
            .as_object_mut()
            .expect("an object")
            .remove("why");
        refused(&value, "/connectors/0/why", "kit_field_missing");
        let mut value = base();
        value["connectors"][0]["image"] = json!("x@sha256:abc");
        refused(&value, "/connectors/0/image", "kit_field_not_allowed");
    }

    #[test]
    fn refuses_a_label_for_a_tool_it_lacks() {
        let mut value = base();
        set(&mut value, "/connectors/0/labels", json!({ "nope": "x" }));
        refused(&value, "/connectors/0/labels/nope", "label_not_a_tool");
    }

    fn stdio(command: &str, args: &[&str]) -> Value {
        let mut value = base();
        let entry = value["connectors"][0].as_object_mut().expect("an object");
        for gone in [
            "url",
            "headers",
            "credential_keys",
            "key_page",
            "allowances",
        ] {
            entry.remove(gone);
        }
        entry.insert("transport".into(), json!("stdio"));
        entry.insert("command".into(), json!(command));
        entry.insert("args".into(), json!(args));
        value
    }

    #[test]
    fn refuses_an_unpinned_package() {
        refused(
            &stdio("npx", &["-y", "@notionhq/notion-mcp-server"]),
            "/connectors/0/args",
            "package_not_pinned",
        );
        parse(&stdio("npx", &["-y", "@notionhq/notion-mcp-server@1.8.1"])).expect("pinned loads");
        refused(
            &stdio("uvx", &["mcp-server-fetch"]),
            "/connectors/0/args",
            "package_not_pinned",
        );
        refused(
            &stdio("npx", &["-y", "left-pad@latest"]),
            "/connectors/0/args",
            "package_not_pinned",
        );
        refused(
            &stdio("bunx", &["left-pad@^1.2.3"]),
            "/connectors/0/args",
            "package_not_pinned",
        );
        parse(&stdio("uvx", &["mcp-server-fetch==2025.4.7"])).expect("pinned loads");
        parse(&stdio("./server", &[])).expect_err("a relative command is the team file's refusal");
        for (command, args, pointer) in [
            ("npx", vec!["-y", "evil", "--flag=x@1.2.3"], "args"),
            ("npx", vec!["-y", "./local@1.2.3"], "args"),
            ("npx", vec!["-y", "https://example.com/x@1.2.3"], "args"),
            (
                "uvx",
                vec!["--from", "git+https://github.com/u/r@1.2.3", "r"],
                "args",
            ),
            ("uvx", vec!["--from", "pkg", "x==1.2"], "args"),
            ("uvx", vec!["--from=pkg==1.2", "x==1.2"], "args"),
            ("npx", vec!["-p", "evil@1.2.3", "x@1.2.3"], "args"),
            ("npx", vec!["-y", "evil@1"], "args"),
            ("npx", vec!["-y", "@1.2.3"], "args"),
            ("npx", vec!["-y"], "args"),
            ("uvx", vec!["==1.2"], "args"),
            ("pipx", vec!["run", "evil"], "args"),
            ("pipx", vec!["evil==1.2.3"], "args"),
            ("npx.cmd", vec!["-y", "evil"], "args"),
            ("/usr/bin/npx", vec!["-y", "evil"], "args"),
            ("pnpm", vec!["dlx", "evil"], "command"),
            ("npm", vec!["exec", "--yes", "evil"], "command"),
            ("yarn", vec!["dlx", "evil"], "command"),
            ("bun", vec!["x", "evil"], "command"),
            ("uv", vec!["tool", "run", "evil"], "command"),
            ("sh", vec!["-c", "npx -y evil"], "command"),
            ("/bin/bash", vec!["-c", "npx -y evil"], "command"),
            ("docker", vec!["run", "-i", "evil:latest"], "command"),
        ] {
            let value = stdio(command, &args);
            let found = detail(parse(&value));
            assert!(
                found.contains(&format!("/connectors/0/{pointer}: package_not_pinned")),
                "{command} {args:?}: {found}"
            );
        }
        for (command, args) in [
            ("npx", vec!["-y", "@notionhq/notion-mcp-server@1.8.1"]),
            ("uvx", vec!["mcp-server-fetch==2025.4.7"]),
            ("pipx", vec!["run", "mcp-x==1.2.3"]),
            ("/usr/bin/npx", vec!["-y", "left-pad@1.2.3"]),
        ] {
            parse(&stdio(command, &args)).unwrap_or_else(|e| panic!("{command} {args:?}: {e}"));
        }
        // Any other program, even by absolute path, is refused: only Farik's own binary is allowed.
        for command in ["/usr/local/bin/server", "/usr/bin/node", "/usr/bin/python3"] {
            let found = detail(parse(&stdio(command, &["--flag"])));
            assert!(
                found.contains("/connectors/0/command: package_not_pinned"),
                "{command}: {found}"
            );
        }
        parse(&stdio("/usr/local/bin/farik-mcp", &["--flag"])).expect("Farik's own binary");
    }

    #[test]
    fn accepts_farik_by_its_bare_name() {
        parse(&stdio("farik", &["connector", "osv"])).expect("Farik's own OSV server loads");
    }

    #[test]
    fn refuses_farik_with_other_arguments() {
        for args in [
            vec!["serve"],
            vec!["connector", "run"],
            vec!["connector", "osv", "--x"],
            vec!["connector", "other"],
            vec![],
        ] {
            refused(
                &stdio("farik", &args),
                "/connectors/0/args",
                "package_not_pinned",
            );
        }
        assert!(detail(parse(&stdio("farik", &["serve"]))).contains("farik connector <name>"));
    }

    /// A guard: it passes before and after the bare `farik` is accepted.
    #[test]
    fn refuses_any_other_bare_program() {
        for command in [
            "farik-osv",
            "./farik",
            "bin/farik",
            "farikx",
            "FARIK",
            "farik.exe",
            "farik.cmd",
        ] {
            let found = detail(parse(&stdio(command, &["connector", "osv"])));
            assert!(
                found.contains("/connectors/0/command: package_not_pinned"),
                "{command}: {found}"
            );
        }
        parse(&stdio("/usr/local/bin/farik-mcp", &["connector", "osv"]))
            .expect("an absolute Farik binary stays accepted");
    }

    fn browser(role: Role, name: &str, image: &str) -> Result<Kit, KitError> {
        let value = json!({
            "role": role.to_string(),
            "skills": [],
            "connectors": [{
                "name": name, "transport": "container", "image": image,
                "module_root": "/app/node_modules", "args": ["--isolated"],
                "tools": { "browser_navigate": "network" }
            }]
        });
        parse_in(role, &value)
    }

    const PINNED: &str = "mcr.microsoft.com/playwright/mcp:v0.0.82@sha256:77dccc5ce9e94cb8ae7ebea87ddbb6cd54b05760c4d63c54e16accf2726b8734";

    #[test]
    fn refuses_a_container_farik_does_not_run() {
        browser(Role::UiUxDesigner, "playwright", PINNED).expect("the built-in loads");
        let text = detail(browser(Role::UiUxDesigner, "selenium", PINNED));
        assert!(
            text.contains("/connectors/0/name: container_not_builtin"),
            "{text}"
        );
    }

    #[test]
    fn refuses_an_unpinned_image() {
        let text = detail(browser(
            Role::UiUxDesigner,
            "playwright",
            "mcr.microsoft.com/playwright/mcp:v0.0.82",
        ));
        assert!(
            text.contains("/connectors/0/image: image_not_pinned"),
            "{text}"
        );
    }

    #[test]
    fn refuses_the_browser_in_another_roles_kit() {
        let text = detail(browser(Role::SoftwareDeveloper, "playwright", PINNED));
        assert!(
            text.contains("/connectors/0/name: container_not_builtin"),
            "{text}"
        );
    }

    #[test]
    fn refuses_copy_that_names_the_plumbing() {
        for word in ["MCP", "OAuth", "tokens"] {
            let text = format!("Paste the {word} here");
            for (pointer, field) in [
                ("/connectors/0/setup", "setup"),
                ("/connectors/0/why", "why"),
                ("/connectors/0/about", "about"),
                ("/connectors/0/title", "title"),
                ("/connectors/0/labels/search", "label"),
                ("/connectors/0/allowances/create_page/what", "what"),
            ] {
                let mut value = base();
                set(&mut value, pointer, json!(text));
                let detail = detail(parse(&value));
                assert!(
                    detail.contains(&format!("{pointer}: copy_word_refused")),
                    "{word} in {field}: {detail}"
                );
            }
        }
    }

    fn setup_text(text: &str) -> Result<Kit, KitError> {
        let mut value = base();
        set(&mut value, "/connectors/0/setup", json!(text));
        parse(&value)
    }

    #[test]
    fn accepts_a_services_quoted_label_in_setup() {
        for text in [
            "On Slack's page, copy the value labelled ‘Bot User OAuth Token’.",
            "On Slack's page, copy the value labelled “Bot User OAuth Token”.",
            "On Slack's page, copy the value labelled \"Bot User OAuth Token\".",
        ] {
            setup_text(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        }
        let sixty = format!("Copy ‘{}token’ from the page", "x".repeat(55));
        setup_text(&sixty).unwrap_or_else(|error| panic!("{sixty}: {error}"));
    }

    #[test]
    fn checks_the_words_outside_a_quoted_label() {
        let long = format!("‘{}token’", "x".repeat(56));
        assert_eq!(long.chars().count() - 2, 61);
        for text in [
            "Paste the OAuth token from ‘Settings’".to_string(),
            "On Slack's page, copy 'Bot User OAuth Token'.".to_string(),
            "Copy ‘Bot User OAuth Token from the page".to_string(),
            format!("Copy {long} from the page"),
            "Copy ‘Bot User\nOAuth Token’ from the page".to_string(),
        ] {
            let detail = detail(setup_text(&text));
            assert!(
                detail.contains("/connectors/0/setup: copy_word_refused"),
                "{text}: {detail}"
            );
        }
        for pointer in [
            "/connectors/0/why",
            "/connectors/0/about",
            "/connectors/0/title",
            "/connectors/0/labels/search",
            "/connectors/0/allowances/create_page/what",
        ] {
            let mut value = base();
            set(&mut value, pointer, json!("Use ‘Bot User OAuth Token’"));
            assert!(
                detail(parse(&value)).contains(&format!("{pointer}: copy_word_refused")),
                "{pointer}"
            );
        }
    }

    #[test]
    fn finds_quoted_labels() {
        let text = "a ‘b’ c “d” \"e\" f's 'g'";
        let found: Vec<&str> = quoted_labels(text)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(found, ["b", "d", "e"]);
    }

    #[test]
    fn counts_kit_skills_as_shipped() {
        let mut value = base();
        set(&mut value, "/skills", json!(["launch-plans"]));
        let files: &[(&str, &str)] = &[("SKILL.md", SKILL)];
        let kit = parse_kit(
            Role::ProductManager,
            &value.to_string(),
            &[],
            &[("launch-plans", files)],
        )
        .expect("loads");
        let role = load_role(Role::Architect).expect("a role");
        let names = shipped_skill_names(std::slice::from_ref(&role), &[kit]);
        assert!(names.contains("launch-plans"));
        assert!(names.contains(role.skills[0].name.as_str()));
        let every: Vec<_> = SHIPPED
            .iter()
            .map(|r| load_role(*r).expect("a role"))
            .collect();
        let kits: Vec<_> = SHIPPED
            .iter()
            .map(|r| load_kit(*r).expect("a kit"))
            .collect();
        let shipped: std::collections::BTreeSet<String> =
            core_skill_names().into_iter().map(str::to_string).collect();
        assert_eq!(shipped, shipped_skill_names(&every, &kits));
    }

    #[test]
    fn pin_drift_names_what_was_added_and_dropped() {
        let pinned = [("a", ConnectorTag::Network), ("b", ConnectorTag::Denied)]
            .into_iter()
            .map(|(name, tag)| (name.to_string(), tag))
            .collect();
        let drift = pin_drift(&pinned, &["b".to_string(), "c".to_string()]);
        assert_eq!(
            (drift.added, drift.removed),
            (vec!["c".to_string()], vec!["a".to_string()])
        );
        let same = pin_drift(&pinned, &["b".to_string(), "a".to_string()]);
        assert!(same.added.is_empty() && same.removed.is_empty());
    }
}
