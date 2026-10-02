//! The team (`docs/SPEC.md` sections 3, 5.12, 5.14 and 5.16): `docs/schemas/team.schema.json` as
//! Rust types, the validator that turns an untrusted JSON value into one, and the three rules the
//! schema cannot say.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::contract::{named, pointer, repeated_ids, with_integers_normalised};
use crate::governor::permissions::{ConnectorTag, PermissionTier, default_tiers};
use crate::governor::team_rules::TeamRules;

pub use crate::contract::{Role, ValidationError};
pub use crate::generated::team::{
    Agent, AgentId, AgentStatus, Budgets as TeamBudgets, ConnectorTag as ConnectorTagWire,
    FarikTeam as Team, JudgmentJudge as JudgeChoice, JudgmentRequired, McpServer as McpServerWire,
    McpServerSource, McpServerTransport, Model as AgentModel, ModelEffort as Effort,
    PermissionTier as PermissionTierWire, Permissions as TeamPermissions, Policy as TeamPolicy,
    PolicyHumanAcceptsContracts as HumanAcceptsContracts, PolicyIntegration as Integration,
    Role as RoleWire, Rules as RulesWire, SessionLimits as SessionLimitsWire, SkillPin,
};

/// Wire fixtures for tests, in this crate and in others.
pub mod fixtures;

mod defaults;
mod describe;
mod template;

pub use defaults::{SMALL_ENOUGH_QUESTION, TeamDefaults, defaults};
pub use describe::{MODEL_FAMILIES, SprintWork, describe_change};
pub use template::{
    TeamTemplate, TemplateAgent, TemplateApplied, apply_template, template_from_team,
    template_slug, validate_template,
};

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
    (RoleWire::ProductManager, "write its plans"),
    (RoleWire::SoftwareDeveloper, "do the work"),
];

/// The connectors Farik ships, by name (spec 5.6); `farik-roles` holds their definitions.
pub const BUILTIN_CONNECTORS: [&str; 1] = ["playwright"];

/// The most agents a team has that are not retired (spec 4.1). A retired agent stays in the file
/// so that its past events still name someone, so it is not one of them.
pub const MAX_AGENTS: usize = 7;

/// The role a team named to check plans; `None` for `auto`.
fn named_judge(choice: JudgeChoice) -> Option<Role> {
    match choice {
        JudgeChoice::Auto => None,
        JudgeChoice::Architect => Some(Role::Architect),
        JudgeChoice::ScrumMaster => Some(Role::ScrumMaster),
    }
}

/// A role as a person reads it.
#[must_use]
pub fn plain_role(role: Role) -> &'static str {
    match role {
        Role::ProductManager => "Product Manager",
        Role::ScrumMaster => "Scrum Master",
        Role::Architect => "Architect",
        Role::SoftwareDeveloper => "Software Developer",
        Role::MarketingSpecialist => "Marketing Specialist",
        Role::UiUxDesigner => "UI/UX Designer",
        Role::Human => "human",
    }
}

