//! `farik connect` and `farik disconnect` (`docs/SPEC.md` 5.6, ADR 0030): this process reads the
//! keys, lists the server's tools with them and keeps them, so that a key never crosses a socket;
//! the team file and the event go through the command, which carries names only.

use std::collections::BTreeMap;
use std::io::BufRead as _;

use farik_core::contract::ValidationError;
use farik_core::team::{CustomServer, CustomTransport, spec_sha256};
use farik_protocol::command::Command;
use farik_runtime::claude::Secret;
use farik_runtime::connectors::{
    ConnectorEntry, ConnectorError, SecretAt, SecretStore, folder_refusal, list_tools, own_program,
    working_folder,
};
use farik_runtime::credential::CredentialError;
use farik_runtime::daemon::{custom_entry, kit_entry, labelled};
use farik_runtime::registered_apps::{RegisteredApp, app_for_farik_connector};
use farik_runtime::sign_in::{SignInError, revoke, start_app_sign_in, start_sign_in};
use serde_json::{Map, Value, json};

use crate::project::Project;
use crate::start::{command, runtime};
use crate::{CliIo, Report};

/// What the user is told beside a code to type, since any program can ask a service for a code in
/// Farik's name.
const CODE_WARNING: &str =
    "Only enter a code that this page shows you. Farik never sends you a code in a chat.";
/// The labels a tool can be given.
const TAGS: [&str; 3] = ["network", "external_effect", "denied"];

/// What `farik connect` was asked, as typed.
pub(crate) struct Asked<'a> {
    pub(crate) agent: &'a str,
    pub(crate) name: &'a str,
    pub(crate) command: Option<&'a str>,
    pub(crate) args: &'a [String],
    pub(crate) url: Option<&'a str>,
    pub(crate) headers: &'a [String],
    pub(crate) keys: &'a [String],
    pub(crate) tags: &'a [String],
    /// `<tool>=<calls>`, for a kit's service.
    pub(crate) allowances: &'a [String],
    /// Sign in to the server's service instead of giving keys.
    pub(crate) sign_in: bool,
    pub(crate) client_id: Option<&'a str>,
    pub(crate) callback_port: Option<u16>,
    pub(crate) scopes: &'a [String],
}

/// `farik connect`: the entry held to the team's rules, each key read from standard input, the
/// server's tools listed with them and labelled, the keys kept beside the definition's hash, then
/// `connector_connect` handled here or sent. Prints each tool with its label, what the command
/// said, and where the keys were kept, last.
///
/// # Errors
///
/// A key given a value on the command line, a label or header that is not one, the team's
/// errors, a server whose tools cannot be listed, a store that cannot keep the keys, or the
/// command's refusal.
pub(crate) fn connect(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
) -> Result<Report, String> {
    // An opener that fails is no error: the address is printed too, and the user can open it.
    let opener = std::sync::Arc::clone(&io.open_url);
    let apps = io.registered_apps;
    connect_with(
        project,
        asked,
        io,
        &move |url| {
            let _ = opener(url);
        },
        apps,
    )
}

/// [`connect`], opening the sign-in page with `open`, which a test passes to follow it, and signing
/// in with `apps`, the apps Farik has registered with a service.
///
/// # Errors
///
/// As [`connect`], and a service that does not offer, or does not complete, a sign-in.
pub(crate) fn connect_with(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
    open: &dyn Fn(&str),
    apps: &[RegisteredApp],
) -> Result<Report, String> {
    if asked.command.is_none() && asked.url.is_none() {
        return connect_kit(project, asked, io, open, apps);
    }
    if !asked.allowances.is_empty() {
        return Err(
            "kit_names_these: only a service of the role's kit has an allowance, and a server \
             you describe asks every time; leave out --allowance"
                .to_string(),
        );
    }
    needs_its_form(asked)?;
    if asked.sign_in {
        return connect_signed_in(project, asked, io, open, apps);
    }
    let wire = wire_of(asked)?;
    let tags = tags_of(asked.tags)?;
    let (_, server) =
        custom_entry(&project.team, asked.agent, &wire, json!({})).map_err(|e| errors(&e))?;
    let keys = read_keys(asked.keys, io)?;
    keep_keys(
        project,
        io,
        asked.agent,
        &server,
        keys,
        &|listed| labelled(listed, &tags),
        &|tools| custom_entry(&project.team, asked.agent, &wire, Value::Object(tools)),
    )
}