/// Whether a role's tasks change code: the Software Developer's and the UI/UX Designer's
/// (`docs/SPEC.md` sections 5.12 and 5.14). Every other role's task is held to the document paths
/// and works on a `docs/` branch.
#[must_use]
pub fn changes_code(role: Role) -> bool {
    matches!(role, Role::SoftwareDeveloper | Role::UiUxDesigner)
}

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
/// Two more are the plan check's (spec 5.3, spec 10's foolproof configuration): a judge the team
/// names must be an active agent, and a team that checks every plan must ask at least one question.
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
    if team
        .agents
        .iter()
        .filter(|agent| agent.status != AgentStatus::Retired)
        .count()
        > MAX_AGENTS
    {
        errors.push(ValidationError {
            path: "/agents".to_string(),
            message: "A team has seven agents at most. Retire one before adding another."
                .to_string(),
        });
    }
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
                    "{id} must keep reading the project: every agent reads, and one that cannot \
                     is refused everything it tries. Pause {id} instead.",
                    id = agent.id.as_str()
                ),
            });
        }
    }
    errors.extend(connector_errors(&team));
    errors.extend(skill_errors(&team));
    let judgment = team.judgment();
    if let Some(role) = named_judge(judgment.judge)
        && !team.has_active(role)
    {
        errors.push(ValidationError {
            path: "/policy/judgment/judge".to_string(),
            message: format!(
                "No active {} can check plans. Let Farik choose, or add one.",
                plain_role(role)
            ),
        });
    }
    if judgment.required == JudgmentRequired::Always && judgment.questions.is_empty() {
        errors.push(ValidationError {
            path: "/policy/judgment/questions".to_string(),
            message: "Checking plans needs at least one question.".to_string(),
        });
    }
    for (role, what) in REQUIRED_ROLES {
        if !team.has_active(Role::from(role)) {
            errors.push(ValidationError {
                path: "/agents".to_string(),
                message: format!(
                    "A team needs an active {} to {what}, and this one has none.",
                    plain_role(Role::from(role))
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

/// Every connector rule the schema cannot say (spec 5.6, F8), each refusal at its own field: a
/// built-in one Farik ships and nothing else; a custom one named for nothing Farik reserves, with
/// the fields of its transport and no other's, an address that holds no secret, and headers that
/// name only its own keys; and one name once on an agent.
fn connector_errors(team: &Team) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for (index, agent) in team.agents.iter().enumerate() {
        let mut names = Vec::new();
        for (at, server) in agent.mcp_servers.iter().flatten().enumerate() {
            let name = server.name.as_str();
            let mut refused = server_errors(server);
            if names.contains(&name) {
                refused.insert(
                    0,
                    (
                        "name".to_string(),
                        format!(
                            "connector_name_twice: this agent already has a connector named \
                             {name}."
                        ),
                    ),
                );
            }
            names.push(name);
            errors.extend(refused.into_iter().map(|(field, message)| ValidationError {
                path: format!("/agents/{index}/mcp_servers/{at}/{field}"),
                message,
            }));
        }
    }
    errors
}

/// A skill name pinned twice in one list, at the second (spec 6.7): the team's list and each
/// agent's are each checked alone, since an agent's skill replaces the team's of its name.
fn skill_errors(team: &Team) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut check = |pins: &[SkillPin], at: &str| {
        let mut names = Vec::new();
        for (index, pin) in pins.iter().enumerate() {
            let name = pin.name.as_str();
            if names.contains(&name) {
                errors.push(ValidationError {
                    path: format!("{at}/{index}/name"),
                    message: format!("skill_name_twice: {name} is already given in this list."),
                });
            }
            names.push(name);
        }
    };
    check(team.skills(), "/skills");
    for (index, agent) in team.agents.iter().enumerate() {
        check(&agent.skills, &format!("/agents/{index}/skills"));
    }
    errors
}

/// One `mcp_servers` entry's refusals, each as its field below the entry and its message: what
/// `validate_team` gives that entry, which a kit's connector is held to as well.
#[must_use]
pub fn server_errors(server: &McpServerWire) -> Vec<(String, String)> {
    let name = server.name.as_str();
    let given = present_fields(server);
    let mut refused = Vec::new();
    let mut refuse = |field: &str, message: String| refused.push((field.to_string(), message));
    if server.source == McpServerSource::Builtin {
        if !BUILTIN_CONNECTORS.contains(&name) {
            refuse(
                "name",
                format!(
                    "unknown_connector: {name} is not a connector Farik ships; the one it ships \
                     is playwright."
                ),
            );
        }
        for field in given {
            refuse(
                field,
                format!("A connector Farik ships has no {field}: Farik knows it."),
            );
        }
        return refused;
    }
    if name == "farik" || BUILTIN_CONNECTORS.contains(&name) {
        refuse(
            "name",
            format!(
                "connector_name_reserved: {name} is a name Farik keeps for itself. Name this \
                 connector something else."
            ),
        );
    }
    let (other, required, what) = match server.transport {
        None => {
            refuse(
                "transport",
                "Say how this connector is reached: stdio, a command Farik starts, or http, a \
                 web address."
                    .to_string(),
            );
            return refused;
        }
        Some(McpServerTransport::Stdio) => (["url", "headers"], "command", "started by a command"),
        Some(McpServerTransport::Http) => (["command", "args"], "url", "reached at a web address"),
    };
    for field in other {
        if given.contains(&field) {
            refuse(
                field,
                format!("This connector is {what}, so it has no {field}."),
            );
        }
    }
    if !given.contains(&required) {
        refuse(
            required,
            format!("A connector {what} needs its {required}."),
        );
    }
    if let Some(command) = server.command.as_deref().map(String::as_str)
        && command.contains('/')
        && !command.starts_with('/')
    {
        refuse(
            "command",
            format!(
                "command_not_absolute: {command} would be looked for in the folder the connector \
                 runs in. Give the program's name alone, found on PATH, or its full path."
            ),
        );
    }
    refused.extend(secret_errors(server));
    refused.extend(oauth_errors(server));
    refused.extend(allowance_errors(server));
    refused
}

/// The refusals for an entry's allowances (ADR 0037): on a kit entry's external tools alone.
fn allowance_errors(server: &McpServerWire) -> Vec<(String, String)> {
    server
        .allowances
        .keys()
        .filter_map(|tool| {
            let tool = tool.as_str();
            let message = if server.source != McpServerSource::Kit {
                format!(
                    "allowance_not_kit: only a service of the role's kit has an allowance; \
                     {tool} always asks."
                )
            } else if !matches!(
                server
                    .tools
                    .iter()
                    .find(|(name, _)| name.as_str() == tool)
                    .map(|(_, tag)| connector_tag(*tag)),
                Some(ConnectorTag::ExternalEffect)
            ) {
                format!(
                    "allowance_not_external: {tool} is not a tool that spends or changes \
                     something outside Farik, so it has no allowance."
                )
            } else {
                return None;
            };
            Some((format!("allowances/{tool}"), message))
        })
        .collect()
}

/// The refusals for an entry that signs in (ADR 0033): where it cannot, and a port with no client.
fn oauth_errors(server: &McpServerWire) -> Vec<(String, String)> {
    let Some(oauth) = &server.oauth else {
        return Vec::new();
    };
    let mut refused = Vec::new();
    if server.transport == Some(McpServerTransport::Stdio) {
        refused.push((
            "oauth".to_string(),
            "oauth_on_stdio: a connector Farik starts has no sign-in; give it a key.".to_string(),
        ));
    }
    if server
        .credential_keys
        .as_ref()
        .is_some_and(|keys| !keys.is_empty())
    {
        refused.push((
            "credential_keys".to_string(),
            "oauth_with_keys: a connector that signs in takes no keys.".to_string(),
        ));
    }
    for header in server.headers.keys() {
        if header.as_str().eq_ignore_ascii_case("authorization") {
            refused.push((
                format!("headers/{}", header.as_str()),
                "oauth_header_conflict: a connector that signs in sends its own Authorization \
                 header."
                    .to_string(),
            ));
        }
    }
    if oauth.callback_port.is_some() && oauth.client_id.is_none() {
        refused.push((
            "oauth/callback_port".to_string(),
            "callback_port_without_client: the port belongs to a client the service registered \
             for Farik, so give its client_id too."
                .to_string(),
        ));
    }
    refused
}

/// An entry's refusals for a value the committed team file must not hold: a key in its
/// arguments, its address, or a header.
fn secret_errors(server: &McpServerWire) -> Vec<(String, String)> {
    let mut refused = Vec::new();
    let mut refuse = |field: &str, message: String| refused.push((field.to_string(), message));
    let args: Vec<&str> = server.args.iter().map(|arg| arg.as_str()).collect();
    for index in args_holding_secrets(&args) {
        refuse(
            &format!("args/{index}"),
            format!(
                "arg_holds_secret: argument {} looks like a key, and the team file is shared. \
                 Name the key in credential_keys and give it when you connect: the connector \
                 reads it from its environment.",
                index + 1
            ),
        );
    }
    if let Some(url) = &server.url
        && url_holds_secret(url)
    {
        refuse(
            "url",
            "url_holds_secret: this address carries a name, a password or a query, and the team \
             file is shared. A service whose address holds a key is not supported yet."
                .to_string(),
        );
    }
    let keys: Vec<&str> = server
        .credential_keys
        .iter()
        .flatten()
        .map(|key| key.as_str())
        .collect();
    for (header, template) in &server.headers {
        let header = header.as_str();
        if let Some(message) = header_error(header, template, &keys) {
            refuse(&format!("headers/{header}"), message);
        }
    }
    refused
}

/// The fields beside `name` and `source` an entry gives, in the schema's order.
fn present_fields(server: &McpServerWire) -> Vec<&'static str> {
    [
        ("transport", server.transport.is_some()),
        ("command", server.command.is_some()),
        ("args", !server.args.is_empty()),
        ("url", server.url.is_some()),
        ("headers", !server.headers.is_empty()),
        ("credential_keys", server.credential_keys.is_some()),
        ("tools", !server.tools.is_empty()),
        ("allowances", !server.allowances.is_empty()),
        ("oauth", server.oauth.is_some()),
    ]
    .into_iter()
    .filter_map(|(field, given)| given.then_some(field))
    .collect()
}

/// Whether an address carries userinfo or a query, which a committed team file must not hold.
fn url_holds_secret(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority.contains('@') || rest.contains('?')
}

/// Why the header `header` with `template` is refused, when it is: it names a key the connector
/// is not given, or it carries a credential and holds a value itself, alone or beside a `{KEY}`.
fn header_error(header: &str, template: &str, keys: &[&str]) -> Option<String> {
    if !header_names_only(template, keys) {
        return Some(format!(
            "header_key_unknown: {header} may hold {{KEY}} only for a key this connector is given \
             in credential_keys."
        ));
    }
    (names_a_secret(header) && (!template.contains('{') || holds_a_value(template))).then(|| {
        format!(
            "header_holds_secret: {header} holds a value itself, and the team file is shared. \
             Write {{KEY}} where each key goes, with KEY in credential_keys, and give the key when \
             you connect."
        )
    })
}

/// Whether a header's name says it carries a credential: `Authorization`, `Cookie`, or a name
/// with a word such as `key`, `token`, `secret`, `auth` or `password`, in any case. Words are split
/// at `-` and `_`, so `X-Monkey` is no credential.
fn names_a_secret(header: &str) -> bool {
    header.to_ascii_lowercase().split(['-', '_']).any(|word| {
        matches!(
            word,
            "authorization"
                | "cookie"
                | "key"
                | "apikey"
                | "token"
                | "secret"
                | "auth"
                | "password"
                | "passwd"
                | "pass"
                | "credential"
                | "credentials"
        )
    })
}

/// Whether a flag or variable's name says its value is a key (re-review 2 m3): its last word, at
/// `-` and `_`, is `token`, `password`, `secret`, `auth`, `pat` or `credentials`, or it is
/// `api-key`, `apikey`, `access-key` or `secret-key`. A lone `key`, `sort-key` or `primary-key`
/// names a setting; so does `no-auth`.
fn flag_names_a_key(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let words: Vec<&str> = name.split(['-', '_']).filter(|w| !w.is_empty()).collect();
    match words.as_slice() {
        ["no", ..] | [] => false,
        [.., before, "key"] => matches!(*before, "api" | "access" | "secret"),
        [.., last] => matches!(
            *last,
            "apikey"
                | "token"
                | "password"
                | "passwd"
                | "pass"
                | "secret"
                | "auth"
                | "pat"
                | "credential"
                | "credentials"
        ),
    }
}

/// Whether `value`, given to a flag that names a key, is a key: not empty, and not a number,
/// which a setting such as `--max-token 4096` takes.
fn is_a_value(value: &str) -> bool {
    !value.is_empty() && !value.chars().all(|c| c.is_ascii_digit())
}

/// Whether one argument holds a key by itself: a well-known service's key, such as `sk-…`,
/// `sk_live_…`, `ghp_…` or `AKIA…`; a credential header with its value, as `mcp-remote` takes after
/// `--header`; or `NAME=value` where `NAME` names a key.
fn arg_is_a_key(arg: &str) -> bool {
    let prefixed = [
        "sk-",
        "sk_live_",
        "sk_test_",
        "rk_live_",
        "rk_test_",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "glpat-",
        "xoxb-",
        "xoxp-",
    ]
    .iter()
    .any(|prefix| arg.starts_with(prefix));
    let aws = arg.len() == 20
        && arg.starts_with("AKIA")
        && arg
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let header = arg.split_once(':').is_some_and(|(name, value)| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && names_a_secret(name)
            && !value.trim().is_empty()
    });
    let variable = arg.split_once('=').is_some_and(|(name, value)| {
        !name.starts_with('-') && flag_names_a_key(name) && is_a_value(value)
    });
    prefixed || aws || header || variable
}

/// Whether a credential header's template holds text that may be a value (re-review N4). Split at
/// spaces and `;`, each piece is a `{KEY}`, a `name={KEY}`, or, first, a scheme of letters alone,
/// such as `Bearer` or `OAuth2`.
fn holds_a_value(template: &str) -> bool {
    template
        .split([' ', ';'])
        .filter(|piece| !piece.is_empty())
        .enumerate()
        .any(|(index, piece)| match piece.split_once('{') {
            None => {
                index > 0
                    || !piece.starts_with(|c: char| c.is_ascii_alphabetic())
                    || !piece.chars().all(|c| c.is_ascii_alphanumeric())
            }
            Some((before, after)) => {
                let name = before.strip_suffix('=').unwrap_or(before);
                let names = before.is_empty()
                    || (before.ends_with('=')
                        && !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')));
                !names
                    || after
                        .split_once('}')
                        .is_none_or(|(_, rest)| !rest.is_empty())
            }
        })
}

/// The index of each argument that holds a key's value (re-review N4, re-review 2 m3): one given
/// to a flag that names a key ([`flag_names_a_key`]), as `--api-key abc` or `--token=abc`, or one
/// that is a key by itself ([`arg_is_a_key`]).
fn args_holding_secrets(args: &[&str]) -> Vec<usize> {
    let mut held = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        let flag = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-'));
        let holds = match flag.map(|flag| flag.split_once('=')) {
            Some(Some((name, value))) => {
                (flag_names_a_key(name) && is_a_value(value)) || arg_is_a_key(value)
            }
            Some(None) => {
                if flag_names_a_key(flag.unwrap_or_default())
                    && args
                        .get(index + 1)
                        .is_some_and(|next| !next.starts_with("--") && is_a_value(next))
                {
                    held.push(index + 1);
                }
                false
            }
            None => arg_is_a_key(arg),
        };
        if holds {
            held.push(index);
        }
    }
    held.sort_unstable();
    held.dedup();
    held
}

/// Whether every `{` in a header template opens a `{KEY}` for one of `keys`.
fn header_names_only(template: &str, keys: &[&str]) -> bool {
    template.split('{').skip(1).all(|part| {
        part.split_once('}')
            .is_some_and(|(key, _)| keys.contains(&key))
    })
}

/// A connector tool's label as the schema spells it, as the governor's.
fn connector_tag(tag: ConnectorTagWire) -> ConnectorTag {
    match tag {
        ConnectorTagWire::Network => ConnectorTag::Network,
        ConnectorTagWire::ExternalEffect => ConnectorTag::ExternalEffect,
        ConnectorTagWire::Denied => ConnectorTag::Denied,
    }
}

/// A custom connector, as `validate_team` let it through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomServer {
    /// Its name on the agent, and its tools' `mcp__<name>__` prefix.
    pub name: String,
    /// How Farik starts or reaches it.
    pub transport: CustomTransport,
    /// The names of the agent's keys it is given; their values are never in the team file.
    pub credential_keys: Vec<String>,
    /// Each tool the server listed at connect, with the user's label.
    pub tools: BTreeMap<String, ConnectorTag>,
    /// Whether the entry is a kit's (`source: kit`), whose tags are the kit's.
    pub kit: bool,
    /// How many calls each period the agent makes of a tool without asking (ADR 0037), a kit
    /// entry's alone and only for tools it tags `external_effect`.
    pub allowances: BTreeMap<String, u32>,
}

/// How a custom connector is started or reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomTransport {
    /// A program Farik starts on the host, spoken to over its standard input and output.
    Stdio {
        /// The program, found on `PATH`.
        command: String,
        /// Its arguments.
        args: Vec<String>,
    },
    /// A web address, spoken to over streamable HTTP.
    Http {
        /// The address.
        url: String,
        /// Each header's template, which may hold `{KEY}`.
        headers: BTreeMap<String, String>,
        /// Present when the user signed in to the service instead of pasting a key.
        oauth: Option<OAuthSettings>,
    },
}