/// The flags that only make sense with a form of the command, which a name alone (a kit's service)
/// answers `kit_names_these` to instead.
fn needs_its_form(asked: &Asked<'_>) -> Result<(), String> {
    let wrong = [
        (
            !asked.args.is_empty() && asked.command.is_none(),
            "--arg needs --command",
        ),
        (
            !asked.headers.is_empty() && asked.url.is_none(),
            "--header needs --url",
        ),
        (
            asked.sign_in && asked.url.is_none(),
            "--sign-in needs --url",
        ),
        (
            asked.client_id.is_some() && !asked.sign_in,
            "--client-id needs --sign-in",
        ),
        (
            asked.callback_port.is_some() && asked.client_id.is_none(),
            "--callback-port needs --client-id",
        ),
        (
            !asked.scopes.is_empty() && !asked.sign_in,
            "--scope needs --sign-in",
        ),
    ];
    match wrong.iter().find(|(is_wrong, _)| *is_wrong) {
        Some((_, why)) => Err((*why).to_string()),
        None => Ok(()),
    }
}

/// `farik connect <agent> <name>`: the service `name` of the agent's role's kit, which says how
/// it starts, which keys it takes and what each tool may do (ADR 0036). Each key is read in order
/// from standard input after a line saying where it is made; a service the user signs in to is
/// signed in to, as `--sign-in` does.
fn connect_kit(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
    open: &dyn Fn(&str),
    apps: &[RegisteredApp],
) -> Result<Report, String> {
    let name = asked.name;
    let given: Vec<&str> = [
        (!asked.args.is_empty(), "--arg"),
        (!asked.headers.is_empty(), "--header"),
        (!asked.keys.is_empty(), "--key"),
        (!asked.tags.is_empty(), "--tag"),
        (asked.sign_in, "--sign-in"),
        (asked.client_id.is_some(), "--client-id"),
        (asked.callback_port.is_some(), "--callback-port"),
        (!asked.scopes.is_empty(), "--scope"),
    ]
    .into_iter()
    .filter_map(|(is_given, flag)| is_given.then_some(flag))
    .collect();
    if !given.is_empty() {
        return Err(format!(
            "kit_names_these: the kit says how {name} starts, which keys it takes and what each \
             tool may do; leave out {}",
            given.join(", ")
        ));
    }
    let held = project
        .team
        .agents
        .iter()
        .find(|held| held.id.as_str() == asked.agent)
        .ok_or_else(|| format!("there is no agent {}", asked.agent))?;
    let kit = (io.kits)(farik_core::contract::Role::from(held.role))
        .map_err(|error| error.to_string())?;
    let allowances = asked_from(asked.allowances)?;
    let build = || kit_entry(&kit, &project.team, asked.agent, name, &allowances);
    let (_, server) = build().map_err(|refused| {
        let has: Vec<&str> = kit
            .connectors
            .iter()
            .filter(|connector| matches!(connector, farik_roles::KitConnector::Server { .. }))
            .map(farik_roles::KitConnector::name)
            .collect();
        let said = errors(&refused);
        if said.contains("connector_not_in_kit") {
            let list = if has.is_empty() {
                "the kit has no service to connect by name".to_string()
            } else {
                format!("the kit has {}", has.join(", "))
            };
            format!("{said}; {list}")
        } else {
            said
        }
    })?;
    let tools_of = |_: &[farik_runtime::connectors::ListedTool]| {
        Ok(build()
            .map(|(entry, _)| entry["tools"].as_object().cloned().unwrap_or_default())
            .unwrap_or_default())
    };
    let build_with = |_: Map<String, Value>| build();
    if server.oauth().is_some() {
        return keep_sign_in(
            project,
            io,
            &SignInWith { open, apps },
            asked.agent,
            &server,
            &tools_of,
            &build_with,
        );
    }
    let page = kit.connectors.iter().find_map(|connector| match connector {
        farik_roles::KitConnector::Server { entry, copy, .. } if entry.name.as_str() == name => {
            copy.key_page.clone()
        }
        _ => None,
    });
    let mut keys = BTreeMap::new();
    for key in &server.credential_keys {
        if let Some(page) = &page {
            crate::say(
                &mut io.stderr,
                &format!("Make a key at {page}, then paste {key}:"),
            );
        }
        keys.extend(read_keys(std::slice::from_ref(key), io)?);
    }
    keep_keys(
        project,
        io,
        asked.agent,
        &server,
        keys,
        &tools_of,
        &build_with,
    )
}