/// How an http connector signs in (ADR 0033).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSettings {
    /// A public client the service's app registration gave, else Farik registers one.
    pub client_id: Option<String>,
    /// The port the pre-registered client's redirect names.
    pub callback_port: Option<u16>,
    /// The scopes to ask for; none means the service's own selection.
    pub scopes: Vec<String>,
}

/// The sign-in settings as the spec hash holds them.
#[must_use]
pub fn oauth_json(settings: &OAuthSettings) -> Value {
    serde_json::json!({
        "client_id": settings.client_id,
        "callback_port": settings.callback_port,
        "scopes": settings.scopes,
    })
}

/// The custom or kit connector a validated `mcp_servers` entry describes; `None` for a built-in one.
/// This is the crate's one mapping from the generated entry.
#[must_use]
pub fn custom_server(server: &McpServerWire) -> Option<CustomServer> {
    if server.source == McpServerSource::Builtin {
        return None;
    }
    let transport = match server.transport? {
        McpServerTransport::Stdio => CustomTransport::Stdio {
            command: server.command.as_deref()?.clone(),
            args: server
                .args
                .iter()
                .map(|arg| arg.as_str().to_string())
                .collect(),
        },
        McpServerTransport::Http => CustomTransport::Http {
            url: server.url.as_deref()?.clone(),
            headers: server
                .headers
                .iter()
                .map(|(name, value)| (name.as_str().to_string(), value.as_str().to_string()))
                .collect(),
            oauth: server.oauth.as_ref().map(|oauth| OAuthSettings {
                client_id: oauth.client_id.as_ref().map(|id| id.as_str().to_string()),
                callback_port: oauth
                    .callback_port
                    .and_then(|port| u16::try_from(port).ok()),
                scopes: oauth
                    .scopes
                    .iter()
                    .map(|scope| scope.as_str().to_string())
                    .collect(),
            }),
        },
    };
    Some(CustomServer {
        name: server.name.as_str().to_string(),
        transport,
        credential_keys: server
            .credential_keys
            .iter()
            .flatten()
            .map(|key| key.as_str().to_string())
            .collect(),
        tools: server
            .tools
            .iter()
            .map(|(tool, tag)| (tool.as_str().to_string(), connector_tag(*tag)))
            .collect(),
        kit: server.source == McpServerSource::Kit,
        allowances: server
            .allowances
            .iter()
            .filter_map(|(tool, calls)| {
                Some((tool.as_str().to_string(), u32::try_from(*calls).ok()?))
            })
            .collect(),
    })
}

/// `value` as compact JSON with every object's keys sorted, at every depth. The keys are sorted
/// here, so the form does not depend on whether `serde_json`'s `preserve_order` is on.
#[must_use]
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_unstable_by_key(|(key, _)| *key);
            let members: Vec<String> = entries
                .into_iter()
                .map(|(key, item)| {
                    format!("{}:{}", Value::from(key.as_str()), canonical_json(item))
                })
                .collect();
            format!("{{{}}}", members.join(","))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", items.join(","))
        }
        other => other.to_string(),
    }
}

/// The sha256, in lower-case hex, of the canonical JSON of what a custom connector runs as: its
/// transport, command and args or url and headers, key names and tool labels (ADR 0030), and
/// `"source": "kit"` for a kit's entry alone (ADR 0036). Its name is not in it: the name is where
/// the keys are kept.
#[must_use]
pub fn spec_sha256(server: &CustomServer) -> String {
    let mut definition = match &server.transport {
        CustomTransport::Stdio { command, args } => {
            serde_json::json!({ "transport": "stdio", "command": command, "args": args })
        }
        CustomTransport::Http {
            url,
            headers,
            oauth,
        } => {
            let mut definition =
                serde_json::json!({ "transport": "http", "url": url, "headers": headers });
            if let Some(oauth) = oauth {
                definition["oauth"] = oauth_json(oauth);
            }
            definition
        }
    };
    if server.kit {
        // Only a kit entry says so, so every hash kept for a custom one stands.
        definition["source"] = serde_json::json!("kit");
    }
    definition["credential_keys"] = serde_json::json!(server.credential_keys);
    definition["tools"] = serde_json::json!(server.tools);
    if !server.allowances.is_empty() {
        // Only when there is one, so every hash kept before stands (ADR 0037).
        definition["allowances"] = serde_json::json!(server.allowances);
    }
    sha256_hex(&canonical_json(&definition))
}

/// The sha256 of `text`, in lower-case hex.
pub(crate) fn sha256_hex(text: &str) -> String {
    sha256_bytes_hex(text.as_bytes())
}

/// The sha256 of `bytes`, in lower-case hex.
pub(crate) fn sha256_bytes_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The plan check (`docs/SPEC.md` section 5.3), with what the team left out filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgmentPolicy {
    /// Whether every contract's plan is checked before work starts.
    pub required: JudgmentRequired,
    /// What the judge answers about each plan, in order.
    pub questions: Vec<String>,
    /// Who checks, as the team chose it; `Team::judge` resolves it.
    pub judge: JudgeChoice,
}

/// How Farik opens the project's app for the UI/UX Designer (the founder's D2): `prepare` installs
/// and builds with the network on, `start` serves on `port` with it off, and `path` is the first
/// page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Installs and builds; left out, skipped.
    pub prepare: Option<String>,
    /// Starts the app.
    pub start: String,
    /// The port it serves on.
    pub port: u16,
    /// The page to open first, starting with `/`.
    pub path: String,
}

impl Team {
    /// The skills pinned for the whole team (spec 6.7).
    #[must_use]
    pub fn skills(&self) -> &[SkillPin] {
        &self.skills
    }

    /// The team's preview, if it set one, with `path` defaulted to `/`.
    #[must_use]
    pub fn preview(&self) -> Option<Preview> {
        self.preview.as_ref().map(|wire| Preview {
            prepare: wire.prepare.as_deref().cloned(),
            start: wire.start.to_string(),
            port: u16::try_from(wire.port).unwrap_or(u16::MAX),
            path: wire.path.to_string(),
        })
    }

    /// The first active UI/UX Designer, the one a design review names.
    #[must_use]
    pub fn designer(&self) -> Option<&Agent> {
        self.active_agents()
            .find(|agent| agent.role == RoleWire::UiUxDesigner)
    }

    /// Whether a UI/UX Designer is on the team and not retired: a paused one still holds a UI
    /// change in `verifying`, waiting on it.
    #[must_use]
    pub fn has_designer(&self) -> bool {
        self.agents.iter().any(|agent| {
            agent.role == RoleWire::UiUxDesigner && agent.status != AgentStatus::Retired
        })
    }

    /// The plan check this team runs: its `policy.judgment`, or the schema's defaults where it
    /// left something out.
    #[must_use]
    pub fn judgment(&self) -> JudgmentPolicy {
        let wire = self.policy.judgment.clone().unwrap_or_default();
        JudgmentPolicy {
            required: wire.required,
            questions: wire.questions.into_iter().map(String::from).collect(),
            judge: wire.judge,
        }
    }

    /// The role that checks plans (spec 5.3): the named one, or under `auto` the first active
    /// agent of the Architect, the Scrum Master and the Product Manager (the founder,
    /// 2026-09-29). Every team has an active Product Manager (D18), so `auto` always finds one.
    #[must_use]
    pub fn judge(&self) -> Role {
        match named_judge(self.judgment().judge) {
            Some(role) => role,
            None => [Role::Architect, Role::ScrumMaster]
                .into_iter()
                .find(|role| self.has_active(*role))
                .unwrap_or(Role::ProductManager),
        }
    }

    /// Whether the team plans its work in sprints (ADR 0028): its `policy.plan_in_sprints`, off
    /// when left out, so no team file written before the key changes how it works.
    #[must_use]
    pub fn plans_in_sprints(&self) -> bool {
        self.policy.plan_in_sprints == Some(true)
    }