/// The `--allowance <tool>=<calls>` flags as the numbers they ask for.
fn asked_from(given: &[String]) -> Result<BTreeMap<String, u32>, String> {
    let mut asked = Map::new();
    for flag in given {
        let (tool, calls) = flag
            .split_once('=')
            .ok_or_else(|| format!("--allowance wants tool=number, not {flag}"))?;
        asked.insert(
            tool.to_string(),
            calls
                .parse::<u64>()
                .map_or_else(|_| Value::from(calls), Value::from),
        );
    }
    farik_runtime::allowances::asked_allowances(&Value::Object(asked))
}

/// What signing in uses: how a page is opened, and the apps Farik has registered with a service.
struct SignInWith<'a> {
    open: &'a dyn Fn(&str),
    apps: &'a [RegisteredApp],
}

/// What decides the tools an entry is written with: the labels the user gave, or the kit's.
type ToolsOf<'a> =
    &'a dyn Fn(&[farik_runtime::connectors::ListedTool]) -> Result<Map<String, Value>, String>;

/// How the entry is built once its tools are known.
type Build<'a> =
    &'a dyn Fn(Map<String, Value>) -> Result<(Value, CustomServer), Vec<ValidationError>>;

/// The server's tools listed with `keys`, each decided by `tools_of`, the keys kept beside the
/// definition's hash, then `connector_connect` handled here or sent.
fn keep_keys(
    project: &Project,
    io: &mut CliIo<'_>,
    agent: &str,
    server: &CustomServer,
    keys: BTreeMap<String, Secret>,
    tools_of: ToolsOf<'_>,
    build: Build<'_>,
) -> Result<Report, String> {
    let state = state_of(io)?;
    let at = secret_at(&state, project, agent, &server.name)?;
    let folder = working_folder(&state, &project.root, &at)
        .map_err(|error| format!("{}: {}", server.name, folder_refusal(&error)))?;
    let farik = own_program(server, io.own_program.as_deref()).map_err(str::to_string)?;
    let listed = runtime()?
        .block_on(list_tools(server, &keys, None, &folder, &farik))
        .map_err(|error| not_listed(&error))?;
    let tools = tools_of(&listed)?;
    let (entry, server) = build(tools).map_err(|e| errors(&e))?;
    let spec = spec_sha256(&server);
    let stored_in = io
        .connector_secrets
        .save(
            &at,
            &ConnectorEntry {
                spec_sha256: spec.clone(),
                keys,
                oauth: None,
            },
        )
        .map_err(|error| words(&error))?;
    let said = crate::human::said(command(
        project,
        Command::ConnectorConnect {
            agent: agent.to_string(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer: None,
        },
        "connect",
        io,
    ))?;
    Ok(connected_report(&listed, &entry, said, stored_in))
}

/// What `farik connect` prints once the server is kept: each tool with its label, what the
/// command said, and where the keys or the sign-in were kept, last.
fn connected_report(
    listed: &[farik_runtime::connectors::ListedTool],
    entry: &Value,
    said: Report,
    stored_in: SecretStore,
) -> Report {
    let mut lines: Vec<String> = listed
        .iter()
        .map(|tool| match entry["tools"][&tool.name].as_str() {
            Some(tag) => format!("{}: {tag}", tool.name),
            None => format!("{}: Farik can't use this tool", tool.name),
        })
        .collect();
    lines.extend(said.lines);
    lines.push(
        match stored_in {
            SecretStore::Keychain => "Kept in your computer's keychain",
            SecretStore::File => "Kept in a private file only you can read",
        }
        .to_string(),
    );
    Report {
        lines,
        json: json!({ "tools": entry["tools"], "stored_in": stored_in, "said": said.json["said"] }),
        json_lines: None,
    }
}

/// `farik connect --sign-in`: the server's sign-in found out, its page printed and opened, the
/// way back waited for up to ten minutes, the server's tools listed with the token and labelled,
/// the grant kept where keys are, then `connector_connect` handled here or sent, which names who
/// signed in and holds no token (ADR 0033).
fn connect_signed_in(
    project: &Project,
    asked: &Asked<'_>,
    io: &mut CliIo<'_>,
    open: &dyn Fn(&str),
    apps: &[RegisteredApp],
) -> Result<Report, String> {
    let mut wire = wire_of(asked)?;
    let mut oauth = Map::new();
    if let Some(client) = asked.client_id {
        oauth.insert("client_id".to_string(), json!(client));
    }
    if let Some(port) = asked.callback_port {
        oauth.insert("callback_port".to_string(), json!(port));
    }
    if !asked.scopes.is_empty() {
        oauth.insert("scopes".to_string(), json!(asked.scopes));
    }
    wire["oauth"] = Value::Object(oauth);
    let tags = tags_of(asked.tags)?;
    let (_, server) =
        custom_entry(&project.team, asked.agent, &wire, json!({})).map_err(|e| errors(&e))?;
    keep_sign_in(
        project,
        io,
        &SignInWith { open, apps },
        asked.agent,
        &server,
        &|listed| labelled(listed, &tags),
        &|tools| custom_entry(&project.team, asked.agent, &wire, Value::Object(tools)),
    )
}

/// Signs `agent` in to the service of `server`, a web address or one of Farik's own connectors,
/// lists its tools (with the token for a web address, which a connector Farik starts takes none
/// of), decides them with `tools_of`, keeps the grant where keys are, and connects it.
fn keep_sign_in(
    project: &Project,
    io: &mut CliIo<'_>,
    how: &SignInWith<'_>,
    agent: &str,
    server: &CustomServer,
    tools_of: ToolsOf<'_>,
    build: Build<'_>,
) -> Result<Report, String> {
    let Some(settings) = server.oauth() else {
        return Err(format!("{} does not sign in", server.name));
    };
    // What a sentence calls the service before it has said who it is: a web address by its host,
    // one of Farik's own connectors by its name.
    let host = match &server.transport {
        CustomTransport::Http { url, .. } => host_of(url),
        CustomTransport::Stdio { .. } => server.name.clone(),
    };
    let state = state_of(io)?;
    let at = secret_at(&state, project, agent, &server.name)?;
    let folder = working_folder(&state, &project.root, &at)
        .map_err(|error| format!("{}: {}", server.name, folder_refusal(&error)))?;
    let runtime = runtime()?;
    let signing = runtime
        .block_on(start_signing(server, settings, how.apps))
        .map_err(|error| refused(&error, &host))?;
    // Prompts, not results: on stderr, so that `--json` leaves the output as the JSON alone.
    match signing.user_code() {
        // One of Farik's own apps: the user types a code, which is the one thing to warn about.
        Some(code) => {
            crate::say(
                &mut io.stderr,
                &format!(
                    "Open {} and enter the code {code}.",
                    signing.authorize_url()
                ),
            );
            crate::say(&mut io.stderr, CODE_WARNING);
        }
        None => crate::say(
            &mut io.stderr,
            &format!(
                "Sign in to {} in your browser: {}",
                signing
                    .provider()
                    .map_or_else(|| host_of(signing.issuer()), ToString::to_string),
                signing.authorize_url()
            ),
        ),
    }
    (how.open)(signing.authorize_url());
    let issuer = signing.issuer().to_string();
    let provider = signing.provider().map(ToString::to_string);
    // The page the person said yes or no on is the provider's for one of Farik's own apps.
    let grant = runtime
        .block_on(signing.finish())
        .map_err(|error| refused(&error, provider.as_deref().unwrap_or(&host)))?;
    crate::say(
        &mut io.stderr,
        &format!("Signed in to {}.", provider.as_deref().unwrap_or(&issuer)),
    );
    let farik = own_program(server, io.own_program.as_deref()).map_err(str::to_string)?;
    let listed = runtime
        .block_on(list_tools(
            server,
            &BTreeMap::new(),
            Some(&grant.access_token),
            &folder,
            &farik,
        ))
        .map_err(|error| not_listed(&error))?;
    let tools = tools_of(&listed)?;
    let (entry, server) = build(tools).map_err(|e| errors(&e))?;
    let spec = spec_sha256(&server);
    // A sign-in this one replaces is asked to be forgotten, so it does not linger at the service.
    let replaced = io
        .connector_secrets
        .load(&at)
        .ok()
        .flatten()
        .and_then(|old| old.oauth);
    let kept = grant.clone();
    let stored_in = io
        .connector_secrets
        .save(
            &at,
            &ConnectorEntry {
                spec_sha256: spec.clone(),
                keys: BTreeMap::new(),
                oauth: Some(grant),
            },
        )
        .map_err(|error| words(&error))?;
    // Not the grant just kept, nor one of its client's (`revocable_after`).
    if let Some(old) = replaced
        && old.revocable_after(&kept)
    {
        runtime.block_on(revoke(&old));
    }
    let said = crate::human::said(command(
        project,
        Command::ConnectorConnect {
            agent: agent.to_string(),
            server: entry.as_object().cloned().unwrap_or_default(),
            spec_sha256: spec,
            issuer: Some(issuer),
        },
        "connect",
        io,
    ))?;
    Ok(connected_report(&listed, &entry, said, stored_in))
}

/// Starts signing in to `server`: a web address through its own sign-in or the app `apps` has for
/// it, one of Farik's own connectors through the app `apps` has for that.
async fn start_signing(
    server: &CustomServer,
    settings: &farik_core::team::OAuthSettings,
    apps: &[RegisteredApp],
) -> Result<farik_runtime::sign_in::SignIn, SignInError> {
    let now = chrono::Utc::now();
    match &server.transport {
        CustomTransport::Http { url, .. } => start_sign_in(url, settings, apps, now).await,
        CustomTransport::Stdio { command, args, .. } => {
            match app_for_farik_connector(apps, command, args) {
                Some(app) => start_app_sign_in(app, &settings.scopes, now).await,
                None => Err(SignInError::NotSupported),
            }
        }
    }
}

/// The host of `url`, for a sentence: no scheme, no userinfo, no port.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or_default();
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') && port.chars().all(|c| c.is_ascii_digit()) => {
            host.to_string()
        }
        _ => authority.to_string(),
    }
}

/// Why a sign-in did not happen, in a sentence that quotes nothing the service sent.
fn refused(error: &SignInError, host: &str) -> String {
    match error {
        SignInError::NotOffered => {
            format!("{host} does not offer signing in; give its key with --key")
        }
        SignInError::NotSupported => format!(
            "{host} does not let Farik sign in by itself yet; if it gives you a key, use --key"
        ),
        SignInError::PkceNotSupported => {
            format!("{host}'s sign-in is not one Farik will use")
        }
        SignInError::Denied(_) => format!("you said no on {host}'s page, so nothing was connected"),
        SignInError::Mismatch => format!(
            "something did not match on the way back from {host}, so Farik stopped to keep you safe"
        ),
        SignInError::TimedOut => "the sign-in took longer than 10 minutes".to_string(),
        SignInError::Lapsed => format!("{host} ended the sign-in"),
        SignInError::Failed(why) => why.clone(),
    }
}

/// `farik disconnect`: `connector_disconnect` handled here or sent, which takes the entry out of
/// the team file, then the agent's keys for it deleted, and no other agent's.
///
/// # Errors
///
/// The command's refusal, or a store that cannot delete the keys.
pub(crate) fn disconnect(
    project: &Project,
    agent: &str,
    name: &str,
    io: &mut CliIo<'_>,
) -> Result<Report, String> {
    let said = crate::human::said(command(
        project,
        Command::ConnectorDisconnect {
            agent: agent.to_string(),
            server: name.to_string(),
        },
        "disconnect",
        io,
    ))?;
    let at = secret_at(&state_of(io)?, project, agent, name)?;
    // Loaded first, so that a sign-in can be asked to be forgotten once the entry is gone.
    let grant = io
        .connector_secrets
        .load(&at)
        .ok()
        .flatten()
        .and_then(|entry| entry.oauth);
    io.connector_secrets
        .delete(&at)
        .map_err(|error| words(&error))?;
    if let Some(grant) = grant {
        runtime()?.block_on(revoke(&grant));
    }
    Ok(said)
}