    /// The team's answers to what its agents may do: its `policy.permissions`, or the defaults.
    #[must_use]
    pub fn permissions(&self) -> TeamPermissions {
        self.policy.permissions.clone().unwrap_or_default()
    }

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
            ui_paths: self
                .rules
                .ui_paths
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
    /// Every permission tier this agent holds: its role's defaults, then the team's answers
    /// (without `execute` for Developers, Designers and Architects when `run_commands` is false,
    /// with `git_remote` for Developers and Designers when `push` is true), widened by what it was granted and
    /// narrowed by what was taken away. The team's answers apply to every agent of the role, so an
    /// agent added later follows them.
    ///
    /// `docs/SPEC.md` section 5.6 says a role's tiers are the user's to override, and an override
    /// that could only widen would leave no way to say that this Developer does not run commands.
    /// Taking away wins over granting, because a tier in both lists is a person's mistake and the
    /// narrower reading of a mistake is the safer one. The order is the role's own first, then what
    /// a grant added, so that a list read back reads as the role plus the exceptions.
    #[must_use]
    pub fn tiers(&self, permissions: &TeamPermissions) -> Vec<PermissionTier> {
        let role = Role::from(self.role);
        let mut tiers = default_tiers(role).to_vec();
        if !permissions.run_commands && (changes_code(role) || role == Role::Architect) {
            tiers.retain(|tier| *tier != PermissionTier::Execute);
        }
        if permissions.push && changes_code(role) {
            tiers.push(PermissionTier::GitRemote);
        }
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
            RoleWire::UiUxDesigner => Self::UiUxDesigner,
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
        AgentStatus, HumanAcceptsContracts, Integration, JudgeChoice, JudgmentPolicy,
        JudgmentRequired, PermissionTier, PermissionTierWire, Preview, Role, RoleWire,
        SMALL_ENOUGH_QUESTION, Team, TeamPermissions, TeamPolicy, changes_code, defaults,
        validate_team,
    };
    use super::{CustomServer, CustomTransport, canonical_json, custom_server, spec_sha256};
    use crate::governor::permissions::ConnectorTag;
    use crate::governor::team_rules::{
        DEFAULT_DOCUMENT_PATHS, DEFAULT_PROTECTED_PATHS, DEFAULT_UI_PATHS, TeamRules,
    };
    use std::collections::BTreeMap;

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
            ada.model
                .as_ref()
                .and_then(|model| model.id.as_deref())
                .map(String::as_str),
            Some("claude-opus-5-5")
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
            std::iter::once(an_agent_wire("ada", "product_manager"))
                .chain((0..7).map(|n| an_agent_wire(&format!("agent-{n}"), "software_developer")))
                .collect(),
        );
        assert_eq!(
            refusals(&eight),
            [(
                "/agents".to_string(),
                "A team has seven agents at most. Retire one before adding another.".to_string()
            )],
            "and seven at most"
        );

        // A retired agent stays in the file so its past work still names someone, and is not one
        // of the seven: replacing an agent a third time still leaves room.
        let mut retired = eight.clone();
        for n in 0..3 {
            retired["agents"][n + 1]["status"] = json!("retired");
        }
        let mut more = retired["agents"].as_array().expect("agents").clone();
        more.extend((7..9).map(|n| an_agent_wire(&format!("agent-{n}"), "software_developer")));
        retired["agents"] = Value::Array(more);
        validate_team(&retired).expect("seven not retired, three retired, is a team");
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
                "A team needs an active Product Manager to write its plans, and this one has none.",
                "A team needs an active Software Developer to do the work, and this one has none.",
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
            messages[1].contains("Software Developer"),
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
        assert!(messages[0].contains("Product Manager"), "{}", messages[0]);
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
    fn defaults_the_ui_paths_when_left_out() {
        // The seven design globs of ADR 0026 fill a key left out; a team's own list replaces them,
        // and an explicit `[]` is kept, which leaves only the contract's `ui_change` to say so.
        assert_eq!(
            team(&a_team_wire()).rules().ui_paths,
            [
                "**/*.tsx",
                "**/*.jsx",
                "**/*.vue",
                "**/*.svelte",
                "**/*.css",
                "**/*.scss",
                "**/*.html"
            ]
        );
        assert_eq!(
            TeamRules::default().ui_paths,
            DEFAULT_UI_PATHS.map(str::to_string)
        );
        let mut wire = a_team_wire();
        wire["rules"]["ui_paths"] = json!(["app/**"]);
        assert_eq!(team(&wire).rules().ui_paths, ["app/**"]);
        wire["rules"]["ui_paths"] = json!([]);
        assert!(team(&wire).rules().ui_paths.is_empty());
    }

    #[test]
    fn validates_the_preview_and_the_connectors() {
        // Every existing fixture still validates, the full one with a preview and a connector.
        assert!(team(&a_team_wire()).preview().is_none());
        let full = team(&a_full_team_wire());
        assert_eq!(
            full.preview(),
            Some(Preview {
                prepare: Some("pnpm install --frozen-lockfile".to_string()),
                start: "pnpm dev --port 4400".to_string(),
                port: 4400,
                path: "/app".to_string(),
            })
        );

        let mut wire = a_team_wire();
        wire["preview"] = json!({ "start": "npm start", "port": 3000 });
        assert_eq!(
            team(&wire).preview(),
            Some(Preview {
                prepare: None,
                start: "npm start".to_string(),
                port: 3000,
                path: "/".to_string(),
            }),
            "prepare left out is skipped, and path defaults to /"
        );
        let long = "x".repeat(501);
        for (field, value) in [
            ("prepare", json!("")),
            ("prepare", json!(long)),
            ("start", json!("")),
            ("start", json!(long)),
            ("port", json!(1023)),
            ("port", json!(65536)),
            ("path", json!("app")),
        ] {
            let mut wire = a_team_wire();
            wire["preview"] = json!({ "start": "npm start", "port": 3000 });
            wire["preview"][field] = value;
            assert_eq!(paths(&wire), [format!("/preview/{field}")], "{field}");
        }
        for (field, value) in [
            ("prepare", json!("x".repeat(500))),
            ("start", json!("x".repeat(500))),
            ("port", json!(1024)),
            ("port", json!(65535)),
        ] {
            let mut wire = a_team_wire();
            wire["preview"] = json!({ "start": "npm start", "port": 3000 });
            wire["preview"][field] = value;
            assert!(validate_team(&wire).is_ok(), "{field}");
        }
        let mut wire = a_team_wire();
        wire["preview"] = json!({ "port": 3000 });
        assert_eq!(paths(&wire), ["/preview"], "start is required");

        // Any agent may have a connector, and every agent's list is checked (F8).
        let mut wire = a_team_wire();
        wire["agents"][0]["mcp_servers"] = json!([{ "name": "playwright", "source": "builtin" }]);
        wire["agents"][1]["mcp_servers"] = json!([
            { "name": "playwright", "source": "builtin" },
            { "name": "selenium", "source": "builtin" }
        ]);
        assert_eq!(
            refusals(&wire),
            [(
                "/agents/1/mcp_servers/1/name".to_string(),
                "unknown_connector: selenium is not a connector Farik ships; the one it ships is playwright."
                    .to_string()
            )]
        );
        wire["agents"][1]["mcp_servers"][1] = json!({ "name": "playwright", "source": "npm" });
        assert_eq!(paths(&wire), ["/agents/1/mcp_servers/1/source"]);

        // The Designer: the first active one, and whether any is not retired.
        let mut wire = a_team_wire();
        assert!(team(&wire).designer().is_none());
        assert!(!team(&wire).has_designer());
        let mut paused = an_agent_wire("iris", "ui_ux_designer");
        paused["status"] = json!("paused");
        wire["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .extend([paused, an_agent_wire("vera", "ui_ux_designer")]);
        let designers = team(&wire);
        assert_eq!(
            designers.designer().map(|agent| agent.id.as_str()),
            Some("vera")
        );
        assert!(designers.has_designer());
        wire["agents"][3]["status"] = json!("retired");
        assert!(team(&wire).designer().is_none());
        assert!(
            team(&wire).has_designer(),
            "a paused Designer is still the team's"
        );
        wire["agents"][2]["status"] = json!("retired");
        assert!(!team(&wire).has_designer());
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
    fn defaults_the_ambient_allowance() {
        // 5.9: each agent has one unprompted message per sprint unless the team says otherwise.
        assert_eq!(team(&a_team_wire()).policy.ambient_messages_per_sprint, 1);
    }

    #[test]
    fn defaults_the_escalation_age() {
        // 5.7: an open escalation waits a day on the human before the aged rule ages it, unless
        // the team says otherwise.
        assert_eq!(team(&a_team_wire()).policy.escalation_age_hours.get(), 24);
    }

    #[test]
    fn defaults_the_memory_cap() {
        // 5.8: an agent's notebook holds 8,000 tokens unless the team says otherwise.
        assert_eq!(team(&a_team_wire()).policy.memory_cap_tokens, 8000);
    }

    #[test]
    fn refuses_a_number_outside_what_a_rule_allows() {
        // Every bound here carries a spec number: 5.2's work-in-progress limit, 5.7's blocked age,
        // iteration count and escalation age, 5.8's notebook cap, 5.5's daily budget, 5.12's task
        // cap and its list of methods. A bound nothing tests is a bound the next person deletes to
        // make something else compile.
        for (pointer, value) in [
            ("/policy/wip_limit_per_agent", json!(-1)),
            ("/policy/wip_limit_per_agent", json!(101)),
            ("/policy/blocked_limit_hours", json!(0)),
            ("/policy/blocked_limit_hours", json!(721)),
            ("/policy/max_iterations", json!(0)),
            ("/policy/max_iterations", json!(101)),
            ("/policy/escalation_age_hours", json!(0)),
            ("/policy/escalation_age_hours", json!(721)),
            ("/policy/memory_cap_tokens", json!(499)),
            ("/policy/memory_cap_tokens", json!(16001)),
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
    fn reads_an_absent_policy_as_off() {
        let mut wire = a_team_wire();
        assert!(!team(&wire).plans_in_sprints());
        wire["policy"]["plan_in_sprints"] = json!(true);
        assert!(team(&wire).plans_in_sprints());
        wire["policy"]["plan_in_sprints"] = json!("yes");
        assert_eq!(paths(&wire), ["/policy/plan_in_sprints"]);
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
            team.agents[0].tiers(&team.permissions()),
            [PermissionTier::Read, PermissionTier::Execute]
        );
        assert_eq!(
            team.agents[1].tiers(&team.permissions()),
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
            team.agents[1].tiers(&team.permissions()),
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
            "linus must keep reading the project: every agent reads, and one that cannot is \
             refused everything it tries. Pause linus instead."
        );
    }

    #[test]
    fn resolves_the_judge_in_the_founders_order() {
        // The founder, 2026-09-29: the Architect, else the Scrum Master, else the Product Manager,
        // who checks its own plans only when the team has neither.
        let mut wire = a_team_wire();
        wire["policy"]["judgment"] = json!({ "required": "always" });
        assert_eq!(team(&wire).judge(), Role::ProductManager);

        wire["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .push(an_agent_wire("sol", "scrum_master"));
        assert_eq!(team(&wire).judge(), Role::ScrumMaster);

        wire["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .push(an_agent_wire("kai", "architect"));
        assert_eq!(team(&wire).judge(), Role::Architect);

        wire["policy"]["judgment"]["judge"] = json!("scrum_master");
        assert_eq!(
            team(&wire).judge(),
            Role::ScrumMaster,
            "a named judge is the judge"
        );

        wire["policy"]["judgment"]["judge"] = json!("auto");
        wire["agents"][3]["status"] = json!("paused");
        assert_eq!(
            team(&wire).judge(),
            Role::ScrumMaster,
            "a paused Architect checks nothing"
        );
    }

    #[test]
    fn refuses_a_named_judge_no_active_agent_holds() {
        let mut wire = a_team_wire();
        wire["policy"]["judgment"] = json!({ "judge": "architect" });
        assert_eq!(
            refusals(&wire),
            [(
                "/policy/judgment/judge".to_string(),
                "No active Architect can check plans. Let Farik choose, or add one.".to_string()
            )]
        );

        wire["policy"]["judgment"]["judge"] = json!("scrum_master");
        let mut sol = an_agent_wire("sol", "scrum_master");
        sol["status"] = json!("paused");
        wire["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .push(sol);
        assert_eq!(
            refusals(&wire),
            [(
                "/policy/judgment/judge".to_string(),
                "No active Scrum Master can check plans. Let Farik choose, or add one.".to_string()
            )],
            "a paused one is not active"
        );

        wire["policy"]["judgment"]["judge"] = json!("product_manager");
        assert_eq!(
            paths(&wire),
            ["/policy/judgment/judge"],
            "the Product Manager is reached only through auto"
        );
    }

    #[test]
    fn refuses_checking_with_no_questions() {
        let mut wire = a_team_wire();
        wire["policy"]["judgment"] = json!({ "required": "always", "questions": [] });
        assert_eq!(
            refusals(&wire),
            [(
                "/policy/judgment/questions".to_string(),
                "Checking plans needs at least one question.".to_string()
            )]
        );

        wire["policy"]["judgment"]["required"] = json!("never");
        assert!(
            validate_team(&wire).is_ok(),
            "a team that checks no plans needs no questions"
        );

        for questions in [
            json!(["Too short"]),
            json!(["a".repeat(201)]),
            json!(vec!["Does the task fit its budget?"; 6]),
        ] {
            wire["policy"]["judgment"]["questions"] = questions.clone();
            let paths = paths(&wire);
            assert!(
                !paths.is_empty()
                    && paths
                        .iter()
                        .all(|path| path.starts_with("/policy/judgment/questions")),
                "{questions}: {paths:?}"
            );
        }
    }

    #[test]
    fn accepts_every_existing_team() {
        // A team.yaml written before the policy existed says nothing about it, and gets the
        // defaults: every plan is checked, by the two questions of 5.3, and the base fixture's
        // Product Manager checks them.
        let mut wire = a_team_wire();
        wire["policy"]
            .as_object_mut()
            .expect("the fixture's policy is an object")
            .remove("judgment");
        let base = team(&wire);
        assert_eq!(
            base.judgment(),
            JudgmentPolicy {
                required: JudgmentRequired::Always,
                questions: vec![
                    "Does the task fit its budget?".to_string(),
                    "Would its checks notice if the work went wrong the way its intent worries \
                     about?"
                        .to_string(),
                ],
                judge: JudgeChoice::Auto,
            }
        );
        assert_eq!(base.judge(), Role::ProductManager);
        assert_eq!(
            base.permissions(),
            TeamPermissions {
                run_commands: true,
                push: false
            }
        );

        // `farik init`'s starter team, as it was written before the defaults moved to core.
        let starter = team(&json!({
            "name": "notes",
            "agents": [
                an_agent_wire("product-manager", "product_manager"),
                an_agent_wire("developer", "software_developer"),
            ],
            "budgets": {},
            "policy": {
                "human_accepts_contracts": "high_risk",
                "wip_limit_per_agent": 1,
                "blocked_limit_hours": 24,
                "max_iterations": 3,
                "integration": "auto_merge"
            },
            "rules": {}
        }));
        let defaults = defaults();
        assert_eq!(defaults.budgets, starter.budgets, "no daily_usd (ADR 0015)");
        assert_eq!(
            defaults.policy,
            TeamPolicy {
                judgment: Some(base.policy.judgment.clone().unwrap_or_default()),
                permissions: Some(TeamPermissions::default()),
                plan_in_sprints: Some(true),
                ..starter.policy
            },
            "the starter values, with the plan check, the permissions and sprint planning \
             written out"
        );
        assert_eq!(defaults.policy.ambient_messages_per_sprint, 1);
        assert_eq!(defaults.policy.escalation_age_hours.get(), 24);
        assert_eq!(defaults.policy.memory_cap_tokens, 8000);
        assert_eq!(
            SMALL_ENOUGH_QUESTION,
            "Is it small enough to finish in one go?"
        );
    }

    #[test]
    fn applies_the_permission_answers_to_every_agent_of_the_role() {
        // Answered once for the team, so a Developer added later follows the answers too.
        let mut wire = a_team_wire();
        wire["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .extend([
                an_agent_wire("kai", "architect"),
                an_agent_wire("theo", "software_developer"),
            ]);
        let before = team(&wire);
        let developer = before.agents[3].tiers(&before.permissions());
        assert!(developer.contains(&PermissionTier::Execute));
        assert!(!developer.contains(&PermissionTier::GitRemote));

        wire["policy"]["permissions"] = json!({ "run_commands": false, "push": true });
        let after = team(&wire);
        let permissions = after.permissions();
        let tiers = |index: usize| after.agents[index].tiers(&permissions);
        for developer in [1, 3] {
            assert_eq!(
                tiers(developer),
                [
                    PermissionTier::Read,
                    PermissionTier::WriteWorkspace,
                    PermissionTier::GitLocal,
                    PermissionTier::GitRemote,
                ]
            );
        }
        assert_eq!(
            tiers(2),
            [
                PermissionTier::Read,
                PermissionTier::WriteWorkspace,
                PermissionTier::Network,
                PermissionTier::GitLocal,
            ],
            "the Architect runs no commands either, and pushes nothing"
        );
        assert_eq!(
            tiers(0),
            [PermissionTier::Read, PermissionTier::Network],
            "the Product Manager neither runs commands nor pushes"
        );

        wire["agents"][3]["grants"] = json!(["execute"]);
        assert!(
            team(&wire).agents[3]
                .tiers(&permissions)
                .contains(&PermissionTier::Execute),
            "an agent's own grant still widens what the team said"
        );
    }

    #[test]
    fn changes_code_for_the_developer_and_the_designer_only() {
        assert!(changes_code(Role::SoftwareDeveloper));
        assert!(changes_code(Role::UiUxDesigner));
        for role in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::MarketingSpecialist,
            Role::Human,
        ] {
            assert!(!changes_code(role), "{role}");
        }
    }

    #[test]
    fn gives_the_designer_the_developers_tiers_and_permission_answers() {
        let mut wire = a_team_wire();
        wire["agents"]
            .as_array_mut()
            .expect("the fixture's agents are a list")
            .push(an_agent_wire("iris", "ui_ux_designer"));
        let before = team(&wire);
        let full = [
            PermissionTier::Read,
            PermissionTier::WriteWorkspace,
            PermissionTier::Execute,
            PermissionTier::GitLocal,
        ];
        for index in [1, 2] {
            assert_eq!(before.agents[index].tiers(&before.permissions()), full);
        }

        wire["policy"]["permissions"] = json!({ "run_commands": false, "push": true });
        let after = team(&wire);
        let answered = [
            PermissionTier::Read,
            PermissionTier::WriteWorkspace,
            PermissionTier::GitLocal,
            PermissionTier::GitRemote,
        ];
        for index in [1, 2] {
            assert_eq!(
                after.agents[index].tiers(&after.permissions()),
                answered,
                "a Designer as a Developer"
            );
        }
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
                RoleWire::UiUxDesigner,
            ]
            .map(Role::from),
            [
                Role::ProductManager,
                Role::ScrumMaster,
                Role::Architect,
                Role::SoftwareDeveloper,
                Role::MarketingSpecialist,
                Role::UiUxDesigner,
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

    /// A team whose first agent has `servers` as its connectors.
    fn with_servers(servers: Value) -> Value {
        let mut wire = a_team_wire();
        wire["agents"][0]["mcp_servers"] = servers;
        wire
    }

    fn a_stdio_server() -> Value {
        json!({
            "name": "github",
            "source": "custom",
            "transport": "stdio",
            "command": "npx",
            "args": ["-y", "@example/github-mcp"],
            "credential_keys": ["GITHUB_TOKEN"],
            "tools": {
                "search": "network",
                "create_issue": "external_effect",
                "delete_repo": "denied"
            }
        })
    }

    fn an_http_server() -> Value {
        json!({
            "name": "linear",
            "source": "custom",
            "transport": "http",
            "url": "https://mcp.example.com/mcp",
            "headers": { "Authorization": "Bearer {API_KEY}" },
            "credential_keys": ["API_KEY"],
            "tools": { "list_issues": "network" }
        })
    }

    fn the_custom_servers(wire: &Value) -> Vec<Option<CustomServer>> {
        team(wire).agents[0]
            .mcp_servers
            .iter()
            .flatten()
            .map(custom_server)
            .collect()
    }

    #[test]
    fn accepts_a_custom_stdio_and_a_custom_http_server() {
        let wire = with_servers(json!([
            a_stdio_server(),
            an_http_server(),
            { "name": "playwright", "source": "builtin" }
        ]));
        assert_eq!(
            the_custom_servers(&wire),
            [
                Some(CustomServer {
                    name: "github".to_string(),
                    transport: CustomTransport::Stdio {
                        command: "npx".to_string(),
                        args: vec!["-y".to_string(), "@example/github-mcp".to_string()],
                    },
                    credential_keys: vec!["GITHUB_TOKEN".to_string()],
                    tools: BTreeMap::from([
                        ("create_issue".to_string(), ConnectorTag::ExternalEffect),
                        ("delete_repo".to_string(), ConnectorTag::Denied),
                        ("search".to_string(), ConnectorTag::Network),
                    ]),
                    kit: false,
                    allowances: BTreeMap::new(),
                }),
                Some(CustomServer {
                    name: "linear".to_string(),
                    transport: CustomTransport::Http {
                        url: "https://mcp.example.com/mcp".to_string(),
                        headers: BTreeMap::from([(
                            "Authorization".to_string(),
                            "Bearer {API_KEY}".to_string()
                        )]),
                        oauth: None,
                    },
                    credential_keys: vec!["API_KEY".to_string()],
                    tools: BTreeMap::from([("list_issues".to_string(), ConnectorTag::Network)]),
                    kit: false,
                    allowances: BTreeMap::new(),
                }),
                None,
            ]
        );
        let mut bare = a_stdio_server();
        for field in ["args", "credential_keys", "tools"] {
            bare.as_object_mut().expect("an object").remove(field);
        }
        assert_eq!(
            the_custom_servers(&with_servers(json!([bare]))),
            [Some(CustomServer {
                name: "github".to_string(),
                transport: CustomTransport::Stdio {
                    command: "npx".to_string(),
                    args: Vec::new(),
                },
                credential_keys: Vec::new(),
                tools: BTreeMap::new(),
                kit: false,
                allowances: BTreeMap::new(),
            })],
            "a server with no arguments, keys or tools is one"
        );
    }

    #[test]
    fn refuses_a_custom_server_named_farik_or_playwright() {
        for name in ["farik", "playwright"] {
            let mut server = a_stdio_server();
            server["name"] = json!(name);
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{name}: {refused:?}");
            assert_eq!(refused[0].0, "/agents/0/mcp_servers/0/name", "{name}");
            assert!(
                refused[0].1.starts_with("connector_name_reserved: "),
                "{name}: {}",
                refused[0].1
            );
        }
    }

    #[test]
    fn refuses_a_server_name_with_an_underscore() {
        let mut server = a_stdio_server();
        server["name"] = json!("my_server");
        assert_eq!(
            paths(&with_servers(json!([server]))),
            ["/agents/0/mcp_servers/0/name"]
        );
        for name in ["My-server", "1server", "-server", &"x".repeat(41)] {
            let mut server = a_stdio_server();
            server["name"] = json!(name);
            assert_eq!(
                paths(&with_servers(json!([server]))),
                ["/agents/0/mcp_servers/0/name"],
                "{name}"
            );
        }
        let mut server = a_stdio_server();
        server["name"] = json!(format!("my-server-2{}", "x".repeat(29)));
        assert!(validate_team(&with_servers(json!([server]))).is_ok());
    }

    #[test]
    fn refuses_one_name_twice() {
        let mut again = a_stdio_server();
        again["command"] = json!("github-mcp");
        let refused = refusals(&with_servers(json!([
            a_stdio_server(),
            an_http_server(),
            again
        ])));
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].0, "/agents/0/mcp_servers/2/name");
        assert!(
            refused[0].1.starts_with("connector_name_twice: "),
            "{}",
            refused[0].1
        );
        let mut elsewhere = with_servers(json!([a_stdio_server()]));
        elsewhere["agents"][1]["mcp_servers"] = json!([a_stdio_server()]);
        assert!(
            validate_team(&elsewhere).is_ok(),
            "two agents may each have their own github"
        );
    }

    #[test]
    fn refuses_url_on_stdio_and_command_on_http() {
        let at = |field: &str| format!("/agents/0/mcp_servers/0/{field}");
        for (field, value) in [
            ("url", json!("https://mcp.example.com/mcp")),
            ("headers", json!({ "X-Team": "farik" })),
        ] {
            let mut server = a_stdio_server();
            server[field] = value;
            assert_eq!(
                paths(&with_servers(json!([server]))),
                [at(field)],
                "{field}"
            );
        }
        for (field, value) in [("command", json!("npx")), ("args", json!(["-y"]))] {
            let mut server = an_http_server();
            server[field] = value;
            assert_eq!(
                paths(&with_servers(json!([server]))),
                [at(field)],
                "{field}"
            );
        }
        for (mut server, field) in [(a_stdio_server(), "command"), (an_http_server(), "url")] {
            server.as_object_mut().expect("an object").remove(field);
            assert_eq!(
                paths(&with_servers(json!([server]))),
                [at(field)],
                "{field} is required"
            );
        }
        let mut server = a_stdio_server();
        server
            .as_object_mut()
            .expect("an object")
            .remove("transport");
        assert_eq!(
            paths(&with_servers(json!([server]))),
            [at("transport")],
            "a custom server says how it is reached"
        );
        for (field, value) in [
            ("transport", json!("stdio")),
            ("command", json!("npx")),
            ("credential_keys", json!(["API_KEY"])),
            ("tools", json!({ "browser_click": "network" })),
        ] {
            let server = json!({ "name": "playwright", "source": "builtin", field: value });
            assert_eq!(
                paths(&with_servers(json!([server]))),
                [at(field)],
                "a built-in connector has no {field}"
            );
        }
        let mut server = an_http_server();
        server["transport"] = json!("sse");
        assert_eq!(paths(&with_servers(json!([server]))), [at("transport")]);
    }

    #[test]
    fn refuses_a_url_holding_a_secret() {
        for url in [
            "https://u:p@x.example/mcp",
            "https://x.example/mcp?key=v",
            "https://token@x.example/mcp",
        ] {
            let mut server = an_http_server();
            server["url"] = json!(url);
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{url}: {refused:?}");
            assert_eq!(refused[0].0, "/agents/0/mcp_servers/0/url", "{url}");
            assert!(
                refused[0].1.starts_with("url_holds_secret: "),
                "{url}: {}",
                refused[0].1
            );
        }
        let mut server = an_http_server();
        server["url"] = json!("https://x.example/team@farik/mcp");
        assert!(
            validate_team(&with_servers(json!([server]))).is_ok(),
            "an @ in the path is not userinfo"
        );
        let mut server = an_http_server();
        server["url"] = json!("ftp://x.example/mcp");
        assert_eq!(
            paths(&with_servers(json!([server]))),
            ["/agents/0/mcp_servers/0/url"]
        );
    }

    #[test]
    fn refuses_a_relative_command_with_a_folder() {
        // A command such as `./server` or `bin/mcp` resolves in the folder the server runs in, not
        // through PATH; it is no program the user picked by name (finding C1).
        for command in ["./server", "bin/mcp", "../x/mcp"] {
            let mut server = a_stdio_server();
            server["command"] = json!(command);
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{command}: {refused:?}");
            assert_eq!(refused[0].0, "/agents/0/mcp_servers/0/command", "{command}");
            assert!(
                refused[0].1.starts_with("command_not_absolute: "),
                "{command}: {}",
                refused[0].1
            );
        }
        for command in ["npx", "/usr/local/bin/github-mcp"] {
            let mut server = a_stdio_server();
            server["command"] = json!(command);
            assert!(
                validate_team(&with_servers(json!([server]))).is_ok(),
                "{command}: a bare name goes through PATH, and a full path is the user's"
            );
        }
    }

    #[test]
    fn refuses_a_header_holding_a_secret_itself() {
        // The team file is committed: a key typed in place of `{API_KEY}` would be shared.
        for (header, template) in [
            ("Authorization", "Bearer ghp_abc123"),
            ("X-Api-Key", "abc123"),
            ("x-auth-token", "t"),
            ("Client-Secret", "s"),
            ("Private-Token", "t"),
        ] {
            let mut server = an_http_server();
            server["headers"] = json!({ header: template });
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{header}: {refused:?}");
            assert_eq!(
                refused[0].0,
                format!("/agents/0/mcp_servers/0/headers/{header}"),
                "{header}"
            );
            assert!(
                refused[0].1.starts_with("header_holds_secret: "),
                "{header}: {}",
                refused[0].1
            );
        }
        let mut server = an_http_server();
        server["headers"] = json!({
            "Authorization": "Bearer {API_KEY}", "X-Team": "farik", "Accept": "text/plain"
        });
        assert!(validate_team(&with_servers(json!([server]))).is_ok());
    }

    #[test]
    fn refuses_a_secret_beside_a_placeholder_or_in_a_cookie() {
        // A placeholder does not make a header safe: what is written beside it is shared too
        // (re-review N4).
        for (header, template) in [
            ("Authorization", "Bearer ghp_x {API_KEY}"),
            ("Authorization", "{API_KEY} ghp_x"),
            ("Cookie", "session=abc123"),
            ("Cookie", "theme=dark; session=abc123"),
            ("Cookie", "session={API_KEY}; csrf=abc123"),
        ] {
            let mut server = an_http_server();
            server["headers"] = json!({ header: template });
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{template}: {refused:?}");
            assert_eq!(
                refused[0].0,
                format!("/agents/0/mcp_servers/0/headers/{header}"),
                "{template}"
            );
            assert!(
                refused[0].1.starts_with("header_holds_secret: "),
                "{template}: {}",
                refused[0].1
            );
        }
        let mut server = an_http_server();
        server["headers"] = json!({
            "Authorization": "Bearer {API_KEY}",
            "Cookie": "session={API_KEY}; csrf={API_KEY}",
            "X-Api-Key": "{API_KEY}",
        });
        assert!(validate_team(&with_servers(json!([server]))).is_ok());
    }

    #[test]
    fn refuses_a_secret_in_a_commands_arguments() {
        // `npx srv --api-key sk-…` would be written to the committed team file (re-review N4).
        for (args, at) in [
            (json!(["srv", "--api-key", "abc123"]), 2),
            (json!(["srv", "--api-key=abc123"]), 1),
            (json!(["--token", "t0k3n", "srv"]), 1),
            (json!(["srv", "--password", "hunter2"]), 2),
            (json!(["srv", "--client-secret=s"]), 1),
            (json!(["srv", "sk-proj-abc123"]), 1),
            (json!(["srv", "ghp_abc123"]), 1),
        ] {
            let mut server = a_stdio_server();
            server["args"] = args.clone();
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{args}: {refused:?}");
            assert_eq!(
                refused[0].0,
                format!("/agents/0/mcp_servers/0/args/{at}"),
                "{args}"
            );
            assert!(
                refused[0].1.starts_with("arg_holds_secret: "),
                "{args}: {}",
                refused[0].1
            );
            // The refusal reaches the screen and the log: it never quotes the value.
            for value in ["abc123", "t0k3n", "hunter2", "sk-proj", "ghp_"] {
                assert!(!refused[0].1.contains(value), "{}", refused[0].1);
            }
        }
        for args in [
            json!(["-y", "@example/github-mcp"]),
            json!(["srv", "--keyboard", "us", "--token-file", "/home/u/t"]),
            json!(["srv", "--api-key", "--verbose"]),
        ] {
            let mut server = a_stdio_server();
            server["args"] = args.clone();
            assert!(
                validate_team(&with_servers(json!([server]))).is_ok(),
                "{args}"
            );
        }
    }

    #[test]
    fn tells_a_key_from_a_setting_that_only_looks_like_one() {
        // Re-review 2 m3: each is (args, headers, the field refused, or None when it passes).
        let header = |name: &str, template: &str| json!({ name: template });
        for (args, headers, refused_at) in [
            (
                json!(["srv", "--key", "/etc/tls/server.key"]),
                json!({}),
                None,
            ),
            (json!(["srv", "--sort-key", "name"]), json!({}), None),
            (json!(["srv", "--primary-key", "id"]), json!({}), None),
            (json!(["srv", "--max-token", "4096"]), json!({}), None),
            (json!(["srv", "--monkey", "x"]), json!({}), None),
            (json!([]), header("Authorization", "OAuth2 {API_KEY}"), None),
            (
                json!(["srv", "--header", "Authorization: Bearer abc"]),
                json!({}),
                Some("args/2"),
            ),
            (
                json!(["srv", "--header=Authorization: Bearer abc"]),
                json!({}),
                Some("args/1"),
            ),
            (json!(["srv", "sk_live_abc123"]), json!({}), Some("args/1")),
            (
                json!(["srv", "AKIAIOSFODNN7EXAMPLE"]),
                json!({}),
                Some("args/1"),
            ),
            (json!(["srv", "--auth", "abc"]), json!({}), Some("args/2")),
            (
                json!(["srv", "--credentials", "abc"]),
                json!({}),
                Some("args/2"),
            ),
            (
                json!(["srv", "--github-pat", "abc"]),
                json!({}),
                Some("args/2"),
            ),
            (json!(["srv", "API_KEY=abc"]), json!({}), Some("args/1")),
            (
                json!(["srv", "--api-key", "-abc"]),
                json!({}),
                Some("args/2"),
            ),
            (
                json!([]),
                header("X-Password", "hunter2"),
                Some("headers/X-Password"),
            ),
            (
                json!([]),
                header("X-Pass", "hunter2"),
                Some("headers/X-Pass"),
            ),
            // Re-review 2 m4 (N4b): a value after the scheme is a value, letters or not.
            (
                json!([]),
                header("Authorization", "Bearer abcdef {API_KEY}"),
                Some("headers/Authorization"),
            ),
        ] {
            let mut server = if headers == json!({}) {
                a_stdio_server()
            } else {
                an_http_server()
            };
            if headers == json!({}) {
                server["args"] = args.clone();
            } else {
                server["headers"] = headers.clone();
            }
            let at: Vec<String> = refused_at
                .map(|field| format!("/agents/0/mcp_servers/0/{field}"))
                .into_iter()
                .collect();
            let refused: Vec<String> = validate_team(&with_servers(json!([server])))
                .err()
                .unwrap_or_default()
                .into_iter()
                .map(|error| error.path)
                .collect();
            assert_eq!(refused, at, "{args} {headers}");
        }
    }

    #[test]
    fn refuses_a_header_naming_an_undeclared_key() {
        for template in ["Bearer {TOKEN}", "Bearer {API_KEY", "{API_KEY} {}"] {
            let mut server = an_http_server();
            server["headers"] = json!({ "Authorization": template });
            let refused = refusals(&with_servers(json!([server])));
            assert_eq!(refused.len(), 1, "{template}: {refused:?}");
            assert_eq!(
                refused[0].0, "/agents/0/mcp_servers/0/headers/Authorization",
                "{template}"
            );
            assert!(
                refused[0].1.starts_with("header_key_unknown: "),
                "{template}: {}",
                refused[0].1
            );
        }
        let mut server = an_http_server();
        server["headers"] = json!({ "Authorization": "Bearer {API_KEY}", "X-Team": "farik" });
        assert!(validate_team(&with_servers(json!([server]))).is_ok());
        let mut server = an_http_server();
        server["credential_keys"] = json!(["api_key"]);
        server["headers"] = json!({});
        assert_eq!(
            paths(&with_servers(json!([server]))),
            ["/agents/0/mcp_servers/0/credential_keys/0"]
        );
    }

    #[test]
    fn refuses_a_tool_tagged_read() {
        let mut server = a_stdio_server();
        server["tools"]["x"] = json!("read");
        assert_eq!(
            paths(&with_servers(json!([server]))),
            ["/agents/0/mcp_servers/0/tools/x"]
        );
    }

    #[test]
    fn keeps_builtin_entries_as_they_were() {
        let full = a_full_team_wire();
        assert!(validate_team(&full).is_ok());
        let servers: Vec<&Value> = full["agents"]
            .as_array()
            .expect("the fixture's agents are a list")
            .iter()
            .filter_map(|agent| agent.get("mcp_servers"))
            .collect();
        assert_eq!(
            servers,
            [&json!([{ "name": "playwright", "source": "builtin" }])]
        );
    }

    #[test]
    fn keeps_the_source_error_at_its_field() {
        let mut wire = a_team_wire();
        wire["agents"][1]["mcp_servers"] = json!([
            { "name": "playwright", "source": "builtin" },
            { "name": "playwright", "source": "npm" }
        ]);
        assert_eq!(paths(&wire), ["/agents/1/mcp_servers/1/source"]);
    }

    #[test]
    fn canonical_json_sorts_keys_whatever_order_a_map_keeps() {
        let mut inner = serde_json::Map::new();
        inner.insert("z".to_string(), json!(1));
        inner.insert("y".to_string(), json!(2));
        let mut outer = serde_json::Map::new();
        outer.insert("b".to_string(), Value::Array(vec![Value::Object(inner)]));
        outer.insert("a\"".to_string(), json!(null));
        assert_eq!(
            canonical_json(&Value::Object(outer)),
            r#"{"a\"":null,"b":[{"y":2,"z":1}]}"#
        );
    }

    #[test]
    fn spec_hash_ignores_key_order_and_sees_every_field() {
        let ordered: Value = serde_json::from_str(r#"{"a":1,"b":2}"#).expect("json");
        let reversed: Value = serde_json::from_str(r#"{"b":2,"a":1}"#).expect("json");
        assert_eq!(canonical_json(&ordered), canonical_json(&reversed));
        let nested: Value =
            serde_json::from_str(r#"{ "b": { "d": [1, {"f": 2, "e": 3}], "c": null }, "a": "x" }"#)
                .expect("json");
        assert_eq!(
            canonical_json(&nested),
            r#"{"a":"x","b":{"c":null,"d":[1,{"e":3,"f":2}]}}"#
        );

        let server = |wire: Value| {
            custom_server(&serde_json::from_value::<super::McpServerWire>(wire).expect("a server"))
                .expect("a custom server")
        };
        let stdio = server(a_stdio_server());
        let http = server(an_http_server());
        let hash = spec_sha256(&stdio);
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(spec_sha256(&server(a_stdio_server())), hash, "stable");
        let mut renamed = a_stdio_server();
        renamed["name"] = json!("github-two");
        assert_eq!(
            spec_sha256(&server(renamed)),
            hash,
            "the name is the account's, not the definition's"
        );
        let changes: [(Value, &str); 7] = [
            (a_stdio_server(), "/command"),
            (a_stdio_server(), "/args/1"),
            (a_stdio_server(), "/credential_keys/0"),
            (a_stdio_server(), "/tools/delete_repo"),
            (an_http_server(), "/url"),
            (an_http_server(), "/headers/Authorization"),
            (an_http_server(), "/tools/list_issues"),
        ];
        for (mut wire, field) in changes {
            let before = spec_sha256(&server(wire.clone()));
            let value = wire.pointer_mut(field).expect("the field is there");
            *value = match field {
                "/tools/delete_repo" | "/tools/list_issues" => json!("external_effect"),
                "/credential_keys/0" => json!("OTHER_KEY"),
                _ => json!(format!("{}x", value.as_str().expect("a string"))),
            };
            assert_ne!(spec_sha256(&server(wire)), before, "{field}");
        }
        assert_ne!(spec_sha256(&http), hash);
    }

    fn oauth_server(oauth: Value) -> Value {
        let mut server = an_http_server();
        server["headers"] = json!({});
        server["credential_keys"] = json!([]);
        server["oauth"] = oauth;
        server
    }

    #[test]
    fn accepts_an_http_server_that_signs_in() {
        for oauth in [
            json!({}),
            json!({ "client_id": "abc", "callback_port": 33418, "scopes": ["read"] }),
        ] {
            let wire = with_servers(json!([oauth_server(oauth.clone())]));
            let read = team(&wire);
            let back = serde_json::to_value(&read).expect("a team serialises");
            assert_eq!(back["agents"][0]["mcp_servers"][0]["oauth"], oauth);
            let server = the_custom_servers(&wire)
                .remove(0)
                .expect("a custom server");
            let CustomTransport::Http {
                oauth: settings, ..
            } = server.transport
            else {
                panic!("an http server");
            };
            let settings = settings.expect("signs in");
            assert_eq!(settings.client_id.as_deref(), oauth["client_id"].as_str());
            assert_eq!(
                settings.callback_port,
                oauth["callback_port"]
                    .as_u64()
                    .and_then(|port| u16::try_from(port).ok())
            );
            assert_eq!(
                settings.scopes.len(),
                oauth["scopes"].as_array().map_or(0, Vec::len)
            );
        }
        let plain = the_custom_servers(&with_servers(json!([an_http_server()]))).remove(0);
        let CustomTransport::Http { oauth, .. } = plain.expect("custom").transport else {
            panic!("an http server");
        };
        assert_eq!(oauth, None);
    }

    #[test]
    fn refuses_oauth_where_it_cannot_be() {
        let at = |field: &str| format!("/agents/0/mcp_servers/0/{field}");
        let mut stdio = a_stdio_server();
        stdio["oauth"] = json!({});
        assert!(paths(&with_servers(json!([stdio]))).contains(&at("oauth")));
        let mut keys = oauth_server(json!({}));
        keys["credential_keys"] = json!(["API_KEY"]);
        assert!(paths(&with_servers(json!([keys]))).contains(&at("credential_keys")));
        for header in ["Authorization", "authorization"] {
            let mut conflict = oauth_server(json!({}));
            conflict["headers"] = json!({ header: "Bearer x" });
            let found = refusals(&with_servers(json!([conflict])));
            assert!(
                found
                    .iter()
                    .any(|(path, message)| *path == at(&format!("headers/{header}"))
                        && message.starts_with("oauth_header_conflict: ")),
                "{header}: {found:?}"
            );
        }
        let port = oauth_server(json!({ "callback_port": 33418 }));
        let found = refusals(&with_servers(json!([port])));
        assert!(
            found
                .iter()
                .any(|(path, message)| *path == at("oauth/callback_port")
                    && message.starts_with("callback_port_without_client: ")),
            "{found:?}"
        );
        let found = refusals(&with_servers(json!([stdio_with_oauth()])));
        assert!(
            found
                .iter()
                .any(|(path, message)| *path == at("oauth")
                    && message.starts_with("oauth_on_stdio: ")),
            "{found:?}"
        );
        let mut keys = oauth_server(json!({}));
        keys["credential_keys"] = json!(["API_KEY"]);
        let found = refusals(&with_servers(json!([keys])));
        assert!(
            found
                .iter()
                .any(|(path, message)| *path == at("credential_keys")
                    && message.starts_with("oauth_with_keys: ")),
            "{found:?}"
        );
    }

    fn stdio_with_oauth() -> Value {
        let mut stdio = a_stdio_server();
        stdio["oauth"] = json!({});
        stdio
    }

    #[test]
    fn refuses_a_scope_with_a_space_or_quote() {
        for scope in ["a b", "a\"b"] {
            let wire = with_servers(json!([oauth_server(json!({ "scopes": [scope] }))]));
            assert_eq!(
                paths(&wire),
                ["/agents/0/mcp_servers/0/oauth/scopes/0"],
                "{scope}"
            );
        }
    }

    fn a_kit_server() -> Value {
        let mut server = an_http_server();
        server["source"] = json!("kit");
        server
    }

    #[test]
    fn accepts_a_kit_entry_held_to_the_custom_rules() {
        let wire = with_servers(json!([a_kit_server()]));
        team(&wire);
        let mut server = a_kit_server();
        server["url"] = json!("https://mcp.example.com/mcp?key=abc");
        assert_eq!(
            paths(&with_servers(json!([server]))),
            ["/agents/0/mcp_servers/0/url"]
        );
    }

    #[test]
    fn reads_a_kit_entry_as_a_server_marked_kit() {
        let mut kit = a_kit_server();
        kit["name"] = json!("notion");
        let wire = with_servers(json!([
            kit,
            an_http_server(),
            { "name": "playwright", "source": "builtin" }
        ]));
        let servers = the_custom_servers(&wire);
        assert_eq!(servers[0].as_ref().map(|server| server.kit), Some(true));
        assert_eq!(servers[1].as_ref().map(|server| server.kit), Some(false));
        assert_eq!(servers[2], None);
    }

    #[test]
    fn spec_hash_tells_a_kit_entry_from_a_custom_one() {
        let hash = |server: Value| {
            let wire: super::McpServerWire = serde_json::from_value(server).expect("a server");
            spec_sha256(&custom_server(&wire).expect("a server"))
        };
        assert_ne!(hash(a_kit_server()), hash(an_http_server()));
        assert_eq!(
            hash(a_kit_server()),
            "41d345c1449b595f91ec344a2285152a7792b98b1ead2028d83a6aa5cc987b11",
            "recorded after source: kit joined the hash"
        );
    }

    fn a_kit_server_with_allowance(tool: &str, calls: u32) -> Value {
        let mut server = a_kit_server();
        server["tools"]["generate_image"] = json!("external_effect");
        server["allowances"] = json!({ tool: calls });
        server
    }

    fn server_with(wire: &Value) -> CustomServer {
        the_custom_servers(wire)
            .remove(0)
            .expect("the first entry is a custom or kit one")
    }

    #[test]
    fn accepts_allowances_on_a_kit_entrys_external_tools() {
        let wire = with_servers(json!([a_kit_server_with_allowance("generate_image", 20)]));
        team(&wire);
        assert_eq!(
            server_with(&wire).allowances,
            BTreeMap::from([("generate_image".to_string(), 20)])
        );
    }

    #[test]
    fn refuses_an_allowance_on_a_custom_entry() {
        let mut server = a_kit_server_with_allowance("generate_image", 20);
        server["source"] = json!("custom");
        let refused = refusals(&with_servers(json!([server])));
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(
            refused[0].0,
            "/agents/0/mcp_servers/0/allowances/generate_image"
        );
        assert!(
            refused[0].1.starts_with("allowance_not_kit: "),
            "{refused:?}"
        );
    }

    #[test]
    fn refuses_an_allowance_on_a_tool_not_external() {
        for tool in ["list_issues", "unlisted"] {
            let refused = refusals(&with_servers(json!([a_kit_server_with_allowance(tool, 3)])));
            assert_eq!(refused.len(), 1, "{tool}: {refused:?}");
            assert_eq!(
                refused[0].0,
                format!("/agents/0/mcp_servers/0/allowances/{tool}")
            );
            assert!(
                refused[0].1.starts_with("allowance_not_external: "),
                "{tool}: {refused:?}"
            );
        }
    }

    #[test]
    fn refuses_an_allowance_over_a_thousand() {
        let wire = with_servers(json!([a_kit_server_with_allowance("generate_image", 1001)]));
        assert_eq!(
            paths(&wire),
            ["/agents/0/mcp_servers/0/allowances/generate_image"]
        );
        team(&with_servers(json!([a_kit_server_with_allowance(
            "generate_image",
            1000
        )])));
        team(&with_servers(json!([a_kit_server_with_allowance(
            "generate_image",
            0
        )])));
    }

    #[test]
    fn spec_hash_sees_an_allowance() {
        let hash = |server: Value| spec_sha256(&server_with(&with_servers(json!([server]))));
        assert_ne!(
            hash(a_kit_server_with_allowance("generate_image", 20)),
            hash(a_kit_server_with_allowance("generate_image", 21))
        );
        let mut empty = a_kit_server_with_allowance("generate_image", 20);
        empty["allowances"] = json!({});
        let mut none = empty.clone();
        none.as_object_mut()
            .expect("an object")
            .remove("allowances");
        assert_eq!(
            hash(empty),
            hash(none),
            "an empty allowances is not in the hash"
        );
    }

    #[test]
    fn refuses_a_kit_entry_named_for_a_builtin() {
        let mut server = a_kit_server();
        server["name"] = json!("playwright");
        let refused = refusals(&with_servers(json!([server])));
        assert!(
            refused
                .iter()
                .any(|(path, message)| path == "/agents/0/mcp_servers/0/name"
                    && message.starts_with("connector_name_reserved: ")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_server_without_oauth_keeps_its_hash() {
        let wire: super::McpServerWire =
            serde_json::from_value(an_http_server()).expect("a server");
        assert_eq!(
            spec_sha256(&custom_server(&wire).expect("custom")),
            "4342386d7f10fabc3354e601e592e3e414884142ade85d90d7a751a20a046310",
            "recorded before oauth existed"
        );
        let kit: super::McpServerWire = serde_json::from_value(a_kit_server()).expect("a server");
        assert_eq!(
            spec_sha256(&custom_server(&kit).expect("a kit entry")),
            "41d345c1449b595f91ec344a2285152a7792b98b1ead2028d83a6aa5cc987b11",
            "recorded after source: kit joined the hash; step 05b's allowances do not change it"
        );
    }

    #[test]
    fn oauth_settings_change_the_hash() {
        let hash = |oauth: Option<Value>| {
            let mut server = oauth_server(json!({}));
            match oauth {
                Some(oauth) => server["oauth"] = oauth,
                None => {
                    server.as_object_mut().expect("an object").remove("oauth");
                }
            }
            let wire: super::McpServerWire = serde_json::from_value(server).expect("a server");
            spec_sha256(&custom_server(&wire).expect("custom"))
        };
        let base =
            json!({ "client_id": "abc", "callback_port": 33418, "scopes": ["read", "write"] });
        let mut hashes = vec![hash(None), hash(Some(json!({}))), hash(Some(base.clone()))];
        for (field, value) in [
            ("client_id", json!("abd")),
            ("callback_port", json!(33419)),
            ("scopes", json!(["read", "writes"])),
        ] {
            let mut changed = base.clone();
            changed[field] = value;
            hashes.push(hash(Some(changed)));
        }
        let mut unique = hashes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), hashes.len(), "each setting is in the hash");
        let wire: super::McpServerWire =
            serde_json::from_value(oauth_server(json!({}))).expect("a server");
        let CustomTransport::Http {
            url,
            headers,
            oauth,
        } = custom_server(&wire).expect("custom").transport
        else {
            panic!("http");
        };
        assert_eq!(
            (url.as_str(), headers.len()),
            ("https://mcp.example.com/mcp", 0)
        );
        let settings = oauth.expect("oauth");
        assert_eq!(
            canonical_json(&super::oauth_json(&settings)),
            r#"{"callback_port":null,"client_id":null,"scopes":[]}"#
        );
    }

    /// `skills` pinned at the top level and on the first agent.
    fn with_pins(team_pins: Value, agent_pins: Value) -> Value {
        let mut wire = a_team_wire();
        wire["skills"] = team_pins;
        wire["agents"][0]["skills"] = agent_pins;
        wire
    }

    fn a_pin(name: &str) -> Value {
        json!({ "name": name, "sha256": "ab".repeat(32) })
    }

    #[test]
    fn accepts_team_and_agent_skill_pins() {
        let wire = with_pins(json!([a_pin("api-style")]), json!([a_pin("review-notes")]));
        let read = team(&wire);
        let back = serde_json::to_value(&read).expect("a team serialises");
        assert_eq!(back["skills"], wire["skills"]);
        assert_eq!(back["agents"][0]["skills"], wire["agents"][0]["skills"]);
        let pins: Vec<(String, String)> = read
            .skills()
            .iter()
            .map(|pin| {
                (
                    pin.name.as_str().to_string(),
                    pin.sha256.as_str().to_string(),
                )
            })
            .collect();
        assert_eq!(pins, [("api-style".to_string(), "ab".repeat(32))]);
        assert_eq!(read.agents[0].skills.len(), 1);
        assert!(
            team(&a_team_wire()).skills().is_empty(),
            "pins are optional"
        );
    }

    #[test]
    fn refuses_a_bad_pin() {
        let bad_name = with_pins(
            json!([{ "name": "Bad_Name", "sha256": "ab".repeat(32) }]),
            json!([]),
        );
        assert!(
            paths(&bad_name).iter().any(|path| path == "/skills/0/name"),
            "{:?}",
            paths(&bad_name)
        );
        let short = json!({ "name": "api-style", "sha256": "a".repeat(63) });
        let bad_hash = with_pins(json!([]), json!([short]));
        assert!(
            paths(&bad_hash)
                .iter()
                .any(|path| path == "/agents/0/skills/0/sha256"),
            "{:?}",
            paths(&bad_hash)
        );
        let twice = with_pins(
            json!([a_pin("api-style"), a_pin("other"), a_pin("api-style")]),
            json!([]),
        );
        let refused = refusals(&twice);
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].0, "/skills/2/name");
        assert!(
            refused[0].1.starts_with("skill_name_twice: "),
            "{}",
            refused[0].1
        );
        let agent_twice = with_pins(json!([]), json!([a_pin("a"), a_pin("a")]));
        assert_eq!(paths(&agent_twice), ["/agents/0/skills/1/name"]);
        let both = with_pins(json!([a_pin("same")]), json!([a_pin("same")]));
        assert!(
            validate_team(&both).is_ok(),
            "a team and an agent may share a name"
        );
        let twenty: Vec<Value> = (0..20).map(|n| a_pin(&format!("s{n}"))).collect();
        assert!(validate_team(&with_pins(json!(twenty.clone()), json!(twenty.clone()))).is_ok());
        let mut twenty_one = twenty;
        twenty_one.push(a_pin("s20"));
        for wire in [
            with_pins(json!(twenty_one.clone()), json!([])),
            with_pins(json!([]), json!(twenty_one)),
        ] {
            assert!(
                paths(&wire)
                    .iter()
                    .any(|path| path == "/skills" || path == "/agents/0/skills"),
                "{:?}",
                paths(&wire)
            );
        }
    }
}