/// The user's state folder, where the project's id on this machine is kept and each stdio
/// connector runs (ADR 0030).
fn state_of(io: &CliIo<'_>) -> Result<std::path::PathBuf, String> {
    crate::state::state_dir(&io.env).ok_or_else(|| {
        "XDG_CONFIG_HOME, HOME and APPDATA are all unset, so Farik has no state folder to keep \
         this project's connectors in"
            .to_string()
    })
}

/// Where `agent`'s keys for `server` are kept in `project`.
fn secret_at(
    state: &std::path::Path,
    project: &Project,
    agent: &str,
    server: &str,
) -> Result<SecretAt, String> {
    SecretAt::of(state, &project.root, agent, server)
        .map_err(|error| format!("this project's id cannot be read: {error}"))
}

/// The `mcp_servers` entry `asked` describes, without its tools.
fn wire_of(asked: &Asked<'_>) -> Result<Value, String> {
    let mut wire = json!({ "name": asked.name, "credential_keys": asked.keys });
    if let Some((name, _)) = asked.keys.iter().find_map(|key| key.split_once('=')) {
        return Err(format!(
            "--key {name}=...: give the key's name alone; its value is read from standard input, \
             never from the command line, which other programs can see"
        ));
    }
    if let Some(program) = asked.command {
        wire["transport"] = json!("stdio");
        wire["command"] = json!(program);
        wire["args"] = json!(asked.args);
    }
    if let Some(url) = asked.url {
        let mut headers = Map::new();
        for header in asked.headers {
            let (name, template) = header
                .split_once(':')
                .ok_or_else(|| format!("--header {header}: write it as 'Name: template'"))?;
            headers.insert(name.trim().to_string(), json!(template.trim()));
        }
        wire["transport"] = json!("http");
        wire["url"] = json!(url);
        wire["headers"] = Value::Object(headers);
    }
    Ok(wire)
}

/// The labels `--tag` gave, as an object of tool name to tag.
fn tags_of(tags: &[String]) -> Result<Value, String> {
    let mut labelled = Map::new();
    for tag in tags {
        match tag.split_once('=') {
            Some((tool, label)) if TAGS.contains(&label) => {
                labelled.insert(tool.to_string(), json!(label));
            }
            _ => {
                return Err(format!(
                    "--tag {tag}: write it as <tool>=network, <tool>=external_effect or \
                     <tool>=denied"
                ));
            }
        }
    }
    Ok(Value::Object(labelled))
}

/// One line of standard input for each key named, read with its echo off at a terminal.
fn read_keys(names: &[String], io: &mut CliIo<'_>) -> Result<BTreeMap<String, Secret>, String> {
    let mut keys = BTreeMap::new();
    let mut input = std::io::BufReader::new(&mut io.stdin);
    for name in names {
        let value = if io.stdin_is_terminal {
            rpassword::prompt_password(format!("{name}: "))
                .map_err(|error| format!("{name} could not be read: {error}"))?
        } else {
            let mut line = String::new();
            input
                .read_line(&mut line)
                .map_err(|error| format!("{name} could not be read: {error}"))?;
            line.trim_end_matches(['\n', '\r']).to_string()
        };
        if value.is_empty() {
            return Err(format!(
                "the key {name} has no value: give it on standard input, one line per --key"
            ));
        }
        keys.insert(name.clone(), Secret::new(value));
    }
    Ok(keys)
}

/// The team's errors, each at its field, in one sentence.
fn errors(errors: &[ValidationError]) -> String {
    errors
        .iter()
        .map(|error| format!("{}: {}", error.path, error.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Why a server's tools could not be listed, in a sentence that quotes no key.
fn not_listed(error: &ConnectorError) -> String {
    match error {
        ConnectorError::Timeout => "the server did not answer within thirty seconds".to_string(),
        ConnectorError::KeyMissing(name) => format!("the key {name} has no value"),
        ConnectorError::Failed(why) => format!("its tools could not be listed: {why}"),
        // A listing calls no tool, so this is not one it can meet.
        ConnectorError::ToolError { .. } => "its tools could not be listed".to_string(),
    }
}

/// A store's refusal in words.
fn words(error: &CredentialError) -> String {
    match error {
        CredentialError::NoKeychain => "this computer has no keychain".to_string(),
        CredentialError::Failed(why) => why.clone(),
    }
}
